//! Contract-level proof of re-ingestion idempotency (D13) and the revised
//! Connector streaming shape, ahead of the real vault in M2. The fake vault
//! here is a BTreeMap applying batches the way wkyt-vault will: keyed by the
//! deterministic id, tombstones mark deletion.

use chrono::{DateTime, Utc};
use futures_util::{stream, StreamExt};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use wkyt_core::{Connector, Delta, DeltaBatch, DeltaStream, Item, ItemKind, SyncError, SyncToken};

fn ts(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
}

#[derive(Default)]
struct FakeVault {
    rows: BTreeMap<String, Item>,
    deleted: BTreeMap<String, String>, // id -> source_id
    cursor: Option<SyncToken>,
}

impl FakeVault {
    /// Mirrors the M2 contract: deltas + cursor applied as one unit.
    fn apply(&mut self, batch: &DeltaBatch) {
        for delta in &batch.deltas {
            match delta {
                Delta::Upsert(item) => {
                    self.deleted.remove(&item.id);
                    self.rows.insert(item.id.clone(), item.clone());
                }
                Delta::Tombstone { source_id } => {
                    let id = Item::deterministic_id(&batch.connector_id, source_id).to_string();
                    if self.rows.remove(&id).is_some() {
                        self.deleted.insert(id, source_id.clone());
                    }
                }
            }
        }
        if let Some(cursor) = &batch.cursor {
            self.cursor = Some(cursor.clone());
        }
    }
}

fn calendar_item(source_id: &str, version: u32) -> Item {
    Item::new(
        source_id,
        "google-calendar",
        ItemKind::Event,
        ts("2024-07-04T12:00:00Z"),
        json!({ "summary": "standup", "version": version }),
    )
}

fn batch(deltas: Vec<Delta>, cursor: Option<&str>) -> DeltaBatch {
    DeltaBatch {
        connector_id: "google-calendar".into(),
        deltas,
        cursor: cursor.map(|c| SyncToken(c.into())),
    }
}

#[test]
fn replaying_the_same_batch_twice_creates_no_duplicates() {
    let mut vault = FakeVault::default();
    let b = batch(
        vec![
            Delta::Upsert(calendar_item("evt-1", 1)),
            Delta::Upsert(calendar_item("evt-2", 1)),
        ],
        Some("cursor-a"),
    );

    vault.apply(&b);
    assert_eq!(vault.rows.len(), 2);

    // Crash-recovery scenario from D11: the same batch is replayed.
    vault.apply(&b);
    assert_eq!(vault.rows.len(), 2, "replay must not duplicate rows");
    assert_eq!(vault.cursor, Some(SyncToken("cursor-a".into())));
}

#[test]
fn re_sync_updates_in_place_and_latest_write_wins() {
    let mut vault = FakeVault::default();
    vault.apply(&batch(vec![Delta::Upsert(calendar_item("evt-1", 1))], None));
    vault.apply(&batch(vec![Delta::Upsert(calendar_item("evt-1", 2))], None));

    assert_eq!(vault.rows.len(), 1, "same source record must occupy one row");
    let row = vault.rows.values().next().unwrap();
    assert_eq!(row.properties["version"], json!(2));
}

#[test]
fn tombstone_removes_the_row_a_prior_upsert_created() {
    let mut vault = FakeVault::default();
    vault.apply(&batch(vec![Delta::Upsert(calendar_item("evt-1", 1))], None));
    vault.apply(&batch(
        vec![Delta::Tombstone {
            source_id: "evt-1".into(),
        }],
        Some("cursor-b"),
    ));

    assert!(vault.rows.is_empty());
    assert_eq!(vault.deleted.len(), 1);
}

