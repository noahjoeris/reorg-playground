use std::collections::BTreeSet;
use std::str::FromStr;

use bitcoincore_rpc::bitcoin::BlockHash;
use petgraph::Direction;
use petgraph::graph::NodeIndex;

use crate::config::StaleRateRange;
use crate::types::{
    MetricUnavailableReason, NetworkMetricsJson, NodeData, StaleBlockRateJson,
    StaleBlockRateRangeJson, StaleBlockRateWindowJson, Tree, TreeInfo,
};

struct MetricsContext<'a> {
    tree: &'a TreeInfo,
    resolved_tip: NodeIndex,
    resolved_path: Vec<NodeIndex>,
}

pub async fn calculate_network_metrics(
    tree: &Tree,
    node_data: &NodeData,
    stale_rate_ranges: &[StaleRateRange],
) -> NetworkMetricsJson {
    let tree_locked = tree.lock().await;
    calculate_network_metrics_with_tree(&tree_locked, node_data, stale_rate_ranges)
}

fn calculate_network_metrics_with_tree(
    tree: &TreeInfo,
    node_data: &NodeData,
    stale_rate_ranges: &[StaleRateRange],
) -> NetworkMetricsJson {
    let context = match MetricsContext::new(tree, node_data) {
        Ok(context) => context,
        Err(reason) => return NetworkMetricsJson::unavailable(stale_rate_ranges, reason),
    };

    let windows = stale_rate_ranges
        .iter()
        .map(|range| context.calculate_window(range))
        .collect();

    NetworkMetricsJson {
        stale_block_rate: StaleBlockRateJson {
            as_of_height: Some(context.resolved_height()),
            windows,
        },
    }
}

impl<'a> MetricsContext<'a> {
    fn new(tree: &'a TreeInfo, node_data: &NodeData) -> Result<Self, MetricUnavailableReason> {
        let resolved_tip = resolved_tip_index(tree, node_data)?;
        if !has_reachable_stale_tip_observer(node_data) {
            return Err(MetricUnavailableReason::NoReachableStaleTipSupport);
        }
        let resolved_path = path_to_root(tree, resolved_tip);

        Ok(Self {
            tree,
            resolved_tip,
            resolved_path,
        })
    }

    fn resolved_height(&self) -> u64 {
        self.tree.graph[self.resolved_tip].height
    }

    fn calculate_window(&self, range: &StaleRateRange) -> StaleBlockRateWindowJson {
        match range {
            StaleRateRange::Rolling(blocks) => self.calculate_rolling_window(*blocks),
            StaleRateRange::AllTime => self.calculate_all_time_window(),
        }
    }

    fn calculate_rolling_window(&self, blocks: u64) -> StaleBlockRateWindowJson {
        let Some(window_start) = walk_back_window_start(self.tree, self.resolved_tip, blocks)
        else {
            return StaleBlockRateWindowJson::unavailable(
                StaleBlockRateRangeJson::Rolling { blocks },
                MetricUnavailableReason::InsufficientHistory,
            );
        };

        self.build_window(
            StaleBlockRateRangeJson::Rolling { blocks },
            window_start,
            self.resolved_tip,
            blocks,
        )
    }

    fn calculate_all_time_window(&self) -> StaleBlockRateWindowJson {
        let retained_start = *self
            .resolved_path
            .first()
            .expect("resolved path should always contain at least one block");
        self.build_window(
            StaleBlockRateRangeJson::AllTime,
            retained_start,
            self.resolved_tip,
            self.resolved_path.len() as u64,
        )
    }

    fn build_window(
        &self,
        range: StaleBlockRateRangeJson,
        window_start: NodeIndex,
        window_end: NodeIndex,
        active_blocks: u64,
    ) -> StaleBlockRateWindowJson {
        let start_height = self.tree.graph[window_start].height;
        let end_height = self.tree.graph[window_end].height;
        if has_history_gap_in_window(self.tree, start_height, end_height) {
            return StaleBlockRateWindowJson::unavailable(
                range,
                MetricUnavailableReason::IncompleteObservedHistory,
            );
        }
        let total_headers = count_headers_in_height_range(self.tree, start_height, end_height);
        let stale_blocks = total_headers.saturating_sub(active_blocks);

        StaleBlockRateWindowJson {
            range,
            stale_blocks,
            active_blocks,
            rate: stale_blocks as f64 / active_blocks as f64,
            available: true,
            reason: None,
        }
    }
}

