//! RFC 0011 Phase 63 — `LiveEngineHost` and late sampling.

use smol_str::SmolStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use wrela::engine_frame::{
    EngineFrameContext, EngineFrameError, EngineFrameReport, EngineFrameRuntime,
    EngineFrameRuntimePolicy, EngineFrameScheduler, EngineFrameTimeline, EngineGpuTimingPolicy,
    EngineJobAffinity, EngineMeasurementPolicy, EngineRuntimeSource, EngineSpanDomain,
    EngineSubsystemAdapter, EngineSubsystemDescriptor, EngineSubsystemKind, EngineSubsystemPlan,
    EngineSubsystemReport, LateInputSampler, LiveEngineHost, LiveProjectConfig,
    MotionToPhotonContract, apply_latency_budget_to_report,
};
use wrela::gpu_runtime::GpuRuntimeMetrics;
use wrela::query_exec::stable_region_snapshot_handle;
use wrela::state_advance::{
    ChangeClass, ChangeSummary, StateAdvanceResult, TickInputBatch, TickInputEvent, TickInputKind,
    WorldTransitionRecord,
};
use wrela::time_semantics::{SimulationTick, WallClockStamp};

#[derive(Default)]
struct NoopStateAdvanceExecutor;

impl wrela::engine_frame::EngineStateAdvanceExecutor for NoopStateAdvanceExecutor {
    fn advance(
        &mut self,
        input: wrela::engine_frame::EngineStateAdvanceInput,
    ) -> Result<StateAdvanceResult, wrela::engine_frame::EngineFrameError> {
        let previous = input.previous_snapshot.clone();
        let next = previous.with_epoch(wrela::world_identity::SnapshotEpoch(
            previous.epoch().0.saturating_add(1),
        ));
        Ok(StateAdvanceResult::new(
            WorldTransitionRecord::new(
                Some(previous),
                next,
                Some(input.previous_clock),
                input.current_clock,
                input.inputs,
                Vec::new(),
            ),
            ChangeSummary::new(ChangeClass::None, "live host test noop"),
        ))
    }
}

#[derive(Default)]
struct FailingStateAdvanceExecutor;

impl wrela::engine_frame::EngineStateAdvanceExecutor for FailingStateAdvanceExecutor {
    fn advance(
        &mut self,
        _input: wrela::engine_frame::EngineStateAdvanceInput,
    ) -> Result<StateAdvanceResult, wrela::engine_frame::EngineFrameError> {
        Err(wrela::engine_frame::EngineFrameError::Message(
            "intentional state advance failure".to_string(),
        ))
    }
}

#[derive(Default)]
struct RewritingStateAdvanceExecutor;

impl wrela::engine_frame::EngineStateAdvanceExecutor for RewritingStateAdvanceExecutor {
    fn advance(
        &mut self,
        input: wrela::engine_frame::EngineStateAdvanceInput,
    ) -> Result<StateAdvanceResult, wrela::engine_frame::EngineFrameError> {
        let previous = input.previous_snapshot.clone();
        let next = previous.with_epoch(wrela::world_identity::SnapshotEpoch(
            previous.epoch().0.saturating_add(1),
        ));
        Ok(StateAdvanceResult::new(
            WorldTransitionRecord::new(
                Some(previous),
                next,
                Some(input.previous_clock),
                input.current_clock,
                TickInputBatch::new(input.inputs.tick, Vec::new()),
                Vec::new(),
            ),
            ChangeSummary::new(ChangeClass::None, "rewrote test inputs"),
        ))
    }
}

#[derive(Default)]
struct BehaviorStateAdvanceExecutor;

impl wrela::engine_frame::EngineStateAdvanceExecutor for BehaviorStateAdvanceExecutor {
    fn advance(
        &mut self,
        input: wrela::engine_frame::EngineStateAdvanceInput,
    ) -> Result<StateAdvanceResult, wrela::engine_frame::EngineFrameError> {
        let previous = input.previous_snapshot.clone();
        let next = previous.with_epoch(wrela::world_identity::SnapshotEpoch(
            previous.epoch().0.saturating_add(1),
        ));
        Ok(StateAdvanceResult::new(
            WorldTransitionRecord::new(
                Some(previous),
                next,
                Some(input.previous_clock),
                input.current_clock,
                input.inputs,
                Vec::new(),
            ),
            ChangeSummary::new(ChangeClass::Behavior, "disallowed test behavior change"),
        ))
    }
}

