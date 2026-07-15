//! sqlcipher vault: initialization (T2.3), schema + transactional
//! batch-apply (T2.4/T2.5), and DEK rotation (D12 `PRAGMA rekey`).
//!
//! The DEK is applied as a *raw* key (`PRAGMA key = "x'<hex>'"`), not a
//! passphrase: sqlcipher then skips its PBKDF2 derivation, which is both
//! correct (our DEK is already a uniformly random 256-bit key) and avoids
//! pretending a KDF adds anything on top of one.
//!
//! Memory handling around `PRAGMA key`/`PRAGMA rekey` (D12), in order of
//! custody:
//!  1. `Dek` — `Zeroizing<[u8;32]>`, zeroized on drop.
//!  2. The hex expansion and the composed `PRAGMA` SQL string — both
//!     `Zeroizing<String>`, dropped (and erased) immediately after the
//!     statement executes.
//!  3. Beyond our control, documented honestly: `sqlite3_prepare` copies
//!     the SQL text into SQLite's own heap, and sqlcipher retains the raw
//!     key internally for the life of the connection. Mitigation:
//!     `PRAGMA cipher_memory_security = ON`, which makes sqlcipher
//!     mlock/zero its internal crypto buffers (small perf cost, right
//!     trade for a personal vault).
//!
//! Fail-closed: after keying, we immediately query `sqlite_master`. With a
//! wrong key or a plaintext/corrupt file, sqlcipher returns NOTADB and we
//! refuse the connection — there is no path where the vault silently
//! operates unencrypted.
//!
//! Durability contract (D11): [`Vault::apply_batch`] applies a
//! `DeltaBatch`'s deltas AND its cursor in one SQLite transaction. A crash
//! mid-batch rolls back to the previous cursor; replaying from that cursor
//! is harmless because item identity is deterministic (D13) and writes are
//! idempotent upserts.

use crate::hexfmt;
use crate::keys::{Dek, KekStore, KeyError, KeyService};
use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension};
use std::path::Path;
use wkyt_core::{Delta, DeltaBatch, Item, ItemKind, SyncToken};
use zeroize::Zeroizing;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ItemRevision {
    pub revision_id: i64,
    pub item_id: String,
    pub properties: serde_json::Value,
    pub replaced_at: DateTime<Utc>,
}

#[derive(Debug, thiserror::Error)]
pub enum VaultError {
    /// Wrong DEK, tampered file, or a plaintext database where the vault
    /// should be. Deliberately one variant: callers get no oracle.
    #[error("vault cannot be opened: wrong key or not an encrypted vault")]
    WrongKeyOrCorrupt,
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("key service: {0}")]
    Key(#[from] KeyError),
    /// A stored row that no longer maps onto domain types (e.g. kind JSON
    /// from a future schema). Surfaced, never silently skipped.
    #[error("corrupt row for item {id}: {reason}")]
    CorruptRow { id: String, reason: String },
}

