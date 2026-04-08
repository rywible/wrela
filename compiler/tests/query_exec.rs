use smol_str::SmolStr;
use wrela::hir;
use wrela::hir::lower as hir_lower;
use wrela::kernel::{
    KernelStructValue, KernelValue, execute_entry, execute_entry_on, lower_batch_query_plan,
    lower_capture_query_plan, lower_kernel_entry_by_name, lower_world_query_plan,
};
use wrela::parser::ast;
use wrela::parser::ast::AstNode;
use wrela::parser::parse;
use wrela::query_exec::{
    DirectQueryExecutor, QueryExecContext, executable_region_shape_lists,
    execute_batch_query_with_trace, execute_capture_query, execute_capture_query_with_trace_on,
    execute_world_query, execute_world_query_with_trace_on, stable_field_scene_capture_id,
    stable_region_scene_capture_id, stable_shape_capture_id, stable_shape_scene_capture_id,
};
use wrela::query_plan::{
    BatchQueryKind, BatchQueryPlan, CaptureKind, CaptureQueryKind, CaptureQueryPlan,
    DispatchBackend, WorldQueryKind, WorldQueryPlan,
};

fn lower_inline_module_from_source(source: &str) -> hir::Module {
    let node = parse(source);
    let root = ast::Root::cast(node).expect("root");
    hir_lower::lower(root)
}

fn typed_query_module(source: &str) -> (hir::Module, hir::TypeInfo, QueryExecContext) {
    let module = lower_inline_module_from_source(source);
    let semantic = hir::semantic::check_module(&module);
    assert!(
        semantic.errors.is_empty(),
        "semantic errors: {:?}",
        semantic.errors
    );
    let (type_errors, type_info) = hir::typeck::check_module_with_info(&module);
    assert!(type_errors.is_empty(), "type errors: {type_errors:?}");
    let ctx = QueryExecContext::compile(&module, &type_info);
    (module, type_info, ctx)
}

fn query_fixture_source() -> &'static str {
    r#"
field exact distance sphere_field(p: Vec3) -> F32 {
    sphere(radius = 1.0)
}

material shade(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(0.25, 0.35, 0.45),
        roughness=0.5,
        metalness=0.1,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

radiance field glow(p: Vec3, direction: Vec3, feature_id: U32) -> Vec3 {
    return vec3(0.1, 0.2, 0.3) + direction * 0.0 + vec3(f32(feature_id) * 0.0, 0.0, 0.0)
}

volume field fog(p: Vec3, surface_distance: F32) -> Medium {
    return Medium(
        density=0.2,
        emission=vec3(0.05, 0.06, 0.07) + vec3(abs(surface_distance) * 0.0, 0.0, 0.0),
        anisotropy=0.1
    )
}

shape scene_shape {
    field = sphere_field
    material = shade
    radiance = glow
    volume = fog
    payload = Payload(
        entity_id=u32(11),
        material_id=u32(22),
        actor=ActorHandle(id=u32(33), generation=u32(0))
    )
}

shape coarse_shape {
    field = sphere_field
    material = shade
    payload = Payload()
}

shape fine_shape {
    field = sphere_field
    material = shade
    payload = Payload()
}

region layered_region() {
    place coarse = coarse_shape
    place fine = fine_shape
}

region scene_region() {
    place scene = scene_shape
}

domain fine_domain(world: RegionCapture) {
    geometry_detail = 1
    material = true
    radiance = true
    media = true
    max_distance = 6.0
    min_step = 0.05
    hit_epsilon = 0.001
    max_steps = 96
}

value QuerySummary {
    distance: F32
    world_distance: F32
    batch_distance0: F32
    batch_distance1: F32
    occluded0: Boolean
    hit: Hit3
    world_hit: Hit3
    surface: Surface
}

kernel fn portable_entry() -> QuerySummary {
    scene = capture scene_shape
    world = capture scene_region
    domain = fine_domain(world=world)
    hit = trace_shape(
        capture=scene,
        origin=vec3(0.0, 0.0, 3.0),
        direction=vec3(0.0, 0.0, -1.0),
        max_distance=6.0,
        min_step=0.05,
        hit_epsilon=0.001,
        max_steps=96
    )
    world_hit = trace_world(
        capture=world,
        domain=domain,
        origin=vec3(0.0, 0.0, 3.0),
        direction=vec3(0.0, 0.0, -1.0),
        max_distance=6.0,
        min_step=0.05,
        hit_epsilon=0.001,
        max_steps=96
    )
    points = [
        PointQuery(point=vec3(0.0, 0.0, 2.0)),
        PointQuery(point=vec3(0.0, 0.0, 3.0))
    ]
    rays = [
        RayQuery(
            origin=vec3(0.0, 0.0, 3.0),
            direction=vec3(0.0, 0.0, -1.0),
            max_distance=6.0,
            min_step=0.05,
            hit_epsilon=0.001,
            max_steps=96
        ),
        RayQuery(
            origin=vec3(0.0, 0.0, 3.0),
            direction=vec3(0.0, 1.0, 0.0),
            max_distance=6.0,
            min_step=0.05,
            hit_epsilon=0.001,
            max_steps=96
        )
    ]
    distances = distance_at_batch(
        capture=scene,
        points=points,
        backend=dispatch_backend_virtual_gpu()
    )
    occlusions = occluded_batch(
        capture=scene,
        rays=rays,
        backend=dispatch_backend_virtual_gpu()
    )
    return QuerySummary(
        distance=distance_at(capture=scene, point=vec3(0.0, 0.0, 2.0)),
        world_distance=distance_world(capture=world, domain=domain, point=vec3(0.0, 0.0, 2.0)),
        batch_distance0=distances[0].distance,
        batch_distance1=distances[1].distance,
        occluded0=occlusions[0].occluded,
        hit=hit,
        world_hit=world_hit,
        surface=surface_at(capture=scene, hit=hit)
    )
}
"#
}