#[test]
fn live_engine_host_headless_advances_epochs_and_budget_directives() {
    let snapshot = stable_region_snapshot_handle(&SmolStr::new("live_host_test"));
    let runtime = EngineFrameRuntime::new(Box::new(NoopStateAdvanceExecutor));
    let mut policy = EngineFrameRuntimePolicy::live();
    policy.motion_to_photon_target_ms = None;
    let config = LiveProjectConfig {
        scenario_id: "live_host_fixture".to_string(),
        default_query_requests: Vec::new(),
        simulation_hz_override: None,
    };
    let mut host = LiveEngineHost::new_headless(runtime, config, policy.clone(), snapshot, 60.0);
    let step = 1.0 / 60.0;
    let mut last_epoch = 0u64;
    for _ in 0..120 {
        let tick = host.advance(step).expect("advance");
        assert_eq!(tick.outputs.len(), 1);
        let out = &tick.outputs[0];
        let epoch = out.snapshot.epoch().0;
        assert!(epoch > last_epoch, "epoch should increase each tick");
        last_epoch = epoch;
        assert_eq!(out.report.frame_index, host.frame_index - 1);
        assert!(
            out.report.violations.is_empty(),
            "unexpected violations: {:?}",
            out.report.violations
        );
        assert_eq!(
            out.report.budget_directives.frame_wall_time_budget_ms,
            policy.budget.as_ref().map(|b| b.frame_wall_time_median_ms)
        );
        assert_eq!(
            out.report.latency.total_estimate_nanos,
            out.report
                .latency
                .event_arrival_to_state_advance_nanos
                .saturating_add(out.report.latency.state_advance_to_render_submit_nanos)
                .saturating_add(out.report.latency.render_submit_to_gpu_complete_nanos)
                .saturating_add(out.report.latency.gpu_complete_to_present_callback_nanos)
                .saturating_add(out.report.latency.estimated_present_to_photons_nanos)
        );
    }
    assert_eq!(last_epoch, 121);
}

#[derive(Default)]
struct RecordingLateSampler {
    events: Mutex<Vec<(WallClockStamp, u64)>>,
}

impl LateInputSampler for RecordingLateSampler {
    fn drain_up_to(&self, deadline: WallClockStamp) -> TickInputBatch {
        let tick = SimulationTick::new(1);
        let t0 = WallClockStamp::new(100);
        let ev =
            TickInputEvent::with_timestamps(tick, TickInputKind::Event, "test", "ping", t0, 1_000);
        self.events
            .lock()
            .expect("lock")
            .push((deadline, ev.monotonic_nanos));
        TickInputBatch::new(tick, vec![ev])
    }
}

struct HostClockLateSampler {
    sample_deadline: WallClockStamp,
    event_arrival: u64,
    observed_deadlines: Mutex<Vec<u64>>,
}

impl HostClockLateSampler {
    fn new(sample_deadline: u64, event_arrival: u64) -> Self {
        Self {
            sample_deadline: WallClockStamp::new(sample_deadline),
            event_arrival,
            observed_deadlines: Mutex::new(Vec::new()),
        }
    }
}

impl LateInputSampler for HostClockLateSampler {
    fn now(&self) -> WallClockStamp {
        self.sample_deadline
    }

    fn drain_up_to(&self, deadline: WallClockStamp) -> TickInputBatch {
        self.observed_deadlines
            .lock()
            .expect("deadlines")
            .push(deadline.get());
        let tick = SimulationTick::new(1);
        let ev = TickInputEvent::with_timestamps(
            tick,
            TickInputKind::Event,
            "test",
            "host_clock",
            WallClockStamp::new(self.event_arrival),
            self.event_arrival,
        );
        TickInputBatch::new(tick, vec![ev])
    }
}

#[derive(Default)]
struct DeadlineOffsetLateSampler {
    events: Mutex<Vec<(u64, u64)>>,
}

impl LateInputSampler for DeadlineOffsetLateSampler {
    fn drain_up_to(&self, deadline: WallClockStamp) -> TickInputBatch {
        const EVENT_AGE_NANOS: u64 = 500_000;
        let tick = SimulationTick::new(1);
        let event_arrival = deadline.get().saturating_sub(EVENT_AGE_NANOS);
        let ev = TickInputEvent::with_timestamps(
            tick,
            TickInputKind::Event,
            "test",
            "deadline_offset",
            WallClockStamp::new(event_arrival),
            event_arrival,
        );
        self.events
            .lock()
            .expect("lock")
            .push((deadline.get(), ev.monotonic_nanos));
        TickInputBatch::new(tick, vec![ev])
    }
}

#[derive(Default)]
struct PlaceholderTickLateSampler;

impl LateInputSampler for PlaceholderTickLateSampler {
    fn drain_up_to(&self, deadline: WallClockStamp) -> TickInputBatch {
        let placeholder_tick = SimulationTick::new(0);
        let ev = TickInputEvent::with_timestamps(
            placeholder_tick,
            TickInputKind::Event,
            "test",
            "placeholder_tick",
            deadline,
            deadline.get(),
        );
        TickInputBatch::new(placeholder_tick, vec![ev])
    }
}

#[derive(Default)]
struct FutureTimestampLateSampler {
    events: Mutex<Vec<(u64, u64)>>,
}

impl LateInputSampler for FutureTimestampLateSampler {
    fn drain_up_to(&self, deadline: WallClockStamp) -> TickInputBatch {
        let tick = SimulationTick::new(1);
        let future_arrival = deadline.get().saturating_add(500_000);
        let ev = TickInputEvent::with_timestamps(
            tick,
            TickInputKind::Event,
            "test",
            "future",
            WallClockStamp::new(future_arrival),
            future_arrival,
        );
        self.events
            .lock()
            .expect("events")
            .push((deadline.get(), ev.monotonic_nanos));
        TickInputBatch::new(tick, vec![ev])
    }
}

#[derive(Default)]
struct MixedTimestampLateSampler {
    events: Mutex<Vec<(u64, u64)>>,
}

