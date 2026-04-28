use smol_str::SmolStr;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use wrela::collision_exec::{
    CollisionBatchItem, CollisionCandidateGroupingPolicy, CollisionCertificationPolicy,
    CollisionWorkloadBatch,
};
use wrela::collision_plan::{CollisionPlan, CollisionQueryKind};
use wrela::kernel::{KernelStructValue, KernelValue};
use wrela::parser::ast::AstNode;
use wrela::physics_contract::{PhysicsBodyDescriptor, PhysicsBodyId};
use wrela::physics_exec::{
    CollisionExecPhysicsCollisionBatchExecutor, CpuOraclePhysicsCollisionBatchExecutor,
    PhysicsBodyState, PhysicsCollisionBatchExecution, PhysicsCollisionBatchExecutor,
    PhysicsCollisionWorld, PhysicsContact, PhysicsSolver,
};
use wrela::physics_plan::{PhysicsPlan, PhysicsSubstepPolicy};
use wrela::query_contract::DispatchBackend;
use wrela::query_exec::{QueryExecContext, stable_region_scene_capture_id};

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

fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let dz = b[2] - a[2];
    (dx * dx + dy * dy + dz * dz).sqrt()
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
    let mut solver = PhysicsSolver::with_collision_executor(
        plan,
        vec![
            PhysicsBodyState::new(PhysicsBodyId(1), [0.0, 0.5, 0.0]),
            PhysicsBodyState::new(PhysicsBodyId(2), [0.75, 0.5, 0.0]),
        ],
        Arc::new(CpuOraclePhysicsCollisionBatchExecutor),
    )
    .with_collision_world(test_collision_world());

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
        report
            .findings
            .iter()
            .any(|finding| finding == "physics.cpu_oracle_collision_fallback")
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
    assert_eq!(
        report.collision_batches_submitted,
        report.collision_batches.len() as u32,
        "expected all recorded collision batches to be submitted"
    );
}

#[derive(Debug)]
struct CountingCollisionExecutor {
    submissions: Arc<AtomicUsize>,
}

impl PhysicsCollisionBatchExecutor for CountingCollisionExecutor {
    fn submit_collision_batch(
        &self,
        batch: &CollisionWorkloadBatch,
        bodies: &[PhysicsBodyState],
        _descriptors: &std::collections::HashMap<PhysicsBodyId, PhysicsBodyDescriptor>,
    ) -> PhysicsCollisionBatchExecution {
        assert!(
            !batch.items.is_empty(),
            "physics should not submit empty collision batches"
        );
        self.submissions.fetch_add(1, Ordering::SeqCst);
        PhysicsCollisionBatchExecution {
            submitted: true,
            executor: "counting_test_executor".into(),
            used_cpu_oracle_fallback: false,
            error: None,
            contacts: bodies
                .iter()
                .map(|body| wrela::physics_exec::PhysicsContact {
                    body: body.id,
                    other: None,
                    normal_world: [0.0, 1.0, 0.0],
                    penetration: 0.001,
                    generated_by_ccd: false,
                })
                .collect(),
        }
    }
}

#[test]
fn collision_backed_solver_submits_recorded_collision_batches_to_executor() {
    let body = PhysicsBodyDescriptor::dynamic_sphere(PhysicsBodyId(1), 1.0, 0.5);
    let submissions = Arc::new(AtomicUsize::new(0));
    let solver_executor = Arc::new(CountingCollisionExecutor {
        submissions: Arc::clone(&submissions),
    });
    let mut solver = PhysicsSolver::with_collision_executor(
        PhysicsPlan::collision_backed(vec![body]),
        vec![PhysicsBodyState::new(PhysicsBodyId(1), [0.0, 0.5, 0.0])],
        solver_executor,
    )
    .with_collision_world(test_collision_world());

    let report = solver.step(1.0 / 60.0).expect("step");

    assert!(
        !report.collision_batches.is_empty(),
        "test fixture should generate collision-backed workload batches"
    );
    assert_eq!(
        submissions.load(Ordering::SeqCst),
        report.collision_batches.len()
    );
    assert_eq!(
        report.collision_batches_submitted as usize,
        report.collision_batches.len()
    );
}

#[derive(Debug)]
struct EmptyContactCollisionExecutor {
    submissions: Arc<AtomicUsize>,
}

