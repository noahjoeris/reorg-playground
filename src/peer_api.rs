use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
};
use futures_util::StreamExt;
use futures_util::future::{join_all, ready};
use futures_util::stream::Stream;
use log::{error, warn};
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::time::Duration;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;

use crate::api::{CacheChangesQuery, ResyncRequired, get_network, get_node};
use crate::config::Network;
use crate::error::FetchError;
use crate::node::{Node, PeerInfo};
use crate::types::AppState;

// -- API payloads --

/// Peer info annotated with the configured node we believe is on the other end of the connection.
#[derive(Serialize)]
pub(crate) struct MatchedPeerInfo {
    #[serde(flatten)]
    pub peer: PeerInfo,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matched_node_id: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matched_node_name: Option<String>,
}

impl MatchedPeerInfo {
    fn from_peer_and_match(peer: PeerInfo, matched_node: Option<ConfiguredNodeRef>) -> Self {
        match matched_node {
            Some(node) => Self {
                peer,
                matched_node_id: Some(node.id),
                matched_node_name: Some(node.name),
            },
            None => Self {
                peer,
                matched_node_id: None,
                matched_node_name: None,
            },
        }
    }
}

/// Peer connections currently visible from one configured node.
#[derive(Serialize)]
pub(crate) struct NodePeerConnections {
    node_id: u32,
    peers: Vec<MatchedPeerInfo>,
}

/// Best-known routable listen address for a configured node.
///
/// We prefer a live address derived from peer info and fall back to the configured `p2p_address`
/// when peer discovery cannot infer one.
#[derive(Serialize, Clone)]
pub(crate) struct NodeListenAddress {
    pub node_id: u32,
    pub name: String,
    pub address: String,
}

/// Current peer topology view for one network.
#[derive(Serialize)]
pub(crate) struct PeerConnectionsResponse {
    nodes: Vec<NodePeerConnections>,
    node_p2p_addresses: Vec<NodeListenAddress>,
}

impl PeerConnectionsResponse {
    fn empty() -> Self {
        Self {
            nodes: vec![],
            node_p2p_addresses: vec![],
        }
    }
}

/// Request body shared by the peer connect/disconnect endpoints.
#[derive(Deserialize)]
pub(crate) struct PeerConnectionRequest {
    pub node_id: u32,
    pub address: String,
    /// Bitcoin Core `getpeerinfo` peer `id` — preferred for `disconnectnode` (address matching is strict).
    #[serde(default)]
    pub peer_id: Option<u64>,
    /// Exact string(s) passed to `addnode add` for this peer (e.g. catalog P2P address). Required
    /// so `addnode remove` succeeds; otherwise Core reconnects automatically.
    #[serde(default)]
    pub addnode_remove_addresses: Vec<String>,
    /// Optional configured node id for the matched in-app peer on the other side of this
    /// connection. When present, the API also asks that node to remove its side of the
    /// relationship so a persistent remote `addnode` does not immediately reconnect.
    #[serde(default, alias = "symmetric_remote_node_id")]
    pub counterparty_node_id: Option<u32>,
    /// Best-known listen addresses for the local node being disconnected.
    ///
    /// Remote cleanup needs these exact address strings to remove its `addnode` entries that point
    /// back to us. We accept them from the UI because it may have a live-detected address even when
    /// the static config has no `p2p_port` and the server cannot reconstruct that address cheaply
    /// inside this write endpoint.
    #[serde(default, alias = "local_catalog_addnode_strings")]
    pub local_listen_address_candidates: Vec<String>,
}

#[derive(Serialize)]
pub(crate) struct PeerActionResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

type PeerActionApiResponse = (StatusCode, Json<PeerActionResponse>);

#[derive(Serialize)]
pub(crate) struct PeerChangedEvent {
    pub network_id: u32,
}

// -- Peer view assembly --

/// Stable identity for one configured node while we build peer topology views.
#[derive(Clone)]
struct ConfiguredNodeRef {
    id: u32,
    name: String,
}

impl ConfiguredNodeRef {
    fn new(id: u32, name: String) -> Self {
        Self { id, name }
    }
}

/// Observed peer list for one configured node.
///
/// This keeps the configured node identity alongside the raw peer data so later matching steps do
/// not need to re-query config state or pass parallel id/name tuples around.
struct ObservedNodePeers {
    node: ConfiguredNodeRef,
    peers: Vec<PeerInfo>,
}

