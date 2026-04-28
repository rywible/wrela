use smol_str::SmolStr;
use wrela::engine_frame::{
    EngineFrameRuntime, EngineFrameRuntimePolicy, EngineGraphBuilder, EngineStateAdvanceExecutor,
    EngineSubsystemAdapter, EngineSubsystemKind, InputSubsystemAdapter, PhysicsSubsystemAdapter,
    SystemSubsystemAdapter,
};
use wrela::input_map_plan::InputMapPlan;
use wrela::physics_contract::{PhysicsBodyDescriptor, PhysicsBodyId};
use wrela::physics_exec::{PhysicsBodyState, PhysicsSolver};
use wrela::physics_plan::{PhysicsBackend, PhysicsPlan};
use wrela::query_exec::stable_region_snapshot_handle;
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
            ChangeSummary::new(ChangeClass::None, "physics adapter test"),
        ))
    }
}

#[test]
fn collision_backed_physics_advertises_gpu_and_hot_path_readback_contract() {
    let body = PhysicsBodyDescriptor::dynamic_sphere(PhysicsBodyId(1), 1.0, 0.5);
    let solver = PhysicsSolver::new(
        PhysicsPlan::collision_backed(vec![body]),
        vec![PhysicsBodyState::new(PhysicsBodyId(1), [0.0, 1.0, 0.0])],
    );
    let mut physics = PhysicsSubsystemAdapter::new(solver, 1.0 / 60.0);
    let mut builder = EngineGraphBuilder::default();

    let plan = EngineSubsystemAdapter::build(&mut physics, &mut builder).expect("plan");

    assert_eq!(plan.descriptor.kind, EngineSubsystemKind::Physics);
    assert!(plan.descriptor.requires_gpu);
    assert!(plan.descriptor.allows_hot_path_readback);
}

#[test]
fn cpu_oracle_physics_keeps_cpu_only_adapter_contract() {
    let body = PhysicsBodyDescriptor::dynamic_sphere(PhysicsBodyId(1), 1.0, 0.5);
    let solver = PhysicsSolver::new(
        PhysicsPlan::new(PhysicsBackend::CpuOracle, vec![body]),
        vec![PhysicsBodyState::new(PhysicsBodyId(1), [0.0, 1.0, 0.0])],
    );
    let mut physics = PhysicsSubsystemAdapter::new(solver, 1.0 / 60.0);
    let mut builder = EngineGraphBuilder::default();

    let plan = EngineSubsystemAdapter::build(&mut physics, &mut builder).expect("plan");

    assert!(!plan.descriptor.requires_gpu);
    assert!(!plan.descriptor.allows_hot_path_readback);
}

#[test]
fn collision_backed_physics_reports_cpu_solver_time_not_gpu_proxy_time() {
    let mut runtime = EngineFrameRuntime::new(Box::new(NoopStateAdvanceExecutor));
    let input_adapter = InputSubsystemAdapter::new(
        InputMapPlan::empty("empty"),
        runtime.materialized_tick_input_slot(),
    );
    let empty_systems = SystemSubsystemAdapter::new(
        SystemProgram::new(Vec::new()).expect("empty systems"),
        input_adapter.shared_frame(),
    );
    let body = PhysicsBodyDescriptor::dynamic_sphere(PhysicsBodyId(1), 1.0, 0.5);
    let solver = PhysicsSolver::new(
        PhysicsPlan::collision_backed(vec![body]),
        vec![PhysicsBodyState::new(PhysicsBodyId(1), [0.0, 0.1, 0.0])],
    );
    let physics = PhysicsSubsystemAdapter::new(solver, 1.0 / 60.0);
    let previous_snapshot = stable_region_snapshot_handle(&SmolStr::new(
        "collision_backed_physics_reports_cpu_solver_time",
    ));
    let tick = SimulationTick::new(1);

    let output = runtime
        .run_frame_with_subsystems(
            wrela::engine_frame::EngineFrameInput {
                scenario_id: "collision_backed_physics_reports_cpu_solver_time".into(),
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
                Box::new(physics),
            ],
        )
        .expect("frame");

    let report = output
        .report
        .subsystem(EngineSubsystemKind::Physics)
        .expect("physics report");
    assert!(report.cpu_critical_path_micros > 0);
    assert_eq!(report.gpu_critical_path_micros, None);
}

