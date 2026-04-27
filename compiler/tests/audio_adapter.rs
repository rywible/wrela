use std::sync::Arc;

use smol_str::SmolStr;
use wrela::audio_exec::{AudioSnapshotPublisher as AudioPublisher, sine_voice};
use wrela::audio_plan::{AudioConfig, AudioDspPlan};
use wrela::engine_frame::{
    AudioSnapshotPublisher, EngineFrameContext, EngineFrameError, EngineFrameRuntime,
    EngineFrameRuntimePolicy, EngineFrameTimeline, EngineGpuTimingPolicy, EngineGraphBuilder,
    EngineJobAffinity, EngineMeasurementPolicy, EngineRuntimeSource, EngineSpanDomain,
    EngineStateAdvanceExecutor, EngineSubsystemAdapter, EngineSubsystemDescriptor,
    EngineSubsystemKind, EngineSubsystemPlan, EngineSubsystemReport, InputSubsystemAdapter,
    SystemSubsystemAdapter,
};
use wrela::input_map_plan::InputMapPlan;
use wrela::query_exec::stable_region_snapshot_handle;
use wrela::state_advance::{
    ChangeClass, ChangeSummary, StateAdvanceResult, TickInputBatch, WorldTransitionRecord,
};
use wrela::system_plan::SystemProgram;
use wrela::time_semantics::{PresentationFrame, SimulationTick, TemporalClock, WallClockStamp};
use wrela_runtime::audio::voice::VoiceLedger;

#[derive(Default)]
struct NoopStateAdvanceExecutor;

impl EngineStateAdvanceExecutor for NoopStateAdvanceExecutor {
    fn advance(
        &mut self,
        input: wrela::engine_frame::EngineStateAdvanceInput,
    ) -> Result<StateAdvanceResult, wrela::engine_frame::EngineFrameError> {
        let previous = input.previous_snapshot.clone();
        let next =
            previous.with_epoch(wrela::world_identity::SnapshotEpoch(previous.epoch().0 + 1));
        Ok(StateAdvanceResult::new(
            WorldTransitionRecord::new(
                Some(previous),
                next,
                Some(input.previous_clock),
                input.current_clock,
                input.inputs,
                Vec::new(),
            ),
            ChangeSummary::new(ChangeClass::None, "audio adapter test"),
        ))
    }
}

struct StubPhysicsAdapter;