impl PhysicsCollisionBatchExecutor for EmptyContactCollisionExecutor {
    fn submit_collision_batch(
        &self,
        batch: &CollisionWorkloadBatch,
        _bodies: &[PhysicsBodyState],
        _descriptors: &std::collections::HashMap<PhysicsBodyId, PhysicsBodyDescriptor>,
    ) -> PhysicsCollisionBatchExecution {
        assert!(
            !batch.items.is_empty(),
            "physics should not submit empty collision batches"
        );
        self.submissions.fetch_add(1, Ordering::SeqCst);
        PhysicsCollisionBatchExecution {
            submitted: true,
            executor: "empty_contact_test_executor".into(),
            used_cpu_oracle_fallback: false,
            error: None,
            contacts: Vec::new(),
        }
    }
}

#[derive(Debug)]
struct ReversedPairCollisionExecutor {
    submissions: Arc<AtomicUsize>,
}

impl PhysicsCollisionBatchExecutor for ReversedPairCollisionExecutor {
    fn submit_collision_batch(
        &self,
        batch: &CollisionWorkloadBatch,
        _bodies: &[PhysicsBodyState],
        _descriptors: &std::collections::HashMap<PhysicsBodyId, PhysicsBodyDescriptor>,
    ) -> PhysicsCollisionBatchExecution {
        self.submissions.fetch_add(1, Ordering::SeqCst);
        let contacts = if batch.workload_id.as_str().contains("detect_contacts") {
            vec![PhysicsContact {
                body: PhysicsBodyId(2),
                other: Some(PhysicsBodyId(1)),
                normal_world: [-1.0, 0.0, 0.0],
                penetration: 0.2,
                generated_by_ccd: false,
            }]
        } else {
            Vec::new()
        };
        PhysicsCollisionBatchExecution {
            submitted: true,
            executor: "reversed_pair_test_executor".into(),
            used_cpu_oracle_fallback: false,
            error: None,
            contacts,
        }
    }
}

#[test]
fn collision_backed_solver_resolves_body_body_contacts_without_world_contact() {
    let a = PhysicsBodyDescriptor::dynamic_sphere(PhysicsBodyId(1), 1.0, 0.5);
    let b = PhysicsBodyDescriptor::dynamic_sphere(PhysicsBodyId(2), 1.0, 0.5);
    let mut plan = PhysicsPlan::collision_backed(vec![a, b]);
    plan.contact_readback_budget_bytes = 0;
    let submissions = Arc::new(AtomicUsize::new(0));
    let solver_executor = Arc::new(EmptyContactCollisionExecutor {
        submissions: Arc::clone(&submissions),
    });
    let mut solver = PhysicsSolver::with_collision_executor(
        plan,
        vec![
            PhysicsBodyState::new(PhysicsBodyId(1), [0.0, 10.0, 0.0]),
            PhysicsBodyState::new(PhysicsBodyId(2), [0.75, 10.0, 0.0]),
        ],
        solver_executor,
    )
    .with_collision_world(test_collision_world());
    let initial_separation = {
        let bodies = solver.bodies();
        distance(bodies[0].position, bodies[1].position)
    };

    let report = solver.step(1.0 / 60.0).expect("step");
    let final_separation = {
        let bodies = solver.bodies();
        distance(bodies[0].position, bodies[1].position)
    };

    assert!(
        submissions.load(Ordering::SeqCst) > 0,
        "test executor should receive collision batches"
    );
    assert!(
        report.contacts_resolved > 0,
        "expected body-body contact resolution from solver-side pair contacts"
    );
    assert_eq!(
        report.readback_bytes, 0,
        "CPU-generated body-body contacts must not count as collision-exec readback"
    );
    assert_eq!(
        report.contact_readback_micros, 0,
        "CPU-generated body-body contacts must not add readback latency"
    );
    assert!(
        !report
            .findings
            .iter()
            .any(|finding| finding == "physics.contact_readback_over_budget"),
        "CPU-generated body-body contacts must not trip the readback budget"
    );
    assert!(
        !report
            .findings
            .iter()
            .any(|finding| finding == "physics.cpu_oracle_divergence"),
        "solver-appended CPU body-body contacts should match the CPU oracle"
    );
    assert!(
        final_separation > initial_separation,
        "expected overlapping bodies to separate: initial={initial_separation}, final={final_separation}"
    );
}

