//! File-importer connector (M4): watches one local directory,
//! non-recursively, for `.json` and `.ics` files.
//!
//! Cursor design — why mtime alone is not enough: a file *copied into* the
//! watch dir keeps its original (possibly old) modification time, and a
//! *deleted* file has no mtime at all. The cursor is therefore a JSON
//! document inside the opaque `SyncToken`:
//!
//! ```json
//! { "last_mtime_ms": 1720000000000, "known": ["a.json", "b.ics"] }
//! ```
//!
//! A file is selected when its mtime is newer than `last_mtime_ms` OR its
//! name is not in `known` (catches old-mtime copies). A name in `known`
//! that is missing on disk becomes a [`Delta::Tombstone`]. A cursor that
//! fails to parse yields `SyncError::ResyncRequired` — the orchestrator
//! discards it and full-syncs, exactly the taxonomy's purpose.
//!
//! Batches are planned up front from *metadata only* (cheap), then file
//! contents are read lazily one batch at a time as the stream is polled —
//! memory stays O(batch), not O(directory). Each batch carries a cursor
//! that is a valid resume point after that batch commits.
//!
//! Known limits, deliberate for M4: same-millisecond re-modification of a
//! known file is missed until its next touch (mtime granularity); reads
//! are synchronous std::fs (local files, bounded size); files larger than
//! [`MAX_FILE_BYTES`] are indexed by metadata but their content is not
//! ingested.

use chrono::{DateTime, Utc};
use futures_util::{stream, StreamExt as _};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::UNIX_EPOCH;
use wkyt_core::{Connector, Delta, DeltaBatch, DeltaStream, Item, ItemKind, SyncError, SyncToken};

/// Content above this size is not ingested (metadata still is).
pub const MAX_FILE_BYTES: u64 = 4 * 1024 * 1024;
const DEFAULT_BATCH_SIZE: usize = 64;
const EXTENSIONS: [&str; 2] = ["json", "ics"];

pub struct FileImporter {
    id: String,
    dir: PathBuf,
    batch_size: usize,
}

#[derive(Serialize, Deserialize, Default, Clone)]
struct FileCursor {
    last_mtime_ms: i64,
    known: BTreeSet<String>,
}

/// One pre-planned batch: names + mtimes only; contents read lazily.
struct PlannedBatch {
    tombstones: Vec<String>,
    files: Vec<(String, i64)>, // (file name, mtime ms)
    cursor: FileCursor,
}

impl FileImporter {
    pub fn new(id: impl Into<String>, dir: PathBuf) -> Self {
        Self { id: id.into(), dir, batch_size: DEFAULT_BATCH_SIZE }
    }