impl LateInputSampler for MixedTimestampLateSampler {
    fn drain_up_to(&self, deadline: WallClockStamp) -> TickInputBatch {
        let tick = SimulationTick::new(1);
        let past_arrival = deadline.get().saturating_sub(500_000);
        let future_arrival = deadline.get().saturating_add(500_000);
        let events = vec![
            TickInputEvent::with_timestamps(
                tick,
                TickInputKind::Event,
                "test",
                "past",
                WallClockStamp::new(past_arrival),
                past_arrival,
            ),
            TickInputEvent::with_timestamps(
                tick,
                TickInputKind::Event,
                "test",
                "future",
                WallClockStamp::new(future_arrival),
                future_arrival,
            ),
        ];
        {
            let mut guard = self.events.lock().expect("events");
            guard.push((deadline.get(), past_arrival));
            guard.push((deadline.get(), future_arrival));
        }
        TickInputBatch::new(tick, events)
    }
}

struct OverflowLatchLateSampler {
    overflow: AtomicBool,
}

impl OverflowLatchLateSampler {
    fn new() -> Self {
        Self {
            overflow: AtomicBool::new(true),
        }
    }
}

impl LateInputSampler for OverflowLatchLateSampler {
    fn drain_up_to(&self, _deadline: WallClockStamp) -> TickInputBatch {
        TickInputBatch::new(SimulationTick::new(0), Vec::new())
    }

    fn ring_state(&self) -> wrela::engine_frame::InputRingState {
        wrela::engine_frame::InputRingState {
            depth: 0,
            dropped_events: 1,
            overflow: self.overflow.load(Ordering::SeqCst),
        }
    }

    fn clear_overflow(&self) {
        self.overflow.store(false, Ordering::SeqCst);
    }
}

#[test]
fn live_engine_host_late_sampler_materializes_timestamped_events() {
    let snapshot = stable_region_snapshot_handle(&SmolStr::new("live_late_sampler"));
    let runtime = EngineFrameRuntime::new(Box::new(NoopStateAdvanceExecutor));
    let policy = EngineFrameRuntimePolicy::closure();
    let config = LiveProjectConfig {
        scenario_id: "late_sample".to_string(),
        default_query_requests: Vec::new(),
        simulation_hz_override: None,
    };
    let sampler = Arc::new(RecordingLateSampler::default());
    let mut host =
        LiveEngineHost::with_late_sampler(runtime, config, policy, snapshot, 60.0, sampler.clone());
    let tick = host.advance(1.0 / 60.0).expect("one tick");
    assert_eq!(tick.outputs.len(), 1);
    let sa = tick.outputs[0]
        .report
        .state_advance
        .as_ref()
        .expect("state advance report");
    assert!(sa.input_count >= 1);
    let rec = sampler.events.lock().expect("lock");
    assert_eq!(rec.len(), 1);
}

#[test]
fn live_engine_host_records_and_clears_input_ring_overflow() {
    let snapshot = stable_region_snapshot_handle(&SmolStr::new("live_input_overflow"));
    let runtime = EngineFrameRuntime::new(Box::new(NoopStateAdvanceExecutor));
    let policy = EngineFrameRuntimePolicy::closure();
    let config = LiveProjectConfig {
        scenario_id: "input_overflow".to_string(),
        default_query_requests: Vec::new(),
        simulation_hz_override: None,
    };
    let sampler = Arc::new(OverflowLatchLateSampler::new());
    let mut host =
        LiveEngineHost::with_late_sampler(runtime, config, policy, snapshot, 60.0, sampler.clone());

    let tick = host.advance(1.0 / 60.0).expect("one tick");

    assert!(
        tick.outputs[0]
            .report
            .violations
            .contains(&"presentation.input_ring_overflow".to_string()),
        "expected overflow finding in frame violations, got {:?}",
        tick.outputs[0].report.violations
    );
    assert!(
        !sampler.overflow.load(Ordering::SeqCst),
        "overflow latch should be cleared only after the frame records it"
    );
}

#[test]
fn live_engine_host_late_sampler_uses_sampler_clock_for_input_age() {
    const HOST_SAMPLE_NANOS: u64 = 1_000_000_000;
    const EVENT_ARRIVAL_NANOS: u64 = 997_500_000;
    const EXPECTED_AGE_NANOS: u64 = HOST_SAMPLE_NANOS - EVENT_ARRIVAL_NANOS;

    let snapshot = stable_region_snapshot_handle(&SmolStr::new("live_sampler_clock"));
    let runtime = EngineFrameRuntime::new(Box::new(NoopStateAdvanceExecutor));
    let policy = EngineFrameRuntimePolicy::closure();
    let config = LiveProjectConfig {
        scenario_id: "sampler_clock".to_string(),
        default_query_requests: Vec::new(),
        simulation_hz_override: None,
    };
    let sampler = Arc::new(HostClockLateSampler::new(
        HOST_SAMPLE_NANOS,
        EVENT_ARRIVAL_NANOS,
    ));
    let mut host =
        LiveEngineHost::with_late_sampler(runtime, config, policy, snapshot, 60.0, sampler.clone());
    host.wall_nanos = 20_000_000;

    let tick = host.advance(1.0 / 60.0).expect("one tick");

    assert_eq!(tick.outputs.len(), 1);
    assert_eq!(
        sampler
            .observed_deadlines
            .lock()
            .expect("deadlines")
            .as_slice(),
        &[HOST_SAMPLE_NANOS],
        "state advance must drain late input using the sampler monotonic clock, not the synthetic fixed-step wall clock"
    );
    assert_eq!(
        tick.outputs[0]
            .report
            .latency
            .event_arrival_to_state_advance_nanos,
        EXPECTED_AGE_NANOS
    );
}