/// Loads one node's peer list for response building without failing the whole network request.
///
/// The returned snapshot always preserves the configured node identity, even when peer discovery is
/// unsupported or fails. In those cases we degrade to an empty peer list so matching and rendering
/// can continue for the rest of the network.
async fn load_observed_node_peers(network_id: u32, node: Arc<dyn Node>) -> ObservedNodePeers {
    let configured_node = ConfiguredNodeRef::new(node.info().id, node.info().name.clone());
    let peers = match node.get_peer_info().await {
        Ok(peers) => peers,
        Err(FetchError::NotSupported { .. }) => vec![],
        Err(err) => {
            error!(
                "Could not fetch peer info from {} (endpoint={}) on network id={}: {}",
                node.info(),
                node.endpoint(),
                network_id,
                err,
            );
            vec![]
        }
    };

    ObservedNodePeers {
        node: configured_node,
        peers,
    }
}

/// Extracts a routable P2P listen address from inbound peer data.
///
/// For inbound peers, Bitcoin Core reports the local listening socket in `addrbind`
/// (for example `0.0.0.0:38333`). We keep the observed port and combine it with the RPC host to
/// reconstruct a routable `host:port` address that other nodes can use for matching and connect UI.
fn detect_routable_listen_address(rpc_endpoint: &str, peers: &[PeerInfo]) -> Option<String> {
    let port = peers
        .iter()
        .filter(|p| p.inbound && !p.addrbind.is_empty())
        .find_map(|p| {
            p.addrbind
                .rsplit_once(':')
                .and_then(|(_, port)| port.parse::<u16>().ok())
        })?;

    let host = rpc_endpoint
        .strip_prefix("http://")
        .or_else(|| rpc_endpoint.strip_prefix("https://"))
        .or_else(|| rpc_endpoint.strip_prefix("ssl://"))
        .unwrap_or(rpc_endpoint);
    let host = host.rsplit_once(':').map_or(host, |(host, _)| host);
    Some(format!("{}:{}", host, port))
}

/// Best-effort listen port for a `host:port` / `SocketAddr` style P2P address string.
fn listen_port_from_address(address: &str) -> Option<u16> {
    address
        .parse::<SocketAddr>()
        .ok()
        .map(|socket_address| socket_address.port())
        .or_else(|| {
            address
                .rsplit_once(':')
                .and_then(|(_, port)| port.parse().ok())
        })
}

/// Lookup tables used to match raw `getpeerinfo` addresses back to configured nodes.
///
/// Bitcoin Core exposes peers in several inconsistent forms:
/// exact hostnames, resolved socket strings, and ephemeral local bind addresses. This matcher
/// gathers all safe heuristics in one place so the response builder can ask one clear question:
/// "which configured node is this peer most likely connected to?"
struct PeerNodeMatcher {
    node_by_socket_addr: HashMap<SocketAddr, ConfiguredNodeRef>,
    node_by_exact_listen_addr: HashMap<String, ConfiguredNodeRef>,
    node_by_unique_listen_port: HashMap<u16, ConfiguredNodeRef>,
    node_by_outbound_bind_addr: HashMap<String, ConfiguredNodeRef>,
}

