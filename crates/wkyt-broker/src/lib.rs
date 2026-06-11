//! Delta transport (D11): the `Bus` trait pair and its default bounded
//! in-process implementation.
//!
//! Contract recap from D11: the bus is a *dumb pipe with backpressure*.
//! Durability does NOT live here — it lives in the vault, where each
//! batch commits atomically with its cursor, and replay-from-cursor is
//! idempotent (D13). The explicit [`Delivery::ack`] exists so the
//! consumer's "ack only after the vault transaction commits" discipline
//! is visible in the type system today and maps onto a real broker's ack
//! if a multi-process transport ever replaces this one.
//!
//! In-process semantics, stated honestly:
//! - `publish` awaits when the channel is full — that IS the backpressure.
//! - An un-acked `Delivery` dropped on the floor is not redelivered by the
//!   bus; the un-advanced cursor redelivers it on the next sync instead.
//! - Ack bookkeeping (`published()` / `acked()`) lets tests assert the
//!   ack-after-commit ordering.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use wkyt_core::DeltaBatch;

#[derive(Debug, thiserror::Error)]
pub enum BusError {
    #[error("bus is closed (subscriber dropped)")]
    Closed,
}

/// Producer half: connectors (via the orchestrator pump) publish batches.
#[async_trait::async_trait]
pub trait BusPublisher: Send + Sync {
    /// Awaits when the transport is at capacity (backpressure), errors when
    /// the consumer is gone.
    async fn publish(&self, batch: DeltaBatch) -> Result<(), BusError>;
}

/// Consumer half: the vault-side orchestrator pulls deliveries.
#[async_trait::async_trait]
pub trait BusSubscriber: Send {
    /// `None` means the bus is closed and drained: every publisher is gone
    /// and nothing remains in flight.
    async fn next(&mut self) -> Option<Delivery>;
}

/// One batch in flight. Call [`Delivery::into_parts`] and ack **only after
/// the batch's vault transaction has committed**.
pub struct Delivery {
    batch: DeltaBatch,
    ack: Ack,
}

impl Delivery {
    pub fn batch(&self) -> &DeltaBatch {
        &self.batch
    }

    pub fn into_parts(self) -> (DeltaBatch, Ack) {
        (self.batch, self.ack)
    }
}

/// Acknowledgement handle. Consuming `self` is deliberate: a delivery can
/// be acked at most once, and dropping it un-acked is a visible decision.
pub struct Ack(Box<dyn FnOnce() + Send>);

impl Ack {
    pub fn ack(self) {
        (self.0)()
    }
}

/// Create a bounded in-process bus. `capacity` is the maximum number of
/// batches in flight before `publish` blocks the producer.
pub fn in_process(capacity: usize) -> (InProcessPublisher, InProcessSubscriber) {
    assert!(capacity > 0, "a zero-capacity bus cannot move anything");
    let (tx, rx) = mpsc::channel(capacity);
    let counters = Arc::new(Counters::default());
    (
        InProcessPublisher { tx, counters: Arc::clone(&counters) },
        InProcessSubscriber { rx, counters },
    )
}

#[derive(Default)]
struct Counters {
    published: AtomicU64,
    acked: AtomicU64,
}

pub struct InProcessPublisher {
    tx: mpsc::Sender<DeltaBatch>,
    counters: Arc<Counters>,
}

impl InProcessPublisher {
    /// Batches accepted by the bus so far (test/diagnostic surface).
    pub fn published(&self) -> u64 {
        self.counters.published.load(Ordering::SeqCst)
    }

    /// Deliveries the consumer has acked so far (test/diagnostic surface).
    pub fn acked(&self) -> u64 {
        self.counters.acked.load(Ordering::SeqCst)
    }
}

#[async_trait::async_trait]
impl BusPublisher for InProcessPublisher {
    async fn publish(&self, batch: DeltaBatch) -> Result<(), BusError> {
        self.tx.send(batch).await.map_err(|_| BusError::Closed)?;
        self.counters.published.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

pub struct InProcessSubscriber {
    rx: mpsc::Receiver<DeltaBatch>,
    counters: Arc<Counters>,
}

#[async_trait::async_trait]
impl BusSubscriber for InProcessSubscriber {
    async fn next(&mut self) -> Option<Delivery> {
        let batch = self.rx.recv().await?;
        let counters = Arc::clone(&self.counters);
        Some(Delivery {
            batch,
            ack: Ack(Box::new(move || {
                counters.acked.fetch_add(1, Ordering::SeqCst);
            })),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn batch(n: u32) -> DeltaBatch {
        DeltaBatch {
            connector_id: "test".into(),
            deltas: vec![],
            cursor: Some(wkyt_core::SyncToken(format!("c{n}"))),
        }
    }

    #[tokio::test]
    async fn delivers_in_order_and_tracks_acks() {
        let (publisher, mut subscriber) = in_process(8);
        publisher.publish(batch(1)).await.unwrap();
        publisher.publish(batch(2)).await.unwrap();
        assert_eq!(publisher.published(), 2);
        assert_eq!(publisher.acked(), 0);

        let d1 = subscriber.next().await.unwrap();
        assert_eq!(d1.batch().cursor.as_ref().unwrap().0, "c1");
        let (_, ack) = d1.into_parts();
        ack.ack();
        assert_eq!(publisher.acked(), 1);

        let d2 = subscriber.next().await.unwrap();
        assert_eq!(d2.batch().cursor.as_ref().unwrap().0, "c2");
        // Dropped un-acked: bus does not count it, and does not redeliver —
        // the cursor mechanism owns redelivery.
        drop(d2);
        assert_eq!(publisher.acked(), 1);
    }

    #[tokio::test]
    async fn full_bus_applies_backpressure_until_consumed() {
        let (publisher, mut subscriber) = in_process(1);
        publisher.publish(batch(1)).await.unwrap();

        // Second publish must pend: the bus is at capacity.
        let second = publisher.publish(batch(2));
        tokio::pin!(second);
        assert!(
            tokio::time::timeout(Duration::from_millis(50), &mut second).await.is_err(),
            "publish into a full bus must wait, not drop or error"
        );

        // Consuming one frees the slot and the pending publish completes.
        let d = subscriber.next().await.unwrap();
        d.into_parts().1.ack();
        tokio::time::timeout(Duration::from_millis(200), second)
            .await
            .expect("publish should complete once capacity frees")
            .unwrap();
    }

    #[tokio::test]
    async fn closed_bus_reports_closed_to_publishers_and_drains_for_consumers() {
        let (publisher, mut subscriber) = in_process(4);
        publisher.publish(batch(1)).await.unwrap();

        // Consumer side: after all publishers drop, remaining items drain,
        // then next() returns None.
        drop(publisher);
        assert!(subscriber.next().await.is_some());
        assert!(subscriber.next().await.is_none());

        // Publisher side: a dropped subscriber surfaces as Closed.
        let (publisher, subscriber) = in_process(4);
        drop(subscriber);
        assert!(matches!(publisher.publish(batch(1)).await, Err(BusError::Closed)));
    }
}
