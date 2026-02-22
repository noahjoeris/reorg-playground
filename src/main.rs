use bitcoin_pool_identification::{PoolIdentification, default_data};
use bitcoincore_rpc::Error::JsonRpc;
use bitcoincore_rpc::bitcoin::{BlockHash, Network};
use env_logger::Env;
use log::{error, info, warn};
use petgraph::graph::NodeIndex;
use rusqlite::Connection;
use std::cmp::max;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use tokio::sync::mpsc::unbounded_channel;
use tokio::sync::{Mutex, broadcast};
use tokio::task;
use tokio::time::{Duration, Instant, interval_at, sleep};

use axum::{
    Router,
    routing::{get, post},
};

mod api;
mod cache;
mod config;
mod db;
mod error;
mod headertree;
mod jsonrpc;
mod node;
mod rss;
mod types;

use crate::cache::{
    CacheUpdate, MAX_FORKS_IN_CACHE, MINER_UNKNOWN, VERSION_UNKNOWN, is_node_reachable,
    update_cache,
};
use crate::config::BoxedSyncSendNode;
use crate::error::{DbError, MainError};
use types::{AppState, Caches, ChainTip, Db, HeaderInfo, NetworkJson, Tree};

async fn startup() -> Result<(config::Config, Db, Caches), MainError> {
    let config = config::load_config().map_err(|e| {
        error!("Could not load the configuration: {}", e);
        MainError::Config(e)
    })?;
    info!("Configuration loaded");

    let connection = Connection::open(config.database_path.clone()).map_err(|e| {
        error!(
            "Could not open the database {:?}: {}",
            config.database_path, e
        );
        MainError::Db(DbError::from(e))
    })?;
    info!("Opened database: {:?}", config.database_path);

    let db: Db = Arc::new(Mutex::new(connection));
    let caches: Caches = Arc::new(Mutex::new(BTreeMap::new()));

    db::setup_db(db.clone()).await.map_err(|e| {
        error!(
            "Could not setup the database {:?}: {}",
            config.database_path, e
        );
        MainError::Db(e)
    })?;
    info!("Database setup successful");

    Ok((config, db, caches))
}

#[tokio::main]
async fn main() -> Result<(), MainError> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
    let (config, db, caches) = startup().await?;

    let (cache_changed_tx, _) = broadcast::channel(16);
    let network_infos: Vec<NetworkJson> = config.networks.iter().map(NetworkJson::new).collect();

    for network in config.networks.iter().cloned() {
        info!(
            "initializing network '{}' (id={}): first_tracked_height={}, max_interesting_heights={}",
            network.name, network.id, network.first_tracked_height, network.max_interesting_heights
        );
        let tree_info =
            db::load_treeinfos(db.clone(), network.id, network.first_tracked_height)
                .await
                .map_err(|e| {
                    error!("Could not load headers from database: {}", e);
                    MainError::Db(e)
                })?;
        let tree: Tree = Arc::new(Mutex::new(tree_info));
        cache::populate_cache(&network, &tree, &caches).await;

        spawn_network_tasks(
            &network,
            tree,
            &db,
            &caches,
            &cache_changed_tx,
            config.query_interval,
        );
    }

    let state = AppState {
        caches: caches.clone(),
        network_infos,
        rss_base_url: config.rss_base_url.clone(),
        cache_changed_tx: cache_changed_tx.clone(),
        mine_info: Arc::new(config.mine_info),
    };

    let app = Router::new()
        .route("/api/{network_id}/data.json", get(api::data_response))
        .route("/api/networks.json", get(api::networks_response))
        .route("/api/changes", get(api::changes_sse))
        .route("/api/{network_id}/mine-block", post(api::mine_block))
        .route("/rss/{network_id}/forks.xml", get(rss::forks_response))
        .route(
            "/rss/{network_id}/invalid.xml",
            get(rss::invalid_blocks_response),
        )
        .route(
            "/rss/{network_id}/lagging.xml",
            get(rss::lagging_nodes_response),
        )
        .route(
            "/rss/{network_id}/unreachable.xml",
            get(rss::unreachable_nodes_response),
        )
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(config.address)
        .await
        .map_err(|e| {
            error!("Could not bind to {}: {}", config.address, e);
            MainError::Io(e)
        })?;
    info!("listening on {}", config.address);
    axum::serve(listener, app).await.map_err(|e| {
        error!("Server error: {}", e);
        MainError::Io(e)
    })?;
    Ok(())
}