#[test]
fn live_engine_host_publishes_exact_materialized_batch_even_if_executor_rewrites_inputs() {
    let snapshot = stable_region_snapshot_handle(&SmolStr::new("live_rewritten_inputs"));
    let runtime = EngineFrameRuntime::new(Box::new(RewritingStateAdvanceExecutor));
    let slot = runtime.materialized_tick_input_slot();
    let policy = EngineFrameRuntimePolicy::closure();
    let config = LiveProjectConfig {
        scenario_id: "rewritten_inputs".to_string(),
        default_query_requests: Vec::new(),
        simulation_hz_override: None,
    };
    let sampler = Arc::new(DeadlineOffsetLateSampler::default());
    let mut host =
        LiveEngineHost::with_late_sampler(runtime, config, policy, snapshot, 60.0, sampler);

    let tick = host.advance(1.0 / 60.0).expect("one tick");

    assert_eq!(tick.outputs.len(), 1);
    assert_eq!(
        tick.outputs[0]
            .report
            .state_advance
            .as_ref()
            .expect("state advance report")
            .input_count,
        0,
        "executor deliberately rewrites transition_record.inputs to empty"
    );
    let batch = slot
        .snapshot()
        .expect("slot should be readable")
        .expect("successful StateAdvance should publish materialized inputs");
    assert_eq!(batch.inputs.len(), 1);
    assert_eq!(batch.inputs[0].detail.as_str(), "deadline_offset");
}

#[test]
fn live_engine_host_normalizes_late_event_ticks_to_materialized_tick() {
    let snapshot = stable_region_snapshot_handle(&SmolStr::new("live_late_event_tick"));
    let runtime = EngineFrameRuntime::new(Box::new(NoopStateAdvanceExecutor));
    let slot = runtime.materialized_tick_input_slot();
    let policy = EngineFrameRuntimePolicy::closure();
    let config = LiveProjectConfig {
        scenario_id: "late_event_tick".to_string(),
        default_query_requests: Vec::new(),
        simulation_hz_override: None,
    };
    let sampler = Arc::new(PlaceholderTickLateSampler);
    let mut host =
        LiveEngineHost::with_late_sampler(runtime, config, policy, snapshot, 60.0, sampler);

    host.advance(1.0 / 60.0).expect("tick 1");
    let tick = host.advance(1.0 / 60.0).expect("tick 2");

    assert_eq!(tick.outputs.len(), 1);
    let expected_tick = tick.outputs[0].report.identity.simulation_tick;
    assert!(
        expected_tick > 1,
        "regression must advance past tick 1; got tick {expected_tick}"
    );
    let batch = slot
        .snapshot()
        .expect("slot should be readable")
        .expect("successful StateAdvance should publish materialized inputs");
    assert_eq!(batch.tick.get(), expected_tick);
    assert_eq!(batch.inputs.len(), 1);
    assert_eq!(
        batch.inputs[0].tick.get(),
        expected_tick,
        "late sampler event ticks must be normalized with the materialized batch tick"
    );
}

#[test]
fn live_engine_host_latency_uses_state_advance_materialized_input_without_input_subsystem() {
    const EVENT_AGE_NANOS: u64 = 500_000;

    let snapshot = stable_region_snapshot_handle(&SmolStr::new("live_late_latency"));
    let runtime = EngineFrameRuntime::new(Box::new(NoopStateAdvanceExecutor));
    let policy = EngineFrameRuntimePolicy::closure();
    let config = LiveProjectConfig {
        scenario_id: "late_latency".to_string(),
        default_query_requests: Vec::new(),
        simulation_hz_override: None,
    };
    let sampler = Arc::new(DeadlineOffsetLateSampler::default());
    let mut host =
        LiveEngineHost::with_late_sampler(runtime, config, policy, snapshot, 60.0, sampler.clone());
    let wall_step_nanos = host.current_clock.wall_clock.get() - host.wall_nanos;
    host.wall_nanos = u64::MAX
        .saturating_sub(wall_step_nanos)
        .saturating_sub(1_000);

    assert!(
        host.subsystems().is_empty(),
        "test must not register InputSubsystemAdapter"
    );
    let tick = host.advance(1.0 / 60.0).expect("one tick");

    assert_eq!(tick.outputs.len(), 1);
    let output = &tick.outputs[0];
    assert_eq!(
        output
            .report
            .state_advance
            .as_ref()
            .expect("state advance report")
            .input_count,
        1
    );
    let events = sampler.events.lock().expect("events");
    let (deadline, arrival) = events[0];
    assert_eq!(deadline.saturating_sub(arrival), EVENT_AGE_NANOS);
    assert!(
        deadline > u64::MAX - 10_000,
        "test must exercise a near-u64::MAX sample deadline; got {deadline}"
    );
    assert_eq!(
        output.report.latency.event_arrival_to_state_advance_nanos, EVENT_AGE_NANOS,
        "latency should be input arrival to sample deadline, not deadline plus span offset; got {}ns",
        output.report.latency.event_arrival_to_state_advance_nanos
    );
}

