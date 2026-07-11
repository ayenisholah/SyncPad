//! HTTP-level tests for document creation (spec FR1).

use axum::body::Body;
use axum::http::{Request, StatusCode};
use syncpad_server::registry::{SLUG_ALPHABET, SLUG_LEN};
use syncpad_server::{AppState, app};
use tower::ServiceExt;

#[tokio::test]
async fn create_doc_returns_a_valid_slug() {
    let state = AppState::default();
    let registry = state.registry.clone();

    let response = app(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/docs")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .expect("body");
    let value: serde_json::Value = serde_json::from_slice(&bytes).expect("json");

    let doc_id = value["docId"].as_str().expect("docId string");
    assert_eq!(doc_id.len(), SLUG_LEN);
    assert!(doc_id.bytes().all(|b| SLUG_ALPHABET.contains(&b)));
    assert!(registry.contains(doc_id));
}

#[tokio::test]
async fn each_created_doc_gets_a_distinct_slug() {
    let state = AppState::default();
    let registry = state.registry.clone();
    let router = app(state);

    for _ in 0..10 {
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/docs")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::OK);
    }

    assert_eq!(registry.doc_count(), 10);
}
