use bitcoincore_rpc::Error::JsonRpc;
use bitcoincore_rpc::bitcoin::BlockHash;
use env_logger::Env;
use log::{error, info, warn};
use petgraph::graph::NodeIndex;
use rusqlite::Connection;
use std::cmp::max;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
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
mod metrics;
mod node;
mod peer_api;
mod rss;
mod types;

use crate::cache::{
    CacheUpdate, MAX_FORKS_IN_CACHE, MINER_UNKNOWN, VERSION_UNKNOWN, is_node_reachable,
    update_cache,
};
use crate::error::{DbError, MainError};
use crate::node::{Node, fetch_missing_headers_for_unexpected_roots};
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
    // Peer-control actions publish network ids here so `/api/peer-changes` subscribers can refetch.
    let (peer_changed_tx, _) = broadcast::channel(16);
    let network_infos: Vec<NetworkJson> = config.networks.iter().map(NetworkJson::new).collect();

    for network in config.networks.iter().cloned() {
        info!(
            "initializing network '{}' (id={}): first_tracked_height={}, visible_heights_from_tip={}, extra_hotspot_heights={}",
            network.name,
            network.id,
            network.first_tracked_height,
            network.visible_heights_from_tip,
            network.extra_hotspot_heights
        );
        let tree_info = db::load_treeinfos(db.clone(), network.id, network.first_tracked_height)
            .await
            .map_err(|e| {
                error!("Could not load headers from database: {}", e);
                MainError::Db(e)
            })?;
        let tree: Tree = Arc::new(Mutex::new(tree_info));
        let unexpected_roots =
            headertree::unexpected_root_count(&tree, network.first_tracked_height).await;
        if unexpected_roots > 0 {
            warn!(
                "network '{}' loaded with {} unexpected roots above first_tracked_height={}",
                network.name, unexpected_roots, network.first_tracked_height
            );
        }
        cache::populate_cache(&network, &tree, &caches).await;

        spawn_network_tasks(&network, tree, &db, &caches, &cache_changed_tx);
    }

    let state = AppState {
        caches: caches.clone(),
        networks: config.networks.clone(),
        network_infos,
        rss_base_url: config.rss_base_url.clone(),
        cache_changed_tx: cache_changed_tx.clone(),
        peer_changed_tx: peer_changed_tx.clone(),
    };

    let app = Router::new()
        .route("/api/{network_id}/data.json", get(api::data_response))
        .route(
            "/api/{network_id}/p2p-state.json",
            get(api::p2p_state_response),
        )
        .route("/api/networks.json", get(api::networks_response))
        .route("/api/cache-changes", get(api::cache_changes_sse))
        .route("/api/{network_id}/mine-block", post(api::mine_block))
        .route(
            "/api/{network_id}/network-active",
            post(api::set_network_active),
        )
        .route(
            "/api/{network_id}/peer-info.json",
            get(peer_api::peer_info_response),
        )
        .route("/api/peer-changes", get(peer_api::peer_changes_sse))
        .route("/api/{network_id}/add-node", post(peer_api::add_node))
        .route(
            "/api/{network_id}/disconnect-node",
            post(peer_api::disconnect_node),
        )
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

/// Rebuilds the cached tree payload after the in-memory tree changes.
async fn refresh_network_tree_cache(
    tree: &Tree,
    caches: &Caches,
    cache_changed_tx: &broadcast::Sender<u32>,
    network: &config::Network,
) {
    let header_infos_json = headertree::serialize_tree(tree).await;
    let forks = headertree::recent_forks(tree, MAX_FORKS_IN_CACHE).await;

    update_cache(
        caches,
        tree,
        &network.stale_rate_ranges,
        network.id,
        CacheUpdate::HeaderTree {
            header_infos_json,
            forks,
        },
        cache_changed_tx,
    )
    .await;
}

struct NetworkPollContext<'a> {
    tree: &'a Tree,
    db: &'a Db,
    caches: &'a Caches,
    cache_changed_tx: &'a broadcast::Sender<u32>,
    network: &'a config::Network,
    miner_id_tx: &'a UnboundedSender<BlockHash>,
}

fn queue_miner_identification_requests(
    miner_id_tx: &UnboundedSender<BlockHash>,
    block_hashes: impl IntoIterator<Item = BlockHash>,
) {
    for block_hash in block_hashes {
        if let Err(e) = miner_id_tx.send(block_hash) {
            error!(
                "Could not send a block hash into the miner identification channel: {}",
                e
            );
        }
    }
}

