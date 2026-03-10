use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
};
use futures_util::StreamExt;
use futures_util::future::{join_all, ready};
use futures_util::stream::Stream;
use log::error;
use serde::{Deserialize, Serialize};
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;

use crate::config::Network;
use crate::error::FetchError;
use crate::node::Node;
use crate::types::{AppState, DataChanged, DataJsonResponse, NetworksJsonResponse};

fn get_network(state: &AppState, network_id: u32) -> Option<&Network> {
    state
        .networks
        .iter()
        .find(|network| network.id == network_id)
}

fn get_node(network: &Network, node_id: u32) -> Option<&dyn Node> {
    network
        .nodes
        .iter()
        .find(|node| node.info().id == node_id)
        .map(|node| node.as_ref())
}

pub async fn data_response(
    Path(network): Path<u32>,
    State(state): State<AppState>,
) -> Json<DataJsonResponse> {
    let caches_locked = state.caches.lock().await;
    match caches_locked.get(&network) {
        Some(cache) => Json(DataJsonResponse {
            header_infos: cache.header_infos_json.clone(),
            nodes: cache.node_data.values().cloned().collect(),
        }),
        None => Json(DataJsonResponse {
            header_infos: vec![],
            nodes: vec![],
        }),
    }
}

pub async fn networks_response(State(state): State<AppState>) -> Json<NetworksJsonResponse> {
    Json(NetworksJsonResponse {
        networks: state.network_infos.clone(),
    })
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub(crate) struct NodeP2PState {
    node_id: u32,
    active: Option<bool>,
}

#[derive(Serialize)]
pub(crate) struct NodeP2PStateResponse {
    nodes: Vec<NodeP2PState>,
}

async fn load_node_p2p_state(network_id: u32, node: Arc<dyn Node>) -> NodeP2PState {
    let active = match node.p2p_network_active().await {
        Ok(active) => Some(active),
        Err(FetchError::NotSupported { .. }) => None,
        Err(error) => {
            error!(
                "Could not fetch current p2p state from {} (endpoint={}) on network id={}: {}",
                node.info(),
                node.endpoint(),
                network_id,
                error
            );
            None
        }
    };

    NodeP2PState {
        node_id: node.info().id,
        active,
    }
}

pub async fn p2p_state_response(
    Path(network_id): Path<u32>,
    State(state): State<AppState>,
) -> (StatusCode, Json<NodeP2PStateResponse>) {
    let network = match get_network(&state, network_id) {
        Some(network) => network,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(NodeP2PStateResponse { nodes: vec![] }),
            );
        }
    };

    let nodes = join_all(
        network
            .nodes
            .iter()
            .cloned()
            .map(|node| load_node_p2p_state(network_id, node)),
    )
    .await;

    (StatusCode::OK, Json(NodeP2PStateResponse { nodes }))
}

#[derive(Deserialize)]
pub struct CacheChangesQuery {
    pub network_id: Option<u32>,
}

#[derive(Serialize)]
pub struct ResyncRequired {
    pub reason: String,
    pub dropped_messages: u64,
}

