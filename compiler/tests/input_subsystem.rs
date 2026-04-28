use smol_str::SmolStr;
use wrela::engine_frame::{
    EngineFrameRuntime, EngineFrameRuntimePolicy, EngineResourceAccessMode, EngineResourceId,
    EngineStateAdvanceExecutor, InputSubsystemAdapter, LateInputSampler, RawInputRingLateSampler,
};
use wrela::input_contract::{InputMapBinding, SemanticActionState};
use wrela::input_map_plan::InputMapPlan;
use wrela::query_exec::stable_region_snapshot_handle;
use wrela::state_advance::{
    ChangeClass, ChangeSummary, StateAdvanceResult, TickInputBatch, TickInputEvent, TickInputKind,
    TickInputValue, WorldTransitionRecord,
};
use wrela::time_semantics::{PresentationFrame, SimulationTick, TemporalClock, WallClockStamp};
use wrela_runtime::platform::input::{RawInputKind, TimestampedRawEvent};
use wrela_runtime::platform::input_pump::RawInputRing;

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

#[test]
fn raw_input_ring_late_sampler_preserves_bindable_keyboard_detail() {
    let (mut producer, consumer) = RawInputRing::split_with_capacity(4);
    producer.push_event(TimestampedRawEvent::new(
        "keyboard",
        "key.KeyW",
        RawInputKind::Key {
            code: SmolStr::new("KeyW"),
            pressed: true,
        },
        7,
        7000,
    ));

    let sampler = RawInputRingLateSampler::new(consumer);
    let batch = sampler.drain_up_to(WallClockStamp::new(7000));

    assert_eq!(batch.inputs.len(), 1);
    assert_eq!(batch.inputs[0].source, SmolStr::new("keyboard"));
    assert_eq!(batch.inputs[0].detail, SmolStr::new("key.KeyW"));
    assert_eq!(batch.inputs[0].value, TickInputValue::button(true));

    let map = InputMapPlan::new(
        "reference_host",
        vec![InputMapBinding::new("MoveForward", "keyboard", "key.KeyW")],
    )
    .expect("input map");
    let frame = map.translate(&batch, wrela::world_identity::SnapshotEpoch(1));

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

#[test]
fn raw_input_ring_late_sampler_preserves_release_and_axis_values() {
    let (mut producer, consumer) = RawInputRing::split_with_capacity(4);
    producer.push_event(TimestampedRawEvent::new(
        "keyboard",
        "key.KeyW",
        RawInputKind::Key {
            code: SmolStr::new("KeyW"),
            pressed: false,
        },
        7,
        7000,
    ));
    producer.push_event(TimestampedRawEvent::new(
        "gamepad",
        "left_stick_y",
        RawInputKind::GamepadAxis {
            axis: SmolStr::new("left_stick_y"),
            value_micros: -250_000,
        },
        8,
        8000,
    ));
    producer.push_event(TimestampedRawEvent::new(
        "mouse",
        "mouse.move",
        RawInputKind::MouseDelta { x: 12, y: -7 },
        9,
        9000,
    ));

    let sampler = RawInputRingLateSampler::new(consumer);
    let batch = sampler.drain_up_to(WallClockStamp::new(9000));

    assert_eq!(batch.inputs.len(), 3);
    assert_eq!(batch.inputs[0].value, TickInputValue::button(false));
    assert_eq!(
        batch.inputs[1].value,
        TickInputValue::Axis1 {
            value_micros: -250_000
        }
    );
    assert_eq!(
        batch.inputs[2].value,
        TickInputValue::Axis2 {
            x_micros: 12_000,
            y_micros: -7_000
        }
    );

    let map = InputMapPlan::new(
        "reference_host",
        vec![
            InputMapBinding::new("MoveForward", "keyboard", "key.KeyW"),
            InputMapBinding::new("MoveAxis", "gamepad", "left_stick_y"),
            InputMapBinding::new("PointerMove", "mouse", "mouse.move"),
        ],
    )
    .expect("input map");
    let frame = map.translate(&batch, wrela::world_identity::SnapshotEpoch(1));

    assert!(matches!(
        frame
            .actions
            .get(&wrela::input_contract::SemanticActionId::new("MoveForward")),
        Some(SemanticActionState::Button {
            pressed: false,
            just_pressed: false,
            just_released: true
        })
    ));
    assert!(matches!(
        frame
            .actions
            .get(&wrela::input_contract::SemanticActionId::new("MoveAxis")),
        Some(SemanticActionState::Axis1 { value }) if (*value - -0.25).abs() < f32::EPSILON
    ));
    assert!(matches!(
        frame
            .actions
            .get(&wrela::input_contract::SemanticActionId::new("PointerMove")),
        Some(SemanticActionState::Axis2 { x, y })
            if (*x - 0.012).abs() < f32::EPSILON && (*y - -0.007).abs() < f32::EPSILON
    ));
}

#[test]
fn input_map_allows_alternative_bindings_for_same_action() {
    let map = InputMapPlan::new(
        "player",
        vec![
            InputMapBinding::new("MoveForward", "keyboard", "key.KeyW"),
            InputMapBinding::new("MoveForward", "gamepad", "left_stick_y.positive"),
        ],
    )
    .expect("same action can have multiple physical bindings");
    let batch = TickInputBatch::new(
        SimulationTick::new(1),
        vec![TickInputEvent::with_timestamps(
            SimulationTick::new(1),
            TickInputKind::Event,
            "gamepad",
            "left_stick_y.positive",
            WallClockStamp::new(10),
            10,
        )],
    );

    let frame = map.translate(&batch, wrela::world_identity::SnapshotEpoch(1));

    assert_eq!(frame.actions.len(), 1);
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

#[test]
fn input_map_rejects_empty_action_id() {
    let err = InputMapPlan::new(
        "player",
        vec![InputMapBinding::new("", "keyboard", "key.KeyW")],
    )
    .expect_err("empty semantic action ids are not well-formed");

    assert!(err.contains("must not be empty"));
}

#[test]
fn raw_input_late_sampler_reentrant_drain_returns_empty_instead_of_blocking() {
    let (mut producer, consumer) = RawInputRing::split_with_capacity(8);
    producer.push_event(TimestampedRawEvent::new(
        "keyboard",
        "key.w.down",
        RawInputKind::Key {
            code: SmolStr::new("KeyW"),
            pressed: true,
        },
        7,
        7000,
    ));

    let sampler = RawInputRingLateSampler::new(consumer);
    let first = sampler.drain_up_to(WallClockStamp::new(7000));
    let second = sampler.drain_up_to(WallClockStamp::new(7000));

    assert_eq!(first.inputs.len(), 1);
    assert!(second.inputs.is_empty());
}
