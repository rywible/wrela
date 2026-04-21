use smol_str::SmolStr;
use std::env;
use std::sync::{Mutex, MutexGuard, OnceLock};
use wrela::hir;
use wrela::hir::lower as hir_lower;
use wrela::kernel::{KernelStructValue, KernelValue};
use wrela::parser::ast;
use wrela::parser::ast::AstNode;
use wrela::parser::parse;
use wrela::presentation_contract::{
    CanonicalCameraInput, CanonicalLightInput, CanonicalViewportInput, PresentationLightingInputs,
    QualityDegradationStep, RealtimeQualityContract, RealtimeQualityTier,
};
use wrela::presentation_exec::{
    AdaptivePresentationController, AdaptivePresentationSession, PresentationExecutionInput,
    PresentationExecutionPolicy, RayBudgetPolicy, execute_plan, frame_state_value,
    frame_state_value_with_history, frame_state_value_with_temporal_context, scene_domain_value,
    select_presentation_workgroup_size,
};
use wrela::presentation_plan::{PresentationPassKind, PresentationPlan};
use wrela::query_exec::wgsl::override_shader_f16_for_current_thread;
use wrela::query_exec::{
    QueryExecContext, QueryTraceSolverMode, WGSL_WORKGROUP_SIZE_OVERRIDE_ENV,
    stable_region_scene_capture_id, stable_region_snapshot_handle,
};
use wrela::query_plan::DispatchBackend;
use wrela::query_solver::RaySolverIntentDisposition;
use wrela::semantic_evidence::FactAvailability;
use wrela::world_identity::SnapshotEpoch;

fn lower_inline_module(source: &str) -> hir::Module {
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

fn typed_module(source: &str) -> (hir::Module, hir::TypeInfo, QueryExecContext) {
    let module = lower_inline_module(source);
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

fn view_function<'a>(module: &'a hir::Module, name: &str) -> &'a hir::Function {
    module
        .functions
        .iter()
        .find(|(_, func)| func.name == name)
        .map(|(_, func)| func)
        .unwrap_or_else(|| panic!("missing view function '{name}'"))
}

fn presentation_exec_source() -> &'static str {
    r#"
field exact distance exec_field(p: Vec3) -> F32 {
    sphere(radius = 0.6)
}

material exec_material(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(0.7, 0.4, 0.2),
        roughness=0.35,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

shape exec_shape {
    field = exec_field
    material = exec_material
    payload = Payload(
        entity_id=u32(7),
        material_id=u32(9),
        actor=ActorHandle(id=u32(11), generation=u32(0))
    )
}

region exec_region() {
    place scene = exec_shape
}

domain exec_domain(world: RegionCapture) {
    geometry_detail = 1
    material = true
    radiance = false
    media = false
    max_distance = 8.0
    min_step = 0.02
    hit_epsilon = 0.0005
    max_steps = 128
}

view exec_view(world: RegionCapture, camera: Camera) {
    domain = exec_domain(world = world)
    width = 4
    height = 4
    key_light = Light(
        position = vec3(1.8, 2.4, 2.2),
        direction = normalize(vec3(-0.5, -0.8, -0.6)),
        intensity = vec3(1.0, 0.98, 0.95),
        range = 8.0
    )
    fill_direction = normalize(vec3(-0.7, 0.45, 0.2))
    fill_strength = 0.22
    ambient_color = vec3(0.12, 0.12, 0.12)
}
"#
}

fn normalize_vec3(value: [f32; 3]) -> [f32; 3] {
    let len_sq = value[0] * value[0] + value[1] * value[1] + value[2] * value[2];
    let inv_len = len_sq.sqrt().recip();
    [value[0] * inv_len, value[1] * inv_len, value[2] * inv_len]
}

fn presentation_execution_policy(max_steps: i32) -> PresentationExecutionPolicy {
    PresentationExecutionPolicy::conservative(RayBudgetPolicy {
        max_distance: 8.0,
        min_step: 0.02,
        hit_epsilon: 0.0005,
        max_steps,
    })
}