pub async fn cache_changes_sse(
    Query(query): Query<CacheChangesQuery>,
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state.cache_changed_tx.subscribe();
    let filter_network_id = query.network_id;

    let stream = BroadcastStream::new(rx).filter_map(move |result| {
        let maybe_event = match result {
            Ok(network_id) => {
                if filter_network_id.is_some_and(|selected_id| selected_id != network_id) {
                    None
                } else {
                    Some(
                        Event::default()
                            .event("cache_changed")
                            .json_data(DataChanged { network_id })
                            .unwrap_or_default(),
                    )
                }
            }
            Err(BroadcastStreamRecvError::Lagged(dropped_messages)) => {
                error!(
                    "SSE subscriber lagged, dropped {} cache_changed events.",
                    dropped_messages
                );
                Some(
                    Event::default()
                        .event("resync_required")
                        .json_data(ResyncRequired {
                            reason: "lagged".to_string(),
                            dropped_messages,
                        })
                        .unwrap_or_default(),
                )
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

// -- Mine block --

#[derive(Deserialize)]
pub struct MineBlockRequest {
    pub node_id: u32,
    pub count: Option<u64>,
}

#[derive(Serialize)]
pub struct MineBlockResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub async fn mine_block(
    Path(network_id): Path<u32>,
    State(state): State<AppState>,
    Json(body): Json<MineBlockRequest>,
) -> (StatusCode, Json<MineBlockResponse>) {
    let network = match get_network(&state, network_id) {
        Some(network) => network,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(MineBlockResponse {
                    success: false,
                    error: Some("MINE_NETWORK_NOT_FOUND".to_string()),
                }),
            );
        }
    };
    if network.disable_node_controls {
        return (
            StatusCode::BAD_REQUEST,
            Json(MineBlockResponse {
                success: false,
                error: Some("MINE_FEATURE_DISABLED".to_string()),
            }),
        );
    }

    let node = match get_node(network, body.node_id) {
        Some(node) => node,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(MineBlockResponse {
                    success: false,
                    error: Some("MINE_BACKEND_UNSUPPORTED".to_string()),
                }),
            );
        }
    };
    if !node.supports_mining(network.disable_node_controls) {
        return (
            StatusCode::BAD_REQUEST,
            Json(MineBlockResponse {
                success: false,
                error: Some("MINE_NODE_NOT_A_MINER".to_string()),
            }),
        );
    }

    let count = body.count.unwrap_or(1);
    match node.mine_new_blocks(count).await {
        Ok(_) => (
            StatusCode::OK,
            Json(MineBlockResponse {
                success: true,
                error: None,
            }),
        ),
        Err(e @ FetchError::NotSupported { .. }) | Err(e @ FetchError::DataError(_)) => {
            error!(
                "Mine block failed for network={} node={}: {}",
                network_id, body.node_id, e
            );
            let error_code = map_control_error_code(
                &e,
                "MINE_BACKEND_UNSUPPORTED",
                "MINE_INVALID_REQUEST",
                "MINE_EXECUTION_FAILED",
            );
            (
                StatusCode::BAD_REQUEST,
                Json(MineBlockResponse {
                    success: false,
                    error: Some(error_code),
                }),
            )
        }
        Err(e) => {
            error!(
                "Mine block failed for network={} node={}: {}",
                network_id, body.node_id, e
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(MineBlockResponse {
                    success: false,
                    error: Some("MINE_EXECUTION_FAILED".to_string()),
                }),
            )
        }
    }
}

#[derive(Deserialize)]
pub struct SetNetworkActiveRequest {
    pub node_id: u32,
    pub active: bool,
}

#[derive(Serialize)]
pub struct SetNetworkActiveResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub async fn set_network_active(
    Path(network_id): Path<u32>,
    State(state): State<AppState>,
    Json(body): Json<SetNetworkActiveRequest>,
) -> (StatusCode, Json<SetNetworkActiveResponse>) {
    let network = match get_network(&state, network_id) {
        Some(network) => network,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(SetNetworkActiveResponse {
                    success: false,
                    error: Some("NETWORK_CONTROL_NETWORK_NOT_FOUND".to_string()),
                }),
            );
        }
    };
    if network.disable_node_controls {
        return (
            StatusCode::BAD_REQUEST,
            Json(SetNetworkActiveResponse {
                success: false,
                error: Some("NETWORK_CONTROL_FEATURE_DISABLED".to_string()),
            }),
        );
    }

    let node = match get_node(network, body.node_id) {
        Some(node) => node,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(SetNetworkActiveResponse {
                    success: false,
                    error: Some("NETWORK_CONTROL_BACKEND_UNSUPPORTED".to_string()),
                }),
            );
        }
    };

    match node.set_p2p_network_active(body.active).await {
        Ok(_) => (
            StatusCode::OK,
            Json(SetNetworkActiveResponse {
                success: true,
                error: None,
            }),
        ),
        Err(e @ FetchError::NotSupported { .. }) | Err(e @ FetchError::DataError(_)) => {
            error!(
                "set_network_active failed for network={} node={} active={}: {}",
                network_id, body.node_id, body.active, e
            );
            let error_code = map_control_error_code(
                &e,
                "NETWORK_CONTROL_BACKEND_UNSUPPORTED",
                "NETWORK_CONTROL_INVALID_REQUEST",
                "NETWORK_CONTROL_EXECUTION_FAILED",
            );
            (
                StatusCode::BAD_REQUEST,
                Json(SetNetworkActiveResponse {
                    success: false,
                    error: Some(error_code),
                }),
            )
        }
        Err(e) => {
            error!(
                "set_network_active failed for network={} node={} active={}: {}",
                network_id, body.node_id, body.active, e
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(SetNetworkActiveResponse {
                    success: false,
                    error: Some("NETWORK_CONTROL_EXECUTION_FAILED".to_string()),
                }),
            )
        }
    }
}