const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS vault_meta (
        key   TEXT PRIMARY KEY,
        value TEXT NOT NULL
    );

    -- D13: id is the UUIDv5 over (connector_id, source_id); the UNIQUE
    -- constraint is the belt-and-braces guarantee independent of how ids
    -- are derived. Timestamps are epoch millis UTC, matching the wire
    -- format. Tombstones are soft deletes: deleted_at_ms non-NULL.
    CREATE TABLE IF NOT EXISTS items (
        id             TEXT PRIMARY KEY,
        connector_id   TEXT NOT NULL,
        source_id      TEXT NOT NULL,
        kind           TEXT NOT NULL,
        timestamp_ms   INTEGER NOT NULL,
        ingested_at_ms INTEGER NOT NULL,
        properties     TEXT NOT NULL,
        raw_payload    TEXT,
        deleted_at_ms  INTEGER,
        valid_to_ms    INTEGER,
        UNIQUE (connector_id, source_id)
    );
    CREATE INDEX IF NOT EXISTS idx_items_timestamp ON items (timestamp_ms);
    CREATE INDEX IF NOT EXISTS idx_items_connector ON items (connector_id);

    -- D15/M2: Item revision history. We store historical states of items.
    -- Handled via a trigger on update.
    CREATE TABLE IF NOT EXISTS item_revisions (
        revision_id    INTEGER PRIMARY KEY AUTOINCREMENT,
        item_id        TEXT NOT NULL,
        kind           TEXT NOT NULL,
        timestamp_ms   INTEGER NOT NULL,
        ingested_at_ms INTEGER NOT NULL,
        properties     TEXT NOT NULL,
        raw_payload    TEXT,
        deleted_at_ms  INTEGER,
        valid_to_ms    INTEGER,
        replaced_at_ms INTEGER NOT NULL,
        FOREIGN KEY(item_id) REFERENCES items(id)
    );

    CREATE TRIGGER IF NOT EXISTS item_update_revision
    AFTER UPDATE ON items
    FOR EACH ROW
    WHEN old.properties != new.properties OR old.deleted_at_ms IS NOT new.deleted_at_ms OR old.valid_to_ms IS NOT new.valid_to_ms
    BEGIN
        INSERT INTO item_revisions (
            item_id, kind, timestamp_ms, ingested_at_ms,
            properties, raw_payload, deleted_at_ms, valid_to_ms, replaced_at_ms
        ) VALUES (
            old.id, old.kind, old.timestamp_ms, old.ingested_at_ms,
            old.properties, old.raw_payload, old.deleted_at_ms, old.valid_to_ms, (strftime('%s','now') * 1000)
        );
    END;

    -- One opaque resume position per connector (D11): only ever written
    -- inside the same transaction as the batch it covers.
    CREATE TABLE IF NOT EXISTS cursors (
        connector_id  TEXT PRIMARY KEY,
        cursor        TEXT NOT NULL,
        updated_at_ms INTEGER NOT NULL
    );

    INSERT OR IGNORE INTO vault_meta (key, value) VALUES ('schema_version', '1');
";

pub struct Vault {
    conn: Connection,
}

impl Vault {
    /// Open (creating if absent) the encrypted vault at `path` with `dek`.
    pub fn open(path: &Path, dek: &Dek) -> Result<Self, VaultError> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let conn = Connection::open(path)?;
        apply_key_pragma(&conn, "key", dek)?;

