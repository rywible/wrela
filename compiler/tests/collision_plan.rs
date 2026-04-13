use smol_str::SmolStr;
use wrela::collision_contract::{
    COLLISION_POINT_OCCUPANCY_WORLD, CollisionOccupancyClass, CollisionResult, collision_contracts,
};
use wrela::collision_plan::{
    CollisionExecError, CollisionPassKind, CollisionPlan, CollisionQueryKind,
    collision_plans_with_backend,
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

fn expect_f32(value: &KernelValue) -> f32 {
    match value {
        KernelValue::F32(value) => *value,
        other => panic!("expected F32, got {other:?}"),
    }
}

fn assert_approx_eq(lhs: f32, rhs: f32) {
    assert!(
        (lhs - rhs).abs() < 0.01,
        "expected {lhs} ~= {rhs}, delta={}",
        (lhs - rhs).abs()
    );
}

#[test]
fn collision_contract_registry_exposes_typed_static_world_surface() {
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
        ]
    );
    assert!(collision_contracts().iter().all(|descriptor| {
        descriptor.input_record.starts_with("Collision")
            && descriptor.output_record.starts_with("Collision")
            && descriptor.supported_backends.cpu
            && !descriptor.supported_backends.virtual_gpu
            && !descriptor.supported_backends.wgsl
    }));
}

#[test]
fn static_collision_plans_validate_dependencies_and_witness_declarations() {
    for plan in collision_plans_with_backend(DispatchBackend::Auto) {
        assert!(
            plan.validate().is_empty(),
            "expected '{}' to validate cleanly: {:?}",
            plan.name,
            plan.validate()
        );
        assert_eq!(plan.outputs.len(), 1);
        assert!(
            plan.outputs[0].witness_schema.is_some(),
            "each collision output should declare a witness schema"
        );
        assert!(
            !plan.artifact_uses().is_empty(),
            "collision plans should expose explicit artifact use records"
        );
    }
}

#[test]
fn collision_validation_reports_required_guarantee_and_selected_method_class() {
    let plan = CollisionPlan::for_query_with_backend(
        CollisionQueryKind::PointOccupancyWorld,
        DispatchBackend::Wgsl,
    );
    let errors = plan.validate();
    assert!(!errors.is_empty());
    assert!(errors.iter().any(|err| {
        err.message.contains("required_guarantee=exact")
            && err.message.contains("selected_method=exact_oracle")
    }));
}

#[test]
fn point_ray_and_overlap_outputs_remain_cpu_oracle_checkable() {
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
    assert_eq!(overlap_trace.artifact_store.entries, 1);
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
fn renamed_collision_plan_artifacts_and_outputs_still_execute_via_explicit_plan_records() {
    let ctx = typed_query_module(collision_fixture_source());
    let region_capture = KernelValue::Capture(SmolStr::new("collision_region"));
    let region_scene_id = stable_region_scene_capture_id(&SmolStr::new("collision_region"));
    let domain = scene_domain(region_scene_id);
    let mut plan = CollisionPlan::for_query(CollisionQueryKind::PointOccupancyWorld);

    plan.artifacts[0].id = SmolStr::new("artifact.support_summary.plan_renamed");
    if let CollisionPassKind::GatherCandidates { artifact_id, .. } = &mut plan.passes[0].kind {
        *artifact_id = SmolStr::new("artifact.support_summary.plan_renamed");
    } else {
        panic!("expected gather pass");
    }
    plan.passes[0].materializes = vec![SmolStr::new("artifact.support_summary.plan_renamed")];
    if let CollisionPassKind::EvaluatePointOccupancy {
        support_artifact, ..
    } = &mut plan.passes[1].kind
    {
        *support_artifact = SmolStr::new("artifact.support_summary.plan_renamed");
    } else {
        panic!("expected point occupancy pass");
    }
    plan.passes[1].consumes = vec![SmolStr::new("artifact.support_summary.plan_renamed")];
    plan.passes[1].materializes = vec![SmolStr::new("occupancy.stage.plan_renamed")];
    plan.outputs[0].name = SmolStr::new("occupancy.output.plan_renamed");
    plan.passes[2].consumes = vec![SmolStr::new("occupancy.stage.plan_renamed")];
    plan.passes[2].materializes = vec![SmolStr::new("occupancy.output.plan_renamed")];

    assert!(
        plan.validate().is_empty(),
        "renamed plan should still validate: {:?}",
        plan.validate()
    );

    let (result, trace) = plan
        .execute(
            &ctx,
            &[
                region_capture,
                domain,
                collision_point_input([0.0, 0.0, 0.25]),
            ],
        )
        .expect("renamed point occupancy");
    assert_eq!(trace.artifact_store.entries, 1);
    match result {
        CollisionResult::Occupancy(value) => {
            assert_eq!(value.classification, CollisionOccupancyClass::Occupied);
            assert_approx_eq(value.signed_distance, -0.25);
            assert_approx_eq(value.witness.nearest_point_on_world[2], 0.5);
        }
        other => panic!("expected occupancy result, got {other:?}"),
    }
}

#[test]
fn collision_validation_rejects_artifact_output_and_shape_drift() {
    let mut overlap_plan = CollisionPlan::for_query(CollisionQueryKind::SphereOverlapWorld);
    if let CollisionPassKind::GatherCandidates { artifact_id, .. } =
        &mut overlap_plan.passes[0].kind
    {
        *artifact_id = SmolStr::new("artifact.missing_support_summary");
    } else {
        panic!("expected gather pass");
    }
    overlap_plan.passes[0].materializes = vec![SmolStr::new("artifact.missing_support_summary")];
    if let CollisionPassKind::ResolveSphereOverlap {
        supported_shape, ..
    } = &mut overlap_plan.passes[1].kind
    {
        *supported_shape = SmolStr::new("capsule");
    } else {
        panic!("expected overlap pass");
    }
    overlap_plan.passes[2].consumes = vec![SmolStr::new("artifact.support_summary")];
    overlap_plan.passes[2].materializes = vec![SmolStr::new("sphere_overlap.output_drift")];

    let errors = overlap_plan.validate();
    assert!(errors.iter().any(|error| {
        error
            .message
            .contains("materializes undeclared artifact 'artifact.missing_support_summary'")
    }));
    assert!(errors.iter().any(|error| {
        error
            .message
            .contains("references unsupported shape 'capsule'")
    }));
    assert!(errors.iter().any(|error| {
        error.message.contains(
            "must consume a materialized collision intermediate, found 'artifact.support_summary'",
        )
    }));
    assert!(errors.iter().any(|error| {
        error
            .message
            .contains("must materialize collision output 'sphere_overlap'")
    }));
}

#[test]
fn collision_execution_reports_missing_input_fields_as_typed_errors() {
    let ctx = typed_query_module(collision_fixture_source());
    let region_capture = KernelValue::Capture(SmolStr::new("collision_region"));
    let region_scene_id = stable_region_scene_capture_id(&SmolStr::new("collision_region"));
    let domain = scene_domain(region_scene_id);
    let plan = CollisionPlan::for_query(CollisionQueryKind::PointOccupancyWorld);

    let error = plan
        .execute(
            &ctx,
            &[
                region_capture,
                domain,
                KernelValue::Struct(KernelStructValue {
                    name: SmolStr::new("CollisionPointInput"),
                    fields: Vec::new(),
                }),
            ],
        )
        .expect_err("missing point field should be reported");
    assert!(matches!(
        error,
        CollisionExecError::MissingField { ref record, ref field }
            if record == "CollisionPointInput" && field == "point"
    ));
}