/// Writes fetched headers to the tree and database, then refreshes the cache if needed.
async fn persist_headers(
    headers: &[HeaderInfo],
    tree: &Tree,
    db: &Db,
    caches: &Caches,
    cache_changed_tx: &broadcast::Sender<u32>,
    network: &config::Network,
) -> usize {
    if headers.is_empty() {
        return 0;
    }

    let tree_changed = headertree::insert_headers(tree, headers).await;
    let persisted_header_count = match db::write_to_db(headers, db.clone(), network.id).await {
        Ok(_) => headers.len(),
        Err(e) => {
            error!(
                "Could not write headers for network '{}' to database: {}",
                network.name, e
            );
            0
        }
    };

    if tree_changed {
        refresh_network_tree_cache(tree, caches, cache_changed_tx, network).await;
    }

    persisted_header_count
}

/// Consumes progress batches so the UI and database update while fetching is still running.
async fn process_header_progress_updates(
    mut progress_rx: UnboundedReceiver<Vec<HeaderInfo>>,
    tree: Tree,
    db: Db,
    caches: Caches,
    cache_changed_tx: broadcast::Sender<u32>,
    network: config::Network,
) -> usize {
    let mut total_persisted_headers = 0;

    while let Some(batch) = progress_rx.recv().await {
        total_persisted_headers +=
            persist_headers(&batch, &tree, &db, &caches, &cache_changed_tx, &network).await;
    }

    total_persisted_headers
}

/// Loads and sorts chain tips from a node while keeping its reachability state in sync.
async fn load_sorted_tips(
    node: &Arc<dyn Node>,
    ctx: &NetworkPollContext<'_>,
) -> Option<Vec<ChainTip>> {
    let mut tips = match node.tips().await {
        Ok(tips) => {
            if !is_node_reachable(ctx.caches, ctx.network.id, node.info().id).await {
                update_cache(
                    ctx.caches,
                    ctx.tree,
                    &ctx.network.stale_rate_ranges,
                    ctx.network.id,
                    CacheUpdate::NodeReachability {
                        node_id: node.info().id,
                        reachable: true,
                    },
                    ctx.cache_changed_tx,
                )
                .await;
            }
            tips
        }
        Err(e) => {
            error!(
                "Could not fetch chaintips from {} (endpoint={}) on network '{}' (id={}): {:?}",
                node.info(),
                node.endpoint(),
                ctx.network.name,
                ctx.network.id,
                e
            );
            if is_node_reachable(ctx.caches, ctx.network.id, node.info().id).await {
                update_cache(
                    ctx.caches,
                    ctx.tree,
                    &ctx.network.stale_rate_ranges,
                    ctx.network.id,
                    CacheUpdate::NodeReachability {
                        node_id: node.info().id,
                        reachable: false,
                    },
                    ctx.cache_changed_tx,
                )
                .await;
            }
            return None;
        }
    };

    tips.sort();
    Some(tips)
}

/// Runs the normal append-only fetch path for a changed tip set.
async fn fetch_incremental_headers(
    node: &Arc<dyn Node>,
    ctx: &NetworkPollContext<'_>,
    tips: &[ChainTip],
) -> bool {
    let (progress_tx, progress_rx) = unbounded_channel::<Vec<HeaderInfo>>();
    let progress_handle = task::spawn(process_header_progress_updates(
        progress_rx,
        ctx.tree.clone(),
        ctx.db.clone(),
        ctx.caches.clone(),
        ctx.cache_changed_tx.clone(),
        ctx.network.clone(),
    ));

    let fetch_result = node
        .get_new_headers(
            tips,
            ctx.tree,
            ctx.network.first_tracked_height,
            Some(&progress_tx),
        )
        .await;
    drop(progress_tx);

    let total_persisted_headers = match progress_handle.await {
        Ok(total_persisted_headers) => total_persisted_headers,
        Err(e) => {
            error!("Header processing task failed: {}", e);
            0
        }
    };

    if total_persisted_headers > 0 {
        info!(
            "Written {} headers to database for network '{}' by node {}",
            total_persisted_headers,
            ctx.network.name,
            node.info()
        );
    }

    let (_, miner_hashes) = match fetch_result {
        Ok(result) => result,
        Err(e) => {
            error!(
                "Could not fetch headers from {} (endpoint={}) on network '{}' (id={}): {}",
                node.info(),
                node.endpoint(),
                ctx.network.name,
                ctx.network.id,
                e
            );
            return false;
        }
    };

    queue_miner_identification_requests(ctx.miner_id_tx, miner_hashes);
    true
}