        // Fail-closed verification: first real page read. Wrong key or a
        // plaintext SQLite file surfaces as NOTADB here.
        conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get::<_, i64>(0))
            .map_err(map_notadb)?;

        conn.execute_batch(
            // cipher_memory_security: sqlcipher locks + wipes its internal
            //   crypto buffers (see module docs).
            // temp_store = MEMORY: the D10 follow-up, made explicit instead
            //   of assumed — SQLite temp b-trees/spill stay off disk, so no
            //   plaintext intermediate files (Spec: nothing plaintext on disk).
            // foreign_keys: schema integrity from day one.
            "PRAGMA cipher_memory_security = ON;
             PRAGMA temp_store = MEMORY;
             PRAGMA foreign_keys = ON;",
        )
        .map_err(map_notadb)?;
        conn.execute_batch(SCHEMA)?;

        restrict_permissions(path)?;
        Ok(Self { conn })
    }

    /// Re-encrypt the database under a new DEK (D12 rotation). The page
    /// rewrite is journaled by sqlcipher. Callers must go through
    /// [`rotate_dek`], which sequences this with the wrapped-blob updates.
    fn rekey(&self, new_dek: &Dek) -> Result<(), VaultError> {
        apply_key_pragma(&self.conn, "rekey", new_dek)
    }

    /// T2.5: apply a batch atomically — every delta AND the cursor commit
    /// together, or none of it does.
    pub fn apply_batch(&mut self, batch: &DeltaBatch) -> Result<(), VaultError> {
        let tx = self.conn.transaction()?;
        for delta in &batch.deltas {
            match delta {
                Delta::Upsert(item) => {
                    // Conflict target is the id (deterministic, D13);
                    // connector_id/source_id are immutable under a given id
                    // by construction. A *different* id colliding on
                    // (connector_id, source_id) violates the UNIQUE
                    // constraint and fails the whole batch loudly — that
                    // means id derivation broke, and silently absorbing it
                    // would corrupt identity.
                    tx.execute(
                        "INSERT INTO items (id, connector_id, source_id, kind,
                                            timestamp_ms, ingested_at_ms,
                                            properties, raw_payload, deleted_at_ms, valid_to_ms)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL, ?9)
                         ON CONFLICT (id) DO UPDATE SET
                             kind           = excluded.kind,
                             timestamp_ms   = excluded.timestamp_ms,
                             ingested_at_ms = excluded.ingested_at_ms,
                             properties     = excluded.properties,
                             raw_payload    = excluded.raw_payload,
                             deleted_at_ms  = NULL,
                             valid_to_ms    = excluded.valid_to_ms",
                        (
                            &item.id,
                            &item.connector_id,
                            &item.source_id,
                            serde_json::to_string(&item.kind)
                                .expect("ItemKind serialization is infallible"),
                            item.timestamp.timestamp_millis(),
                            item.ingested_at.timestamp_millis(),
                            item.properties.to_string(),
                            item.raw_payload.as_ref().map(|v| v.to_string()),
                            item.valid_to.as_ref().map(|v| v.timestamp_millis()),
                        ),
                    )?;
                }
                Delta::Tombstone { source_id } => {
                    // Soft delete; unknown source_id is a no-op (tombstone
                    // for something we never ingested — at-least-once
                    // delivery makes that normal).
                    tx.execute(
                        "UPDATE items SET deleted_at_ms = ?1
                         WHERE connector_id = ?2 AND source_id = ?3",
                        (now_ms(), &batch.connector_id, source_id),
                    )?;
                }
            }
        }
        if let Some(cursor) = &batch.cursor {
            tx.execute(
                "INSERT INTO cursors (connector_id, cursor, updated_at_ms)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT (connector_id) DO UPDATE SET
                     cursor = excluded.cursor,
                     updated_at_ms = excluded.updated_at_ms",
                (&batch.connector_id, &cursor.0, now_ms()),
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// The committed resume position for a connector, if any.
    pub fn cursor(&self, connector_id: &str) -> Result<Option<SyncToken>, VaultError> {
        Ok(self
            .conn
            .query_row(
                "SELECT cursor FROM cursors WHERE connector_id = ?1",
                (connector_id,),
                |r| r.get::<_, String>(0),
            )
            .optional()?
            .map(SyncToken))
    }

    /// Live (non-tombstoned) items for a connector, newest event first.
    pub fn items(&self, connector_id: &str) -> Result<Vec<Item>, VaultError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, connector_id, source_id, kind, timestamp_ms,
                    ingested_at_ms, properties, raw_payload, valid_to_ms
             FROM items
             WHERE connector_id = ?1 AND deleted_at_ms IS NULL
             ORDER BY timestamp_ms DESC",
        )?;
        let rows = stmt.query_map((connector_id,), row_to_item)?;
        let mut items = Vec::new();
        for row in rows {
            items.push(row??);
        }
        Ok(items)
    }

    /// Live items across all connectors, newest event first — the viewer's
    /// query (Spec DoD #7).
    pub fn recent_items(&self, limit: u32) -> Result<Vec<Item>, VaultError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, connector_id, source_id, kind, timestamp_ms,
                    ingested_at_ms, properties, raw_payload, valid_to_ms
             FROM items
             WHERE deleted_at_ms IS NULL
             ORDER BY timestamp_ms DESC
             LIMIT ?1",
        )?;
        let rows = stmt.query_map((limit,), row_to_item)?;
        let mut items = Vec::new();
        for row in rows {
            items.push(row??);
        }
        Ok(items)
    }

    /// Retrieve claims with their associated evidence.
    /// This is a cross-source temporal query that joins claims to their evidence via relationships.
    pub fn temporal_claims_with_evidence(&self) -> Result<Vec<(Item, Vec<Item>)>, VaultError> {
        let mut claim_stmt = self.conn.prepare(
            "SELECT id, connector_id, source_id, kind, timestamp_ms,
                    ingested_at_ms, properties, raw_payload, valid_to_ms
             FROM items
             WHERE kind = '\"claim\"' AND deleted_at_ms IS NULL
             ORDER BY timestamp_ms DESC"
        )?;

        let mut claims = Vec::new();
        let rows = claim_stmt.query_map([], row_to_item)?;
        for row in rows {
            claims.push(row??);
        }

        let mut results = Vec::new();
        
        let mut evidence_stmt = self.conn.prepare(
            "SELECT e.id, e.connector_id, e.source_id, e.kind, e.timestamp_ms,
                    e.ingested_at_ms, e.properties, e.raw_payload, e.valid_to_ms
             FROM items rel
             JOIN items e ON json_extract(rel.properties, '$.target') = e.id
             WHERE rel.kind = '\"relationship\"'
               AND rel.deleted_at_ms IS NULL
               AND e.deleted_at_ms IS NULL
               AND json_extract(rel.properties, '$.source') = ?1"
        )?;

        for claim in claims {
            let mut evidence = Vec::new();
            let ev_rows = evidence_stmt.query_map((&claim.id,), row_to_item)?;
            for row in ev_rows {
                evidence.push(row??);
            }
            results.push((claim, evidence));
        }

        Ok(results)
    }

    /// Retrieve the revision history for a specific item.
    pub fn item_revisions(&self, item_id: &str) -> Result<Vec<ItemRevision>, VaultError> {
        let mut stmt = self.conn.prepare(
            "SELECT revision_id, item_id, properties, replaced_at_ms
             FROM item_revisions
             WHERE item_id = ?1
             ORDER BY replaced_at_ms DESC"
        )?;

        let rows = stmt.query_map((item_id,), |row| {
            let rev_id: i64 = row.get(0)?;
            let i_id: String = row.get(1)?;
            let props: String = row.get(2)?;
            let replaced_at_ms: i64 = row.get(3)?;
            Ok((rev_id, i_id, props, replaced_at_ms))
        })?;

        let mut results = Vec::new();
        for row in rows {
            let (revision_id, item_id, properties_str, replaced_at_ms) = row?;
            let properties = serde_json::from_str(&properties_str)
                .unwrap_or_else(|_| serde_json::json!({ "error": "unparsable properties" }));
            
            results.push(ItemRevision {
                revision_id,
                item_id,
                properties,
                replaced_at: ms_to_dt(replaced_at_ms).unwrap_or_else(Utc::now),
            });
        }

        Ok(results)
    }

    /// Total live items across all connectors.
    pub fn item_count(&self) -> Result<i64, VaultError> {
        Ok(self.conn.query_row(
            "SELECT count(*) FROM items WHERE deleted_at_ms IS NULL",
            [],
            |r| r.get(0),
        )?)
    }

    /// Small KV surface for vault bookkeeping (schema version, ceremony
    /// acknowledgement flag, …).
    pub fn put_meta(&self, key: &str, value: &str) -> Result<(), VaultError> {
        self.conn.execute(
            "INSERT INTO vault_meta (key, value) VALUES (?1, ?2)
             ON CONFLICT (key) DO UPDATE SET value = excluded.value",
            (key, value),
        )?;
        Ok(())
    }

    pub fn get_meta(&self, key: &str) -> Result<Option<String>, VaultError> {
        Ok(self
            .conn
            .query_row("SELECT value FROM vault_meta WHERE key = ?1", (key,), |r| r.get(0))
            .optional()?)
    }
}

