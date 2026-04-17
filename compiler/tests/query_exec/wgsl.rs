use super::provenance::{certified_normal_source, opaque_fallback_source};
use super::*;

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
        if expected_occluded {
            assert!(cpu_trace.observability.trace_steps > 0);
        } else {
            assert_eq!(cpu_trace.observability.trace_steps, 0);
            assert!(cpu_trace.observability.ray_support_interval_rejections > 0);
        }
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

    assert_query_summary_approx_eq(&cpu_value, &vgpu_value);
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
