//! Input-to-photon latency contract (RFC 0011 Phase 62.95).
//! `TickInputSource` and `MotionToPhotonContract` live here so `runtime/` can
//! implement `LateInputSampler` without depending on compiler types in public APIs.

use crate::state_advance::{SimulationTick, TickInputBatch};
use crate::time_semantics::WallClockStamp;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) struct InputArrivalTimestampStats {
    pub earliest_valid_arrival_nanos: Option<u64>,
    pub future_timestamp_count: usize,
}

impl InputArrivalTimestampStats {
    pub fn has_future_timestamp(self) -> bool {
        self.future_timestamp_count > 0
    }
}

pub(super) fn input_arrival_timestamp_stats(
    batch: &TickInputBatch,
    sample_nanos: u64,
) -> InputArrivalTimestampStats {
    let mut stats = InputArrivalTimestampStats::default();
    for event in &batch.inputs {
        let arrival = event.monotonic_nanos;
        if arrival == 0 {
            continue;
        }
        if arrival > sample_nanos {
            stats.future_timestamp_count = stats.future_timestamp_count.saturating_add(1);
            continue;
        }
        stats.earliest_valid_arrival_nanos = Some(
            stats
                .earliest_valid_arrival_nanos
                .map(|prev| prev.min(arrival))
                .unwrap_or(arrival),
        );
    }
    stats
}

/// How motion-to-photon stages were measured for this frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MeasurementQuality {
    Synthetic,
    ExactGpuTimestamp,
    #[default]
    EstimatedFromCpuClock,
}

/// End-to-end latency stages (nanoseconds). Sum should approximate `total_estimate_nanos`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MotionToPhotonContract {
    pub event_arrival_to_state_advance_nanos: u64,
    pub state_advance_to_render_submit_nanos: u64,
    pub render_submit_to_gpu_complete_nanos: u64,
    pub gpu_complete_to_present_callback_nanos: u64,
    pub estimated_present_to_photons_nanos: u64,
    pub total_estimate_nanos: u64,
    #[serde(default)]
    pub measurement_quality: MeasurementQuality,
}

impl MotionToPhotonContract {
    /// Deterministic synthetic contract for benchmark / closure parity.
    pub fn synthetic_idle() -> Self {
        Self {
            event_arrival_to_state_advance_nanos: 100,
            state_advance_to_render_submit_nanos: 200,
            render_submit_to_gpu_complete_nanos: 300,
            gpu_complete_to_present_callback_nanos: 400,
            estimated_present_to_photons_nanos: 0,
            total_estimate_nanos: 1000,
            measurement_quality: MeasurementQuality::Synthetic,
        }
    }

    pub fn recompute_total(&mut self) {
        self.total_estimate_nanos = self
            .event_arrival_to_state_advance_nanos
            .saturating_add(self.state_advance_to_render_submit_nanos)
            .saturating_add(self.render_submit_to_gpu_complete_nanos)
            .saturating_add(self.gpu_complete_to_present_callback_nanos)
            .saturating_add(self.estimated_present_to_photons_nanos);
    }
}