fn scene_domain(
    scene_id: u32,
    detail: i32,
    material: bool,
    radiance: bool,
    media: bool,
) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("SceneDomain"),
        fields: vec![
            (SmolStr::new("scene_id"), KernelValue::U32(scene_id)),
            (
                SmolStr::new("world"),
                KernelValue::Struct(KernelStructValue {
                    name: SmolStr::new("RegionCapture"),
                    fields: vec![
                        (SmolStr::new("scene_id"), KernelValue::U32(scene_id)),
                        (SmolStr::new("epoch"), KernelValue::U32(0)),
                        (SmolStr::new("root_feature_id"), KernelValue::U32(0)),
                    ],
                }),
            ),
            (SmolStr::new("geometry_detail"), KernelValue::I32(detail)),
            (SmolStr::new("material"), KernelValue::Bool(material)),
            (SmolStr::new("radiance"), KernelValue::Bool(radiance)),
            (SmolStr::new("media"), KernelValue::Bool(media)),
            (SmolStr::new("max_distance"), KernelValue::F32(6.0)),
            (SmolStr::new("min_step"), KernelValue::F32(0.05)),
            (SmolStr::new("hit_epsilon"), KernelValue::F32(0.001)),
            (SmolStr::new("max_steps"), KernelValue::I32(96)),
        ],
    })
}

fn point_query(point: [f32; 3]) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("PointQuery"),
        fields: vec![(SmolStr::new("point"), KernelValue::Vec3(point))],
    })
}

fn ray_query(origin: [f32; 3], direction: [f32; 3]) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("RayQuery"),
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

fn expect_struct<'a>(value: &'a KernelValue, name: &str) -> &'a KernelStructValue {
    match value {
        KernelValue::Struct(value) if value.name.as_str() == name => value,
        other => panic!("expected {name}, got {other:?}"),
    }
}

fn field<'a>(value: &'a KernelStructValue, name: &str) -> &'a KernelValue {
    value
        .fields
        .iter()
        .find(|(field_name, _)| field_name.as_str() == name)
        .map(|(_, value)| value)
        .unwrap_or_else(|| panic!("missing field {name} on {}", value.name))
}

fn expect_f32(value: &KernelValue) -> f32 {
    match value {
        KernelValue::F32(value) => *value,
        other => panic!("expected F32, got {other:?}"),
    }
}

fn expect_bool(value: &KernelValue) -> bool {
    match value {
        KernelValue::Bool(value) => *value,
        other => panic!("expected Bool, got {other:?}"),
    }
}

