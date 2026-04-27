//! Live engine host and shared `EngineFrameInput` construction (RFC 0011 Phase 63).

use super::EngineFrameError;
use super::EngineQueryRequest;
use super::EngineSubsystemAdapter;
use super::latency::{LateInputSampler, TickInputSource};
use super::runtime::{
    EngineFrameInput, EngineFrameOutput, EngineFrameRuntime, EngineFrameRuntimePolicy,
};
use crate::state_advance::{SimulationTick, TickInputBatch};
use crate::time_semantics::{PresentationFrame, SnapshotEpoch, TemporalClock, WallClockStamp};
use crate::world_identity::WorldSnapshotHandle;
use std::sync::Arc;

/// Scenario + default query wiring shared by perf collection and live hosts.
#[derive(Debug, Clone)]
pub struct LiveProjectConfig {
    pub scenario_id: String,
    pub default_query_requests: Vec<EngineQueryRequest>,
    pub simulation_hz_override: Option<f64>,
}

impl Default for LiveProjectConfig {
    fn default() -> Self {
        Self {
            scenario_id: "live".to_string(),
            default_query_requests: Vec::new(),
            simulation_hz_override: None,
        }
    }
}

fn wall_step_nanos(simulation_hz: f64) -> u64 {
    if simulation_hz <= f64::EPSILON {
        return 16_666;
    }
    (1_000_000_000.0 / simulation_hz).round().max(1.0) as u64
}

/// Temporal clocks for one engine-frame input, aligned with perf benchmark conventions.
///
/// `wall_start_nanos` is the monotonic host time at the **start** of this tick's simulation step;
/// `wall_step_nanos` is the fixed simulation quantum (`1 / simulation_hz`).
pub fn live_temporal_clocks_for_frame(
    previous_snapshot: &WorldSnapshotHandle,
    frame_index: u32,
    wall_start_nanos: u64,
    wall_step_nanos: u64,
) -> (TemporalClock, TemporalClock) {
    let previous_epoch = previous_snapshot.epoch().0;
    let current_epoch = previous_epoch.saturating_add(1);
    let previous_tick = u64::from(frame_index);
    let current_tick = previous_tick.saturating_add(1);
    let previous_clock = TemporalClock::new(
        SnapshotEpoch::new(previous_epoch),
        SimulationTick::new(previous_tick),
        PresentationFrame::new(previous_tick),
        WallClockStamp::new(wall_start_nanos),
    );
    let current_clock = TemporalClock::new(
        SnapshotEpoch::new(current_epoch),
        SimulationTick::new(current_tick),
        PresentationFrame::new(current_tick),
        WallClockStamp::new(wall_start_nanos.saturating_add(wall_step_nanos)),
    );
    (previous_clock, current_clock)
}

/// Single construction path for [`EngineFrameInput`] (benchmark eager vs live late sampling).
pub fn build_engine_frame_input(
    config: &LiveProjectConfig,
    frame_index: u32,
    previous_snapshot: WorldSnapshotHandle,
    previous_clock: TemporalClock,
    current_clock: TemporalClock,
    tick_inputs: TickInputSource,
    policy: EngineFrameRuntimePolicy,
) -> EngineFrameInput {
    EngineFrameInput {
        scenario_id: config.scenario_id.clone(),
        frame_index,
        previous_snapshot,
        previous_clock,
        current_clock,
        tick_inputs,
        policy,
        query_requests: config.default_query_requests.clone(),
        readback_requests: Vec::new(),
    }
}

/// Eager tick source used for headless / recorded paths (Phase 63).
pub trait EagerTickInputSource: Send {
    fn drain_for_tick(&mut self, tick: SimulationTick, wall: WallClockStamp) -> TickInputBatch;
}

/// Empty input batch each tick.
#[derive(Debug, Default)]
pub struct HeadlessTickSource;

impl EagerTickInputSource for HeadlessTickSource {
    fn drain_for_tick(&mut self, tick: SimulationTick, _wall: WallClockStamp) -> TickInputBatch {
        TickInputBatch::new(tick, Vec::new())
    }
}

/// Headless live loop driver over [`EngineFrameRuntime`].
///
/// RFC 0011 C1: the host **owns** the subsystem adapters across frames and
/// re-uses them, so per-frame state slots (audio plan, save request, physics
/// dt, residency target, etc.) are persistent and `prepare_frame` can be the
/// only point where shared state is updated.
pub struct LiveEngineHost {
    pub runtime: EngineFrameRuntime,
    pub policy: EngineFrameRuntimePolicy,
    pub config: LiveProjectConfig,
    simulation_hz: f64,
    accumulator_secs: f64,
    wall_step_nanos: u64,
    /// Monotonic wall cursor (nanoseconds) advanced by each executed simulation step.
    pub wall_nanos: u64,
    pub previous_snapshot: WorldSnapshotHandle,
    pub previous_clock: TemporalClock,
    pub current_clock: TemporalClock,
    pub frame_index: u32,
    eager_source: Option<Box<dyn EagerTickInputSource>>,
    late_sampler: Option<Arc<dyn LateInputSampler + Send + Sync>>,
    /// Persistent engine subsystems. Re-used across frames so adapters that
    /// hold per-frame mutable state in `Arc<Mutex<...>>` slots keep that
    /// state stable for the lifetime of the host.
    subsystems: Vec<Box<dyn EngineSubsystemAdapter>>,
}

