use super::*;

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

#[test]
fn frame_state_value_with_history_uses_explicit_snapshot_epochs() {
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
    let value = frame_state_value_with_history(
        camera,
        camera,
        viewport,
        viewport,
        [0.0, 0.0],
        [0.0, 0.0],
        4,
        3,
        1.0 / 60.0,
        false,
        SnapshotEpoch(7),
        SnapshotEpoch(3),
    );
    let frame = expect_struct(&value, "FrameState");
    let transition = expect_struct(
        field(frame, "snapshot_transition"),
        "SnapshotTransitionContext",
    );
    let current = expect_struct(field(transition, "current_snapshot_epoch"), "SnapshotEpoch");
    let previous = expect_struct(
        field(transition, "previous_snapshot_epoch"),
        "SnapshotEpoch",
    );
    assert_eq!(u32_field(current, "epoch"), 7);
    assert_eq!(u32_field(previous, "epoch"), 3);
}

pub(super) fn temporal_alias_source() -> &'static str {
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

pub(super) fn temporal_disocclusion_source() -> &'static str {
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
    assert_eq!(result.metrics.gpu_runtime, result.frame_cost.gpu_runtime);
    assert_eq!(
        result.metrics.gpu_runtime.timestamps_supported,
        result.metrics.gpu_runtime.timestamped_pass_count > 0
    );
    assert!(result.metrics.candidates_before_pruning >= 16);
    assert!(result.metrics.trace_steps_max > 0);
    assert_eq!(
        result.metrics.dense_fallback_count,
        result.query_trace.observability.solver_dense_fallback_rays
            + result
                .query_trace
                .observability
                .solver_generated_dense_fallback_rays
    );
    assert!(result.metrics.acceleration_node_visits > 0);
    assert!(result.metrics.cache_brick_visits > 0);
    assert_eq!(
        result.metrics.interval_subdivisions,
        result.query_trace.observability.interval_subdivisions
    );
    assert!(result.frame_cost.acceleration_node_visits > 0);
    assert!(result.frame_cost.cache_brick_visits > 0);
    assert_eq!(
        result.frame_cost.interval_subdivisions,
        result.query_trace.observability.interval_subdivisions
    );
    let rendered_frame_cost =
        wrela::presentation_exec::render_frame_cost_report(&result.frame_cost);
    assert!(rendered_frame_cost.contains("acceleration_node_visits="));
    assert!(rendered_frame_cost.contains("cache_brick_visits="));
    assert!(rendered_frame_cost.contains("interval_subdivisions="));
    assert!(rendered_frame_cost.contains("solver_relaxed_attempts="));
    assert!(rendered_frame_cost.contains("solver_interval_attempts="));
    assert!(rendered_frame_cost.contains("solver_repeat_cells_enumerated="));
    assert!(rendered_frame_cost.contains("frame_timing cpu_time_total_micros="));
    assert!(rendered_frame_cost.contains("timestamps_supported="));
    assert!(rendered_frame_cost.contains("gpu_runtime timestamped_pass_count="));
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
fn cpu_first_color_frame_cost_report_exposes_solver_counters() {
    let (plan, ctx, input) = presentation_fixture(DispatchBackend::Cpu);
    let result = execute_plan(&ctx, &plan, &input).expect("cpu presentation execution");
    let rendered_frame_cost =
        wrela::presentation_exec::render_frame_cost_report(&result.frame_cost);
    assert!(rendered_frame_cost.contains("solver_relaxed_attempts="));
    assert!(rendered_frame_cost.contains("solver_interval_attempts="));
    assert!(rendered_frame_cost.contains("solver_repeat_cells_enumerated="));
    assert!(rendered_frame_cost.contains("frame_timing cpu_time_total_micros="));
    assert!(rendered_frame_cost.contains("timestamps_supported="));
    assert!(rendered_frame_cost.contains("gpu_runtime timestamped_pass_count="));
    let frame_cost_json =
        serde_json::to_value(&result.frame_cost).expect("serialize frame cost report");
    assert_eq!(
        frame_cost_json
            .pointer("/solver_relaxed_attempts")
            .and_then(|value| value.as_u64()),
        Some(0)
    );
    assert_eq!(
        frame_cost_json
            .pointer("/solver_repeat_cells_enumerated")
            .and_then(|value| value.as_u64()),
        Some(0)
    );
}

#[test]
fn cpu_presentation_attachment_reports_make_storage_and_precision_policy_explicit() {
    let (plan, ctx, input) = presentation_fixture(DispatchBackend::Cpu);
    let result = execute_plan(&ctx, &plan, &input).expect("cpu presentation execution");

    assert!(
        result
            .frame_cost
            .attachment_bytes
            .iter()
            .all(|attachment| attachment.backing.starts_with("cpu_bytes("))
    );
    assert!(
        result
            .frame_cost
            .attachment_bytes
            .iter()
            .all(|attachment| attachment.backing.contains("storage=buffer"))
    );
    assert!(
        result
            .frame_cost
            .attachment_bytes
            .iter()
            .all(|attachment| attachment.backing.contains("precision=f32"))
    );
    let color_attachment = result
        .frame_cost
        .attachment_bytes
        .iter()
        .find(|attachment| attachment.attachment == "color")
        .expect("color attachment report");
    assert!(color_attachment.backing.contains("optional_precision=f16"));
    let depth_attachment = result
        .frame_cost
        .attachment_bytes
        .iter()
        .find(|attachment| attachment.attachment == "depth")
        .expect("depth attachment report");
    assert!(!depth_attachment.backing.contains("optional_precision=f16"));
    assert_eq!(
        result
            .attachments
            .attachment("depth")
            .expect("depth attachment")
            .layout
            .attachment
            .policy_description(),
        "storage=buffer precision=f32"
    );
}

#[test]
fn why_not_120_findings_reflect_shared_gpu_runtime_churn() {
    let (plan, ctx, input) = presentation_fixture(DispatchBackend::Cpu);
    let result = execute_plan(&ctx, &plan, &input).expect("cpu presentation execution");
    let mut report = result.frame_cost.clone();
    report.gpu_runtime.cpu_screen_sample_allocations =
        report.output_width.saturating_mul(report.output_height);
    report.gpu_runtime.upload_bytes = 1_500_000;
    report.gpu_runtime.readback_bytes = 1_500_000;
    report.gpu_runtime.dispatch_fragmentation_count = 3;
    report.gpu_runtime.scene_reupload_bytes = 1_500_000;

    let findings =
        wrela::presentation_exec::cost::explain_why_not_120_findings(&report, None, None, 8.0, 8.0);
    let focuses = findings
        .iter()
        .map(|finding| finding.focus.as_str())
        .collect::<Vec<_>>();
    assert!(focuses.contains(&"cpu_primary_setup"));
    assert!(focuses.contains(&"cpu_gpu_churn"));
    assert!(focuses.contains(&"dispatch_fragmentation"));
    assert!(focuses.contains(&"steady_state_scene_reupload"));
}