/// Present-mode preference for interactive hosts (RFC 0011).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PresentModePolicy {
    #[default]
    PreferMailboxThenVrrFifoThenFifo,
    Fifo,
    Mailbox,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolvedPresentMode {
    Mailbox,
    FifoRelaxed,
    Fifo,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PresentModeSelection {
    pub mode: ResolvedPresentMode,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub findings: Vec<String>,
}

impl PresentModePolicy {
    pub fn select(
        self,
        supports_mailbox: bool,
        supports_vrr_fifo_relaxed: bool,
    ) -> PresentModeSelection {
        match self {
            PresentModePolicy::Mailbox if supports_mailbox => PresentModeSelection {
                mode: ResolvedPresentMode::Mailbox,
                findings: Vec::new(),
            },
            PresentModePolicy::Mailbox => PresentModeSelection {
                mode: if supports_vrr_fifo_relaxed {
                    ResolvedPresentMode::FifoRelaxed
                } else {
                    ResolvedPresentMode::Fifo
                },
                findings: vec!["presentation.fallback_to_vsync_fifo".to_string()],
            },
            PresentModePolicy::Fifo => PresentModeSelection {
                mode: ResolvedPresentMode::Fifo,
                findings: Vec::new(),
            },
            PresentModePolicy::PreferMailboxThenVrrFifoThenFifo if supports_mailbox => {
                PresentModeSelection {
                    mode: ResolvedPresentMode::Mailbox,
                    findings: Vec::new(),
                }
            }
            PresentModePolicy::PreferMailboxThenVrrFifoThenFifo if supports_vrr_fifo_relaxed => {
                PresentModeSelection {
                    mode: ResolvedPresentMode::FifoRelaxed,
                    findings: Vec::new(),
                }
            }
            PresentModePolicy::PreferMailboxThenVrrFifoThenFifo => PresentModeSelection {
                mode: ResolvedPresentMode::Fifo,
                findings: vec!["presentation.fallback_to_vsync_fifo".to_string()],
            },
        }
    }
}

/// Ring-buffer telemetry for late-sampled input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct InputRingState {
    pub depth: u32,
    pub dropped_events: u32,
    pub overflow: bool,
}

/// Late-sampled platform input (implemented in `runtime/` for winit).
///
/// The `deadline` passed to [`LateInputSampler::drain_up_to`] and every
/// non-zero [`TickInputEvent::monotonic_nanos`](crate::state_advance::TickInputEvent::monotonic_nanos)
/// returned in that batch must share one monotonic nanosecond origin. The
/// engine-frame latency contract computes input age as `deadline - event`,
/// so samplers must not mix wall-clock, frame-relative, and platform
/// monotonic domains.
pub trait LateInputSampler: Send + Sync {
    fn now(&self) -> WallClockStamp {
        WallClockStamp::new(0)
    }

    fn drain_up_to(&self, deadline: WallClockStamp) -> TickInputBatch;

    fn ring_state(&self) -> InputRingState {
        InputRingState::default()
    }

    fn clear_overflow(&self) {}
}

/// Raw tick input is either pre-built (`Eager`) or drained at state-advance time (`Late`).
#[derive(Clone)]
pub enum TickInputSource {
    Eager(TickInputBatch),
    Late(Arc<dyn LateInputSampler + Send + Sync>),
}

impl TickInputSource {
    pub fn eager(batch: TickInputBatch) -> Self {
        Self::Eager(batch)
    }

    pub fn empty_eager(tick: SimulationTick) -> Self {
        Self::Eager(TickInputBatch::new(tick, Vec::new()))
    }

    /// Materialize the batch used for this simulation step.
    /// For [`TickInputSource::Late`], overwrites `TickInputBatch.tick` and
    /// every drained [`TickInputEvent`](crate::state_advance::TickInputEvent)
    /// tick with `tick`.
    pub fn materialize_for_simulation_tick(
        &self,
        tick: SimulationTick,
        deadline: WallClockStamp,
    ) -> TickInputBatch {
        match self {
            TickInputSource::Eager(batch) => batch.clone(),
            TickInputSource::Late(sampler) => {
                let sample_deadline = sampler.now();
                let deadline = if sample_deadline.get() == 0 {
                    deadline
                } else {
                    sample_deadline
                };
                let mut batch = sampler.drain_up_to(deadline);
                batch.tick = tick;
                for event in &mut batch.inputs {
                    event.tick = tick;
                }
                batch
            }
        }
    }

    pub fn sample_deadline(&self, fallback: WallClockStamp) -> WallClockStamp {
        match self {
            TickInputSource::Eager(_) => fallback,
            TickInputSource::Late(sampler) => {
                let deadline = sampler.now();
                if deadline.get() == 0 {
                    fallback
                } else {
                    deadline
                }
            }
        }
    }
}

impl std::fmt::Debug for TickInputSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TickInputSource::Eager(b) => f.debug_tuple("Eager").field(b).finish(),
            TickInputSource::Late(_) => f.write_str("Late(..)"),
        }
    }
}
