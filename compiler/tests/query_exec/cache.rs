use super::*;

fn hybrid_cache_regression_source() -> &'static str {
    r#"
field exact distance front_flank_field(p: Vec3) -> F32 {
    translate = vec3(2.25, 0.0, 0.0) {
        sphere(radius = 0.65)
    }
}

field exact distance deep_center_field(p: Vec3) -> F32 {
    translate = vec3(0.0, 0.0, -4.5) {
        sphere(radius = 0.45)
    }
}

material cache_shade(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(0.3, 0.35, 0.4),
        roughness=0.5,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

shape front_flank_shape {
    field = front_flank_field
    material = cache_shade
    payload = Payload()
}

shape deep_center_shape {
    field = deep_center_field
    material = cache_shade
    payload = Payload()
}

shape cache_world_shape {
    union {
        use front_flank_shape
        use deep_center_shape
    }
}

region cache_world_region() {
    place scene = cache_world_shape
}

domain cache_world_domain(world: RegionCapture) {
    geometry_detail = 1
    material = false
    radiance = false
    media = false
    max_distance = 8.0
    min_step = 0.05
    hit_epsilon = 0.001
    max_steps = 192
}
"#
}

#[test]
fn query_exec_cpu_hybrid_shape_cache_advances_far_field_and_reduces_work() {
    let source = hybrid_cache_regression_source();
    let (_, _, mut ctx_with_cache) = typed_query_module(source);
    let (_, _, mut ctx_without_cache) = typed_query_module(source);
    ctx_with_cache
        .shared_acceleration
        .cache_catalog
        .world_support
        .clear();
    ctx_with_cache
        .shared_acceleration
        .cache_catalog
        .world_distance
        .clear();
    ctx_without_cache
        .shared_acceleration
        .cache_catalog
        .shape_support
        .clear();
    ctx_without_cache
        .shared_acceleration
        .cache_catalog
        .shape_distance
        .clear();
    ctx_without_cache
        .shared_acceleration
        .cache_catalog
        .world_support
        .clear();
    ctx_without_cache
        .shared_acceleration
        .cache_catalog
        .world_distance
        .clear();
    let plan = lower_world_query_plan(&WorldQueryPlan::for_query(WorldQueryKind::Trace));
    let region_scene_id = stable_region_scene_capture_id(&SmolStr::new("cache_world_region"));
    let domain = scene_domain_with_limits(
        region_scene_id,
        1,
        false,
        false,
        false,
        8.0,
        0.05,
        0.001,
        192,
    );
    let capture = KernelValue::Capture(SmolStr::new("cache_world_region"));
    let mut cached_field_samples = 0u32;
    let mut uncached_field_samples = 0u32;
    let mut cached_steps = 0u32;
    let mut uncached_steps = 0u32;
    let mut cached_cache_hits = 0u32;
    let mut cached_interval_advances = 0u32;

    for y in 0..6 {
        for x in 0..6 {
            let px = x as f32 * 0.12 - 0.30;
            let py = y as f32 * 0.12 - 0.30;
            let ray = ray_query_with_limits([px, py, 0.0], [0.0, 0.0, -1.0], 8.0, 0.05, 0.001, 192);

            let (cached_hit, cached_trace) = execute_world_query_with_trace_on(
                &ctx_with_cache,
                DispatchBackend::Cpu,
                &plan,
                &[capture.clone(), domain.clone(), ray.clone()],
            )
            .expect("cpu cached trace");
            let (uncached_hit, uncached_trace) = execute_world_query_with_trace_on(
                &ctx_without_cache,
                DispatchBackend::Cpu,
                &plan,
                &[capture.clone(), domain.clone(), ray],
            )
            .expect("cpu uncached trace");

            let cached_hit_ref = expect_struct(&cached_hit, "Hit3");
            let uncached_hit_ref = expect_struct(&uncached_hit, "Hit3");
            let cached_did_hit = expect_bool(field(cached_hit_ref, "hit"));
            let uncached_did_hit = expect_bool(field(uncached_hit_ref, "hit"));
            assert_eq!(cached_did_hit, uncached_did_hit);
            if cached_did_hit {
                assert!(
                    (expect_f32(field(cached_hit_ref, "distance"))
                        - expect_f32(field(uncached_hit_ref, "distance")))
                    .abs()
                        < 0.05
                );
                assert_eq!(
                    expect_u32(field(cached_hit_ref, "feature_id")),
                    expect_u32(field(uncached_hit_ref, "feature_id"))
                );
                assert_eq!(
                    expect_u32(field(cached_hit_ref, "instance_id")),
                    expect_u32(field(uncached_hit_ref, "instance_id"))
                );
                assert_eq!(
                    expect_u32(field(cached_hit_ref, "repeat_id")),
                    expect_u32(field(uncached_hit_ref, "repeat_id"))
                );
            }
            cached_field_samples += cached_trace.observability.field_samples;
            uncached_field_samples += uncached_trace.observability.field_samples;
            cached_steps += cached_trace.observability.trace_steps;
            uncached_steps += uncached_trace.observability.trace_steps;
            cached_cache_hits += cached_trace.observability.cache_brick_hits;
            cached_interval_advances += cached_trace.observability.cache_interval_advances;
            assert_eq!(uncached_trace.observability.cache_interval_advances, 0);
        }
    }

    assert!(cached_cache_hits > 0);
    assert!(cached_interval_advances > 0);
    assert!(
        cached_field_samples < uncached_field_samples,
        "cached_field_samples={} uncached_field_samples={}",
        cached_field_samples,
        uncached_field_samples
    );
    assert!(
        cached_steps < uncached_steps,
        "cached_steps={} uncached_steps={}",
        cached_steps,
        uncached_steps
    );
}