/// Spawns three background tasks per network:
/// 1. Per-node polling task: queries tips + headers at `query_interval`
/// 2. One-shot backfill task: identifies miners for existing blocks (5 min after start)
/// 3. Miner identification task: processes block hashes from the miner_id channel
fn spawn_network_tasks(
    network: &config::Network,
    tree: Tree,
    db: &Db,
    caches: &Caches,
    cache_changed_tx: &broadcast::Sender<u32>,
    query_interval: Duration,
) {
    let (miner_id_tx, mut miner_id_rx) = unbounded_channel::<BlockHash>();

    info!(
        "network '{}' (id={}) has {} nodes",
        network.name,
        network.id,
        network.nodes.len()
    );

    for node in network.nodes.iter().cloned() {
        let network = network.clone();
        let mut interval = interval_at(
            Instant::now()
                + Duration::from_millis(
                    (query_interval.as_millis() / network.nodes.len() as u128) as u64,
                )
                + Duration::from_secs((network.id % 10) as u64),
            query_interval,
        );
        let db_write = db.clone();
        let tree_clone = tree.clone();
        let caches_clone = caches.clone();
        let cache_changed_tx_cloned = cache_changed_tx.clone();
        let miner_id_tx_clone = miner_id_tx.clone();

        let mut last_tips: Vec<ChainTip> = vec![];
        task::spawn(async move {
            update_cache(
                &caches_clone,
                network.id,
                CacheUpdate::NodeVersion {
                    node_id: node.info().id,
                    version: load_node_version(node.clone(), &network.name).await,
                },
                &cache_changed_tx_cloned,
            )
            .await;

            loop {
                interval.tick().await;
                let mut tips = match node.tips().await {
                    Ok(tips) => {
                        if !is_node_reachable(&caches_clone, network.id, node.info().id).await {
                            update_cache(
                                &caches_clone,
                                network.id,
                                CacheUpdate::NodeReachability {
                                    node_id: node.info().id,
                                    reachable: true,
                                },
                                &cache_changed_tx_cloned,
                            )
                            .await;
                        }
                        tips
                    }
                    Err(e) => {
                        error!(
                            "Could not fetch chaintips from {} on network '{}' (id={}): {:?}",
                            node.info(),
                            network.name,
                            network.id,
                            e
                        );
                        if is_node_reachable(&caches_clone, network.id, node.info().id).await {
                            update_cache(
                                &caches_clone,
                                network.id,
                                CacheUpdate::NodeReachability {
                                    node_id: node.info().id,
                                    reachable: false,
                                },
                                &cache_changed_tx_cloned,
                            )
                            .await;
                        }
                        continue;
                    }
                };

                tips.sort();

                if last_tips != tips {
                    // Set up a channel for incremental header processing.
                    // As headers are fetched from RPC, they are sent through this
                    // channel and processed (inserted into tree, written to DB,
                    // and broadcast via SSE) so the UI updates live.
                    let (progress_tx, mut progress_rx) = unbounded_channel::<Vec<HeaderInfo>>();
                    let tree_for_receiver = tree_clone.clone();
                    let db_for_receiver = db_write.clone();
                    let caches_for_receiver = caches_clone.clone();
                    let cache_changed_tx_for_receiver = cache_changed_tx_cloned.clone();
                    let network_for_receiver = network.clone();
                    let tips_for_receiver = tips.clone();

                    let receiver_handle = task::spawn(async move {
                        let mut total_written: usize = 0;
                        while let Some(batch) = progress_rx.recv().await {
                            if batch.is_empty() {
                                continue;
                            }
                            let tree_changed =
                                headertree::insert_headers(&tree_for_receiver, &batch).await;

                            match db::write_to_db(
                                &batch,
                                db_for_receiver.clone(),
                                network_for_receiver.id,
                            )
                            .await
                            {
                                Ok(_) => {
                                    total_written += batch.len();
                                }
                                Err(e) => {
                                    error!(
                                        "Could not write headers for network '{}' to database: {}",
                                        network_for_receiver.name, e
                                    );
                                }
                            }

                            if tree_changed {
                                let mut tip_heights: BTreeSet<u64> = cache::tip_heights(
                                    network_for_receiver.id,
                                    &caches_for_receiver,
                                )
                                .await;
                                for tip in tips_for_receiver.iter() {
                                    tip_heights.insert(tip.height);
                                }
                                let header_infos_json = headertree::strip_tree(
                                    &tree_for_receiver,
                                    network_for_receiver.max_interesting_heights,
                                    network_for_receiver.first_tracked_height,
                                    tip_heights,
                                )
                                .await;
                                let forks = headertree::recent_forks(
                                    &tree_for_receiver,
                                    MAX_FORKS_IN_CACHE,
                                )
                                .await;

                                update_cache(
                                    &caches_for_receiver,
                                    network_for_receiver.id,
                                    CacheUpdate::HeaderTree {
                                        header_infos_json,
                                        forks,
                                    },
                                    &cache_changed_tx_for_receiver,
                                )
                                .await;
                            }
                        }
                        total_written
                    });

                    let (_, miners_needed): (Vec<HeaderInfo>, Vec<BlockHash>) = match node
                        .new_headers(
                            &tips,
                            &tree_clone,
                            network.first_tracked_height,
                            Some(&progress_tx),
                        )
                        .await
                    {
                        Ok(headers) => headers,
                        Err(e) => {
                            error!(
                                "Could not fetch headers from {} on network '{}' (id={}): {}",
                                node.info(),
                                network.name,
                                network.id,
                                e
                            );
                            continue;
                        }
                    };

                    // Drop sender so receiver loop terminates
                    drop(progress_tx);
                    match receiver_handle.await {
                        Ok(total_written) => {
                            if total_written > 0 {
                                info!(
                                    "Written {} headers to database for network '{}' by node {}",
                                    total_written,
                                    network.name,
                                    node.info()
                                );
                            }
                        }
                        Err(e) => {
                            error!("Header processing task failed: {}", e);
                        }
                    }

                    for hash in miners_needed.iter() {
                        if let Err(e) = miner_id_tx_clone.send(*hash) {
                            error!(
                                "Could not send a block hash into the miner identification channel: {}",
                                e
                            );
                        }
                    }

                    last_tips = tips.clone();

                    update_cache(
                        &caches_clone,
                        network.id,
                        CacheUpdate::NodeTips {
                            node_id: node.info().id,
                            tips: tips.clone(),
                        },
                        &cache_changed_tx_cloned,
                    )
                    .await;
                }
            }
        });
    }

    // One-shot miner backfill (runs 5 min after startup)
    let tree_clone = tree.clone();
    let caches_clone = caches.clone();
    let network_clone = network.clone();
    let miner_id_tx_clone = miner_id_tx.clone();
    task::spawn(async move {
        sleep(Duration::from_secs(5 * 60)).await;

        let tip_heights: BTreeSet<u64> = cache::tip_heights(network_clone.id, &caches_clone).await;
        let interesting_heights = headertree::sorted_interesting_heights(
            &tree_clone,
            network_clone.max_interesting_heights,
            network_clone.first_tracked_height,
            tip_heights,
        )
        .await;

        let tree_locked = tree_clone.lock().await;

        for header_info in tree_locked
            .graph
            .raw_nodes()
            .iter()
            .filter(|node| node.weight.miner.is_empty() || node.weight.miner == MINER_UNKNOWN)
            .filter(|node| {
                let h = node.weight.height;
                interesting_heights.contains(&h)
                    || interesting_heights.contains(&(h + 1))
                    || interesting_heights.contains(&(h + 2))
                    || interesting_heights.contains(&(max(h, 1) - 1))
            })
            .map(|node| node.weight.clone())
        {
            if let Err(e) = miner_id_tx_clone.send(header_info.header.block_hash()) {
                error!(
                    "Could not send block hash into the miner identification channel: {}",
                    e
                );
            }
        }
    });

    // Miner identification consumer
    let tree_clone = tree.clone();
    let db_clone = db.clone();
    let caches_clone = caches.clone();
    let network_clone = network.clone();
    let network_for_miner = network.clone();
    let cache_changed_tx_clone = cache_changed_tx.clone();
    task::spawn(async move {
        let miner_network_type = match network_for_miner.network_type.as_ref() {
            Some(network_type) => network_type.as_bitcoin_network(),
            None => Network::Regtest,
        };
        let miner_identification_data = default_data(miner_network_type);

        let limit = 100;
        let mut buffer: Vec<BlockHash> = Vec::with_capacity(limit);
        loop {
            buffer.clear();
            miner_id_rx.recv_many(&mut buffer, limit).await;
            for hash in buffer.iter() {
                let idx: NodeIndex = {
                    let tree_locked = tree_clone.lock().await;
                    match tree_locked.index.get(hash) {
                        Some(idx) => *idx,
                        None => {
                            error!(
                                "Block hash {} not (yet) present in tree for network: {}. Skipping identification...",
                                hash, network_clone.name
                            );
                            continue;
                        }
                    }
                };

                let mut header_info = {
                    let tree_locked = tree_clone.lock().await;
                    tree_locked.graph[idx].clone()
                };

                if header_info.miner != MINER_UNKNOWN && !header_info.miner.is_empty() {
                    continue;
                }

                let mut miner = MINER_UNKNOWN.to_string();
                for node in network_clone.nodes.iter().cloned() {
                    match node
                        .coinbase(&header_info.header.block_hash(), header_info.height)
                        .await
                    {
                        Ok(coinbase) => {
                            miner = match coinbase
                                .identify_pool(miner_network_type, &miner_identification_data)
                            {
                                Some(result) => result.pool.name,
                                None => MINER_UNKNOWN.to_string(),
                            };
                        }
                        Err(e) => {
                            warn!(
                                "Could not get coinbase for block {} from node {}: {}",
                                header_info.header.block_hash(),
                                node.info().name,
                                e
                            );
                        }
                    }
                    if miner != MINER_UNKNOWN {
                        info!(
                            "Updated miner for block {} from node {}: {}",
                            header_info.height,
                            node.info().name,
                            miner
                        );
                        break;
                    }
                }
                header_info.update_miner(miner);

                {
                    let mut tree_locked = tree_clone.lock().await;
                    tree_locked.graph[idx] = header_info.clone();
                }
                if let Err(e) = db::update_miner(
                    db_clone.clone(),
                    &header_info.header.block_hash(),
                    header_info.miner.clone(),
                )
                .await
                {
                    warn!(
                        "Could not update miner to {} for block {}: {}",
                        header_info.miner,
                        header_info.header.block_hash(),
                        e
                    );
                }
                update_cache(
                    &caches_clone,
                    network_for_miner.id,
                    CacheUpdate::HeaderMiner { header_info },
                    &cache_changed_tx_clone,
                )
                .await;
            }
        }
    });
}