#[test]
fn live_engine_host_flags_future_input_timestamps() {
    let snapshot = stable_region_snapshot_handle(&SmolStr::new("live_future_input"));
    let runtime = EngineFrameRuntime::new(Box::new(NoopStateAdvanceExecutor));
    let policy = EngineFrameRuntimePolicy::closure();
    let config = LiveProjectConfig {
        scenario_id: "future_input".to_string(),
        default_query_requests: Vec::new(),
        simulation_hz_override: None,
    };
    let sampler = Arc::new(FutureTimestampLateSampler::default());
    let mut host =
        LiveEngineHost::with_late_sampler(runtime, config, policy, snapshot, 60.0, sampler.clone());

    let tick = host.advance(1.0 / 60.0).expect("one tick");

    assert_eq!(tick.outputs.len(), 1);
    let events = sampler.events.lock().expect("events");
    let (deadline, arrival) = events[0];
    assert!(arrival > deadline);
    assert!(
        tick.outputs[0]
            .report
            .violations
            .contains(&"latency.input_timestamp_after_sample".to_string()),
        "expected future timestamp violation, got {:?}",
        tick.outputs[0].report.violations
    );
    assert!(
        tick.outputs[0]
            .report
            .active_degradations
            .contains(&"latency.input_timestamp_domain_invalid".to_string()),
        "expected future timestamp degradation, got {:?}",
        tick.outputs[0].report.active_degradations
    );
}

#[test]
fn live_engine_host_flags_mixed_past_and_future_input_timestamps() {
    let snapshot = stable_region_snapshot_handle(&SmolStr::new("live_mixed_input"));
    let runtime = EngineFrameRuntime::new(Box::new(NoopStateAdvanceExecutor));
    let policy = EngineFrameRuntimePolicy::closure();
    let config = LiveProjectConfig {
        scenario_id: "mixed_input".to_string(),
        default_query_requests: Vec::new(),
        simulation_hz_override: None,
    };
    let sampler = Arc::new(MixedTimestampLateSampler::default());
    let mut host =
        LiveEngineHost::with_late_sampler(runtime, config, policy, snapshot, 60.0, sampler.clone());

    let tick = host.advance(1.0 / 60.0).expect("one tick");

    assert_eq!(tick.outputs.len(), 1);
    let events = sampler.events.lock().expect("events");
    assert!(events.iter().any(|(deadline, arrival)| arrival < deadline));
    assert!(events.iter().any(|(deadline, arrival)| arrival > deadline));
    assert_eq!(
        tick.outputs[0]
            .report
            .latency
            .event_arrival_to_state_advance_nanos,
        500_000,
        "valid past event should still drive stage-1 latency"
    );
    assert!(
        tick.outputs[0]
            .report
            .violations
            .contains(&"latency.input_timestamp_after_sample".to_string()),
        "expected mixed future timestamp violation, got {:?}",
        tick.outputs[0].report.violations
    );
    assert!(
        tick.outputs[0]
            .report
            .active_degradations
            .contains(&"latency.input_timestamp_domain_invalid".to_string()),
        "expected mixed future timestamp degradation, got {:?}",
        tick.outputs[0].report.active_degradations
    );
}

struct DependentLoggingAdapter {
    log: Arc<Mutex<Vec<&'static str>>>,
}

impl EngineSubsystemAdapter for DependentLoggingAdapter {
    fn build(
        &mut self,
        builder: &mut wrela::engine_frame::EngineGraphBuilder,
    ) -> Result<EngineSubsystemPlan, EngineFrameError> {
        let descriptor = EngineSubsystemDescriptor {
            kind: EngineSubsystemKind::Presentation,
            label: "dependent".to_string(),
            runs_after: vec![EngineSubsystemKind::StateAdvance],
            requires_gpu: false,
            allows_hot_path_readback: false,
        };
        let log = Arc::clone(&self.log);
        let job = builder.add_job(
            descriptor.kind.clone(),
            "dependent.noop".to_string(),
            EngineJobAffinity::Cpu,
            EngineSpanDomain::Cpu,
            Vec::new(),
            false,
            move || {
                log.lock().expect("log").push("ran");
                Ok(())
            },
        );
        Ok(EngineSubsystemPlan::new(
            descriptor.clone(),
            vec![job],
            vec![job],
            move |_timeline: &EngineFrameTimeline, _ctx: &mut EngineFrameContext| {
                Ok(EngineSubsystemReport {
                    kind: descriptor.kind.clone(),
                    label: descriptor.label.clone(),
                    work_items: 0,
                    cpu_critical_path_micros: 0,
                    gpu_critical_path_micros: None,
                    executed_wall_time_micros: 0,
                    self_reported_runtime_micros: None,
                    orchestration_gap_micros: 0,
                    measurement_policy: EngineMeasurementPolicy {
                        runtime_source: EngineRuntimeSource::TimelineSpans,
                        gpu_timing: EngineGpuTimingPolicy::Disabled,
                        hot_path_readback_allowed: false,
                        export_readback_allowed: false,
                    },
                    queue_submit_count: 0,
                    hot_path_readback_bytes: 0,
                    scene_reupload_bytes: 0,
                    timestamped_pass_count: 0,
                    timing_readback_bytes: 0,
                    wait_time_micros: 0,
                    notes: Vec::new(),
                })
            },
        ))
    }
}

