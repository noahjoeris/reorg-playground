use std::collections::BTreeMap;
use std::collections::BTreeSet;

use crate::types::{Fork, HeaderInfo, HeaderInfoJson, Tree};

use log::{debug, info, warn};
use petgraph::graph::NodeIndex;
use petgraph::visit::{Dfs, EdgeRef};

fn hotspot_budget(max_interesting_heights: usize) -> usize {
    if max_interesting_heights == 0 {
        return 0;
    }
    if max_interesting_heights <= 10 {
        return 2.min(max_interesting_heights);
    }
    (max_interesting_heights / 5).max(8)
}

/// Hybrid selection policy: always includes a stable recent window of
/// `max_interesting_heights`, then overlays a bounded set of fork/tip hotspots.
pub async fn sorted_interesting_heights(
    tree: &Tree,
    max_interesting_heights: usize,
    first_tracked_height: u64,
    tip_heights: BTreeSet<u64>,
) -> Vec<u64> {
    let tree_locked = tree.lock().await;
    if tree_locked.graph.node_count() == 0 {
        warn!("tried to collapse an empty tree!");
        return vec![];
    }
    if max_interesting_heights == 0 {
        warn!("max_interesting_heights=0; no heights can be selected");
        return vec![];
    }

    // Count how many blocks exist at each height (>1 means a fork).
    let mut height_occurences: BTreeMap<u64, usize> = BTreeMap::new();
    for node in tree_locked.graph.raw_nodes() {
        let counter = height_occurences.entry(node.weight.height).or_insert(0);
        *counter += 1;
    }

    let max_height: u64 = height_occurences
        .iter()
        .map(|(k, _)| *k)
        .max()
        .expect("we should have at least one height here as we have blocks");

    // 1. Always include the recent window from first_tracked_height onward.
    let window_start = max_height
        .saturating_sub(max_interesting_heights.saturating_sub(1) as u64)
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
    let hotspot_budget = hotspot_budget(max_interesting_heights);
    for h in hotspot_heights.iter().take(hotspot_budget) {
        interesting_heights_set.insert(*h);
    }
    let interesting_heights: Vec<u64> = interesting_heights_set.into_iter().collect();

    let fork_count = height_occurences.iter().filter(|(_, v)| **v > 1).count();

    debug!(
        "interesting heights: first_tracked_height={}, window_start={}, max_height={}, window_budget={}, hotspot_budget={}, fork_count={}, tip_count={}, selected={}",
        first_tracked_height,
        window_start,
        max_height,
        max_interesting_heights,
        hotspot_budget,
        fork_count,
        tip_heights.len(),
        interesting_heights.len(),
    );

    interesting_heights
}

