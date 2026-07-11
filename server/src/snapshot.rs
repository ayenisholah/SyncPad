//! Document persistence (spec §6.4, FR9). Snapshots are the only durability
//! in SyncPad — there is no database (N2). Each dirty document is written to
//! `<data_dir>/<docId>.json` periodically and on graceful shutdown; on the
//! next access an unknown id is hydrated from its snapshot if one exists.
//!
//! Writes are atomic: content goes to a `.tmp` sibling which is then renamed
//! over the target, so a crash mid-write never leaves a half-written file a
//! reader could hydrate from.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tokio::task::JoinHandle;

use crate::registry::Registry;

/// The on-disk form of a document (spec §6.4). The revision log is not
/// persisted — a hydrated document starts with an empty replay window, so
/// reconnects below the snapshot revision get a full resync.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub content: String,
    pub revision: u64,
    pub language: String,
    pub updated_at: u64,
}

/// Milliseconds since the Unix epoch, for the `updatedAt` field (also the
/// mtime the reaper will use, spec §6.4).
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// The snapshot file path for a document id.
pub fn path_for(data_dir: &Path, id: &str) -> PathBuf {
    data_dir.join(format!("{id}.json"))
}

/// Atomically write a document's snapshot: serialize to a `.tmp` sibling, then
/// rename over the target. Creates `data_dir` if it does not exist.
pub async fn write(data_dir: &Path, id: &str, snapshot: &Snapshot) -> std::io::Result<()> {
    tokio::fs::create_dir_all(data_dir).await?;
    let json = serde_json::to_vec_pretty(snapshot)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let final_path = path_for(data_dir, id);
    let tmp_path = final_path.with_extension("json.tmp");
    tokio::fs::write(&tmp_path, &json).await?;
    tokio::fs::rename(&tmp_path, &final_path).await?;
    Ok(())
}

/// Load a document's snapshot, or `None` if there is no readable snapshot for
/// this id. A missing or corrupt file yields `None` so a bad file on disk is
/// treated as a new document rather than crashing the join (spec §6.4).
pub async fn load(data_dir: &Path, id: &str) -> Option<Snapshot> {
    let bytes = tokio::fs::read(path_for(data_dir, id)).await.ok()?;
    match serde_json::from_slice(&bytes) {
        Ok(snapshot) => Some(snapshot),
        Err(error) => {
            tracing::warn!(id, %error, "ignoring unreadable snapshot; treating id as new");
            None
        }
    }
}

/// Ask every live document for a snapshot and persist the dirty ones. Shared by
/// the periodic service and the graceful-shutdown flush.
pub async fn flush_all(registry: &Registry, data_dir: &Path) {
    for (id, handle) in registry.handles() {
        if let Some(snapshot) = handle.snapshot().await {
            if let Err(error) = write(data_dir, &id, &snapshot).await {
                tracing::error!(id, %error, "failed to write snapshot");
            } else {
                tracing::debug!(id, revision = snapshot.revision, "snapshot written");
            }
        }
    }
}

/// Spawn the periodic snapshot service: every `interval`, flush all dirty
/// documents to disk. The first tick fires after one interval, not immediately.
pub fn spawn_service(
    registry: Arc<Registry>,
    data_dir: PathBuf,
    interval: Duration,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        ticker.tick().await; // consume the immediate first tick
        loop {
            ticker.tick().await;
            flush_all(&registry, &data_dir).await;
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A unique temporary directory for one test, removed on drop.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "syncpad-snap-{tag}-{}-{}",
                std::process::id(),
                now_ms()
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

    fn sample() -> Snapshot {
        Snapshot {
            content: "let x = 1;\n".to_string(),
            revision: 7,
            language: "rust".to_string(),
            updated_at: 1_700_000_000_000,
        }
    }

    #[test]
    fn snapshot_round_trips_and_uses_wire_names() {
        let value = serde_json::to_value(sample()).expect("to_value");
        assert_eq!(value["content"], "let x = 1;\n");
        assert_eq!(value["revision"], 7);
        assert_eq!(value["language"], "rust");
        assert_eq!(value["updatedAt"], 1_700_000_000_000u64);

        let back: Snapshot = serde_json::from_value(value).expect("from_value");
        assert_eq!(back, sample());
    }

    #[tokio::test]
    async fn write_then_load_returns_identical_snapshot() {
        let dir = TempDir::new("roundtrip");
        write(&dir.0, "doc1", &sample()).await.expect("write");
        let loaded = load(&dir.0, "doc1").await.expect("load");
        assert_eq!(loaded, sample());
    }

    #[tokio::test]
    async fn write_leaves_no_tmp_file_behind() {
        let dir = TempDir::new("notmp");
        write(&dir.0, "doc1", &sample()).await.expect("write");
        assert!(path_for(&dir.0, "doc1").exists());
        assert!(!path_for(&dir.0, "doc1").with_extension("json.tmp").exists());
    }

    #[tokio::test]
    async fn load_missing_id_is_none() {
        let dir = TempDir::new("missing");
        assert!(load(&dir.0, "nope").await.is_none());
    }

    #[tokio::test]
    async fn load_corrupt_file_is_none() {
        let dir = TempDir::new("corrupt");
        tokio::fs::write(path_for(&dir.0, "doc1"), b"{ not json")
            .await
            .expect("write corrupt");
        assert!(load(&dir.0, "doc1").await.is_none());
    }
}