impl EngineSubsystemAdapter for StubPhysicsAdapter {
    fn build(
        &mut self,
        builder: &mut EngineGraphBuilder,
    ) -> Result<EngineSubsystemPlan, EngineFrameError> {
        let descriptor = EngineSubsystemDescriptor {
            kind: EngineSubsystemKind::Physics,
            label: "physics".to_string(),
            runs_after: vec![
                EngineSubsystemKind::StateAdvance,
                EngineSubsystemKind::System,
            ],
            requires_gpu: false,
            allows_hot_path_readback: false,
        };
        let job = builder.add_synthetic_job(
            EngineSubsystemKind::Physics,
            "physics.stub",
            EngineJobAffinity::Cpu,
            EngineSpanDomain::Cpu,
            Vec::new(),
            false,
            10,
        );
        Ok(EngineSubsystemPlan::new(
            descriptor,
            vec![job],
            vec![job],
            |_timeline: &EngineFrameTimeline, _ctx: &mut EngineFrameContext| {
                Ok(EngineSubsystemReport {
                    kind: EngineSubsystemKind::Physics,
                    label: "physics".to_string(),
                    work_items: 1,
                    cpu_critical_path_micros: 10,
                    gpu_critical_path_micros: None,
                    executed_wall_time_micros: 10,
                    self_reported_runtime_micros: Some(10),
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
fn audio_snapshot_publisher_reports_voice_ledger_resource() {
    let mut runtime = EngineFrameRuntime::new(Box::new(NoopStateAdvanceExecutor));
    let input_adapter = InputSubsystemAdapter::new(
        InputMapPlan::empty("empty"),
        runtime.materialized_tick_input_slot(),
    );
    let empty_systems = SystemSubsystemAdapter::new(
        SystemProgram::new(Vec::new()).expect("empty systems"),
        input_adapter.shared_frame(),
    );
    let ledger = Arc::new(VoiceLedger::new());
    let publisher = AudioPublisher::new(
        AudioConfig {
            max_voices: 1,
            ..AudioConfig::default()
        },
        ledger,
    );
    let audio = AudioSnapshotPublisher::new(
        publisher,
        AudioDspPlan {
            voices: vec![sine_voice(1, 5, 1.0), sine_voice(2, 1, 1.0)],
        },
        1,
    );
    let previous_snapshot = stable_region_snapshot_handle(&SmolStr::new("audio_adapter"));
    let tick = SimulationTick::new(1);
    let output = runtime
        .run_frame_with_subsystems(
            wrela::engine_frame::EngineFrameInput {
                scenario_id: "audio_adapter".into(),
                frame_index: 0,
                previous_snapshot: previous_snapshot.clone(),
                previous_clock: TemporalClock::new(
                    wrela::time_semantics::SnapshotEpoch::new(previous_snapshot.epoch().0),
                    SimulationTick::new(0),
                    PresentationFrame::new(0),
                    WallClockStamp::new(0),
                ),
                current_clock: TemporalClock::new(
                    wrela::time_semantics::SnapshotEpoch::new(previous_snapshot.epoch().0 + 1),
                    tick,
                    PresentationFrame::new(1),
                    WallClockStamp::new(16_666),
                ),
                tick_inputs: wrela::engine_frame::TickInputSource::eager(TickInputBatch::new(
                    tick,
                    Vec::new(),
                )),
                policy: EngineFrameRuntimePolicy::live(),
                query_requests: Vec::new(),
                readback_requests: Vec::new(),
            },
            vec![
                Box::new(input_adapter),
                Box::new(empty_systems),
                Box::new(audio),
            ],
        )
        .expect("frame");
    let report = output
        .report
        .subsystem(EngineSubsystemKind::Audio)
        .expect("audio report");
    assert_eq!(report.work_items, 1);
    assert!(
        output
            .report
            .violations
            .iter()
            .any(|violation| violation == "audio.voice_count_over_cap")
    );
    assert!(
        output
            .report
            .resource_ledger
            .states
            .iter()
            .any(|state| matches!(
                state.resource,
                wrela::engine_frame::EngineResourceId::AudioVoiceLedger { .. }
            ))
    );
}

#[test]
fn audio_snapshot_publisher_runs_after_system_and_physics_when_configured() {
    let mut runtime = EngineFrameRuntime::new(Box::new(NoopStateAdvanceExecutor));
    let input_adapter = InputSubsystemAdapter::new(
        InputMapPlan::empty("empty"),
        runtime.materialized_tick_input_slot(),
    );
    let empty_systems = SystemSubsystemAdapter::new(
        SystemProgram::new(Vec::new()).expect("empty systems"),
        input_adapter.shared_frame(),
    );
    let ledger = Arc::new(VoiceLedger::new());
    let publisher = AudioPublisher::new(AudioConfig::default(), ledger);
    let audio = AudioSnapshotPublisher::new(
        publisher,
        AudioDspPlan {
            voices: vec![sine_voice(1, 5, 1.0)],
        },
        1,
    )
    .with_physics_dependency();
    let previous_snapshot = stable_region_snapshot_handle(&SmolStr::new("audio_order"));
    let tick = SimulationTick::new(1);

    let output = runtime
        .run_frame_with_subsystems(
            wrela::engine_frame::EngineFrameInput {
                scenario_id: "audio_order".into(),
                frame_index: 0,
                previous_snapshot: previous_snapshot.clone(),
                previous_clock: TemporalClock::new(
                    wrela::time_semantics::SnapshotEpoch::new(previous_snapshot.epoch().0),
                    SimulationTick::new(0),
                    PresentationFrame::new(0),
                    WallClockStamp::new(0),
                ),
                current_clock: TemporalClock::new(
                    wrela::time_semantics::SnapshotEpoch::new(previous_snapshot.epoch().0 + 1),
                    tick,
                    PresentationFrame::new(1),
                    WallClockStamp::new(16_666),
                ),
                tick_inputs: wrela::engine_frame::TickInputSource::eager(TickInputBatch::new(
                    tick,
                    Vec::new(),
                )),
                policy: EngineFrameRuntimePolicy::live(),
                query_requests: Vec::new(),
                readback_requests: Vec::new(),
            },
            vec![
                Box::new(input_adapter),
                Box::new(empty_systems),
                Box::new(StubPhysicsAdapter),
                Box::new(audio),
            ],
        )
        .expect("frame");

    let physics_span = output
        .report
        .timeline_spans
        .iter()
        .find(|span| span.subsystem == EngineSubsystemKind::Physics)
        .expect("physics span");
    let audio_span = output
        .report
        .timeline_spans
        .iter()
        .find(|span| span.subsystem == EngineSubsystemKind::Audio)
        .expect("audio span");
    assert!(
        audio_span.started_micros >= physics_span.ended_micros,
        "audio must publish after physics has completed"
    );
}