#[test]
fn collision_backed_solver_reports_cpu_oracle_divergence_for_missing_world_contact() {
    let body = PhysicsBodyDescriptor::dynamic_sphere(PhysicsBodyId(1), 1.0, 0.5);
    let submissions = Arc::new(AtomicUsize::new(0));
    let solver_executor = Arc::new(EmptyContactCollisionExecutor {
        submissions: Arc::clone(&submissions),
    });
    let mut solver = PhysicsSolver::with_collision_executor(
        PhysicsPlan::collision_backed(vec![body]),
        vec![PhysicsBodyState::new(PhysicsBodyId(1), [0.0, 0.25, 0.0])],
        solver_executor,
    )
    .with_collision_world(test_collision_world());

    let report = solver.step(1.0 / 60.0).expect("step");

    assert!(
        submissions.load(Ordering::SeqCst) > 0,
        "test executor should receive collision batches"
    );
    assert!(
        report
            .findings
            .iter()
            .any(|finding| finding == "physics.cpu_oracle_divergence"),
        "missing collision-backed world contact evidence should diverge from the CPU oracle"
    );
}

#[test]
fn collision_backed_solver_resolves_reversed_pair_contacts() {
    let a = PhysicsBodyDescriptor::dynamic_sphere(PhysicsBodyId(1), 1.0, 0.5);
    let b = PhysicsBodyDescriptor::dynamic_sphere(PhysicsBodyId(2), 1.0, 0.5);
    let submissions = Arc::new(AtomicUsize::new(0));
    let solver_executor = Arc::new(ReversedPairCollisionExecutor {
        submissions: Arc::clone(&submissions),
    });
    let mut solver = PhysicsSolver::with_collision_executor(
        PhysicsPlan::collision_backed(vec![a, b]),
        vec![
            PhysicsBodyState::new(PhysicsBodyId(1), [0.0, 10.0, 0.0]),
            PhysicsBodyState::new(PhysicsBodyId(2), [2.0, 10.0, 0.0]),
        ],
        solver_executor,
    )
    .with_collision_world(test_collision_world());

    let report = solver.step(1.0 / 60.0).expect("step");
    let bodies = solver.bodies();

    assert!(
        submissions.load(Ordering::SeqCst) > 0,
        "test executor should receive collision batches"
    );
    assert!(
        report.contacts_resolved > 0,
        "reversed pair contacts should be counted after correction"
    );
    assert!(
        bodies[0].position[0] < 0.0,
        "body 1 should move left from reversed contact, got {:?}",
        bodies[0].position
    );
    assert!(
        bodies[1].position[0] > 2.0,
        "body 2 should move right from reversed contact, got {:?}",
        bodies[1].position
    );
}

#[test]
fn collision_backed_solver_without_real_executor_reports_not_submitted() {
    let body = PhysicsBodyDescriptor::dynamic_sphere(PhysicsBodyId(1), 1.0, 0.5);
    let mut solver = PhysicsSolver::new(
        PhysicsPlan::collision_backed(vec![body]),
        vec![PhysicsBodyState::new(PhysicsBodyId(1), [0.0, 0.5, 0.0])],
    )
    .with_collision_world(test_collision_world());

    let report = solver.step(1.0 / 60.0).expect("step");

    assert!(!report.collision_batches.is_empty());
    assert_eq!(report.collision_batches_submitted, 0);
    assert!(
        report
            .findings
            .iter()
            .any(|finding| finding == "physics.collision_batch_not_submitted")
    );
}

#[test]
fn collision_exec_physics_executor_calls_collision_exec_and_converts_overlap_contact() {
    let ctx = Arc::new(typed_query_module(collision_fixture_source()));
    let scene_id = stable_region_scene_capture_id(&SmolStr::new("collision_region"));
    let plan = CollisionPlan::for_query_with_backend(
        CollisionQueryKind::SphereOverlapWorld,
        DispatchBackend::Cpu,
    );
    let batch = CollisionWorkloadBatch::new(
        "physics overlap",
        "physics_overlap_real_exec",
        "physics",
        plan.clone(),
        plan.contract_id,
        "snapshot:physics:overlap:1",
        region_capture(scene_id, 1),
        scene_domain(scene_id),
        CollisionCandidateGroupingPolicy::SharedCandidateDigest,
        CollisionCertificationPolicy::CpuOracleParity,
        vec![CollisionBatchItem::SphereOverlap {
            center: [0.0, 0.0, 0.9],
            radius: 0.6,
        }],
        1,
    );
    let descriptor = PhysicsBodyDescriptor::dynamic_sphere(PhysicsBodyId(1), 1.0, 0.6);
    let bodies = vec![PhysicsBodyState::new(PhysicsBodyId(1), [0.0, 0.0, 0.9])];
    let descriptors = std::iter::once((descriptor.id, descriptor)).collect();
    let executor = CollisionExecPhysicsCollisionBatchExecutor::new(ctx);

    let execution = executor.submit_collision_batch(&batch, &bodies, &descriptors);

    assert!(execution.submitted, "{execution:?}");
    assert_eq!(execution.contacts.len(), 1);
    assert_eq!(execution.contacts[0].body, PhysicsBodyId(1));
    assert!(!execution.contacts[0].generated_by_ccd);
}

