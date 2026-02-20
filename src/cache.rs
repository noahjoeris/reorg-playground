use std::collections::{BTreeSet, HashMap};
use std::fmt;

use log::debug;

use crate::headertree;
use crate::types::{
    Cache, Caches, ChainTip, Fork, HeaderInfo, HeaderInfoJson, NodeData, NodeDataJson, Tree,
};

pub const VERSION_UNKNOWN: &str = "unknown";
pub const MINER_UNKNOWN: &str = "Unknown";
pub const MAX_FORKS_IN_CACHE: usize = 50;

pub async fn populate_cache(
    network: &crate::config::Network,
    tree: &Tree,
    caches: &Caches,
) {
    let forks = headertree::recent_forks(tree, MAX_FORKS_IN_CACHE).await;
    let hij = headertree::strip_tree(tree, network.max_interesting_heights, BTreeSet::new()).await;
    let mut locked_caches = caches.lock().await;
    let node_data: NodeData = network
        .nodes
        .iter()
        .cloned()
        .map(|n| {
            (
                n.info().id,
                NodeDataJson::new(n.info(), &[], VERSION_UNKNOWN.to_string(), 0, true),
            )
        })
        .collect();
    locked_caches.insert(
        network.id,
        Cache {
            header_infos_json: hij.clone(),
            node_data,
            forks,
            recent_miners: vec![],
        },
    );
}

pub async fn tip_heights(network_id: u32, caches: &Caches) -> BTreeSet<u64> {
    let mut tip_heights: BTreeSet<u64> = BTreeSet::new();
    let locked_cache = caches.lock().await;
    let this_network = locked_cache
        .get(&network_id)
        .expect("network should already exist in cache");
    let node_infos: NodeData = this_network.node_data.clone();
    for node in node_infos.iter() {
        for tip in node.1.tips.iter() {
            tip_heights.insert(tip.height);
        }
    }
    tip_heights
}

#[derive(Debug)]
pub enum CacheUpdate {
    HeaderMiner {
        header_info: HeaderInfo,
    },
    HeaderTree {
        header_infos_json: Vec<HeaderInfoJson>,
        forks: Vec<Fork>,
    },
    NodeTips {
        node_id: u32,
        tips: Vec<ChainTip>,
    },
    NodeReachability {
        node_id: u32,
        reachable: bool,
    },
    NodeVersion {
        node_id: u32,
        version: String,
    },
}

impl fmt::Display for CacheUpdate {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            CacheUpdate::HeaderMiner { header_info } => {
                write!(
                    f,
                    "Setting miner of block {} to miner={}",
                    header_info.header.block_hash(),
                    header_info.miner
                )
            }
            CacheUpdate::HeaderTree {
                header_infos_json, ..
            } => match header_infos_json.last() {
                Some(last) => {
                    write!(
                        f,
                        "Updating headertree with last header hash={} and miner={}",
                        last.hash, last.miner
                    )
                }
                None => {
                    write!(f, "Updating headertree with empty header list")
                }
            },
            CacheUpdate::NodeTips { node_id, .. } => {
                write!(f, "Update tips of node={}", node_id)
            }
            CacheUpdate::NodeVersion { node_id, version } => {
                write!(f, "Update node={} version={}", node_id, version)
            }
            CacheUpdate::NodeReachability { node_id, reachable } => {
                write!(f, "Setting node {} to reachable={}", node_id, reachable)
            }
        }
    }
}

pub async fn is_node_reachable(caches: &Caches, network_id: u32, node_id: u32) -> bool {
    let locked_cache = caches.lock().await;
    locked_cache
        .get(&network_id)
        .expect("this network should be in the caches")
        .node_data
        .get(&node_id)
        .expect("this node should be in the network cache")
        .reachable
}

