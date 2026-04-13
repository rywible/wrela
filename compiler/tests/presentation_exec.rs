use smol_str::SmolStr;
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
};
use wrela::presentation_plan::{PresentationPassKind, PresentationPlan};
use wrela::query_exec::{
    QueryExecContext, stable_region_scene_capture_id, stable_region_snapshot_handle,
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
        quality_override: None,
        backend,
    };
    (plan, ctx, input)
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
        ),
        history,
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
        quality_override: None,
        backend,
    };
    (plan, ctx, input)
}

#[test]
fn scene_domain_value_does_not_emit_placeholder_guarantee() {
    let domain = scene_domain_value(7, 1, true, false, false);
    let domain = match domain {
        KernelValue::Struct(value) => value,
        other => panic!("expected scene domain struct, found {:?}", other),
    };
    let spatial = match domain
        .fields
        .iter()
        .find(|(name, _)| name.as_str() == "spatial")
        .map(|(_, value)| value)
    {
        Some(KernelValue::Struct(value)) => value,
        Some(other) => panic!("expected spatial struct, found {:?}", other),
        None => panic!("missing spatial field"),
    };
    let field_names = spatial
        .fields
        .iter()
        .map(|(name, _)| name.to_string())
        .collect::<Vec<_>>();
    assert!(field_names.iter().any(|name| name == "geometry_detail"));
    assert!(
        !field_names.iter().any(|name| name == "guarantee"),
        "scene_domain_value must not emit placeholder guarantee"
    );
}

fn temporal_alias_source() -> &'static str {
    r#"
field exact distance alias_field(p: Vec3) -> F32 {
    translate = vec3(0.18, 0.0, 0.0) {
        capsule(a = vec3(0.0, -0.82, 0.0), b = vec3(0.0, 0.82, 0.0), radius = 0.12)
    }
}

material alias_material(hit: Hit3) -> Surface {
    band = clamp(0.5 + hit.local_position.x * 4.5 + hit.local_position.y * 0.35, 0.0, 1.0)
    rim = clamp(hit.local_normal.x * 0.5 + 0.5, 0.0, 1.0)
    return Surface(
        albedo=vec3(0.18, 0.10, 0.04) + vec3(0.55, 0.62, 0.18) * band,
        roughness=0.14 + abs(hit.local_position.z) * 0.18,
        metalness=0.02 + rim * 0.08,
        clearcoat=0.08 + rim * 0.14,
        clearcoat_roughness=0.06 + (1.0 - rim) * 0.10,
        sheen=0.05 + abs(hit.local_normal.y) * 0.12,
        emissive=vec3(0.08, 0.05, 0.0) * rim
    )
}

shape alias_shape {
    field = alias_field
    material = alias_material
    payload = Payload(
        entity_id=u32(17),
        material_id=u32(23),
        actor=ActorHandle(id=u32(29), generation=u32(0))
    )
}

region alias_region() {
    place scene = alias_shape
}

domain alias_domain(world: RegionCapture) {
    geometry_detail = 1
    material = true
    radiance = false
    media = false
    max_distance = 8.0
    min_step = 0.01
    hit_epsilon = 0.0004
    max_steps = 128
}

