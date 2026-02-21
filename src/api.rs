use std::convert::Infallible;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
    Json,
};
use futures_util::stream::Stream;
use futures_util::StreamExt;
use log::error;
use serde::{Deserialize, Serialize};
use tokio_stream::wrappers::BroadcastStream;

use crate::config::NetworkType;
use crate::types::{AppState, DataChanged, DataJsonResponse, MineAuth, MineableNodeInfo, NetworksJsonResponse};

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

pub async fn changes_sse(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state.cache_changed_tx.subscribe();
    let stream = BroadcastStream::new(rx).map(|result| {
        let network_id = match result {
            Ok(id) => id,
            Err(e) => {
                error!("Could not SSE notify about tip changed event: {}", e);
                u32::MAX
            }
        };
        Ok::<_, Infallible>(
            Event::default()
                .event("cache_changed")
                .json_data(DataChanged { network_id })
                .unwrap_or_default(),
        )
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}

// -- Mine block --

#[derive(Deserialize)]
pub struct MineBlockRequest {
    pub node_id: u32,
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
    let network_mine_info = match state.mine_info.get(&network_id) {
        Some(info) => info,
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

    match &network_mine_info.network_type {
        Some(NetworkType::Regtest) => { /* OK */ }
        Some(NetworkType::Signet) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(MineBlockResponse {
                    success: false,
                    error: Some("MINE_SIGNET_NOT_IMPLEMENTED".to_string()),
                }),
            );
        }
        Some(NetworkType::Mainnet) | Some(NetworkType::Testnet) | None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(MineBlockResponse {
                    success: false,
                    error: Some("MINE_VIEW_ONLY_NETWORK".to_string()),
                }),
            );
        }
    }

    let node_info = match network_mine_info.nodes.get(&body.node_id) {
        Some(info) => info,
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

    match execute_mine(node_info).await {
        Ok(_) => (
            StatusCode::OK,
            Json(MineBlockResponse {
                success: true,
                error: None,
            }),
        ),
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

const MINE_WALLET: &str = "miner";

fn base_args(node: &MineableNodeInfo) -> Vec<String> {
    let mut args = vec![
        "-regtest".to_string(),
        format!("-rpcconnect={}", node.rpc_host),
        format!("-rpcport={}", node.rpc_port),
    ];
    match &node.rpc_auth {
        MineAuth::CookieFile(path) => {
            args.push(format!("-rpccookiefile={}", path.display()));
        }
        MineAuth::UserPass(user, pass) => {
            args.push(format!("-rpcuser={}", user));
            args.push(format!("-rpcpassword={}", pass));
        }
    }
    args
}

async fn run_cli(args: &[String]) -> Result<String, String> {
    let output: std::process::Output = tokio::process::Command::new("bitcoin-cli")
        .args(args)
        .output()
        .await
        .map_err(|e| format!("Failed to spawn bitcoin-cli: {}", e))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(format!(
            "bitcoin-cli exited with {}: {}",
            output.status, stderr
        ))
    }
}

async fn ensure_wallet(node: &MineableNodeInfo) -> Result<(), String> {
    let mut args = base_args(node);
    args.push("listwallets".to_string());
    let loaded = run_cli(&args).await?;

    if loaded.contains(MINE_WALLET) {
        return Ok(());
    }

    // Try loading an existing wallet first
    let mut args = base_args(node);
    args.extend(["loadwallet".to_string(), MINE_WALLET.to_string()]);
    if run_cli(&args).await.is_ok() {
        return Ok(());
    }

    // Wallet doesn't exist yet — create it
    let mut args = base_args(node);
    args.extend(["createwallet".to_string(), MINE_WALLET.to_string()]);
    run_cli(&args).await?;
    Ok(())
}

async fn execute_mine(node: &MineableNodeInfo) -> Result<String, String> {
    ensure_wallet(node).await?;

    let mut args = base_args(node);
    args.push(format!("-rpcwallet={}", MINE_WALLET));
    args.push("-generate".to_string());
    args.push("1".to_string());

    run_cli(&args).await
}
