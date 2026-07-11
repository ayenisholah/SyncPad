//! WebSocket-level tests: connecting to `/ws/:docId` yields the `init`
//! message with the document's current state (spec FR2).

use futures_util::StreamExt;
use syncpad_server::{AppState, app};
use tokio::net::TcpListener;
use tokio_tungstenite::connect_async;

async fn spawn_server(state: AppState) -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("local addr");
    tokio::spawn(async move {
        axum::serve(listener, app(state)).await.expect("serve");
    });
    addr
}

#[tokio::test]
async fn connect_receives_init_with_fresh_doc_state() {
    let state = AppState::default();
    let doc_id = state.registry.create();
    let addr = spawn_server(state).await;

    let (mut ws, _) = connect_async(format!("ws://{addr}/ws/{doc_id}"))
        .await
        .expect("ws connect");
    let message = ws.next().await.expect("first message").expect("frame");
    let value: serde_json::Value =
        serde_json::from_str(message.to_text().expect("text frame")).expect("json");

    assert_eq!(value["type"], "init");
    assert_eq!(value["revision"], 0);
    assert_eq!(value["content"], "");
    assert_eq!(value["language"], "plaintext");
    assert_eq!(value["participants"], serde_json::json!([]));
    assert!(!value["selfId"].as_str().expect("selfId").is_empty());
}

#[tokio::test]
async fn connect_to_unknown_doc_creates_it() {
    let state = AppState::default();
    let registry = state.registry.clone();
    let addr = spawn_server(state).await;

    let (mut ws, _) = connect_async(format!("ws://{addr}/ws/x7k2p9q1"))
        .await
        .expect("ws connect");
    let message = ws.next().await.expect("first message").expect("frame");
    let value: serde_json::Value =
        serde_json::from_str(message.to_text().expect("text frame")).expect("json");

    assert_eq!(value["type"], "init");
    assert!(registry.contains("x7k2p9q1"));
}

#[tokio::test]
async fn two_connections_get_distinct_self_ids() {
    let state = AppState::default();
    let doc_id = state.registry.create();
    let addr = spawn_server(state).await;

    let mut self_ids = Vec::new();
    for _ in 0..2 {
        let (mut ws, _) = connect_async(format!("ws://{addr}/ws/{doc_id}"))
            .await
            .expect("ws connect");
        let message = ws.next().await.expect("first message").expect("frame");
        let value: serde_json::Value =
            serde_json::from_str(message.to_text().expect("text frame")).expect("json");
        self_ids.push(value["selfId"].as_str().expect("selfId").to_string());
    }

    assert_ne!(self_ids[0], self_ids[1]);
}