#[test]
fn live_engine_host_failed_state_advance_does_not_publish_inputs_or_run_dependents() {
    let snapshot = stable_region_snapshot_handle(&SmolStr::new("live_failed_state_advance"));
    let runtime = EngineFrameRuntime::new(Box::new(FailingStateAdvanceExecutor));
    let slot = runtime.materialized_tick_input_slot();
    let policy = EngineFrameRuntimePolicy::closure();
    let config = LiveProjectConfig {
        scenario_id: "failed_state_advance".to_string(),
        default_query_requests: Vec::new(),
        simulation_hz_override: None,
    };
    let sampler = Arc::new(DeadlineOffsetLateSampler::default());
    let log = Arc::new(Mutex::new(Vec::new()));
    let mut host =
        LiveEngineHost::with_late_sampler(runtime, config, policy, snapshot, 60.0, sampler);
    host.add_subsystem(Box::new(DependentLoggingAdapter {
        log: Arc::clone(&log),
    }));

    let err = host
        .advance(1.0 / 60.0)
        .expect_err("state advance failure should fail the frame");

    assert!(format!("{err}").contains("intentional state advance failure"));
    assert!(
        slot.snapshot().expect("slot should be readable").is_none(),
        "failed state advance must not leave materialized inputs published"
    );
    assert!(
        log.lock().expect("log").is_empty(),
        "dependent subsystems must not run after failed state advance"
    );
}

#[test]
fn live_engine_host_disallowed_state_advance_does_not_publish_inputs_or_run_dependents() {
    let snapshot = stable_region_snapshot_handle(&SmolStr::new("live_disallowed_state_advance"));
    let runtime = EngineFrameRuntime::new(Box::new(BehaviorStateAdvanceExecutor));
    let slot = runtime.materialized_tick_input_slot();
    let policy = EngineFrameRuntimePolicy::closure();
    let config = LiveProjectConfig {
        scenario_id: "disallowed_state_advance".to_string(),
        default_query_requests: Vec::new(),
        simulation_hz_override: None,
    };
    let sampler = Arc::new(DeadlineOffsetLateSampler::default());
    let log = Arc::new(Mutex::new(Vec::new()));
    let mut host =
        LiveEngineHost::with_late_sampler(runtime, config, policy, snapshot, 60.0, sampler);
    host.add_subsystem(Box::new(DependentLoggingAdapter {
        log: Arc::clone(&log),
    }));

    let err = host
        .advance(1.0 / 60.0)
        .expect_err("disallowed state advance should fail the frame");

    assert!(format!("{err}").contains("state advance change is incompatible"));
    assert!(
        slot.snapshot().expect("slot should be readable").is_none(),
        "disallowed state advance must not leave materialized inputs published"
    );
    assert!(
        log.lock().expect("log").is_empty(),
        "dependent subsystems must not run after disallowed state advance"
    );
}

#[test]
fn motion_to_photon_over_budget_appends_violation() {
    let mut policy = EngineFrameRuntimePolicy::live();
    policy.motion_to_photon_target_ms = Some(16.0);
    let mut latency = MotionToPhotonContract::synthetic_idle();
    latency.event_arrival_to_state_advance_nanos = 5_000_000;
    latency.state_advance_to_render_submit_nanos = 5_000_000;
    latency.render_submit_to_gpu_complete_nanos = 5_000_000;
    latency.gpu_complete_to_present_callback_nanos = 5_000_000;
    latency.estimated_present_to_photons_nanos = 0;
    latency.recompute_total();
    let mut report = EngineFrameReport {
        scenario_id: "t".into(),
        frame_index: 0,
        identity: Default::default(),
        state_advance: None,
        resource_ledger: Default::default(),
        readback_ledger: Default::default(),
        query_ledger: Default::default(),
        gpu_frame_ledger: Default::default(),
        budget_directives: Default::default(),
        frame_wall_time_micros: 0,
        cpu_critical_path_micros: 0,
        gpu_critical_path_micros: None,
        present_wait_micros: 0,
        gpu_wait_micros: 0,
        readback_wait_micros: 0,
        steady_state_fps: 0.0,
        gpu_runtime: GpuRuntimeMetrics::default(),
        timeline_version: wrela::engine_frame::ENGINE_FRAME_TIMELINE_VERSION,
        critical_path_span_ids: Vec::new(),
        cpu_busy_micros: 0,
        gpu_busy_micros: 0,
        overlap_ratio: 0.0,
        queue_submission_spans: Vec::new(),
        subsystem_span_ranges: Vec::new(),
        timeline_spans: Vec::new(),
        subsystems: Vec::new(),
        future_subsystem_reserve: Default::default(),
        active_degradations: Vec::new(),
        violations: Vec::new(),
        latency,
        closure_findings: Vec::new(),
    };
    apply_latency_budget_to_report(&policy, &mut report);
    assert!(
        report
            .violations
            .iter()
            .any(|v| v == "presentation.motion_to_photon_over_budget")
    );
    assert!(
        report
            .active_degradations
            .iter()
            .any(|d| d.contains("latency")),
        "expected latency degradation hint: {:?}",
        report.active_degradations
    );
    assert!(
        report
            .closure_findings
            .iter()
            .any(|finding| finding.focus == "motion_to_photon_budget"),
        "expected structured latency closure finding: {:?}",
        report.closure_findings
    );
}

