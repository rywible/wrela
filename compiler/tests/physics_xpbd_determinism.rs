use wrela::collision_exec::CollisionBatchItem;
use wrela::physics_contract::{PhysicsBodyDescriptor, PhysicsBodyId};
use wrela::physics_exec::{PhysicsBodyState, PhysicsSolver};
use wrela::physics_plan::{PhysicsPlan, PhysicsSubstepPolicy};

fn solver() -> PhysicsSolver {
    let body = PhysicsBodyDescriptor::dynamic_sphere(PhysicsBodyId(1), 1.0, 0.5);
    let mut plan = PhysicsPlan::cpu(vec![body]);
    plan.substeps = PhysicsSubstepPolicy {
        requested_substeps_per_tick: 2,
        max_substeps_per_tick: 4,
        positional_iterations: 4,
        velocity_iterations: 1,
    };
    PhysicsSolver::new(
        plan,
        vec![PhysicsBodyState::new(PhysicsBodyId(1), [0.0, 2.0, 0.0])],
    )
}

#[test]
fn xpbd_cpu_solver_is_deterministic_for_fixed_steps() {
    let mut left = solver();
    let mut right = solver();
    for _ in 0..600 {
        let l = left.step(1.0 / 60.0).expect("left");
        let r = right.step(1.0 / 60.0).expect("right");
        assert_eq!(l, r);
        assert_eq!(left.bodies(), right.bodies());
    }
}

#[test]
fn xpbd_clamps_substeps_and_reports_finding() {
    let body = PhysicsBodyDescriptor::dynamic_sphere(PhysicsBodyId(1), 1.0, 0.5);
    let mut plan = PhysicsPlan::cpu(vec![body]);
    plan.substeps.requested_substeps_per_tick = 16;
    plan.substeps.max_substeps_per_tick = 4;
    let mut solver = PhysicsSolver::new(
        plan,
        vec![PhysicsBodyState::new(PhysicsBodyId(1), [0.0, 2.0, 0.0])],
    );
    let report = solver.step(1.0 / 60.0).expect("step");
    assert_eq!(report.substeps, 4);
    assert!(
        report
            .findings
            .iter()
            .any(|f| f == "physics.substep_clamped")
    );
}

#[test]
fn collision_backed_solver_reports_collision_batch_intent_and_contact_readback_budget() {
    let a = PhysicsBodyDescriptor::dynamic_sphere(PhysicsBodyId(1), 1.0, 0.5);
    let b = PhysicsBodyDescriptor::dynamic_sphere(PhysicsBodyId(2), 1.0, 0.5);
    let mut plan = PhysicsPlan::collision_backed(vec![a, b]);
    plan.contact_readback_budget_bytes = 0;
    let mut solver = PhysicsSolver::new(
        plan,
        vec![
            PhysicsBodyState::new(PhysicsBodyId(1), [0.0, 0.5, 0.0]),
            PhysicsBodyState::new(PhysicsBodyId(2), [0.75, 0.5, 0.0]),
        ],
    );

    let report = solver.step(1.0 / 60.0).expect("step");

    assert!(report.contacts_detected > 0);
    assert!(report.readback_bytes > 0);
    assert!(
        report
            .findings
            .iter()
            .any(|finding| finding == "physics.contact_readback_over_budget")
    );
    assert!(
        report.collision_batches.iter().any(|batch| {
            batch
                .items
                .iter()
                .any(|item| matches!(item, CollisionBatchItem::SphereOverlap { .. }))
        }),
        "expected collision-backed solver to emit SphereOverlap workload intent"
    );
}

#[test]
fn collision_backed_ccd_reports_sweep_and_time_of_impact_intent() {
    let body = PhysicsBodyDescriptor::dynamic_sphere(PhysicsBodyId(1), 1.0, 0.5);
    let plan = PhysicsPlan::collision_backed(vec![body]);
    let mut state = PhysicsBodyState::new(PhysicsBodyId(1), [0.0, 2.0, 0.0]);
    state.linear_velocity = [0.0, -240.0, 0.0];
    let mut solver = PhysicsSolver::new(plan, vec![state]);

    let report = solver.step(1.0 / 60.0).expect("step");

    assert!(
        report.collision_batches.iter().any(|batch| {
            batch.items.iter().any(|item| {
                matches!(
                    item,
                    CollisionBatchItem::SphereSweep { .. }
                        | CollisionBatchItem::SphereTimeOfImpact { .. }
                )
            })
        }),
        "expected collision-backed CCD to emit sweep/time-of-impact workload intent"
    );
}
