use smol_str::SmolStr;
use std::path::PathBuf;
use wrela::artifact_contract::{ArtifactUseKind, ArtifactUseSource};
use wrela::hir;
use wrela::hir::lower as hir_lower;
use wrela::hir::project::load_project;
use wrela::kernel::{
    KernelBatchItemContract, KernelPlanStage, KernelStructValue, KernelValue, execute_entry,
    execute_entry_on, lower_batch_query_plan, lower_capture_query_plan, lower_kernel_entry_by_name,
    lower_world_query_plan,
};
use wrela::parser::ast;
use wrela::parser::ast::AstNode;
use wrela::parser::parse;
use wrela::query_contract;
use wrela::query_exec::{
    BatchQueryExecutionTrace, CostFidelity, DirectQueryExecutionTrace, DirectQueryExecutor,
    QueryExecContext, QueryExecutionPolicy, RayBudgetPolicy, RequiredGuaranteeClass,
    SelectedMethodClass, SemanticCostCauseKind, SemanticCostUnit, SemanticStageKind,
    executable_region_shape_lists, execute_batch_query_with_trace,
    execute_batch_query_with_trace_on, execute_capture_query, execute_capture_query_on,
    execute_capture_query_with_trace_on, execute_world_query, execute_world_query_on,
    execute_world_query_with_policy_with_trace_on, execute_world_query_with_trace_on,
    render_semantic_cost_report, stable_field_scene_capture_id, stable_region_scene_capture_id,
    stable_shape_capture_id, stable_shape_scene_capture_id,
};
use wrela::query_plan::{
    ArtifactSchema, BatchQueryKind, BatchQueryPlan, CaptureKind, CaptureQueryKind,
    CaptureQueryPlan, DispatchBackend, WorldQueryKind, WorldQueryPlan,
};
use wrela::query_solver::RaySolverMethod;

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

fn assert_direct_trace_contract(
    trace: &DirectQueryExecutionTrace,
    contract_id: query_contract::QueryContractId,
) {
    let descriptor = query_contract::query_contract(contract_id).expect("query contract");
    assert_eq!(trace.contract_id, descriptor.id);
    assert_eq!(trace.family, descriptor.family);
    assert_eq!(trace.question, descriptor.question);
    assert_eq!(trace.surface, descriptor.surface);
    assert_eq!(trace.contract_version, descriptor.version);
}

fn assert_batch_trace_contract(
    trace: &BatchQueryExecutionTrace,
    contract_id: query_contract::QueryContractId,
) {
    let descriptor = query_contract::query_contract(contract_id).expect("query contract");
    assert_eq!(trace.contract_id, descriptor.id);
    assert_eq!(trace.family, descriptor.family);
    assert_eq!(trace.question, descriptor.question);
    assert_eq!(trace.surface, descriptor.surface);
    assert_eq!(trace.contract_version, descriptor.version);
    assert_eq!(trace.plan_trace.contract_id, descriptor.id);
    assert_eq!(trace.plan_trace.family, descriptor.family);
    assert_eq!(trace.plan_trace.question, descriptor.question);
    assert_eq!(trace.plan_trace.surface, descriptor.surface);
    assert_eq!(trace.plan_trace.contract_version, descriptor.version);
}

fn assert_normal_role(trace: &DirectQueryExecutionTrace, expected: &str) {
    assert_eq!(trace.observability.normal_role.as_deref(), Some(expected));
}

fn assert_batch_normal_role(trace: &BatchQueryExecutionTrace, expected: &str) {
    assert_eq!(trace.observability.normal_role.as_deref(), Some(expected));
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("compiler crate should have repo parent")
        .to_path_buf()
}

fn typed_project_query_module(
    project_root: &str,
) -> (hir::Module, hir::TypeInfo, QueryExecContext) {
    let entry_path = repo_root().join(project_root).join("src").join("main.wr");
    let project = load_project(&entry_path).expect("load project");
    let module = project.module;
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
    scalar_occluded: Boolean
    world_occluded: Boolean
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
        ray=ray_query(
            origin=vec3(0.0, 0.0, 3.0),
            direction=vec3(0.0, 0.0, -1.0),
            max_distance=6.0,
            min_step=0.05,
            hit_epsilon=0.001,
            max_steps=96
        )
    )
    world_hit = trace_world(
        capture=world,
        domain=domain,
        ray=ray_query(
            origin=vec3(0.0, 0.0, 3.0),
            direction=vec3(0.0, 0.0, -1.0),
            max_distance=6.0,
            min_step=0.05,
            hit_epsilon=0.001,
            max_steps=96
        )
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
    scalar_occlusion = occluded(
        capture=scene,
        ray=ray_query(
            origin=vec3(0.0, 0.0, 3.0),
            direction=vec3(0.0, 0.0, -1.0),
            max_distance=6.0,
            min_step=0.05,
            hit_epsilon=0.001,
            max_steps=96
        )
    )
    world_occlusion = occluded_world(
        capture=world,
        domain=domain,
        ray=ray_query(
            origin=vec3(0.0, 0.0, 3.0),
            direction=vec3(0.0, 0.0, -1.0),
            max_distance=6.0,
            min_step=0.05,
            hit_epsilon=0.001,
            max_steps=96
        ),
        backend=dispatch_backend_virtual_gpu()
    )
    return QuerySummary(
        distance=distance_at(capture=scene, point=vec3(0.0, 0.0, 2.0)),
        world_distance=distance_world(capture=world, domain=domain, point=vec3(0.0, 0.0, 2.0)),
        batch_distance0=distances[0].distance,
        batch_distance1=distances[1].distance,
        occluded0=occlusions[0].occluded,
        scalar_occluded=scalar_occlusion.occluded,
        world_occluded=world_occlusion.occluded,
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
    scene_domain_with_limits(
        scene_id, detail, material, radiance, media, 6.0, 0.05, 0.001, 96,
    )
}

fn scene_domain_with_limits(
    scene_id: u32,
    detail: i32,
    material: bool,
    radiance: bool,
    media: bool,
    _max_distance: f32,
    _min_step: f32,
    _hit_epsilon: f32,
    _max_steps: i32,
) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("SceneDomain"),
        fields: vec![
            (SmolStr::new("scene_id"), KernelValue::U32(scene_id)),
            (
                SmolStr::new("spatial"),
                KernelValue::Struct(KernelStructValue {
                    name: SmolStr::new("SpatialDomainContract"),
                    fields: vec![(SmolStr::new("geometry_detail"), KernelValue::I32(detail))],
                }),
            ),
            (
                SmolStr::new("surface"),
                KernelValue::Struct(KernelStructValue {
                    name: SmolStr::new("SurfaceDomainContract"),
                    fields: vec![(SmolStr::new("material"), KernelValue::Bool(material))],
                }),
            ),
            (
                SmolStr::new("participants"),
                KernelValue::Struct(KernelStructValue {
                    name: SmolStr::new("ParticipantDomainContract"),
                    fields: vec![
                        (SmolStr::new("radiance"), KernelValue::Bool(radiance)),
                        (SmolStr::new("media"), KernelValue::Bool(media)),
                    ],
                }),
            ),
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
    ray_query_with_limits(origin, direction, 6.0, 0.05, 0.001, 96)
}

fn ray_query_with_limits(
    origin: [f32; 3],
    direction: [f32; 3],
    max_distance: f32,
    min_step: f32,
    hit_epsilon: f32,
    max_steps: i32,
) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("RayQuery"),
        fields: vec![
            (SmolStr::new("origin"), KernelValue::Vec3(origin)),
            (SmolStr::new("direction"), KernelValue::Vec3(direction)),
            (SmolStr::new("max_distance"), KernelValue::F32(max_distance)),
            (SmolStr::new("min_step"), KernelValue::F32(min_step)),
            (SmolStr::new("hit_epsilon"), KernelValue::F32(hit_epsilon)),
            (SmolStr::new("max_steps"), KernelValue::I32(max_steps)),
        ],
    })
}

