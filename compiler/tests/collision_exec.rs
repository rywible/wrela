use smol_str::SmolStr;
use wrela::collision_exec::cpu::{CollisionArtifactStore, execute_with_store};
use wrela::collision_plan::{CollisionPlan, CollisionQueryKind};
use wrela::hir;
use wrela::hir::lower as hir_lower;
use wrela::kernel::{KernelStructValue, KernelValue};
use wrela::parser::ast;
use wrela::parser::ast::AstNode;
use wrela::parser::parse;
use wrela::query_exec::{QueryExecContext, stable_region_scene_capture_id};
use wrela::state_advance::ChangeClass;

fn lower_inline_module_from_source(source: &str) -> hir::Module {
    let node = parse(source);
    let root = ast::Root::cast(node).expect("root");
    hir_lower::lower(root)
}

fn typed_query_module(source: &str) -> QueryExecContext {
    let module = lower_inline_module_from_source(source);
    let semantic = hir::semantic::check_module(&module);
    assert!(
        semantic.errors.is_empty(),
        "semantic errors: {:?}",
        semantic.errors
    );
    let (type_errors, type_info) = hir::typeck::check_module_with_info(&module);
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
    hit_epsilon = 0.001
    max_steps = 96
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

fn region_capture(scene_id: u32, epoch: u32) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("RegionCapture"),
        fields: vec![
            (SmolStr::new("scene_id"), KernelValue::U32(scene_id)),
            (SmolStr::new("epoch"), KernelValue::U32(epoch)),
        ],
    })
}

fn collision_transition_input(
    current_epoch: u32,
    previous_epoch: u32,
    change_class: ChangeClass,
) -> KernelValue {
    let change_class_id = match change_class {
        ChangeClass::None => 0,
        ChangeClass::Presentation => 1,
        ChangeClass::Structural => 2,
        ChangeClass::Topology => 3,
        ChangeClass::Identity => 4,
        ChangeClass::Incompatible => 5,
    };
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("CollisionSnapshotTransitionInput"),
        fields: vec![
            (
                SmolStr::new("current_snapshot_epoch"),
                KernelValue::U32(current_epoch),
            ),
            (
                SmolStr::new("previous_snapshot_epoch"),
                KernelValue::U32(previous_epoch),
            ),
            (
                SmolStr::new("change_class"),
                KernelValue::U32(change_class_id),
            ),
        ],
    })
}

fn collision_sweep_input(start_center: [f32; 3], end_center: [f32; 3], radius: f32) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("CollisionSphereSweepInput"),
        fields: vec![
            (
                SmolStr::new("start_center"),
                KernelValue::Vec3(start_center),
            ),
            (SmolStr::new("end_center"), KernelValue::Vec3(end_center)),
            (SmolStr::new("radius"), KernelValue::F32(radius)),
            (SmolStr::new("contact_tolerance"), KernelValue::F32(0.001)),
            (SmolStr::new("max_iterations"), KernelValue::I32(64)),
        ],
    })
}

fn assert_approx_eq(lhs: f32, rhs: f32) {
    assert!(
        (lhs - rhs).abs() < 0.02,
        "expected {lhs} ~= {rhs}, delta={}",
        (lhs - rhs).abs()
    );
}

#[test]
fn static_and_transition_collision_plans_execute_on_cpu() {
    let ctx = typed_query_module(collision_fixture_source());
    let scene_id = stable_region_scene_capture_id(&SmolStr::new("collision_region"));
    let domain = scene_domain(scene_id);
    let capture = region_capture(scene_id, 2);
    let transition = collision_transition_input(2, 1, ChangeClass::Presentation);
    let sweep = collision_sweep_input([0.0, 0.0, 2.0], [0.0, 0.0, -2.0], 0.25);

    let sweep_plan = CollisionPlan::for_query(CollisionQueryKind::SphereSweepTransition);
    let (result, trace) = sweep_plan
        .execute(
            &ctx,
            &[
                capture.clone(),
                domain.clone(),
                transition.clone(),
                sweep.clone(),
            ],
        )
        .expect("sphere sweep");
    assert_eq!(trace.reuse_metrics.unavailable_count, 2);
    assert!(
        trace.executed_query_contracts.len() >= 3,
        "expected transition sweep trace to record candidate, distance, and normal queries: {trace:?}"
    );
    match result {
        wrela::collision_contract::CollisionResult::Sweep(value) => {
            assert!(value.hit);
            let witness = value.witness.expect("witness");
            assert_approx_eq(witness.contact_fraction_upper_bound, 0.3125);
        }
        other => panic!("expected sweep result, got {other:?}"),
    }

    let toi_plan = CollisionPlan::for_query(CollisionQueryKind::SphereTimeOfImpactTransition);
    let (result, _) = toi_plan
        .execute(&ctx, &[capture, domain, transition, sweep])
        .expect("time of impact");
    match result {
        wrela::collision_contract::CollisionResult::TimeOfImpact(value) => {
            assert!(value.hit);
            assert_approx_eq(value.time_fraction_upper_bound.expect("toi"), 0.3125);
        }
        other => panic!("expected time-of-impact result, got {other:?}"),
    }
}

