//! Lock-free SPSC input ring (RFC 0011 Phase 64).
//!
//! The platform thread owns [`RawInputProducer`] and the engine thread owns
//! [`RawInputConsumer`]. The ring preserves producer FIFO order; out-of-order
//! source timestamps are reported through [`RawInputRingState`] but are not
//! silently reordered.
//!
//! Drains reuse caller-supplied buffers so the steady-state path performs zero
//! heap allocations even at full ring depth.

use super::input::TimestampedRawEvent;
use crossbeam_queue::ArrayQueue;
use rtrb::{Consumer, Producer, PushError, RingBuffer};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RawInputRingState {
    pub depth: u32,
    pub dropped_events: u32,
    /// Number of producer pushes that observed a smaller monotonic timestamp
    /// than the previous push. Reported but not corrected: corrections would
    /// require global ordering that the SPSC contract cannot offer.
    pub out_of_order_events: u32,
    /// `true` while the ring has observed an overflow that has not yet been
    /// accepted and cleared by the host.
    pub overflow: bool,
}

#[derive(Debug)]
struct RawInputTelemetry {
    dropped_events: AtomicU32,
    out_of_order_events: AtomicU32,
    last_monotonic: AtomicU64,
    overflow_latch: AtomicBool,
    approx_depth: AtomicU32,
    oldest_events_to_drop: AtomicU32,
    overflow_events: ArrayQueue<TimestampedRawEvent>,
}

impl RawInputTelemetry {
    fn new(capacity: usize) -> Self {
        Self {
            dropped_events: AtomicU32::new(0),
            out_of_order_events: AtomicU32::new(0),
            last_monotonic: AtomicU64::new(0),
            overflow_latch: AtomicBool::new(false),
            approx_depth: AtomicU32::new(0),
            oldest_events_to_drop: AtomicU32::new(0),
            overflow_events: ArrayQueue::new(capacity),
        }
    }

    fn ring_state(&self) -> RawInputRingState {
        RawInputRingState {
            depth: self.approx_depth.load(Ordering::Acquire),
            dropped_events: self.dropped_events.load(Ordering::Relaxed),
            out_of_order_events: self.out_of_order_events.load(Ordering::Relaxed),
            overflow: self.overflow_latch.load(Ordering::Acquire),
        }
    }

    fn clear_overflow(&self) {
        self.overflow_latch.store(false, Ordering::Release);
    }
}

/// Producer half of the raw input ring. This owns [`rtrb::Producer`] directly.
pub struct RawInputProducer {
    producer: Producer<TimestampedRawEvent>,
    telemetry: Arc<RawInputTelemetry>,
    capacity: u32,
}

/// Consumer half of the raw input ring. This owns [`rtrb::Consumer`] directly.
pub struct RawInputConsumer {
    consumer: Consumer<TimestampedRawEvent>,
    telemetry: Arc<RawInputTelemetry>,
    capacity: u32,
}

#[derive(Clone)]
pub struct RawInputRingObserver {
    telemetry: Arc<RawInputTelemetry>,
}

/// Convenience wrapper for tests and simple single-threaded callers. Runtime
/// hosts should split the ring and move each half to its owning thread.
pub struct RawInputRing {
    producer: RawInputProducer,
    consumer: RawInputConsumer,
}

impl RawInputRing {
    pub fn split_with_capacity(capacity: usize) -> (RawInputProducer, RawInputConsumer) {
        let capacity = capacity.max(1);
        let (producer, consumer) = RingBuffer::new(capacity);
        let telemetry = Arc::new(RawInputTelemetry::new(capacity));
        (
            RawInputProducer {
                producer,
                telemetry: Arc::clone(&telemetry),
                capacity: capacity as u32,
            },
            RawInputConsumer {
                consumer,
                telemetry,
                capacity: capacity as u32,
            },
        )
    }

    pub fn with_capacity(capacity: usize) -> Self {
        let (producer, consumer) = Self::split_with_capacity(capacity);
        Self { producer, consumer }
    }

    pub fn into_split(self) -> (RawInputProducer, RawInputConsumer) {
        (self.producer, self.consumer)
    }

    pub fn push_event(&mut self, event: TimestampedRawEvent) {
        self.producer.push_event(event);
    }

    pub fn drain_up_to_nanos(
        &mut self,
        deadline_nanos: u64,
        out: &mut Vec<TimestampedRawEvent>,
    ) -> usize {
        self.consumer.drain_up_to_nanos(deadline_nanos, out)
    }

    pub fn ring_state(&self) -> RawInputRingState {
        self.consumer.ring_state()
    }

    pub fn clear_overflow(&self) {
        self.consumer.clear_overflow();
    }

    pub fn capacity(&self) -> u32 {
        self.consumer.capacity()
    }
}

impl Default for RawInputRing {
    fn default() -> Self {
        Self::with_capacity(4096)
    }
}

impl RawInputProducer {
    pub fn push_event(&mut self, event: TimestampedRawEvent) {
        let prev = self.telemetry.last_monotonic.load(Ordering::Relaxed);
        if event.monotonic_nanos < prev {
            self.telemetry
                .out_of_order_events
                .fetch_add(1, Ordering::Relaxed);
        } else {
            self.telemetry
                .last_monotonic
                .store(event.monotonic_nanos, Ordering::Relaxed);
        }

        match self.producer.push(event) {
            Ok(()) => {
                self.telemetry.approx_depth.fetch_add(1, Ordering::Release);
            }
            Err(PushError::Full(event)) => {
                self.telemetry
                    .dropped_events
                    .fetch_add(1, Ordering::Relaxed);
                let pending = self.telemetry.oldest_events_to_drop.load(Ordering::Acquire);
                if pending < self.capacity {
                    self.telemetry
                        .oldest_events_to_drop
                        .fetch_add(1, Ordering::Release);
                }
                let _ = self.telemetry.overflow_events.force_push(event);
                self.telemetry.overflow_latch.store(true, Ordering::Release);
            }
        }
    }