/// Rotate the vault's DEK (D12). Requires the user's recovery key — the
/// recovery wrapper can only be rebuilt by holding it. Sequence: stage new
/// blobs → `PRAGMA rekey` → promote blobs. Each crash window is covered:
/// before rekey, the old key still opens everything (staged debris is
/// discarded later); after rekey, [`unlock_vault`]'s self-heal path
/// promotes the staged blobs.
pub fn rotate_dek<S: KekStore>(
    svc: &KeyService<S>,
    vault: &Vault,
    recovery_input: &str,
) -> Result<Dek, VaultError> {
    let new_dek = svc.stage_rotation(recovery_input)?;
    if let Err(e) = vault.rekey(&new_dek) {
        svc.discard_staged();
        return Err(e);
    }
    svc.commit_rotation()?;
    Ok(new_dek)
}

/// The cold-start open: silent keychain unlock, plus self-healing for a
/// rotation that crashed between rekey and commit. On a healthy open, any
/// stale staged blobs (pre-rekey crash debris wrapping a DEK the database
/// never adopted) are discarded.
pub fn unlock_vault<S: KekStore>(
    svc: &KeyService<S>,
    db_path: &Path,
) -> Result<(Vault, Dek), VaultError> {
    let dek = svc.unlock()?;
    match Vault::open(db_path, &dek) {
        Ok(vault) => {
            svc.discard_staged();
            Ok((vault, dek))
        }
        Err(VaultError::WrongKeyOrCorrupt) if svc.has_staged() => {
            // Rotation crashed after the DB adopted the new DEK: the
            // primary blob holds the old key, the staged blob the new one.
            let staged = svc
                .unlock_staged()?
                .expect("has_staged() checked above");
            let vault = Vault::open(db_path, &staged)?;
            svc.commit_rotation()?;
            Ok((vault, staged))
        }
        Err(e) => Err(e),
    }
}

