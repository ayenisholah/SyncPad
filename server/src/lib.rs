//! SyncPad server: axum routes for document creation, the sync WebSocket,
//! and static serving of the built frontend.

pub mod doc;
pub mod limits;
pub mod protocol;
pub mod registry;
pub mod snapshot;

use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{ConnectInfo, Path, State};
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use tokio::sync::broadcast::error::RecvError;
use tower_http::services::{ServeDir, ServeFile};

use crate::doc::{DocHandle, Joined, Recipients};
use crate::limits::{IpLimiter, TokenBucket};
use crate::protocol::{ClientMessage, ServerMessage};
use crate::registry::Registry;

/// Shared application state, cheap to clone per request.
#[derive(Clone, Default)]
pub struct AppState {
    pub registry: Arc<Registry>,
    pub limiter: Arc<IpLimiter>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CreateDocResponse {
    doc_id: String,
}

/// Build the full application router. The static directory holds the built
/// frontend; the SPA fallback lets `/` and `/d/:id` all serve `index.html`
/// while unknown files still 404.
pub fn app(state: AppState) -> Router {
    let static_dir = std::env::var("SYNCPAD_STATIC_DIR").unwrap_or_else(|_| "web/dist".to_string());
    let index = PathBuf::from(&static_dir).join("index.html");

    Router::new()
        .route("/api/docs", post(create_doc))
        .route("/ws/{doc_id}", get(ws_upgrade))
        .fallback_service(ServeDir::new(&static_dir).fallback(ServeFile::new(index)))
        .with_state(state)
}

async fn create_doc(State(state): State<AppState>) -> Json<CreateDocResponse> {
    let doc_id = state.registry.create();
    tracing::info!(doc_id, "document created");
    Json(CreateDocResponse { doc_id })
}

async fn ws_upgrade(
    Path(doc_id): Path<String>,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    // Cap message size at the protocol layer (spec §6.6); oversized frames
    // close the socket.
    let ws = ws.max_message_size(limits::MAX_MESSAGE_BYTES);
    let client_ip = client_ip(&headers, peer);
    ws.on_upgrade(move |socket| handle_socket(socket, doc_id, state, client_ip))
}

/// The client's IP for the per-IP cap (spec §6.6). Behind our reverse proxy the
/// socket peer is the proxy, so prefer the `X-Real-IP` / `X-Forwarded-For`
/// header it sets; fall back to the direct peer when there is no proxy (dev,
/// tests). The container is bound to loopback and only reachable through the
/// proxy, so trusting the header here is safe for this light abuse guard.
fn client_ip(headers: &HeaderMap, peer: SocketAddr) -> IpAddr {
    let from_header = |name: &str| {
        headers
            .get(name)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.split(',').next())
            .and_then(|value| value.trim().parse::<IpAddr>().ok())
    };
    from_header("x-real-ip")
        .or_else(|| from_header("x-forwarded-for"))
        .unwrap_or_else(|| peer.ip())
}

async fn handle_socket(socket: WebSocket, doc_id: String, state: AppState, peer_ip: IpAddr) {
    // Hold a per-IP slot for the connection's lifetime (spec §6.6). Refusing
    // here closes the socket before any document work happens.
    let Some(_ip_guard) = state.limiter.try_acquire(peer_ip, &doc_id) else {
        tracing::warn!(%peer_ip, doc_id, "per-IP document cap reached; refusing connection");
        return;
    };

    let handle = state.registry.handle(&doc_id).await;
    let Some(joined) = handle.join().await else {
        tracing::warn!(doc_id, "document task unavailable; closing connection");
        return;
    };
    let self_id = joined.self_id.clone();
    tracing::debug!(doc_id, self_id, "connection joined");

    run_connection(socket, handle.clone(), joined).await;

    handle.leave(self_id.clone()).await;
    tracing::debug!(doc_id, self_id, "connection closed");
}

/// Drive one connection: send `init`, then forward document events to the
/// socket from a writer task while reading client frames until the peer
/// goes away (spec §6.6).
async fn run_connection(socket: WebSocket, handle: DocHandle, joined: Joined) {
    let Joined {
        self_id,
        revision,
        content,
        language,
        participants,
        mut events,
    } = joined;

    let (mut sink, mut stream) = socket.split();

    let init = ServerMessage::Init {
        revision,
        content,
        language,
        participants,
        self_id: self_id.clone(),
    };
    let Ok(text) = serde_json::to_string(&init) else {
        return;
    };
    if sink.send(Message::Text(text.into())).await.is_err() {
        return;
    }

    let writer_id = self_id.clone();
    let mut writer = tokio::spawn(async move {
        loop {
            match events.recv().await {
                Ok(envelope) => {
                    let deliver = match &envelope.recipients {
                        Recipients::All => true,
                        Recipients::Except(id) => *id != writer_id,
                        Recipients::Only(id) => *id == writer_id,
                    };
                    if !deliver {
                        continue;
                    }
                    let Ok(text) = serde_json::to_string(&envelope.msg) else {
                        continue;
                    };
                    if sink.send(Message::Text(text.into())).await.is_err() {
                        break;
                    }
                }
                // A receiver that cannot keep up has missed events; per the
                // recovery design it must drop state and re-initialize.
                Err(RecvError::Lagged(skipped)) => {
                    tracing::warn!(
                        skipped,
                        "connection lagged behind broadcasts; forcing resync"
                    );
                    if let Ok(text) = serde_json::to_string(&ServerMessage::Resync) {
                        let _ = sink.send(Message::Text(text.into())).await;
                    }
                    break;
                }
                Err(RecvError::Closed) => break,
            }
        }
    });

    // Per-connection op-rate guard (spec §6.6). A well-behaved client keeps one
    // op in flight, so exceeding this means a broken or hostile peer; we close
    // the connection and let it reconnect + resync.
    let mut op_budget = TokenBucket::ops();

    loop {
        tokio::select! {
            _ = &mut writer => break,
            frame = stream.next() => match frame {
                Some(Ok(Message::Text(text))) => {
                    match serde_json::from_str::<ClientMessage>(text.as_str()) {
                        Ok(ClientMessage::Op {
                            base_revision,
                            ops,
                            sent_at,
                        }) => {
                            if !op_budget.try_take() {
                                tracing::warn!(self_id, "op rate exceeded; closing connection");
                                break;
                            }
                            handle.op(self_id.clone(), base_revision, ops, sent_at).await;
                        }
                        Ok(ClientMessage::Cursor {
                            position,
                            selection,
                        }) => {
                            handle.cursor(self_id.clone(), position, selection).await;
                        }
                        Ok(ClientMessage::SetLanguage { language }) => {
                            handle.set_language(language).await;
                        }
                        // ping handling arrives with the latency feature; valid
                        // messages are not an error.
                        Ok(message) => {
                            tracing::debug!(?message, "client message not handled yet");
                        }
                        Err(error) => {
                            tracing::debug!(%error, "ignoring malformed client message");
                        }
                    }
                }
                Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                Some(Ok(_)) => {}
            },
        }
    }

    writer.abort();
}
