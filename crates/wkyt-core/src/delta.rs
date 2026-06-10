use crate::item::Item;
use serde::{Deserialize, Serialize};

/// Opaque, connector-defined sync position (e.g. Google's `syncToken`).
/// The orchestrator never interprets it — it persists the token atomically
/// with the batch it covers and hands it back verbatim on the next sync.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncToken(pub String);

/// One change from a source. Tombstones are how deletions reach the vault.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Delta {
    Upsert(Item),
    Tombstone { source_id: String },
}

/// The unit of transfer AND the unit of durability: the vault applies
/// `deltas` and persists `cursor` in a single transaction. Bounded size
/// (connectors should target a few hundred deltas per batch) is what makes
/// a full sync O(batch) memory instead of O(dataset).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeltaBatch {
    pub connector_id: String,
    pub deltas: Vec<Delta>,
    /// Resume position valid *after* this batch is applied. `None` means
    /// "checkpoint unchanged" (keep the previously committed cursor).
    pub cursor: Option<SyncToken>,
}
