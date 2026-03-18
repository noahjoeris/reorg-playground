use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::HashMap;

use crate::types::{Fork, HeaderInfo, HeaderInfoJson, Tree};

use log::{debug, info, warn};
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::visit::{Dfs, EdgeRef};

/// Hybrid selection policy: always includes a stable recent window of
/// `visible_heights_from_tip`, then overlays up to `extra_hotspot_heights`
/// fork/tip hotspots.
pub async fn sorted_interesting_heights(
    tree: &Tree,
    visible_heights_from_tip: usize,
    extra_hotspot_heights: usize,
    first_tracked_height: u64,
    tip_heights: BTreeSet<u64>,
) -> Vec<u64> {
    let tree_locked = tree.lock().await;
    if tree_locked.graph.node_count() == 0 {
        warn!("tried to collapse an empty tree!");
        return vec![];
    }
    if visible_heights_from_tip == 0 {
        warn!("visible_heights_from_tip=0; no heights can be selected");
        return vec![];
    }

    // Count how many blocks exist at each height (>1 means a fork).
    let mut height_occurences: BTreeMap<u64, usize> = BTreeMap::new();
    for node in tree_locked.graph.raw_nodes() {
        let counter = height_occurences.entry(node.weight.height).or_insert(0);
        *counter += 1;
    }

    let max_height: u64 = height_occurences
        .keys()
        .copied()
        .max()
        .expect("we should have at least one height here as we have blocks");

    // 1. Always include the recent window from first_tracked_height onward.
    let window_start = max_height
        .saturating_sub(visible_heights_from_tip.saturating_sub(1) as u64)
        .max(first_tracked_height);
    let mut interesting_heights_set: BTreeSet<u64> = BTreeSet::new();
    for h in window_start..=max_height {
        if height_occurences.contains_key(&h) {
            interesting_heights_set.insert(h);
        }
    }

    // 2. Collect tip/fork hotspots and keep only heights we actually have in the tree.
    let mut hotspot_candidates: BTreeSet<u64> = height_occurences
        .iter()
        .filter(|(_, v)| **v > 1)
        .map(|(k, _)| *k)
        .collect();
    for h in &tip_heights {
        hotspot_candidates.insert(*h);
    }
    hotspot_candidates.insert(max_height);
    let mut hotspot_heights: Vec<u64> = hotspot_candidates
        .into_iter()
        .filter(|h| *h >= first_tracked_height)
        .filter(|h| height_occurences.contains_key(h))
        .collect();
    hotspot_heights.sort_unstable_by(|a, b| b.cmp(a));
    for h in hotspot_heights.iter().take(extra_hotspot_heights) {
        interesting_heights_set.insert(*h);
    }
    let interesting_heights: Vec<u64> = interesting_heights_set.into_iter().collect();

    let fork_count = height_occurences.iter().filter(|(_, v)| **v > 1).count();

    debug!(
        "interesting heights: first_tracked_height={}, window_start={}, max_height={}, visible_heights_from_tip={}, extra_hotspot_heights={}, fork_count={}, tip_count={}, selected={}",
        first_tracked_height,
        window_start,
        max_height,
        visible_heights_from_tip,
        extra_hotspot_heights,
        fork_count,
        tip_heights.len(),
        interesting_heights.len(),
    );

    interesting_heights
}

/// Serializes the tracked header tree for the API without rewriting parent edges.
pub async fn serialize_tree(tree: &Tree) -> Vec<HeaderInfoJson> {
    let tree_locked = tree.lock().await;
    info!(
        "serialize_tree: tree_nodes={}",
        tree_locked.graph.node_count()
    );
    graph_to_header_infos(&tree_locked.graph)
}

fn graph_to_header_infos(graph: &DiGraph<HeaderInfo, bool>) -> Vec<HeaderInfoJson> {
    let mut headers: Vec<HeaderInfoJson> = Vec::with_capacity(graph.node_count());

    for idx in graph.node_indices() {
        let parent_nodes = graph.neighbors_directed(idx, petgraph::Direction::Incoming);
        let parent_id = match parent_nodes.clone().count() {
            0 => usize::MAX, // signals "no parent" to the JavaScript frontend
            1 => parent_nodes
                .last()
                .expect("count was 1 so last() must succeed")
                .index(),
            parent_count => {
                warn!(
                    "block at height {} has {} incoming edges; using first",
                    graph[idx].height, parent_count
                );
                parent_nodes
                    .last()
                    .expect("count > 1 so last() must succeed")
                    .index()
            }
        };

        headers.push(HeaderInfoJson::new(&graph[idx], idx.index(), parent_id));
    }

    headers.sort_by_key(|header| header.id);
    headers
}