async fn update_node_tips_cache(
    ctx: &NetworkPollContext<'_>,
    node: &Arc<dyn Node>,
    tips: &[ChainTip],
) {
    update_cache(
        ctx.caches,
        ctx.tree,
        &ctx.network.stale_rate_ranges,
        ctx.network.id,
        CacheUpdate::NodeTips {
            node_id: node.info().id,
            tips: tips.to_vec(),
        },
        ctx.cache_changed_tx,
    )
    .await;
}

/// Repairs disconnected tracked subtrees by fetching the headers below their roots.
async fn repair_missing_headers_from_unexpected_roots(
    node: &Arc<dyn Node>,
    ctx: &NetworkPollContext<'_>,
) {
    let unexpected_root_count =
        headertree::unexpected_root_count(ctx.tree, ctx.network.first_tracked_height).await;
    if unexpected_root_count == 0 {
        return;
    }

    info!(
        "repairing {} unexpected roots for network '{}' using node {}",
        unexpected_root_count,
        ctx.network.name,
        node.info()
    );

    let missing_headers = match fetch_missing_headers_for_unexpected_roots(
        node.as_ref(),
        ctx.tree,
        ctx.network.first_tracked_height,
        None,
    )
    .await
    {
        Ok(headers) => headers,
        Err(e) => {
            error!(
                "Could not repair header gaps from {} (endpoint={}) on network '{}' (id={}): {}",
                node.info(),
                node.endpoint(),
                ctx.network.name,
                ctx.network.id,
                e
            );
            return;
        }
    };

    if missing_headers.is_empty() {
        return;
    }

    let persisted_header_count = persist_headers(
        &missing_headers,
        ctx.tree,
        ctx.db,
        ctx.caches,
        ctx.cache_changed_tx,
        ctx.network,
    )
    .await;
    if persisted_header_count > 0 {
        info!(
            "Repaired {} missing headers for network '{}' using node {}",
            missing_headers.len(),
            ctx.network.name,
            node.info()
        );
    }

    queue_miner_identification_requests(
        ctx.miner_id_tx,
        missing_headers
            .iter()
            .map(|header| header.header.block_hash()),
    );

    let remaining_unexpected_roots =
        headertree::unexpected_root_count(ctx.tree, ctx.network.first_tracked_height).await;
    if remaining_unexpected_roots > 0 {
        warn!(
            "network '{}' still has {} unexpected roots after repair attempt",
            ctx.network.name, remaining_unexpected_roots
        );
    }
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
) {
    let (miner_id_tx, mut miner_id_rx) = unbounded_channel::<BlockHash>();

    info!(
        "network '{}' (id={}) has {} nodes",
        network.name,
        network.id,
        network.nodes.len()
    );

    for node in &network.nodes {
        let node = Arc::clone(node);
        let network = network.clone();
        let query_interval = network.query_interval;
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
                &tree_clone,
                &network.stale_rate_ranges,
                network.id,
                CacheUpdate::NodeVersion {
                    node_id: node.info().id,
                    version: load_node_version(node.clone(), &network.name).await,
                },
                &cache_changed_tx_cloned,
            )
            .await;

            let poll_context = NetworkPollContext {
                tree: &tree_clone,
                db: &db_write,
                caches: &caches_clone,
                cache_changed_tx: &cache_changed_tx_cloned,
                network: &network,
                miner_id_tx: &miner_id_tx_clone,
            };

            loop {
                interval.tick().await;
                let tips = match load_sorted_tips(&node, &poll_context).await {
                    Some(tips) => tips,
                    None => continue,
                };

                if last_tips != tips {
                    if !fetch_incremental_headers(&node, &poll_context, &tips).await {
                        continue;
                    }

                    last_tips = tips.clone();

                    update_node_tips_cache(&poll_context, &node, &tips).await;
                }

                repair_missing_headers_from_unexpected_roots(&node, &poll_context).await;
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
            network_clone.visible_heights_from_tip,
            network_clone.extra_hotspot_heights,
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
        let miner_network_type = network_for_miner.network_type.as_bitcoin_network();

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
                for node in &network_clone.nodes {
                    let node = Arc::clone(node);
                    match node
                        .get_miner_pool(
                            &header_info.header.block_hash(),
                            header_info.height,
                            miner_network_type,
                        )
                        .await
                    {
                        Ok(Some(pool_name)) => {
                            miner = pool_name;
                        }
                        Ok(None) => {}
                        Err(e) => {
                            warn!(
                                "Could not identify miner pool for block {} from node {}: {}",
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
                    &tree_clone,
                    &network_for_miner.stale_rate_ranges,
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

async fn load_node_version(node: Arc<dyn Node>, network: &str) -> String {
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
