use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::error::Error;
use chrono::{DateTime, Utc};

/// The core entity types in the LifeGraph ontology.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    Person,
    Organization,
    Transaction,
    Message,
    File,
    Metric,
    Event,
    Other(String),
}

/// A normalized unit of data within the LifeGraph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Item {
    /// Universally unique ID for this item in the Vault
    pub id: String,
    
    /// The ID of the item in the source system (e.g., Gmail Message ID)
    pub source_id: String,
    
    /// The ID of the connector that produced this item
    pub connector_id: String,
    
    /// The type of data
    pub kind: ItemKind,
    
    /// When this item was created or occurred in reality
    pub timestamp: DateTime<Utc>,
    
    /// When this item was ingested into the vault
    pub ingested_at: DateTime<Utc>,
    
    /// structured metadata specific to the kind
    pub properties: Value,
    
    /// The raw original payload for traceability
    pub raw_payload: Option<Value>,
}

impl Item {
    pub fn new(
        source_id: impl Into<String>,
        connector_id: impl Into<String>,
        kind: ItemKind,
        properties: Value,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            source_id: source_id.into(),
            connector_id: connector_id.into(),
            kind,
            timestamp: Utc::now(),
            ingested_at: Utc::now(),
            properties,
            raw_payload: None,
        }
    }
}

/// The contract that all Data Connectors must fulfill.
#[async_trait::async_trait]
pub trait Connector: Send + Sync {
    fn id(&self) -> &str;
    
    // UPDATED: Error type must be Send + Sync to work in async threads
    async fn init(&self) -> Result<(), Box<dyn Error + Send + Sync>>;
    
    async fn full_sync(&self) -> Result<Vec<Item>, Box<dyn Error + Send + Sync>>;
    
    async fn incremental_sync(&self, since: DateTime<Utc>) -> Result<Vec<Item>, Box<dyn Error + Send + Sync>>;
}

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

    async fn init(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        println!("MockConnector[{}] initialized.", self.id);
        Ok(())
    }

    async fn full_sync(&self) -> Result<Vec<Item>, Box<dyn Error + Send + Sync>> {
        let item = Item::new(
            "mock_msg_1",
            &self.id,
            ItemKind::Message,
            serde_json::json!({
                "subject": "Hello World",
                "body": "This is a test message from the mock connector."
            }),
        );
        Ok(vec![item])
    }

    async fn incremental_sync(&self, _since: DateTime<Utc>) -> Result<Vec<Item>, Box<dyn Error + Send + Sync>> {
        Ok(vec![])
    }
}