    /// Metadata-only scan and batch planning. No file contents touched.
    fn plan(&self, cursor: Option<SyncToken>) -> Result<Vec<PlannedBatch>, SyncError> {
        let prev: FileCursor = match cursor {
            None => FileCursor::default(),
            // Unintelligible cursor => the resume position is meaningless:
            // signal a full resync rather than guessing.
            Some(tok) => serde_json::from_str(&tok.0).map_err(|_| SyncError::ResyncRequired)?,
        };

        if !self.dir.is_dir() {
            return Err(SyncError::Fatal {
                source: format!("watch directory {:?} does not exist", self.dir).into(),
            });
        }

        // Current state of the directory (names + mtimes).
        let mut current: Vec<(String, i64)> = Vec::new();
        let entries = std::fs::read_dir(&self.dir).map_err(retryable)?;
        for entry in entries {
            let entry = entry.map_err(retryable)?;
            let path = entry.path();
            let Some(ext) = path.extension().and_then(|e| e.to_str()) else { continue };
            if !EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()) || !path.is_file() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else { continue };
            let meta = entry.metadata().map_err(retryable)?;
            let mtime_ms = meta
                .modified()
                .map_err(retryable)?
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);
            current.push((name.to_string(), mtime_ms));
        }

        let current_names: BTreeSet<String> = current.iter().map(|(n, _)| n.clone()).collect();
        let deleted: Vec<String> =
            prev.known.iter().filter(|n| !current_names.contains(*n)).cloned().collect();

        // New or modified: newer mtime, or a name we have never seen
        // (catches copied-in files that kept an old mtime).
        let mut changed: Vec<(String, i64)> = current
            .into_iter()
            .filter(|(name, mtime)| *mtime > prev.last_mtime_ms || !prev.known.contains(name))
            .collect();
        changed.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)));

        let mut plans = Vec::new();
        // Running cursor state, advanced batch by batch so every batch's
        // cursor is a correct resume point once that batch commits.
        let mut running = FileCursor {
            last_mtime_ms: prev.last_mtime_ms,
            known: prev.known.iter().filter(|n| current_names.contains(*n)).cloned().collect(),
        };

        if !deleted.is_empty() {
            plans.push(PlannedBatch {
                tombstones: deleted,
                files: Vec::new(),
                cursor: running.clone(),
            });
        }

        for chunk in changed.chunks(self.batch_size) {
            for (name, mtime) in chunk {
                running.known.insert(name.clone());
                running.last_mtime_ms = running.last_mtime_ms.max(*mtime);
            }
            plans.push(PlannedBatch {
                tombstones: Vec::new(),
                files: chunk.to_vec(),
                cursor: running.clone(),
            });
        }
        Ok(plans)
    }

    /// Read contents and materialize one planned batch. Called lazily as
    /// the stream is polled.
    fn build(&self, plan: PlannedBatch) -> Result<DeltaBatch, SyncError> {
        let mut deltas: Vec<Delta> =
            plan.tombstones.into_iter().map(|source_id| Delta::Tombstone { source_id }).collect();

        for (name, mtime_ms) in plan.files {
            let path = self.dir.join(&name);
            let meta = std::fs::metadata(&path).map_err(retryable)?;
            let timestamp = DateTime::<Utc>::from_timestamp_millis(mtime_ms)
                .unwrap_or_else(Utc::now);

            let mut properties = serde_json::json!({
                "filename": name,
                "extension": path.extension().and_then(|e| e.to_str()).unwrap_or(""),
                "size_bytes": meta.len(),
                "modified_ms": mtime_ms,
            });
            let mut raw_payload = None;

            if meta.len() <= MAX_FILE_BYTES {
                let content = std::fs::read_to_string(&path).map_err(retryable)?;
                // .json contents become structured properties when they
                // parse; anything else rides along as the raw string.
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&content) {
                    properties["content"] = parsed;
                }
                raw_payload = Some(serde_json::Value::String(content));
            } else {
                properties["content_skipped"] = serde_json::json!("exceeds MAX_FILE_BYTES");
            }

            let mut item = Item::new(&name, &self.id, ItemKind::File, timestamp, properties);
            item.raw_payload = raw_payload;
            let item_id = item.id.clone();
            deltas.push(Delta::Upsert(item));

            // Derive a claim from this file
            let claim_source_id = format!("{}-claim", name);
            let claim = Item::new(
                &claim_source_id,
                &self.id,
                ItemKind::Claim,
                timestamp,
                serde_json::json!({
                    "assertion": format!("File '{}' exists in the watched directory", name),
                    "source": "file_importer"
                })
            );
            let claim_id = claim.id.clone();

            // Create a relationship (evidence linkage)
            let rel_source_id = format!("{}-rel", name);
            let rel = Item::new(
                &rel_source_id,
                &self.id,
                ItemKind::Relationship,
                timestamp,
                serde_json::json!({
                    "source": claim_id,
                    "target": item_id,
                    "type": "has_evidence"
                })
            );
            deltas.push(Delta::Upsert(claim));
            deltas.push(Delta::Upsert(rel));
        }

        Ok(DeltaBatch {
            connector_id: self.id.clone(),
            deltas,
            cursor: Some(SyncToken(
                serde_json::to_string(&plan.cursor).expect("cursor serialization is infallible"),
            )),
        })
    }
}

fn retryable(e: std::io::Error) -> SyncError {
    SyncError::Retryable { source: Box::new(e), retry_after: None }
}

#[async_trait::async_trait]
impl Connector for FileImporter {
    fn id(&self) -> &str {
        &self.id
    }

    async fn init(&self) -> Result<(), SyncError> {
        std::fs::create_dir_all(&self.dir).map_err(retryable)?;
        Ok(())
    }

