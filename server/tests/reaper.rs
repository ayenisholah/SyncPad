//! Reaper tests (spec FR8, §6.4): idle, connection-less documents are expired
//! from memory and their snapshots deleted, orphan snapshot files older than
//! the TTL are cleaned up, and documents with a live connection are spared.

use std::path::PathBuf;
use std::time::Duration;

use operational_transform::OperationSeq;
use syncpad_server::registry::Registry;
use syncpad_server::snapshot::{self, Snapshot};

/// A unique temporary directory, removed on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "syncpad-reaptest-{tag}-{}-{nanos}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).expect("create temp dir");
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn insert_op(text: &str) -> serde_json::Value {
    let mut op = OperationSeq::default();
    op.insert(text);
    serde_json::to_value(&op).expect("op json")
}

#[tokio::test]
async fn idle_document_and_its_snapshot_are_reaped() {
    let dir = TempDir::new("expire");
    let registry = Registry::with_data_dir(&dir.0);

    // Edit a doc, persist it, then leave so no connection remains.
    let handle = registry.handle("doc-idle").await;
    let joined = handle.join().await.expect("join");
    handle
        .op(joined.self_id.clone(), 0, insert_op("hi"), 1)
        .await;
    snapshot::flush_all(&registry, &dir.0).await;
    handle.leave(joined.self_id).await;
    assert!(snapshot::path_for(&dir.0, "doc-idle").exists());

    // Zero idle window: the connection-less doc is expired immediately.
    snapshot::reap(&registry, &dir.0, Duration::ZERO).await;

    assert!(!registry.contains("doc-idle"));
    assert!(!snapshot::path_for(&dir.0, "doc-idle").exists());
}

#[tokio::test]
async fn live_document_is_spared() {
    let dir = TempDir::new("live");
    let registry = Registry::with_data_dir(&dir.0);

    // Join and stay connected: presence is non-empty.
    let handle = registry.handle("doc-live").await;
    let _joined = handle.join().await.expect("join");

    snapshot::reap(&registry, &dir.0, Duration::ZERO).await;

    assert!(registry.contains("doc-live"));
}

#[tokio::test]
async fn orphan_snapshot_files_are_deleted() {
    let dir = TempDir::new("orphan");
    let registry = Registry::with_data_dir(&dir.0);

    // A snapshot on disk with no live document (e.g. left by a prior run).
    let snap = Snapshot {
        content: "stale".to_string(),
        revision: 3,
        language: "plaintext".to_string(),
        updated_at: snapshot::now_ms(),
    };
    snapshot::write(&dir.0, "doc-orphan", &snap)
        .await
        .expect("write");
    assert!(snapshot::path_for(&dir.0, "doc-orphan").exists());

    snapshot::reap(&registry, &dir.0, Duration::ZERO).await;

    assert!(!snapshot::path_for(&dir.0, "doc-orphan").exists());
}
