//! Delta transport (Milestone 3).
//!
//! Will own the `Bus` trait and its default in-process bounded-channel
//! implementation (D11). The trait keeps the transport swappable: if a
//! future phase needs multi-process ingestion, a broker-backed `Bus` can be
//! added without touching connectors or the vault.
