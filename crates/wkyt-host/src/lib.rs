//! Connector orchestrator (M3/M4).
//!
//! One pipeline run = connector stream → bus → vault:
//!
//! ```text
//! connector.sync(cursor) ──publish──> Bus ──next──> apply_batch ──commit──> ack
//! ```
//!
//! The consumer acks a delivery **only after** [`Vault::apply_batch`]'s
//! transaction commits — before that, a crash leaves the cursor
//! un-advanced and the batch is simply re-delivered by the next run
//! (idempotent by D13).
//!
//! Error policy by taxonomy (`SyncError`):
//! - `ResyncRequired` mid-stream → the stored cursor is abandoned and the
//!   sync restarts from `None`, once per run.
//! - `Retryable` → surfaced to the caller; the caller's schedule (the app
//!   polls) naturally retries from the committed cursor. In-run backoff
//!   policy arrives with the orchestrator maturation work, as does the
//!   per-connector lease (callers must not run one connector concurrently).
//! - `AuthRequired` / `Fatal` → surfaced; the connector needs operator or
//!   re-auth attention.

use futures_util::StreamExt;
use std::sync::{Arc, Mutex};
use wkyt_broker::{in_process, BusError, BusPublisher, BusSubscriber};
use wkyt_core::{Connector, SyncError, SyncToken};
use wkyt_vault::{Vault, VaultError};

#[derive(Debug, thiserror::Error)]
pub enum HostError {
    #[error("connector: {0}")]
    Sync(#[from] SyncError),
    #[error("vault: {0}")]
    Vault(#[from] VaultError),
    #[error("bus: {0}")]
    Bus(#[from] BusError),
    #[error("consumer task panicked or was cancelled: {0}")]
    Join(String),
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct PipelineStats {
    pub batches_applied: u64,
    pub deltas_applied: u64,
}

/// Run one full pipeline pass for `connector`: resume from the vault's
/// committed cursor, stream batches over a bounded bus, apply each to the
/// vault, ack after commit. Returns once the stream is drained and every
/// in-flight batch is applied (or the first error).
pub async fn run_pipeline_once<C: Connector + ?Sized>(
    connector: &C,
    vault: Arc<Mutex<Vault>>,
) -> Result<PipelineStats, HostError> {
    connector.init().await?;
    let starting_cursor = vault.lock().unwrap().cursor(connector.id())?;

    let (publisher, subscriber) = in_process(8);
    let consumer = tokio::spawn(consume(subscriber, Arc::clone(&vault)));

    let pump_result = pump(connector, &publisher, starting_cursor).await;

    // Closing the publisher lets the consumer drain and finish.
    drop(publisher);
    let stats = consumer.await.map_err(|e| HostError::Join(e.to_string()))??;

    // Consumer success with a pump failure still reports the pump failure:
    // partial progress is durable (cursor committed per batch), and the
    // caller's next run resumes from it.
    pump_result?;
    Ok(stats)
}

/// Drain the connector's stream into the bus, honoring the error taxonomy:
/// `ResyncRequired` triggers exactly one restart from `None`; a second
/// `ResyncRequired` (a full sync demanding a full resync) surfaces as the
/// error it is rather than looping.
async fn pump<C: Connector + ?Sized>(
    connector: &C,
    publisher: &impl BusPublisher,
    starting_cursor: Option<SyncToken>,
) -> Result<(), HostError> {
    match drain_stream(connector, publisher, starting_cursor).await {
        Err(HostError::Sync(SyncError::ResyncRequired)) => {
            drain_stream(connector, publisher, None).await
        }
        other => other,
    }
}

async fn drain_stream<C: Connector + ?Sized>(
    connector: &C,
    publisher: &impl BusPublisher,
    cursor: Option<SyncToken>,
) -> Result<(), HostError> {
    let mut stream = connector.sync(cursor);
    while let Some(next) = stream.next().await {
        publisher.publish(next?).await?;
    }
    Ok(())
}

/// Pull deliveries, apply each batch in its own vault transaction, and ack
/// strictly after commit.
async fn consume<S: BusSubscriber>(
    mut subscriber: S,
    vault: Arc<Mutex<Vault>>,
) -> Result<PipelineStats, HostError> {
    let mut stats = PipelineStats::default();
    while let Some(delivery) = subscriber.next().await {
        let (batch, ack) = delivery.into_parts();
        let delta_count = batch.deltas.len() as u64;
        let v = Arc::clone(&vault);
        // apply_batch is blocking (sqlite); keep it off the async threads.
        tokio::task::spawn_blocking(move || v.lock().unwrap().apply_batch(&batch))
            .await
            .map_err(|e| HostError::Join(e.to_string()))??;
        // The transaction is committed — and only now is it safe to ack.
        ack.ack();
        stats.batches_applied += 1;
        stats.deltas_applied += delta_count;
    }
    Ok(stats)
}
