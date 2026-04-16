use smol_str::SmolStr;
use wrela::artifact_key::ArtifactReuseKey;
use wrela::artifact_store::{ArtifactInstanceMetadata, ArtifactLookupRequest, StoredArtifact};
use wrela::collision_contract::{
    CollisionContactNormalFlavor, CollisionContactNormalProvenance, CollisionResult,
};
use wrela::collision_exec::cpu::{
    CollisionArtifactPayload, CollisionArtifactStore, CollisionContinuationSeed,
    CollisionStoredWitness, execute_with_store,
};
use wrela::collision_plan::{
    CollisionArtifactKind, CollisionPlan, CollisionQueryKind, collision_history_compatibility_hash,
};
use wrela::hir;
use wrela::hir::lower as hir_lower;
use wrela::kernel::{KernelStructValue, KernelValue};
use wrela::parser::ast;
use wrela::parser::ast::AstNode;
use wrela::parser::parse;
use wrela::query_exec::{QueryExecContext, stable_region_scene_capture_id};
use wrela::query_solver::{
    CertificateReuseClass, RayStepCertificate, RayStepCertificateMetadata,
    RayStepCertificateSubjectKind, RequiredGuaranteeClass, StepCertificateKind,
};
use wrela::state_advance::ChangeClass;
use wrela::world_identity::SnapshotEpoch;

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

domain collision_domain_coarse(world: RegionCapture) {
    geometry_detail = 0
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

fn collision_clutter_fixture_source() -> &'static str {
    r#"
field exact distance collision_center_field(p: Vec3) -> F32 {
    sphere(radius = 0.5)
}

field exact distance collision_left_field(p: Vec3) -> F32 {
    translate = vec3(-4.0, 0.0, 0.0) {
        sphere(radius = 0.5)
    }
}

field exact distance collision_right_field(p: Vec3) -> F32 {
    translate = vec3(4.0, 0.0, 0.0) {
        sphere(radius = 0.5)
    }
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

shape collision_center_shape {
    field = collision_center_field
    material = collision_surface
}

shape collision_left_shape {
    field = collision_left_field
    material = collision_surface
}

shape collision_right_shape {
    field = collision_right_field
    material = collision_surface
}

region collision_clutter_region() {
    place center = collision_center_shape
    place left = collision_left_shape
    place right = collision_right_shape
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
    scene_domain_with_detail(scene_id, 1)
}

fn scene_domain_with_detail(scene_id: u32, geometry_detail: i32) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("SceneDomain"),
        fields: vec![
            (SmolStr::new("scene_id"), KernelValue::U32(scene_id)),
            (
                SmolStr::new("spatial"),
                KernelValue::Struct(KernelStructValue {
                    name: SmolStr::new("SpatialDomainContract"),
                    fields: vec![(
                        SmolStr::new("geometry_detail"),
                        KernelValue::I32(geometry_detail),
                    )],
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

fn collision_point_input(point: [f32; 3]) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("CollisionPointInput"),
        fields: vec![(SmolStr::new("point"), KernelValue::Vec3(point))],
    })
}

fn collision_ray_input(origin: [f32; 3], direction: [f32; 3]) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("CollisionRayInput"),
        fields: vec![
            (SmolStr::new("origin"), KernelValue::Vec3(origin)),
            (SmolStr::new("direction"), KernelValue::Vec3(direction)),
            (SmolStr::new("max_distance"), KernelValue::F32(6.0)),
            (SmolStr::new("min_step"), KernelValue::F32(0.05)),
            (SmolStr::new("hit_epsilon"), KernelValue::F32(0.001)),
            (SmolStr::new("max_steps"), KernelValue::I32(96)),
        ],
    })
}

fn collision_sphere_probe(center: [f32; 3], radius: f32) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("CollisionSphereProbe"),
        fields: vec![
            (SmolStr::new("center"), KernelValue::Vec3(center)),
            (SmolStr::new("radius"), KernelValue::F32(radius)),
        ],
    })
}

fn collision_sweep_input(start_center: [f32; 3], end_center: [f32; 3], radius: f32) -> KernelValue {
    collision_sweep_input_with_iterations(start_center, end_center, radius, 64)
}