impl PeerNodeMatcher {
    /// Builds the address match tables from the network's listen addresses and live peer data.
    async fn build(
        node_listen_addresses: &[NodeListenAddress],
        observed_node_peers: &[ObservedNodePeers],
    ) -> Self {
        let mut node_by_socket_addr = HashMap::new();
        let mut node_by_exact_listen_addr = HashMap::new();
        let mut port_owners: HashMap<u16, Vec<ConfiguredNodeRef>> = HashMap::new();

        for listen_address in node_listen_addresses {
            let node = ConfiguredNodeRef::new(listen_address.node_id, listen_address.name.clone());
            node_by_exact_listen_addr.insert(listen_address.address.clone(), node.clone());

            if let Some(port) = listen_port_from_address(&listen_address.address) {
                port_owners.entry(port).or_default().push(node.clone());
            }

            match tokio::net::lookup_host(&listen_address.address).await {
                Ok(resolved_addresses) => {
                    for resolved_address in resolved_addresses {
                        node_by_socket_addr.insert(resolved_address, node.clone());
                        node_by_exact_listen_addr
                            .insert(resolved_address.to_string(), node.clone());
                        port_owners
                            .entry(resolved_address.port())
                            .or_default()
                            .push(node.clone());
                    }
                }
                Err(err) => {
                    warn!(
                        "Could not resolve P2P address '{}' for node '{}': {}",
                        listen_address.address, listen_address.name, err
                    );
                }
            }
        }

        let node_by_unique_listen_port = port_owners
            .into_iter()
            .filter_map(|(port, mut owners)| {
                owners.sort_by_key(|owner| owner.id);
                owners.dedup_by_key(|owner| owner.id);
                if owners.len() == 1 {
                    owners.pop().map(|owner| (port, owner))
                } else {
                    None
                }
            })
            .collect();

        let mut node_by_outbound_bind_addr = HashMap::new();
        for observed_node in observed_node_peers {
            for peer in &observed_node.peers {
                if !peer.inbound && !peer.addrbind.is_empty() {
                    node_by_outbound_bind_addr
                        .insert(peer.addrbind.clone(), observed_node.node.clone());
                }
            }
        }

        Self {
            node_by_socket_addr,
            node_by_exact_listen_addr,
            node_by_unique_listen_port,
            node_by_outbound_bind_addr,
        }
    }

    fn match_peer(&self, local_node_id: u32, peer: &PeerInfo) -> Option<ConfiguredNodeRef> {
        let matched_node = if peer.inbound {
            self.match_inbound_peer(peer)
        } else {
            self.match_outbound_peer(peer)
        };

        matched_node.filter(|node| node.id != local_node_id)
    }

    fn match_inbound_peer(&self, peer: &PeerInfo) -> Option<ConfiguredNodeRef> {
        self.match_by_socket_addr(&peer.addr)
            .or_else(|| self.node_by_outbound_bind_addr.get(&peer.addr).cloned())
    }

    fn match_outbound_peer(&self, peer: &PeerInfo) -> Option<ConfiguredNodeRef> {
        self.match_by_socket_addr(&peer.addr)
            .or_else(|| self.node_by_exact_listen_addr.get(&peer.addr).cloned())
            .or_else(|| {
                peer.addr
                    .parse::<SocketAddr>()
                    .ok()
                    .and_then(|socket_address| {
                        self.node_by_unique_listen_port
                            .get(&socket_address.port())
                            .cloned()
                    })
            })
    }

    fn match_by_socket_addr(&self, peer_addr: &str) -> Option<ConfiguredNodeRef> {
        peer_addr
            .parse::<SocketAddr>()
            .ok()
            .and_then(|socket_address| self.node_by_socket_addr.get(&socket_address).cloned())
    }
}

/// Collects the best-known listen address for each configured node in the network.
///
/// Live peer data is more accurate than config because it captures the actual listen port currently
/// in use, but some backends cannot report peer data. In those cases we fall back to the configured
/// `p2p_address`.
fn collect_node_listen_addresses(
    network: &Network,
    observed_node_peers: &[ObservedNodePeers],
) -> Vec<NodeListenAddress> {
    network
        .nodes
        .iter()
        .filter_map(|node| {
            let info = node.info();
            let observed_node = observed_node_peers
                .iter()
                .find(|observed_node| observed_node.node.id == info.id);
            let detected_listen_address = observed_node.and_then(|observed_node| {
                detect_routable_listen_address(node.endpoint(), &observed_node.peers)
            });
            let address = detected_listen_address.or_else(|| info.p2p_address.clone())?;

            Some(NodeListenAddress {
                node_id: info.id,
                name: info.name.clone(),
                address,
            })
        })
        .collect()
}

/// Converts one node's raw peer observations into the JSON payload returned to the UI.
fn build_node_peer_connections(
    observed_node: ObservedNodePeers,
    matcher: &PeerNodeMatcher,
) -> NodePeerConnections {
    let local_node_id = observed_node.node.id;
    let peers = observed_node
        .peers
        .into_iter()
        .map(|peer| {
            let matched_node = matcher.match_peer(local_node_id, &peer);
            MatchedPeerInfo::from_peer_and_match(peer, matched_node)
        })
        .collect();

    NodePeerConnections {
        node_id: local_node_id,
        peers,
    }
}

