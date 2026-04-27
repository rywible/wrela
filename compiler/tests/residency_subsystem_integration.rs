use smol_str::SmolStr;
use wrela::engine_frame::{
    EngineFrameRuntime, EngineFrameRuntimePolicy, EngineStateAdvanceExecutor, EngineSubsystemKind,
    InputSubsystemAdapter, ResidencySubsystemAdapter, SystemSubsystemAdapter,
};
use wrela::input_map_plan::InputMapPlan;
use wrela::query_exec::stable_region_snapshot_handle;
use wrela::residency::follow::{FollowTarget, Transform3};
use wrela::residency::{
    RegionId, RegionLine, RegionResidencyService, ResidencyCandidate, ResidencyPolicy,
};
use wrela::state_advance::{
    ChangeClass, ChangeSummary, StateAdvanceResult, TickInputBatch, WorldTransitionRecord,
};
use wrela::system_plan::SystemProgram;
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
            ChangeSummary::new(ChangeClass::None, "residency adapter test"),
        ))
    }
}

#[test]
fn residency_subsystem_reports_spans_and_resource_state() {
    let mut runtime = EngineFrameRuntime::new(Box::new(NoopStateAdvanceExecutor));
    let previous_snapshot = stable_region_snapshot_handle(&SmolStr::new("residency_adapter"));
    let topology = RegionLine {
        regions: vec![ResidencyCandidate {
            region_id: RegionId::new("near"),
            center: [0.0, 0.0, 0.0],
            bytes: 128,
            compatibility_hash: 1,
        }],
    };
    let service = RegionResidencyService::new(
        ResidencyPolicy {
            candidate_window: 10.0,
            ..ResidencyPolicy::default()
        },
        Box::new(topology),
    );
    let residency = ResidencySubsystemAdapter::new(
        service,
        FollowTarget {
            transform: Transform3 {
                translation: [0.0, 0.0, 0.0],
            },
            velocity: None,
        },
        previous_snapshot.clone(),
        SimulationTick::new(1),
    );
    let input_adapter = InputSubsystemAdapter::new(
        InputMapPlan::empty("empty"),
        runtime.materialized_tick_input_slot(),
    );
    let empty_systems = SystemSubsystemAdapter::new(
        SystemProgram::new(Vec::new()).expect("empty systems"),
        input_adapter.shared_frame(),
    );
    let tick = SimulationTick::new(1);
    let output = runtime
        .run_frame_with_subsystems(
            wrela::engine_frame::EngineFrameInput {
                scenario_id: "residency_adapter".into(),
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
                Box::new(residency),
            ],
        )
        .expect("frame");
    let report = output
        .report
        .subsystem(EngineSubsystemKind::Residency)
        .expect("residency report");
    assert_eq!(report.work_items, 1);
    assert!(report.scene_reupload_bytes > 0);
    assert!(output.report.resource_ledger.states.iter().any(|state| {
        matches!(
            &state.resource,
            wrela::engine_frame::EngineResourceId::ResidentRegion { region_id, .. }
                if region_id == "near"
        )
    }));
    assert!(!output.report.resource_ledger.states.iter().any(|state| {
        matches!(
            &state.resource,
            wrela::engine_frame::EngineResourceId::ResidentRegion { region_id, .. }
                if region_id == "*"
        )
    }));
}