view alias_view(world: RegionCapture, camera: Camera) {
    domain = alias_domain(world = world)
    width = 16
    height = 16
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

fn temporal_disocclusion_source() -> &'static str {
    r#"
field exact distance disocclusion_field(p: Vec3) -> F32 {
    translate = vec3(1.45, 0.0, 0.0) {
        capsule(a = vec3(0.0, -0.7, 0.0), b = vec3(0.0, 0.7, 0.0), radius = 0.12)
    }
}

material disocclusion_material(hit: Hit3) -> Surface {
    band = clamp(0.5 + hit.local_position.x * 5.5, 0.0, 1.0)
    return Surface(
        albedo=vec3(0.22, 0.14, 0.06) + vec3(0.45, 0.38, 0.12) * band,
        roughness=0.18,
        metalness=0.02,
        clearcoat=0.08,
        clearcoat_roughness=0.08,
        sheen=0.04,
        emissive=vec3(0.02, 0.01, 0.0)
    )
}

shape disocclusion_shape {
    field = disocclusion_field
    material = disocclusion_material
    payload = Payload(
        entity_id=u32(31),
        material_id=u32(37),
        actor=ActorHandle(id=u32(41), generation=u32(0))
    )
}

region disocclusion_region() {
    place scene = disocclusion_shape
}

domain disocclusion_domain(world: RegionCapture) {
    geometry_detail = 1
    material = true
    radiance = false
    media = false
    max_distance = 8.0
    min_step = 0.01
    hit_epsilon = 0.0004
    max_steps = 128
}

view disocclusion_view(world: RegionCapture, camera: Camera) {
    domain = disocclusion_domain(world = world)
    width = 24
    height = 24
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

#[test]
fn cpu_first_color_path_materializes_surface_participants_and_color_attachments() {
    let (plan, ctx, input) = presentation_fixture(DispatchBackend::Cpu);
    let result = execute_plan(&ctx, &plan, &input).expect("cpu presentation execution");

    assert_eq!(result.width, 4);
    assert_eq!(result.height, 4);
    assert!(
        result
            .frame_cost
            .execution_policy
            .contains("required_guarantee=conservative_no_false_miss")
    );
    assert!(
        result
            .frame_cost
            .execution_policy
            .contains("selected_method=conservative_solver")
    );
    assert_eq!(result.metrics.sample_count, 16);
    assert_eq!(result.metrics.hit_count + result.metrics.miss_count, 16);
    assert!(result.metrics.candidates_before_pruning >= 16);
    assert!(result.metrics.trace_steps_max > 0);
    assert!(result.metrics.dense_fallback_count > 0);
    assert_eq!(
        result
            .metrics
            .solver_summary
            .as_ref()
            .map(|summary| summary.plan_id.as_str()),
        Some("ray-solver:spatial.nearest.batch.world:v1")
    );

    let primary_hits = result
        .attachments
        .decode_attachment("primary_hit")
        .expect("primary_hit attachment");
    let depths = result
        .attachments
        .decode_attachment("depth")
        .expect("depth attachment");
    let normals = result
        .attachments
        .decode_attachment("world_normal")
        .expect("world_normal attachment");
    let surfaces = result
        .attachments
        .decode_attachment("surface")
        .expect("surface attachment");
    let radiance = result
        .attachments
        .decode_attachment("radiance")
        .expect("radiance attachment");
    let medium = result
        .attachments
        .decode_attachment("medium")
        .expect("medium attachment");
    let shaded = result
        .attachments
        .decode_attachment("shaded_color")
        .expect("shaded_color attachment");
    let motion = result
        .attachments
        .decode_attachment("motion")
        .expect("motion attachment");
    let history_color = result
        .attachments
        .decode_attachment("history_color")
        .expect("history_color attachment");
    let history_primary_hit = result
        .attachments
        .decode_attachment("history_primary_hit")
        .expect("history_primary_hit attachment");
    let color = result
        .attachments
        .decode_attachment("color")
        .expect("color attachment");
    assert_eq!(primary_hits.len(), 16);
    assert_eq!(depths.len(), 16);
    assert_eq!(normals.len(), 16);
    assert_eq!(surfaces.len(), 16);
    assert_eq!(radiance.len(), 16);
    assert_eq!(medium.len(), 16);
    assert_eq!(shaded.len(), 16);
    assert_eq!(motion.len(), 16);
    assert_eq!(history_color.len(), 16);
    assert_eq!(history_primary_hit.len(), 16);
    assert_eq!(color.len(), 16);
    assert!(result.history.is_some());
    assert_eq!(
        result.history.as_ref().map(|history| (
            history.snapshot.capture_name.as_str(),
            history.snapshot.epoch.0
        )),
        Some(("exec_region", 1))
    );
    assert!(result.metrics.continuation_unavailable_count > 0);

    assert!(!hit_flag(&primary_hits[0]));
    assert!(depth_value(&depths[0]).is_infinite());
    assert_eq!(normal_value(&normals[0]), [0.0, 0.0, 0.0]);
    assert_eq!(surface_albedo(&surfaces[0]), [0.0, 0.0, 0.0]);
    assert_eq!(medium_density(&medium[0]), 0.0);
    assert!(max_lane(color_value(&color[0])) >= 0.0);

    assert!(hit_flag(&primary_hits[5]));
    assert!(depth_value(&depths[5]).is_finite());
    assert_ne!(root_shape_id(&primary_hits[5]), 0);
    assert_eq!(payload_entity_id(&primary_hits[5]), 7);
    assert_eq!(payload_material_id(&primary_hits[5]), 9);
    assert_eq!(payload_actor_id(&primary_hits[5]), 11);
    assert_ne!(normal_value(&normals[5]), [0.0, 0.0, 0.0]);
    assert_vec3_approx_eq(
        surface_albedo(&surfaces[5]),
        [0.7, 0.4, 0.2],
        "surface.albedo",
    );
    assert!(max_lane(radiance_value(&radiance[5])) >= 0.0);
    assert_eq!(medium_density(&medium[5]), 0.0);
    assert!(max_lane(color_value(&color[5])) > 0.0);
    assert!(!motion_valid(&motion[5]));
    assert!(!motion_disoccluded(&motion[0]));
}

#[test]
fn wgsl_first_color_path_matches_cpu_for_final_color_and_semantic_attachments() {
    let (cpu_plan, cpu_ctx, cpu_input) = presentation_fixture(DispatchBackend::Cpu);
    let (wgsl_plan, wgsl_ctx, wgsl_input) = presentation_fixture(DispatchBackend::Wgsl);

    let cpu = execute_plan(&cpu_ctx, &cpu_plan, &cpu_input).expect("cpu presentation execution");
    let wgsl =
        execute_plan(&wgsl_ctx, &wgsl_plan, &wgsl_input).expect("wgsl presentation execution");

    assert_eq!(cpu.width, wgsl.width);
    assert_eq!(cpu.height, wgsl.height);
    assert_eq!(cpu.metrics.sample_count, wgsl.metrics.sample_count);
    assert_eq!(cpu.metrics.hit_count, wgsl.metrics.hit_count);
    assert_eq!(cpu.metrics.miss_count, wgsl.metrics.miss_count);

    for (cpu_hit, wgsl_hit) in cpu
        .attachments
        .decode_attachment("primary_hit")
        .expect("cpu primary hit")
        .iter()
        .zip(
            wgsl.attachments
                .decode_attachment("primary_hit")
                .expect("wgsl primary hit")
                .iter(),
        )
    {
        assert_eq!(hit_flag(cpu_hit), hit_flag(wgsl_hit));
        assert_approx_eq(
            distance_value(cpu_hit),
            distance_value(wgsl_hit),
            "distance",
        );
        assert_vec3_approx_eq(
            position_value(cpu_hit),
            position_value(wgsl_hit),
            "position",
        );
        assert_vec3_approx_eq(
            normal_from_hit(cpu_hit),
            normal_from_hit(wgsl_hit),
            "normal",
        );
        assert_eq!(root_shape_id(cpu_hit), root_shape_id(wgsl_hit));
        assert_eq!(payload_entity_id(cpu_hit), payload_entity_id(wgsl_hit));
        assert_eq!(payload_material_id(cpu_hit), payload_material_id(wgsl_hit));
        assert_eq!(payload_actor_id(cpu_hit), payload_actor_id(wgsl_hit));
    }

    for (cpu_depth, wgsl_depth) in cpu
        .attachments
        .decode_attachment("depth")
        .expect("cpu depth")
        .iter()
        .zip(
            wgsl.attachments
                .decode_attachment("depth")
                .expect("wgsl depth")
                .iter(),
        )
    {
        let cpu_depth = depth_value(cpu_depth);
        let wgsl_depth = depth_value(wgsl_depth);
        if cpu_depth.is_infinite() || wgsl_depth.is_infinite() {
            assert!(cpu_depth.is_infinite());
            assert!(wgsl_depth.is_infinite());
        } else {
            assert_approx_eq(cpu_depth, wgsl_depth, "depth");
        }
    }

    for (cpu_normal, wgsl_normal) in cpu
        .attachments
        .decode_attachment("world_normal")
        .expect("cpu world normal")
        .iter()
        .zip(
            wgsl.attachments
                .decode_attachment("world_normal")
                .expect("wgsl world normal")
                .iter(),
        )
    {
        assert_vec3_approx_eq(
            normal_value(cpu_normal),
            normal_value(wgsl_normal),
            "world_normal",
        );
    }

    for (cpu_surface, wgsl_surface) in cpu
        .attachments
        .decode_attachment("surface")
        .expect("cpu surface")
        .iter()
        .zip(
            wgsl.attachments
                .decode_attachment("surface")
                .expect("wgsl surface")
                .iter(),
        )
    {
        assert_vec3_approx_eq(
            surface_albedo(cpu_surface),
            surface_albedo(wgsl_surface),
            "surface.albedo",
        );
    }

    for (cpu_color, wgsl_color) in cpu
        .attachments
        .decode_attachment("color")
        .expect("cpu color")
        .iter()
        .zip(
            wgsl.attachments
                .decode_attachment("color")
                .expect("wgsl color")
                .iter(),
        )
    {
        assert_vec3_approx_eq_tol(
            color_value(cpu_color),
            color_value(wgsl_color),
            "color",
            1.0e-2,
        );
    }
}

#[test]
fn static_repeated_frames_reuse_history_deterministically() {
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
    let (plan0, ctx0, input0) = presentation_fixture_with_state(
        presentation_exec_source(),
        "exec_view",
        "exec_region",
        DispatchBackend::Cpu,
        viewport,
        camera,
        camera,
        0,
        0,
        true,
        None,
    );
    let frame0 = execute_plan(&ctx0, &plan0, &input0).expect("first temporal frame");

    let (plan1, ctx1, input1) = presentation_fixture_with_state(
        presentation_exec_source(),
        "exec_view",
        "exec_region",
        DispatchBackend::Cpu,
        viewport,
        camera,
        camera,
        1,
        0,
        false,
        frame0.history.clone(),
    );
    let frame1 = execute_plan(&ctx1, &plan1, &input1).expect("second temporal frame");

    assert_eq!(
        frame0.attachments.decode_attachment("color").unwrap(),
        frame1.attachments.decode_attachment("color").unwrap()
    );
    assert!(frame1.metrics.continuation_available_count > 0);
    assert!(frame1.metrics.continuation_consumed_count > 0);
    let solver_summary = frame1
        .metrics
        .solver_summary
        .as_ref()
        .expect("solver summary");
    assert_eq!(
        solver_summary.artifact_reuse_intents[0].disposition,
        RaySolverIntentDisposition::Used
    );
    assert_eq!(
        solver_summary.continuation_intents[0].disposition,
        RaySolverIntentDisposition::Used
    );
    assert!(
        solver_summary.artifact_reuse_intents[0]
            .reasons
            .iter()
            .any(|reason| reason.contains("capture-cache") || reason.contains("support-summary"))
    );
    assert!(
        solver_summary.continuation_intents[0]
            .reasons
            .iter()
            .any(|reason| reason.contains("verdict=available"))
    );
    assert!(
        frame1
            .metrics
            .continuation_diagnostics
            .iter()
            .any(|entry| entry.contains("verdict=available")
                && entry.contains("change_class=stable"))
    );
}

#[test]
fn epoch_compatible_transition_reuses_history_when_previous_snapshot_matches() {
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

    let (plan0, ctx0, mut input0) = presentation_fixture_with_state(
        presentation_exec_source(),
        "exec_view",
        "exec_region",
        DispatchBackend::Cpu,
        viewport,
        camera,
        camera,
        0,
        0,
        true,
        None,
    );
    input0.region_snapshot = stable_region_snapshot_handle(&SmolStr::new("exec_region"));
    let frame0 = execute_plan(&ctx0, &plan0, &input0).expect("seed epoch frame");

    let (plan1, ctx1, mut input1) = presentation_fixture_with_state(
        presentation_exec_source(),
        "exec_view",
        "exec_region",
        DispatchBackend::Cpu,
        viewport,
        camera,
        camera,
        1,
        0,
        false,
        frame0.history.clone(),
    );
    input1.region_snapshot =
        stable_region_snapshot_handle(&SmolStr::new("exec_region")).with_epoch(SnapshotEpoch(2));
    input1.frame_state = frame_state_value_with_temporal_context(
        camera,
        camera,
        viewport,
        viewport,
        [0.0, 0.0],
        [0.0, 0.0],
        1,
        0,
        1.0 / 60.0,
        false,
        1,
        0,
        1,
        1.0 / 60.0,
        SnapshotEpoch(2),
        SnapshotEpoch(1),
        true,
        0,
        true,
        false,
        false,
    );
    let frame1 = execute_plan(&ctx1, &plan1, &input1).expect("epoch-compatible reuse");

    assert!(frame1.metrics.continuation_available_count > 0);
    assert!(frame1.metrics.continuation_consumed_count > 0);
    assert!(
        frame1
            .metrics
            .continuation_diagnostics
            .iter()
            .any(|entry| entry.contains("expected_previous_epoch=1 history_epoch=1"))
    );
}

#[test]
fn topology_change_rejects_history_even_when_snapshot_epochs_line_up() {
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

    let (plan0, ctx0, mut input0) = presentation_fixture_with_state(
        presentation_exec_source(),
        "exec_view",
        "exec_region",
        DispatchBackend::Cpu,
        viewport,
        camera,
        camera,
        0,
        0,
        true,
        None,
    );
    input0.region_snapshot = stable_region_snapshot_handle(&SmolStr::new("exec_region"));
    let frame0 = execute_plan(&ctx0, &plan0, &input0).expect("seed topology frame");

    let (plan1, ctx1, mut input1) = presentation_fixture_with_state(
        presentation_exec_source(),
        "exec_view",
        "exec_region",
        DispatchBackend::Cpu,
        viewport,
        camera,
        camera,
        1,
        0,
        false,
        frame0.history.clone(),
    );
    input1.region_snapshot =
        stable_region_snapshot_handle(&SmolStr::new("exec_region")).with_epoch(SnapshotEpoch(2));
    input1.frame_state = frame_state_value_with_temporal_context(
        camera,
        camera,
        viewport,
        viewport,
        [0.0, 0.0],
        [0.0, 0.0],
        1,
        0,
        1.0 / 60.0,
        false,
        1,
        0,
        1,
        1.0 / 60.0,
        SnapshotEpoch(2),
        SnapshotEpoch(1),
        true,
        3,
        false,
        true,
        false,
    );
    let frame1 = execute_plan(&ctx1, &plan1, &input1).expect("topology rejection");

    assert_eq!(frame1.metrics.continuation_consumed_count, 0);
    assert!(frame1.metrics.continuation_rejected_count > 0);
    assert!(frame1.metrics.continuation_diagnostics.iter().any(|entry| {
        entry.contains("reason=change-compatibility-mismatch")
            && entry.contains("expected_previous_epoch=1 history_epoch=1")
    }));
}

#[test]
fn typed_presentation_frame_history_age_ignores_legacy_frame_index() {
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

    let (plan0, ctx0, input0) = presentation_fixture_with_state(
        presentation_exec_source(),
        "exec_view",
        "exec_region",
        DispatchBackend::Cpu,
        viewport,
        camera,
        camera,
        0,
        0,
        true,
        None,
    );
    let frame0 = execute_plan(&ctx0, &plan0, &input0).expect("seed typed frame history");

    let (plan1, ctx1, mut input1) = presentation_fixture_with_state(
        presentation_exec_source(),
        "exec_view",
        "exec_region",
        DispatchBackend::Cpu,
        viewport,
        camera,
        camera,
        100,
        99,
        false,
        frame0.history.clone(),
    );
    input1.frame_state = frame_state_value_with_temporal_context(
        camera,
        camera,
        viewport,
        viewport,
        [0.0, 0.0],
        [0.0, 0.0],
        100,
        99,
        1.0 / 60.0,
        false,
        1,
        0,
        1,
        1.0 / 60.0,
        SnapshotEpoch(1),
        SnapshotEpoch(1),
        true,
        0,
        true,
        false,
        false,
    );
    let frame1 = execute_plan(&ctx1, &plan1, &input1).expect("typed presentation frame reuse");

    assert!(frame1.metrics.continuation_available_count > 0);
    assert!(frame1.metrics.continuation_consumed_count > 0);
    assert!(
        frame1
            .metrics
            .continuation_diagnostics
            .iter()
            .any(|entry| entry.contains("verdict=available"))
    );
}

#[test]
fn authoritative_incompatible_transition_summary_rejects_history() {
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

    let (plan0, ctx0, input0) = presentation_fixture_with_state(
        presentation_exec_source(),
        "exec_view",
        "exec_region",
        DispatchBackend::Cpu,
        viewport,
        camera,
        camera,
        0,
        0,
        true,
        None,
    );
    let frame0 = execute_plan(&ctx0, &plan0, &input0).expect("seed authoritative compatibility");

    let (plan1, ctx1, mut input1) = presentation_fixture_with_state(
        presentation_exec_source(),
        "exec_view",
        "exec_region",
        DispatchBackend::Cpu,
        viewport,
        camera,
        camera,
        1,
        0,
        false,
        frame0.history.clone(),
    );
    input1.frame_state = frame_state_value_with_temporal_context(
        camera,
        camera,
        viewport,
        viewport,
        [0.0, 0.0],
        [0.0, 0.0],
        1,
        0,
        1.0 / 60.0,
        false,
        1,
        0,
        1,
        1.0 / 60.0,
        SnapshotEpoch(2),
        SnapshotEpoch(1),
        true,
        1,
        false,
        false,
        false,
    );
    let frame1 = execute_plan(&ctx1, &plan1, &input1).expect("authoritative incompatibility");

    assert_eq!(frame1.metrics.continuation_consumed_count, 0);
    assert!(frame1.metrics.continuation_rejected_count > 0);
    assert!(frame1.metrics.continuation_diagnostics.iter().any(|entry| {
        entry.contains("reason=change-compatibility-mismatch")
            && entry.contains("change_class=camera-motion")
    }));
}

#[test]
fn temporal_evidence_requirements_reject_otherwise_compatible_camera_motion() {
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

    let (mut plan0, ctx0, input0) = presentation_fixture_with_state(
        presentation_exec_source(),
        "exec_view",
        "exec_region",
        DispatchBackend::Cpu,
        viewport,
        camera,
        camera,
        0,
        0,
        true,
        None,
    );
    plan0
        .frame
        .temporal
        .as_mut()
        .expect("temporal contract")
        .required_evidence
        .stationary = FactAvailability::Available;
    let frame0 = execute_plan(&ctx0, &plan0, &input0).expect("seed evidence gate");

    let (mut plan1, ctx1, mut input1) = presentation_fixture_with_state(
        presentation_exec_source(),
        "exec_view",
        "exec_region",
        DispatchBackend::Cpu,
        viewport,
        camera,
        camera,
        1,
        0,
        false,
        frame0.history.clone(),
    );
    plan1
        .frame
        .temporal
        .as_mut()
        .expect("temporal contract")
        .required_evidence
        .stationary = FactAvailability::Available;
    input1.frame_state = frame_state_value_with_temporal_context(
        camera,
        camera,
        viewport,
        viewport,
        [0.0, 0.0],
        [0.0, 0.0],
        1,
        0,
        1.0 / 60.0,
        false,
        1,
        0,
        1,
        1.0 / 60.0,
        SnapshotEpoch(2),
        SnapshotEpoch(1),
        true,
        1,
        true,
        false,
        false,
    );
    let frame1 = execute_plan(&ctx1, &plan1, &input1).expect("evidence mismatch rejection");

    assert_eq!(frame1.metrics.continuation_consumed_count, 0);
    assert!(frame1.metrics.continuation_rejected_count > 0);
    assert!(frame1.metrics.continuation_diagnostics.iter().any(|entry| {
        entry.contains("reason=temporal-evidence-mismatch")
            && entry.contains("change_class=camera-motion")
    }));
}

#[test]
fn temporal_evidence_requirements_apply_without_change_summary() {
    let camera_a = CanonicalCameraInput {
        position: [0.0, 0.0, 2.0],
        forward: [0.0, 0.0, -1.0],
        up: [0.0, 1.0, 0.0],
        vertical_fov_degrees: 75.0,
    };
    let mut camera_b = camera_a;
    camera_b.position = [0.18, 0.0, 2.0];
    camera_b.forward = normalize_vec3([-0.09, 0.0, -1.0]);
    let viewport = CanonicalViewportInput {
        width: 4,
        height: 4,
    };

    let (mut plan0, ctx0, input0) = presentation_fixture_with_state(
        presentation_exec_source(),
        "exec_view",
        "exec_region",
        DispatchBackend::Cpu,
        viewport,
        camera_a,
        camera_a,
        0,
        0,
        true,
        None,
    );
    plan0
        .frame
        .temporal
        .as_mut()
        .expect("temporal contract")
        .required_evidence
        .stationary = FactAvailability::Available;
    let frame0 = execute_plan(&ctx0, &plan0, &input0).expect("seed heuristic evidence gate");

    let (mut plan1, ctx1, input1) = presentation_fixture_with_state(
        presentation_exec_source(),
        "exec_view",
        "exec_region",
        DispatchBackend::Cpu,
        viewport,
        camera_b,
        camera_a,
        1,
        0,
        false,
        frame0.history.clone(),
    );
    plan1
        .frame
        .temporal
        .as_mut()
        .expect("temporal contract")
        .required_evidence
        .stationary = FactAvailability::Available;
    let frame1 = execute_plan(&ctx1, &plan1, &input1).expect("heuristic evidence mismatch");

    assert_eq!(frame1.metrics.continuation_consumed_count, 0);
    assert!(frame1.metrics.continuation_rejected_count > 0);
    assert!(frame1.metrics.continuation_diagnostics.iter().any(|entry| {
        entry.contains("reason=temporal-evidence-mismatch")
            && entry.contains("change_class=camera-motion")
    }));
}

#[test]
fn slow_camera_motion_reuses_history_and_wgsl_matches_cpu_temporal_resolve() {
    let camera_a = CanonicalCameraInput {
        position: [0.0, 0.0, 2.0],
        forward: [0.0, 0.0, -1.0],
        up: [0.0, 1.0, 0.0],
        vertical_fov_degrees: 75.0,
    };
    let mut camera_b = camera_a;
    camera_b.position = [0.18, 0.0, 2.0];
    camera_b.forward = normalize_vec3([-0.09, 0.0, -1.0]);
    let viewport = CanonicalViewportInput {
        width: 16,
        height: 16,
    };

    let (cpu_plan0, cpu_ctx0, cpu_input0) = presentation_fixture_with_state(
        temporal_alias_source(),
        "alias_view",
        "alias_region",
        DispatchBackend::Cpu,
        viewport,
        camera_a,
        camera_a,
        0,
        0,
        true,
        None,
    );
    let cpu_frame0 = execute_plan(&cpu_ctx0, &cpu_plan0, &cpu_input0).expect("cpu temporal seed");

    let (cpu_plan1, cpu_ctx1, cpu_input1) = presentation_fixture_with_state(
        temporal_alias_source(),
        "alias_view",
        "alias_region",
        DispatchBackend::Cpu,
        viewport,
        camera_b,
        camera_a,
        1,
        0,
        false,
        cpu_frame0.history.clone(),
    );
    let cpu_with_history =
        execute_plan(&cpu_ctx1, &cpu_plan1, &cpu_input1).expect("cpu temporal reuse");

    let (cpu_plan2, cpu_ctx2, cpu_input2) = presentation_fixture_with_state(
        temporal_alias_source(),
        "alias_view",
        "alias_region",
        DispatchBackend::Cpu,
        viewport,
        camera_b,
        camera_a,
        1,
        0,
        false,
        None,
    );
    let cpu_without_history =
        execute_plan(&cpu_ctx2, &cpu_plan2, &cpu_input2).expect("cpu fallback current frame");

    let frame0_color = cpu_frame0.attachments.decode_attachment("color").unwrap();
    let motion = cpu_with_history
        .attachments
        .decode_attachment("motion")
        .unwrap();
    let with_history_color = cpu_with_history
        .attachments
        .decode_attachment("color")
        .unwrap();
    let without_history_color = cpu_without_history
        .attachments
        .decode_attachment("color")
        .unwrap();
    let with_history_delta = mean_color_delta(&frame0_color, &with_history_color);
    let without_history_delta = mean_color_delta(&frame0_color, &without_history_color);
    assert!(
        with_history_delta < without_history_delta,
        "temporal history should reduce inter-frame color drift for slow camera motion: with_history={with_history_delta} without_history={without_history_delta}"
    );
    assert!(cpu_with_history.metrics.continuation_available_count > 0);
    assert!(cpu_with_history.metrics.continuation_consumed_count > 0);
    assert!(
        cpu_with_history
            .metrics
            .continuation_diagnostics
            .iter()
            .any(|entry| entry.contains("verdict=available")
                && entry.contains("change_class=camera-motion"))
    );
    assert!(motion.iter().any(motion_valid));

    let (wgsl_plan0, wgsl_ctx0, wgsl_input0) = presentation_fixture_with_state(
        temporal_alias_source(),
        "alias_view",
        "alias_region",
        DispatchBackend::Wgsl,
        viewport,
        camera_a,
        camera_a,
        0,
        0,
        true,
        None,
    );
    let wgsl_frame0 = execute_plan(&wgsl_ctx0, &wgsl_plan0, &wgsl_input0).expect("wgsl seed");
    let (wgsl_plan1, wgsl_ctx1, wgsl_input1) = presentation_fixture_with_state(
        temporal_alias_source(),
        "alias_view",
        "alias_region",
        DispatchBackend::Wgsl,
        viewport,
        camera_b,
        camera_a,
        1,
        0,
        false,
        wgsl_frame0.history.clone(),
    );
    let wgsl_with_history =
        execute_plan(&wgsl_ctx1, &wgsl_plan1, &wgsl_input1).expect("wgsl temporal reuse");
    assert_attachment_vec3_approx_eq(
        &with_history_color,
        &wgsl_with_history
            .attachments
            .decode_attachment("color")
            .unwrap(),
        1.0e-2,
        "temporal color",
    );
}

#[test]
fn motion_resolve_marks_newly_visible_pixels_as_disoccluded() {
    let camera_a = CanonicalCameraInput {
        position: [0.0, 0.0, 2.0],
        forward: [0.0, 0.0, -1.0],
        up: [0.0, 1.0, 0.0],
        vertical_fov_degrees: 45.0,
    };
    let mut camera_b = camera_a;
    camera_b.vertical_fov_degrees = 75.0;
    camera_b.forward = normalize_vec3([0.72, 0.0, -1.0]);
    let viewport = CanonicalViewportInput {
        width: 24,
        height: 24,
    };

    let (plan0, ctx0, input0) = presentation_fixture_with_state(
        temporal_disocclusion_source(),
        "disocclusion_view",
        "disocclusion_region",
        DispatchBackend::Cpu,
        viewport,
        camera_a,
        camera_a,
        0,
        0,
        true,
        None,
    );
    let frame0 = execute_plan(&ctx0, &plan0, &input0).expect("seed frame");

    let (plan1, ctx1, input1) = presentation_fixture_with_state(
        temporal_disocclusion_source(),
        "disocclusion_view",
        "disocclusion_region",
        DispatchBackend::Cpu,
        viewport,
        camera_b,
        camera_a,
        1,
        0,
        false,
        frame0.history.clone(),
    );
    let frame1 = execute_plan(&ctx1, &plan1, &input1).expect("reprojected frame");
    let motion = frame1.attachments.decode_attachment("motion").unwrap();
    let valid_count = motion.iter().filter(|value| motion_valid(value)).count();
    let disoccluded_count = motion
        .iter()
        .filter(|value| motion_disoccluded(value))
        .count();

    assert!(
        disoccluded_count > 0,
        "expected some disoccluded motion samples, got valid_count={valid_count} disoccluded_count={disoccluded_count} rejected={} unavailable={}",
        frame1.metrics.continuation_rejected_count,
        frame1.metrics.continuation_unavailable_count
    );
    assert!(frame1.metrics.continuation_rejected_count > 0);
}

#[test]
fn camera_cut_invalidates_history_and_falls_back_to_current_color() {
    let camera_a = CanonicalCameraInput {
        position: [0.0, 0.0, 2.0],
        forward: [0.0, 0.0, -1.0],
        up: [0.0, 1.0, 0.0],
        vertical_fov_degrees: 75.0,
    };
    let mut camera_cut = camera_a;
    camera_cut.position = [1.2, 0.2, 2.0];
    let viewport = CanonicalViewportInput {
        width: 4,
        height: 4,
    };

    let (plan0, ctx0, input0) = presentation_fixture_with_state(
        presentation_exec_source(),
        "exec_view",
        "exec_region",
        DispatchBackend::Cpu,
        viewport,
        camera_a,
        camera_a,
        0,
        0,
        true,
        None,
    );
    let frame0 = execute_plan(&ctx0, &plan0, &input0).expect("seed frame");

    let (plan1, ctx1, input1) = presentation_fixture_with_state(
        presentation_exec_source(),
        "exec_view",
        "exec_region",
        DispatchBackend::Cpu,
        viewport,
        camera_cut,
        camera_a,
        1,
        0,
        true,
        frame0.history.clone(),
    );
    let with_history = execute_plan(&ctx1, &plan1, &input1).expect("cut with history");

    let (plan2, ctx2, input2) = presentation_fixture_with_state(
        presentation_exec_source(),
        "exec_view",
        "exec_region",
        DispatchBackend::Cpu,
        viewport,
        camera_cut,
        camera_a,
        1,
        0,
        true,
        None,
    );
    let without_history = execute_plan(&ctx2, &plan2, &input2).expect("cut without history");

    assert_eq!(
        with_history.attachments.decode_attachment("color").unwrap(),
        without_history
            .attachments
            .decode_attachment("color")
            .unwrap()
    );
    assert!(with_history.metrics.continuation_rejected_count > 0);
    assert_eq!(with_history.metrics.continuation_consumed_count, 0);
    assert!(
        with_history
            .metrics
            .continuation_diagnostics
            .iter()
            .any(|entry| entry.contains("reason=history-reset")
                && entry.contains("change_class=history-reset"))
    );
}

#[test]
fn participants_resolve_can_be_disabled_when_frame_contract_does_not_request_it() {
    let (mut plan, ctx, input) = presentation_fixture(DispatchBackend::Cpu);
    plan.apply_participant_policy(false, false);
    assert!(
        plan.validate().is_empty(),
        "disabled-participants plan must remain valid"
    );
    assert!(
        !plan
            .passes
            .iter()
            .any(|pass| matches!(pass.kind, PresentationPassKind::ParticipantsResolve { .. }))
    );

    let result = execute_plan(&ctx, &plan, &input).expect("cpu presentation execution");
    assert!(result.attachments.attachment("radiance").is_none());
    assert!(result.attachments.attachment("medium").is_none());
    assert!(result.attachments.attachment("color").is_some());
}

#[test]
fn quality_override_enables_hit_compaction_and_half_res_participants_with_cpu_wgsl_parity() {
    let (cpu_plan, cpu_ctx, mut cpu_input) = presentation_fixture(DispatchBackend::Cpu);
    let (wgsl_plan, wgsl_ctx, mut wgsl_input) = presentation_fixture(DispatchBackend::Wgsl);
    let mut quality = cpu_plan.frame.quality.initial_state();
    quality.hit_compaction_enabled = true;
    quality.half_res_participants = true;
    quality.active_degradations = vec![
        QualityDegradationStep::EnableHitCompaction,
        QualityDegradationStep::HalfResolutionParticipants,
    ];

    cpu_input.quality_override = Some(quality.clone());
    wgsl_input.quality_override = Some(quality);

    let cpu = execute_plan(&cpu_ctx, &cpu_plan, &cpu_input).expect("cpu quality override");
    let wgsl = execute_plan(&wgsl_ctx, &wgsl_plan, &wgsl_input).expect("wgsl quality override");

    let cpu_color = cpu
        .attachments
        .decode_attachment("color")
        .expect("cpu color attachment");
    let wgsl_color = wgsl
        .attachments
        .decode_attachment("color")
        .expect("wgsl color attachment");
    assert_attachment_vec3_approx_eq(&cpu_color, &wgsl_color, 0.05, "quality color parity");

    let cpu_radiance = cpu
        .attachments
        .attachment("radiance")
        .expect("cpu radiance");
    let cpu_medium = cpu.attachments.attachment("medium").expect("cpu medium");
    assert_eq!(cpu_radiance.layout.width, 2);
    assert_eq!(cpu_radiance.layout.height, 2);
    assert_eq!(cpu_medium.layout.width, 2);
    assert_eq!(cpu_medium.layout.height, 2);

    assert!(cpu.frame_cost.quality.hit_compaction_enabled);
    assert!(cpu.frame_cost.quality.half_res_participants);
    assert_eq!(cpu.frame_cost.surface_resolve_count, cpu.metrics.hit_count);
    assert_eq!(cpu.frame_cost.participant_resolve_count, 8);
    assert!(
        cpu.frame_cost
            .active_acceleration_artifacts
            .iter()
            .any(|artifact| artifact == "hit_compaction")
    );
    assert!(
        cpu.frame_cost
            .active_acceleration_artifacts
            .iter()
            .any(|artifact| artifact == "half_res_participants")
    );
    assert_eq!(cpu.frame_cost.quality, wgsl.frame_cost.quality);
}

#[test]
fn quality_override_reduces_primary_work_and_scales_surface_attachment() {
    let (cpu_plan, cpu_ctx, mut cpu_input) = presentation_fixture(DispatchBackend::Cpu);
    let (wgsl_plan, wgsl_ctx, mut wgsl_input) = presentation_fixture(DispatchBackend::Wgsl);
    let mut quality = cpu_plan.frame.quality.initial_state();
    quality.internal_resolution_scale = 0.5;
    quality.active_degradations = vec![QualityDegradationStep::ReduceInternalResolution];

    cpu_input.quality_override = Some(quality.clone());
    wgsl_input.quality_override = Some(quality);

    let cpu = execute_plan(&cpu_ctx, &cpu_plan, &cpu_input).expect("cpu dynamic resolution");
    let wgsl = execute_plan(&wgsl_ctx, &wgsl_plan, &wgsl_input).expect("wgsl dynamic resolution");

    let cpu_color = cpu
        .attachments
        .decode_attachment("color")
        .expect("cpu color attachment");
    let wgsl_color = wgsl
        .attachments
        .decode_attachment("color")
        .expect("wgsl color attachment");
    assert_attachment_vec3_approx_eq(&cpu_color, &wgsl_color, 0.05, "dynamic resolution parity");

    let cpu_surface = cpu.attachments.attachment("surface").expect("cpu surface");
    let cpu_radiance = cpu
        .attachments
        .attachment("radiance")
        .expect("cpu radiance");
    let cpu_medium = cpu.attachments.attachment("medium").expect("cpu medium");
    assert_eq!(cpu_surface.layout.width, 2);
    assert_eq!(cpu_surface.layout.height, 2);
    assert_eq!(cpu_radiance.layout.width, 2);
    assert_eq!(cpu_radiance.layout.height, 2);
    assert_eq!(cpu_medium.layout.width, 2);
    assert_eq!(cpu_medium.layout.height, 2);
    assert_eq!(cpu.attachments.attachment("color").unwrap().layout.width, 4);
    assert!(cpu.frame_cost.quality.reconstructed_output);
    assert_eq!(cpu.frame_cost.quality.internal_width, 2);
    assert_eq!(cpu.frame_cost.quality.internal_height, 2);
    assert!(
        cpu.frame_cost
            .active_acceleration_artifacts
            .iter()
            .any(|artifact| artifact == "dynamic_resolution")
    );
    assert!(
        cpu.frame_cost
            .passes
            .iter()
            .any(|pass| { pass.pass_kind == "primary_visibility" && pass.work_items == 4 }),
        "expected internal-resolution primary visibility work items, got {:?}",
        cpu.frame_cost.passes
    );
    assert!(
        cpu.frame_cost
            .passes
            .iter()
            .any(|pass| pass.pass_kind == "surface_resolve" && pass.work_items == 4)
    );
}

#[test]
fn quarter_scale_reports_divisor_aligned_internal_dimensions_for_odd_viewports() {
    let (plan, ctx, mut input) = presentation_fixture(DispatchBackend::Cpu);
    let camera = CanonicalCameraInput {
        position: [0.0, 0.0, 2.0],
        forward: [0.0, 0.0, -1.0],
        up: [0.0, 1.0, 0.0],
        vertical_fov_degrees: 75.0,
    };
    let viewport = CanonicalViewportInput {
        width: 5,
        height: 5,
    };
    input.frame_state = frame_state_value(camera, camera, viewport, [0.0, 0.0], 0, 1.0 / 60.0);
    let mut quality = plan.frame.quality.initial_state();
    quality.internal_resolution_scale = 0.25;
    quality.active_degradations = vec![QualityDegradationStep::ReduceInternalResolution];
    input.quality_override = Some(quality);

    let result = execute_plan(&ctx, &plan, &input).expect("cpu odd viewport quarter scale");

    assert_eq!(result.frame_cost.quality.internal_width, 2);
    assert_eq!(result.frame_cost.quality.internal_height, 2);
    assert!(result.frame_cost.quality.reconstructed_output);
    let surface = result
        .attachments
        .attachment("surface")
        .expect("surface attachment");
    assert_eq!(surface.layout.width, 2);
    assert_eq!(surface.layout.height, 2);
}

#[test]
fn frame_cost_reports_tile_culling_when_support_bounds_shrink_screen_work() {
    let viewport = CanonicalViewportInput {
        width: 32,
        height: 16,
    };
    let camera = CanonicalCameraInput {
        position: [0.0, 0.0, 4.0],
        forward: [0.0, 0.0, -1.0],
        up: [0.0, 1.0, 0.0],
        vertical_fov_degrees: 75.0,
    };
    let (plan, ctx, input) = presentation_fixture_with_state(
        presentation_exec_source(),
        "exec_view",
        "exec_region",
        DispatchBackend::Cpu,
        viewport,
        camera,
        camera,
        0,
        0,
        false,
        None,
    );

    let result = execute_plan(&ctx, &plan, &input).expect("cpu culling execution");
    assert!(result.frame_cost.tile_cull_total_tiles > 0);
    assert!(result.frame_cost.tile_cull_active_tiles > 0);
    assert!(result.frame_cost.tile_cull_active_tiles < result.frame_cost.tile_cull_total_tiles);
    assert!(
        result
            .frame_cost
            .active_acceleration_artifacts
            .iter()
            .any(|artifact| artifact == "view_tile_culling")
    );
}

#[test]
fn adaptive_controller_steps_down_and_recovers_deterministically() {
    let contract = RealtimeQualityContract::named(RealtimeQualityTier::Realtime60);
    let mut controller = AdaptivePresentationController::new(contract).with_window(1);

    assert!(controller.observe_frame_time_ms(19.0));
    assert!(controller.quality().internal_resolution_scale < 1.0);
    assert_eq!(
        controller.quality().active_degradations,
        vec![QualityDegradationStep::ReduceInternalResolution]
    );

    assert!(!controller.observe_frame_time_ms(10.0));
    assert!(!controller.observe_frame_time_ms(10.0));
    assert!(controller.observe_frame_time_ms(10.0));
    assert_eq!(controller.quality().internal_resolution_scale, 1.0);
    assert!(controller.quality().active_degradations.is_empty());
}

#[test]
fn adaptive_session_uses_frame_cost_feedback_to_degrade_next_frame() {
    let (mut plan, ctx, input) = presentation_fixture(DispatchBackend::Cpu);
    let mut contract = RealtimeQualityContract::named(RealtimeQualityTier::Realtime60);
    contract.target_fps = 100_000;
    plan.frame.quality = contract.clone();

    let mut session = AdaptivePresentationSession::new(contract).with_window(1);
    let frame0 = session
        .execute_frame(&ctx, &plan, &input)
        .expect("adaptive session frame0");
    assert_eq!(frame0.frame_cost.quality.internal_resolution_scale, 1.0);

    let frame1 = session
        .execute_frame(&ctx, &plan, &input)
        .expect("adaptive session frame1");
    assert!(session.controller().quality().internal_resolution_scale < 1.0);
    assert!(frame1.frame_cost.quality.internal_resolution_scale < 1.0);
}

#[test]
fn adaptive_controller_only_uses_degradations_allowed_by_contract() {
    let contract = RealtimeQualityContract::named(RealtimeQualityTier::Debug);
    let mut controller = AdaptivePresentationController::new(contract.clone()).with_window(1);

    assert!(controller.observe_frame_time_ms(50.0));
    assert!(!controller.quality().hit_compaction_enabled);
    assert!(!controller.quality().half_res_participants);
    assert!(controller.quality().primary_max_steps < contract.primary_max_steps);
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