#[test]
fn query_exec_cpu_hybrid_world_cache_advances_far_field_without_changing_hits() {
    let source = hybrid_cache_regression_source();
    let (_, _, mut ctx_with_cache) = typed_query_module(source);
    let (_, _, mut ctx_without_cache) = typed_query_module(source);
    ctx_with_cache
        .shared_acceleration
        .cache_catalog
        .shape_support
        .clear();
    ctx_with_cache
        .shared_acceleration
        .cache_catalog
        .shape_distance
        .clear();
    ctx_without_cache
        .shared_acceleration
        .cache_catalog
        .shape_support
        .clear();
    ctx_without_cache
        .shared_acceleration
        .cache_catalog
        .shape_distance
        .clear();
    ctx_without_cache
        .shared_acceleration
        .cache_catalog
        .world_support
        .clear();
    ctx_without_cache
        .shared_acceleration
        .cache_catalog
        .world_distance
        .clear();
    let plan = lower_world_query_plan(&WorldQueryPlan::for_query(WorldQueryKind::Trace));
    let region_scene_id = stable_region_scene_capture_id(&SmolStr::new("cache_world_region"));
    let domain = scene_domain_with_limits(
        region_scene_id,
        1,
        false,
        false,
        false,
        8.0,
        0.05,
        0.001,
        192,
    );
    let capture = KernelValue::Capture(SmolStr::new("cache_world_region"));
    let mut cached_cache_hits = 0u32;
    let mut cached_interval_advances = 0u32;

    for y in 0..6 {
        for x in 0..6 {
            let px = x as f32 * 0.12 - 0.30;
            let py = y as f32 * 0.12 - 0.30;
            let ray = ray_query_with_limits([px, py, 0.0], [0.0, 0.0, -1.0], 8.0, 0.05, 0.001, 192);

            let (cached_hit, cached_trace) = execute_world_query_with_trace_on(
                &ctx_with_cache,
                DispatchBackend::Cpu,
                &plan,
                &[capture.clone(), domain.clone(), ray.clone()],
            )
            .expect("cpu cached trace");
            let (uncached_hit, uncached_trace) = execute_world_query_with_trace_on(
                &ctx_without_cache,
                DispatchBackend::Cpu,
                &plan,
                &[capture.clone(), domain.clone(), ray],
            )
            .expect("cpu uncached trace");

            let cached_hit_ref = expect_struct(&cached_hit, "Hit3");
            let uncached_hit_ref = expect_struct(&uncached_hit, "Hit3");
            let cached_did_hit = expect_bool(field(cached_hit_ref, "hit"));
            let uncached_did_hit = expect_bool(field(uncached_hit_ref, "hit"));
            assert_eq!(cached_did_hit, uncached_did_hit);
            if cached_did_hit {
                assert!(
                    (expect_f32(field(cached_hit_ref, "distance"))
                        - expect_f32(field(uncached_hit_ref, "distance")))
                    .abs()
                        < 0.05
                );
                assert_eq!(
                    expect_u32(field(cached_hit_ref, "feature_id")),
                    expect_u32(field(uncached_hit_ref, "feature_id"))
                );
                assert_eq!(
                    expect_u32(field(cached_hit_ref, "instance_id")),
                    expect_u32(field(uncached_hit_ref, "instance_id"))
                );
                assert_eq!(
                    expect_u32(field(cached_hit_ref, "repeat_id")),
                    expect_u32(field(uncached_hit_ref, "repeat_id"))
                );
            }
            cached_cache_hits += cached_trace.observability.cache_brick_hits;
            cached_interval_advances += cached_trace.observability.cache_interval_advances;
            assert_eq!(uncached_trace.observability.cache_interval_advances, 0);
        }
    }

    assert!(cached_cache_hits > 0);
    assert!(cached_interval_advances > 0);
}

#[test]
fn query_wgsl_workgroup_selection_is_bounded_and_uses_documented_defaults() {
    let _lock = workgroup_override_test_lock();
    let _guard = EnvVarGuard::set(WGSL_WORKGROUP_SIZE_OVERRIDE_ENV, "");
    let mut limits = wgpu::Limits::downlevel_defaults();
    limits.max_compute_workgroup_size_x = 128;
    limits.max_compute_invocations_per_workgroup = 128;

    let query_selected = wrela::query_exec::select_query_wgsl_workgroup_size(&limits)
        .expect("query workgroup selection");
    let presentation_selected =
        wrela::presentation_exec::select_presentation_workgroup_size(&limits)
            .expect("presentation workgroup selection");

    assert_eq!(query_selected, 128);
    assert_eq!(presentation_selected, 64);
    assert_eq!(
        wrela::query_exec::validate_query_wgsl_workgroup_size(64, &limits)
            .expect("query workgroup validation"),
        64
    );

    limits.max_compute_workgroup_size_x = 64;
    limits.max_compute_invocations_per_workgroup = 64;
    assert!(wrela::query_exec::validate_query_wgsl_workgroup_size(128, &limits).is_err());
    assert!(wrela::presentation_exec::validate_presentation_workgroup_size(128, &limits).is_err());

    limits.max_compute_workgroup_size_x = 16;
    limits.max_compute_invocations_per_workgroup = 16;
    assert!(wrela::query_exec::select_query_wgsl_workgroup_size(&limits).is_err());
    assert!(wrela::presentation_exec::select_presentation_workgroup_size(&limits).is_err());
}
