use wrela::input_contract::InputFrame;
use wrela::input_contract::{SemanticActionId, SemanticActionState};
use wrela::state_advance::SimulationTick;
use wrela::system_contract::{
    EventTypeId, SystemAccessSummary, SystemContractId, SystemId, SystemPhase, SystemResourceId,
};
use wrela::system_exec::{SystemExecutor, SystemInvocationContext, SystemMirInvoker};
use wrela::system_plan::{SystemPlan, SystemProgram};
use wrela::world_identity::SnapshotEpoch;

use std::sync::{Arc, Mutex};

struct NoopMirInvoker;

impl SystemMirInvoker for NoopMirInvoker {
    fn invoke(
        &self,
        _mir_function_id: u32,
        _ctx: &mut SystemInvocationContext<'_>,
    ) -> Result<(), String> {
        Ok(())
    }
}

struct ConditionalEmitterInvoker {
    emit: bool,
}

impl SystemMirInvoker for ConditionalEmitterInvoker {
    fn invoke(
        &self,
        mir_function_id: u32,
        ctx: &mut SystemInvocationContext<'_>,
    ) -> Result<(), String> {
        if self.emit && mir_function_id == 2 {
            ctx.emitted_events.push(EventTypeId::new("FrameSummary"));
        }
        Ok(())
    }
}

struct DtRecordingMirInvoker {
    recorded: Arc<Mutex<Vec<f64>>>,
}

impl SystemMirInvoker for DtRecordingMirInvoker {
    fn invoke(
        &self,
        _mir_function_id: u32,
        ctx: &mut SystemInvocationContext<'_>,
    ) -> Result<(), String> {
        self.recorded
            .lock()
            .map_err(|_| "recorded dt lock poisoned".to_string())?
            .push(ctx.dt_seconds);
        Ok(())
    }
}

fn sample_program() -> SystemProgram {
    SystemProgram::new([
        SystemPlan::new(
            SystemId::new("DrainInput"),
            SystemContractId::new("drain"),
            SystemPhase::PreSim,
            SystemAccessSummary::default()
                .reads(SystemResourceId::InputFrame)
                .writes(SystemResourceId::Resource("player".into())),
            1,
        ),
        SystemPlan::new(
            SystemId::new("EmitFrameEvents"),
            SystemContractId::new("emit"),
            SystemPhase::PostSim,
            SystemAccessSummary::default().emits_event(EventTypeId::new("FrameSummary")),
            2,
        ),
    ])
    .expect("program")
}

fn sample_input() -> InputFrame {
    let mut actions = std::collections::BTreeMap::new();
    actions.insert(
        SemanticActionId::new("MoveForward"),
        SemanticActionState::pressed_button(),
    );
    InputFrame {
        epoch: SnapshotEpoch(1),
        tick: SimulationTick::new(1),
        actions,
    }
}

#[test]
fn system_executor_is_deterministic_for_fixed_input_trace() {
    let program = sample_program();
    let input = sample_input();
    let mut left = SystemExecutor::new(Arc::new(NoopMirInvoker));
    let mut right = SystemExecutor::new(Arc::new(NoopMirInvoker));
    for _ in 0..10_000 {
        let l = left.run_program(&program, &input).expect("left");
        let r = right.run_program(&program, &input).expect("right");
        assert_eq!(l, r);
    }
}

#[test]
fn default_system_executor_fails_loudly_for_authored_program() {
    let program = sample_program();
    let input = sample_input();
    let mut executor = SystemExecutor::default();

    let err = executor
        .run_program(&program, &input)
        .expect_err("default executor must not silently run authored systems");
    assert!(
        err.to_string()
            .contains("system MIR invoker is not configured for function 1"),
        "unexpected error: {err}"
    );
}

#[test]
fn default_system_executor_runs_empty_program_without_invoking() {
    let program = SystemProgram::new(Vec::new()).expect("empty program");
    let input = sample_input();
    let mut executor = SystemExecutor::default();

    let report = executor
        .run_program(&program, &input)
        .expect("empty program should not invoke MIR");
    assert!(report.records.is_empty());
}