#[test]
fn proto_round_trip_preserves_batches_exactly() {
    let mut item = calendar_item("evt-1", 7);
    item.raw_payload = Some(json!({ "etag": "xyz", "big": 9_007_199_254_740_993i64 }));
    item.kind = ItemKind::Other("custom-kind".into());
    // The wire format carries epoch millis (see delta.proto); equality holds
    // at the contract's granularity, so pin ingested_at to a sub-ms-free
    // value rather than the nanosecond-bearing Utc::now() default.
    item.ingested_at = ts("2024-07-04T12:34:56.789Z");

    let original = batch(
        vec![
            Delta::Upsert(item),
            Delta::Tombstone {
                source_id: "evt-9".into(),
            },
        ],
        Some("sync-token-123"),
    );

    let decoded = DeltaBatch::decode(&original.encode_to_vec()).expect("decode");
    assert_eq!(decoded, original);
}

/// A connector exercising the revised trait: streams two bounded batches,
/// each carrying its own checkpoint cursor.
struct TwoBatchConnector;

#[async_trait::async_trait]
impl Connector for TwoBatchConnector {
    fn id(&self) -> &str {
        "google-calendar"
    }

    async fn init(&self) -> Result<(), SyncError> {
        Ok(())
    }

    fn sync(&self, cursor: Option<SyncToken>) -> DeltaStream<'_> {
        match cursor {
            // Incremental from a known position: nothing new.
            Some(_) => Box::pin(stream::iter(vec![])),
            // Full sync: the dataset arrives as bounded batches, never as
            // one Vec of everything.
            None => Box::pin(stream::iter(vec![
                Ok(batch(vec![Delta::Upsert(calendar_item("evt-1", 1))], Some("after-1"))),
                Ok(batch(vec![Delta::Upsert(calendar_item("evt-2", 1))], Some("after-2"))),
            ])),
        }
    }
}

#[tokio::test]
async fn full_sync_then_replayed_full_sync_is_idempotent_end_to_end() {
    let connector = TwoBatchConnector;
    connector.init().await.unwrap();
    let mut vault = FakeVault::default();

    // First full sync.
    let mut s = connector.sync(None);
    while let Some(b) = s.next().await {
        vault.apply(&b.unwrap());
    }
    assert_eq!(vault.rows.len(), 2);
    assert_eq!(vault.cursor, Some(SyncToken("after-2".into())));

    // Simulate a lost cursor → orchestrator falls back to a full resync.
    let mut s = connector.sync(None);
    while let Some(b) = s.next().await {
        vault.apply(&b.unwrap());
    }
    assert_eq!(vault.rows.len(), 2, "full resync over existing data must not duplicate");

    // Incremental from the committed cursor: clean no-op.
    let mut s = connector.sync(vault.cursor.clone());
    assert!(s.next().await.is_none());
}

#[test]
fn orchestrator_can_dispatch_on_the_error_taxonomy() {
    // The match below is the orchestrator's whole contract with connectors:
    // adding a variant breaks this test (and the orchestrator) at compile
    // time, which is the point.
    fn dispatch(e: &SyncError) -> &'static str {
        match e {
            SyncError::Retryable { .. } => "retry-with-backoff",
            SyncError::AuthRequired { .. } => "pause-and-reauth",
            SyncError::ResyncRequired => "drop-cursor-full-resync",
            SyncError::Fatal { .. } => "disable-connector",
        }
    }

    assert_eq!(
        dispatch(&SyncError::ResyncRequired),
        "drop-cursor-full-resync"
    );
    assert_eq!(
        dispatch(&SyncError::AuthRequired { reason: "token revoked".into() }),
        "pause-and-reauth"
    );
}

#[test]
fn properties_survive_value_round_trip() {
    // Guard the precision rationale in delta.proto: i64s that would be
    // mangled by a double-based encoding survive the JSON-string fields.
    let big = 9_007_199_254_740_993i64; // 2^53 + 1, not representable as f64
    let v: Value = json!({ "n": big });
    let item = Item::new("evt-p", "c", ItemKind::Metric, ts("2024-01-01T00:00:00Z"), v);
    let p: wkyt_core::proto::v1::Item = (&item).into();
    let back: Item = p.try_into().unwrap();
    assert_eq!(back.properties["n"].as_i64(), Some(big));
}