/// One [`advance`](LiveEngineHost::advance) call may run zero or more simulation ticks.
#[derive(Debug)]
pub struct LiveEngineTick {
    pub outputs: Vec<EngineFrameOutput>,
}

impl LiveEngineHost {
    /// Headless mode: materialize empty eager batches each tick.
    ///
    pub fn new_headless(
        runtime: EngineFrameRuntime,
        config: LiveProjectConfig,
        policy: EngineFrameRuntimePolicy,
        initial_snapshot: WorldSnapshotHandle,
        simulation_hz: f64,
    ) -> Self {
        let hz = config.simulation_hz_override.unwrap_or(simulation_hz);
        let wall_step_nanos = wall_step_nanos(hz);
        // Headless synthetic time starts honestly at t=0. The first emitted
        // report still carries a positive current wall stamp because the
        // frame advances by `wall_step_nanos` before publication.
        let initial_wall_nanos = 0;
        let (previous_clock, current_clock) = live_temporal_clocks_for_frame(
            &initial_snapshot,
            0,
            initial_wall_nanos,
            wall_step_nanos,
        );
        Self {
            runtime,
            policy,
            config,
            simulation_hz: hz,
            accumulator_secs: 0.0,
            wall_step_nanos,
            wall_nanos: initial_wall_nanos,
            previous_snapshot: initial_snapshot,
            previous_clock,
            current_clock,
            frame_index: 0,
            eager_source: Some(Box::new(HeadlessTickSource)),
            late_sampler: None,
            subsystems: Vec::new(),
        }
    }

    /// Late-sampled platform input (sampler drained inside state-advance job).
    pub fn with_late_sampler(
        runtime: EngineFrameRuntime,
        config: LiveProjectConfig,
        policy: EngineFrameRuntimePolicy,
        initial_snapshot: WorldSnapshotHandle,
        simulation_hz: f64,
        sampler: Arc<dyn LateInputSampler + Send + Sync>,
    ) -> Self {
        let hz = config.simulation_hz_override.unwrap_or(simulation_hz);
        let wall_step_nanos = wall_step_nanos(hz);
        let initial_wall_nanos = 0;
        let (previous_clock, current_clock) = live_temporal_clocks_for_frame(
            &initial_snapshot,
            0,
            initial_wall_nanos,
            wall_step_nanos,
        );
        Self {
            runtime,
            policy,
            config,
            simulation_hz: hz,
            accumulator_secs: 0.0,
            wall_step_nanos,
            wall_nanos: initial_wall_nanos,
            previous_snapshot: initial_snapshot,
            previous_clock,
            current_clock,
            frame_index: 0,
            eager_source: None,
            late_sampler: Some(sampler),
            subsystems: Vec::new(),
        }
    }

    /// Register a subsystem adapter that will be passed to every
    /// `run_frame_with_persistent_subsystems` call. Adapters are run in
    /// registration order; the runtime is still the source of truth for
    /// dependency ordering between subsystem kinds.
    pub fn add_subsystem(&mut self, subsystem: Box<dyn EngineSubsystemAdapter>) {
        self.subsystems.push(subsystem);
    }

    /// Replace the registered subsystem set wholesale.
    pub fn set_subsystems(&mut self, subsystems: Vec<Box<dyn EngineSubsystemAdapter>>) {
        self.subsystems = subsystems;
    }

    pub fn subsystems(&self) -> &[Box<dyn EngineSubsystemAdapter>] {
        &self.subsystems
    }

    pub fn subsystems_mut(&mut self) -> &mut [Box<dyn EngineSubsystemAdapter>] {
        &mut self.subsystems
    }

    /// Fixed-step simulation: feed wall time; each full `1/simulation_hz` slice runs one frame.
    pub fn advance(&mut self, wall_elapsed_secs: f64) -> Result<LiveEngineTick, EngineFrameError> {
        self.accumulator_secs += wall_elapsed_secs;
        let step = 1.0 / self.simulation_hz;
        let mut outputs = Vec::new();
        while self.accumulator_secs + f64::EPSILON >= step {
            let (previous_clock, current_clock) = live_temporal_clocks_for_frame(
                &self.previous_snapshot,
                self.frame_index,
                self.wall_nanos,
                self.wall_step_nanos,
            );
            self.previous_clock = previous_clock;
            self.current_clock = current_clock;

            let tick_source = if let Some(ref mut eager) = self.eager_source {
                TickInputSource::eager(eager.drain_for_tick(
                    self.current_clock.simulation_tick,
                    self.current_clock.wall_clock,
                ))
            } else if let Some(ref late) = self.late_sampler {
                TickInputSource::Late(Arc::clone(late))
            } else {
                TickInputSource::eager(TickInputBatch::new(
                    self.current_clock.simulation_tick,
                    Vec::new(),
                ))
            };

            let input = build_engine_frame_input(
                &self.config,
                self.frame_index,
                self.previous_snapshot.clone(),
                self.previous_clock,
                self.current_clock,
                tick_source,
                self.policy.clone(),
            );
            // RFC 0011 C1: re-use persistent subsystem adapters across frames
            // instead of constructing a fresh `Vec::new()` every frame.
            let output = self
                .runtime
                .run_frame_with_persistent_subsystems(input, &mut self.subsystems)?;
            self.previous_snapshot = output.snapshot.clone();
            self.frame_index = self.frame_index.saturating_add(1);
            self.wall_nanos = self.wall_nanos.saturating_add(self.wall_step_nanos);
            self.accumulator_secs -= step;
            outputs.push(output);
        }
        Ok(LiveEngineTick { outputs })
    }
}