fn collision_sweep_input_with_iterations(
    start_center: [f32; 3],
    end_center: [f32; 3],
    radius: f32,
    max_iterations: i32,
) -> KernelValue {
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
            (
                SmolStr::new("max_iterations"),
                KernelValue::I32(max_iterations),
            ),
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

fn transition_certificate(reusable_by: CertificateReuseClass) -> RayStepCertificate {
    RayStepCertificate {
        kind: StepCertificateKind::RefinementBracket,
        metadata: RayStepCertificateMetadata {
            guarantee: RequiredGuaranteeClass::ConservativeNoFalseMiss,
            proof_family: SmolStr::new("collision.transition"),
            subject: SmolStr::new("collision.test"),
            subject_kind: RayStepCertificateSubjectKind::Interval,
            tolerance_context: SmolStr::new("collision test certificate"),
            reusable_by,
            invalidation_reasons: vec![SmolStr::new("collision test changed")],
        },
        t_start: 0.25,
        t_end: 0.3125,
        no_hit_before_t_end: true,
        bracket: Some([0.25, 0.3125]),
        provenance: None,
    }
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
    let sweep_bracket = trace.interval_bracket.expect("sweep interval bracket");
    assert_approx_eq(sweep_bracket[0], 0.3125);
    assert_approx_eq(sweep_bracket[1], 0.3125);
    assert_eq!(trace.fallback_count, 0);
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
            assert_eq!(
                witness.normal_flavor,
                wrela::collision_contract::CollisionContactNormalFlavor::SurfaceGradient
            );
            assert_eq!(
                witness.normal_provenance,
                CollisionContactNormalProvenance::FeatureNormal
            );
        }
        other => panic!("expected sweep result, got {other:?}"),
    }
    assert_eq!(
        trace.contact_normal_provenance,
        Some(CollisionContactNormalProvenance::FeatureNormal)
    );
    assert_eq!(trace.broadphase_candidate_count, 1);
    assert!(
        trace
            .executed_query_contracts
            .contains(&wrela::query_contract::SPATIAL_DISTANCE_CAPTURE_SHAPE),
        "expected sweep to evaluate the shape-capture distance contract: {trace:?}"
    );

    let toi_plan = CollisionPlan::for_query(CollisionQueryKind::SphereTimeOfImpactTransition);
    let (result, toi_trace) = toi_plan
        .execute(&ctx, &[capture, domain, transition, sweep])
        .expect("time of impact");
    let toi_bracket = toi_trace.interval_bracket.expect("toi interval bracket");
    assert_approx_eq(toi_bracket[0], 0.3125);
    assert_approx_eq(toi_bracket[1], 0.3125);
    assert_eq!(toi_trace.fallback_count, 0);
    assert!(
        toi_trace
            .executed_query_contracts
            .contains(&wrela::query_contract::SPATIAL_DISTANCE_CAPTURE_SHAPE),
        "expected TOI to evaluate the shape-capture distance contract: {toi_trace:?}"
    );
    match result {
        wrela::collision_contract::CollisionResult::TimeOfImpact(value) => {
            assert!(value.hit);
            assert_approx_eq(value.time_fraction_upper_bound.expect("toi"), 0.3125);
            let witness = value.witness.expect("toi witness");
            assert_eq!(
                witness.normal_provenance,
                CollisionContactNormalProvenance::FeatureNormal
            );
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
    assert_eq!(trace.broadphase_candidate_count, 1);
}

#[test]
fn collision_broadphase_reuse_is_keyed_by_query_input() {
    let ctx = typed_query_module(collision_fixture_source());
    let scene_id = stable_region_scene_capture_id(&SmolStr::new("collision_region"));
    let domain = scene_domain(scene_id);
    let plan = CollisionPlan::for_query(CollisionQueryKind::PointOccupancyWorld);
    let mut store = CollisionArtifactStore::default();

    let (_, first_trace) = execute_with_store(
        &plan,
        &ctx,
        &[
            region_capture(scene_id, 2),
            domain.clone(),
            collision_point_input([0.0, 0.0, 0.25]),
        ],
        &mut store,
    )
    .expect("first point occupancy");
    assert!(
        first_trace
            .executed_query_contracts
            .contains(&wrela::query_contract::SPATIAL_DISTANCE_CAPTURE_SHAPE)
    );
    assert!(
        first_trace
            .executed_query_contracts
            .contains(&wrela::query_contract::SPATIAL_NORMAL_CAPTURE_SHAPE)
    );

    let (result, second_trace) = execute_with_store(
        &plan,
        &ctx,
        &[
            region_capture(scene_id, 2),
            domain,
            collision_point_input([20.0, 0.0, 20.0]),
        ],
        &mut store,
    )
    .expect("second point occupancy");
    assert_eq!(second_trace.broadphase_candidate_count, 0);
    match result {
        CollisionResult::Occupancy(value) => {
            assert!(!value.occupied);
            assert!(value.signed_distance > 0.0);
        }
        other => panic!("expected occupancy result, got {other:?}"),
    }
}

#[test]
fn shared_broadphase_prunes_far_shapes_on_cluttered_collision_scene() {
    let ctx = typed_query_module(collision_clutter_fixture_source());
    let scene_id = stable_region_scene_capture_id(&SmolStr::new("collision_clutter_region"));
    let domain = scene_domain(scene_id);
    let plan = CollisionPlan::for_query(CollisionQueryKind::PointOccupancyWorld);

    let (_, trace) = plan
        .execute(
            &ctx,
            &[
                region_capture(scene_id, 2),
                domain,
                collision_point_input([0.0, 0.0, 0.25]),
            ],
        )
        .expect("cluttered point occupancy");
    assert_eq!(trace.broadphase_candidate_count, 1);
    assert!(
        trace.broadphase_rejected_candidate_count >= 2,
        "expected the shared broadphase to reject the distant clutter shapes: {trace:?}"
    );
}

#[test]
fn collision_artifact_reuse_distinguishes_scene_domain_detail() {
    let ctx = typed_query_module(collision_fixture_source());
    let scene_id = stable_region_scene_capture_id(&SmolStr::new("collision_region"));
    let fine_domain = scene_domain(scene_id);
    let coarse_domain = scene_domain_with_detail(scene_id, 0);
    let plan = CollisionPlan::for_query(CollisionQueryKind::PointOccupancyWorld);
    let mut store = CollisionArtifactStore::default();

    let (_, first_trace) = execute_with_store(
        &plan,
        &ctx,
        &[
            region_capture(scene_id, 2),
            fine_domain.clone(),
            collision_point_input([0.0, 0.0, 0.25]),
        ],
        &mut store,
    )
    .expect("fine point occupancy");
    assert_eq!(first_trace.broadphase_candidate_count, 1);

    let (_, second_trace) = execute_with_store(
        &plan,
        &ctx,
        &[
            region_capture(scene_id, 2),
            coarse_domain,
            collision_point_input([0.0, 0.0, 0.25]),
        ],
        &mut store,
    )
    .expect("coarse point occupancy");
    let support_artifact = plan
        .artifacts
        .iter()
        .find(|artifact| artifact.kind == CollisionArtifactKind::SupportSummary)
        .expect("support artifact");
    let broadphase_artifact = plan
        .artifacts
        .iter()
        .find(|artifact| artifact.kind == CollisionArtifactKind::BroadphaseCandidates)
        .expect("broadphase artifact");
    let support_bucket_count = second_trace
        .artifact_store
        .buckets
        .iter()
        .filter(|bucket| bucket.contract_id == support_artifact.id)
        .count();
    let broadphase_bucket_count = second_trace
        .artifact_store
        .buckets
        .iter()
        .filter(|bucket| bucket.contract_id == broadphase_artifact.id)
        .count();
    assert_eq!(support_bucket_count, 2);
    assert_eq!(broadphase_bucket_count, 2);
}

#[test]
fn transition_collision_dense_fallback_detects_contact_after_iteration_budget_exhaustion() {
    let ctx = typed_query_module(collision_fixture_source());
    let scene_id = stable_region_scene_capture_id(&SmolStr::new("collision_region"));
    let domain = scene_domain(scene_id);
    let transition = collision_transition_input(2, 1, ChangeClass::Presentation);
    let sweep = collision_sweep_input_with_iterations([-1.0, 0.74, 0.0], [1.0, 0.74, 0.0], 0.25, 1);
    let plan = CollisionPlan::for_query(CollisionQueryKind::SphereSweepTransition);

    let (result, trace) = execute_with_store(
        &plan,
        &ctx,
        &[region_capture(scene_id, 2), domain, transition, sweep],
        &mut CollisionArtifactStore::default(),
    )
    .expect("dense fallback sweep");
    assert_eq!(trace.fallback_count, 1);
    assert_eq!(trace.certificate_successes, 0);
    match result {
        CollisionResult::Sweep(value) => {
            assert!(value.hit);
            let witness = value.witness.expect("fallback sweep witness");
            assert!(
                witness.contact_fraction_upper_bound > 0.2
                    && witness.contact_fraction_upper_bound < 0.8
            );
        }
        other => panic!("expected sweep result, got {other:?}"),
    }
}

#[test]
fn transition_collision_dense_fallback_only_certifies_proven_prefix_on_no_hit() {
    let ctx = typed_query_module(collision_fixture_source());
    let scene_id = stable_region_scene_capture_id(&SmolStr::new("collision_region"));
    let domain = scene_domain(scene_id);
    let transition = collision_transition_input(2, 1, ChangeClass::Presentation);
    let sweep =
        collision_sweep_input_with_iterations([-1.0, 0.54, 0.54], [1.0, 0.54, 0.54], 0.25, 1);
    let plan = CollisionPlan::for_query(CollisionQueryKind::SphereTimeOfImpactTransition);

    let (result, trace) = execute_with_store(
        &plan,
        &ctx,
        &[region_capture(scene_id, 2), domain, transition, sweep],
        &mut CollisionArtifactStore::default(),
    )
    .expect("dense fallback toi");
    assert!(trace.broadphase_candidate_count > 0);
    assert_eq!(trace.fallback_count, 1);
    assert_eq!(trace.certificate_successes, 0);
    match result {
        CollisionResult::TimeOfImpact(value) => {
            assert!(!value.hit);
            let certificate = value
                .no_hit_certificate
                .expect("partial no-hit certificate");
            assert!(certificate.valid_through_fraction < 1.0);
        }
        other => panic!("expected time-of-impact result, got {other:?}"),
    }
}

#[test]
fn static_collision_paths_use_candidate_capture_queries() {
    let ctx = typed_query_module(collision_fixture_source());
    let scene_id = stable_region_scene_capture_id(&SmolStr::new("collision_region"));
    let domain = scene_domain(scene_id);
    let capture = region_capture(scene_id, 2);

    let occupancy_plan = CollisionPlan::for_query(CollisionQueryKind::PointOccupancyWorld);
    let (_, occupancy_trace) = occupancy_plan
        .execute(
            &ctx,
            &[
                capture.clone(),
                domain.clone(),
                collision_point_input([0.0, 0.0, 0.25]),
            ],
        )
        .expect("point occupancy");
    assert!(
        occupancy_trace
            .executed_query_contracts
            .contains(&wrela::query_contract::SPATIAL_DISTANCE_CAPTURE_SHAPE),
        "expected point occupancy to evaluate the shape-capture distance contract: {occupancy_trace:?}"
    );
    assert!(
        occupancy_trace
            .executed_query_contracts
            .contains(&wrela::query_contract::SPATIAL_NORMAL_CAPTURE_SHAPE),
        "expected point occupancy to evaluate the shape-capture normal contract: {occupancy_trace:?}"
    );

    let ray_plan = CollisionPlan::for_query(CollisionQueryKind::RayCastWorld);
    let (_, ray_trace) = ray_plan
        .execute(
            &ctx,
            &[
                capture.clone(),
                domain.clone(),
                collision_ray_input([0.0, 0.0, 2.0], [0.0, 0.0, -1.0]),
            ],
        )
        .expect("ray cast");
    assert!(
        ray_trace
            .executed_query_contracts
            .contains(&wrela::query_contract::SPATIAL_TRACE_CAPTURE_SHAPE),
        "expected ray casting to evaluate the shape-capture trace contract: {ray_trace:?}"
    );

    let overlap_plan = CollisionPlan::for_query(CollisionQueryKind::SphereOverlapWorld);
    let (_, overlap_trace) = overlap_plan
        .execute(
            &ctx,
            &[
                capture,
                domain,
                collision_sphere_probe([0.0, 0.0, 0.9], 0.6),
            ],
        )
        .expect("sphere overlap");
    assert!(
        overlap_trace
            .executed_query_contracts
            .contains(&wrela::query_contract::SPATIAL_DISTANCE_CAPTURE_SHAPE),
        "expected sphere overlap to evaluate the shape-capture distance contract: {overlap_trace:?}"
    );
}

#[test]
fn static_collision_paths_execute_on_wgsl_and_use_world_queries() {
    let ctx = typed_query_module(collision_fixture_source());
    let scene_id = stable_region_scene_capture_id(&SmolStr::new("collision_region"));
    let domain = scene_domain(scene_id);
    let capture = region_capture(scene_id, 1);

    for (kind, args, expected_kind) in [
        (
            CollisionQueryKind::PointOccupancyWorld,
            vec![
                capture.clone(),
                domain.clone(),
                collision_point_input([0.0, 0.0, 0.25]),
            ],
            "occupancy",
        ),
        (
            CollisionQueryKind::RayCastWorld,
            vec![
                capture.clone(),
                domain.clone(),
                collision_ray_input([0.0, 0.0, 2.0], [0.0, 0.0, -1.0]),
            ],
            "ray",
        ),
        (
            CollisionQueryKind::SphereOverlapWorld,
            vec![
                capture.clone(),
                domain.clone(),
                collision_sphere_probe([0.0, 0.0, 0.9], 0.6),
            ],
            "overlap",
        ),
    ] {
        let plan = CollisionPlan::for_query_with_backend(
            kind,
            wrela::query_contract::DispatchBackend::Wgsl,
        );
        assert!(
            plan.validate().is_empty(),
            "expected Wgsl {expected_kind} plan to validate cleanly: {:?}",
            plan.validate()
        );
        let (result, trace) = plan.execute(&ctx, &args).expect("wgsl collision execution");
        assert_eq!(trace.backend, wrela::query_contract::DispatchBackend::Wgsl);
        match (kind, result) {
            (CollisionQueryKind::PointOccupancyWorld, CollisionResult::Occupancy(value)) => {
                assert!(value.occupied);
                assert!(value.signed_distance < 0.0);
                assert!(
                    trace
                        .executed_query_contracts
                        .contains(&wrela::query_contract::SPATIAL_DISTANCE_BATCH_WORLD),
                    "expected Wgsl occupancy to use the batch world distance contract: {trace:?}"
                );
                assert!(
                    trace
                        .executed_query_contracts
                        .contains(&wrela::query_contract::SPATIAL_NORMAL_BATCH_WORLD),
                    "expected Wgsl occupancy to use the batch world normal contract: {trace:?}"
                );
                assert!(
                    !trace
                        .executed_query_contracts
                        .contains(&wrela::query_contract::SPATIAL_DISTANCE_CAPTURE_SHAPE),
                    "expected Wgsl occupancy to avoid the capture distance contract: {trace:?}"
                );
                assert!(
                    !trace
                        .executed_query_contracts
                        .contains(&wrela::query_contract::SPATIAL_DISTANCE_WORLD),
                    "expected Wgsl occupancy to avoid the direct world distance contract: {trace:?}"
                );
            }
            (CollisionQueryKind::RayCastWorld, CollisionResult::RayCast(value)) => {
                assert!(value.hit);
                let witness = value.witness.expect("ray witness");
                assert!(witness.travel_distance > 1.0 && witness.travel_distance < 2.0);
                assert!(
                    trace
                        .executed_query_contracts
                        .contains(&wrela::query_contract::SPATIAL_NEAREST_BATCH_WORLD),
                    "expected Wgsl ray casting to use the batch world trace contract: {trace:?}"
                );
                assert!(
                    !trace
                        .executed_query_contracts
                        .contains(&wrela::query_contract::SPATIAL_TRACE_CAPTURE_SHAPE),
                    "expected Wgsl ray casting to avoid the capture trace contract: {trace:?}"
                );
                assert!(
                    !trace
                        .executed_query_contracts
                        .contains(&wrela::query_contract::SPATIAL_NEAREST_WORLD),
                    "expected Wgsl ray casting to avoid the direct world trace contract: {trace:?}"
                );
            }
            (CollisionQueryKind::SphereOverlapWorld, CollisionResult::SphereOverlap(value)) => {
                assert!(value.overlaps);
                assert!(value.signed_separation < 0.0);
                assert!(
                    trace
                        .executed_query_contracts
                        .contains(&wrela::query_contract::SPATIAL_DISTANCE_BATCH_WORLD),
                    "expected Wgsl overlap to use the batch world distance contract: {trace:?}"
                );
                assert!(
                    trace
                        .executed_query_contracts
                        .contains(&wrela::query_contract::SPATIAL_NORMAL_BATCH_WORLD),
                    "expected Wgsl overlap to use the batch world normal contract: {trace:?}"
                );
                assert!(
                    !trace
                        .executed_query_contracts
                        .contains(&wrela::query_contract::SPATIAL_DISTANCE_CAPTURE_SHAPE),
                    "expected Wgsl overlap to avoid the capture distance contract: {trace:?}"
                );
                assert!(
                    !trace
                        .executed_query_contracts
                        .contains(&wrela::query_contract::SPATIAL_DISTANCE_WORLD),
                    "expected Wgsl overlap to avoid the direct world distance contract: {trace:?}"
                );
            }
            other => panic!("unexpected collision result for {kind:?}: {other:?}"),
        }
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
    let current_snapshot =
        wrela::query_exec::stable_region_snapshot_handle(&SmolStr::new("collision_region"))
            .with_epoch(SnapshotEpoch(2));
    for (kind, artifact_id, normal_flavor) in [
        (
            CollisionArtifactKind::WitnessCache,
            "artifact.witness_cache.sphere_sweep",
            CollisionContactNormalFlavor::SurfaceGradient,
        ),
        (
            CollisionArtifactKind::ContinuationSeed,
            "artifact.continuation_seed.sphere_sweep",
            CollisionContactNormalFlavor::SurfaceGradient,
        ),
    ] {
        let artifact = plan
            .artifacts
            .iter()
            .find(|artifact| artifact.id == artifact_id)
            .expect("transition artifact");
        let (stored, report) = store.lookup(&ArtifactLookupRequest {
            contract: artifact.contract.clone(),
            reuse_key: None,
            current_snapshot: current_snapshot.clone(),
            previous_snapshot_epoch: Some(SnapshotEpoch(1)),
            change_class: Some(ChangeClass::Presentation),
            policy_digest: Some(policy_digest(plan.policy)),
            presentation_frame: None,
            layout_signature: None,
            history_compatibility_hash: Some(collision_history_compatibility_hash(
                plan.contract_id,
                kind,
                Some(normal_flavor),
            )),
            evidence_summary: Some(artifact.contract.evidence_summary.clone()),
        });
        assert!(
            stored.is_some(),
            "expected transition artifact with history hash to be reusable: {report:?}"
        );
    }

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
fn transition_collision_reuse_reduces_followup_refinement_work() {
    let ctx = typed_query_module(collision_fixture_source());
    let scene_id = stable_region_scene_capture_id(&SmolStr::new("collision_region"));
    let domain = scene_domain(scene_id);
    let sweep = collision_sweep_input([0.0, 0.0, 2.0], [0.0, 0.0, -2.0], 0.25);
    let plan = CollisionPlan::for_query(CollisionQueryKind::SphereSweepTransition);
    let transition = collision_transition_input(2, 1, ChangeClass::Presentation);

    let (_, baseline_trace) = execute_with_store(
        &plan,
        &ctx,
        &[
            region_capture(scene_id, 2),
            domain.clone(),
            transition.clone(),
            sweep.clone(),
        ],
        &mut CollisionArtifactStore::default(),
    )
    .expect("baseline sweep without reuse");

    let mut store = CollisionArtifactStore::default();
    execute_with_store(
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
    .expect("seed sweep");
    let (_, reused_trace) = execute_with_store(
        &plan,
        &ctx,
        &[region_capture(scene_id, 2), domain, transition, sweep],
        &mut store,
    )
    .expect("followup sweep with reuse");

    assert!(reused_trace.reuse_metrics.consumed_count >= 1);
    assert!(reused_trace.interval_subdivisions <= baseline_trace.interval_subdivisions);
    assert!(reused_trace.interval_refinements <= baseline_trace.interval_refinements);
    assert!(
        reused_trace.interval_subdivisions < baseline_trace.interval_subdivisions
            || reused_trace.interval_refinements < baseline_trace.interval_refinements,
        "expected continuation reuse to reduce interval work: baseline={baseline_trace:?} reused={reused_trace:?}"
    );
}

#[test]
fn transition_collision_rejects_rendering_only_certificates_for_reuse() {
    let ctx = typed_query_module(collision_fixture_source());
    let scene_id = stable_region_scene_capture_id(&SmolStr::new("collision_region"));
    let domain = scene_domain(scene_id);
    let sweep = collision_sweep_input([0.0, 0.0, 2.0], [0.0, 0.0, -2.0], 0.25);
    let plan = CollisionPlan::for_query(CollisionQueryKind::SphereSweepTransition);
    let mut store = CollisionArtifactStore::default();
    let previous_snapshot =
        wrela::query_exec::stable_region_snapshot_handle(&SmolStr::new("collision_region"))
            .with_epoch(SnapshotEpoch(1));
    let policy_digest = policy_digest(plan.policy);
    let witness = plan
        .artifacts
        .iter()
        .find(|artifact| artifact.kind == CollisionArtifactKind::WitnessCache)
        .expect("witness artifact");
    let continuation = plan
        .artifacts
        .iter()
        .find(|artifact| artifact.kind == CollisionArtifactKind::ContinuationSeed)
        .expect("continuation artifact");
    let rendering_only = transition_certificate(CertificateReuseClass::RenderingOnly);

    for artifact in [witness, continuation] {
        store.insert(StoredArtifact {
            contract: artifact.contract.clone(),
            metadata: ArtifactInstanceMetadata {
                snapshot: previous_snapshot.clone(),
                reuse_key: ArtifactReuseKey::new(
                    &previous_snapshot,
                    Some(artifact.id.clone()),
                    artifact.contract.logical_schema.describe(),
                    artifact.contract.logical_schema.stable_hash(),
                    Some(policy_digest),
                    artifact.contract.compatibility.policy.mode,
                ),
                policy_digest: Some(policy_digest),
                presentation_frame: None,
                layout_signature: None,
                history_compatibility_hash: None,
                evidence_summary: artifact.contract.evidence_summary.clone(),
            },
            payload: if artifact.kind == CollisionArtifactKind::WitnessCache {
                CollisionArtifactPayload::WitnessCache(CollisionStoredWitness {
                    hit: true,
                    contact_fraction_upper_bound: Some(0.3125),
                    separation_upper_bound: Some(-0.2),
                    normal_provenance: Some(
                        CollisionContactNormalProvenance::CertifiedFieldGradient,
                    ),
                    normal_flavor: CollisionContactNormalFlavor::SurfaceGradient,
                    certificate: rendering_only.clone(),
                })
            } else {
                CollisionArtifactPayload::ContinuationSeed(CollisionContinuationSeed {
                    fraction_hint: 0.3125,
                    no_hit_certificate: true,
                    separation_upper_bound: Some(-0.2),
                    normal_provenance: Some(
                        CollisionContactNormalProvenance::CertifiedFieldGradient,
                    ),
                    normal_flavor: CollisionContactNormalFlavor::SurfaceGradient,
                    certificate: rendering_only.clone(),
                })
            },
        });
    }

    let (_, trace) = execute_with_store(
        &plan,
        &ctx,
        &[
            region_capture(scene_id, 2),
            domain,
            wrela::kernel::KernelValue::Struct(KernelStructValue {
                name: SmolStr::new("CollisionSnapshotTransitionInput"),
                fields: vec![
                    (SmolStr::new("current_snapshot_epoch"), KernelValue::U32(2)),
                    (SmolStr::new("previous_snapshot_epoch"), KernelValue::U32(1)),
                    (SmolStr::new("change_class"), KernelValue::U32(1)),
                ],
            }),
            sweep,
        ],
        &mut store,
    )
    .expect("rendering-only sweep");
    assert!(trace.reuse_metrics.rejected_count >= 1);
    assert!(
        trace
            .reuse_metrics
            .diagnostics
            .iter()
            .any(|entry| entry.contains("reason=rendering_only_certificate"))
    );
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
                collision_sweep_input([20.0, 0.0, 20.0], [20.0, 0.0, -20.0], 0.25),
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
