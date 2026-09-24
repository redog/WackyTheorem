//! Bounded projections over item snapshots recorded at committed batch boundaries.
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use wkyt_core::{Delta, DeltaBatch, Item, ItemKind};

use crate::vault::{row_to_item, Vault, VaultError};

const SUBSTREAM_CONNECTOR: &str = "wkyt-substreams";

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TimeAxis {
    #[default]
    Event,
    Recorded,
}

/// Times use a half-open [from, to) interval on the selected axis.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct StreamQuery {
    pub text: String,
    pub connector_ids: Vec<String>,
    pub kind: Option<ItemKind>,
    pub time_axis: TimeAxis,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    /// A committed history sequence, not a source event timestamp.
    pub as_of: Option<i64>,
    pub include_deleted: bool,
    pub limit: u32,
}

impl Default for StreamQuery {
    fn default() -> Self {
        Self {
            text: String::new(), connector_ids: vec![], kind: None,
            time_axis: TimeAxis::Event, from: None, to: None,
            as_of: None, include_deleted: false, limit: 50,
        }
    }
}

impl StreamQuery {
    pub fn validate(&self) -> Result<(), VaultError> {
        if !(1..=200).contains(&self.limit) || self.text.len() > 1024
            || self.connector_ids.len() > 32
            || self.connector_ids.iter().any(|id| id.is_empty() || id.len() > 256)
            || matches!((self.from, self.to), (Some(a), Some(b)) if a >= b)
            || self.as_of.is_some_and(|n| n < 1)
        {
            return Err(VaultError::InvalidQuery);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryBoundary {
    pub sequence: i64,
    pub recorded_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoricalItem {
    pub item: Item,
    pub revision: i64,
    pub recorded_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamResult {
    pub boundary: HistoryBoundary,
    pub coverage_start: HistoryBoundary,
    pub items: Vec<HistoricalItem>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedSubstream {
    pub id: String,
    pub name: String,
    pub query: StreamQuery,
}

pub(crate) fn initialize(conn: &mut Connection) -> Result<(), VaultError> {
    let tx = conn.transaction()?;
    tx.execute_batch("
        CREATE TABLE IF NOT EXISTS history_commits (
            sequence INTEGER PRIMARY KEY AUTOINCREMENT,
            recorded_at_ms INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS item_versions (
            sequence INTEGER NOT NULL REFERENCES history_commits(sequence),
            id TEXT NOT NULL, connector_id TEXT NOT NULL, source_id TEXT NOT NULL,
            kind TEXT NOT NULL, timestamp_ms INTEGER NOT NULL, ingested_at_ms INTEGER NOT NULL,
            properties TEXT NOT NULL, raw_payload TEXT, valid_to_ms INTEGER, deleted_at_ms INTEGER,
            PRIMARY KEY (id, sequence)
        );
        CREATE INDEX IF NOT EXISTS idx_item_versions_sequence ON item_versions(sequence);
    ")?;
    let initialized: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM history_commits)", [], |r| r.get(0),
    )?;
    if !initialized {
        let sequence = begin_commit(&tx)?;
        tx.execute(
            "INSERT INTO item_versions SELECT ?1, id, connector_id, source_id, kind,
             timestamp_ms, ingested_at_ms, properties, raw_payload, valid_to_ms, deleted_at_ms
             FROM items", [sequence],
        )?;
    }
    tx.commit()?;
    Ok(())
}

fn begin_commit(tx: &Transaction<'_>) -> Result<i64, VaultError> {
    tx.execute(
        "INSERT INTO history_commits(recorded_at_ms)
         VALUES (MAX(?1, COALESCE((SELECT MAX(recorded_at_ms) FROM history_commits), ?1)))",
        [Utc::now().timestamp_millis()],
    )?;
    Ok(tx.last_insert_rowid())
}

pub(crate) fn record_changes(
    tx: &Transaction<'_>, ids: &std::collections::BTreeSet<String>,
) -> Result<(), VaultError> {
    if ids.is_empty() { return Ok(()); }
    let sequence = begin_commit(tx)?;
    for id in ids {
        tx.execute(
            "INSERT INTO item_versions SELECT ?1, id, connector_id, source_id, kind,
             timestamp_ms, ingested_at_ms, properties, raw_payload, valid_to_ms, deleted_at_ms
             FROM items WHERE id = ?2", params![sequence, id],
        )?;
    }
    Ok(())
}

fn boundary(conn: &Connection, sequence: i64) -> Result<HistoryBoundary, VaultError> {
    let time: Option<i64> = conn.query_row(
        "SELECT recorded_at_ms FROM history_commits WHERE sequence = ?1",
        [sequence], |r| r.get(0),
    ).optional()?;
    let recorded_at = time.and_then(DateTime::from_timestamp_millis)
        .ok_or(VaultError::InvalidQuery)?;
    Ok(HistoryBoundary { sequence, recorded_at })
}

impl Vault {
    pub fn query_stream(&self, query: &StreamQuery) -> Result<StreamResult, VaultError> {
        query.validate()?;
        // Hold one read transaction even if another connection commits while we query.
        let tx = self.conn.unchecked_transaction()?;
        let latest: i64 = tx.query_row("SELECT MAX(sequence) FROM history_commits", [], |r| r.get(0))?;
        let boundary = boundary(&tx, query.as_of.unwrap_or(latest))?;
        let first: i64 = tx.query_row("SELECT MIN(sequence) FROM history_commits", [], |r| r.get(0))?;
        let coverage_start = self::boundary(&tx, first)?;
        let kind = query.kind.as_ref().map(|k| serde_json::to_string(k).expect("serializable kind"));
        let connectors = serde_json::to_string(&query.connector_ids).expect("serializable IDs");
        let mut stmt = tx.prepare("
            WITH latest AS (
                SELECT id, MAX(sequence) AS sequence FROM item_versions
                WHERE sequence <= ?1 GROUP BY id
            ), snapshot AS (
                SELECT v.*, c.recorded_at_ms,
                       CASE WHEN ?2 = 'recorded' THEN c.recorded_at_ms ELSE v.timestamp_ms END AS sort_time
                FROM latest l JOIN item_versions v ON v.id = l.id AND v.sequence = l.sequence
                JOIN history_commits c ON c.sequence = v.sequence
            )
            SELECT id, connector_id, source_id, kind, timestamp_ms, ingested_at_ms,
                   properties, raw_payload, valid_to_ms, sequence, recorded_at_ms, deleted_at_ms
            FROM snapshot
            WHERE connector_id != ?3
              AND (?4 OR deleted_at_ms IS NULL)
              AND (?5 IS NULL OR kind = ?5)
              AND (json_array_length(?6) = 0 OR connector_id IN (SELECT value FROM json_each(?6)))
              AND (?7 = '' OR instr(lower(properties), lower(?7)) > 0 OR instr(lower(source_id), lower(?7)) > 0)
              AND (?8 IS NULL OR sort_time >= ?8) AND (?9 IS NULL OR sort_time < ?9)
            ORDER BY sort_time DESC, sequence DESC, id ASC LIMIT ?10
        ")?;
        let rows = stmt.query_map(params![
            boundary.sequence, if query.time_axis == TimeAxis::Recorded { "recorded" } else { "event" },
            SUBSTREAM_CONNECTOR, query.include_deleted, kind, connectors, query.text,
            query.from.map(|t| t.timestamp_millis()), query.to.map(|t| t.timestamp_millis()), query.limit + 1,
        ], |r| {
            Ok((row_to_item(r)?, r.get::<_, i64>(9)?, r.get::<_, i64>(10)?, r.get::<_, Option<i64>>(11)?))
        })?;
        let mut items = Vec::new();
        for row in rows {
            let (item, revision, recorded, deleted) = row?;
            items.push(HistoricalItem {
                item: item?, revision,
                recorded_at: DateTime::from_timestamp_millis(recorded).ok_or(VaultError::InvalidQuery)?,
                deleted_at: deleted.map(|t| DateTime::from_timestamp_millis(t).ok_or(VaultError::InvalidQuery)).transpose()?,
            });
        }
        let truncated = items.len() > query.limit as usize;
        items.truncate(query.limit as usize);
        Ok(StreamResult { boundary, coverage_start, items, truncated })
    }

    pub fn save_substream(&mut self, saved: &SavedSubstream) -> Result<(), VaultError> {
        saved.query.validate()?;
        if saved.id.is_empty() || saved.id.len() > 128 || saved.name.trim().is_empty() || saved.name.len() > 256 {
            return Err(VaultError::InvalidQuery);
        }
        if let Some(sequence) = saved.query.as_of { boundary(&self.conn, sequence)?; }
        let properties = serde_json::to_value(saved).expect("serializable query");
        let existing = self.items(SUBSTREAM_CONNECTOR)?.into_iter().find(|i| i.source_id == saved.id);
        if existing.as_ref().is_some_and(|i| i.properties == properties) { return Ok(()); }
        let item = Item::new(&saved.id, SUBSTREAM_CONNECTOR, ItemKind::Other("saved_substream".into()), Utc::now(), properties);
        self.apply_batch(&DeltaBatch { connector_id: SUBSTREAM_CONNECTOR.into(), cursor: None, deltas: vec![Delta::Upsert(item)] })
    }

    pub fn saved_substreams(&self) -> Result<Vec<SavedSubstream>, VaultError> {
        let mut saved = self.items(SUBSTREAM_CONNECTOR)?.into_iter().map(|i| {
            serde_json::from_value(i.properties).map_err(|_| VaultError::CorruptRow {
                id: i.id, reason: "invalid saved substream".into(),
            })
        }).collect::<Result<Vec<SavedSubstream>, _>>()?;
        saved.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
        Ok(saved)
    }

    pub fn delete_substream(&mut self, id: &str) -> Result<(), VaultError> {
        self.apply_batch(&DeltaBatch {
            connector_id: SUBSTREAM_CONNECTOR.into(), cursor: None,
            deltas: vec![Delta::Tombstone { source_id: id.into() }],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{KeyService, MemoryKekStore};
    use serde_json::json;

    fn rig() -> (tempfile::TempDir, crate::Dek, Vault) {
        let dir = tempfile::tempdir().unwrap();
        let (dek, _) = KeyService::new(MemoryKekStore::default(), dir.path()).provision().unwrap();
        let vault = Vault::open(&dir.path().join("vault.db"), &dek).unwrap();
        (dir, dek, vault)
    }
    fn item(id: &str, connector: &str, text: &str) -> Item {
        let mut item = Item::new(id, connector, ItemKind::File,
            DateTime::from_timestamp_millis(1000).unwrap(), json!({"text": text}));
        item.ingested_at = DateTime::from_timestamp_millis(2000).unwrap();
        item
    }
    fn apply(v: &mut Vault, items: Vec<Item>) {
        v.apply_batch(&DeltaBatch { connector_id: "files".into(), cursor: None,
            deltas: items.into_iter().map(Delta::Upsert).collect() }).unwrap();
    }
    fn query(v: &Vault) -> StreamResult { v.query_stream(&StreamQuery::default()).unwrap() }

    #[test]
    fn late_evidence_corrections_and_replays_preserve_prior_knowledge() {
        let (_dir, _dek, mut v) = rig();
        let initial = query(&v).boundary.sequence;
        let a = item("a", "files", "Alpha");
        apply(&mut v, vec![a.clone()]);
        let before = query(&v).boundary.sequence;
        let mut replay = a.clone();
        replay.ingested_at += chrono::Duration::hours(1);
        apply(&mut v, vec![replay]);
        assert_eq!(query(&v).boundary.sequence, before);
        let mut changed = a.clone();
        changed.properties = json!({"text": "Beta"});
        let mut late = item("b", "calendar", "Alpha");
        late.timestamp = DateTime::from_timestamp_millis(500).unwrap();
        apply(&mut v, vec![changed, late]);
        let old = v.query_stream(&StreamQuery { as_of: Some(before), text: "Alpha".into(), ..Default::default() }).unwrap();
        assert_eq!(old.items.len(), 1);
        assert_eq!(old.items[0].item, a);
        let current = v.query_stream(&StreamQuery { text: "Alpha".into(), ..Default::default() }).unwrap();
        assert_eq!(current.items.len(), 1, "filter after reconstructing latest versions");
        assert_eq!(current.items[0].item.connector_id, "calendar");
        assert!(v.query_stream(&StreamQuery { as_of: Some(initial), ..Default::default() }).unwrap().items.is_empty());
    }

    #[test]
    fn field_changes_tombstones_and_reviving_are_historical() {
        let (_dir, _dek, mut v) = rig();
        let mut a = item("a", "files", "Alpha");
        apply(&mut v, vec![a.clone()]);
        let first = query(&v).boundary.sequence;
        a.raw_payload = Some(json!({"original": true}));
        a.kind = ItemKind::Event;
        a.timestamp += chrono::Duration::seconds(1);
        a.valid_to = Some(a.timestamp + chrono::Duration::days(1));
        apply(&mut v, vec![a.clone()]);
        let changed = query(&v).boundary.sequence;
        assert!(changed > first);
        let tombstone = DeltaBatch { connector_id: "files".into(), cursor: None,
            deltas: vec![Delta::Tombstone { source_id: "a".into() }] };
        v.apply_batch(&tombstone).unwrap();
        let deleted = query(&v).boundary.sequence;
        assert!(query(&v).items.is_empty());
        v.apply_batch(&tombstone).unwrap();
        assert_eq!(query(&v).boundary.sequence, deleted);
        let historical = v.query_stream(&StreamQuery { as_of: Some(changed), ..Default::default() }).unwrap();
        assert_eq!(historical.items[0].item, a);
        assert!(v.query_stream(&StreamQuery { include_deleted: true, ..Default::default() }).unwrap().items[0].deleted_at.is_some());
        apply(&mut v, vec![a]);
        assert_eq!(query(&v).items.len(), 1);
    }

    #[test]
    fn failed_batch_rolls_back_history_items_and_cursor() {
        let (_dir, _dek, mut v) = rig();
        let a = item("a", "files", "Alpha");
        apply(&mut v, vec![a.clone()]);
        let before = query(&v).boundary.sequence;
        let mut invalid = a;
        invalid.id = "identity-collision".into();
        let batch = DeltaBatch { connector_id: "files".into(), cursor: Some(wkyt_core::SyncToken("bad".into())),
            deltas: vec![Delta::Upsert(item("b", "files", "Beta")), Delta::Upsert(invalid)] };
        assert!(v.apply_batch(&batch).is_err());
        assert_eq!(query(&v).boundary.sequence, before);
        assert_eq!(query(&v).items.len(), 1);
        assert!(v.cursor("files").unwrap().is_none());
    }

    #[test]
    fn existing_vault_baseline_survives_reopen_without_fabricated_past() {
        let (dir, dek, mut v) = rig();
        apply(&mut v, vec![item("a", "files", "Alpha")]);
        // Simulate the pre-extension schema while retaining items and legacy revisions.
        v.conn.execute_batch("DROP TABLE item_versions; DROP TABLE history_commits;").unwrap();
        drop(v);
        let v = Vault::open(&dir.path().join("vault.db"), &dek).unwrap();
        let baseline = query(&v);
        assert_eq!(baseline.items.len(), 1);
        assert_eq!(baseline.boundary.sequence, baseline.coverage_start.sequence);
        assert!(v.query_stream(&StreamQuery { as_of: Some(0), ..Default::default() }).is_err());
        assert!(v.query_stream(&StreamQuery { as_of: Some(999), ..Default::default() }).is_err());
        drop(v);
        let v = Vault::open(&dir.path().join("vault.db"), &dek).unwrap();
        assert_eq!(query(&v).boundary.sequence, baseline.boundary.sequence);
    }

    #[test]
    fn ordering_limits_axes_and_validation_are_explicit() {
        let (_dir, _dek, mut v) = rig();
        apply(&mut v, vec![item("a", "files", "100%_Alpha"), item("b", "calendar", "Alpha")]);
        let all = query(&v);
        assert_eq!(all.items[0].revision, all.items[1].revision, "one atomic batch boundary");
        let limited = v.query_stream(&StreamQuery { limit: 1, ..Default::default() }).unwrap();
        assert!(limited.truncated);
        assert_eq!(limited.items[0].item.id, all.items[0].item.id);
        let filtered = v.query_stream(&StreamQuery { text: "%_".into(), ..Default::default() }).unwrap();
        assert_eq!(filtered.items.len(), 1, "literal matching, not SQL wildcards");
        let start = DateTime::from_timestamp_millis(1000).unwrap();
        let end = start + chrono::Duration::seconds(1);
        let event_query = StreamQuery { from: Some(start), to: Some(end), ..Default::default() };
        assert_eq!(v.query_stream(&event_query).unwrap().items.len(), 2);
        assert!(v.query_stream(&StreamQuery { time_axis: TimeAxis::Recorded, ..event_query }).unwrap().items.is_empty());
        assert_eq!(v.query_stream(&StreamQuery { connector_ids: vec!["calendar".into()], ..Default::default() }).unwrap().items.len(), 1);
        assert!(v.query_stream(&StreamQuery { limit: 201, ..Default::default() }).is_err());
        assert!(v.query_stream(&StreamQuery { from: Some(end), to: Some(start), ..Default::default() }).is_err());
        // Simulate a backwards clock relative to the last recorded boundary.
        v.conn.execute("UPDATE history_commits SET recorded_at_ms = ?1", [Utc::now().timestamp_millis() + 60000]).unwrap();
        let before = query(&v).boundary;
        apply(&mut v, vec![item("c", "files", "next")]);
        let after = query(&v).boundary;
        assert!(after.sequence > before.sequence);
        assert!(after.recorded_at >= before.recorded_at);
    }

    #[test]
    fn saved_views_overlap_update_survive_restart_and_do_not_own_evidence() {
        let (dir, dek, mut v) = rig();
        apply(&mut v, vec![item("a", "files", "Alpha")]);
        let boundary = query(&v).boundary.sequence;
        let mut live = SavedSubstream { id: "live".into(), name: "Alpha".into(),
            query: StreamQuery { text: "Alpha".into(), ..Default::default() } };
        let pinned = SavedSubstream { id: "pinned".into(), name: "Earlier Alpha".into(),
            query: StreamQuery { as_of: Some(boundary), ..live.query.clone() } };
        v.save_substream(&live).unwrap();
        v.save_substream(&pinned).unwrap();
        let before = query(&v).boundary.sequence;
        v.save_substream(&live).unwrap();
        assert_eq!(query(&v).boundary.sequence, before);
        live.name = "Renamed Alpha".into();
        v.save_substream(&live).unwrap();
        assert!(query(&v).boundary.sequence > before);
        let id = Item::deterministic_id(SUBSTREAM_CONNECTOR, "live").to_string();
        assert_eq!(v.item_revisions(&id).unwrap().len(), 1);
        apply(&mut v, vec![item("b", "calendar", "Alpha")]);
        assert_eq!(v.query_stream(&live.query).unwrap().items.len(), 2);
        assert_eq!(v.query_stream(&pinned.query).unwrap().items.len(), 1);
        drop(v);
        let mut v = Vault::open(&dir.path().join("vault.db"), &dek).unwrap();
        assert_eq!(v.saved_substreams().unwrap().len(), 2);
        v.delete_substream("live").unwrap();
        assert_eq!(v.saved_substreams().unwrap().len(), 1);
        assert_eq!(query(&v).items.len(), 2, "removing a view preserves sources");
        assert_eq!(v.query_stream(&pinned.query).unwrap().items.len(), 1);
    }
}
