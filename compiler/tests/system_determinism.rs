use wrela::input_contract::InputFrame;
use wrela::input_contract::{SemanticActionId, SemanticActionState};
use wrela::state_advance::SimulationTick;
use wrela::system_contract::{
    EventTypeId, SystemAccessSummary, SystemContractId, SystemId, SystemPhase, SystemResourceId,
};
use wrela::system_exec::{SystemExecutor, SystemMirInvoker};
use wrela::system_plan::{SystemPlan, SystemProgram};
use wrela::world_identity::SnapshotEpoch;

use std::sync::Arc;

struct NoopMirInvoker;

impl SystemMirInvoker for NoopMirInvoker {
    fn invoke(&self, _mir_function_id: u32, _input: &InputFrame) -> Result<(), String> {
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
fn system_events_are_one_tick_deferred() {
    let program = sample_program();
    let input = sample_input();
    let mut executor = SystemExecutor::new(Arc::new(NoopMirInvoker));
    let first = executor.run_program(&program, &input).expect("first");
    assert!(
        first
            .records
            .iter()
            .all(|record| record.visible_events.is_empty())
    );
    let second = executor.run_program(&program, &input).expect("second");
    assert!(second.records.iter().any(|record| {
        record
            .visible_events
            .iter()
            .any(|event| event.0 == "FrameSummary")
    }));
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
