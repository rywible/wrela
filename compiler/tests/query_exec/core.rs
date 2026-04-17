use super::*;

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
    assert_hit3_approx_eq(&cpu_world_trace, &vgpu_world_trace);
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
    assert!(cpu_trace.observability.shape_leaf_visits > 0);
    assert_eq!(cpu_trace.observability.cache_brick_visits, 0);
    assert_eq!(cpu_trace.observability.cache_brick_hits, 0);
    assert_eq!(
        cpu_trace.observability.cache_upload_attempts,
        cpu_trace
            .observability
            .cache_resident_shared_snapshot_artifacts
    );
    assert_eq!(
        cpu_trace.observability.cache_upload_rejections,
        cpu_trace
            .observability
            .cache_resident_observer_local_artifacts
    );
    assert_eq!(cpu_trace.observability.accepted_relaxed_steps, 0);
    assert!(
        cpu_trace
            .observability
            .step_certificate_kinds
            .get(&StepCertificateKind::DenseDistanceBound)
            .copied()
            .unwrap_or_default()
            > 0
    );
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
    assert!(rendered.contains("shape_leaf_visits="));
    assert!(rendered.contains("union_cluster_visits="));
    assert!(rendered.contains("cache_brick_visits="));
    assert_eq!(cpu_trace.observability.cache_interval_advances, 0);
    assert!(rendered.contains("cache_interval_advances="));
    assert!(rendered.contains("cache_resident_shared_snapshot_artifacts="));
    assert!(rendered.contains("cache_upload_attempts="));
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
    assert!(
        vgpu_world_trace
            .cost_report
            .causes
            .iter()
            .any(|cause| { cause.kind == SemanticCostCauseKind::CacheTraversal })
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
        wgsl_world_trace.observability.cache_upload_attempts,
        wgsl_world_trace
            .observability
            .cache_resident_shared_snapshot_artifacts
    );
    assert_eq!(
        wgsl_world_trace.observability.cache_upload_rejections,
        wgsl_world_trace
            .observability
            .cache_resident_observer_local_artifacts
    );
    assert!(wgsl_world_trace.observability.cache_brick_visits > 0);
    assert!(wgsl_world_trace.observability.cache_brick_hits > 0);
    assert!(
        wgsl_world_trace
            .observability
            .wgsl_layout_signature
            .is_some()
    );
    assert_eq!(wgsl_world_trace.observability.wgsl_bind_group_count, 4);
    assert!(
        wgsl_world_trace
            .observability
            .gpu_runtime
            .queue_submit_count
            > 0
    );
    assert!(
        wgsl_world_trace
            .observability
            .gpu_runtime
            .transient_buffer_creations
            > 0
    );
    assert_eq!(
        wgsl_world_trace
            .observability
            .gpu_runtime
            .timestamps_supported,
        wgsl_world_trace
            .observability
            .gpu_runtime
            .timestamped_pass_count
            > 0
    );
    if wgsl_world_trace
        .observability
        .gpu_runtime
        .timestamps_supported
    {
        assert!(
            wgsl_world_trace
                .observability
                .gpu_runtime
                .gpu_time_total_micros
                > 0
        );
        assert!(
            wgsl_world_trace
                .observability
                .gpu_runtime
                .gpu_time_max_micros
                > 0
        );
    } else {
        assert_eq!(
            wgsl_world_trace
                .observability
                .gpu_runtime
                .gpu_time_total_micros,
            0
        );
        assert_eq!(
            wgsl_world_trace
                .observability
                .gpu_runtime
                .gpu_time_max_micros,
            0
        );
    }
    assert!(
        wgsl_world_trace
            .observability
            .wgsl_requested_max_storage_buffer_bytes
            >= wgsl_world_trace
                .observability
                .wgsl_used_max_storage_buffer_bytes
    );
    assert!(
        wgsl_world_trace
            .observability
            .wgsl_used_max_storage_buffer_bytes
            >= 4
    );
    assert!(matches!(
        wgsl_world_trace.observability.wgsl_selected_workgroup_size,
        32 | 64 | 128
    ));
    assert_eq!(
        wgsl_world_trace
            .observability
            .solver_generated_dense_fallback_rays,
        1
    );
    let rendered_wgsl_world_cost = render_semantic_cost_report(&wgsl_world_trace.cost_report);
    assert!(rendered_wgsl_world_cost.contains("solver_generated_dense_fallback_rays=1"));
    assert!(rendered_wgsl_world_cost.contains("cache_shared_snapshot="));
    assert!(rendered_wgsl_world_cost.contains("observer_continuation_seed_hits="));
    assert!(rendered_wgsl_world_cost.contains("wgsl_layout_signature="));
    assert!(rendered_wgsl_world_cost.contains("wgsl_bind_groups=4"));
    assert!(rendered_wgsl_world_cost.contains("wgsl_storage_requested="));
    assert!(rendered_wgsl_world_cost.contains("wgsl_storage_used="));
    assert!(rendered_wgsl_world_cost.contains("wgsl_workgroup_size="));
    assert!(rendered_wgsl_world_cost.contains("gpu_runtime timestamps_supported="));
    assert!(rendered_wgsl_world_cost.contains("timestamped_pass_count="));
    assert!(rendered_wgsl_world_cost.contains("queue_submit_count="));
    assert!(rendered_wgsl_world_cost.contains("upload_bytes="));
    assert!(rendered_wgsl_world_cost.contains("readback_bytes="));
    assert!(rendered_wgsl_world_cost.contains("pipeline_cache_hits="));
    assert!(rendered_wgsl_world_cost.contains("pipeline_cache_misses="));
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
    assert!(
        wgsl_batch_trace
            .observability
            .gpu_runtime
            .queue_submit_count
            > 0
    );
    assert_eq!(
        wgsl_batch_trace
            .observability
            .gpu_runtime
            .timestamps_supported,
        wgsl_batch_trace
            .observability
            .gpu_runtime
            .timestamped_pass_count
            > 0
    );
}

