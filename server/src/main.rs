use std::sync::Arc;
use std::time::Duration;

use syncpad_server::registry::Registry;
use syncpad_server::{AppState, app, snapshot};
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

/// Default snapshot interval when `SYNCPAD_SNAPSHOT_SECS` is unset (spec §6.4).
const DEFAULT_SNAPSHOT_SECS: u64 = 30;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let data_dir = std::env::var("SYNCPAD_DATA_DIR").unwrap_or_else(|_| "data".to_string());
    let snapshot_secs = std::env::var("SYNCPAD_SNAPSHOT_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_SNAPSHOT_SECS);

    let registry = Arc::new(Registry::with_data_dir(&data_dir));
    let state = AppState {
        registry: registry.clone(),
    };
    let snapshots = snapshot::spawn_service(
        registry.clone(),
        registry.data_dir().to_path_buf(),
        Duration::from_secs(snapshot_secs),
    );

    let port = std::env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    let addr = format!("0.0.0.0:{port}");
    let listener = TcpListener::bind(&addr)
        .await
        .unwrap_or_else(|error| panic!("failed to bind {addr}: {error}"));
    tracing::info!(%addr, snapshot_secs, data_dir, "syncpad server listening");

    axum::serve(listener, app(state))
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("server error");

    // Graceful shutdown: stop the periodic task and flush every dirty document
    // once more so nothing accepted before shutdown is lost (spec §6.4).
    snapshots.abort();
    tracing::info!("flushing snapshots before exit");
    snapshot::flush_all(&registry, registry.data_dir()).await;
}

async fn shutdown_signal() {
    if tokio::signal::ctrl_c().await.is_ok() {
        tracing::info!("shutdown signal received");
    }
}
