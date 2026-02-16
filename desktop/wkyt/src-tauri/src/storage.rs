use crate::lifegraph::{Item, ItemKind};
use duckdb::{params, Connection, Result};
use std::sync::Mutex;
use chrono::{DateTime, Utc};
use serde_json::Value;

pub struct Storage {
    conn: Mutex<Connection>,
}

impl Storage {
    pub fn new(path: &str) -> Result<Self> {
        let conn = Connection::open(path)?;

        // Initialize the DB schema
        conn.execute_batch(
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
            CREATE INDEX IF NOT EXISTS idx_items_timestamp ON items(timestamp);
            "
        )?;

        Ok(Storage {
            conn: Mutex::new(conn),
        })
    }

    pub fn add_item(&self, item: &Item) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        let kind_json = serde_json::to_string(&item.kind).unwrap();
        let properties_json = serde_json::to_string(&item.properties).unwrap();
        let raw_payload_json = match &item.raw_payload {
            Some(v) => Some(serde_json::to_string(v).unwrap()),
            None => None,
        };

        conn.execute(
            "INSERT OR REPLACE INTO items (id, source_id, connector_id, kind, timestamp, ingested_at, properties, raw_payload) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                item.id,
                item.source_id,
                item.connector_id,
                kind_json,
                item.timestamp.to_rfc3339(),
                item.ingested_at.to_rfc3339(),
                properties_json,
                raw_payload_json
            ],
        )?;
        Ok(())
    }

    pub fn get_all_items(&self) -> Result<Vec<Item>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT id, source_id, connector_id, kind, timestamp, ingested_at, properties, raw_payload FROM items ORDER BY timestamp DESC")?;

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
        })?;

        let mut items = Vec::new();
        for item in item_iter {
            items.push(item?);
        }
        Ok(items)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lifegraph::ItemKind;
    use chrono::TimeZone;

    #[test]
    fn test_storage_basic() {
        let db_path = "test_wkyt.db";
        // remove file if exists
        let _ = std::fs::remove_file(db_path);

        let storage = Storage::new(db_path).expect("failed to init storage");

        let item = Item {
            id: "test-id-1".to_string(),
            source_id: "src-1".to_string(),
            connector_id: "conn-1".to_string(),
            kind: ItemKind::Message,
            timestamp: Utc.timestamp_opt(1600000000, 0).unwrap(),
            ingested_at: Utc::now(),
            properties: serde_json::json!({"subject": "test"}),
            raw_payload: None,
        };

        storage.add_item(&item).expect("failed to add item");

        let items = storage.get_all_items().expect("failed to get items");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, "test-id-1");
        assert_eq!(items[0].kind, ItemKind::Message);

        // Clean up
        let _ = std::fs::remove_file(db_path);
    }
}