#[test]
fn query_exec_wgsl_world_trace_reuses_resident_scene_after_first_upload() {
    let source = r#"
field exact distance phase43_resident_scene_field(p: Vec3) -> F32 {
    sphere(radius = 1.0)
}

material phase43_resident_scene_material(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(0.2, 0.35, 0.45),
        roughness=0.5,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

shape phase43_resident_scene_shape {
    field = phase43_resident_scene_field
    material = phase43_resident_scene_material
    payload = Payload(entity_id=u32(43))
}

region phase43_resident_scene_region() {
    place scene = phase43_resident_scene_shape
}
"#;
    let (_, _, ctx) = typed_query_module(source);
    let capture_name = SmolStr::new("phase43_resident_scene_region");
    let region_scene_id = stable_region_scene_capture_id(&capture_name);
    let world_trace_plan =
        lower_world_query_plan(&WorldQueryPlan::for_query(WorldQueryKind::Trace));
    let args = [
        KernelValue::Capture(capture_name.clone()),
        scene_domain(region_scene_id, 1, false, false, false),
        ray_query_with_limits([0.0, 0.0, 3.0], [0.0, 0.0, -1.0], 6.0, 0.05, 0.001, 96),
    ];

    let (first_hit, first_trace) =
        execute_world_query_with_trace_on(&ctx, DispatchBackend::Wgsl, &world_trace_plan, &args)
            .expect("first wgsl world trace");
    let (second_hit, second_trace) =
        execute_world_query_with_trace_on(&ctx, DispatchBackend::Wgsl, &world_trace_plan, &args)
            .expect("second wgsl world trace");

    assert_hit3_approx_eq(&first_hit, &second_hit);
    assert_eq!(first_trace.backend, DispatchBackend::Wgsl);
    assert_eq!(second_trace.backend, DispatchBackend::Wgsl);
    assert_eq!(first_trace.executor, DirectQueryExecutor::Wgsl);
    assert_eq!(second_trace.executor, DirectQueryExecutor::Wgsl);
    assert!(first_trace.observability.gpu_runtime.scene_reupload_bytes > 0);
    assert_eq!(
        second_trace.observability.gpu_runtime.scene_reupload_bytes,
        0
    );
    assert!(
        first_trace.observability.gpu_runtime.upload_bytes
            >= first_trace.observability.gpu_runtime.scene_reupload_bytes
    );
    assert_eq!(second_trace.observability.wgsl_bind_group_count, 4);
    assert!(second_trace.observability.gpu_runtime.queue_submit_count > 0);

    let rendered_second_cost = render_semantic_cost_report(&second_trace.cost_report);
    assert!(rendered_second_cost.contains("scene_reupload_bytes=0"));
}

#[test]
fn query_exec_wgsl_batch_reuses_resident_scene_across_dispatch_size_changes() {
    let _lock = wgsl_resident_cache_test_lock();
    clear_native_wgsl_test_caches();
    let (_, _, ctx) = typed_query_module(query_fixture_source());
    let region_capture = KernelValue::Capture(SmolStr::new("scene_region"));
    let fine_domain = scene_domain(
        stable_region_scene_capture_id(&SmolStr::new("scene_region")),
        1,
        true,
        true,
        true,
    );
    let world_batch_plan = lower_batch_query_plan(
        &BatchQueryPlan::for_contract(
            query_contract::SPATIAL_NEAREST_BATCH_WORLD,
            DispatchBackend::Wgsl,
            None,
        )
        .expect("world nearest batch plan"),
    );
    let small_ray_items = KernelValue::Array(vec![
        ray_query([0.0, 0.0, 3.0], [0.0, 0.0, -1.0]),
        ray_query([0.0, 0.0, 3.0], [0.0, 1.0, 0.0]),
    ]);
    let large_ray_items = KernelValue::Array(
        (0..48)
            .map(|index| {
                if index % 2 == 0 {
                    ray_query([0.0, 0.0, 3.0], [0.0, 0.0, -1.0])
                } else {
                    ray_query([0.0, 0.0, 3.0], [0.0, 1.0, 0.0])
                }
            })
            .collect(),
    );

    let (_small_hits, small_trace) = execute_batch_query_with_trace_on(
        &ctx,
        DispatchBackend::Wgsl,
        &world_batch_plan,
        &[region_capture.clone(), fine_domain.clone(), small_ray_items],
    )
    .expect("small wgsl world batch");
    let (_large_hits, large_trace) = execute_batch_query_with_trace_on(
        &ctx,
        DispatchBackend::Wgsl,
        &world_batch_plan,
        &[region_capture, fine_domain, large_ray_items],
    )
    .expect("large wgsl world batch");

    assert_eq!(
        large_trace.observability.gpu_runtime.scene_reupload_bytes,
        0
    );
    assert!(
        small_trace.observability.gpu_runtime.scene_reupload_bytes
            >= large_trace.observability.gpu_runtime.scene_reupload_bytes
    );
    assert!(
        !large_trace
            .observability
            .gpu_runtime
            .requested_limits_profile
            .is_empty()
    );
}