/// Returns the current peer topology for all configured nodes in one network.
///
/// The response includes each node's peer list plus the best-known listen address for every node so
/// the UI can render relationships and issue connect/disconnect actions without another round trip.
pub async fn peer_info_response(
    Path(network_id): Path<u32>,
    State(state): State<AppState>,
) -> (StatusCode, Json<PeerConnectionsResponse>) {
    let network = match get_network(&state, network_id) {
        Some(network) => network,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(PeerConnectionsResponse::empty()),
            );
        }
    };

    let observed_node_peers = join_all(
        network
            .nodes
            .iter()
            .cloned()
            .map(|node| load_observed_node_peers(network_id, node)),
    )
    .await;

    let node_p2p_addresses = collect_node_listen_addresses(network, &observed_node_peers);
    let matcher = PeerNodeMatcher::build(&node_p2p_addresses, &observed_node_peers).await;

    let nodes = observed_node_peers
        .into_iter()
        .map(|observed_node| build_node_peer_connections(observed_node, &matcher))
        .collect();

    (
        StatusCode::OK,
        Json(PeerConnectionsResponse {
            nodes,
            node_p2p_addresses,
        }),
    )
}

fn publish_peer_change_event(state: &AppState, network_id: u32) {
    let _ = state.peer_changed_tx.send(network_id);
}

fn peer_changed_event(network_id: u32) -> Event {
    Event::default()
        .event("peer_changed")
        .json_data(PeerChangedEvent { network_id })
        .unwrap_or_default()
}

fn lagged_peer_changes_event(dropped_messages: u64) -> Event {
    Event::default()
        .event("resync_required")
        .json_data(ResyncRequired {
            reason: "lagged".to_string(),
            dropped_messages,
        })
        .unwrap_or_default()
}

/// Streams peer-topology invalidation events so clients know when to refetch `peer-info.json`.
pub async fn peer_changes_sse(
    axum::extract::Query(query): axum::extract::Query<CacheChangesQuery>,
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state.peer_changed_tx.subscribe();
    let filter_network_id = query.network_id;

    let stream = BroadcastStream::new(rx).filter_map(move |result| {
        let maybe_event = match result {
            Ok(network_id) => {
                if filter_network_id.is_some_and(|selected_id| selected_id != network_id) {
                    None
                } else {
                    Some(peer_changed_event(network_id))
                }
            }
            Err(BroadcastStreamRecvError::Lagged(dropped_messages)) => {
                error!(
                    "SSE subscriber lagged, dropped {} peer_changed events.",
                    dropped_messages
                );
                Some(lagged_peer_changes_event(dropped_messages))
            }
        };

        ready(maybe_event.map(Ok::<_, Infallible>))
    });

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(10))
            .text("keep-alive"),
    )
}

// -- Peer mutation handlers --

struct ResolvedPeerActionTarget<'a> {
    network: &'a Network,
    node: &'a dyn Node,
}

/// Planned cleanup that must run on the counterparty node after a local disconnect succeeds.
struct CounterpartyDisconnectPlan<'a> {
    remote_node_id: u32,
    remote_node: &'a dyn Node,
    local_listen_address_candidates: Vec<String>,
}

fn ok_peer_action_response() -> PeerActionApiResponse {
    (
        StatusCode::OK,
        Json(PeerActionResponse {
            success: true,
            error: None,
        }),
    )
}

fn peer_action_failure_response(status: StatusCode, code: &str) -> PeerActionApiResponse {
    (
        status,
        Json(PeerActionResponse {
            success: false,
            error: Some(code.to_string()),
        }),
    )
}

/// Resolves the network and node targeted by a mutating peer-control request.
fn resolve_peer_action_target(
    state: &AppState,
    network_id: u32,
    node_id: u32,
) -> Result<ResolvedPeerActionTarget<'_>, PeerActionApiResponse> {
    let network = get_network(state, network_id).ok_or_else(|| {
        peer_action_failure_response(StatusCode::NOT_FOUND, "PEER_NETWORK_NOT_FOUND")
    })?;

    if network.view_only_mode {
        return Err(peer_action_failure_response(
            StatusCode::BAD_REQUEST,
            "PEER_FEATURE_DISABLED",
        ));
    }

    let node = get_node(network, node_id).ok_or_else(|| {
        peer_action_failure_response(StatusCode::BAD_REQUEST, "PEER_NODE_NOT_FOUND")
    })?;

    Ok(ResolvedPeerActionTarget { network, node })
}

