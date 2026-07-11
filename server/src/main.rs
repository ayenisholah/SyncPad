use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use syncpad_server::registry::Registry;
use syncpad_server::{AppState, app, snapshot};
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

/// Default snapshot interval when `SYNCPAD_SNAPSHOT_SECS` is unset (spec §6.4).
const DEFAULT_SNAPSHOT_SECS: u64 = 30;
/// Default document idle TTL when `SYNCPAD_DOC_TTL_SECS` is unset: 24 h (FR8).
const DEFAULT_DOC_TTL_SECS: u64 = 24 * 60 * 60;
/// Default reaper interval when `SYNCPAD_REAP_SECS` is unset: 1 h.
const DEFAULT_REAP_SECS: u64 = 60 * 60;

fn env_secs(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let data_dir = std::env::var("SYNCPAD_DATA_DIR").unwrap_or_else(|_| "data".to_string());
    let snapshot_secs = env_secs("SYNCPAD_SNAPSHOT_SECS", DEFAULT_SNAPSHOT_SECS);
    let doc_ttl_secs = env_secs("SYNCPAD_DOC_TTL_SECS", DEFAULT_DOC_TTL_SECS);
    let reap_secs = env_secs("SYNCPAD_REAP_SECS", DEFAULT_REAP_SECS);

    let registry = Arc::new(Registry::with_data_dir(&data_dir));
    let state = AppState {
        registry: registry.clone(),
        ..Default::default()
    };
    let snapshots = snapshot::spawn_service(
        registry.clone(),
        registry.data_dir().to_path_buf(),
        Duration::from_secs(snapshot_secs),
    );
    let reaper = snapshot::spawn_reaper(
        registry.clone(),
        registry.data_dir().to_path_buf(),
        Duration::from_secs(reap_secs),
        Duration::from_secs(doc_ttl_secs),
    );

    let port = std::env::var("PORT").unwrap_or_else(|_| "8090".to_string());
    let addr = format!("0.0.0.0:{port}");
    let listener = TcpListener::bind(&addr)
        .await
        .unwrap_or_else(|error| panic!("failed to bind {addr}: {error}"));
    tracing::info!(%addr, snapshot_secs, doc_ttl_secs, data_dir, "syncpad server listening");

    // Connect info gives the handler each peer's IP for the per-IP cap (§6.6).
    let service = app(state).into_make_service_with_connect_info::<SocketAddr>();
    axum::serve(listener, service)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("server error");

    // Graceful shutdown: stop the periodic tasks and flush every dirty document
    // once more so nothing accepted before shutdown is lost (spec §6.4).
    snapshots.abort();
    reaper.abort();
    tracing::info!("flushing snapshots before exit");
    snapshot::flush_all(&registry, registry.data_dir()).await;
}

/// Resolve when the process is asked to stop: Ctrl-C anywhere, or SIGTERM on
/// unix (how `docker stop` and service managers signal shutdown, spec §12).
async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut term) => {
                term.recv().await;
            }
            Err(error) => {
                tracing::warn!(%error, "failed to install SIGTERM handler");
                std::future::pending::<()>().await;
            }
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }
    tracing::info!("shutdown signal received");
}