fn presentation_fixture(
    backend: DispatchBackend,
) -> (
    PresentationPlan,
    QueryExecContext,
    PresentationExecutionInput,
) {
    let (module, _type_info, ctx) = typed_module(presentation_exec_source());
    let view = view_function(&module, "exec_view");
    let plan = PresentationPlan::from_view_function(view, backend).expect("presentation plan");
    let camera = CanonicalCameraInput {
        position: [0.0, 0.0, 2.0],
        forward: [0.0, 0.0, -1.0],
        up: [0.0, 1.0, 0.0],
        vertical_fov_degrees: 75.0,
    };
    let viewport = CanonicalViewportInput {
        width: 4,
        height: 4,
    };
    let input = PresentationExecutionInput {
        region_snapshot: stable_region_snapshot_handle(&SmolStr::new("exec_region")),
        frame_domain: scene_domain_value(
            stable_region_scene_capture_id(&SmolStr::new("exec_region")),
            1,
            true,
            false,
            false,
        ),
        frame_state: frame_state_value(camera, camera, viewport, [0.0, 0.0], 0, 1.0 / 60.0),
        history: None,
        resident_history_attachments: None,
        materialize_cpu_attachments: true,
        runtime_summary_only: false,
        collect_gpu_timing_readback: true,
        lighting: PresentationLightingInputs {
            key_light: CanonicalLightInput {
                position: [1.8, 2.4, 2.2],
                direction: normalize_vec3([-0.5, -0.8, -0.6]),
                intensity: [1.0, 0.98, 0.95],
                range: 8.0,
            },
            fill_direction: normalize_vec3([-0.7, 0.45, 0.2]),
            fill_strength: 0.22,
            ambient_color: [0.12, 0.12, 0.12],
        },
        compatibility_projection: None,
        execution_policy: presentation_execution_policy(128),
        query_trace_solver_mode: QueryTraceSolverMode::Hybrid,
        quality_override: None,
        backend,
    };
    (plan, ctx, input)
}

fn strip_export_passes(plan: &mut PresentationPlan) {
    let export_bindings = plan
        .passes
        .iter()
        .filter_map(|pass| match pass.kind {
            PresentationPassKind::ExportAttachment { .. } => pass.binding.clone(),
            _ => None,
        })
        .collect::<Vec<_>>();
    plan.passes
        .retain(|pass| !matches!(pass.kind, PresentationPassKind::ExportAttachment { .. }));
    plan.bindings
        .retain(|binding| !export_bindings.iter().any(|id| binding.id == *id));
}

fn presentation_fixture_with_state(
    source: &str,
    view_name: &str,
    region_name: &str,
    backend: DispatchBackend,
    viewport: CanonicalViewportInput,
    camera: CanonicalCameraInput,
    previous_camera: CanonicalCameraInput,
    frame_index: u32,
    previous_frame_index: u32,
    history_reset: bool,
    current_snapshot_epoch: SnapshotEpoch,
    previous_snapshot_epoch: SnapshotEpoch,
    history: Option<wrela::presentation_exec::PresentationTemporalHistory>,
) -> (
    PresentationPlan,
    QueryExecContext,
    PresentationExecutionInput,
) {
    let (module, _type_info, ctx) = typed_module(source);
    let view = view_function(&module, view_name);
    let plan = PresentationPlan::from_view_function(view, backend).expect("presentation plan");
    let input = PresentationExecutionInput {
        region_snapshot: stable_region_snapshot_handle(&SmolStr::new(region_name)),
        frame_domain: scene_domain_value(
            stable_region_scene_capture_id(&SmolStr::new(region_name)),
            1,
            true,
            false,
            false,
        ),
        frame_state: frame_state_value_with_history(
            camera,
            previous_camera,
            viewport,
            viewport,
            [0.0, 0.0],
            [0.0, 0.0],
            frame_index,
            previous_frame_index,
            1.0 / 60.0,
            history_reset,
            current_snapshot_epoch,
            previous_snapshot_epoch,
        ),
        history,
        resident_history_attachments: None,
        materialize_cpu_attachments: true,
        runtime_summary_only: false,
        collect_gpu_timing_readback: true,
        lighting: PresentationLightingInputs {
            key_light: CanonicalLightInput {
                position: [1.8, 2.4, 2.2],
                direction: normalize_vec3([-0.5, -0.8, -0.6]),
                intensity: [1.0, 0.98, 0.95],
                range: 8.0,
            },
            fill_direction: normalize_vec3([-0.7, 0.45, 0.2]),
            fill_strength: 0.22,
            ambient_color: [0.12, 0.12, 0.12],
        },
        compatibility_projection: None,
        execution_policy: presentation_execution_policy(128),
        query_trace_solver_mode: QueryTraceSolverMode::Hybrid,
        quality_override: None,
        backend,
    };
    (plan, ctx, input)
}