fn point_direction_query(point: [f32; 3], direction: [f32; 3]) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("PointDirectionQuery"),
        fields: vec![
            (SmolStr::new("point"), KernelValue::Vec3(point)),
            (SmolStr::new("direction"), KernelValue::Vec3(direction)),
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

fn expect_i32(value: &KernelValue) -> i32 {
    match value {
        KernelValue::I32(value) => *value,
        other => panic!("expected I32, got {other:?}"),
    }
}

fn expect_vec3(value: &KernelValue) -> [f32; 3] {
    match value {
        KernelValue::Vec3(value) => *value,
        other => panic!("expected Vec3, got {other:?}"),
    }
}

fn expect_u32(value: &KernelValue) -> u32 {
    match value {
        KernelValue::U32(value) => *value,
        other => panic!("expected U32, got {other:?}"),
    }
}

fn expect_mat4(value: &KernelValue) -> [f32; 16] {
    match value {
        KernelValue::Mat4(value) => *value,
        other => panic!("expected Mat4, got {other:?}"),
    }
}

fn expect_array(value: &KernelValue) -> &[KernelValue] {
    match value {
        KernelValue::Array(items) => items,
        other => panic!("expected Array, got {other:?}"),
    }
}

fn assert_approx_eq(lhs: f32, rhs: f32) {
    assert!(
        (lhs - rhs).abs() < 0.01,
        "expected {lhs} ~= {rhs}, delta={}",
        (lhs - rhs).abs()
    );
}

fn assert_approx_eq_at(lhs: f32, rhs: f32, label: &str, x: usize, y: usize) {
    let delta = (lhs - rhs).abs();
    assert!(
        delta < 0.01,
        "{label} mismatch at pixel ({x}, {y}): lhs={lhs} rhs={rhs} delta={delta}"
    );
}

fn assert_vec3_approx_eq(lhs: [f32; 3], rhs: [f32; 3]) {
    for (lhs, rhs) in lhs.into_iter().zip(rhs) {
        assert_approx_eq(lhs, rhs);
    }
}

fn assert_vec3_approx_eq_at(lhs: [f32; 3], rhs: [f32; 3], label: &str, x: usize, y: usize) {
    for (index, (lhs, rhs)) in lhs.into_iter().zip(rhs).enumerate() {
        assert_approx_eq_at(lhs, rhs, &format!("{label}[{index}]"), x, y);
    }
}

fn assert_mat4_approx_eq(lhs: [f32; 16], rhs: [f32; 16]) {
    for (lhs, rhs) in lhs.into_iter().zip(rhs) {
        assert_approx_eq(lhs, rhs);
    }
}

fn assert_hit3_approx_eq(lhs: &KernelValue, rhs: &KernelValue) {
    let lhs = expect_struct(lhs, "Hit3");
    let rhs = expect_struct(rhs, "Hit3");
    assert_eq!(
        expect_bool(field(lhs, "hit")),
        expect_bool(field(rhs, "hit"))
    );
    assert_approx_eq(
        expect_f32(field(lhs, "distance")),
        expect_f32(field(rhs, "distance")),
    );
    assert_vec3_approx_eq(
        expect_vec3(field(lhs, "position")),
        expect_vec3(field(rhs, "position")),
    );
    assert_vec3_approx_eq(
        expect_vec3(field(lhs, "normal")),
        expect_vec3(field(rhs, "normal")),
    );
    assert_vec3_approx_eq(
        expect_vec3(field(lhs, "local_position")),
        expect_vec3(field(rhs, "local_position")),
    );
    assert_vec3_approx_eq(
        expect_vec3(field(lhs, "local_normal")),
        expect_vec3(field(rhs, "local_normal")),
    );
    assert_eq!(
        expect_u32(field(lhs, "feature_id")),
        expect_u32(field(rhs, "feature_id"))
    );
    assert_eq!(
        expect_u32(field(lhs, "instance_id")),
        expect_u32(field(rhs, "instance_id"))
    );
    assert_eq!(
        expect_u32(field(lhs, "repeat_id")),
        expect_u32(field(rhs, "repeat_id"))
    );
    assert_eq!(
        expect_u32(field(lhs, "root_shape_id")),
        expect_u32(field(rhs, "root_shape_id"))
    );
    assert_eq!(field(lhs, "payload"), field(rhs, "payload"));

    let lhs_frame = expect_struct(field(lhs, "shading_frame"), "Transform3");
    let rhs_frame = expect_struct(field(rhs, "shading_frame"), "Transform3");
    assert_mat4_approx_eq(
        expect_mat4(field(lhs_frame, "matrix")),
        expect_mat4(field(rhs_frame, "matrix")),
    );
    assert_mat4_approx_eq(
        expect_mat4(field(lhs_frame, "inverse")),
        expect_mat4(field(rhs_frame, "inverse")),
    );
}

fn assert_occlusion_approx_eq(lhs: &KernelValue, rhs: &KernelValue) {
    let lhs = expect_struct(lhs, "OcclusionResult");
    let rhs = expect_struct(rhs, "OcclusionResult");
    assert_eq!(
        expect_bool(field(lhs, "occluded")),
        expect_bool(field(rhs, "occluded"))
    );
    assert_approx_eq(
        expect_f32(field(lhs, "distance")),
        expect_f32(field(rhs, "distance")),
    );
    assert_eq!(
        expect_i32(field(lhs, "steps")),
        expect_i32(field(rhs, "steps"))
    );
}

fn assert_surface_approx_eq(lhs: &KernelValue, rhs: &KernelValue) {
    let lhs = expect_struct(lhs, "Surface");
    let rhs = expect_struct(rhs, "Surface");
    assert_vec3_approx_eq(
        expect_vec3(field(lhs, "albedo")),
        expect_vec3(field(rhs, "albedo")),
    );
    assert_approx_eq(
        expect_f32(field(lhs, "roughness")),
        expect_f32(field(rhs, "roughness")),
    );
    assert_approx_eq(
        expect_f32(field(lhs, "metalness")),
        expect_f32(field(rhs, "metalness")),
    );
    assert_approx_eq(
        expect_f32(field(lhs, "clearcoat")),
        expect_f32(field(rhs, "clearcoat")),
    );
    assert_approx_eq(
        expect_f32(field(lhs, "clearcoat_roughness")),
        expect_f32(field(rhs, "clearcoat_roughness")),
    );
    assert_approx_eq(
        expect_f32(field(lhs, "sheen")),
        expect_f32(field(rhs, "sheen")),
    );
    assert_vec3_approx_eq(
        expect_vec3(field(lhs, "emissive")),
        expect_vec3(field(rhs, "emissive")),
    );
}

fn assert_medium_approx_eq(lhs: &KernelValue, rhs: &KernelValue) {
    let lhs = expect_struct(lhs, "Medium");
    let rhs = expect_struct(rhs, "Medium");
    assert_approx_eq(
        expect_f32(field(lhs, "density")),
        expect_f32(field(rhs, "density")),
    );
    assert_vec3_approx_eq(
        expect_vec3(field(lhs, "emission")),
        expect_vec3(field(rhs, "emission")),
    );
    assert_approx_eq(
        expect_f32(field(lhs, "anisotropy")),
        expect_f32(field(rhs, "anisotropy")),
    );
}

fn add3(lhs: [f32; 3], rhs: [f32; 3]) -> [f32; 3] {
    [lhs[0] + rhs[0], lhs[1] + rhs[1], lhs[2] + rhs[2]]
}

fn sub3(lhs: [f32; 3], rhs: [f32; 3]) -> [f32; 3] {
    [lhs[0] - rhs[0], lhs[1] - rhs[1], lhs[2] - rhs[2]]
}

fn mul3(value: [f32; 3], scalar: f32) -> [f32; 3] {
    [value[0] * scalar, value[1] * scalar, value[2] * scalar]
}

fn dot3(lhs: [f32; 3], rhs: [f32; 3]) -> f32 {
    lhs[0] * rhs[0] + lhs[1] * rhs[1] + lhs[2] * rhs[2]
}

fn cross3(lhs: [f32; 3], rhs: [f32; 3]) -> [f32; 3] {
    [
        lhs[1] * rhs[2] - lhs[2] * rhs[1],
        lhs[2] * rhs[0] - lhs[0] * rhs[2],
        lhs[0] * rhs[1] - lhs[1] * rhs[0],
    ]
}

fn length3(value: [f32; 3]) -> f32 {
    dot3(value, value).sqrt()
}

fn normalize3(value: [f32; 3]) -> [f32; 3] {
    let len = length3(value);
    if len <= f32::EPSILON {
        [0.0, 0.0, 1.0]
    } else {
        [value[0] / len, value[1] / len, value[2] / len]
    }
}

fn preview_ray(
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    camera_forward: [f32; 3],
    world_up: [f32; 3],
    view_scale: f32,
) -> [f32; 3] {
    let width_float = width as f32;
    let height_float = height as f32;
    let aspect = width_float / height_float;
    let right = normalize3(cross3(camera_forward, world_up));
    let up = normalize3(cross3(right, camera_forward));
    let sample_u = (x as f32 + 0.5) / width_float;
    let sample_v = (y as f32 + 0.5) / height_float;
    let screen_x = ((sample_u - 0.5) * 2.0) * aspect * view_scale;
    let screen_y = ((0.5 - sample_v) * 2.0) * view_scale;
    normalize3(add3(
        add3(camera_forward, mul3(right, screen_x)),
        mul3(up, screen_y),
    ))
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
            ray_query_with_limits([0.0, 0.0, 3.0], [0.0, 0.0, -1.0], 6.0, 0.05, 0.001, 96),
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
            point_direction_query([0.0, 0.0, 1.0], [0.0, 1.0, 0.0]),
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
            ray_query_with_limits([0.0, 0.0, 3.0], [0.0, 0.0, -1.0], 6.0, 0.05, 0.001, 96),
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
            ray_query_with_limits([0.0, 0.0, 3.0], [0.0, 0.0, -1.0], 6.0, 0.05, 0.001, 96),
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
            point_direction_query([0.0, 0.0, 1.0], [0.0, 1.0, 0.0]),
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
    assert_eq!(
        vgpu_field_capture_trace.backend,
        DispatchBackend::VirtualGpu
    );
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
    assert_eq!(
        vgpu_capture_normal_trace.backend,
        DispatchBackend::VirtualGpu
    );
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

    let world_normal_plan =
        lower_world_query_plan(&WorldQueryPlan::for_query(WorldQueryKind::Normal));
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
        ray_query_with_limits([0.0, 0.0, 3.0], [0.0, 0.0, -1.0], 6.0, 0.05, 0.001, 96),
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
    assert_eq!(
        vgpu_capture_trace_trace.backend,
        DispatchBackend::VirtualGpu
    );
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
    let cpu_capture_surface =
        execute_capture_query(&ctx, &capture_surface_plan, &capture_surface_args)
            .expect("cpu capture surface");
    let (vgpu_capture_surface, vgpu_capture_surface_trace) = execute_capture_query_with_trace_on(
        &ctx,
        DispatchBackend::VirtualGpu,
        &capture_surface_plan,
        &capture_surface_args,
    )
    .expect("vgpu capture surface");
    assert_eq!(cpu_capture_surface, vgpu_capture_surface);
    assert_eq!(
        vgpu_capture_surface_trace.backend,
        DispatchBackend::VirtualGpu
    );
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
        point_direction_query([0.0, 0.0, 1.0], [0.0, 0.0, -1.0]),
    ];
    let cpu_capture_radiance =
        execute_capture_query(&ctx, &capture_radiance_plan, &capture_radiance_args)
            .expect("cpu capture radiance");
    let (vgpu_capture_radiance, vgpu_capture_radiance_trace) = execute_capture_query_with_trace_on(
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
    let cpu_capture_medium =
        execute_capture_query(&ctx, &capture_medium_plan, &capture_medium_args)
            .expect("cpu capture medium");
    let (vgpu_capture_medium, vgpu_capture_medium_trace) = execute_capture_query_with_trace_on(
        &ctx,
        DispatchBackend::VirtualGpu,
        &capture_medium_plan,
        &capture_medium_args,
    )
    .expect("vgpu capture medium");
    assert_eq!(cpu_capture_medium, vgpu_capture_medium);
    assert_eq!(
        vgpu_capture_medium_trace.backend,
        DispatchBackend::VirtualGpu
    );
    assert_eq!(
        vgpu_capture_medium_trace.executor,
        DirectQueryExecutor::VirtualGpu
    );

    let world_trace_plan =
        lower_world_query_plan(&WorldQueryPlan::for_query(WorldQueryKind::Trace));
    let world_trace_args = vec![
        KernelValue::Capture(SmolStr::new("scene_region")),
        scene_domain(region_scene_id, 1, true, true, true),
        ray_query_with_limits([0.0, 0.0, 3.0], [0.0, 0.0, -1.0], 6.0, 0.05, 0.001, 96),
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
    let cpu_world_surface = execute_world_query(&ctx, &world_surface_plan, &world_surface_args)
        .expect("cpu world surface");
    let (vgpu_world_surface, vgpu_world_surface_trace) = execute_world_query_with_trace_on(
        &ctx,
        DispatchBackend::VirtualGpu,
        &world_surface_plan,
        &world_surface_args,
    )
    .expect("vgpu world surface");
    assert_eq!(cpu_world_surface, vgpu_world_surface);
    assert_eq!(
        vgpu_world_surface_trace.backend,
        DispatchBackend::VirtualGpu
    );
    assert_eq!(
        vgpu_world_surface_trace.executor,
        DirectQueryExecutor::VirtualGpu
    );

    let world_radiance_plan =
        lower_world_query_plan(&WorldQueryPlan::for_query(WorldQueryKind::Radiance));
    let world_radiance_args = vec![
        KernelValue::Capture(SmolStr::new("scene_region")),
        scene_domain(region_scene_id, 1, true, true, true),
        point_direction_query([0.0, 0.0, 1.0], [0.0, 0.0, -1.0]),
    ];
    let cpu_world_radiance = execute_world_query(&ctx, &world_radiance_plan, &world_radiance_args)
        .expect("cpu world radiance");
    let (vgpu_world_radiance, vgpu_world_radiance_trace) = execute_world_query_with_trace_on(
        &ctx,
        DispatchBackend::VirtualGpu,
        &world_radiance_plan,
        &world_radiance_args,
    )
    .expect("vgpu world radiance");
    assert_eq!(cpu_world_radiance, vgpu_world_radiance);
    assert_eq!(
        vgpu_world_radiance_trace.backend,
        DispatchBackend::VirtualGpu
    );
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
    let cpu_world_medium = execute_world_query(&ctx, &world_medium_plan, &world_medium_args)
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
fn query_exec_virtual_gpu_batch_execution_uses_lowered_item_contracts() {
    let (_module, _type_info, ctx) = typed_query_module(query_fixture_source());
    let mut plan = lower_batch_query_plan(&BatchQueryPlan::for_shape_query(
        BatchQueryKind::Trace,
        DispatchBackend::VirtualGpu,
        None,
    ));
    plan.item_contract = KernelBatchItemContract::CaptureQuery {
        plan: lower_capture_query_plan(
            &CaptureQueryPlan::for_query(CaptureQueryKind::Surface, CaptureKind::Shape, None)
                .expect("surface capture plan"),
        ),
    };

    let capture = KernelValue::Capture(SmolStr::new("scene_shape"));
    let rays = KernelValue::Array(vec![ray_query([0.0, 0.0, 3.0], [0.0, 0.0, -1.0])]);
    let error = execute_batch_query_with_trace(&ctx, &plan, &[capture, rays])
        .expect_err("mismatched item contract should fail");
    assert!(
        format!("{error:?}").contains("capture item contract result kind")
            && format!("{error:?}").contains("does not match batch result contract"),
        "expected contract validation mismatch, got {error:?}"
    );
}

#[test]
fn query_exec_traces_report_observability_counters() {
    let (_, _, ctx) = typed_query_module(query_fixture_source());

    let shape_trace_plan = lower_capture_query_plan(
        &CaptureQueryPlan::for_query(CaptureQueryKind::Trace, CaptureKind::Shape, None)
            .expect("shape trace plan"),
    );
    let (_cpu_hit, cpu_trace) = execute_capture_query_with_trace_on(
        &ctx,
        DispatchBackend::Cpu,
        &shape_trace_plan,
        &[
            KernelValue::Capture(SmolStr::new("scene_shape")),
            ray_query_with_limits([0.0, 0.0, 3.0], [0.0, 0.0, -1.0], 6.0, 0.05, 0.001, 96),
        ],
    )
    .expect("cpu trace");
    assert_eq!(cpu_trace.observability.dispatch_count, 1);
    assert!(cpu_trace.observability.trace_steps > 0);
    assert!(cpu_trace.observability.field_samples > 0);
    assert!(cpu_trace.observability.branch_visits > 0);
    assert!(cpu_trace.observability.artifact_loads > 0);
    assert!(cpu_trace.observability.acceleration_node_visits > 0);
    assert!(cpu_trace.observability.cache_brick_visits > 0);
    assert!(cpu_trace.observability.cache_brick_hits > 0);
    assert!(cpu_trace.observability.accepted_relaxed_steps > 0);
    assert_eq!(
        cpu_trace
            .snapshot
            .as_ref()
            .map(|snapshot| (snapshot.capture_name.as_str(), snapshot.epoch.0)),
        Some(("scene_shape", 1))
    );
    assert_eq!(
        cpu_trace.cost_report.unit,
        SemanticCostUnit::CaptureCandidates
    );
    assert_eq!(cpu_trace.cost_report.fidelity, CostFidelity::Exact);
    assert!(
        cpu_trace
            .cost_report
            .causes
            .iter()
            .any(|cause| { cause.kind == SemanticCostCauseKind::MarchPressure })
    );
    let rendered = render_semantic_cost_report(&cpu_trace.cost_report);
    assert!(rendered.contains("acceleration_node_visits="));
    assert!(rendered.contains("cache_brick_visits="));
    assert!(rendered.contains("accepted_relaxed_steps="));
    assert!(rendered.contains("observer_continuation_seed_hits="));

    let region_scene_id = stable_region_scene_capture_id(&SmolStr::new("scene_region"));
    let world_trace_plan =
        lower_world_query_plan(&WorldQueryPlan::for_query(WorldQueryKind::Trace));
    let (_vgpu_world_hit, vgpu_world_trace) = execute_world_query_with_trace_on(
        &ctx,
        DispatchBackend::VirtualGpu,
        &world_trace_plan,
        &[
            KernelValue::Capture(SmolStr::new("scene_region")),
            scene_domain(region_scene_id, 1, true, true, true),
            ray_query_with_limits([0.0, 0.0, 3.0], [0.0, 0.0, -1.0], 6.0, 0.05, 0.001, 96),
        ],
    )
    .expect("vgpu world trace");
    assert_eq!(vgpu_world_trace.observability.dispatch_count, 1);
    assert!(vgpu_world_trace.observability.candidate_count > 0);
    assert!(vgpu_world_trace.observability.trace_steps > 0);
    assert!(vgpu_world_trace.observability.artifact_loads > 0);
    assert!(vgpu_world_trace.observability.solver_plan_id.is_some());
    assert!(vgpu_world_trace.observability.solver_dense_fallback_rays > 0);
    assert_eq!(
        vgpu_world_trace
            .snapshot
            .as_ref()
            .map(|snapshot| (snapshot.capture_name.as_str(), snapshot.epoch.0)),
        Some(("scene_region", 1))
    );
    assert!(
        vgpu_world_trace
            .cost_report
            .dominant_stages
            .iter()
            .any(|stage| stage.stage == SemanticStageKind::RaySolver)
    );
    assert_eq!(
        vgpu_world_trace.cost_report.unit,
        SemanticCostUnit::WorldShapes
    );
    assert_eq!(vgpu_world_trace.cost_report.fidelity, CostFidelity::Exact);
    assert!(
        vgpu_world_trace
            .cost_report
            .causes
            .iter()
            .any(|cause| { cause.kind == SemanticCostCauseKind::SupportTopology })
    );

    let (_wgsl_world_hit, wgsl_world_trace) = execute_world_query_with_trace_on(
        &ctx,
        DispatchBackend::Wgsl,
        &world_trace_plan,
        &[
            KernelValue::Capture(SmolStr::new("scene_region")),
            scene_domain(region_scene_id, 1, true, true, true),
            ray_query_with_limits([0.0, 0.0, 3.0], [0.0, 0.0, -1.0], 6.0, 0.05, 0.001, 96),
        ],
    )
    .expect("wgsl world trace");
    assert_eq!(wgsl_world_trace.observability.dispatch_count, 1);
    assert!(wgsl_world_trace.observability.candidate_count > 0);
    assert!(wgsl_world_trace.observability.trace_steps > 0);
    assert!(wgsl_world_trace.observability.artifact_loads > 0);
    assert!(wgsl_world_trace.observability.solver_plan_id.is_some());
    assert_eq!(
        wgsl_world_trace
            .observability
            .solver_generated_dense_fallback_rays,
        1
    );
    assert!(
        render_semantic_cost_report(&wgsl_world_trace.cost_report)
            .contains("solver_generated_dense_fallback_rays=1")
    );
    assert!(
        render_semantic_cost_report(&wgsl_world_trace.cost_report)
            .contains("observer_continuation_seed_hits=")
    );
    assert_eq!(
        wgsl_world_trace.cost_report.fidelity,
        CostFidelity::StructuralApproximation
    );

    let batch_plan = lower_batch_query_plan(&BatchQueryPlan::for_shape_query(
        BatchQueryKind::Trace,
        DispatchBackend::VirtualGpu,
        None,
    ));
    let (_batch_hits, batch_trace) = execute_batch_query_with_trace(
        &ctx,
        &batch_plan,
        &[
            KernelValue::Capture(SmolStr::new("scene_shape")),
            KernelValue::Array(vec![
                ray_query([0.0, 0.0, 3.0], [0.0, 0.0, -1.0]),
                ray_query([0.0, 0.0, 3.0], [0.0, 1.0, 0.0]),
            ]),
        ],
    )
    .expect("vgpu batch trace");
    assert_eq!(batch_trace.observability.dispatch_count, 1);
    assert_eq!(batch_trace.observability.candidate_count, 2);
    assert!(batch_trace.observability.artifact_loads > 0);
    assert_eq!(batch_trace.cost_report.unit, SemanticCostUnit::BatchItems);
    assert!(
        batch_trace
            .cost_report
            .dominant_stages
            .iter()
            .any(|stage| { stage.stage == SemanticStageKind::ItemIteration })
    );

    let wgsl_batch_plan = lower_batch_query_plan(&BatchQueryPlan::for_shape_query(
        BatchQueryKind::Trace,
        DispatchBackend::Wgsl,
        None,
    ));
    let (_wgsl_batch_hits, wgsl_batch_trace) = execute_batch_query_with_trace(
        &ctx,
        &wgsl_batch_plan,
        &[
            KernelValue::Capture(SmolStr::new("scene_shape")),
            KernelValue::Array(vec![
                ray_query([0.0, 0.0, 3.0], [0.0, 0.0, -1.0]),
                ray_query([0.0, 0.0, 3.0], [0.0, 1.0, 0.0]),
            ]),
        ],
    )
    .expect("wgsl batch trace");
    assert_eq!(wgsl_batch_trace.observability.dispatch_count, 1);
    assert_eq!(wgsl_batch_trace.observability.candidate_count, 2);
    assert!(wgsl_batch_trace.observability.trace_steps > 0);
    assert!(wgsl_batch_trace.observability.artifact_loads > 0);
    assert_eq!(
        wgsl_batch_trace.cost_report.fidelity,
        CostFidelity::StructuralApproximation
    );
}

#[test]
fn query_plans_declare_store_backed_artifact_dependencies_with_explicit_validity_rules() {
    let plan =
        BatchQueryPlan::for_shape_query(BatchQueryKind::Trace, DispatchBackend::VirtualGpu, None);
    let semantic_artifacts = plan
        .semantic_artifact_contracts()
        .into_iter()
        .map(|contract| (contract.id.clone(), contract))
        .collect::<std::collections::BTreeMap<_, _>>();
    let store_loads = plan
        .artifact_uses()
        .into_iter()
        .filter(|use_record| use_record.source == ArtifactUseSource::ArtifactStore)
        .collect::<Vec<_>>();

    assert!(
        !store_loads.is_empty(),
        "shape trace plans should declare store-backed query artifacts"
    );
    for use_record in &store_loads {
        assert_eq!(use_record.kind, ArtifactUseKind::Load);
        let contract = semantic_artifacts
            .get(&use_record.artifact_id)
            .expect("semantic artifact contract for store-backed use");
        assert!(
            contract.validity.is_explicit(),
            "store-backed artifact '{}' must declare explicit validity",
            contract.id
        );
        assert_eq!(
            use_record.required_validity.as_ref(),
            Some(&contract.validity),
            "artifact use should preserve the contract validity rule for '{}'",
            contract.id
        );
    }

    let store_backed_schema_names = store_loads
        .iter()
        .map(|use_record| {
            semantic_artifacts
                .get(&use_record.artifact_id)
                .expect("store-backed semantic artifact")
                .logical_schema
                .name
                .clone()
        })
        .collect::<std::collections::BTreeSet<_>>();
    assert!(
        store_backed_schema_names.contains("support-summary"),
        "support summaries should remain store-backed query artifacts"
    );
    assert!(
        store_backed_schema_names.contains("capture-cache"),
        "capture caches should remain store-backed query artifacts"
    );
}

#[test]
fn query_exec_world_policy_is_reported_and_exact_oracle_is_rejected_on_wgsl() {
    let (_, _, ctx) = typed_query_module(query_fixture_source());
    let region_scene_id = stable_region_scene_capture_id(&SmolStr::new("scene_region"));
    let domain = scene_domain(region_scene_id, 1, true, true, true);
    let ray = ray_query_with_limits([0.0, 0.0, 3.0], [0.0, 0.0, -1.0], 6.0, 0.05, 0.001, 96);
    let policy = QueryExecutionPolicy::new(
        DispatchBackend::Cpu,
        RequiredGuaranteeClass::Exact,
        SelectedMethodClass::ExactOracle,
        Some(RayBudgetPolicy {
            max_distance: 6.0,
            min_step: 0.05,
            hit_epsilon: 0.001,
            max_steps: 96,
        }),
    );
    let plan = lower_world_query_plan(&WorldQueryPlan::for_query_with_backend(
        WorldQueryKind::Trace,
        DispatchBackend::Cpu,
    ));

    let (_hit, trace) = execute_world_query_with_policy_with_trace_on(
        &ctx,
        DispatchBackend::Cpu,
        &policy,
        None,
        &plan,
        &[
            KernelValue::Capture(SmolStr::new("scene_region")),
            domain,
            ray,
        ],
    )
    .expect("cpu exact/oracle world trace");
    let rendered = render_semantic_cost_report(&trace.cost_report);
    assert!(rendered.contains("execution_policy=backend_preference=cpu"));
    assert!(rendered.contains("required_guarantee=exact"));
    assert!(rendered.contains("selected_method=exact_oracle"));
    assert!(rendered.contains("degradations=none"));

    let conservative_wgsl_policy = QueryExecutionPolicy::conservative(
        DispatchBackend::Wgsl,
        Some(RayBudgetPolicy {
            max_distance: 6.0,
            min_step: 0.05,
            hit_epsilon: 0.001,
            max_steps: 96,
        }),
    );
    let (_wgsl_hit, wgsl_trace) = execute_world_query_with_policy_with_trace_on(
        &ctx,
        DispatchBackend::Wgsl,
        &conservative_wgsl_policy,
        None,
        &plan,
        &[
            KernelValue::Capture(SmolStr::new("scene_region")),
            scene_domain(region_scene_id, 1, true, true, true),
            ray_query_with_limits([0.0, 0.0, 3.0], [0.0, 0.0, -1.0], 6.0, 0.05, 0.001, 96),
        ],
    )
    .expect("wgsl conservative world trace");
    let wgsl_rendered = render_semantic_cost_report(&wgsl_trace.cost_report);
    assert!(wgsl_rendered.contains("execution_policy=backend_preference=wgsl"));
    assert!(
        wgsl_rendered.contains("degradations=backend=wgsl runs without the CPU legality oracle")
    );

    let wgsl_err = execute_world_query_with_policy_with_trace_on(
        &ctx,
        DispatchBackend::Wgsl,
        &QueryExecutionPolicy::new(
            DispatchBackend::Wgsl,
            RequiredGuaranteeClass::Exact,
            SelectedMethodClass::ExactOracle,
            Some(RayBudgetPolicy {
                max_distance: 6.0,
                min_step: 0.05,
                hit_epsilon: 0.001,
                max_steps: 96,
            }),
        ),
        None,
        &plan,
        &[
            KernelValue::Capture(SmolStr::new("scene_region")),
            scene_domain(region_scene_id, 1, true, true, true),
            ray_query_with_limits([0.0, 0.0, 3.0], [0.0, 0.0, -1.0], 6.0, 0.05, 0.001, 96),
        ],
    )
    .expect_err("wgsl exact/oracle policy should be rejected");
    let wgsl_err = wgsl_err.to_string();
    assert!(
        wgsl_err.contains("backend cannot satisfy execution policy"),
        "{wgsl_err}"
    );
    assert!(wgsl_err.contains("required_guarantee=exact"), "{wgsl_err}");
    assert!(
        wgsl_err.contains("selected_method=exact_oracle"),
        "{wgsl_err}"
    );
}

#[test]
fn query_exec_semantic_cost_reports_explain_support_domain_and_identity_causes() {
    let (_, _, support_ctx) = typed_query_module(world_support_cost_fixture_source());
    let support_region_scene_id = stable_region_scene_capture_id(&SmolStr::new("scene_region"));
    let support_domain = scene_domain(support_region_scene_id, 1, true, false, false);
    let support_trace_plan = lower_world_query_plan(&WorldQueryPlan::for_query_with_backend(
        WorldQueryKind::Trace,
        DispatchBackend::VirtualGpu,
    ));
    let (support_hit, _support_trace) = execute_world_query_with_trace_on(
        &support_ctx,
        DispatchBackend::VirtualGpu,
        &support_trace_plan,
        &[
            KernelValue::Capture(SmolStr::new("scene_region")),
            support_domain,
            ray_query_with_limits([0.0, 0.0, 3.0], [0.0, 0.0, -1.0], 6.0, 0.05, 0.001, 96),
        ],
    )
    .expect("support-pruned world trace");
    let support_surface_plan = lower_world_query_plan(&WorldQueryPlan::for_query_with_backend(
        WorldQueryKind::Surface,
        DispatchBackend::VirtualGpu,
    ));
    let (_support_surface, support_surface_trace) = execute_world_query_with_trace_on(
        &support_ctx,
        DispatchBackend::VirtualGpu,
        &support_surface_plan,
        &[
            KernelValue::Capture(SmolStr::new("scene_region")),
            scene_domain(support_region_scene_id, 1, true, false, false),
            support_hit,
        ],
    )
    .expect("support-pruned world surface");
    assert_eq!(
        support_surface_trace
            .observability
            .support_pruned_candidates,
        1
    );
    let support_rendered = render_semantic_cost_report(&support_surface_trace.cost_report);
    assert!(support_rendered.contains("scope=world:surface backend=virtual-gpu"));
    assert!(support_rendered.contains("artifacts=capture-cache"));
    assert!(support_rendered.contains("pruned=1"));
    assert!(
        support_surface_trace
            .cost_report
            .causes
            .iter()
            .any(|cause| { cause.kind == SemanticCostCauseKind::SupportTopology })
    );

    let (_, _, ctx) = typed_query_module(query_fixture_source());
    let region_scene_id = stable_region_scene_capture_id(&SmolStr::new("scene_region"));
    let fine_domain = scene_domain(region_scene_id, 1, true, true, true);
    let medium_plan = lower_world_query_plan(&WorldQueryPlan::for_query(WorldQueryKind::Medium));
    let (_medium, medium_trace) = execute_world_query_with_trace_on(
        &ctx,
        DispatchBackend::Cpu,
        &medium_plan,
        &[
            KernelValue::Capture(SmolStr::new("scene_region")),
            fine_domain,
            KernelValue::Vec3([0.0, 0.1, 0.75]),
        ],
    )
    .expect("world medium trace");
    assert!(
        medium_trace
            .cost_report
            .causes
            .iter()
            .any(|cause| { cause.kind == SemanticCostCauseKind::DomainGating })
    );
    assert!(
        medium_trace
            .cost_report
            .causes
            .iter()
            .any(|cause| { cause.kind == SemanticCostCauseKind::ParticipantAccumulation })
    );

    let (_, _, identity_ctx) = typed_query_module(direct_semantics_source());
    let trace_plan = lower_capture_query_plan(
        &CaptureQueryPlan::for_query(CaptureQueryKind::Trace, CaptureKind::Shape, None)
            .expect("trace plan"),
    );
    let (_identity_hit, identity_trace) = execute_capture_query_with_trace_on(
        &identity_ctx,
        DispatchBackend::Cpu,
        &trace_plan,
        &[
            KernelValue::Capture(SmolStr::new("identity_shape")),
            ray_query_with_limits([3.25, 0.0, 3.0], [0.0, 0.0, -1.0], 6.0, 0.05, 0.001, 96),
        ],
    )
    .expect("identity trace");
    assert!(
        identity_trace
            .cost_report
            .causes
            .iter()
            .any(|cause| { cause.kind == SemanticCostCauseKind::IdentityLocality })
    );
}

#[test]
fn query_exec_ray_solver_support_rejects_far_world_candidates() {
    let (_, _, ctx) = typed_query_module(world_ray_solver_support_fixture_source());
    let region_scene_id = stable_region_scene_capture_id(&SmolStr::new("scene_region"));
    let domain = scene_domain(region_scene_id, 1, true, false, false);
    let plan = lower_world_query_plan(&WorldQueryPlan::for_query_with_backend(
        WorldQueryKind::Trace,
        DispatchBackend::VirtualGpu,
    ));
    let (_hit, trace) = execute_world_query_with_trace_on(
        &ctx,
        DispatchBackend::VirtualGpu,
        &plan,
        &[
            KernelValue::Capture(SmolStr::new("scene_region")),
            domain,
            ray_query_with_limits([0.0, 0.0, 3.0], [0.0, 0.0, -1.0], 6.0, 0.05, 0.001, 96),
        ],
    )
    .expect("support-pruned solver trace");

    assert_eq!(trace.observability.candidate_count, 1);
    assert_eq!(trace.observability.support_pruned_candidates, 1);
    assert_eq!(trace.observability.solver_support_rejections, 1);
    assert_eq!(trace.observability.solver_dense_fallback_rays, 1);
    assert_eq!(trace.observability.solver_generated_dense_fallback_rays, 0);
    assert!(trace.observability.solver_plan_id.is_some());
    assert!(
        trace
            .observability
            .solver_methods
            .contains(&RaySolverMethod::SupportBoundCandidateRejection)
    );
    let rendered = render_semantic_cost_report(&trace.cost_report);
    assert!(rendered.contains("ray-solver"));
    assert!(rendered.contains("solver_support_rejections=1"));
    assert!(rendered.contains("solver_dense_fallback_rays=1"));
}

#[test]
fn query_exec_ray_solver_reports_specific_dense_fallback_reasons() {
    let (_, _, ctx) = typed_query_module(ray_solver_opaque_fixture_source());
    let region_scene_id = stable_region_scene_capture_id(&SmolStr::new("scene_region"));
    let domain = scene_domain(region_scene_id, 1, true, false, false);
    let plan = lower_world_query_plan(&WorldQueryPlan::for_query(WorldQueryKind::Nearest));
    let (_hit, trace) = execute_world_query_with_trace_on(
        &ctx,
        DispatchBackend::Cpu,
        &plan,
        &[
            KernelValue::Capture(SmolStr::new("scene_region")),
            domain,
            ray_query_with_limits([0.0, 0.0, 3.0], [0.0, 0.0, -1.0], 6.0, 0.05, 0.001, 96),
        ],
    )
    .expect("opaque solver trace");

    assert_eq!(trace.observability.solver_dense_fallback_rays, 1);
    assert_eq!(trace.observability.solver_fallback_contract_dense, 1);
    assert_eq!(trace.observability.solver_fallback_missing_facts, 1);
    assert_eq!(trace.observability.solver_fallback_analytic_unsupported, 1);
    assert_eq!(trace.observability.solver_analytic_hits, 0);
    let rendered = render_semantic_cost_report(&trace.cost_report);
    assert!(rendered.contains("solver_fallback_missing_facts=1"));
    assert!(rendered.contains("solver_fallback_analytic_unsupported=1"));
}

#[test]
fn query_exec_ray_solver_cpu_oracle_covers_analytic_dense_miss_and_provenance() {
    let (_, _, ctx) = typed_query_module(query_fixture_source());
    let shape_capture = KernelValue::Capture(SmolStr::new("scene_shape"));
    let region_capture = KernelValue::Capture(SmolStr::new("scene_region"));
    let region_scene_id = stable_region_scene_capture_id(&SmolStr::new("scene_region"));
    let domain = scene_domain(region_scene_id, 1, true, true, true);

    let capture_plan = lower_capture_query_plan(
        &CaptureQueryPlan::for_query(CaptureQueryKind::Trace, CaptureKind::Shape, None)
            .expect("capture trace plan"),
    );
    let world_plan = lower_world_query_plan(&WorldQueryPlan::for_query(WorldQueryKind::Nearest));
    assert!(world_plan.normalized_behavior.requires_trace());
    assert!(world_plan.normalized_behavior.requires_root_shape_lookup());
    assert_eq!(
        lower_world_query_plan(&WorldQueryPlan::for_query(WorldQueryKind::Nearest))
            .normalized_behavior,
        world_plan.normalized_behavior
    );
    let hit_ray = ray_query_with_limits([0.0, 0.0, 3.0], [0.0, 0.0, -1.0], 6.0, 0.05, 0.001, 96);
    let dense_oracle = execute_capture_query(
        &ctx,
        &capture_plan,
        &[shape_capture.clone(), hit_ray.clone()],
    )
    .expect("dense capture oracle");
    let (solver_hit, solver_trace) = execute_world_query_with_trace_on(
        &ctx,
        DispatchBackend::Cpu,
        &world_plan,
        &[region_capture.clone(), domain.clone(), hit_ray],
    )
    .expect("solver world hit");
    assert_hit3_approx_eq(&dense_oracle, &solver_hit);
    assert_eq!(solver_trace.observability.solver_analytic_hits, 1);
    assert_eq!(solver_trace.observability.solver_dense_fallback_rays, 0);
    assert!(solver_trace.observability.solver_subject.is_some());
    assert_ne!(
        solver_trace.observability.solver_subject.as_deref(),
        Some(world_plan.contract_id.as_str())
    );
    let hit_ref = expect_struct(&solver_hit, "Hit3");
    assert_eq!(expect_u32(field(hit_ref, "feature_id")), 1);
    assert_eq!(
        expect_u32(field(hit_ref, "root_shape_id")),
        stable_shape_capture_id(&SmolStr::new("scene_shape"))
    );
    assert_eq!(
        field(hit_ref, "payload"),
        field(expect_struct(&dense_oracle, "Hit3"), "payload")
    );

    let miss_ray = ray_query_with_limits([0.0, 0.0, 3.0], [0.0, 1.0, 0.0], 6.0, 0.05, 0.001, 96);
    let (miss, miss_trace) = execute_world_query_with_trace_on(
        &ctx,
        DispatchBackend::Cpu,
        &world_plan,
        &[region_capture, domain, miss_ray],
    )
    .expect("solver world miss");
    assert!(!expect_bool(field(expect_struct(&miss, "Hit3"), "hit")));
    assert_eq!(miss_trace.observability.solver_analytic_hits, 0);
    assert!(miss_trace.observability.solver_dense_fallback_rays > 0);
    assert!(render_semantic_cost_report(&miss_trace.cost_report).contains("ray-solver-fallback"));
}

#[test]
fn query_exec_explicit_wgsl_backend_matches_cpu_for_capture_and_world_queries() {
    let (_, _, ctx) = typed_query_module(query_fixture_source());
    let field_capture = KernelValue::Capture(SmolStr::new("sphere_field"));
    let shape_capture = KernelValue::Capture(SmolStr::new("scene_shape"));
    let region_capture = KernelValue::Capture(SmolStr::new("scene_region"));
    let region_scene_id = stable_region_scene_capture_id(&SmolStr::new("scene_region"));
    let fine_domain = scene_domain(region_scene_id, 1, true, true, true);

    let field_distance_plan = lower_capture_query_plan(
        &CaptureQueryPlan::for_query(CaptureQueryKind::Distance, CaptureKind::Field, None)
            .expect("field capture distance plan"),
    );
    let cpu_field_distance = execute_capture_query(
        &ctx,
        &field_distance_plan,
        &[field_capture.clone(), KernelValue::Vec3([0.0, 0.0, 2.0])],
    )
    .expect("cpu field capture distance");
    let (wgsl_field_distance, wgsl_field_distance_trace) = execute_capture_query_with_trace_on(
        &ctx,
        DispatchBackend::Wgsl,
        &field_distance_plan,
        &[field_capture.clone(), KernelValue::Vec3([0.0, 0.0, 2.0])],
    )
    .expect("wgsl field capture distance");
    assert_approx_eq(
        expect_f32(&cpu_field_distance),
        expect_f32(&wgsl_field_distance),
    );
    assert_eq!(wgsl_field_distance_trace.backend, DispatchBackend::Wgsl);
    assert_direct_trace_contract(
        &wgsl_field_distance_trace,
        query_contract::SPATIAL_DISTANCE_CAPTURE_FIELD,
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
    let wgsl_field_normal = execute_capture_query_on(
        &ctx,
        DispatchBackend::Wgsl,
        &field_normal_plan,
        &[
            KernelValue::Capture(SmolStr::new("sphere_field")),
            KernelValue::Vec3([0.0, 0.0, 2.0]),
        ],
    )
    .expect("wgsl field capture normal");
    assert_vec3_approx_eq(
        expect_vec3(&cpu_field_normal),
        expect_vec3(&wgsl_field_normal),
    );

    let capture_distance_plan = lower_capture_query_plan(
        &CaptureQueryPlan::for_query(CaptureQueryKind::Distance, CaptureKind::Shape, None)
            .expect("shape capture distance plan"),
    );
    let cpu_capture_distance = execute_capture_query(
        &ctx,
        &capture_distance_plan,
        &[shape_capture.clone(), KernelValue::Vec3([0.0, 0.0, 2.0])],
    )
    .expect("cpu shape capture distance");
    let wgsl_capture_distance = execute_capture_query_on(
        &ctx,
        DispatchBackend::Wgsl,
        &capture_distance_plan,
        &[shape_capture.clone(), KernelValue::Vec3([0.0, 0.0, 2.0])],
    )
    .expect("wgsl shape capture distance");
    assert_approx_eq(
        expect_f32(&cpu_capture_distance),
        expect_f32(&wgsl_capture_distance),
    );

    let capture_normal_plan = lower_capture_query_plan(
        &CaptureQueryPlan::for_query(CaptureQueryKind::Normal, CaptureKind::Shape, None)
            .expect("shape capture normal plan"),
    );
    let cpu_capture_normal = execute_capture_query(
        &ctx,
        &capture_normal_plan,
        &[shape_capture.clone(), KernelValue::Vec3([0.0, 0.0, 2.0])],
    )
    .expect("cpu shape capture normal");
    let wgsl_capture_normal = execute_capture_query_on(
        &ctx,
        DispatchBackend::Wgsl,
        &capture_normal_plan,
        &[shape_capture.clone(), KernelValue::Vec3([0.0, 0.0, 2.0])],
    )
    .expect("wgsl shape capture normal");
    assert_vec3_approx_eq(
        expect_vec3(&cpu_capture_normal),
        expect_vec3(&wgsl_capture_normal),
    );

    let capture_trace_plan = lower_capture_query_plan(
        &CaptureQueryPlan::for_query(CaptureQueryKind::Trace, CaptureKind::Shape, None)
            .expect("capture trace plan"),
    );
    let capture_trace_args = vec![
        shape_capture.clone(),
        ray_query_with_limits([0.0, 0.0, 3.0], [0.0, 0.0, -1.0], 6.0, 0.05, 0.001, 96),
    ];
    let cpu_capture_hit =
        execute_capture_query(&ctx, &capture_trace_plan, &capture_trace_args).expect("cpu trace");
    let (wgsl_capture_hit, wgsl_capture_trace) = execute_capture_query_with_trace_on(
        &ctx,
        DispatchBackend::Wgsl,
        &capture_trace_plan,
        &capture_trace_args,
    )
    .expect("wgsl trace");
    assert_hit3_approx_eq(&cpu_capture_hit, &wgsl_capture_hit);
    assert_eq!(wgsl_capture_trace.backend, DispatchBackend::Wgsl);
    assert_eq!(wgsl_capture_trace.executor, DirectQueryExecutor::Wgsl);
    assert_direct_trace_contract(
        &wgsl_capture_trace,
        query_contract::SPATIAL_TRACE_CAPTURE_SHAPE,
    );

    let capture_surface_plan = lower_capture_query_plan(
        &CaptureQueryPlan::for_query(CaptureQueryKind::Surface, CaptureKind::Shape, None)
            .expect("capture surface plan"),
    );
    let cpu_capture_surface = execute_capture_query(
        &ctx,
        &capture_surface_plan,
        &[shape_capture.clone(), cpu_capture_hit.clone()],
    )
    .expect("cpu capture surface");
    let wgsl_capture_surface = execute_capture_query_on(
        &ctx,
        DispatchBackend::Wgsl,
        &capture_surface_plan,
        &[shape_capture.clone(), wgsl_capture_hit.clone()],
    )
    .expect("wgsl capture surface");
    assert_surface_approx_eq(&cpu_capture_surface, &wgsl_capture_surface);

    let capture_radiance_plan = lower_capture_query_plan(
        &CaptureQueryPlan::for_query(CaptureQueryKind::Radiance, CaptureKind::Shape, None)
            .expect("capture radiance plan"),
    );
    let capture_radiance_args = vec![
        shape_capture.clone(),
        point_direction_query([0.0, 0.0, 1.0], [0.0, 0.0, -1.0]),
    ];
    let cpu_capture_radiance =
        execute_capture_query(&ctx, &capture_radiance_plan, &capture_radiance_args)
            .expect("cpu capture radiance");
    let wgsl_capture_radiance = execute_capture_query_on(
        &ctx,
        DispatchBackend::Wgsl,
        &capture_radiance_plan,
        &capture_radiance_args,
    )
    .expect("wgsl capture radiance");
    assert_vec3_approx_eq(
        expect_vec3(&cpu_capture_radiance),
        expect_vec3(&wgsl_capture_radiance),
    );

    let capture_medium_plan = lower_capture_query_plan(
        &CaptureQueryPlan::for_query(CaptureQueryKind::Medium, CaptureKind::Shape, None)
            .expect("capture medium plan"),
    );
    let capture_medium_args = vec![shape_capture.clone(), KernelValue::Vec3([0.0, 0.0, 1.0])];
    let cpu_capture_medium =
        execute_capture_query(&ctx, &capture_medium_plan, &capture_medium_args)
            .expect("cpu capture medium");
    let wgsl_capture_medium = execute_capture_query_on(
        &ctx,
        DispatchBackend::Wgsl,
        &capture_medium_plan,
        &capture_medium_args,
    )
    .expect("wgsl capture medium");
    assert_medium_approx_eq(&cpu_capture_medium, &wgsl_capture_medium);

    let world_distance_plan =
        lower_world_query_plan(&WorldQueryPlan::for_query(WorldQueryKind::Distance));
    let cpu_world_distance = execute_world_query(
        &ctx,
        &world_distance_plan,
        &[
            region_capture.clone(),
            fine_domain.clone(),
            KernelValue::Vec3([0.0, 0.0, 2.0]),
        ],
    )
    .expect("cpu world distance");
    let wgsl_world_distance = execute_world_query_on(
        &ctx,
        DispatchBackend::Wgsl,
        &world_distance_plan,
        &[
            region_capture.clone(),
            fine_domain.clone(),
            KernelValue::Vec3([0.0, 0.0, 2.0]),
        ],
    )
    .expect("wgsl world distance");
    assert_approx_eq(
        expect_f32(&cpu_world_distance),
        expect_f32(&wgsl_world_distance),
    );

    let world_normal_plan =
        lower_world_query_plan(&WorldQueryPlan::for_query(WorldQueryKind::Normal));
    let cpu_world_normal = execute_world_query(
        &ctx,
        &world_normal_plan,
        &[
            region_capture.clone(),
            fine_domain.clone(),
            KernelValue::Vec3([0.0, 0.0, 2.0]),
        ],
    )
    .expect("cpu world normal");
    let wgsl_world_normal = execute_world_query_on(
        &ctx,
        DispatchBackend::Wgsl,
        &world_normal_plan,
        &[
            region_capture.clone(),
            fine_domain.clone(),
            KernelValue::Vec3([0.0, 0.0, 2.0]),
        ],
    )
    .expect("wgsl world normal");
    assert_vec3_approx_eq(
        expect_vec3(&cpu_world_normal),
        expect_vec3(&wgsl_world_normal),
    );

    let world_trace_plan =
        lower_world_query_plan(&WorldQueryPlan::for_query(WorldQueryKind::Trace));
    let world_trace_args = vec![
        region_capture.clone(),
        fine_domain.clone(),
        ray_query_with_limits([0.0, 0.0, 3.0], [0.0, 0.0, -1.0], 6.0, 0.05, 0.001, 96),
    ];
    let cpu_world_hit =
        execute_world_query(&ctx, &world_trace_plan, &world_trace_args).expect("cpu world trace");
    let (wgsl_world_hit, wgsl_world_trace) = execute_world_query_with_trace_on(
        &ctx,
        DispatchBackend::Wgsl,
        &world_trace_plan,
        &world_trace_args,
    )
    .expect("wgsl world trace");
    assert_hit3_approx_eq(&cpu_world_hit, &wgsl_world_hit);
    assert_eq!(wgsl_world_trace.backend, DispatchBackend::Wgsl);
    assert_eq!(wgsl_world_trace.executor, DirectQueryExecutor::Wgsl);
    assert_direct_trace_contract(&wgsl_world_trace, query_contract::SPATIAL_TRACE_WORLD);

    let world_surface_plan =
        lower_world_query_plan(&WorldQueryPlan::for_query(WorldQueryKind::Surface));
    let cpu_world_surface = execute_world_query(
        &ctx,
        &world_surface_plan,
        &[
            region_capture.clone(),
            fine_domain.clone(),
            cpu_world_hit.clone(),
        ],
    )
    .expect("cpu world surface");
    let wgsl_world_surface = execute_world_query_on(
        &ctx,
        DispatchBackend::Wgsl,
        &world_surface_plan,
        &[
            region_capture.clone(),
            fine_domain.clone(),
            wgsl_world_hit.clone(),
        ],
    )
    .expect("wgsl world surface");
    assert_surface_approx_eq(&cpu_world_surface, &wgsl_world_surface);

    let world_radiance_plan =
        lower_world_query_plan(&WorldQueryPlan::for_query(WorldQueryKind::Radiance));
    let world_radiance_args = vec![
        region_capture.clone(),
        fine_domain.clone(),
        point_direction_query([0.0, 0.0, 1.0], [0.0, 0.0, -1.0]),
    ];
    let cpu_world_radiance = execute_world_query(&ctx, &world_radiance_plan, &world_radiance_args)
        .expect("cpu world radiance");
    let wgsl_world_radiance = execute_world_query_on(
        &ctx,
        DispatchBackend::Wgsl,
        &world_radiance_plan,
        &world_radiance_args,
    )
    .expect("wgsl world radiance");
    assert_vec3_approx_eq(
        expect_vec3(&cpu_world_radiance),
        expect_vec3(&wgsl_world_radiance),
    );

    let world_medium_plan =
        lower_world_query_plan(&WorldQueryPlan::for_query(WorldQueryKind::Medium));
    let world_medium_args = vec![
        region_capture,
        fine_domain,
        KernelValue::Vec3([0.0, 0.0, 1.0]),
    ];
    let cpu_world_medium = execute_world_query(&ctx, &world_medium_plan, &world_medium_args)
        .expect("cpu world medium");
    let wgsl_world_medium = execute_world_query_on(
        &ctx,
        DispatchBackend::Wgsl,
        &world_medium_plan,
        &world_medium_args,
    )
    .expect("wgsl world medium");
    assert_medium_approx_eq(&cpu_world_medium, &wgsl_world_medium);
}

#[test]
fn query_exec_scalar_occlusion_matches_cpu_virtual_gpu_and_wgsl_for_capture_and_world() {
    let (_, _, ctx) = typed_query_module(query_fixture_source());
    let shape_capture = KernelValue::Capture(SmolStr::new("scene_shape"));
    let region_capture = KernelValue::Capture(SmolStr::new("scene_region"));
    let fine_domain = scene_domain(
        stable_region_scene_capture_id(&SmolStr::new("scene_region")),
        1,
        true,
        true,
        true,
    );

    let capture_plan = lower_capture_query_plan(
        &CaptureQueryPlan::for_query(CaptureQueryKind::Occluded, CaptureKind::Shape, None)
            .expect("capture occluded plan"),
    );
    for (ray, expected_occluded) in [
        (
            ray_query_with_limits([0.0, 0.0, 3.0], [0.0, 0.0, -1.0], 6.0, 0.05, 0.001, 96),
            true,
        ),
        (
            ray_query_with_limits([0.0, 0.0, 3.0], [0.0, 1.0, 0.0], 6.0, 0.05, 0.001, 96),
            false,
        ),
    ] {
        let args = vec![shape_capture.clone(), ray];
        let (cpu, cpu_trace) =
            execute_capture_query_with_trace_on(&ctx, DispatchBackend::Cpu, &capture_plan, &args)
                .expect("cpu capture occlusion");
        let (vgpu, vgpu_trace) = execute_capture_query_with_trace_on(
            &ctx,
            DispatchBackend::VirtualGpu,
            &capture_plan,
            &args,
        )
        .expect("vgpu capture occlusion");
        let (wgsl, wgsl_trace) =
            execute_capture_query_with_trace_on(&ctx, DispatchBackend::Wgsl, &capture_plan, &args)
                .expect("wgsl capture occlusion");
        assert_occlusion_approx_eq(&cpu, &vgpu);
        assert_occlusion_approx_eq(&cpu, &wgsl);
        assert_eq!(
            expect_bool(field(expect_struct(&cpu, "OcclusionResult"), "occluded")),
            expected_occluded
        );
        assert_direct_trace_contract(&cpu_trace, query_contract::SPATIAL_OCCLUDED_CAPTURE_SHAPE);
        assert_direct_trace_contract(&vgpu_trace, query_contract::SPATIAL_OCCLUDED_CAPTURE_SHAPE);
        assert_direct_trace_contract(&wgsl_trace, query_contract::SPATIAL_OCCLUDED_CAPTURE_SHAPE);
        assert!(cpu_trace.observability.trace_steps > 0);
        assert!(vgpu_trace.observability.trace_steps > 0);
        if expected_occluded {
            assert!(wgsl_trace.observability.trace_steps > 0);
        } else {
            assert_eq!(wgsl_trace.observability.trace_steps, 0);
            assert_eq!(
                wgsl_trace.cost_report.fidelity,
                CostFidelity::StructuralApproximation
            );
        }
    }

    let world_plan = lower_world_query_plan(&WorldQueryPlan::for_query(WorldQueryKind::Occluded));
    for (ray, expected_occluded) in [
        (
            ray_query_with_limits([0.0, 0.0, 3.0], [0.0, 0.0, -1.0], 6.0, 0.05, 0.001, 96),
            true,
        ),
        (
            ray_query_with_limits([0.0, 0.0, 3.0], [0.0, 1.0, 0.0], 6.0, 0.05, 0.001, 96),
            false,
        ),
    ] {
        let args = vec![region_capture.clone(), fine_domain.clone(), ray];
        let (cpu, cpu_trace) =
            execute_world_query_with_trace_on(&ctx, DispatchBackend::Cpu, &world_plan, &args)
                .expect("cpu world occlusion");
        let (vgpu, vgpu_trace) = execute_world_query_with_trace_on(
            &ctx,
            DispatchBackend::VirtualGpu,
            &world_plan,
            &args,
        )
        .expect("vgpu world occlusion");
        let (wgsl, wgsl_trace) =
            execute_world_query_with_trace_on(&ctx, DispatchBackend::Wgsl, &world_plan, &args)
                .expect("wgsl world occlusion");
        assert_occlusion_approx_eq(&cpu, &vgpu);
        assert_occlusion_approx_eq(&cpu, &wgsl);
        assert_eq!(
            expect_bool(field(expect_struct(&cpu, "OcclusionResult"), "occluded")),
            expected_occluded
        );
        assert_direct_trace_contract(&cpu_trace, query_contract::SPATIAL_OCCLUDED_WORLD);
        assert_direct_trace_contract(&vgpu_trace, query_contract::SPATIAL_OCCLUDED_WORLD);
        assert_direct_trace_contract(&wgsl_trace, query_contract::SPATIAL_OCCLUDED_WORLD);
        assert!(cpu_trace.observability.trace_steps > 0);
        assert!(vgpu_trace.observability.trace_steps > 0);
        if expected_occluded {
            assert!(wgsl_trace.observability.trace_steps > 0);
        } else {
            assert_eq!(wgsl_trace.observability.trace_steps, 0);
            assert_eq!(
                wgsl_trace.cost_report.fidelity,
                CostFidelity::StructuralApproximation
            );
        }
    }
}

#[test]
fn query_exec_wgsl_matches_cpu_for_preview_project_render_sampling() {
    let (_, _, ctx) = typed_project_query_module("language/preview");
    let region_capture = KernelValue::Capture(SmolStr::new("scene_region"));
    let domain = scene_domain_with_limits(
        stable_region_scene_capture_id(&SmolStr::new("scene_region")),
        1,
        true,
        true,
        true,
        12.0,
        0.02,
        0.0008,
        96,
    );
    let distance_plan =
        lower_world_query_plan(&WorldQueryPlan::for_query(WorldQueryKind::Distance));
    let trace_plan = lower_world_query_plan(&WorldQueryPlan::for_query(WorldQueryKind::Trace));
    let surface_plan = lower_world_query_plan(&WorldQueryPlan::for_query(WorldQueryKind::Surface));
    let radiance_plan =
        lower_world_query_plan(&WorldQueryPlan::for_query(WorldQueryKind::Radiance));
    let medium_plan = lower_world_query_plan(&WorldQueryPlan::for_query(WorldQueryKind::Medium));

    let camera_position = [0.0, 0.1, 2.7];
    let camera_forward = normalize3([0.0, 0.0, -1.0]);
    let world_up = [0.0, 1.0, 0.0];
    let light_position = [2.4, 2.8, 2.4];
    let light_range = 12.0;
    let sample_xs = [0usize, 6, 12, 18, 24, 30, 36, 39];
    let sample_ys = [0usize, 8, 16, 24, 31, 32, 33, 39];

    for y in sample_ys {
        for x in sample_xs {
            let ray = preview_ray(x, y, 40, 40, camera_forward, world_up, 0.72);
            let trace_args = vec![
                region_capture.clone(),
                domain.clone(),
                ray_query_with_limits(camera_position, ray, 12.0, 0.02, 0.0008, 96),
            ];
            let cpu_hit =
                execute_world_query(&ctx, &trace_plan, &trace_args).expect("cpu preview trace");
            let wgsl_hit =
                execute_world_query_on(&ctx, DispatchBackend::Wgsl, &trace_plan, &trace_args)
                    .expect("wgsl preview trace");

            let cpu_hit_ref = expect_struct(&cpu_hit, "Hit3");
            let wgsl_hit_ref = expect_struct(&wgsl_hit, "Hit3");
            assert_eq!(
                expect_bool(field(cpu_hit_ref, "hit")),
                expect_bool(field(wgsl_hit_ref, "hit")),
                "hit flag mismatch at pixel ({x}, {y})"
            );
            assert_approx_eq_at(
                expect_f32(field(cpu_hit_ref, "distance")),
                expect_f32(field(wgsl_hit_ref, "distance")),
                "hit.distance",
                x,
                y,
            );
            assert_vec3_approx_eq_at(
                expect_vec3(field(cpu_hit_ref, "position")),
                expect_vec3(field(wgsl_hit_ref, "position")),
                "hit.position",
                x,
                y,
            );
            assert_vec3_approx_eq_at(
                expect_vec3(field(cpu_hit_ref, "normal")),
                expect_vec3(field(wgsl_hit_ref, "normal")),
                "hit.normal",
                x,
                y,
            );
            assert_eq!(
                expect_u32(field(cpu_hit_ref, "feature_id")),
                expect_u32(field(wgsl_hit_ref, "feature_id")),
                "feature_id mismatch at pixel ({x}, {y})"
            );
            assert_eq!(
                expect_u32(field(cpu_hit_ref, "root_shape_id")),
                expect_u32(field(wgsl_hit_ref, "root_shape_id")),
                "root_shape_id mismatch at pixel ({x}, {y})"
            );

            if !expect_bool(field(cpu_hit_ref, "hit")) {
                let miss_point = add3(camera_position, mul3(ray, 4.0));
                let radiance_args = vec![
                    region_capture.clone(),
                    domain.clone(),
                    point_direction_query(miss_point, ray),
                ];
                let medium_args = vec![
                    region_capture.clone(),
                    domain.clone(),
                    KernelValue::Vec3(miss_point),
                ];
                let cpu_radiance = execute_world_query(&ctx, &radiance_plan, &radiance_args)
                    .expect("cpu miss radiance");
                let wgsl_radiance = execute_world_query_on(
                    &ctx,
                    DispatchBackend::Wgsl,
                    &radiance_plan,
                    &radiance_args,
                )
                .expect("wgsl miss radiance");
                assert_vec3_approx_eq_at(
                    expect_vec3(&cpu_radiance),
                    expect_vec3(&wgsl_radiance),
                    "miss.radiance",
                    x,
                    y,
                );

                let cpu_medium =
                    execute_world_query(&ctx, &medium_plan, &medium_args).expect("cpu miss medium");
                let wgsl_medium =
                    execute_world_query_on(&ctx, DispatchBackend::Wgsl, &medium_plan, &medium_args)
                        .expect("wgsl miss medium");
                let cpu_medium_ref = expect_struct(&cpu_medium, "Medium");
                let wgsl_medium_ref = expect_struct(&wgsl_medium, "Medium");
                assert_approx_eq_at(
                    expect_f32(field(cpu_medium_ref, "density")),
                    expect_f32(field(wgsl_medium_ref, "density")),
                    "miss.medium.density",
                    x,
                    y,
                );
                assert_vec3_approx_eq_at(
                    expect_vec3(field(cpu_medium_ref, "emission")),
                    expect_vec3(field(wgsl_medium_ref, "emission")),
                    "miss.medium.emission",
                    x,
                    y,
                );
                continue;
            }

            let hit_position = expect_vec3(field(cpu_hit_ref, "position"));
            let hit_normal = expect_vec3(field(cpu_hit_ref, "normal"));
            let cpu_surface = execute_world_query(
                &ctx,
                &surface_plan,
                &[region_capture.clone(), domain.clone(), cpu_hit.clone()],
            )
            .expect("cpu preview surface");
            let wgsl_surface = execute_world_query_on(
                &ctx,
                DispatchBackend::Wgsl,
                &surface_plan,
                &[region_capture.clone(), domain.clone(), wgsl_hit.clone()],
            )
            .expect("wgsl preview surface");
            let cpu_surface_ref = expect_struct(&cpu_surface, "Surface");
            let wgsl_surface_ref = expect_struct(&wgsl_surface, "Surface");
            assert_vec3_approx_eq_at(
                expect_vec3(field(cpu_surface_ref, "albedo")),
                expect_vec3(field(wgsl_surface_ref, "albedo")),
                "surface.albedo",
                x,
                y,
            );
            assert_approx_eq_at(
                expect_f32(field(cpu_surface_ref, "roughness")),
                expect_f32(field(wgsl_surface_ref, "roughness")),
                "surface.roughness",
                x,
                y,
            );
            assert_approx_eq_at(
                expect_f32(field(cpu_surface_ref, "metalness")),
                expect_f32(field(wgsl_surface_ref, "metalness")),
                "surface.metalness",
                x,
                y,
            );
            assert_vec3_approx_eq_at(
                expect_vec3(field(cpu_surface_ref, "emissive")),
                expect_vec3(field(wgsl_surface_ref, "emissive")),
                "surface.emissive",
                x,
                y,
            );

            let radiance_args = vec![
                region_capture.clone(),
                domain.clone(),
                point_direction_query(hit_position, ray),
            ];
            let cpu_radiance =
                execute_world_query(&ctx, &radiance_plan, &radiance_args).expect("cpu radiance");
            let wgsl_radiance =
                execute_world_query_on(&ctx, DispatchBackend::Wgsl, &radiance_plan, &radiance_args)
                    .expect("wgsl radiance");
            assert_vec3_approx_eq_at(
                expect_vec3(&cpu_radiance),
                expect_vec3(&wgsl_radiance),
                "radiance",
                x,
                y,
            );

            let medium_args = vec![
                region_capture.clone(),
                domain.clone(),
                KernelValue::Vec3(hit_position),
            ];
            let cpu_medium =
                execute_world_query(&ctx, &medium_plan, &medium_args).expect("cpu medium");
            let wgsl_medium =
                execute_world_query_on(&ctx, DispatchBackend::Wgsl, &medium_plan, &medium_args)
                    .expect("wgsl medium");
            let cpu_medium_ref = expect_struct(&cpu_medium, "Medium");
            let wgsl_medium_ref = expect_struct(&wgsl_medium, "Medium");
            assert_approx_eq_at(
                expect_f32(field(cpu_medium_ref, "density")),
                expect_f32(field(wgsl_medium_ref, "density")),
                "medium.density",
                x,
                y,
            );
            assert_vec3_approx_eq_at(
                expect_vec3(field(cpu_medium_ref, "emission")),
                expect_vec3(field(wgsl_medium_ref, "emission")),
                "medium.emission",
                x,
                y,
            );

            for offset in [0.06f32, 0.14, 0.28] {
                let sample_point = add3(hit_position, mul3(hit_normal, offset));
                let distance_args = vec![
                    region_capture.clone(),
                    domain.clone(),
                    KernelValue::Vec3(sample_point),
                ];
                let cpu_distance = execute_world_query(&ctx, &distance_plan, &distance_args)
                    .expect("cpu ao distance");
                let wgsl_distance = execute_world_query_on(
                    &ctx,
                    DispatchBackend::Wgsl,
                    &distance_plan,
                    &distance_args,
                )
                .expect("wgsl ao distance");
                assert_approx_eq_at(
                    expect_f32(&cpu_distance),
                    expect_f32(&wgsl_distance),
                    &format!("ao.distance@{offset:.2}"),
                    x,
                    y,
                );
            }

            let shadow_origin = add3(hit_position, mul3(hit_normal, 0.01));
            let light_delta = sub3(light_position, shadow_origin);
            let shadow_direction = normalize3(light_delta);
            let shadow_limit = length3(light_delta).min(light_range);
            let shadow_args = vec![
                region_capture.clone(),
                domain.clone(),
                ray_query_with_limits(
                    shadow_origin,
                    shadow_direction,
                    shadow_limit,
                    0.02,
                    0.0008,
                    96,
                ),
            ];
            let cpu_shadow =
                execute_world_query(&ctx, &trace_plan, &shadow_args).expect("cpu shadow trace");
            let wgsl_shadow =
                execute_world_query_on(&ctx, DispatchBackend::Wgsl, &trace_plan, &shadow_args)
                    .expect("wgsl shadow trace");
            let cpu_shadow_ref = expect_struct(&cpu_shadow, "Hit3");
            let wgsl_shadow_ref = expect_struct(&wgsl_shadow, "Hit3");
            assert_eq!(
                expect_bool(field(cpu_shadow_ref, "hit")),
                expect_bool(field(wgsl_shadow_ref, "hit")),
                "shadow hit flag mismatch at pixel ({x}, {y})"
            );
            assert_approx_eq_at(
                expect_f32(field(cpu_shadow_ref, "distance")),
                expect_f32(field(wgsl_shadow_ref, "distance")),
                "shadow.distance",
                x,
                y,
            );
        }
    }
}

#[test]
fn query_exec_wgsl_matches_cpu_for_preview_probe_b_world_and_scene_medium() {
    let (_, _, ctx) = typed_project_query_module("language/preview");
    let region_capture = KernelValue::Capture(SmolStr::new("scene_region"));
    let domain = scene_domain_with_limits(
        stable_region_scene_capture_id(&SmolStr::new("scene_region")),
        1,
        true,
        true,
        true,
        12.0,
        0.02,
        0.0008,
        96,
    );
    let trace_plan = lower_world_query_plan(&WorldQueryPlan::for_query(WorldQueryKind::Trace));
    let medium_plan = lower_world_query_plan(&WorldQueryPlan::for_query(WorldQueryKind::Medium));
    let capture_medium_plan = lower_capture_query_plan(
        &CaptureQueryPlan::for_query(CaptureQueryKind::Medium, CaptureKind::Shape, None)
            .expect("shape capture medium plan"),
    );

    let origin = [0.0, 0.1, 2.7];
    let direction = [-0.379642, -0.379642, -0.843649];
    let trace_args = vec![
        region_capture.clone(),
        domain.clone(),
        ray_query_with_limits(origin, direction, 12.0, 0.02, 0.0008, 96),
    ];
    let cpu_hit = execute_world_query(&ctx, &trace_plan, &trace_args).expect("cpu probe_b trace");
    let wgsl_hit = execute_world_query_on(&ctx, DispatchBackend::Wgsl, &trace_plan, &trace_args)
        .expect("wgsl probe_b trace");

    let cpu_hit_ref = expect_struct(&cpu_hit, "Hit3");
    let wgsl_hit_ref = expect_struct(&wgsl_hit, "Hit3");
    assert_vec3_approx_eq(
        expect_vec3(field(cpu_hit_ref, "position")),
        expect_vec3(field(wgsl_hit_ref, "position")),
    );

    let hit_position = expect_vec3(field(cpu_hit_ref, "position"));
    let medium_args = vec![region_capture, domain, KernelValue::Vec3(hit_position)];
    let cpu_medium =
        execute_world_query(&ctx, &medium_plan, &medium_args).expect("cpu probe_b medium");
    let wgsl_medium =
        execute_world_query_on(&ctx, DispatchBackend::Wgsl, &medium_plan, &medium_args)
            .expect("wgsl probe_b medium");

    assert_medium_approx_eq(&cpu_medium, &wgsl_medium);

    let scene_medium_args = vec![
        KernelValue::Capture(SmolStr::new("scene_shape")),
        KernelValue::Vec3(hit_position),
    ];
    let cpu_scene_medium = execute_capture_query(&ctx, &capture_medium_plan, &scene_medium_args)
        .expect("cpu probe_b scene medium");
    let wgsl_scene_medium = execute_capture_query_on(
        &ctx,
        DispatchBackend::Wgsl,
        &capture_medium_plan,
        &scene_medium_args,
    )
    .expect("wgsl probe_b scene medium");
    assert_medium_approx_eq(&cpu_scene_medium, &wgsl_scene_medium);

    let foot_medium_args = vec![
        KernelValue::Capture(SmolStr::new("foot_shape")),
        KernelValue::Vec3(hit_position),
    ];
    let cpu_foot_medium = execute_capture_query(&ctx, &capture_medium_plan, &foot_medium_args)
        .expect("cpu probe_b foot medium");
    let wgsl_foot_medium = execute_capture_query_on(
        &ctx,
        DispatchBackend::Wgsl,
        &capture_medium_plan,
        &foot_medium_args,
    )
    .expect("wgsl probe_b foot medium");
    assert_medium_approx_eq(&cpu_foot_medium, &wgsl_foot_medium);
}

#[test]
fn query_exec_wgsl_batch_queries_match_cpu_results() {
    let (_, _, ctx) = typed_query_module(query_fixture_source());
    let shape_capture = KernelValue::Capture(SmolStr::new("scene_shape"));
    let field_capture = KernelValue::Capture(SmolStr::new("sphere_field"));
    let region_capture = KernelValue::Capture(SmolStr::new("scene_region"));
    let fine_domain = scene_domain(
        stable_region_scene_capture_id(&SmolStr::new("scene_region")),
        1,
        true,
        true,
        true,
    );
    let point_items = KernelValue::Array(vec![
        point_query([0.0, 0.0, 2.0]),
        point_query([0.0, 0.0, 3.0]),
    ]);
    let ray_items = KernelValue::Array(vec![
        ray_query([0.0, 0.0, 3.0], [0.0, 0.0, -1.0]),
        ray_query([0.0, 0.0, 3.0], [0.0, 1.0, 0.0]),
    ]);

    let cpu_field_distance = lower_batch_query_plan(
        &BatchQueryPlan::for_contract(
            query_contract::SPATIAL_DISTANCE_BATCH_FIELD,
            DispatchBackend::Cpu,
            None,
        )
        .unwrap(),
    );
    let wgsl_field_distance = lower_batch_query_plan(
        &BatchQueryPlan::for_contract(
            query_contract::SPATIAL_DISTANCE_BATCH_FIELD,
            DispatchBackend::Wgsl,
            None,
        )
        .unwrap(),
    );
    let (cpu_field_distances, _) = execute_batch_query_with_trace(
        &ctx,
        &cpu_field_distance,
        &[field_capture.clone(), point_items.clone()],
    )
    .expect("cpu field distance batch");
    let (wgsl_field_distances, wgsl_field_distance_trace) = execute_batch_query_with_trace(
        &ctx,
        &wgsl_field_distance,
        &[field_capture.clone(), point_items.clone()],
    )
    .expect("wgsl field distance batch");
    for (cpu, wgsl) in expect_array(&cpu_field_distances)
        .iter()
        .zip(expect_array(&wgsl_field_distances))
    {
        assert_approx_eq(
            expect_f32(field(expect_struct(cpu, "DistanceResult"), "distance")),
            expect_f32(field(expect_struct(wgsl, "DistanceResult"), "distance")),
        );
    }
    assert_eq!(wgsl_field_distance_trace.backend, DispatchBackend::Wgsl);
    assert_batch_trace_contract(
        &wgsl_field_distance_trace,
        query_contract::SPATIAL_DISTANCE_BATCH_FIELD,
    );

    let cpu_shape_distance = lower_batch_query_plan(
        &BatchQueryPlan::for_contract(
            query_contract::SPATIAL_DISTANCE_BATCH_SHAPE,
            DispatchBackend::Cpu,
            None,
        )
        .unwrap(),
    );
    let wgsl_shape_distance = lower_batch_query_plan(
        &BatchQueryPlan::for_contract(
            query_contract::SPATIAL_DISTANCE_BATCH_SHAPE,
            DispatchBackend::Wgsl,
            None,
        )
        .unwrap(),
    );
    let (cpu_shape_distances, _) = execute_batch_query_with_trace(
        &ctx,
        &cpu_shape_distance,
        &[shape_capture.clone(), point_items.clone()],
    )
    .expect("cpu shape distance batch");
    let (wgsl_shape_distances, wgsl_shape_distance_trace) = execute_batch_query_with_trace(
        &ctx,
        &wgsl_shape_distance,
        &[shape_capture.clone(), point_items.clone()],
    )
    .expect("wgsl shape distance batch");
    for (cpu, wgsl) in expect_array(&cpu_shape_distances)
        .iter()
        .zip(expect_array(&wgsl_shape_distances))
    {
        assert_approx_eq(
            expect_f32(field(expect_struct(cpu, "DistanceResult"), "distance")),
            expect_f32(field(expect_struct(wgsl, "DistanceResult"), "distance")),
        );
    }
    assert_batch_trace_contract(
        &wgsl_shape_distance_trace,
        query_contract::SPATIAL_DISTANCE_BATCH_SHAPE,
    );

    let cpu_field_normal = lower_batch_query_plan(
        &BatchQueryPlan::for_contract(
            query_contract::SPATIAL_NORMAL_BATCH_FIELD,
            DispatchBackend::Cpu,
            None,
        )
        .unwrap(),
    );
    let wgsl_field_normal = lower_batch_query_plan(
        &BatchQueryPlan::for_contract(
            query_contract::SPATIAL_NORMAL_BATCH_FIELD,
            DispatchBackend::Wgsl,
            None,
        )
        .unwrap(),
    );
    let (cpu_field_normals, _) = execute_batch_query_with_trace(
        &ctx,
        &cpu_field_normal,
        &[field_capture, point_items.clone()],
    )
    .expect("cpu field normal batch");
    let (wgsl_field_normals, wgsl_field_normal_trace) = execute_batch_query_with_trace(
        &ctx,
        &wgsl_field_normal,
        &[
            KernelValue::Capture(SmolStr::new("sphere_field")),
            point_items.clone(),
        ],
    )
    .expect("wgsl field normal batch");
    for (cpu, wgsl) in expect_array(&cpu_field_normals)
        .iter()
        .zip(expect_array(&wgsl_field_normals))
    {
        assert_vec3_approx_eq(
            expect_vec3(field(expect_struct(cpu, "NormalResult"), "normal")),
            expect_vec3(field(expect_struct(wgsl, "NormalResult"), "normal")),
        );
    }
    assert_batch_normal_role(
        &wgsl_field_normal_trace,
        "normal_role::certified_field_gradient",
    );
    assert_batch_trace_contract(
        &wgsl_field_normal_trace,
        query_contract::SPATIAL_NORMAL_BATCH_FIELD,
    );

    let cpu_shape_normal = lower_batch_query_plan(
        &BatchQueryPlan::for_contract(
            query_contract::SPATIAL_NORMAL_BATCH_SHAPE,
            DispatchBackend::Cpu,
            None,
        )
        .unwrap(),
    );
    let wgsl_shape_normal = lower_batch_query_plan(
        &BatchQueryPlan::for_contract(
            query_contract::SPATIAL_NORMAL_BATCH_SHAPE,
            DispatchBackend::Wgsl,
            None,
        )
        .unwrap(),
    );
    let (cpu_shape_normals, _) = execute_batch_query_with_trace(
        &ctx,
        &cpu_shape_normal,
        &[shape_capture.clone(), point_items.clone()],
    )
    .expect("cpu shape normal batch");
    let (wgsl_shape_normals, wgsl_shape_normal_trace) = execute_batch_query_with_trace(
        &ctx,
        &wgsl_shape_normal,
        &[shape_capture.clone(), point_items.clone()],
    )
    .expect("wgsl shape normal batch");
    for (cpu, wgsl) in expect_array(&cpu_shape_normals)
        .iter()
        .zip(expect_array(&wgsl_shape_normals))
    {
        assert_vec3_approx_eq(
            expect_vec3(field(expect_struct(cpu, "NormalResult"), "normal")),
            expect_vec3(field(expect_struct(wgsl, "NormalResult"), "normal")),
        );
    }
    assert_batch_normal_role(&wgsl_shape_normal_trace, "normal_role::feature_normal");
    assert_batch_trace_contract(
        &wgsl_shape_normal_trace,
        query_contract::SPATIAL_NORMAL_BATCH_SHAPE,
    );

    let cpu_trace_plan = lower_batch_query_plan(
        &BatchQueryPlan::for_contract(
            query_contract::SPATIAL_TRACE_BATCH_SHAPE,
            DispatchBackend::Cpu,
            None,
        )
        .unwrap(),
    );
    let wgsl_trace_plan = lower_batch_query_plan(
        &BatchQueryPlan::for_contract(
            query_contract::SPATIAL_TRACE_BATCH_SHAPE,
            DispatchBackend::Wgsl,
            None,
        )
        .unwrap(),
    );
    let (cpu_hits, _) = execute_batch_query_with_trace(
        &ctx,
        &cpu_trace_plan,
        &[shape_capture.clone(), ray_items.clone()],
    )
    .expect("cpu trace batch");
    let (wgsl_hits, wgsl_trace_batch) = execute_batch_query_with_trace(
        &ctx,
        &wgsl_trace_plan,
        &[shape_capture.clone(), ray_items.clone()],
    )
    .expect("wgsl trace batch");
    for (cpu, wgsl) in expect_array(&cpu_hits).iter().zip(expect_array(&wgsl_hits)) {
        assert_hit3_approx_eq(cpu, wgsl);
    }
    assert_eq!(wgsl_trace_batch.backend, DispatchBackend::Wgsl);
    assert_batch_trace_contract(&wgsl_trace_batch, query_contract::SPATIAL_TRACE_BATCH_SHAPE);

    let cpu_surface_plan = lower_batch_query_plan(
        &BatchQueryPlan::for_contract(
            query_contract::SURFACE_SAMPLE_BATCH_SHAPE,
            DispatchBackend::Cpu,
            None,
        )
        .unwrap(),
    );
    let wgsl_surface_plan = lower_batch_query_plan(
        &BatchQueryPlan::for_contract(
            query_contract::SURFACE_SAMPLE_BATCH_SHAPE,
            DispatchBackend::Wgsl,
            None,
        )
        .unwrap(),
    );
    let (cpu_surfaces, _) = execute_batch_query_with_trace(
        &ctx,
        &cpu_surface_plan,
        &[shape_capture.clone(), cpu_hits.clone()],
    )
    .expect("cpu surface batch");
    let (wgsl_surfaces, wgsl_surface_trace) = execute_batch_query_with_trace(
        &ctx,
        &wgsl_surface_plan,
        &[shape_capture.clone(), wgsl_hits.clone()],
    )
    .expect("wgsl surface batch");
    for (cpu, wgsl) in expect_array(&cpu_surfaces)
        .iter()
        .zip(expect_array(&wgsl_surfaces))
    {
        assert_surface_approx_eq(cpu, wgsl);
    }
    assert_batch_trace_contract(
        &wgsl_surface_trace,
        query_contract::SURFACE_SAMPLE_BATCH_SHAPE,
    );

    let cpu_occluded_plan = lower_batch_query_plan(
        &BatchQueryPlan::for_contract(
            query_contract::SPATIAL_OCCLUDED_BATCH_SHAPE,
            DispatchBackend::Cpu,
            None,
        )
        .unwrap(),
    );
    let wgsl_occluded_plan = lower_batch_query_plan(
        &BatchQueryPlan::for_contract(
            query_contract::SPATIAL_OCCLUDED_BATCH_SHAPE,
            DispatchBackend::Wgsl,
            None,
        )
        .unwrap(),
    );
    let (cpu_occlusions, _) =
        execute_batch_query_with_trace(&ctx, &cpu_occluded_plan, &[shape_capture, ray_items])
            .expect("cpu occluded batch");
    let (wgsl_occlusions, wgsl_occluded_trace) = execute_batch_query_with_trace(
        &ctx,
        &wgsl_occluded_plan,
        &[
            KernelValue::Capture(SmolStr::new("scene_shape")),
            KernelValue::Array(vec![
                ray_query([0.0, 0.0, 3.0], [0.0, 0.0, -1.0]),
                ray_query([0.0, 0.0, 3.0], [0.0, 1.0, 0.0]),
            ]),
        ],
    )
    .expect("wgsl occluded batch");
    for (cpu, wgsl) in expect_array(&cpu_occlusions)
        .iter()
        .zip(expect_array(&wgsl_occlusions))
    {
        let cpu = expect_struct(cpu, "OcclusionResult");
        let wgsl = expect_struct(wgsl, "OcclusionResult");
        assert_eq!(
            expect_bool(field(cpu, "occluded")),
            expect_bool(field(wgsl, "occluded"))
        );
        assert_approx_eq(
            expect_f32(field(cpu, "distance")),
            expect_f32(field(wgsl, "distance")),
        );
    }
    assert_batch_trace_contract(
        &wgsl_occluded_trace,
        query_contract::SPATIAL_OCCLUDED_BATCH_SHAPE,
    );

    let world_nearest_cpu = lower_batch_query_plan(
        &BatchQueryPlan::for_contract(
            query_contract::SPATIAL_NEAREST_BATCH_WORLD,
            DispatchBackend::Cpu,
            None,
        )
        .unwrap(),
    );
    let world_nearest_wgsl = lower_batch_query_plan(
        &BatchQueryPlan::for_contract(
            query_contract::SPATIAL_NEAREST_BATCH_WORLD,
            DispatchBackend::Wgsl,
            None,
        )
        .unwrap(),
    );
    let world_ray_items = KernelValue::Array(vec![
        ray_query([0.0, 0.0, 3.0], [0.0, 0.0, -1.0]),
        ray_query([0.0, 0.0, 3.0], [0.0, 1.0, 0.0]),
    ]);
    let (cpu_world_hits, _) = execute_batch_query_with_trace(
        &ctx,
        &world_nearest_cpu,
        &[
            region_capture.clone(),
            fine_domain.clone(),
            world_ray_items.clone(),
        ],
    )
    .expect("cpu world nearest batch");
    let (wgsl_world_hits, wgsl_world_nearest_trace) = execute_batch_query_with_trace(
        &ctx,
        &world_nearest_wgsl,
        &[
            region_capture.clone(),
            fine_domain.clone(),
            world_ray_items.clone(),
        ],
    )
    .expect("wgsl world nearest batch");
    for (cpu, wgsl) in expect_array(&cpu_world_hits)
        .iter()
        .zip(expect_array(&wgsl_world_hits))
    {
        assert_hit3_approx_eq(cpu, wgsl);
    }
    assert_batch_trace_contract(
        &wgsl_world_nearest_trace,
        query_contract::SPATIAL_NEAREST_BATCH_WORLD,
    );
    assert_eq!(
        wgsl_world_nearest_trace
            .observability
            .world_batch_item_count,
        2
    );
    assert_eq!(
        wgsl_world_nearest_trace.observability.screen_sample_count,
        2
    );
    assert_eq!(wgsl_world_nearest_trace.observability.dispatch_items, 2);
    assert!(
        wgsl_world_nearest_trace
            .observability
            .candidates_before_pruning
            >= 2
    );
    assert!(
        wgsl_world_nearest_trace
            .observability
            .candidates_after_pruning
            >= 2
    );
    assert!(wgsl_world_nearest_trace.observability.trace_steps_max > 0);
    assert_eq!(
        wgsl_world_nearest_trace.observability.hit_count
            + wgsl_world_nearest_trace.observability.miss_count,
        2
    );
    assert_eq!(
        wgsl_world_nearest_trace
            .observability
            .semantic_pruned_batches,
        1
    );
    assert_eq!(
        wgsl_world_nearest_trace
            .observability
            .solver_generated_dense_fallback_rays,
        2
    );
    let rendered_cost = render_semantic_cost_report(&wgsl_world_nearest_trace.cost_report);
    assert!(rendered_cost.contains("world_batch_items=2"));
    assert!(rendered_cost.contains("dispatch_items=2"));
    assert!(rendered_cost.contains("trace_steps_avg="));
    assert!(rendered_cost.contains("semantic_pruned_batches=1"));
    assert!(rendered_cost.contains("solver_generated_dense_fallback_rays=2"));

    let world_occluded_cpu = lower_batch_query_plan(
        &BatchQueryPlan::for_contract(
            query_contract::SPATIAL_OCCLUDED_BATCH_WORLD,
            DispatchBackend::Cpu,
            None,
        )
        .unwrap(),
    );
    let world_occluded_wgsl = lower_batch_query_plan(
        &BatchQueryPlan::for_contract(
            query_contract::SPATIAL_OCCLUDED_BATCH_WORLD,
            DispatchBackend::Wgsl,
            None,
        )
        .unwrap(),
    );
    let (cpu_world_occlusions, _) = execute_batch_query_with_trace(
        &ctx,
        &world_occluded_cpu,
        &[
            region_capture.clone(),
            fine_domain.clone(),
            world_ray_items.clone(),
        ],
    )
    .expect("cpu world occluded batch");
    let (wgsl_world_occlusions, wgsl_world_occluded_trace) = execute_batch_query_with_trace(
        &ctx,
        &world_occluded_wgsl,
        &[
            region_capture.clone(),
            fine_domain.clone(),
            world_ray_items.clone(),
        ],
    )
    .expect("wgsl world occluded batch");
    for (cpu, wgsl) in expect_array(&cpu_world_occlusions)
        .iter()
        .zip(expect_array(&wgsl_world_occlusions))
    {
        let cpu = expect_struct(cpu, "OcclusionResult");
        let wgsl = expect_struct(wgsl, "OcclusionResult");
        assert_eq!(
            expect_bool(field(cpu, "occluded")),
            expect_bool(field(wgsl, "occluded"))
        );
        assert_approx_eq(
            expect_f32(field(cpu, "distance")),
            expect_f32(field(wgsl, "distance")),
        );
    }
    assert_batch_trace_contract(
        &wgsl_world_occluded_trace,
        query_contract::SPATIAL_OCCLUDED_BATCH_WORLD,
    );
    assert_eq!(
        wgsl_world_occluded_trace
            .observability
            .solver_generated_dense_fallback_rays,
        2
    );

    let world_surface_cpu = lower_batch_query_plan(
        &BatchQueryPlan::for_contract(
            query_contract::SURFACE_SAMPLE_BATCH_WORLD,
            DispatchBackend::Cpu,
            None,
        )
        .unwrap(),
    );
    let world_surface_wgsl = lower_batch_query_plan(
        &BatchQueryPlan::for_contract(
            query_contract::SURFACE_SAMPLE_BATCH_WORLD,
            DispatchBackend::Wgsl,
            None,
        )
        .unwrap(),
    );
    let (cpu_world_surfaces, _) = execute_batch_query_with_trace(
        &ctx,
        &world_surface_cpu,
        &[
            region_capture.clone(),
            fine_domain.clone(),
            cpu_world_hits.clone(),
        ],
    )
    .expect("cpu world surface batch");
    let (wgsl_world_surfaces, wgsl_world_surface_trace) = execute_batch_query_with_trace(
        &ctx,
        &world_surface_wgsl,
        &[
            region_capture.clone(),
            fine_domain.clone(),
            wgsl_world_hits.clone(),
        ],
    )
    .expect("wgsl world surface batch");
    for (cpu, wgsl) in expect_array(&cpu_world_surfaces)
        .iter()
        .zip(expect_array(&wgsl_world_surfaces))
    {
        assert_surface_approx_eq(cpu, wgsl);
    }
    assert_batch_trace_contract(
        &wgsl_world_surface_trace,
        query_contract::SURFACE_SAMPLE_BATCH_WORLD,
    );

    let sample_items = KernelValue::Array(vec![
        point_direction_query([0.0, 0.0, 1.0], [0.0, 0.0, -1.0]),
        point_direction_query([0.0, 0.0, 2.0], [0.0, 0.0, -1.0]),
    ]);
    let world_radiance_cpu = lower_batch_query_plan(
        &BatchQueryPlan::for_contract(
            query_contract::PARTICIPANTS_RADIANCE_BATCH_WORLD,
            DispatchBackend::Cpu,
            None,
        )
        .unwrap(),
    );
    let world_radiance_wgsl = lower_batch_query_plan(
        &BatchQueryPlan::for_contract(
            query_contract::PARTICIPANTS_RADIANCE_BATCH_WORLD,
            DispatchBackend::Wgsl,
            None,
        )
        .unwrap(),
    );
    let (cpu_world_radiance, _) = execute_batch_query_with_trace(
        &ctx,
        &world_radiance_cpu,
        &[
            region_capture.clone(),
            fine_domain.clone(),
            sample_items.clone(),
        ],
    )
    .expect("cpu world radiance batch");
    let (wgsl_world_radiance, wgsl_world_radiance_trace) = execute_batch_query_with_trace(
        &ctx,
        &world_radiance_wgsl,
        &[region_capture.clone(), fine_domain.clone(), sample_items],
    )
    .expect("wgsl world radiance batch");
    for (cpu, wgsl) in expect_array(&cpu_world_radiance)
        .iter()
        .zip(expect_array(&wgsl_world_radiance))
    {
        assert_vec3_approx_eq(expect_vec3(cpu), expect_vec3(wgsl));
    }
    assert_batch_trace_contract(
        &wgsl_world_radiance_trace,
        query_contract::PARTICIPANTS_RADIANCE_BATCH_WORLD,
    );

    let world_medium_cpu = lower_batch_query_plan(
        &BatchQueryPlan::for_contract(
            query_contract::PARTICIPANTS_MEDIUM_BATCH_WORLD,
            DispatchBackend::Cpu,
            None,
        )
        .unwrap(),
    );
    let world_medium_wgsl = lower_batch_query_plan(
        &BatchQueryPlan::for_contract(
            query_contract::PARTICIPANTS_MEDIUM_BATCH_WORLD,
            DispatchBackend::Wgsl,
            None,
        )
        .unwrap(),
    );
    let (cpu_world_medium, _) = execute_batch_query_with_trace(
        &ctx,
        &world_medium_cpu,
        &[
            region_capture.clone(),
            fine_domain.clone(),
            point_items.clone(),
        ],
    )
    .expect("cpu world medium batch");
    let (wgsl_world_medium, wgsl_world_medium_trace) = execute_batch_query_with_trace(
        &ctx,
        &world_medium_wgsl,
        &[region_capture, fine_domain, point_items],
    )
    .expect("wgsl world medium batch");
    for (cpu, wgsl) in expect_array(&cpu_world_medium)
        .iter()
        .zip(expect_array(&wgsl_world_medium))
    {
        assert_medium_approx_eq(cpu, wgsl);
    }
    assert_batch_trace_contract(
        &wgsl_world_medium_trace,
        query_contract::PARTICIPANTS_MEDIUM_BATCH_WORLD,
    );
}

#[test]
fn query_exec_wgsl_matches_virtual_gpu_for_world_and_batch_queries() {
    let (_, _, ctx) = typed_query_module(query_fixture_source());
    let region_capture = KernelValue::Capture(SmolStr::new("scene_region"));
    let shape_capture = KernelValue::Capture(SmolStr::new("scene_shape"));
    let fine_domain = scene_domain(
        stable_region_scene_capture_id(&SmolStr::new("scene_region")),
        1,
        true,
        true,
        true,
    );
    let point_items = KernelValue::Array(vec![
        point_query([0.0, 0.0, 2.0]),
        point_query([0.0, 0.0, 3.0]),
    ]);
    let ray_items = KernelValue::Array(vec![
        ray_query([0.0, 0.0, 3.0], [0.0, 0.0, -1.0]),
        ray_query([0.0, 0.0, 3.0], [0.0, 1.0, 0.0]),
    ]);

    let world_distance_plan = lower_world_query_plan(&WorldQueryPlan::for_query_with_backend(
        WorldQueryKind::Distance,
        DispatchBackend::Wgsl,
    ));
    let vgpu_world_distance = execute_world_query_on(
        &ctx,
        DispatchBackend::VirtualGpu,
        &world_distance_plan,
        &[
            region_capture.clone(),
            fine_domain.clone(),
            KernelValue::Vec3([0.0, 0.0, 2.0]),
        ],
    )
    .expect("vgpu world distance");
    let wgsl_world_distance = execute_world_query_on(
        &ctx,
        DispatchBackend::Wgsl,
        &world_distance_plan,
        &[
            region_capture.clone(),
            fine_domain.clone(),
            KernelValue::Vec3([0.0, 0.0, 2.0]),
        ],
    )
    .expect("wgsl world distance");
    assert_approx_eq(
        expect_f32(&vgpu_world_distance),
        expect_f32(&wgsl_world_distance),
    );

    let world_trace_plan = lower_world_query_plan(&WorldQueryPlan::for_query_with_backend(
        WorldQueryKind::Trace,
        DispatchBackend::Wgsl,
    ));
    let world_trace_args = vec![
        region_capture.clone(),
        fine_domain.clone(),
        ray_query_with_limits([0.0, 0.0, 3.0], [0.0, 0.0, -1.0], 6.0, 0.05, 0.001, 96),
    ];
    let vgpu_world_hit = execute_world_query_on(
        &ctx,
        DispatchBackend::VirtualGpu,
        &world_trace_plan,
        &world_trace_args,
    )
    .expect("vgpu world trace");
    let wgsl_world_hit = execute_world_query_on(
        &ctx,
        DispatchBackend::Wgsl,
        &world_trace_plan,
        &world_trace_args,
    )
    .expect("wgsl world trace");
    assert_hit3_approx_eq(&vgpu_world_hit, &wgsl_world_hit);

    let world_surface_plan = lower_world_query_plan(&WorldQueryPlan::for_query_with_backend(
        WorldQueryKind::Surface,
        DispatchBackend::Wgsl,
    ));
    let vgpu_world_surface = execute_world_query_on(
        &ctx,
        DispatchBackend::VirtualGpu,
        &world_surface_plan,
        &[
            region_capture.clone(),
            fine_domain.clone(),
            vgpu_world_hit.clone(),
        ],
    )
    .expect("vgpu world surface");
    let wgsl_world_surface = execute_world_query_on(
        &ctx,
        DispatchBackend::Wgsl,
        &world_surface_plan,
        &[
            region_capture.clone(),
            fine_domain.clone(),
            wgsl_world_hit.clone(),
        ],
    )
    .expect("wgsl world surface");
    assert_surface_approx_eq(&vgpu_world_surface, &wgsl_world_surface);

    let world_medium_plan = lower_world_query_plan(&WorldQueryPlan::for_query_with_backend(
        WorldQueryKind::Medium,
        DispatchBackend::Wgsl,
    ));
    let vgpu_world_medium = execute_world_query_on(
        &ctx,
        DispatchBackend::VirtualGpu,
        &world_medium_plan,
        &[
            region_capture.clone(),
            fine_domain.clone(),
            KernelValue::Vec3([0.0, 0.1, 0.75]),
        ],
    )
    .expect("vgpu world medium");
    let wgsl_world_medium = execute_world_query_on(
        &ctx,
        DispatchBackend::Wgsl,
        &world_medium_plan,
        &[
            region_capture.clone(),
            fine_domain.clone(),
            KernelValue::Vec3([0.0, 0.1, 0.75]),
        ],
    )
    .expect("wgsl world medium");
    assert_medium_approx_eq(&vgpu_world_medium, &wgsl_world_medium);

    let world_batch_plan = lower_batch_query_plan(
        &BatchQueryPlan::for_contract(
            query_contract::SPATIAL_NEAREST_BATCH_WORLD,
            DispatchBackend::Wgsl,
            None,
        )
        .unwrap(),
    );
    let (vgpu_world_hits, vgpu_world_batch_trace) = execute_batch_query_with_trace_on(
        &ctx,
        DispatchBackend::VirtualGpu,
        &world_batch_plan,
        &[
            region_capture.clone(),
            fine_domain.clone(),
            ray_items.clone(),
        ],
    )
    .expect("vgpu world nearest batch");
    let (wgsl_world_hits, wgsl_world_batch_trace) = execute_batch_query_with_trace_on(
        &ctx,
        DispatchBackend::Wgsl,
        &world_batch_plan,
        &[
            region_capture.clone(),
            fine_domain.clone(),
            ray_items.clone(),
        ],
    )
    .expect("wgsl world nearest batch");
    for (vgpu, wgsl) in expect_array(&vgpu_world_hits)
        .iter()
        .zip(expect_array(&wgsl_world_hits))
    {
        assert_hit3_approx_eq(vgpu, wgsl);
    }
    assert_batch_trace_contract(
        &vgpu_world_batch_trace,
        query_contract::SPATIAL_NEAREST_BATCH_WORLD,
    );
    assert_batch_trace_contract(
        &wgsl_world_batch_trace,
        query_contract::SPATIAL_NEAREST_BATCH_WORLD,
    );
    assert_eq!(
        vgpu_world_batch_trace.observability.world_batch_item_count,
        2
    );
    assert_eq!(
        wgsl_world_batch_trace.observability.world_batch_item_count,
        2
    );

    let shape_distance_plan = lower_batch_query_plan(&BatchQueryPlan::for_field_query(
        BatchQueryKind::Distance,
        CaptureKind::Shape,
        DispatchBackend::Wgsl,
        None,
    ));
    let (vgpu_shape_distances, _) = execute_batch_query_with_trace_on(
        &ctx,
        DispatchBackend::VirtualGpu,
        &shape_distance_plan,
        &[shape_capture.clone(), point_items.clone()],
    )
    .expect("vgpu shape distance batch");
    let (wgsl_shape_distances, _) = execute_batch_query_with_trace_on(
        &ctx,
        DispatchBackend::Wgsl,
        &shape_distance_plan,
        &[shape_capture.clone(), point_items.clone()],
    )
    .expect("wgsl shape distance batch");
    for (vgpu, wgsl) in expect_array(&vgpu_shape_distances)
        .iter()
        .zip(expect_array(&wgsl_shape_distances))
    {
        assert_approx_eq(
            expect_f32(field(expect_struct(vgpu, "DistanceResult"), "distance")),
            expect_f32(field(expect_struct(wgsl, "DistanceResult"), "distance")),
        );
    }

    let trace_plan = lower_batch_query_plan(&BatchQueryPlan::for_shape_query(
        BatchQueryKind::Trace,
        DispatchBackend::Wgsl,
        None,
    ));
    let (vgpu_hits, _) = execute_batch_query_with_trace_on(
        &ctx,
        DispatchBackend::VirtualGpu,
        &trace_plan,
        &[shape_capture.clone(), ray_items.clone()],
    )
    .expect("vgpu trace batch");
    let (wgsl_hits, _) = execute_batch_query_with_trace_on(
        &ctx,
        DispatchBackend::Wgsl,
        &trace_plan,
        &[shape_capture.clone(), ray_items.clone()],
    )
    .expect("wgsl trace batch");
    for (vgpu, wgsl) in expect_array(&vgpu_hits)
        .iter()
        .zip(expect_array(&wgsl_hits))
    {
        assert_hit3_approx_eq(vgpu, wgsl);
    }

    let surface_plan = lower_batch_query_plan(&BatchQueryPlan::for_shape_query(
        BatchQueryKind::Surface,
        DispatchBackend::Wgsl,
        None,
    ));
    let (vgpu_surfaces, _) = execute_batch_query_with_trace_on(
        &ctx,
        DispatchBackend::VirtualGpu,
        &surface_plan,
        &[shape_capture.clone(), vgpu_hits.clone()],
    )
    .expect("vgpu surface batch");
    let (wgsl_surfaces, _) = execute_batch_query_with_trace_on(
        &ctx,
        DispatchBackend::Wgsl,
        &surface_plan,
        &[shape_capture.clone(), wgsl_hits.clone()],
    )
    .expect("wgsl surface batch");
    for (vgpu, wgsl) in expect_array(&vgpu_surfaces)
        .iter()
        .zip(expect_array(&wgsl_surfaces))
    {
        assert_surface_approx_eq(vgpu, wgsl);
    }

    let occluded_plan = lower_batch_query_plan(&BatchQueryPlan::for_shape_query(
        BatchQueryKind::Occluded,
        DispatchBackend::Wgsl,
        None,
    ));
    let (vgpu_occlusions, _) = execute_batch_query_with_trace_on(
        &ctx,
        DispatchBackend::VirtualGpu,
        &occluded_plan,
        &[shape_capture, ray_items.clone()],
    )
    .expect("vgpu occluded batch");
    let (wgsl_occlusions, _) = execute_batch_query_with_trace_on(
        &ctx,
        DispatchBackend::Wgsl,
        &occluded_plan,
        &[KernelValue::Capture(SmolStr::new("scene_shape")), ray_items],
    )
    .expect("wgsl occluded batch");
    for (vgpu, wgsl) in expect_array(&vgpu_occlusions)
        .iter()
        .zip(expect_array(&wgsl_occlusions))
    {
        let vgpu = expect_struct(vgpu, "OcclusionResult");
        let wgsl = expect_struct(wgsl, "OcclusionResult");
        assert_eq!(
            expect_bool(field(vgpu, "occluded")),
            expect_bool(field(wgsl, "occluded"))
        );
        assert_approx_eq(
            expect_f32(field(vgpu, "distance")),
            expect_f32(field(wgsl, "distance")),
        );
    }
}

#[test]
fn query_exec_opaque_fallback_updates_observability_counters() {
    let (_, _, ctx) = typed_query_module(opaque_fallback_source());
    let distance_plan = lower_capture_query_plan(
        &CaptureQueryPlan::for_query(CaptureQueryKind::Distance, CaptureKind::Field, None)
            .expect("distance plan"),
    );

    let (_value, trace) = execute_capture_query_with_trace_on(
        &ctx,
        DispatchBackend::Cpu,
        &distance_plan,
        &[
            KernelValue::Capture(SmolStr::new("opaque_field")),
            KernelValue::Vec3([0.0, 0.0, 0.0]),
        ],
    )
    .expect("opaque distance");
    assert_eq!(trace.observability.dispatch_count, 1);
    assert!(trace.observability.opaque_fallbacks > 0);
    assert!(trace.observability.artifact_loads > 0);
    assert!(trace.observability.field_samples > 0);
}

#[test]
fn query_exec_cpu_certified_normals_record_roles_without_sampling_fallbacks() {
    let (_, _, ctx) = typed_query_module(certified_normal_source());
    let field_plan = lower_capture_query_plan(
        &CaptureQueryPlan::for_query(CaptureQueryKind::Normal, CaptureKind::Field, None)
            .expect("field normal plan"),
    );
    let shape_plan = lower_capture_query_plan(
        &CaptureQueryPlan::for_query(CaptureQueryKind::Normal, CaptureKind::Shape, None)
            .expect("shape normal plan"),
    );
    let world_plan = lower_world_query_plan(&WorldQueryPlan::for_query(WorldQueryKind::Normal));

    let (plane_normal, plane_trace) = execute_capture_query_with_trace_on(
        &ctx,
        DispatchBackend::Cpu,
        &field_plan,
        &[
            KernelValue::Capture(SmolStr::new("exact_plane_field")),
            KernelValue::Vec3([0.0, 2.0, 0.0]),
        ],
    )
    .expect("plane normal");
    assert_eq!(expect_vec3(&plane_normal), [0.0, 1.0, 0.0]);
    assert_normal_role(&plane_trace, "normal_role::certified_field_gradient");
    assert_eq!(plane_trace.observability.field_samples, 0);

    let (translated_normal, translated_trace) = execute_capture_query_with_trace_on(
        &ctx,
        DispatchBackend::Cpu,
        &field_plan,
        &[
            KernelValue::Capture(SmolStr::new("translated_sphere_field")),
            KernelValue::Vec3([1.5, 0.0, 2.0]),
        ],
    )
    .expect("translated sphere normal");
    assert_eq!(expect_vec3(&translated_normal), [0.0, 0.0, 1.0]);
    assert_normal_role(&translated_trace, "normal_role::certified_field_gradient");
    assert_eq!(translated_trace.observability.field_samples, 0);

    let (rotated_normal, rotated_trace) = execute_capture_query_with_trace_on(
        &ctx,
        DispatchBackend::Cpu,
        &field_plan,
        &[
            KernelValue::Capture(SmolStr::new("rotated_sphere_field")),
            KernelValue::Vec3([0.0, 0.0, 2.0]),
        ],
    )
    .expect("rotated sphere normal");
    assert_eq!(expect_vec3(&rotated_normal), [0.0, 0.0, 1.0]);
    assert_normal_role(&rotated_trace, "normal_role::certified_field_gradient");
    assert_eq!(rotated_trace.observability.field_samples, 0);

    let (scaled_normal, scaled_trace) = execute_capture_query_with_trace_on(
        &ctx,
        DispatchBackend::Cpu,
        &field_plan,
        &[
            KernelValue::Capture(SmolStr::new("scaled_sphere_field")),
            KernelValue::Vec3([0.0, 0.0, 2.0]),
        ],
    )
    .expect("scaled sphere normal");
    assert_eq!(expect_vec3(&scaled_normal), [0.0, 0.0, 1.0]);
    assert_normal_role(&scaled_trace, "normal_role::certified_field_gradient");
    assert_eq!(scaled_trace.observability.field_samples, 0);

    let (shape_normal, shape_trace) = execute_capture_query_with_trace_on(
        &ctx,
        DispatchBackend::Cpu,
        &shape_plan,
        &[
            KernelValue::Capture(SmolStr::new("translated_sphere_shape")),
            KernelValue::Vec3([1.5, 0.0, 2.0]),
        ],
    )
    .expect("shape normal");
    assert_eq!(expect_vec3(&shape_normal), [0.0, 0.0, 1.0]);
    assert_normal_role(&shape_trace, "normal_role::feature_normal");
    assert_eq!(shape_trace.observability.field_samples, 0);

    let region_id = stable_region_scene_capture_id(&SmolStr::new("translated_region"));
    let world_domain = scene_domain(region_id, 1, true, true, true);
    let (world_normal, world_trace) = execute_world_query_with_trace_on(
        &ctx,
        DispatchBackend::Cpu,
        &world_plan,
        &[
            KernelValue::Capture(SmolStr::new("translated_region")),
            world_domain.clone(),
            KernelValue::Vec3([1.5, 0.0, 2.0]),
        ],
    )
    .expect("world normal");
    assert_eq!(expect_vec3(&world_normal), [0.0, 0.0, 1.0]);
    assert_normal_role(&world_trace, "normal_role::feature_normal");
    assert_eq!(world_trace.observability.field_samples, 0);

    let (wgsl_world_normal, wgsl_world_trace) = execute_world_query_with_trace_on(
        &ctx,
        DispatchBackend::Wgsl,
        &world_plan,
        &[
            KernelValue::Capture(SmolStr::new("translated_region")),
            world_domain,
            KernelValue::Vec3([1.5, 0.0, 2.0]),
        ],
    )
    .expect("wgsl world normal");
    assert_eq!(expect_vec3(&wgsl_world_normal), [0.0, 0.0, 1.0]);
    assert_normal_role(&wgsl_world_trace, "normal_role::feature_normal");
}

#[test]
fn query_exec_cpu_certifies_supported_smooth_normals_and_falls_back_for_repetition() {
    let (_, _, ctx) = typed_query_module(certified_normal_source());
    let field_plan = lower_capture_query_plan(
        &CaptureQueryPlan::for_query(CaptureQueryKind::Normal, CaptureKind::Field, None)
            .expect("field normal plan"),
    );
    let world_plan = lower_world_query_plan(&WorldQueryPlan::for_query(WorldQueryKind::Normal));

    let (smooth_normal, smooth_trace) = execute_capture_query_with_trace_on(
        &ctx,
        DispatchBackend::Cpu,
        &field_plan,
        &[
            KernelValue::Capture(SmolStr::new("smooth_certified_field")),
            KernelValue::Vec3([0.25, 0.0, 0.0]),
        ],
    )
    .expect("smooth certified normal");
    assert!(length3(expect_vec3(&smooth_normal)) > 0.0);
    assert_normal_role(&smooth_trace, "normal_role::certified_field_gradient");
    assert!(smooth_trace.observability.field_samples <= 1);

    let (fallback_normal, fallback_trace) = execute_capture_query_with_trace_on(
        &ctx,
        DispatchBackend::Cpu,
        &field_plan,
        &[
            KernelValue::Capture(SmolStr::new("repeated_fallback_field")),
            KernelValue::Vec3([0.4, 0.0, 0.0]),
        ],
    )
    .expect("repeated fallback normal");
    assert!(length3(expect_vec3(&fallback_normal)) > 0.0);
    assert_normal_role(&fallback_trace, "normal_role::heuristic_shading_normal");
    assert!(fallback_trace.observability.field_samples > smooth_trace.observability.field_samples);

    let region_id = stable_region_scene_capture_id(&SmolStr::new("repeated_region"));
    let world_domain = scene_domain(region_id, 1, true, true, true);
    let (world_fallback_normal, world_fallback_trace) = execute_world_query_with_trace_on(
        &ctx,
        DispatchBackend::Cpu,
        &world_plan,
        &[
            KernelValue::Capture(SmolStr::new("repeated_region")),
            world_domain,
            KernelValue::Vec3([0.4, 0.0, 0.0]),
        ],
    )
    .expect("repeated world fallback normal");
    assert!(length3(expect_vec3(&world_fallback_normal)) > 0.0);
    assert_normal_role(
        &world_fallback_trace,
        "normal_role::heuristic_shading_normal",
    );
    assert!(world_fallback_trace.observability.field_samples > 0);
}

#[test]
fn query_exec_virtual_gpu_rejects_invalid_batch_contracts_before_execution() {
    let (_, _, ctx) = typed_query_module(query_fixture_source());
    let capture = KernelValue::Capture(SmolStr::new("scene_shape"));
    let rays = KernelValue::Array(vec![ray_query([0.0, 0.0, 3.0], [0.0, 0.0, -1.0])]);

    let mut version_plan = lower_batch_query_plan(&BatchQueryPlan::for_shape_query(
        BatchQueryKind::Trace,
        DispatchBackend::VirtualGpu,
        None,
    ));
    version_plan.contract_version = 0;
    let version_error =
        execute_batch_query_with_trace(&ctx, &version_plan, &[capture.clone(), rays.clone()])
            .expect_err("invalid contract version should fail");
    assert!(
        format!("{version_error:?}").contains("contract version"),
        "expected contract-version validation failure, got {version_error:?}"
    );

    let mut artifact_plan = lower_batch_query_plan(&BatchQueryPlan::for_shape_query(
        BatchQueryKind::Trace,
        DispatchBackend::VirtualGpu,
        None,
    ));
    artifact_plan
        .artifact_contracts
        .retain(|artifact| !matches!(artifact.schema, ArtifactSchema::DispatchRecord { .. }));
    let artifact_error =
        execute_batch_query_with_trace(&ctx, &artifact_plan, &[capture.clone(), rays.clone()])
            .expect_err("missing dispatch artifact should fail");
    assert!(
        format!("{artifact_error:?}").contains("dispatch artifact contract"),
        "expected dispatch artifact validation failure, got {artifact_error:?}"
    );

    let mut stage_plan = lower_batch_query_plan(&BatchQueryPlan::for_shape_query(
        BatchQueryKind::Trace,
        DispatchBackend::VirtualGpu,
        None,
    ));
    stage_plan
        .stages
        .retain(|stage| !matches!(stage, KernelPlanStage::EndVirtualGpuDispatch));
    let stage_error = execute_batch_query_with_trace(&ctx, &stage_plan, &[capture, rays])
        .expect_err("missing virtual GPU end stage should fail");
    assert!(
        format!("{stage_error:?}").contains("both begin and end stages"),
        "expected stage validation failure, got {stage_error:?}"
    );

    let mut nested_plan = lower_batch_query_plan(&BatchQueryPlan::for_shape_query(
        BatchQueryKind::Trace,
        DispatchBackend::VirtualGpu,
        None,
    ));
    let KernelBatchItemContract::CaptureQuery { plan: nested } = &mut nested_plan.item_contract
    else {
        panic!("expected capture item contract");
    };
    nested.contract_version = 0;
    let nested_error = execute_batch_query_with_trace(
        &ctx,
        &nested_plan,
        &[
            KernelValue::Capture(SmolStr::new("scene_shape")),
            KernelValue::Array(vec![ray_query([0.0, 0.0, 3.0], [0.0, 0.0, -1.0])]),
        ],
    )
    .expect_err("invalid nested item contract should fail");
    assert!(
        format!("{nested_error:?}").contains("capture query contract")
            && format!("{nested_error:?}").contains("contract version"),
        "expected nested capture-plan validation failure, got {nested_error:?}"
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
    assert!(expect_bool(field(summary, "scalar_occluded")));
    assert!(expect_bool(field(summary, "world_occluded")));

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

#[test]
fn direct_world_queries_use_plan_backend_and_auto_can_resolve_to_wgsl() {
    let (_, _, ctx) = typed_query_module(query_fixture_source());
    let region_capture = KernelValue::Capture(SmolStr::new("scene_region"));
    let fine_domain = scene_domain(
        stable_region_scene_capture_id(&SmolStr::new("scene_region")),
        1,
        true,
        true,
        true,
    );
    let query_point = KernelValue::Vec3([0.0, 0.0, 2.0]);

    let vgpu_plan = lower_world_query_plan(&WorldQueryPlan::for_query_with_backend(
        WorldQueryKind::Distance,
        DispatchBackend::VirtualGpu,
    ));
    let (vgpu_value, vgpu_trace) = execute_world_query_with_trace_on(
        &ctx,
        DispatchBackend::Auto,
        &vgpu_plan,
        &[
            region_capture.clone(),
            fine_domain.clone(),
            query_point.clone(),
        ],
    )
    .expect("auto should resolve to plan virtual gpu backend");
    assert_approx_eq(expect_f32(&vgpu_value), 1.0);
    assert_eq!(vgpu_trace.backend, DispatchBackend::VirtualGpu);
    assert_eq!(vgpu_trace.executor, DirectQueryExecutor::VirtualGpu);

    let wgsl_plan = lower_world_query_plan(&WorldQueryPlan::for_query_with_backend(
        WorldQueryKind::Distance,
        DispatchBackend::Wgsl,
    ));
    let (wgsl_value, wgsl_trace) = execute_world_query_with_trace_on(
        &ctx,
        DispatchBackend::Auto,
        &wgsl_plan,
        &[region_capture, fine_domain, query_point],
    )
    .expect("auto should resolve to plan wgsl backend");
    assert_approx_eq(expect_f32(&wgsl_value), 1.0);
    assert_eq!(wgsl_trace.backend, DispatchBackend::Wgsl);
    assert_eq!(wgsl_trace.executor, DirectQueryExecutor::Wgsl);
    assert_eq!(wgsl_trace.observability.dispatch_count, 1);
}

fn direct_semantics_source() -> &'static str {
    r#"
field exact distance near_field(p: Vec3) -> F32 {
    sphere(radius = 0.6)
}
field exact distance far_field(p: Vec3) -> F32 {
    translate = vec3(0.0, 0.0, -0.35) {
        sphere(radius = 0.8)
    }
}

field conservative distance identity_field(p: Vec3) -> F32 {
    instance_array = Transform3(
        matrix=mat4_cols(
            vec4(1.0, 0.0, 0.0, 0.0),
            vec4(0.0, 1.0, 0.0, 0.0),
            vec4(0.0, 0.0, 1.0, 0.0),
            vec4(1.0, 0.0, 0.0, 1.0)
        ),
        inverse=mat4_cols(
            vec4(1.0, 0.0, 0.0, 0.0),
            vec4(0.0, 1.0, 0.0, 0.0),
            vec4(0.0, 0.0, 1.0, 0.0),
            vec4(-1.0, 0.0, 0.0, 1.0)
        )
    ) {
        repeat_linear = vec3(2.0, 0.0, 0.0) {
            translate = vec3(0.25, 0.0, 0.0) {
                sphere(radius = 0.5)
            }
        }
    }
}

field exact distance left_glow_field(p: Vec3) -> F32 {
    translate = vec3(-1.5, 0.0, 0.0) {
        sphere(radius = 0.25)
    }
}

field exact distance right_glow_field(p: Vec3) -> F32 {
    translate = vec3(0.25, 0.0, 0.0) {
        sphere(radius = 0.25)
    }
}

material near_surface(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(0.1, 0.2, 0.3),
        roughness=0.0,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

material far_surface(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(0.9, 0.1, 0.1),
        roughness=0.0,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

material identity_surface(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(0.3, 0.3, 0.3),
        roughness=0.0,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

radiance field glow_local(p: Vec3, direction: Vec3, feature_id: U32) -> Vec3 {
    return vec3(abs(p.x), abs(p.x) * 0.5, f32(feature_id) * 0.0) + direction * 0.0
}

volume field fog_local(p: Vec3, surface_distance: F32) -> Medium {
    return Medium(
        density=abs(p.x),
        emission=vec3(abs(surface_distance), 0.0, 0.0),
        anisotropy=0.25
    )
}

shape near_shape {
    field = near_field
    material = near_surface
    payload = Payload(
        entity_id=u32(101),
        material_id=u32(101),
        actor=ActorHandle(id=u32(101), generation=u32(0))
    )
}

shape far_shape {
    field = far_field
    material = far_surface
    payload = Payload(
        entity_id=u32(202),
        material_id=u32(202),
        actor=ActorHandle(id=u32(202), generation=u32(0))
    )
}

shape nearest_scene {
    union {
        provenance_policy = nearest
        use far_shape
        use near_shape
    }
}

shape ordered_scene {
    union {
        provenance_policy = ordered
        use far_shape
        use near_shape
    }
}

shape identity_shape {
    field = identity_field
    material = identity_surface
    payload = Payload(
        entity_id=u32(303),
        material_id=u32(303),
        actor=ActorHandle(id=u32(303), generation=u32(0))
    )
}

shape left_glow_shape {
    field = left_glow_field
    material = identity_surface
    radiance = glow_local
    volume = fog_local
    payload = Payload()
}

shape right_glow_shape {
    field = right_glow_field
    material = identity_surface
    radiance = glow_local
    volume = fog_local
    payload = Payload()
}

shape lighting_scene {
    union {
        provenance_policy = nearest
        use left_glow_shape
        use right_glow_shape
    }
}
"#
}

fn world_support_cost_fixture_source() -> &'static str {
    r#"
field exact distance near_field(p: Vec3) -> F32 {
    sphere(radius = 0.6)
}

field conservative distance far_supported_field(p: Vec3) -> F32 {
    support = Support3(bounds=Bounds3(
        min=vec3(8.8, -0.8, -0.8),
        max=vec3(10.2, 0.8, 0.8)
    ))
    bounds = Bounds3(
        min=vec3(8.8, -0.8, -0.8),
        max=vec3(10.2, 0.8, 0.8)
    )
    return length(p - vec3(9.5, 0.0, 0.0)) - 0.5
}

material shade(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(0.25, 0.35, 0.45),
        roughness=0.5,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

shape near_shape {
    field = near_field
    material = shade
    payload = Payload(entity_id=u32(1), material_id=u32(1), actor=ActorHandle(id=u32(1), generation=u32(0)))
}

shape far_shape {
    field = far_supported_field
    material = shade
    payload = Payload(entity_id=u32(2), material_id=u32(2), actor=ActorHandle(id=u32(2), generation=u32(0)))
}

region scene_region() {
    place near = near_shape
    place far = far_shape
}

domain scene_domain(world: RegionCapture) {
    geometry_detail = 1
    material = false
    radiance = false
    media = false
    max_distance = 6.0
    min_step = 0.05
    hit_epsilon = 0.001
    max_steps = 96
}
"#
}

fn world_ray_solver_support_fixture_source() -> &'static str {
    r#"
field exact distance near_field(p: Vec3) -> F32 {
    sphere(radius = 0.6)
}

field exact distance far_supported_field(p: Vec3) -> F32 {
    translate = vec3(9.5, 0.0, 0.0) {
        sphere(radius = 0.5)
    }
}

material shade(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(0.25, 0.35, 0.45),
        roughness=0.5,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

shape near_shape {
    field = near_field
    material = shade
    payload = Payload(entity_id=u32(1), material_id=u32(1), actor=ActorHandle(id=u32(1), generation=u32(0)))
}

shape far_shape {
    field = far_supported_field
    material = shade
    payload = Payload(entity_id=u32(2), material_id=u32(2), actor=ActorHandle(id=u32(2), generation=u32(0)))
}

region scene_region() {
    place near = near_shape
    place far = far_shape
}

domain scene_domain(world: RegionCapture) {
    geometry_detail = 1
    material = false
    radiance = false
    media = false
    max_distance = 6.0
    min_step = 0.05
    hit_epsilon = 0.001
    max_steps = 96
}
"#
}

fn ray_solver_opaque_fixture_source() -> &'static str {
    r#"
field conservative distance opaque_field(p: Vec3) -> F32 {
    support = Support3(bounds=Bounds3(
        min=vec3(-0.6, -0.6, -0.6),
        max=vec3(0.6, 0.6, 0.6)
    ))
    bounds = Bounds3(
        min=vec3(-0.6, -0.6, -0.6),
        max=vec3(0.6, 0.6, 0.6)
    )
    return length(p) - 0.6
}

material shade(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(0.25, 0.35, 0.45),
        roughness=0.5,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

shape opaque_shape {
    field = opaque_field
    material = shade
    payload = Payload(entity_id=u32(7), material_id=u32(8), actor=ActorHandle(id=u32(9), generation=u32(0)))
}

region scene_region() {
    place opaque = opaque_shape
}

domain scene_domain(world: RegionCapture) {
    geometry_detail = 1
    material = false
    radiance = false
    media = false
    max_distance = 6.0
    min_step = 0.05
    hit_epsilon = 0.001
    max_steps = 96
}
"#
}

fn wgsl_profile_fixture_source() -> &'static str {
    r#"
field conservative distance polygon_plate(p: Vec3) -> F32 {
    extrude = f32(0.4) {
        polygon2(vertices = [
            vec2(-0.4, -0.3),
            vec2(0.5, -0.2),
            vec2(0.3, 0.4),
            vec2(-0.3, 0.35)
        ])
    }
}

field conservative distance polyline_strip(p: Vec3) -> F32 {
    extrude = f32(0.16) {
        polyline2(vertices = [
            vec2(-0.28, -0.10),
            vec2(0.0, 0.14),
            vec2(0.28, -0.10)
        ])
    }
}

field conservative distance repeated_plate(p: Vec3) -> F32 {
    instance_array = Transform3(
        matrix=mat4_cols(
            vec4(1.0, 0.0, 0.0, 0.0),
            vec4(0.0, 1.0, 0.0, 0.0),
            vec4(0.0, 0.0, 1.0, 0.0),
            vec4(0.5, 0.0, 0.0, 1.0)
        ),
        inverse=mat4_cols(
            vec4(1.0, 0.0, 0.0, 0.0),
            vec4(0.0, 1.0, 0.0, 0.0),
            vec4(0.0, 0.0, 1.0, 0.0),
            vec4(-0.5, 0.0, 0.0, 1.0)
        )
    ) {
        repeat_linear = vec3(1.0, 0.0, 0.0) {
            use polygon_plate
        }
    }
}
"#
}

fn profile_ops_source() -> &'static str {
    r#"
field conservative distance extruded_disc(p: Vec3) -> F32 {
    extrude = f32(1.6) {
        circle2(radius = 0.75)
    }
}

field conservative distance revolved_orb(p: Vec3) -> F32 {
    revolve {
        circle2(radius = 0.5)
    }
}

field conservative distance swept_beam(p: Vec3) -> F32 {
    sweep = vec3(0.0, 1.6, 0.0) {
        circle2(radius = 0.15)
    }
}

field conservative distance lofted_form(p: Vec3) -> F32 {
    loft = f32(1.2) {
        from circle2(radius = 0.32)
        to rounded_rect2(half = vec2(0.42, 0.28), radius = 0.08)
    }
}
"#
}

fn certified_normal_source() -> &'static str {
    r#"
material shade(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(0.2, 0.3, 0.4),
        roughness=0.4,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

field exact distance exact_plane_field(p: Vec3) -> F32 {
    plane(normal = vec3(0.0, 1.0, 0.0), offset = 0.0)
}

field exact distance translated_sphere_field(p: Vec3) -> F32 {
    translate = vec3(1.5, 0.0, 0.0) {
        sphere(radius = 1.0)
    }
}

field exact distance rotated_sphere_field(p: Vec3) -> F32 {
    rotate = vec3(0.0, 0.0, 1.5707963) {
        sphere(radius = 1.0)
    }
}

field exact distance scaled_sphere_field(p: Vec3) -> F32 {
    uniform_scale = f32(2.0) {
        sphere(radius = 1.0)
    }
}

field conservative distance smooth_certified_field(p: Vec3) -> F32 {
    smooth_union {
        smoothing = f32(0.35)
        use translated_sphere_field
        translate = vec3(2.1, 0.0, 0.0) {
            sphere(radius = 1.0)
        }
    }
}

field conservative distance repeated_fallback_field(p: Vec3) -> F32 {
    repeat_linear = vec3(2.5, 0.0, 0.0) {
        sphere(radius = 1.0)
    }
}

shape translated_sphere_shape {
    field = translated_sphere_field
    material = shade
    payload = Payload()
}

shape smooth_certified_shape {
    field = smooth_certified_field
    material = shade
    payload = Payload()
}

shape repeated_fallback_shape {
    field = repeated_fallback_field
    material = shade
    payload = Payload()
}

region translated_region() {
    place translated = translated_sphere_shape
}

region repeated_region() {
    place repeated = repeated_fallback_shape
}
"#
}

fn opaque_fallback_source() -> &'static str {
    r#"
field conservative distance opaque_field(p: Vec3) -> F32 {
    support = Support3(bounds=Bounds3(
        min=vec3(-1.0, -1.0, -1.0),
        max=vec3(1.0, 1.0, 1.0)
    ))
    bounds = Bounds3(
        min=vec3(-1.0, -1.0, -1.0),
        max=vec3(1.0, 1.0, 1.0)
    )
    return length(p - vec3(3.0, 0.0, 0.0)) - 0.5
}
"#
}

#[test]
fn query_exec_direct_trace_preserves_local_context_and_shape_provenance() {
    let (_, _, ctx) = typed_query_module(direct_semantics_source());
    let trace_plan = lower_capture_query_plan(
        &CaptureQueryPlan::for_query(CaptureQueryKind::Trace, CaptureKind::Shape, None)
            .expect("trace plan"),
    );
    let surface_plan = lower_capture_query_plan(
        &CaptureQueryPlan::for_query(CaptureQueryKind::Surface, CaptureKind::Shape, None)
            .expect("surface plan"),
    );

    let nearest_hit = execute_capture_query(
        &ctx,
        &trace_plan,
        &[
            KernelValue::Capture(SmolStr::new("nearest_scene")),
            ray_query_with_limits([0.0, 0.0, 3.0], [0.0, 0.0, -1.0], 6.0, 0.05, 0.001, 96),
        ],
    )
    .expect("nearest trace");
    let ordered_hit = execute_capture_query(
        &ctx,
        &trace_plan,
        &[
            KernelValue::Capture(SmolStr::new("ordered_scene")),
            ray_query_with_limits([0.0, 0.0, 3.0], [0.0, 0.0, -1.0], 6.0, 0.05, 0.001, 96),
        ],
    )
    .expect("ordered trace");

    let nearest_hit = expect_struct(&nearest_hit, "Hit3");
    let ordered_hit = expect_struct(&ordered_hit, "Hit3");
    let nearest_payload = expect_struct(field(nearest_hit, "payload"), "Payload");
    let ordered_payload = expect_struct(field(ordered_hit, "payload"), "Payload");

    assert_eq!(expect_u32(field(nearest_payload, "entity_id")), 101);
    assert_eq!(expect_u32(field(ordered_payload, "entity_id")), 202);

    let ordered_surface = execute_capture_query(
        &ctx,
        &surface_plan,
        &[
            KernelValue::Capture(SmolStr::new("ordered_scene")),
            KernelValue::Struct(ordered_hit.clone()),
        ],
    )
    .expect("ordered surface");
    let ordered_surface = expect_struct(&ordered_surface, "Surface");
    assert_vec3_approx_eq(
        expect_vec3(field(ordered_surface, "albedo")),
        [0.9, 0.1, 0.1],
    );

    let identity_hit = execute_capture_query(
        &ctx,
        &trace_plan,
        &[
            KernelValue::Capture(SmolStr::new("identity_shape")),
            ray_query_with_limits([3.25, 0.0, 3.0], [0.0, 0.0, -1.0], 6.0, 0.05, 0.001, 96),
        ],
    )
    .expect("identity trace");
    let identity_hit = expect_struct(&identity_hit, "Hit3");
    assert_vec3_approx_eq(
        expect_vec3(field(identity_hit, "local_position")),
        [0.0, 0.0, 0.5],
    );
    assert_vec3_approx_eq(
        expect_vec3(field(identity_hit, "local_normal")),
        [0.0, 0.0, 1.0],
    );
    assert!(expect_u32(field(identity_hit, "instance_id")) != 0);
    assert!(expect_u32(field(identity_hit, "repeat_id")) != 0);

    let shading_frame = expect_struct(field(identity_hit, "shading_frame"), "Transform3");
    let shading_matrix = expect_mat4(field(shading_frame, "matrix"));
    assert_ne!(
        shading_matrix,
        [
            1.0, 0.0, 0.0, 0.0, //
            0.0, 1.0, 0.0, 0.0, //
            0.0, 0.0, 1.0, 0.0, //
            0.0, 0.0, 0.0, 1.0,
        ]
    );
    assert_vec3_approx_eq(
        [shading_matrix[12], shading_matrix[13], shading_matrix[14]],
        expect_vec3(field(identity_hit, "position")),
    );
}

#[test]
fn query_exec_virtual_gpu_matches_cpu_for_local_context_and_provenance_sensitive_queries() {
    let (_, _, ctx) = typed_query_module(direct_semantics_source());
    let trace_plan = lower_capture_query_plan(
        &CaptureQueryPlan::for_query(CaptureQueryKind::Trace, CaptureKind::Shape, None)
            .expect("trace plan"),
    );
    let surface_plan = lower_capture_query_plan(
        &CaptureQueryPlan::for_query(CaptureQueryKind::Surface, CaptureKind::Shape, None)
            .expect("surface plan"),
    );
    let radiance_plan = lower_capture_query_plan(
        &CaptureQueryPlan::for_query(CaptureQueryKind::Radiance, CaptureKind::Shape, None)
            .expect("radiance plan"),
    );
    let medium_plan = lower_capture_query_plan(
        &CaptureQueryPlan::for_query(CaptureQueryKind::Medium, CaptureKind::Shape, None)
            .expect("medium plan"),
    );

    let ordered_trace_args = vec![
        KernelValue::Capture(SmolStr::new("ordered_scene")),
        ray_query_with_limits([0.0, 0.0, 3.0], [0.0, 0.0, -1.0], 6.0, 0.05, 0.001, 96),
    ];
    let cpu_ordered_hit =
        execute_capture_query(&ctx, &trace_plan, &ordered_trace_args).expect("cpu ordered trace");
    let (vgpu_ordered_hit, vgpu_ordered_trace) = execute_capture_query_with_trace_on(
        &ctx,
        DispatchBackend::VirtualGpu,
        &trace_plan,
        &ordered_trace_args,
    )
    .expect("vgpu ordered trace");
    assert_eq!(cpu_ordered_hit, vgpu_ordered_hit);
    assert_eq!(vgpu_ordered_trace.backend, DispatchBackend::VirtualGpu);
    assert_eq!(vgpu_ordered_trace.executor, DirectQueryExecutor::VirtualGpu);

    let cpu_surface = execute_capture_query(
        &ctx,
        &surface_plan,
        &[
            KernelValue::Capture(SmolStr::new("ordered_scene")),
            cpu_ordered_hit.clone(),
        ],
    )
    .expect("cpu ordered surface");
    let vgpu_surface = execute_capture_query_on(
        &ctx,
        DispatchBackend::VirtualGpu,
        &surface_plan,
        &[
            KernelValue::Capture(SmolStr::new("ordered_scene")),
            vgpu_ordered_hit.clone(),
        ],
    )
    .expect("vgpu ordered surface");
    assert_eq!(cpu_surface, vgpu_surface);

    let identity_trace_args = vec![
        KernelValue::Capture(SmolStr::new("identity_shape")),
        ray_query_with_limits([3.25, 0.0, 3.0], [0.0, 0.0, -1.0], 6.0, 0.05, 0.001, 96),
    ];
    let cpu_identity_hit =
        execute_capture_query(&ctx, &trace_plan, &identity_trace_args).expect("cpu identity trace");
    let vgpu_identity_hit = execute_capture_query_on(
        &ctx,
        DispatchBackend::VirtualGpu,
        &trace_plan,
        &identity_trace_args,
    )
    .expect("vgpu identity trace");
    assert_eq!(cpu_identity_hit, vgpu_identity_hit);

    let radiance_args = vec![
        KernelValue::Capture(SmolStr::new("lighting_scene")),
        point_direction_query([0.25, 0.0, 0.0], [0.0, 0.0, 1.0]),
    ];
    let cpu_radiance =
        execute_capture_query(&ctx, &radiance_plan, &radiance_args).expect("cpu radiance");
    let vgpu_radiance = execute_capture_query_on(
        &ctx,
        DispatchBackend::VirtualGpu,
        &radiance_plan,
        &radiance_args,
    )
    .expect("vgpu radiance");
    assert_eq!(cpu_radiance, vgpu_radiance);

    let medium_args = vec![
        KernelValue::Capture(SmolStr::new("lighting_scene")),
        KernelValue::Vec3([0.25, 0.0, 0.0]),
    ];
    let cpu_medium = execute_capture_query(&ctx, &medium_plan, &medium_args).expect("cpu medium");
    let vgpu_medium = execute_capture_query_on(
        &ctx,
        DispatchBackend::VirtualGpu,
        &medium_plan,
        &medium_args,
    )
    .expect("vgpu medium");
    assert_eq!(cpu_medium, vgpu_medium);
}

#[test]
fn query_exec_wgsl_matches_cpu_for_local_context_and_provenance_sensitive_queries() {
    let (_, _, ctx) = typed_query_module(direct_semantics_source());
    let trace_plan = lower_capture_query_plan(
        &CaptureQueryPlan::for_query(CaptureQueryKind::Trace, CaptureKind::Shape, None)
            .expect("trace plan"),
    );
    let surface_plan = lower_capture_query_plan(
        &CaptureQueryPlan::for_query(CaptureQueryKind::Surface, CaptureKind::Shape, None)
            .expect("surface plan"),
    );
    let radiance_plan = lower_capture_query_plan(
        &CaptureQueryPlan::for_query(CaptureQueryKind::Radiance, CaptureKind::Shape, None)
            .expect("radiance plan"),
    );
    let medium_plan = lower_capture_query_plan(
        &CaptureQueryPlan::for_query(CaptureQueryKind::Medium, CaptureKind::Shape, None)
            .expect("medium plan"),
    );

    let ordered_trace_args = vec![
        KernelValue::Capture(SmolStr::new("ordered_scene")),
        ray_query_with_limits([0.0, 0.0, 3.0], [0.0, 0.0, -1.0], 6.0, 0.05, 0.001, 96),
    ];
    let cpu_ordered_hit =
        execute_capture_query(&ctx, &trace_plan, &ordered_trace_args).expect("cpu ordered trace");
    let (wgsl_ordered_hit, wgsl_ordered_trace) = execute_capture_query_with_trace_on(
        &ctx,
        DispatchBackend::Wgsl,
        &trace_plan,
        &ordered_trace_args,
    )
    .expect("wgsl ordered trace");
    assert_hit3_approx_eq(&cpu_ordered_hit, &wgsl_ordered_hit);
    assert_eq!(wgsl_ordered_trace.backend, DispatchBackend::Wgsl);
    assert_eq!(wgsl_ordered_trace.executor, DirectQueryExecutor::Wgsl);

    let cpu_surface = execute_capture_query(
        &ctx,
        &surface_plan,
        &[
            KernelValue::Capture(SmolStr::new("ordered_scene")),
            cpu_ordered_hit.clone(),
        ],
    )
    .expect("cpu ordered surface");
    let wgsl_surface = execute_capture_query_on(
        &ctx,
        DispatchBackend::Wgsl,
        &surface_plan,
        &[
            KernelValue::Capture(SmolStr::new("ordered_scene")),
            wgsl_ordered_hit.clone(),
        ],
    )
    .expect("wgsl ordered surface");
    assert_surface_approx_eq(&cpu_surface, &wgsl_surface);

    let identity_trace_args = vec![
        KernelValue::Capture(SmolStr::new("identity_shape")),
        ray_query_with_limits([3.25, 0.0, 3.0], [0.0, 0.0, -1.0], 6.0, 0.05, 0.001, 96),
    ];
    let cpu_identity_hit =
        execute_capture_query(&ctx, &trace_plan, &identity_trace_args).expect("cpu identity trace");
    let wgsl_identity_hit = execute_capture_query_on(
        &ctx,
        DispatchBackend::Wgsl,
        &trace_plan,
        &identity_trace_args,
    )
    .expect("wgsl identity trace");
    assert_hit3_approx_eq(&cpu_identity_hit, &wgsl_identity_hit);

    let radiance_args = vec![
        KernelValue::Capture(SmolStr::new("lighting_scene")),
        point_direction_query([0.25, 0.0, 0.0], [0.0, 0.0, 1.0]),
    ];
    let cpu_radiance =
        execute_capture_query(&ctx, &radiance_plan, &radiance_args).expect("cpu radiance");
    let wgsl_radiance =
        execute_capture_query_on(&ctx, DispatchBackend::Wgsl, &radiance_plan, &radiance_args)
            .expect("wgsl radiance");
    assert_vec3_approx_eq(expect_vec3(&cpu_radiance), expect_vec3(&wgsl_radiance));

    let medium_args = vec![
        KernelValue::Capture(SmolStr::new("lighting_scene")),
        KernelValue::Vec3([0.25, 0.0, 0.0]),
    ];
    let cpu_medium = execute_capture_query(&ctx, &medium_plan, &medium_args).expect("cpu medium");
    let wgsl_medium =
        execute_capture_query_on(&ctx, DispatchBackend::Wgsl, &medium_plan, &medium_args)
            .expect("wgsl medium");
    assert_medium_approx_eq(&cpu_medium, &wgsl_medium);
}

#[test]
fn query_exec_wgsl_supports_polygon2_polyline2_and_repeat_wrappers() {
    let (_, _, ctx) = typed_query_module(wgsl_profile_fixture_source());
    let distance_plan = lower_capture_query_plan(
        &CaptureQueryPlan::for_query(CaptureQueryKind::Distance, CaptureKind::Field, None)
            .expect("distance plan"),
    );

    for (capture, point) in [
        ("polygon_plate", [0.05, 0.0, 0.05]),
        ("polyline_strip", [0.0, 0.0, 0.0]),
        ("repeated_plate", [0.55, 0.0, 0.05]),
    ] {
        let args = [
            KernelValue::Capture(SmolStr::new(capture)),
            KernelValue::Vec3(point),
        ];
        let cpu = execute_capture_query(&ctx, &distance_plan, &args)
            .unwrap_or_else(|err| panic!("cpu profile distance for {capture}: {err:?}"));
        let wgsl = execute_capture_query_on(&ctx, DispatchBackend::Wgsl, &distance_plan, &args)
            .unwrap_or_else(|err| panic!("wgsl profile distance for {capture}: {err:?}"));
        assert_approx_eq(expect_f32(&cpu), expect_f32(&wgsl));
    }
}

#[test]
fn query_exec_direct_radiance_and_medium_use_local_participation() {
    let (_, _, ctx) = typed_query_module(direct_semantics_source());
    let radiance_plan = lower_capture_query_plan(
        &CaptureQueryPlan::for_query(CaptureQueryKind::Radiance, CaptureKind::Shape, None)
            .expect("radiance plan"),
    );
    let medium_plan = lower_capture_query_plan(
        &CaptureQueryPlan::for_query(CaptureQueryKind::Medium, CaptureKind::Shape, None)
            .expect("medium plan"),
    );

    let radiance = execute_capture_query(
        &ctx,
        &radiance_plan,
        &[
            KernelValue::Capture(SmolStr::new("lighting_scene")),
            point_direction_query([0.25, 0.0, 0.0], [0.0, 0.0, 1.0]),
        ],
    )
    .expect("radiance");
    assert_vec3_approx_eq(expect_vec3(&radiance), [1.75, 0.875, 0.0]);

    let medium = execute_capture_query(
        &ctx,
        &medium_plan,
        &[
            KernelValue::Capture(SmolStr::new("lighting_scene")),
            KernelValue::Vec3([0.25, 0.0, 0.0]),
        ],
    )
    .expect("medium");
    let medium = expect_struct(&medium, "Medium");
    assert_approx_eq(expect_f32(field(medium, "density")), 1.75);
    assert_vec3_approx_eq(expect_vec3(field(medium, "emission")), [1.75, 0.0, 0.0]);
    assert_approx_eq(expect_f32(field(medium, "anisotropy")), 0.25);
}

#[test]
fn query_exec_direct_cpu_closes_profile_op_distance_gaps() {
    let (_, _, ctx) = typed_query_module(profile_ops_source());
    let distance_plan = lower_capture_query_plan(
        &CaptureQueryPlan::for_query(CaptureQueryKind::Distance, CaptureKind::Field, None)
            .expect("distance plan"),
    );

    for (field_name, center, far) in [
        ("extruded_disc", [0.0, 0.0, 0.0], [2.0, 0.0, 0.0]),
        ("revolved_orb", [0.0, 0.0, 0.0], [2.0, 0.0, 0.0]),
        ("swept_beam", [0.0, 0.0, 0.0], [0.0, 2.0, 0.0]),
        ("lofted_form", [0.0, 0.0, 0.0], [2.0, 0.0, 0.0]),
    ] {
        let center_distance = execute_capture_query(
            &ctx,
            &distance_plan,
            &[
                KernelValue::Capture(SmolStr::new(field_name)),
                KernelValue::Vec3(center),
            ],
        )
        .unwrap_or_else(|error| panic!("center distance for {field_name} failed: {error}"));
        let far_distance = execute_capture_query(
            &ctx,
            &distance_plan,
            &[
                KernelValue::Capture(SmolStr::new(field_name)),
                KernelValue::Vec3(far),
            ],
        )
        .unwrap_or_else(|error| panic!("far distance for {field_name} failed: {error}"));
        assert!(
            expect_f32(&center_distance) < 0.0,
            "{field_name} center should be inside"
        );
        assert!(
            expect_f32(&far_distance) > 0.0,
            "{field_name} far point should be outside"
        );
    }
}

#[test]
fn query_exec_opaque_fields_use_authored_bounds_fallback() {
    let (_, _, ctx) = typed_query_module(opaque_fallback_source());
    let distance_plan = lower_capture_query_plan(
        &CaptureQueryPlan::for_query(CaptureQueryKind::Distance, CaptureKind::Field, None)
            .expect("distance plan"),
    );

    let inside = execute_capture_query(
        &ctx,
        &distance_plan,
        &[
            KernelValue::Capture(SmolStr::new("opaque_field")),
            KernelValue::Vec3([0.0, 0.0, 0.0]),
        ],
    )
    .expect("inside distance");
    let outside = execute_capture_query(
        &ctx,
        &distance_plan,
        &[
            KernelValue::Capture(SmolStr::new("opaque_field")),
            KernelValue::Vec3([2.0, 0.0, 0.0]),
        ],
    )
    .expect("outside distance");

    assert_approx_eq(expect_f32(&inside), -1.0);
    assert_approx_eq(expect_f32(&outside), 1.0);
}