#[test]
fn physics_subsystem_reports_body_state_and_contacts() {
    let mut runtime = EngineFrameRuntime::new(Box::new(NoopStateAdvanceExecutor));
    let input_adapter = InputSubsystemAdapter::new(
        InputMapPlan::empty("empty"),
        runtime.materialized_tick_input_slot(),
    );
    let empty_systems = SystemSubsystemAdapter::new(
        SystemProgram::new(Vec::new()).expect("empty systems"),
        input_adapter.shared_frame(),
    );
    let body = PhysicsBodyDescriptor::dynamic_sphere(PhysicsBodyId(1), 1.0, 0.5);
    let solver = PhysicsSolver::new(
        PhysicsPlan::cpu(vec![body]),
        vec![PhysicsBodyState::new(PhysicsBodyId(1), [0.0, 0.1, 0.0])],
    );
    let physics = PhysicsSubsystemAdapter::new(solver, 1.0 / 60.0);
    let previous_snapshot = stable_region_snapshot_handle(&SmolStr::new("physics_adapter"));
    let tick = SimulationTick::new(1);
    let output = runtime
        .run_frame_with_subsystems(
            wrela::engine_frame::EngineFrameInput {
                scenario_id: "physics_adapter".into(),
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
                Box::new(physics),
            ],
        )
        .expect("frame");
    let report = output
        .report
        .subsystem(EngineSubsystemKind::Physics)
        .expect("physics report");
    assert!(report.work_items > 0);
    assert!(
        output
            .report
            .resource_ledger
            .states
            .iter()
            .any(|state| matches!(
                state.resource,
                wrela::engine_frame::EngineResourceId::PhysicsBodyState { .. }
            ))
    );
}

fn run_physics_frame_with_wall_delta(wall_delta_nanos: u64) -> f32 {
    let mut runtime = EngineFrameRuntime::new(Box::new(NoopStateAdvanceExecutor));
    let input_adapter = InputSubsystemAdapter::new(
        InputMapPlan::empty("empty"),
        runtime.materialized_tick_input_slot(),
    );
    let empty_systems = SystemSubsystemAdapter::new(
        SystemProgram::new(Vec::new()).expect("empty systems"),
        input_adapter.shared_frame(),
    );
    let body = PhysicsBodyDescriptor::dynamic_sphere(PhysicsBodyId(1), 1.0, 0.5);
    let solver = PhysicsSolver::new(
        PhysicsPlan::cpu(vec![body]),
        vec![PhysicsBodyState::new(PhysicsBodyId(1), [0.0, 10.0, 0.0])],
    );
    let physics = PhysicsSubsystemAdapter::new(solver, 1.0 / 60.0);
    let solver = physics.solver();
    let previous_snapshot = stable_region_snapshot_handle(&SmolStr::new("physics_adapter_dt"));
    let tick = SimulationTick::new(1);

    runtime
        .run_frame_with_subsystems(
            wrela::engine_frame::EngineFrameInput {
                scenario_id: "physics_adapter_dt".into(),
                frame_index: 0,
                previous_snapshot: previous_snapshot.clone(),
                previous_clock: TemporalClock::new(
                    wrela::time_semantics::SnapshotEpoch::new(previous_snapshot.epoch().0),
                    SimulationTick::new(0),
                    PresentationFrame::new(0),
                    WallClockStamp::new(1_000_000_000),
                ),
                current_clock: TemporalClock::new(
                    wrela::time_semantics::SnapshotEpoch::new(previous_snapshot.epoch().0 + 1),
                    tick,
                    PresentationFrame::new(1),
                    WallClockStamp::new(1_000_000_000 + wall_delta_nanos),
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
                Box::new(physics),
            ],
        )
        .expect("frame");

    solver.lock().expect("solver").bodies()[0].position[1]
}

#[test]
fn physics_adapter_uses_frame_clock_dt_each_frame() {
    let y_after_120hz = run_physics_frame_with_wall_delta(8_333_333);
    let y_after_30hz = run_physics_frame_with_wall_delta(33_333_333);

    assert!(
        y_after_30hz < y_after_120hz,
        "30Hz step should move farther downward: 30Hz y={y_after_30hz}, 120Hz y={y_after_120hz}"
    );
}