const NODE_VERSION_RETRIES: u32 = 5;
const NODE_VERSION_RETRY_DELAY: Duration = Duration::from_secs(10);

async fn load_node_version(node: BoxedSyncSendNode, network: &str) -> String {
    for attempt in 0..NODE_VERSION_RETRIES {
        match node.version().await {
            Ok(version) => return version,
            Err(e) => match e {
                error::FetchError::BitcoinCoreRPC(JsonRpc(msg)) => {
                    warn!(
                        "Could not fetch getnetworkinfo from node='{}' on network '{}': {:?}. Retrying in {:?}...",
                        node.info().name,
                        network,
                        msg,
                        NODE_VERSION_RETRY_DELAY
                    );
                }
                _ => {
                    error!(
                        "Could not load version from node='{}' on network='{}': {:?}",
                        node.info().name,
                        network,
                        e
                    );
                    return VERSION_UNKNOWN.to_string();
                }
            },
        }
        // Wait before next attempt
        if attempt < NODE_VERSION_RETRIES - 1 {
            tokio::time::sleep(NODE_VERSION_RETRY_DELAY).await;
        }
    }
    warn!(
        "Could not load version from node='{}' on network='{}' after {} attempts. Using '{}'.",
        node.info().name,
        network,
        NODE_VERSION_RETRIES,
        VERSION_UNKNOWN
    );
    VERSION_UNKNOWN.to_string()
}
