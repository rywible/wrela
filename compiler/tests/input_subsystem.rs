use smol_str::SmolStr;
use wrela::engine_frame::{
    EngineFrameRuntime, EngineFrameRuntimePolicy, EngineResourceAccessMode, EngineResourceId,
    EngineStateAdvanceExecutor, InputSubsystemAdapter,
};
use wrela::input_contract::{InputMapBinding, SemanticActionState};
use wrela::input_map_plan::InputMapPlan;
use wrela::query_exec::stable_region_snapshot_handle;
use wrela::state_advance::{
    ChangeClass, ChangeSummary, StateAdvanceResult, TickInputBatch, TickInputEvent, TickInputKind,
    WorldTransitionRecord,
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
            ChangeSummary::new(ChangeClass::None, "input subsystem test"),
        ))
    }
}

#[test]
fn input_subsystem_writes_input_frame_after_state_advance() {
    let mut runtime = EngineFrameRuntime::new(Box::new(NoopStateAdvanceExecutor));
    let input_slot = runtime.materialized_tick_input_slot();
    let map = InputMapPlan::new(
        "player",
        vec![InputMapBinding::new(
            "MoveForward",
            "keyboard",
            "key.w.down",
        )],
    )
    .expect("input map");
    let adapter = InputSubsystemAdapter::new(map, input_slot);
    let shared_frame = adapter.shared_frame();
    let tick = SimulationTick::new(1);
    let previous_snapshot = stable_region_snapshot_handle(&SmolStr::new("input_subsystem"));
    let input = wrela::engine_frame::EngineFrameInput {
        scenario_id: "input_subsystem".into(),
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
            vec![TickInputEvent::with_timestamps(
                tick,
                TickInputKind::Event,
                "keyboard",
                "key.w.down",
                WallClockStamp::new(10),
                10,
            )],
        )),
        policy: EngineFrameRuntimePolicy::live(),
        query_requests: Vec::new(),
        readback_requests: Vec::new(),
    };
    let output = runtime
        .run_frame_with_subsystems(input, vec![Box::new(adapter)])
        .expect("frame");
    assert!(
        output.report.resource_ledger.accesses.iter().any(|access| {
            matches!(access.resource, EngineResourceId::InputFrame { epoch: 2 })
                && access.mode == EngineResourceAccessMode::Write
        }),
        "Input subsystem should write an InputFrame resource"
    );
    let frame = shared_frame
        .lock()
        .expect("input frame")
        .clone()
        .expect("translated frame");
    assert_eq!(frame.epoch.0, 2);
    assert!(matches!(
        frame
            .actions
            .get(&wrela::input_contract::SemanticActionId::new("MoveForward")),
        Some(SemanticActionState::Button {
            pressed: true,
            just_pressed: true,
            just_released: false
        })
    ));
}