fn peer_action_failure_from_fetch_error(
    err: &FetchError,
    unsupported_code: &str,
    invalid_request_code: &str,
    execution_failed_code: &str,
) -> PeerActionApiResponse {
    let (status, error_code) = match err {
        FetchError::NotSupported { .. } | FetchError::DataError(_) => (
            StatusCode::BAD_REQUEST,
            match err {
                FetchError::NotSupported { .. } => unsupported_code.to_string(),
                FetchError::DataError(_) => invalid_request_code.to_string(),
                _ => execution_failed_code.to_string(),
            },
        ),
        _ => (
            StatusCode::INTERNAL_SERVER_ERROR,
            execution_failed_code.to_string(),
        ),
    };
    peer_action_failure_response(status, &error_code)
}

fn log_and_map_local_peer_action_error(
    operation: &str,
    network_id: u32,
    node_id: u32,
    address: &str,
    err: &FetchError,
) -> PeerActionApiResponse {
    error!(
        "{} failed for network={} node={} addr={}: {}",
        operation, network_id, node_id, address, err
    );
    peer_action_failure_from_fetch_error(
        err,
        "PEER_BACKEND_UNSUPPORTED",
        "PEER_INVALID_REQUEST",
        "PEER_EXECUTION_FAILED",
    )
}

fn collect_local_listen_address_candidates(
    network: &Network,
    request: &PeerConnectionRequest,
) -> Vec<String> {
    let mut local_listen_address_candidates: Vec<String> = request
        .local_listen_address_candidates
        .iter()
        .map(|address| address.trim().to_string())
        .filter(|address| !address.is_empty())
        .collect();

    if local_listen_address_candidates.is_empty()
        && let Some(local_node) = get_node(network, request.node_id)
        && let Some(address) = &local_node.info().p2p_address
    {
        local_listen_address_candidates.push(address.clone());
    }

    local_listen_address_candidates
}

/// Validates and prepares the optional counterparty cleanup before the local disconnect mutates anything.
fn plan_counterparty_disconnect<'a>(
    network: &'a Network,
    request: &PeerConnectionRequest,
) -> Result<Option<CounterpartyDisconnectPlan<'a>>, PeerActionApiResponse> {
    let Some(remote_node_id) = request.counterparty_node_id else {
        return Ok(None);
    };

    let local_listen_address_candidates = collect_local_listen_address_candidates(network, request);
    if local_listen_address_candidates.is_empty() {
        return Err(peer_action_failure_response(
            StatusCode::BAD_REQUEST,
            "PEER_LOCAL_CATALOG_ADDRESS_REQUIRED",
        ));
    }

    let remote_node = get_node(network, remote_node_id).ok_or_else(|| {
        peer_action_failure_response(StatusCode::BAD_REQUEST, "PEER_REMOTE_NODE_NOT_FOUND")
    })?;

    Ok(Some(CounterpartyDisconnectPlan {
        remote_node_id,
        remote_node,
        local_listen_address_candidates,
    }))
}

/// Runs the planned counterparty cleanup and maps backend failures into the HTTP API contract.
async fn execute_counterparty_disconnect(
    network_id: u32,
    cleanup: CounterpartyDisconnectPlan<'_>,
) -> Result<(), PeerActionApiResponse> {
    if let Err(err) = cleanup
        .remote_node
        .remove_counterparty_peer_connection(&cleanup.local_listen_address_candidates)
        .await
    {
        error!(
            "remove_counterparty_peer_connection failed for network={} remote_node={} local_listen_addresses={:?}: {}",
            network_id, cleanup.remote_node_id, cleanup.local_listen_address_candidates, err
        );
        return Err(peer_action_failure_from_fetch_error(
            &err,
            "PEER_REMOTE_BACKEND_UNSUPPORTED",
            "PEER_REMOTE_INVALID_REQUEST",
            "PEER_REMOTE_EXECUTION_FAILED",
        ));
    }

    Ok(())
}

