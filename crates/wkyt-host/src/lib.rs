//! Connector orchestrator (Milestone 4).
//!
//! Will own: per-connector sync leases (no concurrent syncs of one
//! connector), backoff policy keyed by the `SyncError` taxonomy, the
//! incremental→full resync fallback on `ResyncRequired`, and — in M5 —
//! hosting WASM connectors behind the same `Connector` contract.
