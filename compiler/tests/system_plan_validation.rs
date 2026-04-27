use wrela::system_contract::{
    SystemAccessSummary, SystemContractId, SystemId, SystemPhase, SystemResourceId,
};
use wrela::system_plan::{SystemPlan, SystemPlanError, SystemProgram};

#[test]
fn system_plan_rejects_read_write_conflict_without_declared_ordering() {
    let read_player =
        SystemAccessSummary::default().reads(SystemResourceId::Resource("player".into()));
    let write_player =
        SystemAccessSummary::default().writes(SystemResourceId::Resource("player".into()));
    let reader = SystemPlan::new(
        SystemId::new("ReadPlayer"),
        SystemContractId::new("read"),
        SystemPhase::Sim,
        read_player,
        1,
    );
    let writer = SystemPlan::new(
        SystemId::new("WritePlayer"),
        SystemContractId::new("write"),
        SystemPhase::Sim,
        write_player,
        2,
    );

    let err = SystemProgram::new([writer, reader]).expect_err("implicit reader/writer ordering");
    assert_eq!(
        err,
        SystemPlanError::MissingExplicitOrdering {
            phase: SystemPhase::Sim,
            left: SystemId::new("WritePlayer"),
            right: SystemId::new("ReadPlayer"),
            resource: SystemResourceId::Resource("player".into()),
        }
    );
}

#[test]
fn system_plan_orders_reader_writer_with_declared_ordering() {
    let read_player =
        SystemAccessSummary::default().reads(SystemResourceId::Resource("player".into()));
    let write_player =
        SystemAccessSummary::default().writes(SystemResourceId::Resource("player".into()));
    let reader = SystemPlan::new(
        SystemId::new("ReadPlayer"),
        SystemContractId::new("read"),
        SystemPhase::Sim,
        read_player,
        1,
    )
    .runs_before(SystemId::new("WritePlayer"));
    let writer = SystemPlan::new(
        SystemId::new("WritePlayer"),
        SystemContractId::new("write"),
        SystemPhase::Sim,
        write_player,
        2,
    );

    let program = SystemProgram::new([writer, reader]).expect("declared reader/writer order");
    let ids = program
        .phase(SystemPhase::Sim)
        .iter()
        .map(|plan| plan.id.0.as_str())
        .collect::<Vec<_>>();
    assert_eq!(ids, vec!["ReadPlayer", "WritePlayer"]);
}

#[test]
fn system_plan_rejects_aliasing_writers_in_same_phase() {
    let access = SystemAccessSummary::default().writes(SystemResourceId::Resource("player".into()));
    let first = SystemPlan::new(
        SystemId::new("DrainInput"),
        SystemContractId::new("drain"),
        SystemPhase::Sim,
        access.clone(),
        1,
    );
    let second = SystemPlan::new(
        SystemId::new("Integrate"),
        SystemContractId::new("integrate"),
        SystemPhase::Sim,
        access,
        2,
    );
    let err = SystemProgram::new([first, second]).expect_err("aliasing writers");
    assert_eq!(
        err,
        SystemPlanError::AliasingWriters {
            phase: SystemPhase::Sim,
            left: SystemId::new("DrainInput"),
            right: SystemId::new("Integrate"),
        }
    );
}

#[test]
fn system_plan_rejects_duplicate_system_ids() {
    let first = SystemPlan::new(
        SystemId::new("MovePlayer"),
        SystemContractId::new("move"),
        SystemPhase::PreSim,
        SystemAccessSummary::default().reads(SystemResourceId::InputFrame),
        1,
    );
    let second = SystemPlan::new(
        SystemId::new("MovePlayer"),
        SystemContractId::new("move_again"),
        SystemPhase::PostSim,
        SystemAccessSummary::default().reads(SystemResourceId::Snapshot),
        2,
    );

    let err = SystemProgram::new([first, second]).expect_err("duplicate system id");
    assert_eq!(
        err,
        SystemPlanError::DuplicateSystemId {
            id: SystemId::new("MovePlayer"),
        }
    );
}
