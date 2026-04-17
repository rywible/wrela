use smol_str::SmolStr;
use std::env;
use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, OnceLock};
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
    WGSL_WORKGROUP_SIZE_OVERRIDE_ENV, clear_native_wgsl_test_caches, executable_region_shape_lists,
    execute_batch_query_with_trace, execute_batch_query_with_trace_on, execute_capture_query,
    execute_capture_query_on, execute_capture_query_with_trace_on, execute_world_query,
    execute_world_query_on, execute_world_query_with_policy_with_trace_on,
    execute_world_query_with_trace_on, render_semantic_cost_report,
    select_query_wgsl_workgroup_size, stable_field_scene_capture_id,
    stable_region_scene_capture_id, stable_shape_capture_id, stable_shape_scene_capture_id,
};
use wrela::query_plan::{
    ArtifactSchema, BatchQueryKind, BatchQueryPlan, CaptureKind, CaptureQueryKind,
    CaptureQueryPlan, DispatchBackend, WorldQueryKind, WorldQueryPlan,
};
use wrela::query_solver::{RaySolverMethod, StepCertificateKind};

fn lower_inline_module_from_source(source: &str) -> hir::Module {
    let node = parse(source);
    let root = ast::Root::cast(node).expect("root");
    hir_lower::lower(root)
}

struct EnvVarGuard {
    key: &'static str,
    previous: Option<String>,
}

impl EnvVarGuard {
    fn set(key: &'static str, value: &str) -> Self {
        let previous = env::var(key).ok();
        unsafe { env::set_var(key, value) };
        Self { key, previous }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        if let Some(previous) = self.previous.as_ref() {
            unsafe { env::set_var(self.key, previous) };
        } else {
            unsafe { env::remove_var(self.key) };
        }
    }
}

fn workgroup_override_test_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
}

fn wgsl_resident_cache_test_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
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
        delta < 0.03,
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

fn assert_query_summary_approx_eq(lhs: &KernelValue, rhs: &KernelValue) {
    let lhs = expect_struct(lhs, "QuerySummary");
    let rhs = expect_struct(rhs, "QuerySummary");
    assert_approx_eq(
        expect_f32(field(lhs, "distance")),
        expect_f32(field(rhs, "distance")),
    );
    assert_approx_eq(
        expect_f32(field(lhs, "world_distance")),
        expect_f32(field(rhs, "world_distance")),
    );
    assert_approx_eq(
        expect_f32(field(lhs, "batch_distance0")),
        expect_f32(field(rhs, "batch_distance0")),
    );
    assert_approx_eq(
        expect_f32(field(lhs, "batch_distance1")),
        expect_f32(field(rhs, "batch_distance1")),
    );
    assert_eq!(
        expect_bool(field(lhs, "occluded0")),
        expect_bool(field(rhs, "occluded0"))
    );
    assert_eq!(
        expect_bool(field(lhs, "scalar_occluded")),
        expect_bool(field(rhs, "scalar_occluded"))
    );
    assert_eq!(
        expect_bool(field(lhs, "world_occluded")),
        expect_bool(field(rhs, "world_occluded"))
    );
    assert_hit3_approx_eq(field(lhs, "hit"), field(rhs, "hit"));
    assert_hit3_approx_eq(field(lhs, "world_hit"), field(rhs, "world_hit"));
    assert_surface_approx_eq(field(lhs, "surface"), field(rhs, "surface"));
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

