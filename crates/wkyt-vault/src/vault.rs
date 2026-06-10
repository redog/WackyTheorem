//! sqlcipher vault initialization (T2.3).
//!
//! The DEK is applied as a *raw* key (`PRAGMA key = "x'<hex>'"`), not a
//! passphrase: sqlcipher then skips its PBKDF2 derivation, which is both
//! correct (our DEK is already a uniformly random 256-bit key) and avoids
//! pretending a KDF adds anything on top of one.
//!
//! Memory handling around `PRAGMA key` (D12), in order of custody:
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

use crate::hexfmt;
use crate::keys::Dek;
use rusqlite::Connection;
use std::path::Path;
use zeroize::Zeroizing;

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
}

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
        apply_key(&conn, dek)?;

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
            // foreign_keys: schema integrity from day one (T2.4 schema).
            "PRAGMA cipher_memory_security = ON;
             PRAGMA temp_store = MEMORY;
             PRAGMA foreign_keys = ON;
             CREATE TABLE IF NOT EXISTS vault_meta (
                 key   TEXT PRIMARY KEY,
                 value TEXT NOT NULL
             );",
        )
        .map_err(map_notadb)?;

        restrict_permissions(path)?;
        Ok(Self { conn })
    }

    /// Small KV surface for vault bookkeeping (schema version, ceremony
    /// acknowledgement flag, …). The items/cursors schema lands in T2.4.
    pub fn put_meta(&self, key: &str, value: &str) -> Result<(), VaultError> {
        self.conn.execute(
            "INSERT INTO vault_meta (key, value) VALUES (?1, ?2)
             ON CONFLICT (key) DO UPDATE SET value = excluded.value",
            (key, value),
        )?;
        Ok(())
    }

    pub fn get_meta(&self, key: &str) -> Result<Option<String>, VaultError> {
        use rusqlite::OptionalExtension;
        Ok(self
            .conn
            .query_row("SELECT value FROM vault_meta WHERE key = ?1", (key,), |r| r.get(0))
            .optional()?)
    }
}

fn apply_key(conn: &Connection, dek: &Dek) -> Result<(), VaultError> {
    // Raw-key form. The hex and the composed SQL both hold key material:
    // both are Zeroizing and die at the end of this function.
    let hex = Zeroizing::new(hexfmt::encode(dek.bytes()));
    let sql = Zeroizing::new(format!("PRAGMA key = \"x'{}'\";", hex.as_str()));
    conn.execute_batch(&sql)?;
    Ok(())
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
    use std::io::Read;

    fn provision(dir: &Path) -> Dek {
        let svc = KeyService::new(MemoryKekStore::default(), dir);
        let (dek, _recovery) = svc.provision().unwrap();
        dek
    }

    #[test]
    fn write_close_reopen_read() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("vault.db");
        let dek = provision(dir.path());

        {
            let vault = Vault::open(&db, &dek).unwrap();
            vault.put_meta("schema_version", "1").unwrap();
        } // connection dropped/closed

        let vault = Vault::open(&db, &dek).unwrap();
        assert_eq!(vault.get_meta("schema_version").unwrap().as_deref(), Some("1"));
        assert_eq!(vault.get_meta("missing").unwrap(), None);
    }

    #[test]
    fn wrong_dek_fails_closed() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("vault.db");
        let dek = provision(dir.path());
        Vault::open(&db, &dek).unwrap();

        // A different provisioning yields a different DEK.
        let other_dir = tempfile::tempdir().unwrap();
        let wrong = provision(other_dir.path());
        assert!(matches!(
            Vault::open(&db, &wrong),
            Err(VaultError::WrongKeyOrCorrupt)
        ));
    }

    #[test]
    fn db_file_on_disk_is_not_plaintext_sqlite() {
        // Spec DoD #6: `file` must not identify the vault as SQLite.
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("vault.db");
        let dek = provision(dir.path());
        let vault = Vault::open(&db, &dek).unwrap();
        vault.put_meta("k", "v").unwrap();
        drop(vault);

        let mut header = [0u8; 16];
        std::fs::File::open(&db).unwrap().read_exact(&mut header).unwrap();
        assert_ne!(
            &header,
            b"SQLite format 3\0",
            "vault file must not carry a plaintext SQLite header"
        );
    }

    #[test]
    fn plaintext_db_at_vault_path_is_rejected() {
        // A pre-existing UNencrypted database (e.g. the old DuckDB/SQLite
        // file, or an attacker-planted one) must not be silently adopted.
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("vault.db");
        rusqlite::Connection::open(&db)
            .unwrap()
            .execute_batch("CREATE TABLE t (x); INSERT INTO t VALUES (1);")
            .unwrap();

        let dek = provision(dir.path());
        assert!(matches!(
            Vault::open(&db, &dek),
            Err(VaultError::WrongKeyOrCorrupt)
        ));
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
        // D10 follow-up, asserted rather than assumed.
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("vault.db");
        let dek = provision(dir.path());
        let vault = Vault::open(&db, &dek).unwrap();
        let v: i64 = vault
            .conn
            .query_row("PRAGMA temp_store", [], |r| r.get(0))
            .unwrap();
        assert_eq!(v, 2, "temp_store must be MEMORY (2)");
    }
}
