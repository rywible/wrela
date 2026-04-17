use super::advanced::{
    direct_semantics_source, large_union_distance_fixture_source, ray_solver_opaque_fixture_source,
    relaxed_torus_solver_fixture_source, transformed_analytic_primitives_fixture_source,
    translated_repeat_linear_solver_fixture_source, world_ray_solver_support_fixture_source,
    world_ray_support_interval_fixture_source, world_ray_support_interval_variants_fixture_source,
    world_support_cost_fixture_source,
};
use super::*;

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
fn query_exec_cpu_world_trace_reports_support_entry_jumps_and_pruned_nodes() {
    let (_, _, ctx) = typed_query_module(world_ray_support_interval_fixture_source());
    let region_scene_id = stable_region_scene_capture_id(&SmolStr::new("scene_region"));
    let domain = scene_domain(region_scene_id, 1, true, false, false);
    let plan = lower_world_query_plan(&WorldQueryPlan::for_query_with_backend(
        WorldQueryKind::Trace,
        DispatchBackend::Cpu,
    ));
    let (hit, trace) = execute_world_query_with_trace_on(
        &ctx,
        DispatchBackend::Cpu,
        &plan,
        &[
            KernelValue::Capture(SmolStr::new("scene_region")),
            domain,
            ray_query_with_limits([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], 6.0, 0.05, 0.001, 96),
        ],
    )
    .expect("cpu world trace with support interval jump");

    let hit = expect_struct(&hit, "Hit3");
    let payload = expect_struct(field(hit, "payload"), "Payload");
    assert_eq!(expect_u32(field(payload, "entity_id")), 11);
    assert!(trace.observability.candidate_count > 0);
    assert!(trace.observability.support_pruned_candidates > 0);
    assert!(trace.observability.ray_support_entry_jumps > 0);
    assert!(trace.observability.shape_leaf_visits > 0);
    assert!(trace.observability.acceleration_pruned_nodes > 0);
    let rendered = render_semantic_cost_report(&trace.cost_report);
    assert!(rendered.contains("acceleration_pruned_nodes="));
    assert!(rendered.contains("shape_leaf_visits="));
    assert!(rendered.contains("ray_support_entry_jumps="));
    assert!(rendered.contains("ray_support_interval_rejections="));
    assert!(
        trace
            .cost_report
            .causes
            .iter()
            .any(|cause| { cause.kind == SemanticCostCauseKind::SupportTopology })
    );
}

