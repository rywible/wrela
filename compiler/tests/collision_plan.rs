use smol_str::SmolStr;
use wrela::collision_contract::{
    COLLISION_POINT_OCCUPANCY_WORLD, COLLISION_SPHERE_SWEEP_TRANSITION,
    COLLISION_TIME_OF_IMPACT_TRANSITION, CollisionAuthorityScope, CollisionContactNormalFlavor,
    CollisionContactNormalProvenance, CollisionOccupancyClass, CollisionResult,
    CollisionTargetKind, collision_contracts,
};
use wrela::collision_plan::{
    CollisionArtifactKind, CollisionPlan, CollisionQueryKind, collision_plans_with_backend,
};
use wrela::hir;
use wrela::hir::lower as hir_lower;
use wrela::kernel::{KernelStructValue, KernelValue, lower_world_query_plan};
use wrela::parser::ast;
use wrela::parser::ast::AstNode;
use wrela::parser::parse;
use wrela::query_contract::DispatchBackend;
use wrela::query_exec::{
    QueryExecContext, execute_world_query_with_trace_on, stable_region_scene_capture_id,
};
use wrela::query_plan::{WorldQueryKind, WorldQueryPlan};
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

fn expect_f32(value: &KernelValue) -> f32 {
    match value {
        KernelValue::F32(value) => *value,
        other => panic!("expected F32, got {other:?}"),
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
fn collision_contract_registry_exposes_static_and_transition_authority() {
    let ids = collision_contracts()
        .iter()
        .map(|descriptor| descriptor.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        ids,
        vec![
            "collision.point_occupancy.world",
            "collision.ray_cast.world",
            "collision.sphere_overlap.world",
            "collision.sphere_sweep.transition",
            "collision.time_of_impact.transition",
        ]
    );
    assert!(collision_contracts().iter().any(|descriptor| {
        descriptor.id == COLLISION_SPHERE_SWEEP_TRANSITION
            && descriptor.target == CollisionTargetKind::WorldTransition
            && descriptor.authority.scope == CollisionAuthorityScope::Transition
    }));
    assert!(collision_contracts().iter().any(|descriptor| {
        descriptor.id == COLLISION_TIME_OF_IMPACT_TRANSITION
            && descriptor.witness_schema.name == "CollisionTimeOfImpactWitness"
    }));
}

#[test]
fn collision_plans_validate_transition_artifacts_and_witness_declarations() {
    for plan in collision_plans_with_backend(DispatchBackend::Auto) {
        assert!(
            plan.validate().is_empty(),
            "expected '{}' to validate cleanly: {:?}",
            plan.name,
            plan.validate()
        );
        assert_eq!(plan.outputs.len(), 1);
        assert!(plan.outputs[0].witness_schema.is_some());
        assert!(!plan.artifact_uses().is_empty());
        if plan.target == CollisionTargetKind::WorldTransition {
            let kinds = plan
                .artifacts
                .iter()
                .map(|artifact| artifact.kind)
                .collect::<Vec<_>>();
            assert!(kinds.contains(&CollisionArtifactKind::WitnessCache));
            assert!(kinds.contains(&CollisionArtifactKind::ContinuationSeed));
        }
        let kinds = plan
            .artifacts
            .iter()
            .map(|artifact| artifact.kind)
            .collect::<Vec<_>>();
        assert!(kinds.contains(&CollisionArtifactKind::BroadphaseCandidates));
    }
}

#[test]
fn transition_collision_validation_reports_conservative_and_interval_methods() {
    let sweep = CollisionPlan::for_query_with_backend(
        CollisionQueryKind::SphereSweepTransition,
        DispatchBackend::Wgsl,
    );
    let toi = CollisionPlan::for_query_with_backend(
        CollisionQueryKind::SphereTimeOfImpactTransition,
        DispatchBackend::Wgsl,
    );
    let sweep_errors = sweep.validate();
    let toi_errors = toi.validate();
    assert!(sweep_errors.iter().any(|error| {
        error
            .message
            .contains("required_guarantee=conservative_no_false_miss")
            && error
                .message
                .contains("selected_method=conservative_solver")
    }));
    assert!(toi_errors.iter().any(|error| {
        error
            .message
            .contains("required_guarantee=interval_bounded")
            && error.message.contains("selected_method=interval_solver")
    }));
}

#[test]
fn static_collision_outputs_remain_cpu_oracle_checkable() {
    let ctx = typed_query_module(collision_fixture_source());
    let region_capture = KernelValue::Capture(SmolStr::new("collision_region"));
    let region_scene_id = stable_region_scene_capture_id(&SmolStr::new("collision_region"));
    let domain = scene_domain(region_scene_id);

    let occupancy_plan = CollisionPlan::for_query(CollisionQueryKind::PointOccupancyWorld);
    let (occupancy, trace) = occupancy_plan
        .execute(
            &ctx,
            &[
                region_capture.clone(),
                domain.clone(),
                collision_point_input([0.0, 0.0, 0.25]),
            ],
        )
        .expect("point occupancy");
    assert_eq!(trace.contract_id, COLLISION_POINT_OCCUPANCY_WORLD);
    match occupancy {
        CollisionResult::Occupancy(value) => {
            assert_eq!(value.classification, CollisionOccupancyClass::Occupied);
            assert!(value.occupied);
            assert_approx_eq(value.signed_distance, -0.25);
            assert_approx_eq(value.witness.nearest_point_on_world[2], 0.5);
        }
        other => panic!("expected occupancy result, got {other:?}"),
    }

    let distance_plan =
        lower_world_query_plan(&WorldQueryPlan::for_query(WorldQueryKind::Distance));
    let (distance_value, _) = execute_world_query_with_trace_on(
        &ctx,
        DispatchBackend::Cpu,
        &distance_plan,
        &[
            region_capture.clone(),
            domain.clone(),
            KernelValue::Vec3([0.0, 0.0, 0.25]),
        ],
    )
    .expect("world distance");
    assert_approx_eq(expect_f32(&distance_value), -0.25);

    let ray_plan = CollisionPlan::for_query(CollisionQueryKind::RayCastWorld);
    let (ray_cast, _) = ray_plan
        .execute(
            &ctx,
            &[
                region_capture.clone(),
                domain.clone(),
                collision_ray_input([0.0, 0.0, 2.0], [0.0, 0.0, -1.0]),
            ],
        )
        .expect("ray cast");
    match ray_cast {
        CollisionResult::RayCast(value) => {
            assert!(value.hit);
            let witness = value.witness.expect("ray witness");
            assert_approx_eq(witness.travel_distance, 1.5);
            assert_approx_eq(witness.position[2], 0.5);
        }
        other => panic!("expected ray cast result, got {other:?}"),
    }

    let overlap_plan = CollisionPlan::for_query(CollisionQueryKind::SphereOverlapWorld);
    let (overlap, overlap_trace) = overlap_plan
        .execute(
            &ctx,
            &[
                region_capture.clone(),
                domain.clone(),
                collision_sphere_probe([0.0, 0.0, 0.9], 0.6),
            ],
        )
        .expect("sphere overlap");
    assert_eq!(overlap_trace.artifact_store.entries, 2);
    assert!(overlap_trace.broadphase_candidate_count > 0);
    match overlap {
        CollisionResult::SphereOverlap(value) => {
            assert!(value.overlaps);
            assert_approx_eq(value.signed_separation, -0.2);
            assert_approx_eq(value.witness.point_on_probe[2], 0.3);
            assert_approx_eq(value.witness.point_on_world[2], 0.5);
        }
        other => panic!("expected sphere overlap result, got {other:?}"),
    }
}

#[test]
fn transition_collision_plans_execute_with_contact_fraction_and_normal_flavor() {
    let ctx = typed_query_module(collision_fixture_source());
    let region_scene_id = stable_region_scene_capture_id(&SmolStr::new("collision_region"));
    let capture = region_capture(region_scene_id, 2);
    let domain = scene_domain(region_scene_id);
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
    assert_eq!(
        trace
            .transition
            .expect("transition")
            .previous_snapshot_epoch,
        1
    );
    match result {
        CollisionResult::Sweep(value) => {
            assert!(value.hit);
            assert!(value.no_hit_certificate.is_none());
            let witness = value.witness.expect("sweep witness");
            assert_approx_eq(witness.contact_fraction_upper_bound, 0.3125);
            assert_eq!(
                witness.normal_flavor,
                CollisionContactNormalFlavor::SurfaceGradient
            );
            assert_eq!(
                witness.normal_provenance,
                CollisionContactNormalProvenance::FeatureNormal
            );
            assert_approx_eq(witness.point_on_probe[2], 0.5);
            assert_approx_eq(witness.point_on_world[2], 0.5);
        }
        other => panic!("expected sweep result, got {other:?}"),
    }
    assert_eq!(
        trace.contact_normal_provenance,
        Some(CollisionContactNormalProvenance::FeatureNormal)
    );
    let sweep_bracket = trace.interval_bracket.expect("sweep interval bracket");
    assert_approx_eq(sweep_bracket[0], 0.3125);
    assert_approx_eq(sweep_bracket[1], 0.3125);
    assert_eq!(trace.fallback_count, 0);

    let toi_plan = CollisionPlan::for_query(CollisionQueryKind::SphereTimeOfImpactTransition);
    let (result, toi_trace) = toi_plan
        .execute(&ctx, &[capture, domain, transition, sweep])
        .expect("time of impact");
    let toi_bracket = toi_trace.interval_bracket.expect("toi interval bracket");
    assert_approx_eq(toi_bracket[0], 0.3125);
    assert_approx_eq(toi_bracket[1], 0.3125);
    assert_eq!(toi_trace.fallback_count, 0);
    match result {
        CollisionResult::TimeOfImpact(value) => {
            assert!(value.hit);
            assert!(value.no_hit_certificate.is_none());
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
fn collision_plan_surfaces_acceleration_artifacts() {
    let plan = CollisionPlan::for_query(CollisionQueryKind::RayCastWorld);
    let contracts = plan.semantic_artifact_contracts();

    assert!(contracts.iter().any(|contract| {
        contract.id == "shared_acceleration_forest"
            && contract.compatibility.evidence.scope
                == wrela::semantic_evidence::EvidenceScope::SnapshotLocal
    }));
    assert!(contracts.iter().any(|contract| {
        contract.id == "distance_brick_cache"
            && contract.compatibility.evidence.scope
                == wrela::semantic_evidence::EvidenceScope::SnapshotLocal
    }));
    assert!(contracts.iter().any(|contract| {
        contract.id == "continuation_seed_table"
            && contract.compatibility.evidence.scope
                == wrela::semantic_evidence::EvidenceScope::ArtifactBound
    }));
    assert!(plan.validate().is_empty());
}
