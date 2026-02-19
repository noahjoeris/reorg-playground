use std::convert::Infallible;

use axum::{
    extract::{Path, State},
    response::sse::{Event, KeepAlive, Sse},
    Json,
};
use futures_util::stream::Stream;
use futures_util::StreamExt;
use log::error;
use tokio_stream::wrappers::BroadcastStream;

use crate::types::{AppState, DataChanged, DataJsonResponse, NetworksJsonResponse};

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
