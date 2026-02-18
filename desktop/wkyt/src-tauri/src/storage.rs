use std::path::PathBuf;
use std::error::Error;
use duckdb::{Connection, params};
use crate::lifegraph::{Item, ItemKind};
use serde_json::Value;
use chrono::{DateTime, Utc};

/// Trait for storage implementations.
/// Must be Send + Sync to be used across threads (e.g. in Tauri commands/tasks).
pub trait Storage: Send + Sync {
    fn init(&self) -> Result<(), Box<dyn Error + Send + Sync>>;
    fn save_item(&self, item: &Item) -> Result<(), Box<dyn Error + Send + Sync>>;
    fn save_items(&self, items: &[Item]) -> Result<(), Box<dyn Error + Send + Sync>>;
    fn get_all_items(&self) -> Result<Vec<Item>, Box<dyn Error + Send + Sync>>;
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
            );
            CREATE INDEX IF NOT EXISTS idx_items_timestamp ON items(timestamp);",
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
                    .map(|v| v.to_string());

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

    fn get_all_items(&self) -> Result<Vec<Item>, Box<dyn Error + Send + Sync>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare("SELECT id, source_id, connector_id, kind, timestamp, ingested_at, properties, raw_payload FROM items ORDER BY timestamp DESC")
            .map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;

        let item_iter = stmt.query_map([], |row| {
            let kind_str: String = row.get(3)?;
            let kind: ItemKind = serde_json::from_str(&kind_str).unwrap_or(ItemKind::Other("parse_error".to_string()));

            let timestamp_str: String = row.get(4)?;
            let timestamp = DateTime::parse_from_rfc3339(&timestamp_str)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()); // Fallback or handle error

            let ingested_at_str: String = row.get(5)?;
            let ingested_at = DateTime::parse_from_rfc3339(&ingested_at_str)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now());

            let properties_str: String = row.get(6)?;
            let properties: Value = serde_json::from_str(&properties_str).unwrap_or(Value::Null);

            let raw_payload_str: Option<String> = row.get(7)?;
            let raw_payload = raw_payload_str.and_then(|s| serde_json::from_str(&s).ok());

            Ok(Item {
                id: row.get(0)?,
                source_id: row.get(1)?,
                connector_id: row.get(2)?,
                kind,
                timestamp,
                ingested_at,
                properties,
                raw_payload,
            })
        }).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;

        let mut items = Vec::new();
        for item in item_iter {
            items.push(item.map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?);
        }
        Ok(items)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lifegraph::{ItemKind, Item};
    use serde_json::json;
    use std::fs;
    use chrono::TimeZone;

    #[test]
    fn test_duckdb_storage() {
        // Use a temporary file for testing
        let db_path = PathBuf::from("test_lifegraph.db");
        if db_path.exists() {
            let _ = fs::remove_file(&db_path);
        }

        let storage = DuckDbStorage::new(db_path.clone());

        // 1. Init
        storage.init().expect("Failed to init db");

        // 2. Save Item
        let item = Item {
            id: "test-id-1".to_string(),
            source_id: "src-1".to_string(),
            connector_id: "conn-1".to_string(),
            kind: ItemKind::Person,
            timestamp: Utc.timestamp_opt(1600000000, 0).unwrap(),
            ingested_at: Utc::now(),
            properties: json!({"name": "Alice"}),
            raw_payload: None,
        };
        storage.save_item(&item).expect("Failed to save item");

        // Verify data via get_all_items
        let items = storage.get_all_items().expect("Failed to get items");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, item.id);
        assert_eq!(items[0].kind, ItemKind::Person);
        assert_eq!(items[0].properties, json!({"name": "Alice"}));

        // Cleanup
        let _ = fs::remove_file(&db_path);
    }
}