fn map_control_error_code(
    err: &FetchError,
    unsupported_code: &str,
    invalid_request_code: &str,
    execution_failed_code: &str,
) -> String {
    match err {
        FetchError::NotSupported { .. } => unsupported_code.to_string(),
        FetchError::DataError(_) => invalid_request_code.to_string(),
        _ => execution_failed_code.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Network, NetworkType};
    use crate::node::{HeaderLocator, Node, NodeInfo};
    use crate::types::{Caches, ChainTip, HeaderInfo, Tree};
    use async_trait::async_trait;
    use bitcoincore_rpc::bitcoin;
    use bitcoincore_rpc::bitcoin::BlockHash;
    use bitcoincore_rpc::bitcoin::blockdata::block::Header;
    use bitcoincore_rpc::bitcoin::hashes::Hash;
    use std::collections::BTreeMap;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::sync::Mutex;
    use tokio::sync::mpsc::UnboundedSender;

    #[derive(Clone, Copy)]
    enum ControlBehavior {
        Ok,
        NotSupported,
        DataError,
        ExecutionError,
    }

    #[derive(Clone, Copy)]
    enum P2PReadBehavior {
        Available,
        Unsupported,
        Error,
    }

    #[derive(Clone)]
    struct MockNode {
        info: NodeInfo,
        mine_behavior: ControlBehavior,
        network_behavior: ControlBehavior,
        p2p_read_behavior: P2PReadBehavior,
        p2p_state: Arc<Mutex<bool>>,
        mine_calls: Arc<Mutex<Vec<u64>>>,
        network_calls: Arc<Mutex<Vec<bool>>>,
    }

    impl MockNode {
        fn new(
            node_id: u32,
            mine_behavior: ControlBehavior,
            network_behavior: ControlBehavior,
        ) -> Self {
            Self {
                info: NodeInfo {
                    id: node_id,
                    name: format!("mock-{}", node_id),
                    description: "mock node".to_string(),
                    implementation: "Bitcoin Core".to_string(),
                    network_type: bitcoin::Network::Regtest,
                    supports_mining: true,
                },
                mine_behavior,
                network_behavior,
                p2p_read_behavior: P2PReadBehavior::Available,
                p2p_state: Arc::new(Mutex::new(true)),
                mine_calls: Arc::new(Mutex::new(Vec::new())),
                network_calls: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn with_supports_mining(mut self, supports_mining: bool) -> Self {
            self.info.supports_mining = supports_mining;
            self
        }

        fn with_p2p_state(mut self, active: bool) -> Self {
            self.p2p_state = Arc::new(Mutex::new(active));
            self
        }

        fn with_p2p_read_behavior(mut self, p2p_read_behavior: P2PReadBehavior) -> Self {
            self.p2p_read_behavior = p2p_read_behavior;
            self
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
            Err(FetchError::DataError("unused in API tests".to_string()))
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

        async fn p2p_network_active(&self) -> Result<bool, FetchError> {
            match self.p2p_read_behavior {
                P2PReadBehavior::Available => Ok(*self.p2p_state.lock().await),
                P2PReadBehavior::Unsupported => Err(FetchError::NotSupported {
                    node: "mock".to_string(),
                    operation: "p2p_network_active",
                }),
                P2PReadBehavior::Error => Err(FetchError::BitcoinCoreREST(
                    "mock p2p read failure".to_string(),
                )),
            }
        }

        async fn mine_new_blocks(&self, count: u64) -> Result<Vec<BlockHash>, FetchError> {
            self.mine_calls.lock().await.push(count);
            match self.mine_behavior {
                ControlBehavior::Ok => Ok(vec![BlockHash::all_zeros()]),
                ControlBehavior::NotSupported => Err(FetchError::NotSupported {
                    node: "mock".to_string(),
                    operation: "mine_new_blocks",
                }),
                ControlBehavior::DataError => Err(FetchError::DataError("bad input".to_string())),
                ControlBehavior::ExecutionError => {
                    Err(FetchError::BitcoinCoreREST("mock failure".to_string()))
                }
            }
        }

        async fn set_p2p_network_active(&self, active: bool) -> Result<(), FetchError> {
            self.network_calls.lock().await.push(active);
            match self.network_behavior {
                ControlBehavior::Ok => {
                    *self.p2p_state.lock().await = active;
                    Ok(())
                }
                ControlBehavior::NotSupported => Err(FetchError::NotSupported {
                    node: "mock".to_string(),
                    operation: "set_p2p_network_active",
                }),
                ControlBehavior::DataError => Err(FetchError::DataError("bad input".to_string())),
                ControlBehavior::ExecutionError => {
                    Err(FetchError::BitcoinCoreREST("mock failure".to_string()))
                }
            }
        }
    }

    fn test_state(networks: Vec<Network>) -> AppState {
        let (cache_changed_tx, _) = tokio::sync::broadcast::channel(4);
        let caches: Caches = Arc::new(Mutex::new(BTreeMap::new()));
        AppState {
            caches,
            networks,
            network_infos: vec![],
            rss_base_url: String::new(),
            cache_changed_tx,
        }
    }

    fn single_node_network(network_id: u32, node: MockNode) -> Vec<Network> {
        vec![Network {
            id: network_id,
            description: "test network".to_string(),
            name: "test".to_string(),
            query_interval: Duration::from_secs(15),
            first_tracked_height: 0,
            visible_heights_from_tip: 0,
            extra_hotspot_heights: 0,
            network_type: NetworkType::Regtest,
            disable_node_controls: false,
            nodes: vec![Arc::new(node) as Arc<dyn Node>],
        }]
    }

    fn network_with_nodes(
        network_id: u32,
        disable_node_controls: bool,
        nodes: Vec<MockNode>,
    ) -> Vec<Network> {
        vec![Network {
            id: network_id,
            description: "test network".to_string(),
            name: "test".to_string(),
            query_interval: Duration::from_secs(15),
            first_tracked_height: 0,
            visible_heights_from_tip: 0,
            extra_hotspot_heights: 0,
            network_type: NetworkType::Regtest,
            disable_node_controls,
            nodes: nodes
                .into_iter()
                .map(|node| Arc::new(node) as Arc<dyn Node>)
                .collect(),
        }]
    }

    async fn p2p_state_for_network(state: &AppState, network_id: u32) -> NodeP2PStateResponse {
        let (status, Json(body)) = p2p_state_response(Path(network_id), State(state.clone())).await;
        assert_eq!(status, StatusCode::OK);
        body
    }

    fn node_p2p_state(response: &NodeP2PStateResponse, node_id: u32) -> Option<bool> {
        response
            .nodes
            .iter()
            .find(|node| node.node_id == node_id)
            .expect("node should exist in p2p response")
            .active
    }

    #[tokio::test]
    async fn mine_block_defaults_to_count_one() {
        let node = MockNode::new(7, ControlBehavior::Ok, ControlBehavior::Ok);
        let state = test_state(single_node_network(1, node.clone()));

        let (status, body) = mine_block(
            Path(1),
            State(state),
            Json(MineBlockRequest {
                node_id: 7,
                count: None,
            }),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert!(body.0.success);
        assert_eq!(node.mine_calls.lock().await.as_slice(), &[1]);
    }

    #[tokio::test]
    async fn mine_block_uses_explicit_count() {
        let node = MockNode::new(7, ControlBehavior::Ok, ControlBehavior::Ok);
        let state = test_state(single_node_network(1, node.clone()));

        let (status, body) = mine_block(
            Path(1),
            State(state),
            Json(MineBlockRequest {
                node_id: 7,
                count: Some(4),
            }),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert!(body.0.success);
        assert_eq!(node.mine_calls.lock().await.as_slice(), &[4]);
    }

    #[tokio::test]
    async fn mine_block_unsupported_node_returns_bad_request() {
        let state = test_state(vec![Network {
            id: 1,
            description: "test network".to_string(),
            name: "test".to_string(),
            query_interval: Duration::from_secs(15),
            first_tracked_height: 0,
            visible_heights_from_tip: 0,
            extra_hotspot_heights: 0,
            network_type: NetworkType::Regtest,
            disable_node_controls: false,
            nodes: vec![],
        }]);

        let (status, body) = mine_block(
            Path(1),
            State(state),
            Json(MineBlockRequest {
                node_id: 99,
                count: None,
            }),
        )
        .await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(!body.0.success);
        assert_eq!(body.0.error.as_deref(), Some("MINE_BACKEND_UNSUPPORTED"));
    }

    #[tokio::test]
    async fn mine_block_feature_disabled_by_network_config() {
        let node = MockNode::new(7, ControlBehavior::Ok, ControlBehavior::Ok);
        let state = test_state(network_with_nodes(1, true, vec![node.clone()]));

        let (status, body) = mine_block(
            Path(1),
            State(state),
            Json(MineBlockRequest {
                node_id: 7,
                count: Some(1),
            }),
        )
        .await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(!body.0.success);
        assert_eq!(body.0.error.as_deref(), Some("MINE_FEATURE_DISABLED"));
        assert!(node.mine_calls.lock().await.is_empty());
    }

    #[tokio::test]
    async fn mine_block_rejected_when_node_not_a_miner() {
        let node = MockNode::new(7, ControlBehavior::Ok, ControlBehavior::Ok)
            .with_supports_mining(false);
        let state = test_state(single_node_network(1, node.clone()));

        let (status, body) = mine_block(
            Path(1),
            State(state),
            Json(MineBlockRequest {
                node_id: 7,
                count: Some(1),
            }),
        )
        .await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(!body.0.success);
        assert_eq!(body.0.error.as_deref(), Some("MINE_NODE_NOT_A_MINER"));
        assert!(node.mine_calls.lock().await.is_empty());
    }

    #[tokio::test]
    async fn p2p_state_response_returns_true_for_active_nodes() {
        let node = MockNode::new(7, ControlBehavior::Ok, ControlBehavior::Ok).with_p2p_state(true);
        let state = test_state(single_node_network(1, node));

        let response = p2p_state_for_network(&state, 1).await;
        assert_eq!(node_p2p_state(&response, 7), Some(true));
    }

    #[tokio::test]
    async fn p2p_state_response_returns_false_for_inactive_nodes() {
        let node = MockNode::new(7, ControlBehavior::Ok, ControlBehavior::Ok).with_p2p_state(false);
        let state = test_state(single_node_network(1, node));

        let response = p2p_state_for_network(&state, 1).await;
        assert_eq!(node_p2p_state(&response, 7), Some(false));
    }

    #[tokio::test]
    async fn p2p_state_response_returns_null_for_unsupported_nodes() {
        let node = MockNode::new(7, ControlBehavior::Ok, ControlBehavior::Ok)
            .with_p2p_read_behavior(P2PReadBehavior::Unsupported);
        let state = test_state(single_node_network(1, node));

        let response = p2p_state_for_network(&state, 1).await;
        assert_eq!(node_p2p_state(&response, 7), None);
    }

    #[tokio::test]
    async fn p2p_state_response_degrades_single_node_failures_to_null() {
        let active_node =
            MockNode::new(7, ControlBehavior::Ok, ControlBehavior::Ok).with_p2p_state(false);
        let failing_node = MockNode::new(8, ControlBehavior::Ok, ControlBehavior::Ok)
            .with_p2p_read_behavior(P2PReadBehavior::Error);
        let state = test_state(network_with_nodes(
            1,
            false,
            vec![active_node, failing_node],
        ));

        let response = p2p_state_for_network(&state, 1).await;
        assert_eq!(node_p2p_state(&response, 7), Some(false));
        assert_eq!(node_p2p_state(&response, 8), None);
    }

    #[tokio::test]
    async fn set_network_active_success_path() {
        let node = MockNode::new(7, ControlBehavior::Ok, ControlBehavior::Ok).with_p2p_state(true);
        let state = test_state(single_node_network(1, node.clone()));

        let (status, body) = set_network_active(
            Path(1),
            State(state.clone()),
            Json(SetNetworkActiveRequest {
                node_id: 7,
                active: false,
            }),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert!(body.0.success);
        assert_eq!(node.network_calls.lock().await.as_slice(), &[false]);
        let response = p2p_state_for_network(&state, 1).await;
        assert_eq!(node_p2p_state(&response, 7), Some(false));
    }

    #[tokio::test]
    async fn set_network_active_unsupported_node_returns_bad_request() {
        let state = test_state(vec![Network {
            id: 1,
            description: "test network".to_string(),
            name: "test".to_string(),
            query_interval: Duration::from_secs(15),
            first_tracked_height: 0,
            visible_heights_from_tip: 0,
            extra_hotspot_heights: 0,
            network_type: NetworkType::Regtest,
            disable_node_controls: false,
            nodes: vec![],
        }]);

        let (status, body) = set_network_active(
            Path(1),
            State(state),
            Json(SetNetworkActiveRequest {
                node_id: 99,
                active: true,
            }),
        )
        .await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(!body.0.success);
        assert_eq!(
            body.0.error.as_deref(),
            Some("NETWORK_CONTROL_BACKEND_UNSUPPORTED")
        );
    }

    #[tokio::test]
    async fn set_network_active_feature_disabled_by_network_config() {
        let node = MockNode::new(7, ControlBehavior::Ok, ControlBehavior::Ok).with_p2p_state(true);
        let state = test_state(network_with_nodes(1, true, vec![node.clone()]));

        let (status, body) = set_network_active(
            Path(1),
            State(state.clone()),
            Json(SetNetworkActiveRequest {
                node_id: 7,
                active: false,
            }),
        )
        .await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(!body.0.success);
        assert_eq!(
            body.0.error.as_deref(),
            Some("NETWORK_CONTROL_FEATURE_DISABLED")
        );
        assert!(node.network_calls.lock().await.is_empty());
        let response = p2p_state_for_network(&state, 1).await;
        assert_eq!(node_p2p_state(&response, 7), Some(true));
    }

    #[tokio::test]
    async fn control_api_validation_errors_map_to_bad_request() {
        let node = MockNode::new(7, ControlBehavior::DataError, ControlBehavior::DataError);
        let state = test_state(single_node_network(1, node.clone()));

        let (mine_status, mine_body) = mine_block(
            Path(1),
            State(state.clone()),
            Json(MineBlockRequest {
                node_id: 7,
                count: Some(0),
            }),
        )
        .await;
        assert_eq!(mine_status, StatusCode::BAD_REQUEST);
        assert_eq!(mine_body.0.error.as_deref(), Some("MINE_INVALID_REQUEST"));

        let (active_status, active_body) = set_network_active(
            Path(1),
            State(state),
            Json(SetNetworkActiveRequest {
                node_id: 7,
                active: true,
            }),
        )
        .await;
        assert_eq!(active_status, StatusCode::BAD_REQUEST);
        assert_eq!(
            active_body.0.error.as_deref(),
            Some("NETWORK_CONTROL_INVALID_REQUEST")
        );
    }

    #[tokio::test]
    async fn control_api_not_supported_maps_to_bad_request() {
        let node = MockNode::new(
            7,
            ControlBehavior::NotSupported,
            ControlBehavior::NotSupported,
        )
        .with_p2p_state(true);
        let state = test_state(single_node_network(1, node));

        let (mine_status, mine_body) = mine_block(
            Path(1),
            State(state.clone()),
            Json(MineBlockRequest {
                node_id: 7,
                count: Some(1),
            }),
        )
        .await;
        assert_eq!(mine_status, StatusCode::BAD_REQUEST);
        assert_eq!(
            mine_body.0.error.as_deref(),
            Some("MINE_BACKEND_UNSUPPORTED")
        );

        let (active_status, active_body) = set_network_active(
            Path(1),
            State(state.clone()),
            Json(SetNetworkActiveRequest {
                node_id: 7,
                active: true,
            }),
        )
        .await;
        assert_eq!(active_status, StatusCode::BAD_REQUEST);
        assert_eq!(
            active_body.0.error.as_deref(),
            Some("NETWORK_CONTROL_BACKEND_UNSUPPORTED")
        );
        let response = p2p_state_for_network(&state, 1).await;
        assert_eq!(node_p2p_state(&response, 7), Some(true));
    }

    #[tokio::test]
    async fn control_api_internal_errors_map_to_server_error() {
        let node = MockNode::new(
            7,
            ControlBehavior::ExecutionError,
            ControlBehavior::ExecutionError,
        )
        .with_p2p_state(true);
        let state = test_state(single_node_network(1, node));

        let (mine_status, mine_body) = mine_block(
            Path(1),
            State(state.clone()),
            Json(MineBlockRequest {
                node_id: 7,
                count: Some(1),
            }),
        )
        .await;
        assert_eq!(mine_status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(mine_body.0.error.as_deref(), Some("MINE_EXECUTION_FAILED"));

        let (active_status, active_body) = set_network_active(
            Path(1),
            State(state),
            Json(SetNetworkActiveRequest {
                node_id: 7,
                active: true,
            }),
        )
        .await;
        assert_eq!(active_status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(
            active_body.0.error.as_deref(),
            Some("NETWORK_CONTROL_EXECUTION_FAILED")
        );
    }
}
