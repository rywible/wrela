use smol_str::SmolStr;
use wrela::artifact_store::ArtifactLookupRequest;
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

fn policy_digest(policy: wrela::collision_contract::CollisionExecutionPolicy) -> u64 {
    let backend_tag = [match policy.backend_preference {
        wrela::query_contract::DispatchBackend::Cpu => 0,
        wrela::query_contract::DispatchBackend::VirtualGpu => 1,
        wrela::query_contract::DispatchBackend::Wgsl => 2,
        wrela::query_contract::DispatchBackend::Auto => 3,
    }];
    wrela::query_exec::ids::stable_semantic_id(&[
        &policy.required_guarantee.id().to_le_bytes(),
        &policy.selected_method.id().to_le_bytes(),
        &backend_tag,
    ])
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
    assert!(trace.broadphase_candidate_count > 0);
    assert!(trace.interval_subdivisions > 0);
    assert!(trace.interval_refinements > 0);
    assert!(trace.certificate_successes > 0);
    assert!(
        trace
            .executed_query_contracts
            .contains(&wrela::query_contract::SUPPORT_SUMMARY_WORLD),
        "expected transition sweep trace to execute the support summary query: {trace:?}"
    );
    assert!(
        trace.executed_query_contracts.len() >= 3,
        "expected transition sweep trace to record support, distance, and normal queries: {trace:?}"
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
fn transition_collision_materializes_a_typed_broadphase_payload() {
    let ctx = typed_query_module(collision_fixture_source());
    let scene_id = stable_region_scene_capture_id(&SmolStr::new("collision_region"));
    let domain = scene_domain(scene_id);
    let transition = collision_transition_input(2, 1, ChangeClass::Presentation);
    let sweep = collision_sweep_input([0.0, 0.0, 2.0], [0.0, 0.0, -2.0], 0.25);
    let plan = CollisionPlan::for_query(CollisionQueryKind::SphereSweepTransition);
    let mut store = CollisionArtifactStore::default();

    let (_, trace) = execute_with_store(
        &plan,
        &ctx,
        &[
            region_capture(scene_id, 2),
            domain.clone(),
            transition,
            sweep,
        ],
        &mut store,
    )
    .expect("transition sweep with store");
    assert_eq!(trace.artifact_store.entries, 4);
    assert!(trace.broadphase_candidate_count > 0);
    assert!(trace.interval_subdivisions > 0);
    assert!(trace.interval_refinements > 0);
    assert!(trace.certificate_successes > 0);

    let broadphase_artifact = plan
        .artifacts
        .iter()
        .find(|artifact| {
            artifact.kind == wrela::collision_plan::CollisionArtifactKind::BroadphaseCandidates
        })
        .expect("broadphase artifact");
    let current_snapshot =
        wrela::query_exec::stable_region_snapshot_handle(&SmolStr::new("collision_region"))
            .with_epoch(wrela::world_identity::SnapshotEpoch(2));
    let (artifact, report) = store.lookup(&ArtifactLookupRequest {
        contract: broadphase_artifact.contract.clone(),
        reuse_key: None,
        current_snapshot,
        previous_snapshot_epoch: None,
        change_class: None,
        policy_digest: Some(policy_digest(plan.policy)),
        presentation_frame: None,
        layout_signature: None,
        history_compatibility_hash: None,
        evidence_summary: Some(broadphase_artifact.contract.evidence_summary.clone()),
    });
    let artifact = artifact.expect("broadphase artifact lookup");
    assert_eq!(report.index_candidates, 1);
    match &artifact.payload {
        wrela::collision_exec::cpu::CollisionArtifactPayload::BroadphaseCandidates(payload) => {
            assert_eq!(payload.candidate_shape_names.len(), 1);
            assert_eq!(
                payload.candidate_shape_names[0],
                SmolStr::new("collision_shape")
            );
        }
        other => panic!("expected typed broadphase payload, got {other:?}"),
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
    assert!(third_trace.broadphase_candidate_count > 0);
    assert!(third_trace.interval_subdivisions > 0);
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