#[test]
fn transition_collision_reuse_decisions_report_consumed_and_rejected_paths() {
    let ctx = typed_query_module(collision_fixture_source());
    let scene_id = stable_region_scene_capture_id(&SmolStr::new("collision_region"));
    let domain = scene_domain(scene_id);
    let sweep = collision_sweep_input([0.0, 0.0, 2.0], [0.0, 0.0, -2.0], 0.25);
    let plan = CollisionPlan::for_query(CollisionQueryKind::SphereSweepTransition);
    let mut store = CollisionArtifactStore::default();

    let (_, first_trace) = execute_with_store(
        &plan,
        &ctx,
        &[
            region_capture(scene_id, 1),
            domain.clone(),
            collision_transition_input(1, 0, ChangeClass::Presentation),
            sweep.clone(),
        ],
        &mut store,
    )
    .expect("first sweep");
    assert_eq!(first_trace.reuse_metrics.unavailable_count, 2);

    let (_, second_trace) = execute_with_store(
        &plan,
        &ctx,
        &[
            region_capture(scene_id, 2),
            domain.clone(),
            collision_transition_input(2, 1, ChangeClass::Presentation),
            sweep.clone(),
        ],
        &mut store,
    )
    .expect("second sweep");
    assert!(second_trace.reuse_metrics.consumed_count >= 1);
    assert!(second_trace.reuse_metrics.diagnostics.iter().any(|entry| {
        entry.contains("artifact=artifact.witness_cache.sphere_sweep")
            && entry.contains("verdict=consumed")
    }));

    let (_, third_trace) = execute_with_store(
        &plan,
        &ctx,
        &[
            region_capture(scene_id, 3),
            domain,
            collision_transition_input(3, 1, ChangeClass::Presentation),
            sweep,
        ],
        &mut store,
    )
    .expect("third sweep");
    assert!(third_trace.reuse_metrics.rejected_count >= 1);
    assert!(third_trace.reuse_metrics.diagnostics.iter().any(|entry| {
        entry.contains("verdict=rejected") && entry.contains("reason=validity_rejected")
    }));
}

#[test]
fn transition_collision_rejects_out_of_authority_change_class() {
    let ctx = typed_query_module(collision_fixture_source());
    let scene_id = stable_region_scene_capture_id(&SmolStr::new("collision_region"));
    let domain = scene_domain(scene_id);
    let plan = CollisionPlan::for_query(CollisionQueryKind::SphereSweepTransition);
    let result = plan.execute(
        &ctx,
        &[
            region_capture(scene_id, 2),
            domain,
            collision_transition_input(2, 1, ChangeClass::Topology),
            collision_sweep_input([0.0, 0.0, 2.0], [0.0, 0.0, -2.0], 0.25),
        ],
    );
    assert!(
        matches!(
            result,
            Err(
                wrela::collision_plan::CollisionExecError::TransitionAuthorityExceeded {
                    observed: ChangeClass::Topology,
                    maximum: ChangeClass::Presentation,
                }
            )
        ),
        "expected topology transition to exceed declared authority, got {result:?}"
    );
}

#[test]
fn no_hit_certificate_is_reported_for_clear_transition_sweep() {
    let ctx = typed_query_module(collision_fixture_source());
    let scene_id = stable_region_scene_capture_id(&SmolStr::new("collision_region"));
    let domain = scene_domain(scene_id);
    let plan = CollisionPlan::for_query(CollisionQueryKind::SphereSweepTransition);
    let (result, trace) = plan
        .execute(
            &ctx,
            &[
                region_capture(scene_id, 2),
                domain,
                collision_transition_input(2, 1, ChangeClass::Presentation),
                collision_sweep_input([2.0, 0.0, 2.0], [2.0, 0.0, -2.0], 0.25),
            ],
        )
        .expect("clear sweep");
    assert_eq!(trace.reuse_metrics.unavailable_count, 2);
    match result {
        wrela::collision_contract::CollisionResult::Sweep(value) => {
            assert!(!value.hit);
            let certificate = value.no_hit_certificate.expect("no-hit certificate");
            assert_eq!(certificate.valid_through_fraction, 1.0);
        }
        other => panic!("expected sweep result, got {other:?}"),
    }
}
