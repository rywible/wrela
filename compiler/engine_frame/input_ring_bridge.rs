//! Bridges `wrela_runtime` raw input rings to [`crate::engine_frame::LateInputSampler`].
//!
//! Reuses a per-sampler scratch buffer to avoid heap allocations across drains.

use super::latency::{InputRingState, LateInputSampler};
use crate::state_advance::{
    SimulationTick, TickInputBatch, TickInputEvent, TickInputKind, TickInputValue,
};
use crate::time_semantics::WallClockStamp;
use std::cell::UnsafeCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use wrela_runtime::platform::input::TimestampedRawEvent;
use wrela_runtime::platform::input_pump::{RawInputConsumer, RawInputRingObserver};

/// [`LateInputSampler`] backed by a lock-free raw input consumer.
pub struct RawInputRingLateSampler {
    consumer: UnsafeCell<RawInputConsumer>,
    scratch: UnsafeCell<Vec<TimestampedRawEvent>>,
    observer: RawInputRingObserver,
    draining: AtomicBool,
    now_nanos: Arc<dyn Fn() -> u64 + Send + Sync>,
}

// SAFETY: RawInputRingLateSampler owns the single consumer half of an SPSC ring.
// drain_up_to uses `draining` as a non-blocking single-consumer guard before
// taking mutable access through UnsafeCell. ring_state and clear_overflow use a
// separate telemetry observer and never borrow the consumer while a drain holds
// the exclusive consumer reference.
unsafe impl Send for RawInputRingLateSampler {}
unsafe impl Sync for RawInputRingLateSampler {}

struct DrainGuard<'a>(&'a AtomicBool);

impl Drop for DrainGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

impl RawInputRingLateSampler {
    pub fn new(consumer: RawInputConsumer) -> Self {
        Self::with_clock(consumer, Arc::new(|| 0))
    }

    pub fn with_clock(
        consumer: RawInputConsumer,
        now_nanos: Arc<dyn Fn() -> u64 + Send + Sync>,
    ) -> Self {
        let observer = consumer.observer();
        Self {
            consumer: UnsafeCell::new(consumer),
            scratch: UnsafeCell::new(Vec::with_capacity(64)),
            observer,
            draining: AtomicBool::new(false),
            now_nanos,
        }
    }

    fn try_enter_drain(&self) -> Option<DrainGuard<'_>> {
        self.draining
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .ok()
            .map(|_| DrainGuard(&self.draining))
    }
}

impl LateInputSampler for RawInputRingLateSampler {
    fn now(&self) -> WallClockStamp {
        WallClockStamp::new((self.now_nanos)())
    }

    fn drain_up_to(&self, deadline: WallClockStamp) -> TickInputBatch {
        let Some(_guard) = self.try_enter_drain() else {
            return TickInputBatch::new(SimulationTick::new(0), Vec::new());
        };

        // SAFETY: `_guard` proves this is the only active drain. The SPSC
        // consumer half has exactly one logical consumer, owned by this sampler.
        let consumer = unsafe { &mut *self.consumer.get() };
        // SAFETY: scratch is only accessed while `_guard` is held.
        let buffer = unsafe { &mut *self.scratch.get() };
        buffer.clear();
        consumer.drain_up_to_nanos(deadline.get(), buffer);

        let mut inputs = Vec::with_capacity(buffer.len());
        for event in buffer.drain(..) {
            let value = tick_input_value_from_raw(&event.kind);
            inputs.push(TickInputEvent::with_timestamps_and_value(
                SimulationTick::new(0),
                TickInputKind::Event,
                event.source,
                event.detail,
                value,
                WallClockStamp::new(event.wall_clock_micros.saturating_mul(1000)),
                event.monotonic_nanos,
            ));
        }
        TickInputBatch::new(SimulationTick::new(0), inputs)
    }

    fn ring_state(&self) -> InputRingState {
        let s = self.observer.ring_state();
        InputRingState {
            depth: s.depth,
            dropped_events: s.dropped_events,
            overflow: s.overflow,
        }
    }

    fn clear_overflow(&self) {
        self.observer.clear_overflow();
    }
}

fn tick_input_value_from_raw(
    kind: &wrela_runtime::platform::input::RawInputKind,
) -> TickInputValue {
    use wrela_runtime::platform::input::RawInputKind;

    match kind {
        RawInputKind::Key { pressed, .. }
        | RawInputKind::MouseButton { pressed, .. }
        | RawInputKind::GamepadButton { pressed, .. } => TickInputValue::button(*pressed),
        RawInputKind::MouseDelta { x, y } => TickInputValue::Axis2 {
            x_micros: x.saturating_mul(1_000),
            y_micros: y.saturating_mul(1_000),
        },
        RawInputKind::GamepadAxis { value_micros, .. } => TickInputValue::Axis1 {
            value_micros: *value_micros,
        },
    }
}