// We strip the tree of headers that aren't interesting to us.
pub async fn strip_tree(
    tree: &Tree,
    max_interesting_heights: usize,
    first_tracked_height: u64,
    tip_heights: BTreeSet<u64>,
) -> Vec<HeaderInfoJson> {
    let interesting_heights = sorted_interesting_heights(
        tree,
        max_interesting_heights,
        first_tracked_height,
        tip_heights,
    )
    .await;

    let tree_locked = tree.lock().await;

    info!(
        "strip_tree: tree_nodes={}, first_tracked_height={}, interesting_heights={}",
        tree_locked.graph.node_count(),
        first_tracked_height,
        interesting_heights.len(),
    );

    let mut stripped_tree = tree_locked.graph.filter_map(
        |_, header| {
            for x in -2i64..=1 {
                if interesting_heights.contains(&((header.height as i64 - x) as u64)) {
                    return Some(header);
                }
            }
            None
        },
        |_, edge| Some(edge),
    );

    // We now have multiple sub header trees. To reconnect them
    // we figure out the starts of these chains (roots) and sort
    // them by height. We can't assume they are sorted as we
    // added data from multiple nodes to the tree.

    let mut roots: Vec<NodeIndex> = stripped_tree
        .externals(petgraph::Direction::Incoming)
        .collect();

    roots.sort_by_key(|idx| stripped_tree[*idx].height);

    let mut prev_header_to_connect_to: Option<NodeIndex> = None;
    for root in roots.iter() {
        if let Some(prev_idx) = prev_header_to_connect_to {
            stripped_tree.add_edge(prev_idx, *root, &false);
            prev_header_to_connect_to = None;
        }

        // Find the header with the maximum height in the sub chain via DFS.
        // This is the header we connect the next sub-chain root to.
        let mut max_height: u64 = u64::default();
        let mut dfs = Dfs::new(&stripped_tree, *root);
        while let Some(idx) = dfs.next(&stripped_tree) {
            let height = stripped_tree[idx].height;
            if height > max_height {
                max_height = height;
                prev_header_to_connect_to = Some(idx);
            }
        }
    }

    debug!(
        "done collapsing tree: roots={}, tips={}",
        stripped_tree
            .externals(petgraph::Direction::Incoming)
            .count(),
        stripped_tree
            .externals(petgraph::Direction::Outgoing)
            .count(),
    );

    let mut headers: Vec<HeaderInfoJson> = Vec::new();
    for idx in stripped_tree.node_indices() {
        let prev_nodes = stripped_tree.neighbors_directed(idx, petgraph::Direction::Incoming);
        let prev_node_index: usize = match prev_nodes.clone().count() {
            0 => usize::MAX, // signals "no parent" to the JavaScript frontend
            1 => prev_nodes
                .last()
                .expect("count was 1 so last() must succeed")
                .index(),
            n => {
                warn!(
                    "block at height {} has {} incoming edges; using first",
                    stripped_tree[idx].height, n
                );
                prev_nodes
                    .last()
                    .expect("count > 1 so last() must succeed")
                    .index()
            }
        };
        headers.push(HeaderInfoJson::new(
            stripped_tree[idx],
            idx.index(),
            prev_node_index,
        ));
    }

    // Sorting the headers by id helps debugging the API response.
    headers.sort_by_key(|h| h.id);

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
    for new in new_headers {
        let idx_new = *tree_locked
            .index
            .get(&new.header.block_hash())
            .expect("header was just inserted or already present");
        let idx_prev = match tree_locked.index.get(&new.header.prev_blockhash) {
            Some(idx) => *idx,
            None => continue,
        };
        tree_locked.graph.update_edge(idx_prev, idx_new, false);
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

    #[tokio::test]
    async fn test_no_forks_stable_recent_window() {
        let tree = build_linear_tree(100, 250);
        let tip_heights: BTreeSet<u64> = [250].into();
        let max_interesting = 100;

        let heights = sorted_interesting_heights(&tree, max_interesting, 100, tip_heights).await;

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
        let max_interesting = 100;

        let heights = sorted_interesting_heights(&tree, max_interesting, 100, tip_heights).await;

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
        let max_interesting = 150;

        let heights =
            sorted_interesting_heights(&tree, max_interesting, 937000, tip_heights).await;

        // The recent window should cover the full range since it fits within max_interesting
        assert!(
            heights.len() >= 100,
            "startup should still show many blocks, got {}",
            heights.len()
        );
        assert!(heights.contains(&937150), "must contain max height");
    }

    #[tokio::test]
    async fn test_strip_tree_output_count_stable() {
        let tree = build_linear_tree(937000, 937150);
        let max_interesting = 150;

        // Call with empty tips (startup)
        let result_startup = strip_tree(&tree, max_interesting, 937000, BTreeSet::new()).await;

        // Call with tip heights (live)
        let tip_heights: BTreeSet<u64> = [937150].into();
        let result_live = strip_tree(&tree, max_interesting, 937000, tip_heights).await;

        // Both should produce similar counts — not collapse from many to ~9
        let diff = (result_startup.len() as i64 - result_live.len() as i64).unsigned_abs();
        assert!(
            diff <= 5,
            "startup ({}) vs live ({}) header count should be close",
            result_startup.len(),
            result_live.len()
        );
    }

    #[tokio::test]
    async fn test_stale_tips_do_not_collapse_latest_window() {
        let tree = build_forked_tree(937000, 937831, 937404);
        let max_interesting = 150;
        let tip_heights: BTreeSet<u64> = [937831, 937404, 935976, 900000, 500000].into();

        let stripped = strip_tree(&tree, max_interesting, 937000, tip_heights).await;
        let heights: BTreeSet<u64> = stripped.iter().map(|h| h.height).collect();

        assert!(heights.contains(&937831), "must keep chain tip");
        assert!(heights.contains(&937404), "must keep known fork height");
        assert!(
            stripped.len() >= 120,
            "must keep a large recent window, got {} nodes",
            stripped.len()
        );
    }
}
