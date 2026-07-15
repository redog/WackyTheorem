//! Core domain types and the connector contract for wkyt.
//!
//! Everything ingestion-related agrees on lives here: the [`Item`] model,
//! the [`Delta`]/[`DeltaBatch`] change representation, the [`SyncError`]
//! taxonomy the orchestrator dispatches on, and the [`Connector`] trait.
//! See `DECISIONS.md` D11–D13 for the rationale behind each shape.

mod connector;
mod delta;
mod error;
mod item;
pub mod proto;
mod capability;

pub use connector::{Connector, DeltaStream};
pub use delta::{Delta, DeltaBatch, SyncToken};
pub use error::SyncError;
pub use item::{EpistemicType, Item, ItemKind, WKYT_NAMESPACE};
pub use proto::CodecError;
pub use capability::{CapabilityManifest, CapabilityInvocation, CapabilityResult};