pub async fn update_cache(
    caches: &Caches,
    network_id: u32,
    update: CacheUpdate,
    cache_changed_tx: &tokio::sync::broadcast::Sender<u32>,
) {
    debug!("updating cache with: {}", update);
    let mut locked_cache = caches.lock().await;
    let network = locked_cache
        .get(&network_id)
        .expect("this network should be in the caches");
    match update {
        CacheUpdate::HeaderMiner { header_info } => {
            let mut old = network.header_infos_json.clone();
            if let Some(index) = old
                .iter()
                .position(|h| h.hash == header_info.header.block_hash().to_string())
            {
                old[index].update_miner(header_info.miner.clone());
            }

            locked_cache.entry(network_id).and_modify(|cache| {
                cache.header_infos_json = old;

                cache.recent_miners.push((
                    header_info.header.block_hash().to_string(),
                    header_info.miner,
                ));
                if cache.recent_miners.len() > 5 {
                    cache.recent_miners.remove(0);
                }
            });
        }
        CacheUpdate::HeaderTree {
            header_infos_json,
            forks,
        } => {
            let mut new_header_infos_map: HashMap<String, HeaderInfoJson> = header_infos_json
                .iter()
                .map(|h| (h.hash.clone(), h.clone()))
                .collect();
            for (hash, miner) in network.recent_miners.iter() {
                new_header_infos_map.entry(hash.clone()).and_modify(|new| {
                    new.update_miner(miner.clone());
                    debug!(
                        "During CacheUpdate::HeaderTree, updated miner of block {}: {}",
                        hash, miner
                    );
                });
            }

            locked_cache.entry(network_id).and_modify(|e| {
                e.header_infos_json = new_header_infos_map.into_values().collect();
                e.forks = forks;
            });
        }
        CacheUpdate::NodeTips { node_id, tips } => {
            let min_height = network
                .header_infos_json
                .iter()
                .min_by_key(|h| h.height)
                .map_or(0, |h| h.height);
            let relevant_tips: Vec<ChainTip> = tips
                .iter()
                .filter(|t| t.height >= min_height)
                .cloned()
                .collect();

            locked_cache.entry(network_id).and_modify(|network| {
                network
                    .node_data
                    .entry(node_id)
                    .and_modify(|e| e.tips(&relevant_tips));
            });
        }
        CacheUpdate::NodeReachability { node_id, reachable } => {
            locked_cache.entry(network_id).and_modify(|network| {
                network
                    .node_data
                    .entry(node_id)
                    .and_modify(|e| e.reachable(reachable));
            });
        }
        CacheUpdate::NodeVersion { node_id, version } => {
            locked_cache.entry(network_id).and_modify(|network| {
                network
                    .node_data
                    .entry(node_id)
                    .and_modify(|e| e.version(version));
            });
        }
    }

    match cache_changed_tx.send(network_id) {
        Ok(_) => debug!(
            "Sent a cache_changed notification for network={}.",
            network_id,
        ),
        Err(e) => {
            debug!(
                "Could not send cache_changed into the channel for network={}: {}",
                network_id, e
            )
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::NodeInfo;
    use std::sync::Arc;
    use tokio::sync::{Mutex, broadcast};

    async fn get_test_node_reachable(caches: &Caches, net_id: u32, node_id: u32) -> bool {
        let locked_caches = caches.lock().await;
        locked_caches
            .get(&net_id)
            .expect("network id should be there")
            .node_data
            .get(&node_id)
            .expect("node id should be there")
            .reachable
    }

    #[tokio::test]
    async fn test_node_reachable() {
        let network_id: u32 = 0;
        let (dummy_sender, _) = broadcast::channel(2);
        let caches: Caches = Arc::new(Mutex::new(BTreeMap::new()));
        let node = NodeInfo {
            id: 0,
            name: "".to_string(),
            description: "".to_string(),
            implementation: "".to_string(),
        };
        {
            let mut locked_caches = caches.lock().await;
            let mut node_data: NodeData = BTreeMap::new();
            node_data.insert(
                node.id,
                NodeDataJson::new(node.clone(), &[], "".to_string(), 0, true),
            );
            locked_caches.insert(
                network_id,
                Cache {
                    header_infos_json: vec![],
                    node_data,
                    forks: vec![],
                    recent_miners: vec![],
                },
            );
        }
        assert!(get_test_node_reachable(&caches, network_id, node.id).await);

        update_cache(
            &caches,
            network_id,
            CacheUpdate::NodeReachability {
                node_id: node.id,
                reachable: false,
            },
            &dummy_sender,
        )
        .await;
        assert!(!get_test_node_reachable(&caches, network_id, node.id).await);

        update_cache(
            &caches,
            network_id,
            CacheUpdate::NodeReachability {
                node_id: node.id,
                reachable: true,
            },
            &dummy_sender,
        )
        .await;
        assert!(get_test_node_reachable(&caches, network_id, node.id).await);
    }
}
