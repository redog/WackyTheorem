use std::time::Duration;

type Source = Box<dyn std::error::Error + Send + Sync>;

/// Why a sync step failed, and what the orchestrator should do about it.
/// The orchestrator dispatches on the variant, never on string contents.
#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    /// Transient (network, 5xx, rate limit). Retry with backoff, honoring
    /// `retry_after` if the source provided one.
    #[error("retryable: {source}")]
    Retryable {
        #[source]
        source: Source,
        retry_after: Option<Duration>,
    },

    /// Credentials invalid, revoked, or missing scope. Pause the connector
    /// and surface re-authentication UI. Never retried blindly.
    #[error("authentication required: {reason}")]
    AuthRequired { reason: String },

    /// The sync token is no longer honored by the source (e.g. Google
    /// Calendar 410 GONE). Discard the stored cursor and schedule a full
    /// resync (`sync(None)`).
    #[error("sync token expired; full resync required")]
    ResyncRequired,

    /// Non-recoverable logic/config error. Disable the connector until
    /// operator action; do not retry.
    #[error("fatal: {source}")]
    Fatal {
        #[source]
        source: Source,
    },
}
