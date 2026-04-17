use super::advanced::direct_semantics_source;
use super::*;

fn empty_world_fixture_source() -> &'static str {
    r#"
region empty_region() {
}

domain empty_domain(world: RegionCapture) {
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

fn payloadless_shape_fixture_source() -> &'static str {
    r#"
material payloadless_surface(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(0.3, 0.5, 0.8),
        roughness=0.28,
        metalness=0.0,
        clearcoat=0.08,
        clearcoat_roughness=0.06,
        sheen=0.02,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

field exact distance payloadless_field(p: Vec3) -> F32 {
    sphere(radius = 0.55)
}

shape payloadless_shape {
    field = payloadless_field
    material = payloadless_surface
}

region payloadless_region() {
    place primary = payloadless_shape
}

domain payloadless_domain(world: RegionCapture) {
    geometry_detail = 1
    material = false
    radiance = false
    media = false
    max_distance = 6.0
    min_step = 0.04
    hit_epsilon = 0.001
    max_steps = 96
}
"#
}

#[test]
fn query_exec_wgsl_world_trace_records_budget_rejection_on_empty_world() {
    let source = empty_world_fixture_source();
    let (_, _, ctx) = typed_query_module(source);
    let region_name = SmolStr::new("empty_region");
    let region_scene_id = stable_region_scene_capture_id(&region_name);
    let args = [
        KernelValue::Capture(region_name.clone()),
        scene_domain(region_scene_id, 1, false, false, false),
        ray_query_with_limits([0.0, 0.0, 3.0], [0.0, 0.0, -1.0], 6.0, 0.05, 0.001, 96),
    ];
    let plan = lower_world_query_plan(&WorldQueryPlan::for_query(WorldQueryKind::Trace));

    let (wgsl_hit, wgsl_trace) =
        execute_world_query_with_trace_on(&ctx, DispatchBackend::Wgsl, &plan, &args)
            .expect("wgsl empty world trace");

    let hit = expect_struct(&wgsl_hit, "Hit3");
    assert!(!expect_bool(field(hit, "hit")));
    assert!(wgsl_trace.observability.cache_budget_rejections > 0);
    assert!(wgsl_trace.observability.cache_dense_fallback_rays > 0);
    assert!(
        wgsl_trace
            .observability
            .solver_generated_dense_fallback_rays
            > 0
    );
}

#[test]
fn query_exec_wgsl_world_trace_defaults_payload_for_payloadless_shapes() {
    let source = payloadless_shape_fixture_source();
    let (_, _, ctx) = typed_query_module(source);
    let region_name = SmolStr::new("payloadless_region");
    let region_scene_id = stable_region_scene_capture_id(&region_name);
    let args = [
        KernelValue::Capture(region_name.clone()),
        scene_domain(region_scene_id, 1, false, false, false),
        ray_query_with_limits([0.0, 0.0, 2.4], [0.0, 0.0, -1.0], 6.0, 0.04, 0.001, 96),
    ];
    let plan = lower_world_query_plan(&WorldQueryPlan::for_query(WorldQueryKind::Trace));

    let (cpu_hit, _) = execute_world_query_with_trace_on(&ctx, DispatchBackend::Cpu, &plan, &args)
        .expect("cpu payloadless trace");
    let (wgsl_hit, _) =
        execute_world_query_with_trace_on(&ctx, DispatchBackend::Wgsl, &plan, &args)
            .expect("wgsl payloadless trace");

    let cpu_hit = expect_struct(&cpu_hit, "Hit3");
    let wgsl_hit = expect_struct(&wgsl_hit, "Hit3");
    assert!(expect_bool(field(cpu_hit, "hit")));
    assert!(expect_bool(field(wgsl_hit, "hit")));

    let cpu_payload = expect_struct(field(cpu_hit, "payload"), "Payload");
    let wgsl_payload = expect_struct(field(wgsl_hit, "payload"), "Payload");

    assert_eq!(expect_u32(field(cpu_payload, "entity_id")), 0);
    assert_eq!(expect_u32(field(cpu_payload, "material_id")), 0);
    assert_eq!(expect_u32(field(wgsl_payload, "entity_id")), 0);
    assert_eq!(expect_u32(field(wgsl_payload, "material_id")), 0);

    let cpu_actor = expect_struct(field(cpu_payload, "actor"), "ActorHandle");
    let wgsl_actor = expect_struct(field(wgsl_payload, "actor"), "ActorHandle");
    assert_eq!(expect_u32(field(cpu_actor, "id")), 0);
    assert_eq!(expect_u32(field(cpu_actor, "generation")), 0);
    assert_eq!(expect_u32(field(wgsl_actor, "id")), 0);
    assert_eq!(expect_u32(field(wgsl_actor, "generation")), 0);
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

pub(super) fn certified_normal_source() -> &'static str {
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

pub(super) fn opaque_fallback_source() -> &'static str {
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

#[test]
fn query_wgsl_selector_honors_supported_workgroup_override() {
    let _lock = workgroup_override_test_lock();
    let _guard = EnvVarGuard::set(WGSL_WORKGROUP_SIZE_OVERRIDE_ENV, "64");
    let mut adapter_limits = wgpu::Limits::downlevel_defaults();
    adapter_limits.max_compute_invocations_per_workgroup = 128;
    adapter_limits.max_compute_workgroup_size_x = 128;
    assert_eq!(
        select_query_wgsl_workgroup_size(&adapter_limits).expect("select override"),
        64
    );
}

#[test]
fn query_wgsl_selector_rejects_incompatible_workgroup_override() {
    let _lock = workgroup_override_test_lock();
    let _guard = EnvVarGuard::set(WGSL_WORKGROUP_SIZE_OVERRIDE_ENV, "128");
    let mut adapter_limits = wgpu::Limits::downlevel_defaults();
    adapter_limits.max_compute_invocations_per_workgroup = 64;
    adapter_limits.max_compute_workgroup_size_x = 64;
    let err =
        select_query_wgsl_workgroup_size(&adapter_limits).expect_err("reject incompatible size");
    assert!(
        err.to_string().contains("incompatible with adapter limits"),
        "unexpected error: {err}"
    );
}

#[test]
fn query_wgsl_batch_rebuilds_pipeline_when_workgroup_override_changes() {
    let _lock = workgroup_override_test_lock();
    let (_, _, ctx) = typed_query_module(query_fixture_source());
    let region_capture = KernelValue::Capture(SmolStr::new("scene_region"));
    let fine_domain = scene_domain(
        stable_region_scene_capture_id(&SmolStr::new("scene_region")),
        1,
        true,
        true,
        true,
    );
    let first_ray_items =
        KernelValue::Array(vec![ray_query([0.0, 0.0, 3.0], [0.0, 0.0, -1.0]); 48]);
    let mut second_rays = vec![ray_query([0.0, 0.0, 3.0], [0.0, 0.0, -1.0]); 32];
    second_rays.extend(vec![ray_query([0.0, 0.0, 3.0], [0.0, 1.0, 0.0]); 16]);
    let second_ray_items = KernelValue::Array(second_rays);
    let world_batch_plan = lower_batch_query_plan(
        &BatchQueryPlan::for_contract(
            query_contract::SPATIAL_NEAREST_BATCH_WORLD,
            DispatchBackend::Wgsl,
            None,
        )
        .expect("world nearest batch plan"),
    );

    let (_first_hits, first_trace) = {
        let _guard = EnvVarGuard::set(WGSL_WORKGROUP_SIZE_OVERRIDE_ENV, "32");
        execute_batch_query_with_trace_on(
            &ctx,
            DispatchBackend::Wgsl,
            &world_batch_plan,
            &[region_capture.clone(), fine_domain.clone(), first_ray_items],
        )
        .expect("wgsl batch run with 32-wide workgroup")
    };

    let (expected_second_hits, _) = execute_batch_query_with_trace_on(
        &ctx,
        DispatchBackend::VirtualGpu,
        &world_batch_plan,
        &[
            region_capture.clone(),
            fine_domain.clone(),
            second_ray_items.clone(),
        ],
    )
    .expect("virtual gpu batch baseline");
    let (second_hits, second_trace) = {
        let _guard = EnvVarGuard::set(WGSL_WORKGROUP_SIZE_OVERRIDE_ENV, "64");
        execute_batch_query_with_trace_on(
            &ctx,
            DispatchBackend::Wgsl,
            &world_batch_plan,
            &[region_capture, fine_domain, second_ray_items],
        )
        .expect("wgsl batch run with 64-wide workgroup")
    };

    assert_eq!(first_trace.observability.wgsl_selected_workgroup_size, 32);
    assert_eq!(second_trace.observability.wgsl_selected_workgroup_size, 64);

    for (expected, actual) in expect_array(&expected_second_hits)
        .iter()
        .zip(expect_array(&second_hits))
    {
        assert_hit3_approx_eq(expected, actual);
    }
}
