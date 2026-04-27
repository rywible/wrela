use std::collections::BTreeMap;

use ciborium::value::Value;
use smol_str::SmolStr;
use wrela::engine_frame::{
    EngineFrameRuntime, EngineFrameRuntimePolicy, EngineGpuTimingPolicy, EngineGraphBuilder,
    EngineJobAffinity, EngineMeasurementPolicy, EngineRuntimeSource, EngineSpanDomain,
    EngineStateAdvanceExecutor, EngineSubsystemAdapter, EngineSubsystemDescriptor,
    EngineSubsystemKind, EngineSubsystemPlan, SavePublisher,
};
use wrela::persistence::{PersistenceProject, PersistentHandle, SnapshotLedgerRecord};
use wrela::query_exec::stable_region_snapshot_handle;
use wrela::state_advance::{
    ChangeClass, ChangeSummary, StateAdvanceResult, TickInputBatch, WorldTransitionRecord,
};
use wrela::time_semantics::{PresentationFrame, SimulationTick, TemporalClock, WallClockStamp};

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
            ChangeSummary::new(ChangeClass::None, "save adapter test"),
        ))
    }
}

struct DummyPresentation;

impl EngineSubsystemAdapter for DummyPresentation {
    fn build(
        &mut self,
        builder: &mut EngineGraphBuilder,
    ) -> Result<EngineSubsystemPlan, wrela::engine_frame::EngineFrameError> {
        let job = builder.add_job(
            EngineSubsystemKind::Presentation,
            "presentation.noop",
            EngineJobAffinity::Cpu,
            EngineSpanDomain::Cpu,
            Vec::new(),
            false,
            || Ok(()),
        );
        Ok(EngineSubsystemPlan::new(
            EngineSubsystemDescriptor {
                kind: EngineSubsystemKind::Presentation,
                label: "presentation".into(),
                runs_after: vec![EngineSubsystemKind::StateAdvance],
                requires_gpu: false,
                allows_hot_path_readback: false,
            },
            vec![job],
            vec![job],
            |_timeline, _ctx| {
                Ok(wrela::engine_frame::EngineSubsystemReport {
                    kind: EngineSubsystemKind::Presentation,
                    label: "presentation".into(),
                    work_items: 1,
                    cpu_critical_path_micros: 0,
                    gpu_critical_path_micros: None,
                    executed_wall_time_micros: 0,
                    self_reported_runtime_micros: Some(0),
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
fn save_publisher_runs_after_state_advance_without_presentation_and_publishes_record() {
    let mut runtime = EngineFrameRuntime::new(Box::new(NoopStateAdvanceExecutor));
    let snapshot = stable_region_snapshot_handle(&SmolStr::new("save_adapter"));
    let project = PersistenceProject {
        project_id: "save_adapter".into(),
        wrela_version: "test".into(),
        engine_compatibility_hash: 1,
        generator_compatibility_hashes: BTreeMap::new(),
        archetype_schema_hashes: BTreeMap::new(),
    };
    let save = SavePublisher::new(
        true,
        snapshot.clone(),
        project,
        1,
        1,
        vec![SnapshotLedgerRecord {
            handle: PersistentHandle::from_stable_semantic_parts(&[b"T", b"one"]),
            type_id: "T".into(),
            payload: Value::Bytes(vec![1]),
        }],
    );
    let record = save.record();
    let tick = SimulationTick::new(1);
    let output = runtime
        .run_frame_with_subsystems(
            wrela::engine_frame::EngineFrameInput {
                scenario_id: "save_adapter".into(),
                frame_index: 0,
                previous_snapshot: snapshot.clone(),
                previous_clock: TemporalClock::new(
                    wrela::time_semantics::SnapshotEpoch::new(snapshot.epoch().0),
                    SimulationTick::new(0),
                    PresentationFrame::new(0),
                    WallClockStamp::new(0),
                ),
                current_clock: TemporalClock::new(
                    wrela::time_semantics::SnapshotEpoch::new(snapshot.epoch().0 + 1),
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
            vec![Box::new(save)],
        )
        .expect("frame");
    assert!(record.lock().expect("record").is_some());
    assert!(
        output
            .report
            .subsystem(EngineSubsystemKind::Save)
            .expect("save report")
            .work_items
            > 0
    );
    assert!(output.report.resource_ledger.accesses.iter().any(|access| {
        access.subsystem == EngineSubsystemKind::Save
            && access.mode == wrela::engine_frame::EngineResourceAccessMode::Write
            && matches!(
                access.resource,
                wrela::engine_frame::EngineResourceId::SaveRecord { .. }
            )
    }));
}

#[test]
fn save_publisher_headless_state_outcome_serializes_state_advance_output_epoch() {
    let mut runtime = EngineFrameRuntime::new(Box::new(NoopStateAdvanceExecutor));
    let snapshot = stable_region_snapshot_handle(&SmolStr::new("save_adapter_output_epoch"));
    let project = PersistenceProject {
        project_id: "save_adapter_output_epoch".into(),
        wrela_version: "test".into(),
        engine_compatibility_hash: 1,
        generator_compatibility_hashes: BTreeMap::new(),
        archetype_schema_hashes: BTreeMap::new(),
    };
    let save = SavePublisher::with_state_outcome_headless(
        true,
        runtime.state_advance_outcome_slot(),
        project,
        1,
        1,
        Vec::new(),
    );
    let record = save.record();
    let tick = SimulationTick::new(1);
    let output = runtime
        .run_frame_with_subsystems(
            wrela::engine_frame::EngineFrameInput {
                scenario_id: "save_adapter_output_epoch".into(),
                frame_index: 0,
                previous_snapshot: snapshot.clone(),
                previous_clock: TemporalClock::new(
                    wrela::time_semantics::SnapshotEpoch::new(snapshot.epoch().0),
                    SimulationTick::new(0),
                    PresentationFrame::new(0),
                    WallClockStamp::new(0),
                ),
                current_clock: TemporalClock::new(
                    wrela::time_semantics::SnapshotEpoch::new(snapshot.epoch().0 + 1),
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
            vec![Box::new(save)],
        )
        .expect("frame");
    let saved_epoch = record
        .lock()
        .expect("record")
        .as_ref()
        .expect("save record")
        .header
        .snapshot_epoch;

    assert_eq!(saved_epoch, output.snapshot.epoch().0);
    assert_ne!(saved_epoch, snapshot.epoch().0);
}

#[test]
fn save_publisher_with_state_outcome_runs_after_presentation() {
    let mut runtime = EngineFrameRuntime::new(Box::new(NoopStateAdvanceExecutor));
    let snapshot = stable_region_snapshot_handle(&SmolStr::new("save_adapter_with_presentation"));
    let project = PersistenceProject {
        project_id: "save_adapter_with_presentation".into(),
        wrela_version: "test".into(),
        engine_compatibility_hash: 1,
        generator_compatibility_hashes: BTreeMap::new(),
        archetype_schema_hashes: BTreeMap::new(),
    };
    let save = SavePublisher::with_state_outcome(
        true,
        runtime.state_advance_outcome_slot(),
        project,
        1,
        1,
        Vec::new(),
    );
    let record = save.record();
    let tick = SimulationTick::new(1);
    let output = runtime
        .run_frame_with_subsystems(
            wrela::engine_frame::EngineFrameInput {
                scenario_id: "save_adapter_with_presentation".into(),
                frame_index: 0,
                previous_snapshot: snapshot.clone(),
                previous_clock: TemporalClock::new(
                    wrela::time_semantics::SnapshotEpoch::new(snapshot.epoch().0),
                    SimulationTick::new(0),
                    PresentationFrame::new(0),
                    WallClockStamp::new(0),
                ),
                current_clock: TemporalClock::new(
                    wrela::time_semantics::SnapshotEpoch::new(snapshot.epoch().0 + 1),
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
            vec![Box::new(DummyPresentation), Box::new(save)],
        )
        .expect("frame");
    assert!(output.report.subsystem(EngineSubsystemKind::Save).is_some());
    assert!(
        output
            .report
            .subsystem(EngineSubsystemKind::Presentation)
            .is_some()
    );
    let presentation_end = output
        .report
        .subsystem_span_ranges
        .iter()
        .find(|range| range.kind == EngineSubsystemKind::Presentation)
        .expect("presentation span range")
        .end_span_id
        .expect("presentation end span");
    let save_start = output
        .report
        .subsystem_span_ranges
        .iter()
        .find(|range| range.kind == EngineSubsystemKind::Save)
        .expect("save span range")
        .start_span_id
        .expect("save start span");
    assert!(save_start.0 > presentation_end.0);
    assert_eq!(
        record
            .lock()
            .expect("record")
            .as_ref()
            .expect("save record")
            .header
            .snapshot_epoch,
        output.snapshot.epoch().0
    );
}