#[test]
fn collision_backed_ccd_reports_sweep_and_time_of_impact_intent() {
    let body = PhysicsBodyDescriptor::dynamic_sphere(PhysicsBodyId(1), 1.0, 0.5);
    let plan = PhysicsPlan::collision_backed(vec![body]);
    let mut state = PhysicsBodyState::new(PhysicsBodyId(1), [0.0, 2.0, 0.0]);
    state.linear_velocity = [0.0, -240.0, 0.0];
    let mut solver = PhysicsSolver::with_collision_executor(
        plan,
        vec![state],
        Arc::new(CpuOraclePhysicsCollisionBatchExecutor),
    )
    .with_collision_world(test_collision_world());

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

fn typed_query_module(source: &str) -> QueryExecContext {
    let node = wrela::parser::parse(source);
    let root = wrela::parser::ast::Root::cast(node).expect("root");
    let module = wrela::hir::lower::lower(root);
    let semantic = wrela::hir::semantic::check_module(&module);
    assert!(
        semantic.errors.is_empty(),
        "semantic errors: {:?}",
        semantic.errors
    );
    let (type_errors, type_info) = wrela::hir::typeck::check_module_with_info(&module);
    assert!(type_errors.is_empty(), "type errors: {type_errors:?}");
    QueryExecContext::compile(&module, &type_info)
}

fn collision_fixture_source() -> &'static str {
    r#"
field exact distance collision_field(p: Vec3) -> F32 {
    sphere(radius = 0.5)
}

material collision_surface(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(0.8, 0.3, 0.2),
        roughness=0.2,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

shape collision_shape {
    field = collision_field
    material = collision_surface
}

region collision_region() {
    place sample = collision_shape
}

domain collision_domain(world: RegionCapture) {
    geometry_detail = 1
    material = true
    radiance = false
    media = false
    max_distance = 6.0
    min_step = 0.05
}
"#
}

fn scene_domain(scene_id: u32) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("SceneDomain"),
        fields: vec![
            (SmolStr::new("scene_id"), KernelValue::U32(scene_id)),
            (
                SmolStr::new("spatial"),
                KernelValue::Struct(KernelStructValue {
                    name: SmolStr::new("SpatialDomainContract"),
                    fields: vec![(SmolStr::new("geometry_detail"), KernelValue::I32(1))],
                }),
            ),
            (
                SmolStr::new("surface"),
                KernelValue::Struct(KernelStructValue {
                    name: SmolStr::new("SurfaceDomainContract"),
                    fields: vec![(SmolStr::new("material"), KernelValue::Bool(true))],
                }),
            ),
            (
                SmolStr::new("participants"),
                KernelValue::Struct(KernelStructValue {
                    name: SmolStr::new("ParticipantDomainContract"),
                    fields: vec![
                        (SmolStr::new("radiance"), KernelValue::Bool(false)),
                        (SmolStr::new("media"), KernelValue::Bool(false)),
                    ],
                }),
            ),
        ],
    })
}

fn test_collision_world() -> PhysicsCollisionWorld {
    PhysicsCollisionWorld {
        capture: KernelValue::Capture(SmolStr::new("physics_test_capture")),
        domain: KernelValue::Capture(SmolStr::new("physics_test_domain")),
        backend: DispatchBackend::Cpu,
    }
}

fn region_capture(scene_id: u32, epoch: u32) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("RegionCapture"),
        fields: vec![
            (SmolStr::new("scene_id"), KernelValue::U32(scene_id)),
            (SmolStr::new("epoch"), KernelValue::U32(epoch)),
        ],
    })
}
