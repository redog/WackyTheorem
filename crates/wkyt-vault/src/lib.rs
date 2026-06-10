//! Encrypted SQLite vault (Milestone 2).
//!
//! Will own: the sqlcipher database (D1/D10), the KEK/DEK key service and
//! recovery wrappers (D12), the `items`/`cursors` schema with
//! `UNIQUE(connector_id, source_id)` (D13), and transactional batch-apply —
//! deltas and the sync cursor committed as one unit (D11).
