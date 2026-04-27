//! Bridges `wrela_runtime` raw input rings to [`crate::engine_frame::LateInputSampler`].
//!
//! Reuses a per-sampler scratch buffer to avoid heap allocations across drains.

use super::latency::{InputRingState, LateInputSampler};
use crate::state_advance::{SimulationTick, TickInputBatch, TickInputEvent, TickInputKind};
use crate::time_semantics::WallClockStamp;
use smol_str::SmolStr;
use std::sync::{Arc, Mutex};
use wrela_runtime::platform::input::TimestampedRawEvent;
use wrela_runtime::platform::input_pump::RawInputConsumer;

/// [`LateInputSampler`] backed by a lock-free raw input consumer.
pub struct RawInputRingLateSampler {
    consumer: Mutex<RawInputConsumer>,
    scratch: Mutex<Vec<TimestampedRawEvent>>,
    now_nanos: Arc<dyn Fn() -> u64 + Send + Sync>,
}

impl RawInputRingLateSampler {
    pub fn new(consumer: RawInputConsumer) -> Self {
        Self::with_clock(consumer, Arc::new(|| 0))
    }

    pub fn with_clock(
        consumer: RawInputConsumer,
        now_nanos: Arc<dyn Fn() -> u64 + Send + Sync>,
    ) -> Self {
        Self {
            consumer: Mutex::new(consumer),
            scratch: Mutex::new(Vec::with_capacity(64)),
            now_nanos,
        }
    }
}

impl LateInputSampler for RawInputRingLateSampler {
    fn now(&self) -> WallClockStamp {
        WallClockStamp::new((self.now_nanos)())
    }

    fn drain_up_to(&self, deadline: WallClockStamp) -> TickInputBatch {
        let mut buffer = self
            .scratch
            .lock()
            .expect("late input scratch lock poisoned");
        buffer.clear();
        self.consumer
            .lock()
            .expect("late input consumer lock poisoned")
            .drain_up_to_nanos(deadline.get(), &mut buffer);

        let mut inputs = Vec::with_capacity(buffer.len());
        for event in buffer.drain(..) {
            inputs.push(TickInputEvent::with_timestamps(
                SimulationTick::new(0),
                TickInputKind::Event,
                event.source,
                SmolStr::new(format!("{:?}", event.kind)),
                WallClockStamp::new(event.wall_clock_micros.saturating_mul(1000)),
                event.monotonic_nanos,
            ));
        }
        TickInputBatch::new(SimulationTick::new(0), inputs)
    }

    fn ring_state(&self) -> InputRingState {
        let s = self
            .consumer
            .lock()
            .expect("late input consumer lock poisoned")
            .ring_state();
        InputRingState {
            depth: s.depth,
            dropped_events: s.dropped_events,
            overflow: s.overflow,
        }
    }

    fn clear_overflow(&self) {
        self.consumer
            .lock()
            .expect("late input consumer lock poisoned")
            .clear_overflow();
    }
}
