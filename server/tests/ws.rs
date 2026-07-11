//! WebSocket-level tests: connecting to `/ws/:docId` yields the `init`
//! message with the document's current state (spec FR2), and peers observe
//! presence joins and leaves (spec FR6).

use std::time::Duration;

use futures_util::StreamExt;
use syncpad_server::{AppState, app};
use tokio::net::TcpListener;
use tokio::time::timeout;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

type Client = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

async fn spawn_server(state: AppState) -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("local addr");
    tokio::spawn(async move {
        axum::serve(listener, app(state)).await.expect("serve");
    });
    addr
}

/// Next text frame as JSON, failing loudly if nothing arrives in time
/// (presence must reflect reality within a second — spec FR6).
async fn next_message(ws: &mut Client) -> serde_json::Value {
    let frame = timeout(Duration::from_secs(5), ws.next())
        .await
        .expect("timely message")
        .expect("open stream")
        .expect("frame");
    serde_json::from_str(frame.to_text().expect("text frame")).expect("json")
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
async fn peers_see_presence_join_and_leave() {
    let state = AppState::default();
    let doc_id = state.registry.create();
    let addr = spawn_server(state).await;

    let (mut a, _) = connect_async(format!("ws://{addr}/ws/{doc_id}"))
        .await
        .expect("connect a");
    let a_init = next_message(&mut a).await;
    assert_eq!(a_init["type"], "init");
    assert_eq!(a_init["participants"], serde_json::json!([]));
    let a_id = a_init["selfId"].as_str().expect("a selfId").to_string();

    let (mut b, _) = connect_async(format!("ws://{addr}/ws/{doc_id}"))
        .await
        .expect("connect b");
    let b_init = next_message(&mut b).await;
    assert_eq!(b_init["type"], "init");
    let b_id = b_init["selfId"].as_str().expect("b selfId").to_string();

    // The late joiner's init carries the current roster.
    let roster = b_init["participants"].as_array().expect("roster");
    assert_eq!(roster.len(), 1);
    assert_eq!(roster[0]["id"].as_str(), Some(a_id.as_str()));

    // The first client is told about the join — named and colored, and not
    // an echo of its own join.
    let joined = next_message(&mut a).await;
    assert_eq!(joined["type"], "presence");
    assert_eq!(joined["joined"]["id"].as_str(), Some(b_id.as_str()));
    assert!(
        joined["joined"]["name"]
            .as_str()
            .expect("name")
            .contains('-')
    );
    assert!(
        joined["joined"]["color"]
            .as_str()
            .expect("color")
            .starts_with('#')
    );
    assert!(joined.get("left").is_none());

    // Closing the second connection produces a leave event for the peer.
    b.close(None).await.expect("close b");
    let left = next_message(&mut a).await;
    assert_eq!(left["type"], "presence");
    assert_eq!(left["left"].as_str(), Some(b_id.as_str()));
    assert!(left.get("joined").is_none());
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