fn hit_flag(value: &KernelValue) -> bool {
    match field(expect_struct(value, "Hit3"), "hit") {
        KernelValue::Bool(value) => *value,
        other => panic!("expected hit bool, got {other:?}"),
    }
}

fn distance_value(value: &KernelValue) -> f32 {
    match field(expect_struct(value, "Hit3"), "distance") {
        KernelValue::F32(value) => *value,
        other => panic!("expected distance f32, got {other:?}"),
    }
}

fn position_value(value: &KernelValue) -> [f32; 3] {
    match field(expect_struct(value, "Hit3"), "position") {
        KernelValue::Vec3(value) => *value,
        other => panic!("expected position vec3, got {other:?}"),
    }
}

fn normal_from_hit(value: &KernelValue) -> [f32; 3] {
    match field(expect_struct(value, "Hit3"), "normal") {
        KernelValue::Vec3(value) => *value,
        other => panic!("expected normal vec3, got {other:?}"),
    }
}

fn root_shape_id(value: &KernelValue) -> u32 {
    match field(expect_struct(value, "Hit3"), "root_shape_id") {
        KernelValue::U32(value) => *value,
        other => panic!("expected root_shape_id u32, got {other:?}"),
    }
}

fn payload_entity_id(value: &KernelValue) -> u32 {
    match field(expect_struct(payload_value(value), "Payload"), "entity_id") {
        KernelValue::U32(value) => *value,
        other => panic!("expected payload entity_id u32, got {other:?}"),
    }
}

fn payload_material_id(value: &KernelValue) -> u32 {
    match field(
        expect_struct(payload_value(value), "Payload"),
        "material_id",
    ) {
        KernelValue::U32(value) => *value,
        other => panic!("expected payload material_id u32, got {other:?}"),
    }
}

fn payload_actor_id(value: &KernelValue) -> u32 {
    match field(
        expect_struct(
            field(expect_struct(payload_value(value), "Payload"), "actor"),
            "ActorHandle",
        ),
        "id",
    ) {
        KernelValue::U32(value) => *value,
        other => panic!("expected payload actor id u32, got {other:?}"),
    }
}

fn depth_value(value: &KernelValue) -> f32 {
    match value {
        KernelValue::F32(depth) => *depth,
        other => panic!("expected depth f32, got {other:?}"),
    }
}

fn normal_value(value: &KernelValue) -> [f32; 3] {
    match value {
        KernelValue::Vec3(normal) => *normal,
        other => panic!("expected normal vec3, got {other:?}"),
    }
}

fn surface_albedo(value: &KernelValue) -> [f32; 3] {
    match field(expect_struct(value, "Surface"), "albedo") {
        KernelValue::Vec3(value) => *value,
        other => panic!("expected albedo vec3, got {other:?}"),
    }
}