fn count_headers_in_height_range(tree: &TreeInfo, start_height: u64, end_height: u64) -> u64 {
    tree.graph
        .raw_nodes()
        .iter()
        .filter(|node| {
            let height = node.weight.height;
            height >= start_height && height <= end_height
        })
        .count() as u64
}

fn has_reachable_stale_tip_observer(node_data: &NodeData) -> bool {
    node_data.values().any(|node| {
        node.reachable
            && node.supports_stale_tips
            && node.tips.iter().any(|tip| tip.status == "active")
    })
}

/// A root above the window start means at least one ancestor inside the range
/// is missing, so any stale-rate derived from this slice would be understated.
fn has_history_gap_in_window(tree: &TreeInfo, start_height: u64, end_height: u64) -> bool {
    tree.graph.externals(Direction::Incoming).any(|idx| {
        let height = tree.graph[idx].height;
        height > start_height && height <= end_height
    })
}

fn resolved_tip_index(
    tree: &TreeInfo,
    node_data: &NodeData,
) -> Result<NodeIndex, MetricUnavailableReason> {
    let active_tip_indices = active_tip_indices(tree, node_data)?;
    let mut common_path = path_to_root(tree, active_tip_indices[0]);

    for tip_idx in active_tip_indices.into_iter().skip(1) {
        let path = path_to_root(tree, tip_idx);
        let shared_len = common_path
            .iter()
            .zip(path.iter())
            .take_while(|(left, right)| left == right)
            .count();
        common_path.truncate(shared_len);
        if common_path.is_empty() {
            return Err(MetricUnavailableReason::InsufficientHistory);
        }
    }

    common_path
        .last()
        .copied()
        .ok_or(MetricUnavailableReason::InsufficientHistory)
}

fn active_tip_indices(
    tree: &TreeInfo,
    node_data: &NodeData,
) -> Result<Vec<NodeIndex>, MetricUnavailableReason> {
    let active_hashes: BTreeSet<String> = node_data
        .values()
        .filter(|node| node.reachable)
        .flat_map(|node| node.tips.iter())
        .filter(|tip| tip.status == "active")
        .map(|tip| tip.hash.clone())
        .collect();

    if active_hashes.is_empty() {
        return Err(MetricUnavailableReason::NoReachableActiveTip);
    }

    active_hashes
        .into_iter()
        .map(|hash| {
            let block_hash =
                BlockHash::from_str(&hash).map_err(|_| MetricUnavailableReason::TipNotInTree)?;
            tree.index
                .get(&block_hash)
                .copied()
                .ok_or(MetricUnavailableReason::TipNotInTree)
        })
        .collect()
}

fn walk_back_window_start(
    tree: &TreeInfo,
    resolved_tip: NodeIndex,
    window_size: u64,
) -> Option<NodeIndex> {
    let mut current = resolved_tip;
    let mut remaining = window_size.saturating_sub(1);

    while remaining > 0 {
        let parent = parent_index(tree, current)?;
        if tree.graph[parent].height + 1 != tree.graph[current].height {
            return None;
        }
        current = parent;
        remaining -= 1;
    }

    Some(current)
}

fn path_to_root(tree: &TreeInfo, tip_idx: NodeIndex) -> Vec<NodeIndex> {
    let mut path = vec![tip_idx];
    let mut current = tip_idx;

    while let Some(parent) = parent_index(tree, current) {
        path.push(parent);
        current = parent;
    }

    path.reverse();
    path
}

