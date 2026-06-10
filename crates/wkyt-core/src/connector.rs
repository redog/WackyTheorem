use crate::delta::{DeltaBatch, SyncToken};
use crate::error::SyncError;
use futures_core::Stream;
use std::pin::Pin;

/// A bounded-memory stream of delta batches. Defined over `futures_core`
/// only, so implementing crates choose their own combinator library.
pub type DeltaStream<'a> = Pin<Box<dyn Stream<Item = Result<DeltaBatch, SyncError>> + Send + 'a>>;

/// The contract every data connector fulfills (revised per the M0 review;
/// supersedes the old init/full_sync/incremental_sync shape).
#[async_trait::async_trait]
pub trait Connector: Send + Sync {
    /// Stable identifier; feeds the UUIDv5 item identity (D13).
    /// Must not contain U+001F.
    fn id(&self) -> &str;

    /// Idempotent. Called before every sync; cheap if already initialized.
    async fn init(&self) -> Result<(), SyncError>;

    /// Stream batches of deltas starting from `cursor`.
    ///
    /// `None` ⇒ full sync; `Some` ⇒ incremental from that position. The two
    /// legacy methods collapsed into this one so the recovery path is
    /// trivial: yielding `Err(SyncError::ResyncRequired)` mid-stream tells
    /// the orchestrator to abandon the stream, discard the stored cursor,
    /// and restart with `sync(None)`.
    fn sync(&self, cursor: Option<SyncToken>) -> DeltaStream<'_>;
}