fn expect_vec3(value: &KernelValue) -> [f32; 3] {
    match value {
        KernelValue::Vec3(value) => *value,
        other => panic!("expected Vec3, got {other:?}"),
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
fn query_exec_ids_are_stable_and_region_detail_filtering_is_shared() {
    let (module, _, _) = typed_query_module(query_fixture_source());
    let layered_region = module
        .functions
        .iter()
        .find_map(|(_, function)| (function.name == "layered_region").then_some(function))
        .expect("layered region");

    let (coarse, fine) = executable_region_shape_lists(layered_region).expect("region shapes");
    assert_eq!(coarse, vec![SmolStr::new("coarse_shape")]);
    assert_eq!(
        fine,
        vec![SmolStr::new("coarse_shape"), SmolStr::new("fine_shape")]
    );

    let field_id = stable_field_scene_capture_id(&SmolStr::new("sphere_field"));
    let shape_capture_id = stable_shape_capture_id(&SmolStr::new("scene_shape"));
    let shape_scene_id = stable_shape_scene_capture_id(&SmolStr::new("scene_shape"));
    let region_id = stable_region_scene_capture_id(&SmolStr::new("scene_region"));

    assert_eq!(
        field_id,
        stable_field_scene_capture_id(&SmolStr::new("sphere_field"))
    );
    assert_eq!(
        shape_capture_id,
        stable_shape_capture_id(&SmolStr::new("scene_shape"))
    );
    assert_ne!(field_id, 0);
    assert_ne!(shape_capture_id, 0);
    assert_ne!(shape_scene_id, 0);
    assert_ne!(region_id, 0);
}

#[test]
fn query_exec_cpu_runs_capture_world_and_batch_queries_with_shared_results() {
    let (_, _, ctx) = typed_query_module(query_fixture_source());

    let capture_distance = lower_capture_query_plan(
        &CaptureQueryPlan::for_query(CaptureQueryKind::Distance, CaptureKind::Shape, None)
            .expect("capture distance plan"),
    );
    let capture_normal = lower_capture_query_plan(
        &CaptureQueryPlan::for_query(CaptureQueryKind::Normal, CaptureKind::Shape, None)
            .expect("capture normal plan"),
    );
    let capture_trace = lower_capture_query_plan(
        &CaptureQueryPlan::for_query(CaptureQueryKind::Trace, CaptureKind::Shape, None)
            .expect("capture trace plan"),
    );
    let capture_surface = lower_capture_query_plan(
        &CaptureQueryPlan::for_query(CaptureQueryKind::Surface, CaptureKind::Shape, None)
            .expect("capture surface plan"),
    );
    let capture_radiance = lower_capture_query_plan(
        &CaptureQueryPlan::for_query(CaptureQueryKind::Radiance, CaptureKind::Shape, None)
            .expect("capture radiance plan"),
    );
    let capture_medium = lower_capture_query_plan(
        &CaptureQueryPlan::for_query(CaptureQueryKind::Medium, CaptureKind::Shape, None)
            .expect("capture medium plan"),
    );

    let shape_capture = KernelValue::Capture(SmolStr::new("scene_shape"));
    let region_capture = KernelValue::Capture(SmolStr::new("scene_region"));
    let region_scene_id = stable_region_scene_capture_id(&SmolStr::new("scene_region"));
    let fine_domain = scene_domain(region_scene_id, 1, true, true, true);

    let distance = execute_capture_query(
        &ctx,
        &capture_distance,
        &[shape_capture.clone(), KernelValue::Vec3([0.0, 0.0, 2.0])],
    )
    .expect("capture distance");
    assert_approx_eq(expect_f32(&distance), 1.0);

    let normal = execute_capture_query(
        &ctx,
        &capture_normal,
        &[shape_capture.clone(), KernelValue::Vec3([0.0, 0.0, 2.0])],
    )
    .expect("capture normal");
    assert_eq!(expect_vec3(&normal), [0.0, 0.0, 1.0]);

    let hit = execute_capture_query(
        &ctx,
        &capture_trace,
        &[
            shape_capture.clone(),
            KernelValue::Vec3([0.0, 0.0, 3.0]),
            KernelValue::Vec3([0.0, 0.0, -1.0]),
            KernelValue::F32(6.0),
            KernelValue::F32(0.05),
            KernelValue::F32(0.001),
            KernelValue::I32(96),
        ],
    )
    .expect("capture trace");
    let hit_struct = expect_struct(&hit, "Hit3");
    assert!(expect_bool(field(hit_struct, "hit")));
    assert_approx_eq(expect_f32(field(hit_struct, "distance")), 2.0);
    assert_eq!(expect_vec3(field(hit_struct, "position")), [0.0, 0.0, 1.0]);
    assert_eq!(
        field(hit_struct, "root_shape_id"),
        &KernelValue::U32(stable_shape_capture_id(&SmolStr::new("scene_shape")))
    );

    let surface = execute_capture_query(
        &ctx,
        &capture_surface,
        &[shape_capture.clone(), hit.clone()],
    )
    .expect("capture surface");
    let surface_struct = expect_struct(&surface, "Surface");
    assert_eq!(
        expect_vec3(field(surface_struct, "albedo")),
        [0.25, 0.35, 0.45]
    );

    let radiance = execute_capture_query(
        &ctx,
        &capture_radiance,
        &[
            shape_capture.clone(),
            KernelValue::Vec3([0.0, 0.0, 1.0]),
            KernelValue::Vec3([0.0, 1.0, 0.0]),
        ],
    )
    .expect("capture radiance");
    assert_eq!(expect_vec3(&radiance), [0.1, 0.2, 0.3]);

    let medium = execute_capture_query(
        &ctx,
        &capture_medium,
        &[shape_capture.clone(), KernelValue::Vec3([0.0, 0.0, 1.0])],
    )
    .expect("capture medium");
    let medium_struct = expect_struct(&medium, "Medium");
    assert_approx_eq(expect_f32(field(medium_struct, "density")), 0.2);

    let world_distance = execute_world_query(
        &ctx,
        &lower_world_query_plan(&WorldQueryPlan::for_query(WorldQueryKind::Distance)),
        &[
            region_capture.clone(),
            fine_domain.clone(),
            KernelValue::Vec3([0.0, 0.0, 2.0]),
        ],
    )
    .expect("world distance");
    assert_approx_eq(expect_f32(&world_distance), 1.0);

    let world_normal = execute_world_query(
        &ctx,
        &lower_world_query_plan(&WorldQueryPlan::for_query(WorldQueryKind::Normal)),
        &[
            region_capture.clone(),
            fine_domain.clone(),
            KernelValue::Vec3([0.0, 0.0, 2.0]),
        ],
    )
    .expect("world normal");
    assert_eq!(expect_vec3(&world_normal), [0.0, 0.0, 1.0]);

    let world_hit = execute_world_query(
        &ctx,
        &lower_world_query_plan(&WorldQueryPlan::for_query(WorldQueryKind::Trace)),
        &[
            region_capture.clone(),
            fine_domain.clone(),
            KernelValue::Vec3([0.0, 0.0, 3.0]),
            KernelValue::Vec3([0.0, 0.0, -1.0]),
            KernelValue::F32(6.0),
            KernelValue::F32(0.05),
            KernelValue::F32(0.001),
            KernelValue::I32(96),
        ],
    )
    .expect("world trace");
    let world_hit_struct = expect_struct(&world_hit, "Hit3");
    assert!(expect_bool(field(world_hit_struct, "hit")));
    assert_approx_eq(expect_f32(field(world_hit_struct, "distance")), 2.0);

    let cpu_batch_plan = lower_batch_query_plan(&BatchQueryPlan::for_field_query(
        BatchQueryKind::Distance,
        CaptureKind::Shape,
        DispatchBackend::Cpu,
        None,
    ));
    let vgpu_batch_plan = lower_batch_query_plan(&BatchQueryPlan::for_field_query(
        BatchQueryKind::Distance,
        CaptureKind::Shape,
        DispatchBackend::VirtualGpu,
        None,
    ));
    let points = KernelValue::Array(vec![
        point_query([0.0, 0.0, 2.0]),
        point_query([0.0, 0.0, 3.0]),
    ]);

    let (cpu_distances, cpu_trace) = execute_batch_query_with_trace(
        &ctx,
        &cpu_batch_plan,
        &[shape_capture.clone(), points.clone()],
    )
    .expect("cpu distances");
    let (vgpu_distances, vgpu_trace) =
        execute_batch_query_with_trace(&ctx, &vgpu_batch_plan, &[shape_capture.clone(), points])
            .expect("vgpu distances");
    assert_eq!(cpu_distances, vgpu_distances);
    assert_eq!(cpu_trace.backend, DispatchBackend::Cpu);
    assert_eq!(vgpu_trace.backend, DispatchBackend::VirtualGpu);
    assert!(!cpu_trace.plan_trace.begins_virtual_gpu_dispatch);
    assert!(vgpu_trace.plan_trace.begins_virtual_gpu_dispatch);

    let (trace_cpu, trace_cpu_backend) = execute_batch_query_with_trace(
        &ctx,
        &lower_batch_query_plan(&BatchQueryPlan::for_shape_query(
            BatchQueryKind::Trace,
            DispatchBackend::Cpu,
            None,
        )),
        &[
            shape_capture.clone(),
            KernelValue::Array(vec![
                ray_query([0.0, 0.0, 3.0], [0.0, 0.0, -1.0]),
                ray_query([0.0, 0.0, 3.0], [0.0, 1.0, 0.0]),
            ]),
        ],
    )
    .expect("cpu trace batch");
    let (trace_vgpu, trace_vgpu_backend) = execute_batch_query_with_trace(
        &ctx,
        &lower_batch_query_plan(&BatchQueryPlan::for_shape_query(
            BatchQueryKind::Trace,
            DispatchBackend::VirtualGpu,
            None,
        )),
        &[
            shape_capture.clone(),
            KernelValue::Array(vec![
                ray_query([0.0, 0.0, 3.0], [0.0, 0.0, -1.0]),
                ray_query([0.0, 0.0, 3.0], [0.0, 1.0, 0.0]),
            ]),
        ],
    )
    .expect("vgpu trace batch");
    assert_eq!(trace_cpu, trace_vgpu);
    assert_eq!(trace_cpu_backend.backend, DispatchBackend::Cpu);
    assert_eq!(trace_vgpu_backend.backend, DispatchBackend::VirtualGpu);
    assert!(trace_vgpu_backend.plan_trace.ends_virtual_gpu_dispatch);
}

#[test]
fn query_exec_world_queries_enforce_domain_contract_and_flags() {
    let (_, _, ctx) = typed_query_module(query_fixture_source());

    let region_capture = KernelValue::Capture(SmolStr::new("scene_region"));
    let region_scene_id = stable_region_scene_capture_id(&SmolStr::new("scene_region"));
    let mismatched_scene_id = stable_region_scene_capture_id(&SmolStr::new("layered_region"));

    let mismatch = execute_world_query(
        &ctx,
        &lower_world_query_plan(&WorldQueryPlan::for_query(WorldQueryKind::Distance)),
        &[
            region_capture.clone(),
            scene_domain(mismatched_scene_id, 1, true, true, true),
            KernelValue::Vec3([0.0, 0.0, 2.0]),
        ],
    )
    .expect_err("mismatched world domain should fail");
    assert!(
        mismatch
            .to_string()
            .contains("requires a domain derived from the same region capture")
    );

    let valid_domain = scene_domain(region_scene_id, 1, true, true, true);
    let hit = execute_world_query(
        &ctx,
        &lower_world_query_plan(&WorldQueryPlan::for_query(WorldQueryKind::Trace)),
        &[
            region_capture.clone(),
            valid_domain.clone(),
            KernelValue::Vec3([0.0, 0.0, 3.0]),
            KernelValue::Vec3([0.0, 0.0, -1.0]),
            KernelValue::F32(6.0),
            KernelValue::F32(0.05),
            KernelValue::F32(0.001),
            KernelValue::I32(96),
        ],
    )
    .expect("world trace");

    let surface_disabled = execute_world_query(
        &ctx,
        &lower_world_query_plan(&WorldQueryPlan::for_query(WorldQueryKind::Surface)),
        &[
            region_capture.clone(),
            scene_domain(region_scene_id, 1, false, true, true),
            hit.clone(),
        ],
    )
    .expect("surface_world with material disabled");
    assert_eq!(
        expect_vec3(field(expect_struct(&surface_disabled, "Surface"), "albedo")),
        [0.0, 0.0, 0.0]
    );

    let radiance_disabled = execute_world_query(
        &ctx,
        &lower_world_query_plan(&WorldQueryPlan::for_query(WorldQueryKind::Radiance)),
        &[
            region_capture.clone(),
            scene_domain(region_scene_id, 1, true, false, true),
            KernelValue::Vec3([0.0, 0.0, 1.0]),
            KernelValue::Vec3([0.0, 1.0, 0.0]),
        ],
    )
    .expect("radiance_world with radiance disabled");
    assert_eq!(expect_vec3(&radiance_disabled), [0.0, 0.0, 0.0]);

    let medium_disabled = execute_world_query(
        &ctx,
        &lower_world_query_plan(&WorldQueryPlan::for_query(WorldQueryKind::Medium)),
        &[
            region_capture,
            scene_domain(region_scene_id, 1, true, true, false),
            KernelValue::Vec3([0.0, 0.0, 1.0]),
        ],
    )
    .expect("medium_world with media disabled");
    assert_approx_eq(
        expect_f32(field(expect_struct(&medium_disabled, "Medium"), "density")),
        0.0,
    );
}

#[test]
fn query_exec_explicit_virtual_gpu_backend_matches_cpu_for_direct_queries() {
    let (_, _, ctx) = typed_query_module(query_fixture_source());
    let shape_capture = KernelValue::Capture(SmolStr::new("scene_shape"));
    let field_capture = KernelValue::Capture(SmolStr::new("sphere_field"));
    let region_capture = KernelValue::Capture(SmolStr::new("scene_region"));
    let region_scene_id = stable_region_scene_capture_id(&SmolStr::new("scene_region"));
    let fine_domain = scene_domain(region_scene_id, 1, true, true, true);

    let field_capture_plan = lower_capture_query_plan(
        &CaptureQueryPlan::for_query(CaptureQueryKind::Distance, CaptureKind::Field, None)
            .expect("field capture distance plan"),
    );
    let cpu_field_capture = execute_capture_query(
        &ctx,
        &field_capture_plan,
        &[field_capture.clone(), KernelValue::Vec3([0.0, 0.0, 2.0])],
    )
    .expect("cpu field capture distance");
    let (vgpu_field_capture, vgpu_field_capture_trace) = execute_capture_query_with_trace_on(
        &ctx,
        DispatchBackend::VirtualGpu,
        &field_capture_plan,
        &[field_capture.clone(), KernelValue::Vec3([0.0, 0.0, 2.0])],
    )
    .expect("vgpu field capture distance");
    assert_eq!(cpu_field_capture, vgpu_field_capture);
    assert_eq!(vgpu_field_capture_trace.backend, DispatchBackend::VirtualGpu);
    assert_eq!(
        vgpu_field_capture_trace.executor,
        DirectQueryExecutor::VirtualGpu
    );

    let field_normal_plan = lower_capture_query_plan(
        &CaptureQueryPlan::for_query(CaptureQueryKind::Normal, CaptureKind::Field, None)
            .expect("field capture normal plan"),
    );
    let cpu_field_normal = execute_capture_query(
        &ctx,
        &field_normal_plan,
        &[field_capture, KernelValue::Vec3([0.0, 0.0, 2.0])],
    )
    .expect("cpu field capture normal");
    let (vgpu_field_normal, vgpu_field_normal_trace) = execute_capture_query_with_trace_on(
        &ctx,
        DispatchBackend::VirtualGpu,
        &field_normal_plan,
        &[
            KernelValue::Capture(SmolStr::new("sphere_field")),
            KernelValue::Vec3([0.0, 0.0, 2.0]),
        ],
    )
    .expect("vgpu field capture normal");
    assert_eq!(cpu_field_normal, vgpu_field_normal);
    assert_eq!(vgpu_field_normal_trace.backend, DispatchBackend::VirtualGpu);
    assert_eq!(
        vgpu_field_normal_trace.executor,
        DirectQueryExecutor::VirtualGpu
    );

    let capture_plan = lower_capture_query_plan(
        &CaptureQueryPlan::for_query(CaptureQueryKind::Distance, CaptureKind::Shape, None)
            .expect("capture distance plan"),
    );
    let cpu_capture = execute_capture_query(
        &ctx,
        &capture_plan,
        &[shape_capture.clone(), KernelValue::Vec3([0.0, 0.0, 2.0])],
    )
    .expect("cpu capture distance");
    let (vgpu_capture, vgpu_capture_trace) = execute_capture_query_with_trace_on(
        &ctx,
        DispatchBackend::VirtualGpu,
        &capture_plan,
        &[shape_capture, KernelValue::Vec3([0.0, 0.0, 2.0])],
    )
    .expect("vgpu capture distance");
    assert_eq!(cpu_capture, vgpu_capture);
    assert_eq!(vgpu_capture_trace.backend, DispatchBackend::VirtualGpu);
    assert_eq!(vgpu_capture_trace.executor, DirectQueryExecutor::VirtualGpu);

    let capture_normal_plan = lower_capture_query_plan(
        &CaptureQueryPlan::for_query(CaptureQueryKind::Normal, CaptureKind::Shape, None)
            .expect("capture normal plan"),
    );
    let cpu_capture_normal = execute_capture_query(
        &ctx,
        &capture_normal_plan,
        &[
            KernelValue::Capture(SmolStr::new("scene_shape")),
            KernelValue::Vec3([0.0, 0.0, 2.0]),
        ],
    )
    .expect("cpu capture normal");
    let (vgpu_capture_normal, vgpu_capture_normal_trace) = execute_capture_query_with_trace_on(
        &ctx,
        DispatchBackend::VirtualGpu,
        &capture_normal_plan,
        &[
            KernelValue::Capture(SmolStr::new("scene_shape")),
            KernelValue::Vec3([0.0, 0.0, 2.0]),
        ],
    )
    .expect("vgpu capture normal");
    assert_eq!(cpu_capture_normal, vgpu_capture_normal);
    assert_eq!(vgpu_capture_normal_trace.backend, DispatchBackend::VirtualGpu);
    assert_eq!(
        vgpu_capture_normal_trace.executor,
        DirectQueryExecutor::VirtualGpu
    );

    let world_plan = lower_world_query_plan(&WorldQueryPlan::for_query(WorldQueryKind::Distance));
    let cpu_world = execute_world_query(
        &ctx,
        &world_plan,
        &[
            region_capture.clone(),
            fine_domain.clone(),
            KernelValue::Vec3([0.0, 0.0, 2.0]),
        ],
    )
    .expect("cpu world distance");
    let (vgpu_world, vgpu_world_trace) = execute_world_query_with_trace_on(
        &ctx,
        DispatchBackend::VirtualGpu,
        &world_plan,
        &[
            region_capture,
            fine_domain,
            KernelValue::Vec3([0.0, 0.0, 2.0]),
        ],
    )
    .expect("vgpu world distance");
    assert_eq!(cpu_world, vgpu_world);
    assert_eq!(vgpu_world_trace.backend, DispatchBackend::VirtualGpu);
    assert_eq!(vgpu_world_trace.executor, DirectQueryExecutor::VirtualGpu);

    let world_normal_plan = lower_world_query_plan(&WorldQueryPlan::for_query(WorldQueryKind::Normal));
    let cpu_world_normal = execute_world_query(
        &ctx,
        &world_normal_plan,
        &[
            KernelValue::Capture(SmolStr::new("scene_region")),
            scene_domain(region_scene_id, 1, true, true, true),
            KernelValue::Vec3([0.0, 0.0, 2.0]),
        ],
    )
    .expect("cpu world normal");
    let (vgpu_world_normal, vgpu_world_normal_trace) = execute_world_query_with_trace_on(
        &ctx,
        DispatchBackend::VirtualGpu,
        &world_normal_plan,
        &[
            KernelValue::Capture(SmolStr::new("scene_region")),
            scene_domain(region_scene_id, 1, true, true, true),
            KernelValue::Vec3([0.0, 0.0, 2.0]),
        ],
    )
    .expect("vgpu world normal");
    assert_eq!(cpu_world_normal, vgpu_world_normal);
    assert_eq!(vgpu_world_normal_trace.backend, DispatchBackend::VirtualGpu);
    assert_eq!(
        vgpu_world_normal_trace.executor,
        DirectQueryExecutor::VirtualGpu
    );

    let capture_trace_plan = lower_capture_query_plan(
        &CaptureQueryPlan::for_query(CaptureQueryKind::Trace, CaptureKind::Shape, None)
            .expect("capture trace plan"),
    );
    let capture_trace_args = vec![
        KernelValue::Capture(SmolStr::new("scene_shape")),
        KernelValue::Vec3([0.0, 0.0, 3.0]),
        KernelValue::Vec3([0.0, 0.0, -1.0]),
        KernelValue::F32(6.0),
        KernelValue::F32(0.05),
        KernelValue::F32(0.001),
        KernelValue::I32(96),
    ];
    let cpu_capture_trace =
        execute_capture_query(&ctx, &capture_trace_plan, &capture_trace_args).expect("cpu trace");
    let (vgpu_capture_trace, vgpu_capture_trace_trace) = execute_capture_query_with_trace_on(
        &ctx,
        DispatchBackend::VirtualGpu,
        &capture_trace_plan,
        &capture_trace_args,
    )
    .expect("vgpu trace");
    assert_eq!(cpu_capture_trace, vgpu_capture_trace);
    assert_eq!(vgpu_capture_trace_trace.backend, DispatchBackend::VirtualGpu);
    assert_eq!(
        vgpu_capture_trace_trace.executor,
        DirectQueryExecutor::VirtualGpu
    );

    let capture_surface_plan = lower_capture_query_plan(
        &CaptureQueryPlan::for_query(CaptureQueryKind::Surface, CaptureKind::Shape, None)
            .expect("capture surface plan"),
    );
    let capture_surface_args = vec![
        KernelValue::Capture(SmolStr::new("scene_shape")),
        cpu_capture_trace.clone(),
    ];
    let cpu_capture_surface = execute_capture_query(&ctx, &capture_surface_plan, &capture_surface_args)
        .expect("cpu capture surface");
    let (vgpu_capture_surface, vgpu_capture_surface_trace) = execute_capture_query_with_trace_on(
        &ctx,
        DispatchBackend::VirtualGpu,
        &capture_surface_plan,
        &capture_surface_args,
    )
    .expect("vgpu capture surface");
    assert_eq!(cpu_capture_surface, vgpu_capture_surface);
    assert_eq!(vgpu_capture_surface_trace.backend, DispatchBackend::VirtualGpu);
    assert_eq!(
        vgpu_capture_surface_trace.executor,
        DirectQueryExecutor::VirtualGpu
    );

    let capture_radiance_plan = lower_capture_query_plan(
        &CaptureQueryPlan::for_query(CaptureQueryKind::Radiance, CaptureKind::Shape, None)
            .expect("capture radiance plan"),
    );
    let capture_radiance_args = vec![
        KernelValue::Capture(SmolStr::new("scene_shape")),
        KernelValue::Vec3([0.0, 0.0, 1.0]),
        KernelValue::Vec3([0.0, 0.0, -1.0]),
    ];
    let cpu_capture_radiance =
        execute_capture_query(&ctx, &capture_radiance_plan, &capture_radiance_args)
            .expect("cpu capture radiance");
    let (vgpu_capture_radiance, vgpu_capture_radiance_trace) =
        execute_capture_query_with_trace_on(
            &ctx,
            DispatchBackend::VirtualGpu,
            &capture_radiance_plan,
            &capture_radiance_args,
        )
        .expect("vgpu capture radiance");
    assert_eq!(cpu_capture_radiance, vgpu_capture_radiance);
    assert_eq!(
        vgpu_capture_radiance_trace.backend,
        DispatchBackend::VirtualGpu
    );
    assert_eq!(
        vgpu_capture_radiance_trace.executor,
        DirectQueryExecutor::VirtualGpu
    );

    let capture_medium_plan = lower_capture_query_plan(
        &CaptureQueryPlan::for_query(CaptureQueryKind::Medium, CaptureKind::Shape, None)
            .expect("capture medium plan"),
    );
    let capture_medium_args = vec![
        KernelValue::Capture(SmolStr::new("scene_shape")),
        KernelValue::Vec3([0.0, 0.0, 1.0]),
    ];
    let cpu_capture_medium = execute_capture_query(&ctx, &capture_medium_plan, &capture_medium_args)
        .expect("cpu capture medium");
    let (vgpu_capture_medium, vgpu_capture_medium_trace) = execute_capture_query_with_trace_on(
        &ctx,
        DispatchBackend::VirtualGpu,
        &capture_medium_plan,
        &capture_medium_args,
    )
    .expect("vgpu capture medium");
    assert_eq!(cpu_capture_medium, vgpu_capture_medium);
    assert_eq!(vgpu_capture_medium_trace.backend, DispatchBackend::VirtualGpu);
    assert_eq!(
        vgpu_capture_medium_trace.executor,
        DirectQueryExecutor::VirtualGpu
    );

    let world_trace_plan = lower_world_query_plan(&WorldQueryPlan::for_query(WorldQueryKind::Trace));
    let world_trace_args = vec![
        KernelValue::Capture(SmolStr::new("scene_region")),
        scene_domain(region_scene_id, 1, true, true, true),
        KernelValue::Vec3([0.0, 0.0, 3.0]),
        KernelValue::Vec3([0.0, 0.0, -1.0]),
        KernelValue::F32(6.0),
        KernelValue::F32(0.05),
        KernelValue::F32(0.001),
        KernelValue::I32(96),
    ];
    let cpu_world_trace =
        execute_world_query(&ctx, &world_trace_plan, &world_trace_args).expect("cpu world trace");
    let (vgpu_world_trace, vgpu_world_trace_trace) = execute_world_query_with_trace_on(
        &ctx,
        DispatchBackend::VirtualGpu,
        &world_trace_plan,
        &world_trace_args,
    )
    .expect("vgpu world trace");
    assert_eq!(cpu_world_trace, vgpu_world_trace);
    assert_eq!(vgpu_world_trace_trace.backend, DispatchBackend::VirtualGpu);
    assert_eq!(
        vgpu_world_trace_trace.executor,
        DirectQueryExecutor::VirtualGpu
    );

    let world_surface_plan =
        lower_world_query_plan(&WorldQueryPlan::for_query(WorldQueryKind::Surface));
    let world_surface_args = vec![
        KernelValue::Capture(SmolStr::new("scene_region")),
        scene_domain(region_scene_id, 1, true, true, true),
        cpu_world_trace.clone(),
    ];
    let cpu_world_surface =
        execute_world_query(&ctx, &world_surface_plan, &world_surface_args)
            .expect("cpu world surface");
    let (vgpu_world_surface, vgpu_world_surface_trace) = execute_world_query_with_trace_on(
        &ctx,
        DispatchBackend::VirtualGpu,
        &world_surface_plan,
        &world_surface_args,
    )
    .expect("vgpu world surface");
    assert_eq!(cpu_world_surface, vgpu_world_surface);
    assert_eq!(vgpu_world_surface_trace.backend, DispatchBackend::VirtualGpu);
    assert_eq!(
        vgpu_world_surface_trace.executor,
        DirectQueryExecutor::VirtualGpu
    );

    let world_radiance_plan =
        lower_world_query_plan(&WorldQueryPlan::for_query(WorldQueryKind::Radiance));
    let world_radiance_args = vec![
        KernelValue::Capture(SmolStr::new("scene_region")),
        scene_domain(region_scene_id, 1, true, true, true),
        KernelValue::Vec3([0.0, 0.0, 1.0]),
        KernelValue::Vec3([0.0, 0.0, -1.0]),
    ];
    let cpu_world_radiance =
        execute_world_query(&ctx, &world_radiance_plan, &world_radiance_args)
            .expect("cpu world radiance");
    let (vgpu_world_radiance, vgpu_world_radiance_trace) = execute_world_query_with_trace_on(
        &ctx,
        DispatchBackend::VirtualGpu,
        &world_radiance_plan,
        &world_radiance_args,
    )
    .expect("vgpu world radiance");
    assert_eq!(cpu_world_radiance, vgpu_world_radiance);
    assert_eq!(vgpu_world_radiance_trace.backend, DispatchBackend::VirtualGpu);
    assert_eq!(
        vgpu_world_radiance_trace.executor,
        DirectQueryExecutor::VirtualGpu
    );

    let world_medium_plan =
        lower_world_query_plan(&WorldQueryPlan::for_query(WorldQueryKind::Medium));
    let world_medium_args = vec![
        KernelValue::Capture(SmolStr::new("scene_region")),
        scene_domain(region_scene_id, 1, true, true, true),
        KernelValue::Vec3([0.0, 0.0, 1.0]),
    ];
    let cpu_world_medium =
        execute_world_query(&ctx, &world_medium_plan, &world_medium_args)
            .expect("cpu world medium");
    let (vgpu_world_medium, vgpu_world_medium_trace) = execute_world_query_with_trace_on(
        &ctx,
        DispatchBackend::VirtualGpu,
        &world_medium_plan,
        &world_medium_args,
    )
    .expect("vgpu world medium");
    assert_eq!(cpu_world_medium, vgpu_world_medium);
    assert_eq!(vgpu_world_medium_trace.backend, DispatchBackend::VirtualGpu);
    assert_eq!(
        vgpu_world_medium_trace.executor,
        DirectQueryExecutor::VirtualGpu
    );
}

#[test]
fn kernel_entry_executes_query_expressions_via_query_exec_context() {
    let (module, type_info, _) = typed_query_module(query_fixture_source());
    let program =
        lower_kernel_entry_by_name(&module, &type_info, "portable_entry").expect("kernel lower");
    let mut runtime = Default::default();
    let value = execute_entry(&program, Vec::new(), &mut runtime).expect("execute entry");
    let summary = expect_struct(&value, "QuerySummary");

    assert_approx_eq(expect_f32(field(summary, "distance")), 1.0);
    assert_approx_eq(expect_f32(field(summary, "world_distance")), 1.0);
    assert_approx_eq(expect_f32(field(summary, "batch_distance0")), 1.0);
    assert_approx_eq(expect_f32(field(summary, "batch_distance1")), 2.0);
    assert!(expect_bool(field(summary, "occluded0")));

    let hit = expect_struct(field(summary, "hit"), "Hit3");
    let world_hit = expect_struct(field(summary, "world_hit"), "Hit3");
    let surface = expect_struct(field(summary, "surface"), "Surface");

    assert!(expect_bool(field(hit, "hit")));
    assert!(expect_bool(field(world_hit, "hit")));
    assert_eq!(expect_vec3(field(hit, "position")), [0.0, 0.0, 1.0]);
    assert_eq!(expect_vec3(field(surface, "albedo")), [0.25, 0.35, 0.45]);
}

#[test]
fn kernel_entry_can_route_direct_queries_through_virtual_gpu_backend() {
    let (module, type_info, _) = typed_query_module(query_fixture_source());
    let program =
        lower_kernel_entry_by_name(&module, &type_info, "portable_entry").expect("kernel lower");

    let mut cpu_runtime = Default::default();
    let cpu_value = execute_entry(&program, Vec::new(), &mut cpu_runtime).expect("cpu execute");

    let mut vgpu_runtime = Default::default();
    let vgpu_value = execute_entry_on(
        &program,
        DispatchBackend::VirtualGpu,
        Vec::new(),
        &mut vgpu_runtime,
    )
    .expect("virtual gpu execute");

    assert_eq!(cpu_value, vgpu_value);
}
