//! Persistence tests (spec §6.4, FR9): a graceful flush lets a restart
//! recover a document byte-for-byte, the periodic service bounds unflushed
//! loss to one interval, and clean documents are never written.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use operational_transform::OperationSeq;
use syncpad_server::registry::Registry;
use syncpad_server::snapshot;
use tokio::time::sleep;

/// A unique temporary directory, removed on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "syncpad-snaptest-{tag}-{}-{nanos}",
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

/// Join a document, submit one insert from revision 0, and return its id. The
/// op and any following command share the task's FIFO channel, so a snapshot
/// requested after this call always observes the edit.
async fn seed_doc(registry: &Registry, id: &str, text: &str) {
    let handle = registry.handle(id).await;
    let joined = handle.join().await.expect("join");
    handle.op(joined.self_id, 0, insert_op(text), 1).await;
}

#[tokio::test]
async fn graceful_flush_survives_a_restart() {
    let dir = TempDir::new("restart");

    // First run: edit a document, then flush as a graceful shutdown would.
    {
        let registry = Registry::with_data_dir(&dir.0);
        seed_doc(&registry, "doc-restart", "hello world").await;
        snapshot::flush_all(&registry, &dir.0).await;
    }

    // Second run: a fresh registry over the same data dir hydrates the doc.
    let restarted = Registry::with_data_dir(&dir.0);
    let joined = restarted
        .handle("doc-restart")
        .await
        .join()
        .await
        .expect("join after restart");
    assert_eq!(joined.content, "hello world");
    assert_eq!(joined.revision, 1);
}

#[tokio::test]
async fn periodic_service_bounds_unflushed_loss() {
    let dir = TempDir::new("interval");
    let registry = Arc::new(Registry::with_data_dir(&dir.0));

    let service =
        snapshot::spawn_service(registry.clone(), dir.0.clone(), Duration::from_millis(150));

    seed_doc(&registry, "doc-interval", "typed before crash").await;

    // No graceful flush: only the interval writes. After one interval the
    // snapshot must exist, so an abrupt kill loses at most that interval.
    sleep(Duration::from_millis(500)).await;
    service.abort();

    let path = snapshot::path_for(&dir.0, "doc-interval");
    let bytes = tokio::fs::read(&path).await.expect("snapshot written");
    let snap: snapshot::Snapshot = serde_json::from_slice(&bytes).expect("parse");
    assert_eq!(snap.content, "typed before crash");
    assert_eq!(snap.revision, 1);
}

#[tokio::test]
async fn clean_documents_are_not_written() {
    let dir = TempDir::new("clean");
    let registry = Registry::with_data_dir(&dir.0);

    // Join without editing: nothing is dirty.
    let handle = registry.handle("doc-clean").await;
    let _joined = handle.join().await.expect("join");

    snapshot::flush_all(&registry, &dir.0).await;
    assert!(!snapshot::path_for(&dir.0, "doc-clean").exists());
}

#[tokio::test]
async fn second_snapshot_without_edits_is_skipped() {
    let dir = TempDir::new("resnap");
    let registry = Registry::with_data_dir(&dir.0);
    seed_doc(&registry, "doc-resnap", "content").await;

    let handle = registry.handle("doc-resnap").await;
    // First snapshot clears dirty; the second finds nothing to persist.
    assert!(handle.snapshot().await.is_some());
    assert!(handle.snapshot().await.is_none());
}