fn apply_key_pragma(conn: &Connection, pragma: &str, dek: &Dek) -> Result<(), VaultError> {
    debug_assert!(pragma == "key" || pragma == "rekey");
    // Raw-key form. The hex and the composed SQL both hold key material:
    // both are Zeroizing and die at the end of this function.
    let hex = Zeroizing::new(hexfmt::encode(dek.bytes()));
    let sql = Zeroizing::new(format!("PRAGMA {pragma} = \"x'{}'\";", hex.as_str()));
    conn.execute_batch(&sql).map_err(map_notadb)?;
    Ok(())
}

type RowResult = Result<Item, VaultError>;

fn row_to_item(row: &rusqlite::Row<'_>) -> rusqlite::Result<RowResult> {
    let id: String = row.get(0)?;
    let kind_json: String = row.get(3)?;
    let timestamp_ms: i64 = row.get(4)?;
    let ingested_at_ms: i64 = row.get(5)?;
    let properties: String = row.get(6)?;
    let raw_payload: Option<String> = row.get(7)?;
    let valid_to_ms: Option<i64> = row.get(8)?;

    let parse = || -> Result<Item, String> {
        let kind: ItemKind =
            serde_json::from_str(&kind_json).map_err(|e| format!("kind: {e}"))?;
        Ok(Item {
            id: id.clone(),
            connector_id: row.get(1).map_err(|e| e.to_string())?,
            source_id: row.get(2).map_err(|e| e.to_string())?,
            kind,
            timestamp: ms_to_dt(timestamp_ms).ok_or("timestamp out of range")?,
            ingested_at: ms_to_dt(ingested_at_ms).ok_or("ingested_at out of range")?,
            properties: serde_json::from_str(&properties)
                .map_err(|e| format!("properties: {e}"))?,
            raw_payload: raw_payload
                .as_deref()
                .map(serde_json::from_str)
                .transpose()
                .map_err(|e| format!("raw_payload: {e}"))?,
            valid_to: valid_to_ms.and_then(ms_to_dt),
        })
    };
    Ok(parse().map_err(|reason| VaultError::CorruptRow { id, reason }))
}

fn ms_to_dt(ms: i64) -> Option<DateTime<Utc>> {
    DateTime::from_timestamp_millis(ms)
}

fn now_ms() -> i64 {
    Utc::now().timestamp_millis()
}

fn map_notadb(e: rusqlite::Error) -> VaultError {
    match &e {
        rusqlite::Error::SqliteFailure(f, _)
            if f.code == rusqlite::ErrorCode::NotADatabase =>
        {
            VaultError::WrongKeyOrCorrupt
        }
        _ => VaultError::Sqlite(e),
    }
}