    fn sync(&self, cursor: Option<SyncToken>) -> DeltaStream<'_> {
        match self.plan(cursor) {
            Err(e) => Box::pin(stream::iter(vec![Err(e)])),
            Ok(plans) => {
                Box::pin(stream::iter(plans).map(move |plan| self.build(plan)))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::StreamExt;
    use std::fs;
    use std::path::Path;

    fn importer(dir: &Path) -> FileImporter {
        FileImporter::new("file-import", dir.to_path_buf())
    }

    async fn drain(c: &FileImporter, cursor: Option<SyncToken>) -> Vec<DeltaBatch> {
        c.sync(cursor)
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    }

    fn upserts(batches: &[DeltaBatch]) -> Vec<&Item> {
        batches
            .iter()
            .flat_map(|b| &b.deltas)
            .filter_map(|d| match d {
                Delta::Upsert(i) => Some(i),
                _ => None,
            })
            .collect()
    }

    fn last_cursor(batches: &[DeltaBatch]) -> Option<SyncToken> {
        batches.last().and_then(|b| b.cursor.clone())
    }

    #[tokio::test]
    async fn full_sync_picks_up_supported_files_only() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.json"), r#"{"k": 1}"#).unwrap();
        fs::write(dir.path().join("b.ics"), "BEGIN:VCALENDAR\nEND:VCALENDAR").unwrap();
        fs::write(dir.path().join("ignored.txt"), "nope").unwrap();

        let c = importer(dir.path());
        let batches = drain(&c, None).await;
        let items = upserts(&batches);
        assert_eq!(items.len(), 6); // 2 files + 2 claims + 2 relationships

        let a = items.iter().find(|i| i.source_id == "a.json").unwrap();
        assert_eq!(a.kind, ItemKind::File);
        assert_eq!(a.properties["content"]["k"], 1, "json content becomes structured properties");
        assert_eq!(a.id, Item::deterministic_id("file-import", "a.json").to_string());

        let b = items.iter().find(|i| i.source_id == "b.ics").unwrap();
        assert!(b.raw_payload.as_ref().unwrap().as_str().unwrap().contains("VCALENDAR"));
    }

    #[tokio::test]
    async fn incremental_sync_sees_only_changes_and_copied_in_old_files() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.json"), r#"{"v": 1}"#).unwrap();
        let c = importer(dir.path());

        let first = drain(&c, None).await;
        let cursor = last_cursor(&first);

        // Nothing changed: zero batches.
        assert!(drain(&c, cursor.clone()).await.is_empty());

        // A brand-new file with a deliberately ANCIENT mtime (simulates a
        // copy that preserved timestamps) must still be selected, because
        // it is not in the known set.
        let old = dir.path().join("old.json");
        fs::write(&old, r#"{"old": true}"#).unwrap();
        let ancient = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1000);
        let f = fs::File::options().write(true).open(&old).unwrap();
        f.set_modified(ancient).unwrap();
        drop(f);

        let second = drain(&c, cursor).await;
        let items = upserts(&second);
        assert_eq!(items.len(), 3); // 1 file + 1 claim + 1 relationship
        assert_eq!(items[0].source_id, "old.json");
    }

    #[tokio::test]
    async fn deletion_emits_tombstone() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.json"), "{}").unwrap();
        fs::write(dir.path().join("b.json"), "{}").unwrap();
        let c = importer(dir.path());
        let cursor = last_cursor(&drain(&c, None).await);

        fs::remove_file(dir.path().join("b.json")).unwrap();
        let batches = drain(&c, cursor).await;
        let tombs: Vec<_> = batches
            .iter()
            .flat_map(|b| &b.deltas)
            .filter_map(|d| match d {
                Delta::Tombstone { source_id } => Some(source_id.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(tombs, vec!["b.json"]);

        // And the tombstone is remembered: next sync is quiet.
        assert!(drain(&c, last_cursor(&batches)).await.is_empty());
    }

    #[tokio::test]
    async fn malformed_cursor_demands_full_resync() {
        let dir = tempfile::tempdir().unwrap();
        let c = importer(dir.path());
        let mut s = c.sync(Some(SyncToken("not json at all".into())));
        match s.next().await {
            Some(Err(SyncError::ResyncRequired)) => {}
            other => panic!("expected ResyncRequired, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn large_directories_stream_in_bounded_batches() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..10 {
            fs::write(dir.path().join(format!("f{i:02}.json")), "{}").unwrap();
        }
        let mut c = importer(dir.path());
        c.batch_size = 4;

        let batches = drain(&c, None).await;
        // batch_size is 4. For 10 files, each file generates 3 items (File, Claim, Rel).
        // 10 files means 10 iterations * 3 = 30 items.
        // wait, we changed `deltas.push(claim); deltas.push(rel);` inside the loop.
        // It pushes 3 items per file. The loop chunk size is `batch_size = 4` files.
        assert_eq!(batches.len(), 3, "10 files / batch_size 4 = 3 batches");
        assert!(batches.iter().all(|b| b.cursor.is_some()), "every batch is a resume point");
        assert_eq!(upserts(&batches).len(), 30);

        // Resuming from the FIRST batch's cursor re-delivers only the rest.
        let resumed = drain(&c, batches[0].cursor.clone()).await;
        assert_eq!(upserts(&resumed).len(), 18, "first 4 files already committed; 6 remain (6 * 3 = 18)");
    }

    #[tokio::test]
    async fn missing_watch_dir_is_fatal() {
        let c = FileImporter::new("file-import", PathBuf::from("/nonexistent/wkyt-test"));
        let mut s = c.sync(None);
        assert!(matches!(s.next().await, Some(Err(SyncError::Fatal { .. }))));
    }
}
