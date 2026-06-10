//! LifeGraph domain types now live in `wkyt-core` (workspace crate); this
//! module re-exports them so existing `crate::lifegraph::*` paths keep
//! working, and hosts the debug-only MockConnector.

pub use wkyt_core::{
    Connector, Delta, DeltaBatch, DeltaStream, Item, ItemKind, SyncError, SyncToken,
    WKYT_NAMESPACE,
};

// --- Mock Implementation ---

#[cfg(debug_assertions)]
pub struct MockConnector {
    pub id: String,
}

#[cfg(debug_assertions)]
#[async_trait::async_trait]
impl Connector for MockConnector {
    fn id(&self) -> &str {
        &self.id
    }

    async fn init(&self) -> Result<(), SyncError> {
        println!("MockConnector[{}] initialized.", self.id);
        Ok(())
    }

    fn sync(&self, cursor: Option<SyncToken>) -> DeltaStream<'_> {
        match cursor {
            // Incremental from a known position: nothing new.
            Some(_) => Box::pin(futures_util::stream::iter(vec![])),
            // Full sync: one bounded batch with one item.
            None => {
                let item = Item::new(
                    "mock_msg_1",
                    &self.id,
                    ItemKind::Message,
                    chrono::Utc::now(),
                    serde_json::json!({
                        "subject": "Hello World",
                        "body": "This is a test message from the mock connector."
                    }),
                );
                let batch = DeltaBatch {
                    connector_id: self.id.clone(),
                    deltas: vec![Delta::Upsert(item)],
                    cursor: Some(SyncToken("mock-cursor-1".into())),
                };
                Box::pin(futures_util::stream::iter(vec![Ok(batch)]))
            }
        }
    }
}

#[cfg(test)]
#[cfg(debug_assertions)]
mod tests {
    use super::*;
    use futures_util::StreamExt;

    fn connector() -> MockConnector {
        MockConnector {
            id: "test-conn".to_string(),
        }
    }

    #[test]
    fn test_mock_connector_id() {
        assert_eq!(connector().id(), "test-conn");
    }

    #[test]
    fn test_mock_connector_init() {
        let result = tauri::async_runtime::block_on(connector().init());
        assert!(result.is_ok());
    }

    #[test]
    fn test_mock_connector_full_sync_streams_one_batch() {
        let conn = connector();
        let batches: Vec<_> =
            tauri::async_runtime::block_on(async { conn.sync(None).collect().await });
        assert_eq!(batches.len(), 1);
        let batch = batches[0].as_ref().unwrap();
        assert_eq!(batch.connector_id, "test-conn");
        assert_eq!(batch.cursor, Some(SyncToken("mock-cursor-1".into())));
        match &batch.deltas[..] {
            [Delta::Upsert(item)] => {
                assert_eq!(item.source_id, "mock_msg_1");
                assert_eq!(item.connector_id, "test-conn");
                assert_eq!(item.kind, ItemKind::Message);
                assert_eq!(item.properties["subject"], "Hello World");
                // D13: deterministic identity.
                assert_eq!(
                    item.id,
                    Item::deterministic_id("test-conn", "mock_msg_1").to_string()
                );
            }
            other => panic!("expected one upsert, got {other:?}"),
        }
    }

    #[test]
    fn test_mock_connector_incremental_sync_is_empty() {
        let conn = connector();
        let batches: Vec<_> = tauri::async_runtime::block_on(async {
            conn.sync(Some(SyncToken("mock-cursor-1".into())))
                .collect()
                .await
        });
        assert!(batches.is_empty());
    }
}
