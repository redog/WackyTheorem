use std::path::PathBuf;
use std::error::Error;
use duckdb::{Connection, params};
use crate::lifegraph::Item;

/// Trait for storage implementations.
/// Must be Send + Sync to be used across threads (e.g. in Tauri commands/tasks).
pub trait Storage: Send + Sync {
    fn init(&self) -> Result<(), Box<dyn Error + Send + Sync>>;
    fn save_item(&self, item: &Item) -> Result<(), Box<dyn Error + Send + Sync>>;
    fn save_items(&self, items: &[Item]) -> Result<(), Box<dyn Error + Send + Sync>>;
}

pub struct DuckDbStorage {
    path: PathBuf,
}

impl DuckDbStorage {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    fn connect(&self) -> Result<Connection, Box<dyn Error + Send + Sync>> {
        Connection::open(&self.path)
            .map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)
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
        ).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        Ok(())
    }

    fn save_item(&self, item: &Item) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.save_items(std::slice::from_ref(item))
    }

    fn save_items(&self, items: &[Item]) -> Result<(), Box<dyn Error + Send + Sync>> {
        let mut conn = self.connect()?;
        let tx = conn.transaction()
            .map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;

        {
            let mut stmt = tx.prepare(
                "INSERT OR REPLACE INTO items (id, source_id, connector_id, kind, timestamp, ingested_at, properties, raw_payload)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
            ).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;

            for item in items {
                // Serialize complex types to JSON strings
                let kind_json = serde_json::to_string(&item.kind)
                    .map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
                let properties_str = item.properties.to_string();
                let raw_payload_str = item.raw_payload.as_ref()
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "null".to_string());

                stmt.execute(params![
                    item.id,
                    item.source_id,
                    item.connector_id,
                    kind_json,
                    item.timestamp.to_rfc3339(),
                    item.ingested_at.to_rfc3339(),
                    properties_str,
                    raw_payload_str
                ]).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
            }
        }

        tx.commit().map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lifegraph::{ItemKind, Item};
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
        let mut stmt = conn.prepare("SELECT id, kind, properties FROM items WHERE id = ?").unwrap();
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

    #[test]
    fn test_save_items_bulk() {
        let db_path = PathBuf::from("test_bulk.db");
        if db_path.exists() {
            fs::remove_file(&db_path).unwrap();
        }

        let storage = DuckDbStorage::new(db_path.clone());
        storage.init().unwrap();

        let items = vec![
            Item::new("src_1", "conn_1", ItemKind::Message, json!({"msg": 1})),
            Item::new("src_2", "conn_1", ItemKind::Message, json!({"msg": 2})),
            Item::new("src_3", "conn_1", ItemKind::Message, json!({"msg": 3})),
        ];

        storage.save_items(&items).expect("Failed bulk save");

        // Verify count
        let conn = Connection::open(&db_path).unwrap();
        let count: i64 = conn.query_row("SELECT count(*) FROM items", [], |row| row.get(0)).unwrap();
        assert_eq!(count, 3);

        // Test Replace
        let mut item_mod = items[0].clone();
        item_mod.properties = json!({"msg": "updated"});
        storage.save_items(&[item_mod.clone()]).expect("Failed replace save");

        let props: String = conn.query_row("SELECT properties FROM items WHERE id = ?", params![item_mod.id], |row| row.get(0)).unwrap();
        assert_eq!(props, "{\"msg\":\"updated\"}");

        let _ = fs::remove_file(&db_path);
    }
}