/// D9: vault files are owner-only. Covers the main DB and, when present,
/// WAL/journal sidecars (their *contents* are sqlcipher-encrypted; this is
/// belt and braces).
fn restrict_permissions(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for candidate in [
            path.to_path_buf(),
            path.with_extension("db-wal"),
            path.with_extension("db-shm"),
            path.with_extension("db-journal"),
        ] {
            if candidate.exists() {
                std::fs::set_permissions(&candidate, std::fs::Permissions::from_mode(0o600))?;
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::{KeyService, MemoryKekStore};
    use chrono::TimeZone;
    use serde_json::json;
    use std::io::Read;

    fn provision(dir: &Path) -> Dek {
        let svc = KeyService::new(MemoryKekStore::default(), dir);
        let (dek, _recovery) = svc.provision().unwrap();
        dek
    }

    fn ts(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    fn event(source_id: &str, version: u32) -> Item {
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

    // ---- T2.3 (carried forward) ---------------------------------------

    #[test]
    fn write_close_reopen_read() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("vault.db");
        let dek = provision(dir.path());

        {
            let vault = Vault::open(&db, &dek).unwrap();
            vault.put_meta("hello", "world").unwrap();
        } // connection dropped/closed

        let vault = Vault::open(&db, &dek).unwrap();
        assert_eq!(vault.get_meta("hello").unwrap().as_deref(), Some("world"));
        assert_eq!(vault.get_meta("schema_version").unwrap().as_deref(), Some("1"));
    }

    #[test]
    fn wrong_dek_fails_closed() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("vault.db");
        let dek = provision(dir.path());
        Vault::open(&db, &dek).unwrap();

        let other_dir = tempfile::tempdir().unwrap();
        let wrong = provision(other_dir.path());
        assert!(matches!(Vault::open(&db, &wrong), Err(VaultError::WrongKeyOrCorrupt)));
    }

    #[test]
    fn db_file_on_disk_is_not_plaintext_sqlite() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("vault.db");
        let dek = provision(dir.path());
        let vault = Vault::open(&db, &dek).unwrap();
        vault.put_meta("k", "v").unwrap();
        drop(vault);

        let mut header = [0u8; 16];
        std::fs::File::open(&db).unwrap().read_exact(&mut header).unwrap();
        assert_ne!(&header, b"SQLite format 3\0");
    }

    #[test]
    fn plaintext_db_at_vault_path_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("vault.db");
        rusqlite::Connection::open(&db)
            .unwrap()
            .execute_batch("CREATE TABLE t (x); INSERT INTO t VALUES (1);")
            .unwrap();

        let dek = provision(dir.path());
        assert!(matches!(Vault::open(&db, &dek), Err(VaultError::WrongKeyOrCorrupt)));
    }

    #[cfg(unix)]
    #[test]
    fn vault_file_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("vault.db");
        let dek = provision(dir.path());
        Vault::open(&db, &dek).unwrap();
        let mode = std::fs::metadata(&db).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn temp_store_is_memory() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("vault.db");
        let dek = provision(dir.path());
        let vault = Vault::open(&db, &dek).unwrap();
        let v: i64 = vault.conn.query_row("PRAGMA temp_store", [], |r| r.get(0)).unwrap();
        assert_eq!(v, 2, "temp_store must be MEMORY (2)");
    }

    // ---- T2.4: schema + identity --------------------------------------

    #[test]
    fn re_ingesting_same_source_record_updates_in_place() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("vault.db");
        let dek = provision(dir.path());
        let mut vault = Vault::open(&db, &dek).unwrap();

        vault.apply_batch(&batch(vec![Delta::Upsert(event("evt-1", 1))], None)).unwrap();
        vault.apply_batch(&batch(vec![Delta::Upsert(event("evt-1", 2))], None)).unwrap();

        let items = vault.items("google-calendar").unwrap();
        assert_eq!(items.len(), 1, "same (connector, source) must occupy one row");
        assert_eq!(items[0].properties["version"], json!(2));
        assert_eq!(items[0].id, Item::deterministic_id("google-calendar", "evt-1").to_string());
    }

    #[test]
    fn unique_constraint_rejects_identity_drift() {
        // If id derivation ever broke (different id, same pair), the DB
        // must refuse rather than silently keep both.
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("vault.db");
        let dek = provision(dir.path());
        let mut vault = Vault::open(&db, &dek).unwrap();

        vault.apply_batch(&batch(vec![Delta::Upsert(event("evt-1", 1))], None)).unwrap();

        let mut impostor = event("evt-1", 9);
        impostor.id = "not-the-uuidv5".into();
        let err = vault
            .apply_batch(&batch(vec![Delta::Upsert(impostor)], None))
            .unwrap_err();
        assert!(matches!(err, VaultError::Sqlite(_)), "UNIQUE violation must fail the batch");
        // And the failed batch left nothing behind.
        assert_eq!(vault.item_count().unwrap(), 1);
    }

    #[test]
    fn tombstone_soft_deletes_and_reupsert_revives() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("vault.db");
        let dek = provision(dir.path());
        let mut vault = Vault::open(&db, &dek).unwrap();

        vault.apply_batch(&batch(vec![Delta::Upsert(event("evt-1", 1))], None)).unwrap();
        vault
            .apply_batch(&batch(vec![Delta::Tombstone { source_id: "evt-1".into() }], None))
            .unwrap();
        assert_eq!(vault.items("google-calendar").unwrap().len(), 0);
        assert_eq!(vault.item_count().unwrap(), 0);

        // Source re-creates the record (e.g. meeting un-cancelled).
        vault.apply_batch(&batch(vec![Delta::Upsert(event("evt-1", 3))], None)).unwrap();
        let items = vault.items("google-calendar").unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].properties["version"], json!(3));

        // Tombstone for something never ingested: harmless no-op.
        vault
            .apply_batch(&batch(vec![Delta::Tombstone { source_id: "ghost".into() }], None))
            .unwrap();
    }

    #[test]
    fn stored_items_round_trip_through_domain_types() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("vault.db");
        let dek = provision(dir.path());
        let mut vault = Vault::open(&db, &dek).unwrap();

        let mut item = event("evt-rt", 1);
        item.kind = ItemKind::Other("custom".into());
        item.raw_payload = Some(json!({"etag": "xyz", "big": 9_007_199_254_740_993i64}));
        item.ingested_at = Utc.timestamp_millis_opt(1_720_000_000_123).unwrap();
        vault.apply_batch(&batch(vec![Delta::Upsert(item.clone())], None)).unwrap();

        let back = vault.items("google-calendar").unwrap().remove(0);
        assert_eq!(back, item);
    }

    // ---- T2.5: atomic batch-apply + crash recovery ---------------------

    #[test]
    fn crash_mid_batch_resumes_from_last_committed_cursor_without_duplicates() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("vault.db");
        let dek = provision(dir.path());

        let batch1 = batch(
            vec![Delta::Upsert(event("evt-1", 1)), Delta::Upsert(event("evt-2", 1))],
            Some("cursor-after-batch-1"),
        );
        let batch2 = batch(
            vec![Delta::Upsert(event("evt-2", 2)), Delta::Upsert(event("evt-3", 1))],
            Some("cursor-after-batch-2"),
        );

        // Session 1: batch 1 commits; batch 2 is interrupted mid-apply.
        {
            let mut vault = Vault::open(&db, &dek).unwrap();
            vault.apply_batch(&batch1).unwrap();

            // Simulate the crash: perform batch 2's first delta inside an
            // explicit transaction, then "die" before commit (drop = the
            // rollback a real crash gets from sqlite journaling).
            let tx = vault.conn.transaction().unwrap();
            tx.execute(
                "INSERT INTO items (id, connector_id, source_id, kind, timestamp_ms,
                                    ingested_at_ms, properties, raw_payload, deleted_at_ms)
                 VALUES (?1, 'google-calendar', 'evt-3', '\"event\"', 0, 0, '{}', NULL, NULL)",
                (Item::deterministic_id("google-calendar", "evt-3").to_string(),),
            )
            .unwrap();
            drop(tx); // <- crash. No commit. Vault connection dropped below.
        }

        // Session 2 (restart): resume position is exactly batch 1's cursor;
        // none of batch 2's partial work is visible.
        let mut vault = Vault::open(&db, &dek).unwrap();
        assert_eq!(
            vault.cursor("google-calendar").unwrap(),
            Some(SyncToken("cursor-after-batch-1".into())),
            "crash must roll back to the last committed cursor"
        );
        assert_eq!(vault.item_count().unwrap(), 2, "partial batch must leave no rows");

        // The connector re-delivers from the committed cursor. At-least-once
        // delivery means overlap (evt-2 again) — idempotent by D13 identity.
        vault.apply_batch(&batch2).unwrap();

        let items = vault.items("google-calendar").unwrap();
        assert_eq!(items.len(), 3, "evt-1, evt-2, evt-3 — no duplicates");
        let mut ids: Vec<_> = items.iter().map(|i| i.id.clone()).collect();
        ids.sort();
        let mut expected: Vec<_> = ["evt-1", "evt-2", "evt-3"]
            .iter()
            .map(|s| Item::deterministic_id("google-calendar", s).to_string())
            .collect();
        expected.sort();
        assert_eq!(ids, expected, "every row keys to its deterministic UUIDv5");

        let evt2 = items.iter().find(|i| i.source_id == "evt-2").unwrap();
        assert_eq!(evt2.properties["version"], json!(2), "replayed overlap: latest write wins");
        assert_eq!(
            vault.cursor("google-calendar").unwrap(),
            Some(SyncToken("cursor-after-batch-2".into()))
        );
    }

    #[test]
    fn cursor_only_advances_when_its_batch_commits() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("vault.db");
        let dek = provision(dir.path());
        let mut vault = Vault::open(&db, &dek).unwrap();

        // A failing batch (identity drift from unique_constraint test)
        // must not advance the cursor either.
        vault.apply_batch(&batch(vec![Delta::Upsert(event("evt-1", 1))], Some("c1"))).unwrap();
        let mut impostor = event("evt-1", 2);
        impostor.id = "drifted".into();
        assert!(vault
            .apply_batch(&batch(vec![Delta::Upsert(impostor)], Some("c2")))
            .is_err());
        assert_eq!(vault.cursor("google-calendar").unwrap(), Some(SyncToken("c1".into())));
    }

    // ---- D12: DEK rotation ---------------------------------------------

    #[test]
    fn rotate_dek_preserves_data_and_invalidates_old_key() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("vault.db");
        let svc = KeyService::new(MemoryKekStore::default(), dir.path());
        let (old_dek, recovery) = svc.provision().unwrap();

        let mut vault = Vault::open(&db, &old_dek).unwrap();
        vault.apply_batch(&batch(vec![Delta::Upsert(event("evt-1", 1))], Some("c1"))).unwrap();

        let new_dek = rotate_dek(&svc, &vault, &recovery.display()).unwrap();
        drop(vault);

        // Old DEK is dead.
        assert!(matches!(Vault::open(&db, &old_dek), Err(VaultError::WrongKeyOrCorrupt)));
        // Silent unlock path yields the new DEK and the data.
        let (vault, unlocked) = unlock_vault(&svc, &db).unwrap();
        assert_eq!(unlocked.bytes(), new_dek.bytes());
        assert_eq!(vault.items("google-calendar").unwrap().len(), 1);
        // The user's recovery key survived rotation unchanged.
        svc.verify_recovery(&recovery.display()).unwrap();
    }

    #[test]
    fn rotation_crash_after_rekey_self_heals_on_next_unlock() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("vault.db");
        let svc = KeyService::new(MemoryKekStore::default(), dir.path());
        let (_dek, recovery) = svc.provision().unwrap();

        {
            let (mut vault, _) = unlock_vault(&svc, &db).unwrap();
            vault.apply_batch(&batch(vec![Delta::Upsert(event("evt-1", 1))], None)).unwrap();
            // Crash window: blobs staged, DB rekeyed, commit never happens.
            let new_dek = svc.stage_rotation(&recovery.display()).unwrap();
            vault.rekey(&new_dek).unwrap();
        } // process dies before commit_rotation()

        // Next cold start: primary blob no longer opens the DB; the staged
        // blob does, and gets promoted.
        let (vault, _dek) = unlock_vault(&svc, &db).unwrap();
        assert_eq!(vault.items("google-calendar").unwrap().len(), 1);
        assert!(!svc.has_staged(), "staged blobs promoted to primary");
        // Recovery key still valid against the promoted recovery blob.
        svc.verify_recovery(&recovery.display()).unwrap();
        drop(vault);
        // And the next unlock is fully ordinary.
        unlock_vault(&svc, &db).unwrap();
    }

    #[test]
    fn rotation_crash_before_rekey_leaves_old_key_authoritative() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("vault.db");
        let svc = KeyService::new(MemoryKekStore::default(), dir.path());
        let (_dek, recovery) = svc.provision().unwrap();

        {
            let (mut vault, _) = unlock_vault(&svc, &db).unwrap();
            vault.apply_batch(&batch(vec![Delta::Upsert(event("evt-1", 1))], None)).unwrap();
            // Crash window: blobs staged but the DB was never rekeyed.
            let _staged = svc.stage_rotation(&recovery.display()).unwrap();
        } // process dies before rekey

        // Old key still opens; stale staged debris is discarded.
        let (vault, _) = unlock_vault(&svc, &db).unwrap();
        assert_eq!(vault.items("google-calendar").unwrap().len(), 1);
        assert!(!svc.has_staged(), "stale staged blobs must be discarded");
    }

    #[test]
    fn rotation_with_wrong_recovery_key_changes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("vault.db");
        let svc = KeyService::new(MemoryKekStore::default(), dir.path());
        let (dek, _recovery) = svc.provision().unwrap();
        let vault = Vault::open(&db, &dek).unwrap();

        assert!(rotate_dek(&svc, &vault, &"0".repeat(64)).is_err());
        drop(vault);
        // Old key remains authoritative; nothing staged.
        assert!(!svc.has_staged());
        unlock_vault(&svc, &db).unwrap();
    }
}