fn parent_index(tree: &TreeInfo, idx: NodeIndex) -> Option<NodeIndex> {
    tree.graph
        .neighbors_directed(idx, Direction::Incoming)
        .next()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::StaleRateRange;
    use crate::types::{NodeDataJson, TipInfoJson};
    use bitcoincore_rpc::bitcoin::blockdata::block::Header;
    use bitcoincore_rpc::bitcoin::hashes::Hash;
    use bitcoincore_rpc::bitcoin::{CompactTarget, TxMerkleNode};
    use petgraph::graph::DiGraph;
    use std::collections::HashMap;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    fn make_header(prev: BlockHash, unique: u32) -> Header {
        Header {
            version: bitcoincore_rpc::bitcoin::block::Version::from_consensus(1),
            prev_blockhash: prev,
            merkle_root: TxMerkleNode::all_zeros(),
            time: unique,
            bits: CompactTarget::from_consensus(0x1d00ffff),
            nonce: unique,
        }
    }

    fn add_block(tree: &mut TreeInfo, height: u64, prev_hash: BlockHash, unique: u32) -> BlockHash {
        let header = make_header(prev_hash, unique);
        let hash = header.block_hash();
        let idx = tree.graph.add_node(crate::types::HeaderInfo {
            height,
            header,
            miner: String::new(),
        });
        tree.index.insert(hash, idx);
        if let Some(parent_idx) = tree.index.get(&prev_hash) {
            tree.graph.update_edge(*parent_idx, idx, false);
        }
        hash
    }

    fn build_linear_tree(max_height: u64) -> (Tree, Vec<BlockHash>) {
        let mut tree = TreeInfo {
            graph: DiGraph::new(),
            index: HashMap::new(),
        };
        let mut hashes = Vec::with_capacity(max_height as usize + 1);
        let mut prev_hash = BlockHash::all_zeros();

        for height in 0..=max_height {
            let hash = add_block(&mut tree, height, prev_hash, height as u32 + 1);
            hashes.push(hash);
            prev_hash = hash;
        }

        (Arc::new(Mutex::new(tree)), hashes)
    }

    fn build_tree_with_stale_branch() -> (Tree, Vec<BlockHash>, Vec<BlockHash>) {
        let (tree, main_chain) = build_linear_tree(10);
        let mut tree_locked = tree.try_lock().expect("tree should be unlocked in tests");
        let fork_parent = main_chain[7];
        let alt_8 = add_block(&mut tree_locked, 8, fork_parent, 1008);
        let alt_9 = add_block(&mut tree_locked, 9, alt_8, 1009);
        let alt_10 = add_block(&mut tree_locked, 10, alt_9, 1010);
        drop(tree_locked);

        (tree, main_chain, vec![alt_8, alt_9, alt_10])
    }

    fn build_tree_with_unexpected_root() -> (Tree, Vec<BlockHash>) {
        let (tree, main_chain) = build_linear_tree(10);
        let mut tree_locked = tree.try_lock().expect("tree should be unlocked in tests");
        let unexpected_root = add_block(
            &mut tree_locked,
            8,
            BlockHash::from_byte_array([42; 32]),
            2008,
        );
        let unexpected_child = add_block(&mut tree_locked, 9, unexpected_root, 2009);
        add_block(&mut tree_locked, 10, unexpected_child, 2010);
        drop(tree_locked);

        (tree, main_chain)
    }

    fn node_data(active_hashes: &[(&str, bool)], supports_stale_tips: bool) -> NodeData {
        active_hashes
            .iter()
            .enumerate()
            .map(|(idx, (hash, reachable))| {
                (
                    idx as u32,
                    NodeDataJson {
                        id: idx as u32,
                        name: format!("node-{}", idx),
                        description: "test node".to_string(),
                        implementation: "Bitcoin Core".to_string(),
                        supports_controls: false,
                        supports_mining: false,
                        supports_stale_tips,
                        tips: vec![TipInfoJson {
                            hash: (*hash).to_string(),
                            status: "active".to_string(),
                            height: 0,
                        }],
                        last_changed_timestamp: 0,
                        version: "test".to_string(),
                        reachable: *reachable,
                    },
                )
            })
            .collect()
    }

    fn window(
        metrics: &NetworkMetricsJson,
        range: StaleBlockRateRangeJson,
    ) -> &StaleBlockRateWindowJson {
        metrics
            .stale_block_rate
            .windows
            .iter()
            .find(|window| window.range == range)
            .expect("metric window should exist")
    }

    #[tokio::test]
    async fn calculates_zero_stale_rate_for_linear_chain() {
        let (tree, hashes) = build_linear_tree(10);
        let node_data = node_data(&[(&hashes[10].to_string(), true)], true);

        let metrics =
            calculate_network_metrics(&tree, &node_data, &[StaleRateRange::Rolling(5)]).await;

        assert_eq!(metrics.stale_block_rate.as_of_height, Some(10));
        assert_eq!(
            window(&metrics, StaleBlockRateRangeJson::Rolling { blocks: 5 }),
            &StaleBlockRateWindowJson {
                range: StaleBlockRateRangeJson::Rolling { blocks: 5 },
                stale_blocks: 0,
                active_blocks: 5,
                rate: 0.0,
                available: true,
                reason: None,
            }
        );
    }

    #[tokio::test]
    async fn counts_all_stale_blocks_below_resolved_tip() {
        let (tree, main_chain, _stale_chain) = build_tree_with_stale_branch();
        let node_data = node_data(&[(&main_chain[10].to_string(), true)], true);

        let metrics =
            calculate_network_metrics(&tree, &node_data, &[StaleRateRange::Rolling(5)]).await;

        assert_eq!(metrics.stale_block_rate.as_of_height, Some(10));
        assert_eq!(
            window(&metrics, StaleBlockRateRangeJson::Rolling { blocks: 5 }).stale_blocks,
            3
        );
        assert!(
            (window(&metrics, StaleBlockRateRangeJson::Rolling { blocks: 5 }).rate - 0.6).abs()
                < 1e-12
        );
    }

    #[tokio::test]
    async fn excludes_unresolved_live_splits_by_using_common_ancestor() {
        let (tree, main_chain, stale_chain) = build_tree_with_stale_branch();
        let node_data = node_data(
            &[
                (&main_chain[10].to_string(), true),
                (&stale_chain[2].to_string(), true),
            ],
            true,
        );

        let metrics =
            calculate_network_metrics(&tree, &node_data, &[StaleRateRange::Rolling(3)]).await;

        assert_eq!(metrics.stale_block_rate.as_of_height, Some(7));
        assert_eq!(
            window(&metrics, StaleBlockRateRangeJson::Rolling { blocks: 3 }).stale_blocks,
            0
        );
        assert!(window(&metrics, StaleBlockRateRangeJson::Rolling { blocks: 3 }).available);
    }

    #[tokio::test]
    async fn ignores_unreachable_nodes_when_resolving_tip() {
        let (tree, main_chain, stale_chain) = build_tree_with_stale_branch();
        let node_data = node_data(
            &[
                (&main_chain[10].to_string(), true),
                (&stale_chain[2].to_string(), false),
            ],
            true,
        );

        let metrics =
            calculate_network_metrics(&tree, &node_data, &[StaleRateRange::Rolling(5)]).await;

        assert_eq!(metrics.stale_block_rate.as_of_height, Some(10));
        assert_eq!(
            window(&metrics, StaleBlockRateRangeJson::Rolling { blocks: 5 }).stale_blocks,
            3
        );
        assert!(window(&metrics, StaleBlockRateRangeJson::Rolling { blocks: 5 }).available);
    }

    #[tokio::test]
    async fn calculates_all_time_metric_from_retained_history() {
        let (tree, main_chain, _stale_chain) = build_tree_with_stale_branch();
        let node_data = node_data(&[(&main_chain[10].to_string(), true)], true);

        let metrics =
            calculate_network_metrics(&tree, &node_data, &[StaleRateRange::AllTime]).await;

        assert_eq!(metrics.stale_block_rate.as_of_height, Some(10));
        assert_eq!(
            window(&metrics, StaleBlockRateRangeJson::AllTime),
            &StaleBlockRateWindowJson {
                range: StaleBlockRateRangeJson::AllTime,
                stale_blocks: 3,
                active_blocks: 11,
                rate: 3.0 / 11.0,
                available: true,
                reason: None,
            }
        );
    }

    #[tokio::test]
    async fn marks_window_unavailable_when_history_is_too_shallow() {
        let (tree, hashes) = build_linear_tree(10);
        let node_data = node_data(&[(&hashes[10].to_string(), true)], true);

        let metrics =
            calculate_network_metrics(&tree, &node_data, &[StaleRateRange::Rolling(12)]).await;

        assert_eq!(metrics.stale_block_rate.as_of_height, Some(10));
        assert_eq!(
            window(&metrics, StaleBlockRateRangeJson::Rolling { blocks: 12 }),
            &StaleBlockRateWindowJson::unavailable(
                StaleBlockRateRangeJson::Rolling { blocks: 12 },
                MetricUnavailableReason::InsufficientHistory,
            )
        );
    }

    #[tokio::test]
    async fn returns_unavailable_when_no_reachable_active_tip_exists() {
        let (tree, hashes) = build_linear_tree(10);
        let node_data = node_data(&[(&hashes[10].to_string(), false)], true);

        let metrics =
            calculate_network_metrics(&tree, &node_data, &[StaleRateRange::Rolling(5)]).await;

        assert_eq!(metrics.stale_block_rate.as_of_height, None);
        assert_eq!(
            window(&metrics, StaleBlockRateRangeJson::Rolling { blocks: 5 }),
            &StaleBlockRateWindowJson::unavailable(
                StaleBlockRateRangeJson::Rolling { blocks: 5 },
                MetricUnavailableReason::NoReachableActiveTip,
            )
        );
    }

    #[tokio::test]
    async fn returns_unavailable_when_active_tip_is_missing_from_tree() {
        let (tree, _hashes) = build_linear_tree(10);
        let node_data = node_data(
            &[(
                "0000000000000000000000000000000000000000000000000000000000000001",
                true,
            )],
            true,
        );

        let metrics =
            calculate_network_metrics(&tree, &node_data, &[StaleRateRange::Rolling(5)]).await;

        assert_eq!(metrics.stale_block_rate.as_of_height, None);
        assert_eq!(
            window(&metrics, StaleBlockRateRangeJson::Rolling { blocks: 5 }),
            &StaleBlockRateWindowJson::unavailable(
                StaleBlockRateRangeJson::Rolling { blocks: 5 },
                MetricUnavailableReason::TipNotInTree,
            )
        );
    }

    #[tokio::test]
    async fn returns_unavailable_when_no_reachable_node_can_report_stale_tips() {
        let (tree, hashes) = build_linear_tree(10);
        let node_data = node_data(&[(&hashes[10].to_string(), true)], false);

        let metrics =
            calculate_network_metrics(&tree, &node_data, &[StaleRateRange::Rolling(5)]).await;

        assert_eq!(metrics.stale_block_rate.as_of_height, None);
        assert_eq!(
            window(&metrics, StaleBlockRateRangeJson::Rolling { blocks: 5 }),
            &StaleBlockRateWindowJson::unavailable(
                StaleBlockRateRangeJson::Rolling { blocks: 5 },
                MetricUnavailableReason::NoReachableStaleTipSupport,
            )
        );
    }

    #[tokio::test]
    async fn marks_window_unavailable_when_history_contains_gap_inside_window() {
        let (tree, main_chain) = build_tree_with_unexpected_root();
        let node_data = node_data(&[(&main_chain[10].to_string(), true)], true);

        let metrics =
            calculate_network_metrics(&tree, &node_data, &[StaleRateRange::Rolling(5)]).await;

        assert_eq!(metrics.stale_block_rate.as_of_height, Some(10));
        assert_eq!(
            window(&metrics, StaleBlockRateRangeJson::Rolling { blocks: 5 }),
            &StaleBlockRateWindowJson::unavailable(
                StaleBlockRateRangeJson::Rolling { blocks: 5 },
                MetricUnavailableReason::IncompleteObservedHistory,
            )
        );
    }

    #[tokio::test]
    async fn allows_window_when_history_gap_starts_at_window_boundary() {
        let (tree, main_chain) = build_tree_with_unexpected_root();
        let node_data = node_data(&[(&main_chain[10].to_string(), true)], true);

        let metrics =
            calculate_network_metrics(&tree, &node_data, &[StaleRateRange::Rolling(3)]).await;

        assert_eq!(metrics.stale_block_rate.as_of_height, Some(10));
        assert_eq!(
            window(&metrics, StaleBlockRateRangeJson::Rolling { blocks: 3 }),
            &StaleBlockRateWindowJson {
                range: StaleBlockRateRangeJson::Rolling { blocks: 3 },
                stale_blocks: 3,
                active_blocks: 3,
                rate: 1.0,
                available: true,
                reason: None,
            }
        );
    }
}
