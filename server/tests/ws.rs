//! WebSocket-level tests: connecting to `/ws/:docId` yields the `init`
//! message with the document's current state (spec FR2), peers observe
//! presence joins and leaves (spec FR6), and concurrent edits are
//! transformed so all clients converge (spec FR3).

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use operational_transform::OperationSeq;
use syncpad_server::{AppState, app};
use tokio::net::TcpListener;
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

type Client = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

async fn spawn_server(state: AppState) -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("local addr");
    tokio::spawn(async move {
        // Connect info is required by the WS handler for the per-IP cap.
        let service = app(state).into_make_service_with_connect_info::<std::net::SocketAddr>();
        axum::serve(listener, service).await.expect("serve");
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

/// Send an `op` message built with the crate's own serialization.
async fn send_op(
    ws: &mut Client,
    base_revision: u64,
    sent_at: u64,
    build: impl FnOnce(&mut OperationSeq),
) {
    let mut op = OperationSeq::default();
    build(&mut op);
    let message = serde_json::json!({
        "type": "op",
        "baseRevision": base_revision,
        "ops": serde_json::to_value(&op).expect("op json"),
        "sentAt": sent_at,
    });
    ws.send(Message::Text(message.to_string().into()))
        .await
        .expect("send op");
}

/// Apply a received `op` message's ops to a local view of the content.
fn apply_ops(message: &serde_json::Value, view: &str) -> String {
    let op: OperationSeq =
        serde_json::from_value(message["ops"].clone()).expect("received ops parse");
    op.apply(view).expect("received ops apply")
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
async fn edits_reach_peers_and_late_joiners() {
    let state = AppState::default();
    let doc_id = state.registry.create();
    let addr = spawn_server(state).await;

    let (mut a, _) = connect_async(format!("ws://{addr}/ws/{doc_id}"))
        .await
        .expect("connect a");
    let a_init = next_message(&mut a).await;
    let a_id = a_init["selfId"].as_str().expect("a selfId").to_string();

    let (mut b, _) = connect_async(format!("ws://{addr}/ws/{doc_id}"))
        .await
        .expect("connect b");
    let _b_init = next_message(&mut b).await;
    let _b_joined = next_message(&mut a).await;

    send_op(&mut a, 0, 7, |op| op.insert("hello")).await;

    // The author gets exactly an ack; the peer gets the operation.
    let ack = next_message(&mut a).await;
    assert_eq!(ack["type"], "ack");
    assert_eq!(ack["revision"], 1);

    let op = next_message(&mut b).await;
    assert_eq!(op["type"], "op");
    assert_eq!(op["revision"], 1);
    assert_eq!(op["authorId"].as_str(), Some(a_id.as_str()));
    assert_eq!(op["sentAt"], 7);
    assert_eq!(apply_ops(&op, ""), "hello");

    // A late joiner sees the edited content at the current revision (FR2).
    let (mut c, _) = connect_async(format!("ws://{addr}/ws/{doc_id}"))
        .await
        .expect("connect c");
    let c_init = next_message(&mut c).await;
    assert_eq!(c_init["type"], "init");
    assert_eq!(c_init["revision"], 1);
    assert_eq!(c_init["content"], "hello");
}

#[tokio::test]
async fn stale_base_edits_are_transformed_to_convergence() {
    let state = AppState::default();
    let doc_id = state.registry.create();
    let addr = spawn_server(state).await;

    let (mut a, _) = connect_async(format!("ws://{addr}/ws/{doc_id}"))
        .await
        .expect("connect a");
    let _a_init = next_message(&mut a).await;
    let (mut b, _) = connect_async(format!("ws://{addr}/ws/{doc_id}"))
        .await
        .expect("connect b");
    let _b_init = next_message(&mut b).await;
    let _b_joined = next_message(&mut a).await;

    // A establishes revision 1; B sees it.
    send_op(&mut a, 0, 1, |op| op.insert("ab")).await;
    let ack = next_message(&mut a).await;
    assert_eq!(ack["revision"], 1);
    let op_for_b = next_message(&mut b).await;
    let mut a_view = String::from("ab");
    assert_eq!(apply_ops(&op_for_b, ""), a_view);

    // B now sends an operation still based on revision 0 — the server must
    // transform it against A's concurrent edit instead of corrupting state.
    send_op(&mut b, 0, 2, |op| op.insert("z")).await;
    let ack = next_message(&mut b).await;
    assert_eq!(ack["type"], "ack");
    assert_eq!(ack["revision"], 2);

    let transformed = next_message(&mut a).await;
    assert_eq!(transformed["type"], "op");
    assert_eq!(transformed["revision"], 2);
    a_view = apply_ops(&transformed, &a_view);

    // Everyone converges on the authoritative content.
    let (mut c, _) = connect_async(format!("ws://{addr}/ws/{doc_id}"))
        .await
        .expect("connect c");
    let c_init = next_message(&mut c).await;
    assert_eq!(c_init["revision"], 2);
    assert_eq!(c_init["content"].as_str(), Some(a_view.as_str()));
}

#[tokio::test]
async fn out_of_window_ops_force_resync() {
    let state = AppState::default();
    let doc_id = state.registry.create();
    let addr = spawn_server(state).await;

    let (mut a, _) = connect_async(format!("ws://{addr}/ws/{doc_id}"))
        .await
        .expect("connect a");
    let a_init = next_message(&mut a).await;
    let a_id = a_init["selfId"].as_str().expect("a selfId").to_string();

    send_op(&mut a, 99, 0, |op| op.insert("x")).await;

    let resync = next_message(&mut a).await;
    assert_eq!(resync["type"], "resync");

    let init = next_message(&mut a).await;
    assert_eq!(init["type"], "init");
    assert_eq!(init["revision"], 0);
    assert_eq!(init["content"], "");
    assert_eq!(init["selfId"].as_str(), Some(a_id.as_str()));
}

#[tokio::test]
async fn op_flood_closes_the_connection() {
    // Sending far more than the 100 ops/s budget in a burst trips the
    // per-connection token bucket (spec §6.6); the server closes the socket.
    let state = AppState::default();
    let doc_id = state.registry.create();
    let addr = spawn_server(state).await;

    let (mut a, _) = connect_async(format!("ws://{addr}/ws/{doc_id}"))
        .await
        .expect("connect a");
    let _a_init = next_message(&mut a).await;

    // Burst well past the 100-token capacity, faster than it refills. Once the
    // guard trips, the server aborts the socket, so a send may itself fail —
    // that is the closure we are testing for.
    let mut closed = false;
    for i in 0..300u64 {
        let op = {
            let mut op = OperationSeq::default();
            op.insert("x");
            op
        };
        let message = serde_json::json!({
            "type": "op",
            "baseRevision": 0,
            "ops": serde_json::to_value(&op).expect("op json"),
            "sentAt": i,
        });
        if a.send(Message::Text(message.to_string().into()))
            .await
            .is_err()
        {
            closed = true;
            break;
        }
    }

    // If sends all succeeded, drain responses until the stream ends. A bound
    // stops the test if the guard somehow failed to fire.
    if !closed {
        for _ in 0..1000 {
            match timeout(Duration::from_secs(5), a.next()).await {
                Ok(Some(Ok(Message::Close(_)))) | Ok(Some(Err(_))) | Ok(None) => {
                    closed = true;
                    break;
                }
                Ok(Some(Ok(_))) => continue,
                Err(_) => panic!("connection neither closed nor produced frames in time"),
            }
        }
    }
    assert!(closed, "flood should have closed the connection");
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