    pub fn ring_state(&self) -> RawInputRingState {
        self.telemetry.ring_state()
    }

    pub fn capacity(&self) -> u32 {
        self.capacity
    }
}

impl RawInputConsumer {
    /// Drain all events whose `monotonic_nanos <= deadline_nanos`, into `out`.
    /// Returns the number of events drained.
    pub fn drain_up_to_nanos(
        &mut self,
        deadline_nanos: u64,
        out: &mut Vec<TimestampedRawEvent>,
    ) -> usize {
        let start_len = out.len();
        self.drop_overflowed_oldest_events();
        loop {
            let should_pop = match self.consumer.peek() {
                Ok(event) => event.monotonic_nanos <= deadline_nanos,
                Err(_) => false,
            };
            if !should_pop {
                break;
            }
            match self.consumer.pop() {
                Ok(event) => {
                    out.push(event);
                    self.telemetry.approx_depth.fetch_sub(1, Ordering::AcqRel);
                }
                Err(_) => break,
            }
        }
        self.drain_overflow_events_up_to_nanos(deadline_nanos, out);
        out.len() - start_len
    }

    fn drop_overflowed_oldest_events(&mut self) {
        loop {
            let pending = self.telemetry.oldest_events_to_drop.load(Ordering::Acquire);
            if pending == 0 {
                break;
            }
            match self.consumer.pop() {
                Ok(_) => {
                    self.telemetry
                        .oldest_events_to_drop
                        .fetch_sub(1, Ordering::AcqRel);
                }
                Err(_) => break,
            }
        }
    }

    fn drain_overflow_events_up_to_nanos(
        &mut self,
        deadline_nanos: u64,
        out: &mut Vec<TimestampedRawEvent>,
    ) {
        while let Some(event) = self.telemetry.overflow_events.pop() {
            if event.monotonic_nanos <= deadline_nanos {
                out.push(event);
                self.telemetry.approx_depth.fetch_sub(1, Ordering::AcqRel);
            } else {
                let _ = self.telemetry.overflow_events.force_push(event);
                break;
            }
        }
    }

    pub fn ring_state(&self) -> RawInputRingState {
        self.telemetry.ring_state()
    }

    pub fn observer(&self) -> RawInputRingObserver {
        RawInputRingObserver {
            telemetry: Arc::clone(&self.telemetry),
        }
    }

    /// Clear the latched overflow flag after the frame has recorded the drop.
    pub fn clear_overflow(&self) {
        self.telemetry.clear_overflow();
    }

    pub fn capacity(&self) -> u32 {
        self.capacity
    }
}

impl RawInputRingObserver {
    pub fn ring_state(&self) -> RawInputRingState {
        self.telemetry.ring_state()
    }

    pub fn clear_overflow(&self) {
        self.telemetry.clear_overflow();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::input::RawInputKind;

    fn evt(source: &str, t: u64) -> TimestampedRawEvent {
        TimestampedRawEvent::new(
            source,
            "key.a",
            RawInputKind::Key {
                code: smol_str::SmolStr::new("a"),
                pressed: true,
            },
            t / 1000,
            t,
        )
    }

    #[test]
    fn fifo_order_is_preserved_across_drain() {
        let mut ring = RawInputRing::with_capacity(8);
        for i in 0..4u64 {
            ring.push_event(evt("kbd", i * 1000));
        }
        let mut out = Vec::new();
        let drained = ring.drain_up_to_nanos(u64::MAX, &mut out);
        assert_eq!(drained, 4);
        for i in 0..4u64 {
            assert_eq!(out[i as usize].monotonic_nanos, i * 1000);
        }
    }

    #[test]
    fn drain_up_to_deadline_leaves_future_events_in_ring() {
        let mut ring = RawInputRing::with_capacity(8);
        ring.push_event(evt("kbd", 100));
        ring.push_event(evt("kbd", 200));
        ring.push_event(evt("kbd", 300));
        let mut out = Vec::new();
        let drained = ring.drain_up_to_nanos(200, &mut out);
        assert_eq!(drained, 2);
        assert_eq!(out[0].monotonic_nanos, 100);
        assert_eq!(out[1].monotonic_nanos, 200);

        let state = ring.ring_state();
        assert_eq!(state.depth, 1);
        out.clear();
        let drained2 = ring.drain_up_to_nanos(u64::MAX, &mut out);
        assert_eq!(drained2, 1);
        assert_eq!(out[0].monotonic_nanos, 300);
    }

    #[test]
    fn overflow_latches_and_can_be_cleared() {
        let mut ring = RawInputRing::with_capacity(2);
        ring.push_event(evt("kbd", 1));
        ring.push_event(evt("kbd", 2));
        ring.push_event(evt("kbd", 3));
        let state = ring.ring_state();
        assert_eq!(state.dropped_events, 1);
        assert!(state.overflow);
        ring.clear_overflow();
        let state2 = ring.ring_state();
        assert!(!state2.overflow);
        assert_eq!(state2.dropped_events, 1, "drop counter is monotonic");
    }

    #[test]
    fn out_of_order_pushes_are_reported_not_silently_reordered() {
        let mut ring = RawInputRing::with_capacity(8);
        ring.push_event(evt("kbd", 100));
        ring.push_event(evt("kbd", 50));
        let mut out = Vec::new();
        ring.drain_up_to_nanos(u64::MAX, &mut out);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].monotonic_nanos, 100);
        assert_eq!(out[1].monotonic_nanos, 50);
        assert_eq!(ring.ring_state().out_of_order_events, 1);
    }
}