#[test]
fn query_exec_cpu_world_trace_support_intervals_cover_miss_tangent_inside_and_repeat_cells() {
    let (_, _, ctx) = typed_query_module(world_ray_support_interval_variants_fixture_source());
    let plan = lower_world_query_plan(&WorldQueryPlan::for_query_with_backend(
        WorldQueryKind::Trace,
        DispatchBackend::Cpu,
    ));
    let world_trace = |capture: &str, origin: [f32; 3], direction: [f32; 3], max_distance: f32| {
        execute_world_query_with_trace_on(
            &ctx,
            DispatchBackend::Cpu,
            &plan,
            &[
                KernelValue::Capture(SmolStr::new(capture)),
                scene_domain(
                    stable_region_scene_capture_id(&SmolStr::new(capture)),
                    1,
                    true,
                    false,
                    false,
                ),
                ray_query_with_limits(origin, direction, max_distance, 0.05, 0.001, 96),
            ],
        )
        .expect("cpu world trace")
    };

    let (inside_hit, inside_trace) =
        world_trace("inside_region", [0.0, 0.0, 0.0], [1.0, 0.0, 0.0], 6.0);
    let inside_hit = expect_struct(&inside_hit, "Hit3");
    let inside_payload = expect_struct(field(inside_hit, "payload"), "Payload");
    assert!(expect_bool(field(inside_hit, "hit")));
    assert_eq!(expect_u32(field(inside_payload, "entity_id")), 31);
    assert_eq!(inside_trace.observability.ray_support_entry_jumps, 0);
    assert_eq!(
        inside_trace.observability.ray_support_interval_rejections,
        0
    );

    let (tangent_hit, tangent_trace) =
        world_trace("tangent_region", [0.0, 0.5, 0.0], [1.0, 0.0, 0.0], 6.0);
    let tangent_hit = expect_struct(&tangent_hit, "Hit3");
    let tangent_payload = expect_struct(field(tangent_hit, "payload"), "Payload");
    assert!(expect_bool(field(tangent_hit, "hit")));
    assert_eq!(expect_u32(field(tangent_payload, "entity_id")), 32);
    assert!(tangent_trace.observability.ray_support_entry_jumps > 0);

    let (translated_hit, translated_trace) =
        world_trace("translated_region", [0.0, 0.0, 0.0], [1.0, 0.0, 0.0], 6.0);
    let translated_hit = expect_struct(&translated_hit, "Hit3");
    let translated_payload = expect_struct(field(translated_hit, "payload"), "Payload");
    assert!(expect_bool(field(translated_hit, "hit")));
    assert_eq!(expect_u32(field(translated_payload, "entity_id")), 33);
    assert!(translated_trace.observability.ray_support_entry_jumps > 0);

    let (scaled_hit, scaled_trace) =
        world_trace("scaled_region", [0.0, 0.0, 3.0], [0.0, 0.0, -1.0], 6.0);
    let scaled_hit = expect_struct(&scaled_hit, "Hit3");
    let scaled_payload = expect_struct(field(scaled_hit, "payload"), "Payload");
    assert!(expect_bool(field(scaled_hit, "hit")));
    assert_eq!(expect_u32(field(scaled_payload, "entity_id")), 36);
    assert!(scaled_trace.observability.ray_support_entry_jumps > 0);

    let (mirrored_hit, mirrored_trace) =
        world_trace("mirrored_region", [-5.0, 0.0, 0.0], [1.0, 0.0, 0.0], 10.0);
    let mirrored_hit = expect_struct(&mirrored_hit, "Hit3");
    let mirrored_payload = expect_struct(field(mirrored_hit, "payload"), "Payload");
    assert!(expect_bool(field(mirrored_hit, "hit")));
    assert_eq!(expect_u32(field(mirrored_payload, "entity_id")), 34);
    assert!(mirrored_trace.observability.ray_support_entry_jumps > 0);

    let (repeat_linear_hit, repeat_linear_trace) = world_trace(
        "repeat_linear_region",
        [-6.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        12.0,
    );
    let repeat_linear_hit = expect_struct(&repeat_linear_hit, "Hit3");
    let repeat_linear_payload = expect_struct(field(repeat_linear_hit, "payload"), "Payload");
    assert!(expect_bool(field(repeat_linear_hit, "hit")));
    assert_eq!(expect_u32(field(repeat_linear_payload, "entity_id")), 37);
    assert!(repeat_linear_trace.observability.ray_support_entry_jumps > 0);
    assert!(
        repeat_linear_trace
            .observability
            .solver_methods
            .contains(&RaySolverMethod::RepeatAwareTraversal)
    );
    assert_eq!(repeat_linear_trace.observability.solver_repeat_attempts, 1);
    assert_eq!(repeat_linear_trace.observability.solver_repeat_supported, 1);
    assert_eq!(
        repeat_linear_trace.observability.solver_repeat_inapplicable,
        0
    );
    assert_eq!(
        repeat_linear_trace.observability.solver_repeat_unsupported,
        0
    );
    assert!(
        repeat_linear_trace
            .observability
            .solver_repeat_cells_enumerated
            > 0
    );

    let (repeat_grid_hit, repeat_grid_trace) = world_trace(
        "repeat_grid_region",
        [-6.0, 0.25, 0.0],
        [1.0, 0.0, 0.0],
        12.0,
    );
    let repeat_grid_hit = expect_struct(&repeat_grid_hit, "Hit3");
    let repeat_grid_payload = expect_struct(field(repeat_grid_hit, "payload"), "Payload");
    assert!(expect_bool(field(repeat_grid_hit, "hit")));
    assert_eq!(expect_u32(field(repeat_grid_payload, "entity_id")), 38);
    assert!(repeat_grid_trace.observability.ray_support_entry_jumps > 0);
    assert!(
        !repeat_grid_trace
            .observability
            .solver_methods
            .contains(&RaySolverMethod::RepeatAwareTraversal)
    );
    assert_eq!(repeat_grid_trace.observability.solver_repeat_attempts, 0);
    assert_eq!(repeat_grid_trace.observability.solver_repeat_supported, 0);
    assert_eq!(
        repeat_grid_trace.observability.solver_repeat_inapplicable,
        0
    );
    assert_eq!(repeat_grid_trace.observability.solver_repeat_unsupported, 0);

    let (radial_repeat_hit, radial_repeat_trace) = world_trace(
        "radial_repeat_region",
        [0.0, 0.0, -5.0],
        [0.0, 0.0, 1.0],
        12.0,
    );
    let radial_repeat_hit = expect_struct(&radial_repeat_hit, "Hit3");
    let radial_repeat_payload = expect_struct(field(radial_repeat_hit, "payload"), "Payload");
    assert!(expect_bool(field(radial_repeat_hit, "hit")));
    assert_eq!(expect_u32(field(radial_repeat_payload, "entity_id")), 39);
    assert!(radial_repeat_trace.observability.ray_support_entry_jumps > 0);
    assert!(
        !radial_repeat_trace
            .observability
            .solver_methods
            .contains(&RaySolverMethod::RepeatAwareTraversal)
    );
    assert_eq!(radial_repeat_trace.observability.solver_repeat_attempts, 0);
    assert_eq!(radial_repeat_trace.observability.solver_repeat_supported, 0);
    assert_eq!(
        radial_repeat_trace.observability.solver_repeat_inapplicable,
        0
    );
    assert_eq!(
        radial_repeat_trace.observability.solver_repeat_unsupported,
        0
    );

    let (miss_hit, miss_trace) = world_trace("miss_region", [0.0, 0.0, 0.0], [1.0, 0.0, 0.0], 6.0);
    let miss_hit = expect_struct(&miss_hit, "Hit3");
    assert!(!expect_bool(field(miss_hit, "hit")));
    assert_eq!(miss_trace.observability.candidate_count, 0);
    assert!(miss_trace.observability.support_pruned_candidates > 0);
    assert!(miss_trace.observability.ray_support_interval_rejections > 0);

    let (mixed_repeat_hit, mixed_repeat_trace) = world_trace(
        "mixed_repeat_region",
        [-6.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        12.0,
    );
    let mixed_repeat_hit = expect_struct(&mixed_repeat_hit, "Hit3");
    let mixed_repeat_payload = expect_struct(field(mixed_repeat_hit, "payload"), "Payload");
    assert!(expect_bool(field(mixed_repeat_hit, "hit")));
    assert_eq!(expect_u32(field(mixed_repeat_payload, "entity_id")), 37);
    assert!(mixed_repeat_trace.observability.ray_support_entry_jumps > 0);
}

#[test]
fn query_exec_cpu_repeat_linear_supported_subset_reduces_hit_side_field_samples() {
    let source = r#"
field conservative distance probe_repeat_field(p: Vec3) -> F32 {
    repeat_linear = vec3(12.0, 0.0, 0.0) {
        translate = vec3(6.0, 0.0, 0.0) {
            sphere(radius = 0.18)
        }
    }
}

material shade(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(0.2, 0.3, 0.4),
        roughness=0.5,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

shape probe_repeat_shape {
    field = probe_repeat_field
    material = shade
    payload = Payload(
        entity_id=u32(88),
        material_id=u32(88),
        actor=ActorHandle(id=u32(88), generation=u32(0))
    )
}

region probe_repeat_region() {
    place repeated = probe_repeat_shape
}

domain probe_domain(world: RegionCapture) {
    geometry_detail = 1
    material = false
    radiance = false
    media = false
    max_distance = 30.0
    min_step = 0.02
    hit_epsilon = 0.001
    max_steps = 256
}
"#;
    let (_, _, ctx) = typed_query_module(source);
    let region_scene_id = stable_region_scene_capture_id(&SmolStr::new("probe_repeat_region"));
    let domain = scene_domain(region_scene_id, 1, false, false, false);
    let world_plan = lower_world_query_plan(&WorldQueryPlan::for_query(WorldQueryKind::Trace));
    let capture_plan = lower_capture_query_plan(
        &CaptureQueryPlan::for_query(CaptureQueryKind::Trace, CaptureKind::Shape, None)
            .expect("shape trace plan"),
    );

    let region_capture = KernelValue::Capture(SmolStr::new("probe_repeat_region"));
    let shape_capture = KernelValue::Capture(SmolStr::new("probe_repeat_shape"));

    let mut world_hits = 0u32;
    let mut dense_hits = 0u32;
    let mut world_field_samples = 0u32;
    let mut dense_field_samples = 0u32;
    let mut world_steps = 0u32;
    let mut dense_steps = 0u32;
    let mut world_repeat_skips = 0u32;

    for sample_i in 0..64 {
        let py = (sample_i % 8) as f32 * 0.03 - 0.105;
        let pz = (sample_i / 8) as f32 * 0.035 - 0.12;
        let ray = ray_query_with_limits([-15.0, py, pz], [1.0, 0.0, 0.0], 30.0, 0.02, 0.001, 256);

        let (world_hit, world_trace) = execute_world_query_with_trace_on(
            &ctx,
            DispatchBackend::Cpu,
            &world_plan,
            &[region_capture.clone(), domain.clone(), ray.clone()],
        )
        .expect("world repeat trace");
        let (dense_hit, dense_trace) = execute_capture_query_with_trace_on(
            &ctx,
            DispatchBackend::Cpu,
            &capture_plan,
            &[shape_capture.clone(), ray],
        )
        .expect("dense repeat trace");

        let world_hit_ref = expect_struct(&world_hit, "Hit3");
        let dense_hit_ref = expect_struct(&dense_hit, "Hit3");
        let world_did_hit = expect_bool(field(world_hit_ref, "hit"));
        let dense_did_hit = expect_bool(field(dense_hit_ref, "hit"));
        assert_eq!(
            world_did_hit, dense_did_hit,
            "world_hit={world_did_hit} dense_hit={dense_did_hit} world_obs={:?}",
            world_trace.observability
        );
        if world_did_hit {
            world_hits += 1;
            dense_hits += 1;
            assert!(
                (expect_f32(field(world_hit_ref, "distance"))
                    - expect_f32(field(dense_hit_ref, "distance")))
                .abs()
                    < 0.02
            );
            assert_eq!(
                expect_u32(field(world_hit_ref, "repeat_id")),
                expect_u32(field(dense_hit_ref, "repeat_id"))
            );
            assert_eq!(
                expect_u32(field(world_hit_ref, "instance_id")),
                expect_u32(field(dense_hit_ref, "instance_id"))
            );
        }

        world_field_samples += world_trace.observability.field_samples;
        dense_field_samples += dense_trace.observability.field_samples;
        world_steps += world_trace.observability.trace_steps;
        dense_steps += dense_trace.observability.trace_steps;
        world_repeat_skips += world_trace.observability.repeat_cell_skips;
    }

    assert_eq!(world_hits, dense_hits);
    assert!(world_hits > 0);
    assert!(world_field_samples < dense_field_samples);
    assert!(world_steps < dense_steps);
    assert!(world_repeat_skips > 0);

    let probe_ray = ray_query_with_limits(
        [-15.0, 0.0, -0.015],
        [1.0, 0.0, 0.0],
        30.0,
        0.02,
        0.001,
        256,
    );
    let (_probe_hit, probe_trace) = execute_world_query_with_trace_on(
        &ctx,
        DispatchBackend::Cpu,
        &world_plan,
        &[region_capture, domain, probe_ray],
    )
    .expect("probe repeat trace");
    assert!(
        probe_trace
            .observability
            .solver_methods
            .contains(&RaySolverMethod::RepeatAwareTraversal)
    );
    assert!(probe_trace.observability.repeat_cell_skips > 0);
}

#[test]
fn query_exec_cpu_large_union_distance_uses_subtree_pruning() {
    let (_, _, ctx) = typed_query_module(large_union_distance_fixture_source());
    let plan = lower_capture_query_plan(
        &CaptureQueryPlan::for_query(CaptureQueryKind::Distance, CaptureKind::Shape, None)
            .expect("shape capture distance plan"),
    );

    let (cpu_value, cpu_trace) = execute_capture_query_with_trace_on(
        &ctx,
        DispatchBackend::Cpu,
        &plan,
        &[
            KernelValue::Capture(SmolStr::new("large_union_shape")),
            KernelValue::Vec3([0.5, 0.0, 0.0]),
        ],
    )
    .expect("cpu large union distance");
    let wgsl_value = execute_capture_query_on(
        &ctx,
        DispatchBackend::Wgsl,
        &plan,
        &[
            KernelValue::Capture(SmolStr::new("large_union_shape")),
            KernelValue::Vec3([0.5, 0.0, 0.0]),
        ],
    )
    .expect("wgsl large union distance");

    assert_approx_eq(expect_f32(&cpu_value), expect_f32(&wgsl_value));
    assert!(cpu_trace.observability.union_cluster_visits > 0);
    assert!(cpu_trace.observability.shape_leaf_visits < 6);
    assert!(cpu_trace.observability.acceleration_pruned_nodes > 0);
    let rendered = render_semantic_cost_report(&cpu_trace.cost_report);
    assert!(rendered.contains("shape_leaf_visits="));
    assert!(rendered.contains("acceleration_pruned_nodes="));
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
    assert_eq!(miss_trace.observability.solver_dense_fallback_rays, 0);
    assert!(miss_trace.observability.support_pruned_candidates > 0);
    assert!(miss_trace.observability.ray_support_interval_rejections > 0);
}

#[test]
fn query_exec_cpu_transformed_analytic_primitives_match_dense_capture_oracles() {
    let (_, _, ctx) = typed_query_module(transformed_analytic_primitives_fixture_source());
    let capture_plan = lower_capture_query_plan(
        &CaptureQueryPlan::for_query(CaptureQueryKind::Trace, CaptureKind::Shape, None)
            .expect("capture trace plan"),
    );
    let world_plan = lower_world_query_plan(&WorldQueryPlan::for_query(WorldQueryKind::Trace));

    for (shape_name, region_name, origin, entity_id) in [
        (
            "translated_box_shape",
            "translated_box_region",
            [1.25, -0.10, 3.0],
            71u32,
        ),
        (
            "rotated_capsule_shape",
            "rotated_capsule_region",
            [0.18, 0.55, 3.0],
            72u32,
        ),
        (
            "rotated_cylinder_shape",
            "rotated_cylinder_region",
            [0.22, 0.4, 3.0],
            73u32,
        ),
        (
            "scaled_sphere_shape",
            "scaled_sphere_region",
            [0.0, 0.0, 3.0],
            74u32,
        ),
    ] {
        let ray = ray_query_with_limits(origin, [0.0, 0.0, -1.0], 8.0, 0.02, 0.001, 128);
        let dense_oracle = execute_capture_query(
            &ctx,
            &capture_plan,
            &[KernelValue::Capture(SmolStr::new(shape_name)), ray.clone()],
        )
        .unwrap_or_else(|error| panic!("dense capture oracle for {shape_name}: {error:?}"));
        let (solver_hit, solver_trace) = execute_world_query_with_trace_on(
            &ctx,
            DispatchBackend::Cpu,
            &world_plan,
            &[
                KernelValue::Capture(SmolStr::new(region_name)),
                scene_domain(
                    stable_region_scene_capture_id(&SmolStr::new(region_name)),
                    1,
                    true,
                    false,
                    false,
                ),
                ray,
            ],
        )
        .unwrap_or_else(|error| panic!("solver world hit for {region_name}: {error:?}"));

        let dense_ref = expect_struct(&dense_oracle, "Hit3");
        let solver_ref = expect_struct(&solver_hit, "Hit3");
        assert_eq!(
            expect_bool(field(dense_ref, "hit")),
            expect_bool(field(solver_ref, "hit")),
            "hit parity for {shape_name}"
        );
        assert!(
            expect_bool(field(solver_ref, "hit")),
            "expected transformed analytic world hit for {shape_name}"
        );
        let dense_distance = expect_f32(field(dense_ref, "distance"));
        let solver_distance = expect_f32(field(solver_ref, "distance"));
        assert!(
            (dense_distance - solver_distance).abs() < 0.02,
            "distance parity for {shape_name}: dense={dense_distance} solver={solver_distance}"
        );
        let solver_hit_ref = expect_struct(&solver_hit, "Hit3");
        let solver_payload = expect_struct(field(solver_hit_ref, "payload"), "Payload");
        assert_eq!(expect_u32(field(solver_payload, "entity_id")), entity_id);
        assert_eq!(solver_trace.observability.solver_analytic_hits, 1);
        assert!(solver_trace.observability.analytic_transformed_hits > 0);
        assert_eq!(solver_trace.observability.solver_dense_fallback_rays, 0);
        assert!(
            solver_trace
                .observability
                .step_certificate_kinds
                .get(&StepCertificateKind::AnalyticHit)
                .copied()
                .unwrap_or_default()
                > 0
        );
    }
}

#[test]
fn query_exec_cpu_torus_default_solver_stays_on_dense_certificate_path() {
    let (_, _, ctx) = typed_query_module(relaxed_torus_solver_fixture_source());
    let capture_plan = lower_capture_query_plan(
        &CaptureQueryPlan::for_query(CaptureQueryKind::Trace, CaptureKind::Shape, None)
            .expect("capture trace plan"),
    );
    let world_plan = lower_world_query_plan(&WorldQueryPlan::for_query(WorldQueryKind::Trace));
    let region_capture = KernelValue::Capture(SmolStr::new("relaxed_torus_region"));
    let shape_capture = KernelValue::Capture(SmolStr::new("relaxed_torus_shape"));
    let domain = scene_domain(
        stable_region_scene_capture_id(&SmolStr::new("relaxed_torus_region")),
        1,
        true,
        false,
        false,
    );

    let mut world_steps = 0u32;
    let mut dense_steps = 0u32;
    let mut world_field_samples = 0u32;
    let mut dense_field_samples = 0u32;
    let mut support_entry_jumps = 0u32;
    let mut hit_count = 0u32;
    let mut max_distance_delta = 0.0f32;

    for sample_i in 1..257 {
        let px = (sample_i % 16) as f32 / 15.0 - 0.5;
        let py = ((sample_i / 16) % 16) as f32 / 15.0 - 0.5;
        let ray = ray_query_with_limits(
            [px * 1.6, py * 1.4, 3.2],
            normalize3([-px * 0.08, -py * 0.06, -1.0]),
            10.0,
            0.03,
            0.001,
            128,
        );
        let (world_hit, world_trace) = execute_world_query_with_trace_on(
            &ctx,
            DispatchBackend::Cpu,
            &world_plan,
            &[region_capture.clone(), domain.clone(), ray.clone()],
        )
        .expect("world relaxed torus trace");
        let (dense_hit, dense_trace) = execute_capture_query_with_trace_on(
            &ctx,
            DispatchBackend::Cpu,
            &capture_plan,
            &[shape_capture.clone(), ray],
        )
        .expect("dense relaxed torus trace");

        let world_hit_ref = expect_struct(&world_hit, "Hit3");
        let dense_hit_ref = expect_struct(&dense_hit, "Hit3");
        let world_did_hit = expect_bool(field(world_hit_ref, "hit"));
        let dense_did_hit = expect_bool(field(dense_hit_ref, "hit"));
        assert_eq!(
            world_did_hit, dense_did_hit,
            "sample={sample_i} world_hit={world_did_hit} dense_hit={dense_did_hit}"
        );
        if world_did_hit {
            hit_count += 1;
            max_distance_delta = max_distance_delta.max(
                (expect_f32(field(world_hit_ref, "distance"))
                    - expect_f32(field(dense_hit_ref, "distance")))
                .abs(),
            );
        }

        world_steps += world_trace.observability.trace_steps;
        dense_steps += dense_trace.observability.trace_steps;
        world_field_samples += world_trace.observability.field_samples;
        dense_field_samples += dense_trace.observability.field_samples;
        support_entry_jumps += world_trace.observability.ray_support_entry_jumps;
        assert!(
            !world_trace
                .observability
                .solver_methods
                .contains(&RaySolverMethod::LipschitzSafeStepping)
        );
        assert!(
            !world_trace
                .observability
                .solver_methods
                .contains(&RaySolverMethod::IntervalNewtonIsolation)
        );
        assert!(
            !world_trace
                .observability
                .solver_methods
                .contains(&RaySolverMethod::SafeguardedNewtonRefinement)
        );
    }

    assert!(hit_count > 0);
    assert!(world_steps <= dense_steps);
    assert!(world_field_samples <= dense_field_samples);
    assert!(support_entry_jumps > 0);
    assert!(
        max_distance_delta < 0.05,
        "torus default path drifted too far from the dense oracle: {max_distance_delta}"
    );
}

#[test]
fn query_exec_cpu_translated_repeat_linear_supported_subset_reduces_hit_side_field_samples() {
    let (_, _, ctx) = typed_query_module(translated_repeat_linear_solver_fixture_source());
    let region_scene_id = stable_region_scene_capture_id(&SmolStr::new("translated_repeat_region"));
    let domain = scene_domain(region_scene_id, 1, false, false, false);
    let world_plan = lower_world_query_plan(&WorldQueryPlan::for_query(WorldQueryKind::Trace));
    let capture_plan = lower_capture_query_plan(
        &CaptureQueryPlan::for_query(CaptureQueryKind::Trace, CaptureKind::Shape, None)
            .expect("shape trace plan"),
    );

    let region_capture = KernelValue::Capture(SmolStr::new("translated_repeat_region"));
    let shape_capture = KernelValue::Capture(SmolStr::new("translated_repeat_shape"));

    let mut world_hits = 0u32;
    let mut dense_hits = 0u32;
    let mut world_field_samples = 0u32;
    let mut dense_field_samples = 0u32;
    let mut world_steps = 0u32;
    let mut dense_steps = 0u32;
    let mut repeat_supported = 0u32;
    let mut repeat_cells_enumerated = 0u32;

    for sample_i in 0..64 {
        let py = (sample_i % 8) as f32 * 0.03 - 0.105;
        let pz = (sample_i / 8) as f32 * 0.035 - 0.12;
        let ray = ray_query_with_limits([-15.0, py, pz], [1.0, 0.0, 0.0], 30.0, 0.02, 0.001, 256);

        let (world_hit, world_trace) = execute_world_query_with_trace_on(
            &ctx,
            DispatchBackend::Cpu,
            &world_plan,
            &[region_capture.clone(), domain.clone(), ray.clone()],
        )
        .expect("world translated repeat trace");
        let (dense_hit, dense_trace) = execute_capture_query_with_trace_on(
            &ctx,
            DispatchBackend::Cpu,
            &capture_plan,
            &[shape_capture.clone(), ray],
        )
        .expect("dense translated repeat trace");

        let world_hit_ref = expect_struct(&world_hit, "Hit3");
        let dense_hit_ref = expect_struct(&dense_hit, "Hit3");
        let world_did_hit = expect_bool(field(world_hit_ref, "hit"));
        let dense_did_hit = expect_bool(field(dense_hit_ref, "hit"));
        assert_eq!(world_did_hit, dense_did_hit);
        if world_did_hit {
            world_hits += 1;
            dense_hits += 1;
            assert!(
                (expect_f32(field(world_hit_ref, "distance"))
                    - expect_f32(field(dense_hit_ref, "distance")))
                .abs()
                    < 0.02
            );
            assert_eq!(
                expect_u32(field(world_hit_ref, "repeat_id")),
                expect_u32(field(dense_hit_ref, "repeat_id"))
            );
            assert_eq!(
                expect_u32(field(world_hit_ref, "instance_id")),
                expect_u32(field(dense_hit_ref, "instance_id"))
            );
        }

        world_field_samples += world_trace.observability.field_samples;
        dense_field_samples += dense_trace.observability.field_samples;
        world_steps += world_trace.observability.trace_steps;
        dense_steps += dense_trace.observability.trace_steps;
        repeat_supported += world_trace.observability.solver_repeat_supported;
        repeat_cells_enumerated += world_trace.observability.solver_repeat_cells_enumerated;
    }

    assert_eq!(world_hits, dense_hits);
    assert!(world_hits > 0);
    assert!(world_field_samples < dense_field_samples);
    assert!(world_steps < dense_steps);
    assert!(repeat_supported > 0);
    assert!(repeat_cells_enumerated > 0);
}

#[test]
fn query_exec_cpu_repeat_linear_unbounded_child_reports_runtime_repeat_fallback() {
    let source = r#"
field conservative distance unbounded_repeat_field(p: Vec3) -> F32 {
    repeat_linear = vec3(2.0, 0.0, 0.0) {
        plane(normal = vec3(1.0, 0.0, 0.0), offset = -0.25)
    }
}

material shade(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(0.2, 0.3, 0.4),
        roughness=0.5,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

shape unbounded_repeat_shape {
    field = unbounded_repeat_field
    material = shade
    payload = Payload(
        entity_id=u32(90),
        material_id=u32(90),
        actor=ActorHandle(id=u32(90), generation=u32(0))
    )
}

region unbounded_repeat_region() {
    place repeated = unbounded_repeat_shape
}

domain probe_domain(world: RegionCapture) {
    geometry_detail = 1
    material = false
    radiance = false
    media = false
    max_distance = 6.0
    min_step = 0.02
    hit_epsilon = 0.001
    max_steps = 128
}
"#;
    let (_, _, ctx) = typed_query_module(source);
    let region_scene_id = stable_region_scene_capture_id(&SmolStr::new("unbounded_repeat_region"));
    let domain = scene_domain(region_scene_id, 1, false, false, false);
    let world_plan = lower_world_query_plan(&WorldQueryPlan::for_query(WorldQueryKind::Trace));

    let (hit, trace) = execute_world_query_with_trace_on(
        &ctx,
        DispatchBackend::Cpu,
        &world_plan,
        &[
            KernelValue::Capture(SmolStr::new("unbounded_repeat_region")),
            domain,
            ray_query_with_limits([-4.0, 0.0, 0.0], [1.0, 0.0, 0.0], 6.0, 0.02, 0.001, 128),
        ],
    )
    .expect("unbounded repeat trace");

    assert!(expect_bool(field(expect_struct(&hit, "Hit3"), "hit")));
    assert_eq!(trace.observability.solver_repeat_attempts, 1);
    assert_eq!(trace.observability.solver_repeat_supported, 0);
    assert_eq!(trace.observability.solver_repeat_inapplicable, 0);
    assert_eq!(trace.observability.solver_repeat_unsupported, 1);
    assert_eq!(trace.observability.solver_repeat_unsupported_form, 0);
    assert_eq!(trace.observability.solver_repeat_unsupported_bounds, 1);
    assert_eq!(trace.observability.solver_repeat_cells_enumerated, 0);
}

#[test]
fn query_exec_cpu_repeat_linear_unsupported_form_reports_runtime_reason() {
    let source = r#"
field conservative distance rotated_repeat_field(p: Vec3) -> F32 {
    rotate = vec3(0.0, 0.0, 0.5) {
        repeat_linear = vec3(2.0, 0.0, 0.0) {
            sphere(radius = 0.25)
        }
    }
}

material shade(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(0.2, 0.3, 0.4),
        roughness=0.5,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

shape rotated_repeat_shape {
    field = rotated_repeat_field
    material = shade
    payload = Payload(
        entity_id=u32(91),
        material_id=u32(91),
        actor=ActorHandle(id=u32(91), generation=u32(0))
    )
}

region rotated_repeat_region() {
    place repeated = rotated_repeat_shape
}

domain probe_domain(world: RegionCapture) {
    geometry_detail = 1
    material = false
    radiance = false
    media = false
    max_distance = 6.0
    min_step = 0.02
    hit_epsilon = 0.001
    max_steps = 128
}
"#;
    let (_, _, ctx) = typed_query_module(source);
    let region_scene_id = stable_region_scene_capture_id(&SmolStr::new("rotated_repeat_region"));
    let domain = scene_domain(region_scene_id, 1, false, false, false);
    let world_plan = lower_world_query_plan(&WorldQueryPlan::for_query(WorldQueryKind::Trace));

    let (hit, trace) = execute_world_query_with_trace_on(
        &ctx,
        DispatchBackend::Cpu,
        &world_plan,
        &[
            KernelValue::Capture(SmolStr::new("rotated_repeat_region")),
            domain,
            ray_query_with_limits([0.0, 0.0, 2.0], [0.0, 0.0, -1.0], 6.0, 0.02, 0.001, 128),
        ],
    )
    .expect("rotated repeat trace");

    assert!(expect_bool(field(expect_struct(&hit, "Hit3"), "hit")));
    assert_eq!(trace.observability.solver_repeat_attempts, 1);
    assert_eq!(trace.observability.solver_repeat_supported, 0);
    assert_eq!(trace.observability.solver_repeat_inapplicable, 0);
    assert_eq!(trace.observability.solver_repeat_unsupported, 1);
    assert_eq!(trace.observability.solver_repeat_unsupported_form, 1);
    assert_eq!(trace.observability.solver_repeat_unsupported_bounds, 0);
    assert_eq!(trace.observability.solver_repeat_cells_enumerated, 0);
}
