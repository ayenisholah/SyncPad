//! SyncPad server: axum routes for document creation, the sync WebSocket,
//! and static serving of the built frontend.

pub mod protocol;
pub mod registry;

use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Serialize;
use tower_http::services::{ServeDir, ServeFile};

use crate::protocol::ServerMessage;
use crate::registry::Registry;

/// Shared application state, cheap to clone per request.
#[derive(Clone, Default)]
pub struct AppState {
    pub registry: Arc<Registry>,
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
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, doc_id, state))
}

async fn handle_socket(mut socket: WebSocket, doc_id: String, state: AppState) {
    let doc = state.registry.get_or_create(&doc_id);
    let self_id = registry::random_id(16);

    let init = ServerMessage::Init {
        revision: doc.revision,
        content: doc.content,
        language: doc.language,
        participants: Vec::new(),
        self_id: self_id.clone(),
    };
    let Ok(text) = serde_json::to_string(&init) else {
        return;
    };
    if socket.send(Message::Text(text.into())).await.is_err() {
        return;
    }
    tracing::debug!(doc_id, self_id, "connection initialized");

    // The per-document task with op handling, presence, and broadcast is the
    // next milestone; until then the connection just stays open.
    while let Some(Ok(message)) = socket.recv().await {
        if matches!(message, Message::Close(_)) {
            break;
        }
    }
    tracing::debug!(doc_id, self_id, "connection closed");
}