// get recent forks for rss
pub async fn recent_forks(tree: &Tree, how_many: usize) -> Vec<Fork> {
    let tree_locked = tree.lock().await;
    let tree = &tree_locked.graph;

    let mut forks: Vec<Fork> = vec![];
    // it could be, that we have multiple roots. To be safe, do this for all
    // roots.
    tree.externals(petgraph::Direction::Incoming)
        .for_each(|root| {
            let mut dfs = Dfs::new(&tree, root);
            while let Some(idx) = dfs.next(&tree) {
                let outgoing_iter = tree.edges_directed(idx, petgraph::Direction::Outgoing);
                if outgoing_iter.clone().count() > 1 {
                    let common = &tree[idx];
                    let fork = Fork {
                        common: common.clone(),
                        children: outgoing_iter
                            .map(|edge| tree[edge.target()].clone())
                            .collect(),
                    };
                    forks.push(fork);
                }
            }
        });

    forks.sort_by_key(|f| f.common.height);
    forks.iter().rev().take(how_many).cloned().collect()
}

/// Counts roots that indicate an unexpected gap above the tracked lower bound.
pub async fn unexpected_root_count(tree: &Tree, first_tracked_height: u64) -> usize {
    let tree_locked = tree.lock().await;
    tree_locked
        .graph
        .externals(petgraph::Direction::Incoming)
        .filter(|idx| tree_locked.graph[*idx].height > first_tracked_height)
        .count()
}

/// Returns disconnected subtree roots above `first_tracked_height`.
///
/// Each returned root indicates that the tracked tree is missing at least one
/// ancestor between `first_tracked_height` and the root height.
pub async fn unexpected_roots(tree: &Tree, first_tracked_height: u64) -> Vec<HeaderInfo> {
    let tree_locked = tree.lock().await;
    let mut roots: Vec<HeaderInfo> = tree_locked
        .graph
        .externals(petgraph::Direction::Incoming)
        .filter(|idx| tree_locked.graph[*idx].height > first_tracked_height)
        .map(|idx| tree_locked.graph[idx].clone())
        .collect();
    roots.sort_by_key(|header| header.height);
    roots
}

