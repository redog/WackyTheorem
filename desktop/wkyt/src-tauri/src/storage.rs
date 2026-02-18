use crate::lifegraph::Item;
use duckdb::{params, Connection};
use std::error::Error;
use std::path::PathBuf;

/// Trait for storage implementations.
/// Must be Send + Sync to be used across threads (e.g. in Tauri commands/tasks).
pub trait Storage: Send + Sync {
    fn init(&self) -> Result<(), Box<dyn Error + Send + Sync>>;
    fn save_item(&self, item: &Item) -> Result<(), Box<dyn Error + Send + Sync>>;
}

pub struct DuckDbStorage {
    path: PathBuf,
}

impl DuckDbStorage {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    fn connect(&self) -> Result<Connection, Box<dyn Error + Send + Sync>> {
        Connection::open(&self.path).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)
    }
}

impl Storage for DuckDbStorage {
    fn init(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        let conn = self.connect()?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS items (
                id TEXT PRIMARY KEY,
                source_id TEXT,
                connector_id TEXT,
                kind TEXT,
                timestamp TEXT,
                ingested_at TEXT,
                properties TEXT,
                raw_payload TEXT
            )",
            [],
        )
        .map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        Ok(())
    }

    fn save_item(&self, item: &Item) -> Result<(), Box<dyn Error + Send + Sync>> {
        let conn = self.connect()?;

        // Serialize complex types to JSON strings
        let kind_json = serde_json::to_string(&item.kind)
            .map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        let properties_str = item.properties.to_string();
        let raw_payload_str = item
            .raw_payload
            .as_ref()
            .map(|v| v.to_string())
            .unwrap_or_else(|| "null".to_string());

        conn.execute(
            "INSERT OR REPLACE INTO items (id, source_id, connector_id, kind, timestamp, ingested_at, properties, raw_payload)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                item.id,
                item.source_id,
                item.connector_id,
                kind_json,
                item.timestamp.to_rfc3339(),
                item.ingested_at.to_rfc3339(),
                properties_str,
                raw_payload_str
            ],
        ).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lifegraph::{Item, ItemKind};
    use serde_json::json;
    use std::fs;

    #[test]
    fn test_duckdb_storage() {
        // Use a temporary file for testing
        let db_path = PathBuf::from("test_lifegraph.db");
        if db_path.exists() {
            fs::remove_file(&db_path).unwrap();
        }

        let storage = DuckDbStorage::new(db_path.clone());

        // 1. Init
        storage.init().expect("Failed to init db");

        // 2. Save Item
        let item = Item::new(
            "test_src_1",
            "test_conn",
            ItemKind::Person,
            json!({"name": "Alice"}),
        );
        storage.save_item(&item).expect("Failed to save item");

        // Verify data (manually query to check)
        let conn = Connection::open(&db_path).unwrap();
        let mut stmt = conn
            .prepare("SELECT id, kind, properties FROM items WHERE id = ?")
            .unwrap();
        let mut rows = stmt.query(params![item.id]).unwrap();

        if let Some(row) = rows.next().unwrap() {
            let id: String = row.get(0).unwrap();
            let kind: String = row.get(1).unwrap();
            let props: String = row.get(2).unwrap();

            assert_eq!(id, item.id);
            assert_eq!(kind, "\"person\"");
            assert_eq!(props, "{\"name\":\"Alice\"}");
        } else {
            panic!("Item not found in DB");
        }

        // Cleanup
        let _ = fs::remove_file(&db_path);
    }
}