#[test]
fn live_budget_application_promotes_all_known_runtime_violations() {
    let policy = EngineFrameRuntimePolicy::live();
    let mut report = EngineFrameReport {
        scenario_id: "runtime_violations".into(),
        frame_index: 0,
        identity: Default::default(),
        state_advance: None,
        resource_ledger: Default::default(),
        readback_ledger: Default::default(),
        query_ledger: Default::default(),
        gpu_frame_ledger: Default::default(),
        budget_directives: Default::default(),
        frame_wall_time_micros: 0,
        cpu_critical_path_micros: 0,
        gpu_critical_path_micros: None,
        present_wait_micros: 0,
        gpu_wait_micros: 0,
        readback_wait_micros: 0,
        steady_state_fps: 0.0,
        gpu_runtime: GpuRuntimeMetrics::default(),
        timeline_version: wrela::engine_frame::ENGINE_FRAME_TIMELINE_VERSION,
        critical_path_span_ids: Vec::new(),
        cpu_busy_micros: 0,
        gpu_busy_micros: 0,
        overlap_ratio: 0.0,
        queue_submission_spans: Vec::new(),
        subsystem_span_ranges: Vec::new(),
        timeline_spans: Vec::new(),
        subsystems: Vec::new(),
        future_subsystem_reserve: Default::default(),
        active_degradations: Vec::new(),
        violations: vec![
            "audio.underrun".into(),
            "physics.contact_readback_over_budget".into(),
        ],
        latency: MotionToPhotonContract::default(),
        closure_findings: Vec::new(),
    };
    apply_latency_budget_to_report(&policy, &mut report);
    let focuses = report
        .closure_findings
        .iter()
        .map(|finding| finding.focus.as_str())
        .collect::<Vec<_>>();
    assert!(focuses.contains(&"audio.underrun"), "{focuses:?}");
    assert!(
        focuses.contains(&"physics.contact_readback_over_budget"),
        "{focuses:?}"
    );
}

#[test]
fn two_headless_live_hosts_match_snapshot_epochs_per_tick() {
    let snap = stable_region_snapshot_handle(&SmolStr::new("lockstep_host"));
    let mut left = LiveEngineHost::new_headless(
        EngineFrameRuntime::new(Box::new(NoopStateAdvanceExecutor)),
        LiveProjectConfig {
            scenario_id: "a".into(),
            default_query_requests: Vec::new(),
            simulation_hz_override: None,
        },
        EngineFrameRuntimePolicy::closure(),
        snap.clone(),
        60.0,
    );
    let mut right = LiveEngineHost::new_headless(
        EngineFrameRuntime::new(Box::new(NoopStateAdvanceExecutor)),
        LiveProjectConfig {
            scenario_id: "b".into(),
            default_query_requests: Vec::new(),
            simulation_hz_override: None,
        },
        EngineFrameRuntimePolicy::closure(),
        snap,
        60.0,
    );
    let step = 1.0 / 60.0;
    for _ in 0..16 {
        let la = left.advance(step).expect("left");
        let rb = right.advance(step).expect("right");
        assert_eq!(la.outputs.len(), rb.outputs.len());
        assert_eq!(
            la.outputs[0].snapshot.epoch().0,
            rb.outputs[0].snapshot.epoch().0
        );
        assert_eq!(
            la.outputs[0].report.identity.simulation_tick,
            rb.outputs[0].report.identity.simulation_tick
        );
        assert_eq!(
            la.outputs[0].report.identity.wall_clock,
            rb.outputs[0].report.identity.wall_clock
        );
    }
}

struct DummyPresentationAdapter;

impl EngineSubsystemAdapter for DummyPresentationAdapter {
    fn build(
        &mut self,
        builder: &mut wrela::engine_frame::EngineGraphBuilder,
    ) -> Result<EngineSubsystemPlan, EngineFrameError> {
        let descriptor = EngineSubsystemDescriptor {
            kind: EngineSubsystemKind::Presentation,
            label: "p0".to_string(),
            runs_after: Vec::new(),
            requires_gpu: false,
            allows_hot_path_readback: false,
        };
        let job = builder.add_job(
            EngineSubsystemKind::Presentation,
            "p0.noop".to_string(),
            EngineJobAffinity::Cpu,
            EngineSpanDomain::Cpu,
            Vec::new(),
            false,
            || Ok(()),
        );
        Ok(EngineSubsystemPlan::new(
            descriptor,
            vec![job],
            vec![job],
            |_timeline: &EngineFrameTimeline, _ctx: &mut EngineFrameContext| {
                Ok(EngineSubsystemReport {
                    kind: EngineSubsystemKind::Presentation,
                    label: "p0".into(),
                    work_items: 0,
                    cpu_critical_path_micros: 0,
                    gpu_critical_path_micros: None,
                    executed_wall_time_micros: 0,
                    self_reported_runtime_micros: None,
                    orchestration_gap_micros: 0,
                    measurement_policy: EngineMeasurementPolicy {
                        runtime_source: EngineRuntimeSource::ReservedSlotUnsampled,
                        gpu_timing: EngineGpuTimingPolicy::Disabled,
                        hot_path_readback_allowed: false,
                        export_readback_allowed: false,
                    },
                    queue_submit_count: 0,
                    hot_path_readback_bytes: 0,
                    scene_reupload_bytes: 0,
                    timestamped_pass_count: 0,
                    timing_readback_bytes: 0,
                    wait_time_micros: 0,
                    notes: Vec::new(),
                })
            },
        ))
    }
}