/// Connects one configured node to the requested peer address.
pub async fn add_node(
    Path(network_id): Path<u32>,
    State(state): State<AppState>,
    Json(body): Json<PeerConnectionRequest>,
) -> PeerActionApiResponse {
    let target = match resolve_peer_action_target(&state, network_id, body.node_id) {
        Ok(target) => target,
        Err(resp) => return resp,
    };

    match target.node.add_peer(&body.address).await {
        Ok(_) => {
            publish_peer_change_event(&state, network_id);
            ok_peer_action_response()
        }
        Err(err) => log_and_map_local_peer_action_error(
            "add_peer",
            network_id,
            body.node_id,
            &body.address,
            &err,
        ),
    }
}

/// Disconnects a peer from one node and optionally clears the counterparty side of the link.
///
/// The optional counterparty cleanup prevents reconnect loops when the remote node still has a
/// persistent `addnode` entry pointing back at the local node.
pub async fn disconnect_node(
    Path(network_id): Path<u32>,
    State(state): State<AppState>,
    Json(body): Json<PeerConnectionRequest>,
) -> PeerActionApiResponse {
    let target = match resolve_peer_action_target(&state, network_id, body.node_id) {
        Ok(target) => target,
        Err(resp) => return resp,
    };
    let counterparty_disconnect = match plan_counterparty_disconnect(target.network, &body) {
        Ok(plan) => plan,
        Err(resp) => return resp,
    };

    match target
        .node
        .remove_peer_connection(&body.address, body.peer_id, &body.addnode_remove_addresses)
        .await
    {
        Ok(()) => {
            if let Some(cleanup) = counterparty_disconnect
                && let Err(resp) = execute_counterparty_disconnect(network_id, cleanup).await
            {
                publish_peer_change_event(&state, network_id);
                return resp;
            }
            publish_peer_change_event(&state, network_id);
            ok_peer_action_response()
        }
        Err(err) => log_and_map_local_peer_action_error(
            "remove_peer_connection",
            network_id,
            body.node_id,
            &body.address,
            &err,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{NetworkType, StaleRateRange};
    use crate::node::{HeaderLocator, NodeInfo};
    use crate::types::{Caches, ChainTip, HeaderInfo, Tree};
    use async_trait::async_trait;
    use bitcoincore_rpc::bitcoin;
    use bitcoincore_rpc::bitcoin::BlockHash;
    use bitcoincore_rpc::bitcoin::blockdata::block::Header;
    use std::collections::BTreeMap;
    use tokio::sync::Mutex;
    use tokio::sync::broadcast::error::TryRecvError;
    use tokio::sync::mpsc::UnboundedSender;

    #[derive(Clone, Copy)]
    enum PeerMutationBehavior {
        Ok,
        NotSupported,
        DataError,
        ExecutionError,
    }

    #[derive(Clone)]
    struct MockNode {
        info: NodeInfo,
        disconnect_behavior: PeerMutationBehavior,
        unlink_behavior: PeerMutationBehavior,
        disconnect_calls: Arc<Mutex<Vec<(String, Option<u64>, Vec<String>)>>>,
        unlink_calls: Arc<Mutex<Vec<Vec<String>>>>,
    }

    impl MockNode {
        fn new(node_id: u32) -> Self {
            Self {
                info: NodeInfo {
                    id: node_id,
                    name: format!("mock-{node_id}"),
                    description: "mock node".to_string(),
                    implementation: "Bitcoin Core".to_string(),
                    network_type: bitcoin::Network::Regtest,
                    supports_mining: true,
                    signet_challenge: None,
                    signet_nbits: None,
                    p2p_address: None,
                },
                disconnect_behavior: PeerMutationBehavior::Ok,
                unlink_behavior: PeerMutationBehavior::Ok,
                disconnect_calls: Arc::new(Mutex::new(Vec::new())),
                unlink_calls: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn with_disconnect_behavior(mut self, behavior: PeerMutationBehavior) -> Self {
            self.disconnect_behavior = behavior;
            self
        }

        fn with_unlink_behavior(mut self, behavior: PeerMutationBehavior) -> Self {
            self.unlink_behavior = behavior;
            self
        }

        fn with_p2p_address(mut self, address: &str) -> Self {
            self.info.p2p_address = Some(address.to_string());
            self
        }
    }

    fn peer_mutation_result(
        behavior: PeerMutationBehavior,
        operation: &'static str,
    ) -> Result<(), FetchError> {
        match behavior {
            PeerMutationBehavior::Ok => Ok(()),
            PeerMutationBehavior::NotSupported => Err(FetchError::NotSupported {
                node: "mock".to_string(),
                operation,
            }),
            PeerMutationBehavior::DataError => Err(FetchError::DataError("bad input".to_string())),
            PeerMutationBehavior::ExecutionError => {
                Err(FetchError::BitcoinCoreREST("mock failure".to_string()))
            }
        }
    }

    #[async_trait]
    impl Node for MockNode {
        fn info(&self) -> &NodeInfo {
            &self.info
        }

        fn endpoint(&self) -> &str {
            "mock://node"
        }

        async fn version(&self) -> Result<String, FetchError> {
            Ok("mock".to_string())
        }

        async fn block_header(&self, _locator: HeaderLocator) -> Result<Header, FetchError> {
            Err(FetchError::DataError(
                "unused in peer API tests".to_string(),
            ))
        }

        async fn tips(&self) -> Result<Vec<ChainTip>, FetchError> {
            Ok(vec![])
        }

        async fn get_miner_pool(
            &self,
            _hash: &BlockHash,
            _height: u64,
            _network: bitcoin::Network,
        ) -> Result<Option<String>, FetchError> {
            Ok(None)
        }

        async fn get_new_headers(
            &self,
            _tips: &[ChainTip],
            _tree: &Tree,
            _first_tracked_height: u64,
            _progress_tx: Option<&UnboundedSender<Vec<HeaderInfo>>>,
        ) -> Result<(Vec<HeaderInfo>, Vec<BlockHash>), FetchError> {
            Ok((vec![], vec![]))
        }

        async fn remove_peer_connection(
            &self,
            addr: &str,
            peer_id: Option<u64>,
            addnode_remove_candidates: &[String],
        ) -> Result<(), FetchError> {
            self.disconnect_calls.lock().await.push((
                addr.to_string(),
                peer_id,
                addnode_remove_candidates.to_vec(),
            ));
            peer_mutation_result(self.disconnect_behavior, "remove_peer_connection")
        }

        async fn remove_counterparty_peer_connection(
            &self,
            counterparty_listen_address_candidates: &[String],
        ) -> Result<(), FetchError> {
            self.unlink_calls
                .lock()
                .await
                .push(counterparty_listen_address_candidates.to_vec());
            peer_mutation_result(self.unlink_behavior, "remove_counterparty_peer_connection")
        }
    }

    fn test_state(networks: Vec<Network>) -> AppState {
        let (cache_changed_tx, _) = tokio::sync::broadcast::channel(4);
        let (peer_changed_tx, _) = tokio::sync::broadcast::channel(4);
        let caches: Caches = Arc::new(Mutex::new(BTreeMap::new()));
        AppState {
            caches,
            networks,
            network_infos: vec![],
            rss_base_url: String::new(),
            cache_changed_tx,
            peer_changed_tx,
        }
    }

    fn network_with_nodes(network_id: u32, nodes: Vec<MockNode>) -> Vec<Network> {
        vec![Network {
            id: network_id,
            description: "test network".to_string(),
            name: "test".to_string(),
            query_interval: Duration::from_secs(15),
            first_tracked_height: 0,
            visible_heights_from_tip: 0,
            extra_hotspot_heights: 0,
            network_type: NetworkType::Regtest,
            view_only_mode: false,
            stale_rate_ranges: vec![StaleRateRange::Rolling(100)],
            nodes: nodes
                .into_iter()
                .map(|node| Arc::new(node) as Arc<dyn Node>)
                .collect(),
        }]
    }

    #[tokio::test]
    async fn disconnect_node_returns_error_when_remote_unlink_fails() {
        let local_node = MockNode::new(0).with_p2p_address("127.0.0.1:18444");
        let remote_node =
            MockNode::new(1).with_unlink_behavior(PeerMutationBehavior::ExecutionError);
        let state = test_state(network_with_nodes(
            7,
            vec![local_node.clone(), remote_node.clone()],
        ));
        let mut peer_changes_rx = state.peer_changed_tx.subscribe();

        let (status, Json(response)) = disconnect_node(
            Path(7),
            State(state),
            Json(PeerConnectionRequest {
                node_id: 0,
                address: "127.0.0.1:18454".to_string(),
                peer_id: Some(42),
                addnode_remove_addresses: vec!["127.0.0.1:18454".to_string()],
                counterparty_node_id: Some(1),
                local_listen_address_candidates: vec![],
            }),
        )
        .await;

        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert!(!response.success);
        assert_eq!(
            response.error.as_deref(),
            Some("PEER_REMOTE_EXECUTION_FAILED")
        );
        assert_eq!(peer_changes_rx.recv().await.unwrap(), 7);
        assert_eq!(local_node.disconnect_calls.lock().await.len(), 1);
        assert_eq!(remote_node.unlink_calls.lock().await.len(), 1);
    }

    #[tokio::test]
    async fn disconnect_node_rejects_missing_local_catalog_address_before_disconnect() {
        let local_node =
            MockNode::new(0).with_disconnect_behavior(PeerMutationBehavior::ExecutionError);
        let remote_node = MockNode::new(1).with_unlink_behavior(PeerMutationBehavior::DataError);
        let state = test_state(network_with_nodes(
            9,
            vec![local_node.clone(), remote_node.clone()],
        ));
        let mut peer_changes_rx = state.peer_changed_tx.subscribe();

        let (status, Json(response)) = disconnect_node(
            Path(9),
            State(state),
            Json(PeerConnectionRequest {
                node_id: 0,
                address: "127.0.0.1:18454".to_string(),
                peer_id: Some(5),
                addnode_remove_addresses: vec!["127.0.0.1:18454".to_string()],
                counterparty_node_id: Some(1),
                local_listen_address_candidates: vec![],
            }),
        )
        .await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(!response.success);
        assert_eq!(
            response.error.as_deref(),
            Some("PEER_LOCAL_CATALOG_ADDRESS_REQUIRED")
        );
        assert_eq!(local_node.disconnect_calls.lock().await.len(), 0);
        assert_eq!(remote_node.unlink_calls.lock().await.len(), 0);
        assert!(matches!(
            peer_changes_rx.try_recv(),
            Err(TryRecvError::Empty | TryRecvError::Closed)
        ));
    }

    #[tokio::test]
    async fn disconnect_node_rejects_unknown_remote_node_before_disconnect() {
        let local_node = MockNode::new(0).with_p2p_address("127.0.0.1:18444");
        let state = test_state(network_with_nodes(11, vec![local_node.clone()]));
        let mut peer_changes_rx = state.peer_changed_tx.subscribe();

        let (status, Json(response)) = disconnect_node(
            Path(11),
            State(state),
            Json(PeerConnectionRequest {
                node_id: 0,
                address: "127.0.0.1:18454".to_string(),
                peer_id: Some(8),
                addnode_remove_addresses: vec!["127.0.0.1:18454".to_string()],
                counterparty_node_id: Some(99),
                local_listen_address_candidates: vec![],
            }),
        )
        .await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(!response.success);
        assert_eq!(
            response.error.as_deref(),
            Some("PEER_REMOTE_NODE_NOT_FOUND")
        );
        assert_eq!(local_node.disconnect_calls.lock().await.len(), 0);
        assert!(matches!(
            peer_changes_rx.try_recv(),
            Err(TryRecvError::Empty | TryRecvError::Closed)
        ));
    }

    #[tokio::test]
    async fn disconnect_node_maps_remote_not_supported_to_bad_request() {
        let local_node = MockNode::new(0).with_p2p_address("127.0.0.1:18444");
        let remote_node = MockNode::new(1).with_unlink_behavior(PeerMutationBehavior::NotSupported);
        let state = test_state(network_with_nodes(
            13,
            vec![local_node.clone(), remote_node.clone()],
        ));
        let mut peer_changes_rx = state.peer_changed_tx.subscribe();

        let (status, Json(response)) = disconnect_node(
            Path(13),
            State(state),
            Json(PeerConnectionRequest {
                node_id: 0,
                address: "127.0.0.1:18454".to_string(),
                peer_id: Some(21),
                addnode_remove_addresses: vec!["127.0.0.1:18454".to_string()],
                counterparty_node_id: Some(1),
                local_listen_address_candidates: vec![],
            }),
        )
        .await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(!response.success);
        assert_eq!(
            response.error.as_deref(),
            Some("PEER_REMOTE_BACKEND_UNSUPPORTED")
        );
        assert_eq!(peer_changes_rx.recv().await.unwrap(), 13);
        assert_eq!(local_node.disconnect_calls.lock().await.len(), 1);
        assert_eq!(remote_node.unlink_calls.lock().await.len(), 1);
    }
}