#[test]
fn direct_system_executor_uses_configured_default_simulation_dt() {
    let program = sample_program();
    let input = sample_input();
    let recorded = Arc::new(Mutex::new(Vec::new()));
    let mut executor = SystemExecutor::new(Arc::new(DtRecordingMirInvoker {
        recorded: Arc::clone(&recorded),
    }))
    .with_default_simulation_dt_seconds(1.0 / 144.0);

    executor
        .run_program(&program, &input)
        .expect("program should use configured default dt");

    let recorded = recorded.lock().expect("recorded dt");
    assert_eq!(recorded.as_slice(), &[1.0 / 144.0, 1.0 / 144.0]);
}

#[test]
fn direct_system_executor_default_simulation_dt_is_not_implicit_60_hz() {
    let program = sample_program();
    let input = sample_input();
    let recorded = Arc::new(Mutex::new(Vec::new()));
    let mut executor = SystemExecutor::new(Arc::new(DtRecordingMirInvoker {
        recorded: Arc::clone(&recorded),
    }));

    assert_eq!(executor.default_simulation_dt_seconds(), 0.0);
    executor
        .run_system(&program.phase(SystemPhase::PreSim)[0], &input)
        .expect("system should use executor default dt");

    let recorded = recorded.lock().expect("recorded dt");
    assert_eq!(recorded.as_slice(), &[0.0]);
}

#[test]
fn system_events_are_one_tick_deferred() {
    let program = sample_program();
    let input = sample_input();
    let mut executor = SystemExecutor::new(Arc::new(ConditionalEmitterInvoker { emit: true }));
    let first = executor
        .run_program_with_dt(&program, &input, 1.0 / 120.0)
        .expect("first");
    assert!(
        first
            .records
            .iter()
            .all(|record| record.visible_events.is_empty())
    );
    let second = executor
        .run_program_with_dt(&program, &input, 1.0 / 120.0)
        .expect("second");
    assert!(second.records.iter().any(|record| {
        record
            .visible_events
            .iter()
            .any(|event| event.0 == "FrameSummary")
    }));
}

#[test]
fn declared_event_emitter_does_not_publish_without_send() {
    let program = sample_program();
    let input = sample_input();
    let mut executor = SystemExecutor::new(Arc::new(ConditionalEmitterInvoker { emit: false }));

    let first = executor
        .run_program_with_dt(&program, &input, 1.0 / 120.0)
        .expect("first");
    assert!(
        first
            .records
            .iter()
            .all(|record| record.emitted_events.is_empty())
    );

    let second = executor
        .run_program_with_dt(&program, &input, 1.0 / 120.0)
        .expect("second");
    assert!(
        second
            .records
            .iter()
            .all(|record| record.visible_events.is_empty())
    );
}

#[test]
fn actual_event_emission_is_visible_once_next_tick() {
    let program = sample_program();
    let input = sample_input();
    let mut executor = SystemExecutor::new(Arc::new(ConditionalEmitterInvoker { emit: true }));

    let first = executor
        .run_program_with_dt(&program, &input, 1.0 / 120.0)
        .expect("first");
    let first_emitted_count = first
        .records
        .iter()
        .flat_map(|record| &record.emitted_events)
        .filter(|event| event.0 == "FrameSummary")
        .count();
    assert_eq!(first_emitted_count, 1);

    let second = executor
        .run_program_with_dt(&program, &input, 1.0 / 120.0)
        .expect("second");
    let emitter_record = second
        .records
        .iter()
        .find(|record| record.system == SystemId::new("EmitFrameEvents"))
        .expect("emitter record");
    let second_visible_count = emitter_record
        .visible_events
        .iter()
        .filter(|event| event.0 == "FrameSummary")
        .count();
    assert_eq!(second_visible_count, 1);
}

#[test]
fn system_executor_commit_program_execution_records_separates_reporting_from_invocation() {
    let program = sample_program();
    let input = sample_input();
    let mut executor = SystemExecutor::new(Arc::new(NoopMirInvoker));
    executor.begin_tick();

    let report = executor.commit_program_execution_records(&program, &input);

    assert_eq!(report.records.len(), 2);
    assert_eq!(report.records[0].system, SystemId::new("DrainInput"));
    assert_eq!(report.records[1].system, SystemId::new("EmitFrameEvents"));
}
