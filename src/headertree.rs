use std::collections::BTreeMap;
use std::collections::BTreeSet;

use crate::types::{Fork, HeaderInfo, HeaderInfoJson, Tree};

use log::{debug, warn};
use petgraph::graph::NodeIndex;
use petgraph::visit::{Dfs, EdgeRef};

pub async fn sorted_interesting_heights(
    tree: &Tree,
    max_interesting_heights: usize,
    tip_heights: BTreeSet<u64>,
) -> Vec<u64> {
    let tree_locked = tree.lock().await;
    if tree_locked.graph.node_count() == 0 {
        warn!("tried to collapse an empty tree!");
        return vec![];
    }

    // We are intersted in all heights where we know more than one block
    // (as this indicates a fork).
    let mut height_occurences: BTreeMap<u64, usize> = BTreeMap::new();
    for node in tree_locked.graph.raw_nodes() {
        let counter = height_occurences.entry(node.weight.height).or_insert(0);
        *counter += 1;
    }
    let heights_with_multiple_blocks: Vec<u64> = height_occurences
        .iter()
        .filter(|(_, v)| **v > 1)
        .map(|(k, _)| *k)
        .collect();

    let mut interesting_heights_set: BTreeSet<u64> = heights_with_multiple_blocks
        .iter()
        .copied()
        .chain(tip_heights)
        .collect();

    // We are also interested in the block with the max height. We should
    // already have that in `tip_heights`, but include it here just to be
    // sure.
    let max_height: u64 = height_occurences
        .iter()
        .map(|(k, _)| *k)
        .max()
        .expect("we should have at least one height here as we have blocks");
    interesting_heights_set.insert(max_height);

    // For a linear chain (no forks), we only have the tip height so the UI would
    // show ~3 blocks. Add recent heights so short chains (e.g. regtest) show more.
    if heights_with_multiple_blocks.is_empty() {
        let start = max_height.saturating_sub(max_interesting_heights as u64);
        for h in start..=max_height {
            interesting_heights_set.insert(h);
        }
    }

    // As, for example, testnet has a lot of forks we'd return many headers
    // via the API (causing things to slow down), we allow limiting this with
    // max_interesting_heights. BTreeSet iterates in ascending order, so
    // rev→take→rev gives us the highest N heights in ascending order.
    let interesting_heights: Vec<u64> = interesting_heights_set
        .iter()
        .copied()
        .rev()
        .take(max_interesting_heights)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();

    interesting_heights
}

// We strip the tree of headers that aren't interesting to us.
pub async fn strip_tree(
    tree: &Tree,
    max_interesting_heights: usize,
    tip_heights: BTreeSet<u64>,
) -> Vec<HeaderInfoJson> {
    let interesting_heights =
        sorted_interesting_heights(tree, max_interesting_heights, tip_heights).await;

    let tree_locked = tree.lock().await;

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