#[test]
fn engine_frame_scheduler_rejects_duplicate_subsystem_kind() {
    let mut scheduler = EngineFrameScheduler::default();
    let mut adapters: Vec<Box<dyn EngineSubsystemAdapter>> = vec![
        Box::new(DummyPresentationAdapter),
        Box::new(DummyPresentationAdapter),
    ];
    let err = scheduler
        .run_frame("dup_kind", 0, &mut adapters)
        .expect_err("duplicate presentation");
    match err {
        EngineFrameError::DuplicateSubsystemKind(k) => {
            assert_eq!(k, EngineSubsystemKind::Presentation)
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

/// RFC 0011 M7: `validate_unique_subsystem_kinds` must consider
/// `FutureReserve(name)` payloads, not just the variant tag. Two distinct
/// reserve names are allowed, two identical reserve names are not.
struct DummyFutureReserveAdapter {
    name: &'static str,
}

impl EngineSubsystemAdapter for DummyFutureReserveAdapter {
    fn build(
        &mut self,
        builder: &mut wrela::engine_frame::EngineGraphBuilder,
    ) -> Result<EngineSubsystemPlan, EngineFrameError> {
        let kind = EngineSubsystemKind::FutureReserve(self.name.to_string());
        let descriptor = EngineSubsystemDescriptor {
            kind: kind.clone(),
            label: self.name.to_string(),
            runs_after: Vec::new(),
            requires_gpu: false,
            allows_hot_path_readback: false,
        };
        let job = builder.add_job(
            kind.clone(),
            format!("{}.noop", self.name),
            EngineJobAffinity::Cpu,
            EngineSpanDomain::Cpu,
            Vec::new(),
            false,
            || Ok(()),
        );
        let report_kind = kind.clone();
        let report_label = self.name.to_string();
        Ok(EngineSubsystemPlan::new(
            descriptor,
            vec![job],
            vec![job],
            move |_timeline: &EngineFrameTimeline, _ctx: &mut EngineFrameContext| {
                Ok(EngineSubsystemReport {
                    kind: report_kind.clone(),
                    label: report_label.clone(),
                    work_items: 0,
                    cpu_critical_path_micros: 0,
                    gpu_critical_path_micros: None,
                    executed_wall_time_micros: 0,
                    self_reported_runtime_micros: None,
                    orchestration_gap_micros: 0,
                    measurement_policy: EngineMeasurementPolicy {
                        runtime_source: EngineRuntimeSource::ReservedSlotUnsampled,
                        gpu_timing: EngineGpuTimingPolicy::Disabled,
                        hot_path_readback_allowed: false,
                        export_readback_allowed: false,
                    },
                    queue_submit_count: 0,
                    hot_path_readback_bytes: 0,
                    scene_reupload_bytes: 0,
                    timestamped_pass_count: 0,
                    timing_readback_bytes: 0,
                    wait_time_micros: 0,
                    notes: Vec::new(),
                })
            },
        ))
    }
}

#[test]
fn engine_frame_scheduler_allows_distinct_future_reserve_names() {
    let mut scheduler = EngineFrameScheduler::default();
    let mut adapters: Vec<Box<dyn EngineSubsystemAdapter>> = vec![
        Box::new(DummyFutureReserveAdapter { name: "reserve_a" }),
        Box::new(DummyFutureReserveAdapter { name: "reserve_b" }),
    ];
    scheduler
        .run_frame("reserve_unique", 0, &mut adapters)
        .expect("distinct future reserves should validate");
}

#[test]
fn engine_frame_scheduler_rejects_duplicate_future_reserve_names() {
    let mut scheduler = EngineFrameScheduler::default();
    let mut adapters: Vec<Box<dyn EngineSubsystemAdapter>> = vec![
        Box::new(DummyFutureReserveAdapter {
            name: "reserve_dup",
        }),
        Box::new(DummyFutureReserveAdapter {
            name: "reserve_dup",
        }),
    ];
    let err = scheduler
        .run_frame("reserve_dup", 0, &mut adapters)
        .expect_err("duplicate future reserve name");
    match err {
        EngineFrameError::DuplicateSubsystemKind(EngineSubsystemKind::FutureReserve(name)) => {
            assert_eq!(name, "reserve_dup");
        }
        other => panic!("unexpected error: {other:?}"),
    }
}