/// Inserts new headers as nodes and edges into the tree. Returns true if
/// any new nodes were added (i.e. the tree changed).
pub async fn insert_headers(tree: &Tree, new_headers: &[HeaderInfo]) -> bool {
    let mut tree_changed = false;
    let mut tree_locked = tree.lock().await;
    for h in new_headers {
        if !tree_locked.index.contains_key(&h.header.block_hash()) {
            let idx = tree_locked.graph.add_node(h.clone());
            tree_locked.index.insert(h.header.block_hash(), idx);
            tree_changed = true;
        }
    }

    let children_by_prev: HashMap<_, Vec<NodeIndex>> =
        tree_locked
            .graph
            .node_indices()
            .fold(HashMap::new(), |mut acc, idx| {
                acc.entry(tree_locked.graph[idx].header.prev_blockhash)
                    .or_default()
                    .push(idx);
                acc
            });

    for new in new_headers {
        let idx_new = *tree_locked
            .index
            .get(&new.header.block_hash())
            .expect("header was just inserted or already present");
        if let Some(&idx_prev) = tree_locked.index.get(&new.header.prev_blockhash) {
            tree_locked.graph.update_edge(idx_prev, idx_new, false);
        }
        if let Some(children) = children_by_prev.get(&new.header.block_hash()) {
            for idx_child in children {
                tree_locked.graph.update_edge(idx_new, *idx_child, false);
            }
        }
    }
    tree_changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::TreeInfo;
    use bitcoincore_rpc::bitcoin::blockdata::block::Header;
    use bitcoincore_rpc::bitcoin::hashes::Hash;
    use bitcoincore_rpc::bitcoin::{BlockHash, CompactTarget, TxMerkleNode};
    use petgraph::graph::DiGraph;
    use std::collections::HashMap;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    /// Create a fake header with a given prev_blockhash. The block_hash will
    /// be deterministic based on the nonce (which we set to `height` for uniqueness).
    fn make_header(prev: BlockHash, height: u64) -> Header {
        Header {
            version: bitcoincore_rpc::bitcoin::block::Version::from_consensus(1),
            prev_blockhash: prev,
            merkle_root: TxMerkleNode::all_zeros(),
            time: height as u32,
            bits: CompactTarget::from_consensus(0x1d00ffff),
            nonce: height as u32,
        }
    }

    /// Build a linear chain tree from `start_height` to `end_height` (inclusive).
    fn build_linear_tree(start_height: u64, end_height: u64) -> Tree {
        let mut graph: DiGraph<HeaderInfo, bool> = DiGraph::new();
        let mut index: HashMap<BlockHash, petgraph::graph::NodeIndex> = HashMap::new();

        let mut prev_hash = BlockHash::all_zeros();
        for h in start_height..=end_height {
            let header = make_header(prev_hash, h);
            let hash = header.block_hash();
            let info = HeaderInfo {
                height: h,
                header,
                miner: String::new(),
            };
            let idx = graph.add_node(info);
            index.insert(hash, idx);
            prev_hash = hash;
        }

        // Add edges
        for idx in graph.node_indices().collect::<Vec<_>>() {
            let prev_blockhash = graph[idx].header.prev_blockhash;
            if let Some(&prev_idx) = index.get(&prev_blockhash) {
                graph.update_edge(prev_idx, idx, false);
            }
        }

        Arc::new(Mutex::new(TreeInfo { graph, index }))
    }

    /// Build a chain with a single fork at `fork_height`.
    fn build_forked_tree(start_height: u64, end_height: u64, fork_height: u64) -> Tree {
        let tree = build_linear_tree(start_height, end_height);

        // Add a second block at fork_height branching off fork_height-1
        let tree_info = tree.try_lock().unwrap();
        let mut graph = tree_info.graph.clone();
        let mut index = tree_info.index.clone();
        drop(tree_info);

        // Find the block at fork_height - 1 (the fork point)
        let fork_parent_hash = graph
            .raw_nodes()
            .iter()
            .find(|n| n.weight.height == fork_height.saturating_sub(1))
            .map(|n| n.weight.header.block_hash())
            .unwrap_or(BlockHash::all_zeros());

        // Create an alternative block at the fork height with a different nonce
        let alt_header = Header {
            version: bitcoincore_rpc::bitcoin::block::Version::from_consensus(2),
            prev_blockhash: fork_parent_hash,
            merkle_root: TxMerkleNode::all_zeros(),
            time: fork_height as u32,
            bits: CompactTarget::from_consensus(0x1d00ffff),
            nonce: (fork_height + 999999) as u32,
        };
        let alt_hash = alt_header.block_hash();
        let alt_info = HeaderInfo {
            height: fork_height,
            header: alt_header,
            miner: String::new(),
        };
        let alt_idx = graph.add_node(alt_info);
        index.insert(alt_hash, alt_idx);

        // Connect to fork parent
        if let Some(&parent_idx) = index.get(&fork_parent_hash) {
            graph.update_edge(parent_idx, alt_idx, false);
        }

        Arc::new(Mutex::new(TreeInfo { graph, index }))
    }

    fn build_tree(headers: &[(u64, Header)]) -> Tree {
        let mut graph: DiGraph<HeaderInfo, bool> = DiGraph::new();
        let mut index: HashMap<BlockHash, petgraph::graph::NodeIndex> = HashMap::new();

        for (height, header) in headers {
            let idx = graph.add_node(HeaderInfo {
                height: *height,
                header: *header,
                miner: String::new(),
            });
            index.insert(header.block_hash(), idx);
        }

        for idx in graph.node_indices().collect::<Vec<_>>() {
            let prev_blockhash = graph[idx].header.prev_blockhash;
            if let Some(&prev_idx) = index.get(&prev_blockhash) {
                graph.update_edge(prev_idx, idx, false);
            }
        }

        Arc::new(Mutex::new(TreeInfo { graph, index }))
    }

    #[tokio::test]
    async fn test_no_forks_stable_recent_window() {
        let tree = build_linear_tree(100, 250);
        let tip_heights: BTreeSet<u64> = [250].into();
        let visible_heights_from_tip = 100;
        let extra_hotspot_heights = 20;

        let heights = sorted_interesting_heights(
            &tree,
            visible_heights_from_tip,
            extra_hotspot_heights,
            100,
            tip_heights,
        )
        .await;

        // Should include the recent window: 151..=250 (100 heights)
        assert!(
            heights.len() >= 100,
            "expected at least 100 heights, got {}",
            heights.len()
        );
        assert!(heights.contains(&250), "must contain tip");
        assert!(heights.contains(&151), "must contain window start");
    }

    #[tokio::test]
    async fn test_single_fork_keeps_window_and_fork() {
        // Chain from 100..250 with a fork at height 120
        let tree = build_forked_tree(100, 250, 120);
        let tip_heights: BTreeSet<u64> = [250].into();
        let visible_heights_from_tip = 100;
        let extra_hotspot_heights = 20;

        let heights = sorted_interesting_heights(
            &tree,
            visible_heights_from_tip,
            extra_hotspot_heights,
            100,
            tip_heights,
        )
        .await;

        // Must contain the tip and the fork height
        assert!(heights.contains(&250), "must contain tip");
        assert!(heights.contains(&120), "must contain fork height");
        // Must also have the recent window
        assert!(
            heights.contains(&200),
            "must contain heights in recent window"
        );
    }

    #[tokio::test]
    async fn test_empty_tip_heights_still_shows_recent_window() {
        // Simulates startup where no node tips are known yet
        let tree = build_linear_tree(937000, 937150);
        let tip_heights: BTreeSet<u64> = BTreeSet::new();
        let visible_heights_from_tip = 150;
        let extra_hotspot_heights = 30;

        let heights = sorted_interesting_heights(
            &tree,
            visible_heights_from_tip,
            extra_hotspot_heights,
            937000,
            tip_heights,
        )
        .await;

        // The recent window should cover the full range since it fits within the tip window size.
        assert!(
            heights.len() >= 100,
            "startup should still show many blocks, got {}",
            heights.len()
        );
        assert!(heights.contains(&937150), "must contain max height");
    }

    #[tokio::test]
    async fn serialize_tree_returns_all_tracked_blocks() {
        let tree = build_linear_tree(937000, 937150);
        let headers = serialize_tree(&tree).await;

        assert_eq!(headers.len(), 151);
        assert_eq!(headers.first().expect("root").height, 937000);
        assert_eq!(headers.last().expect("tip").height, 937150);
    }

    #[tokio::test]
    async fn serialize_tree_preserves_real_parent_relationships() {
        let tree = build_linear_tree(100, 110);
        let headers = serialize_tree(&tree).await;
        let headers_by_id: HashMap<usize, HeaderInfoJson> = headers
            .iter()
            .cloned()
            .map(|header| (header.id, header))
            .collect();

        for header in headers {
            if header.prev_id == usize::MAX {
                assert_eq!(header.height, 100);
                continue;
            }

            let parent = headers_by_id
                .get(&header.prev_id)
                .expect("serialized parent should exist");
            assert_eq!(
                header.height,
                parent.height + 1,
                "serialized edge should connect real parent-child heights"
            );
        }
    }

    #[tokio::test]
    async fn serialize_tree_keeps_gap_roots_visible() {
        let complete_headers: Vec<(u64, Header)> = (100..=110)
            .scan(BlockHash::all_zeros(), |prev_hash, height| {
                let header = make_header(*prev_hash, height);
                *prev_hash = header.block_hash();
                Some((height, header))
            })
            .collect();
        let missing_tree_headers: Vec<(u64, Header)> = complete_headers
            .iter()
            .copied()
            .filter(|(height, _)| *height != 105 && *height != 106)
            .collect();
        let tree = build_tree(&missing_tree_headers);
        let headers = serialize_tree(&tree).await;
        let root_heights: Vec<u64> = headers
            .iter()
            .filter(|header| header.prev_id == usize::MAX)
            .map(|header| header.height)
            .collect();

        assert_eq!(root_heights, vec![100, 107]);
    }

    #[tokio::test]
    async fn unexpected_root_count_ignores_root_at_first_tracked_height() {
        let tree = build_linear_tree(100, 110);

        assert_eq!(unexpected_root_count(&tree, 100).await, 0);
    }

    #[tokio::test]
    async fn inserting_gap_headers_clears_unexpected_roots() {
        let complete_headers: Vec<(u64, Header)> = (100..=110)
            .scan(BlockHash::all_zeros(), |prev_hash, height| {
                let header = make_header(*prev_hash, height);
                *prev_hash = header.block_hash();
                Some((height, header))
            })
            .collect();
        let missing_tree_headers: Vec<(u64, Header)> = complete_headers
            .iter()
            .copied()
            .filter(|(height, _)| *height != 105 && *height != 106)
            .collect();
        let tree = build_tree(&missing_tree_headers);
        let missing_headers: Vec<HeaderInfo> = complete_headers
            .iter()
            .copied()
            .filter(|(height, _)| *height == 105 || *height == 106)
            .map(|(height, header)| HeaderInfo {
                height,
                header,
                miner: String::new(),
            })
            .collect();

        assert_eq!(unexpected_root_count(&tree, 100).await, 1);

        let tree_changed = insert_headers(&tree, &missing_headers).await;

        assert!(tree_changed);
        assert_eq!(unexpected_root_count(&tree, 100).await, 0);
    }
}