fn medium_density(value: &KernelValue) -> f32 {
    match field(expect_struct(value, "Medium"), "density") {
        KernelValue::F32(value) => *value,
        other => panic!("expected medium density f32, got {other:?}"),
    }
}

fn radiance_value(value: &KernelValue) -> [f32; 3] {
    match value {
        KernelValue::Vec3(value) => *value,
        other => panic!("expected radiance vec3, got {other:?}"),
    }
}

fn color_value(value: &KernelValue) -> [f32; 3] {
    match value {
        KernelValue::Vec3(value) => *value,
        other => panic!("expected color vec3, got {other:?}"),
    }
}

fn motion_valid(value: &KernelValue) -> bool {
    match field(expect_struct(value, "MotionVector"), "valid") {
        KernelValue::Bool(value) => *value,
        other => panic!("expected motion valid bool, got {other:?}"),
    }
}

fn motion_disoccluded(value: &KernelValue) -> bool {
    match field(expect_struct(value, "MotionVector"), "disoccluded") {
        KernelValue::Bool(value) => *value,
        other => panic!("expected motion disoccluded bool, got {other:?}"),
    }
}

fn max_lane(value: [f32; 3]) -> f32 {
    value[0].max(value[1]).max(value[2])
}

fn expect_struct<'a>(value: &'a KernelValue, name: &str) -> &'a KernelStructValue {
    match value {
        KernelValue::Struct(struct_value) if struct_value.name == name => struct_value,
        other => panic!("expected {name} struct, got {other:?}"),
    }
}

fn field<'a>(value: &'a KernelStructValue, name: &str) -> &'a KernelValue {
    value
        .fields
        .iter()
        .find_map(|(field_name, field_value)| (field_name == name).then_some(field_value))
        .unwrap_or_else(|| panic!("missing field {name} on {}", value.name))
}

fn u32_field(value: &KernelStructValue, name: &str) -> u32 {
    match field(value, name) {
        KernelValue::U32(value) => *value,
        other => panic!("expected u32 field {name}, got {other:?}"),
    }
}

fn payload_value<'a>(value: &'a KernelValue) -> &'a KernelValue {
    field(expect_struct(value, "Hit3"), "payload")
}

fn assert_approx_eq(actual: f32, expected: f32, label: &str) {
    assert!(
        (actual - expected).abs() <= 1.0e-3,
        "{label} mismatch: {actual} != {expected}"
    );
}

fn assert_vec3_approx_eq(actual: [f32; 3], expected: [f32; 3], label: &str) {
    assert_vec3_approx_eq_tol(actual, expected, label, 1.0e-3);
}

fn assert_vec3_approx_eq_tol(actual: [f32; 3], expected: [f32; 3], label: &str, tolerance: f32) {
    for (actual, expected) in actual.iter().zip(expected) {
        assert!(
            (actual - expected).abs() <= tolerance,
            "{label} mismatch: {actual} != {expected}"
        );
    }
}

fn assert_attachment_vec3_approx_eq(
    actual: &[KernelValue],
    expected: &[KernelValue],
    tolerance: f32,
    label: &str,
) {
    assert_eq!(actual.len(), expected.len(), "{label} length drift");
    for (actual, expected) in actual.iter().zip(expected) {
        assert_vec3_approx_eq_tol(color_value(actual), color_value(expected), label, tolerance);
    }
}

fn mean_color_delta(lhs: &[KernelValue], rhs: &[KernelValue]) -> f32 {
    assert_eq!(lhs.len(), rhs.len(), "color attachment size drift");
    let mut total = 0.0f32;
    let mut count = 0.0f32;
    for (lhs, rhs) in lhs.iter().zip(rhs) {
        for (lhs, rhs) in color_value(lhs).iter().zip(color_value(rhs)) {
            total += (lhs - rhs).abs();
            count += 1.0;
        }
    }
    total / count.max(1.0)
}
