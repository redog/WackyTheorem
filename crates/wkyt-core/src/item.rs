use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::{uuid, Uuid};

/// Project namespace for UUIDv5 derivation (D13). Generated once at adoption
/// time. NEVER change this value: every item ID in every existing vault is
/// derived from it.
pub const WKYT_NAMESPACE: Uuid = uuid!("bc7bf50f-86a7-4630-a12e-5e612ae91064");

/// ASCII unit separator, used to join `connector_id` and `source_id` before
/// hashing so that ("a|b", "c") and ("a", "b|c") cannot collide. Connector
/// IDs must not contain this byte (enforced nowhere yet — they are short
/// internal identifiers we control; revisit if connector IDs ever become
/// user-supplied).
const ID_SEPARATOR: char = '\u{1F}';

/// The epistemic type of a claim, indicating its provenance and confidence.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum EpistemicType {
    Observation,
    ImportedAssertion,
    Inference,
    Hypothesis,
    GeneratedSuggestion,
    Disagreement,
}

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
    Claim,
    Relationship,
    AgentTrace,
    Other(String),
}

/// A normalized unit of data within the LifeGraph.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Item {
    /// Deterministic UUIDv5 over (connector_id, source_id) — see D13.
    /// The same source record always maps to the same id, which is what
    /// makes vault writes idempotent.
    pub id: String,

    /// The ID of the item in the source system (e.g. a Calendar event ID).
    pub source_id: String,

    /// The ID of the connector that produced this item.
    pub connector_id: String,

    /// The type of data.
    pub kind: ItemKind,

    /// When this item was created or occurred in reality (event time).
    pub timestamp: DateTime<Utc>,

    /// When this item was ingested into the vault.
    pub ingested_at: DateTime<Utc>,

    /// Structured metadata specific to the kind.
    pub properties: Value,

    /// The raw original payload for traceability.
    pub raw_payload: Option<Value>,

    /// When this item ceases to be valid (if applicable).
    pub valid_to: Option<DateTime<Utc>>,
}

impl Item {
    /// Derive the deterministic vault ID for a source record (D13).
    pub fn deterministic_id(connector_id: &str, source_id: &str) -> Uuid {
        Uuid::new_v5(
            &WKYT_NAMESPACE,
            format!("{connector_id}{ID_SEPARATOR}{source_id}").as_bytes(),
        )
    }

    /// `timestamp` is the event time and is deliberately required: a
    /// `Utc::now()` default here silently corrupts the temporal axis that
    /// Phase 2 queries depend on. `ingested_at` defaults to now — that one
    /// genuinely is ingestion time.
    pub fn new(
        source_id: impl Into<String>,
        connector_id: impl Into<String>,
        kind: ItemKind,
        timestamp: DateTime<Utc>,
        properties: Value,
    ) -> Self {
        let source_id = source_id.into();
        let connector_id = connector_id.into();
        let id = Self::deterministic_id(&connector_id, &source_id).to_string();
        Self {
            id,
            source_id,
            connector_id,
            kind,
            timestamp,
            ingested_at: Utc::now(),
            properties,
            raw_payload: None,
            valid_to: None,
        }
    }

    /// Set the temporal validity end time for this item.
    pub fn with_valid_to(mut self, valid_to: DateTime<Utc>) -> Self {
        self.valid_to = Some(valid_to);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn same_source_record_yields_same_id() {
        let ts = Utc::now();
        let a = Item::new("evt-1", "google-calendar", ItemKind::Event, ts, json!({"v": 1}));
        let b = Item::new("evt-1", "google-calendar", ItemKind::Event, ts, json!({"v": 2}));
        assert_eq!(a.id, b.id, "re-ingesting the same source record must reuse the id");
    }

    #[test]
    fn different_source_or_connector_yields_different_id() {
        let ts = Utc::now();
        let a = Item::new("evt-1", "google-calendar", ItemKind::Event, ts, json!({}));
        let b = Item::new("evt-2", "google-calendar", ItemKind::Event, ts, json!({}));
        let c = Item::new("evt-1", "imap", ItemKind::Event, ts, json!({}));
        assert_ne!(a.id, b.id);
        assert_ne!(a.id, c.id);
    }

    #[test]
    fn concatenation_boundaries_cannot_collide() {
        // The D13 example: without a separator, ("a|b", "c") and ("a", "|bc")
        // style splits could hash identically.
        let x = Item::deterministic_id("conn-a", "bsrc");
        let y = Item::deterministic_id("conn-ab", "src");
        assert_ne!(x, y);
    }

    #[test]
    fn id_is_stable_across_releases() {
        // Pin the derivation: if WKYT_NAMESPACE, the separator, or the v5
        // input encoding ever changes, this fails loudly instead of silently
        // orphaning every row in every existing vault.
        assert_eq!(
            Item::deterministic_id("google-calendar", "evt-1").to_string(),
            "3aaae9bc-36b9-540b-9158-3537f91fe703",
        );
    }

    #[test]
    fn event_timestamp_is_preserved_not_defaulted() {
        let ts = DateTime::parse_from_rfc3339("2024-07-04T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let item = Item::new("evt-1", "c", ItemKind::Event, ts, json!({}));
        assert_eq!(item.timestamp, ts);
        assert!(item.ingested_at > ts);
    }
}
