use super::*;

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
fn wgsl_resident_framegraph_reports_gpu_attachment_backing_without_cpu_followup_exception() {
    let (mut plan, ctx, mut input) = presentation_fixture(DispatchBackend::Wgsl);
    strip_export_passes(&mut plan);
    input.materialize_cpu_attachments = false;
    let result = execute_plan(&ctx, &plan, &input).expect("wgsl presentation execution");

    assert!(result.screen_samples.is_empty());
    assert_eq!(
        result.frame_cost.gpu_runtime.cpu_screen_sample_allocations,
        0
    );
    assert_eq!(result.frame_cost.gpu_runtime.attachment_decode_count, 0);
    assert_eq!(result.frame_cost.gpu_runtime.readback_bytes, 0);
    assert_eq!(result.frame_cost.gpu_runtime.queue_submit_count, 1);
    assert!(
        !result
            .frame_cost
            .framegraph_exceptions
            .iter()
            .any(|exception| exception == "cpu_followup_query_passes")
    );
    assert!(
        result
            .frame_cost
            .attachment_bytes
            .iter()
            .all(|attachment| attachment.backing.starts_with("gpu_buffer("))
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
        .expect("wgsl color attachment report");
    assert!(color_attachment.backing.contains("optional_precision=f16"));
    let primary_writeout = result
        .frame_cost
        .passes
        .iter()
        .find(|pass| pass.pass_kind == "primary_writeout")
        .expect("wgsl primary writeout pass");
    assert!(primary_writeout.dispatch_count > 0);
    let surface_resolve = result
        .frame_cost
        .passes
        .iter()
        .find(|pass| pass.pass_kind == "surface_resolve")
        .expect("wgsl surface resolve pass");
    if result.frame_cost.gpu_runtime.timestamped_pass_count > 0 {
        assert!(surface_resolve.gpu_elapsed_micros.is_some());
    } else {
        assert!(surface_resolve.dispatch_count > 0);
    }
    let participants_resolve = result
        .frame_cost
        .passes
        .iter()
        .find(|pass| pass.pass_kind == "participants_resolve")
        .expect("wgsl participants resolve pass");
    if result.frame_cost.gpu_runtime.timestamped_pass_count > 0 {
        assert!(participants_resolve.gpu_elapsed_micros.is_some());
    } else {
        assert!(participants_resolve.dispatch_count > 0);
    }

    let report = wrela::presentation_exec::render_frame_cost_report(&result.frame_cost);
    assert!(report.contains("backing=gpu_buffer"));
    assert!(!report.contains("cpu_followup_query_passes"));
}

#[test]
fn wgsl_no_export_lane_avoids_full_attachment_readback() {
    let (mut plan, ctx, input) = presentation_fixture(DispatchBackend::Wgsl);
    strip_export_passes(&mut plan);
    let fully_materialized =
        execute_plan(&ctx, &plan, &input).expect("wgsl materialized presentation execution");

    let mut resident_input = input.clone();
    resident_input.materialize_cpu_attachments = false;
    let resident =
        execute_plan(&ctx, &plan, &resident_input).expect("wgsl resident presentation execution");

    assert!(resident.history.is_some());
    let history = resident
        .history
        .as_ref()
        .expect("resident temporal history");
    for slot in &history.slots {
        assert_eq!(
            history
                .attachments
                .decode_attachment(slot.attachment.as_str())
                .expect("history slot attachment"),
            resident
                .attachments
                .decode_attachment(slot.attachment.as_str())
                .expect("resident attachment"),
            "resident history should reflect the post-frame GPU attachment state"
        );
    }
    assert_eq!(resident.frame_cost.gpu_runtime.readback_bytes, 0);
    assert_eq!(fully_materialized.frame_cost.gpu_runtime.readback_bytes, 0);
    assert_eq!(
        resident
            .frame_cost
            .gpu_runtime
            .cpu_screen_sample_allocations,
        0
    );
    assert_eq!(resident.frame_cost.gpu_runtime.attachment_decode_count, 0);
}

#[test]
fn wgsl_shader_f16_gate_preserves_color_and_records_fallback_state() {
    let _lock = workgroup_override_test_lock();
    let _guard = override_shader_f16_for_current_thread(Some(true));
    let (cpu_plan, cpu_ctx, cpu_input) = presentation_fixture(DispatchBackend::Cpu);
    let (wgsl_plan, wgsl_ctx, wgsl_input) = presentation_fixture(DispatchBackend::Wgsl);

    let cpu = execute_plan(&cpu_ctx, &cpu_plan, &cpu_input).expect("cpu presentation execution");
    let wgsl = execute_plan(&wgsl_ctx, &wgsl_plan, &wgsl_input).expect("wgsl f16-gated execution");

    let shader_f16_enabled = wgsl
        .frame_cost
        .gpu_runtime
        .enabled_optional_features
        .iter()
        .any(|feature| feature == "shader_f16");
    let cpu_color = cpu
        .attachments
        .decode_attachment("color")
        .expect("cpu color attachment");
    let wgsl_color = wgsl
        .attachments
        .decode_attachment("color")
        .expect("wgsl color attachment");
    if shader_f16_enabled {
        assert_attachment_vec3_approx_eq(&cpu_color, &wgsl_color, 0.08, "shader f16 parity");
    } else {
        assert!(
            !wgsl
                .frame_cost
                .gpu_runtime
                .enabled_optional_features
                .iter()
                .any(|feature| feature == "shader_f16")
        );
        assert_attachment_vec3_approx_eq(&cpu_color, &wgsl_color, 0.02, "shader f16 fallback");
    }
}

#[test]
fn wgsl_resident_optional_feature_fallbacks_remain_explicit_without_subgroups_or_indirect_dispatch()
{
    let _lock = workgroup_override_test_lock();
    let (cpu_plan, cpu_ctx, cpu_input) = presentation_fixture(DispatchBackend::Cpu);
    let (wgsl_plan, wgsl_ctx, wgsl_input) = presentation_fixture(DispatchBackend::Wgsl);

    let cpu = execute_plan(&cpu_ctx, &cpu_plan, &cpu_input).expect("cpu presentation execution");
    let wgsl =
        execute_plan(&wgsl_ctx, &wgsl_plan, &wgsl_input).expect("wgsl presentation execution");
    let features = &wgsl.frame_cost.gpu_runtime.enabled_optional_features;

    assert!(
        !features.iter().any(|feature| feature == "subgroup"),
        "resident presentation path should keep subgroup fallback explicit until subgroup kernels exist"
    );
    assert!(
        !features
            .iter()
            .any(|feature| feature == "indirect_dispatch"),
        "resident presentation path should keep indirect-dispatch fallback explicit while the closure lane stays direct"
    );
    if wgsl.frame_cost.gpu_runtime.timestamps_supported {
        assert!(
            wgsl.frame_cost.gpu_runtime.timestamped_pass_count > 0,
            "timestamp-capable adapters should record timestamped passes"
        );
        assert!(
            features.iter().any(|feature| feature == "timestamp_query"),
            "timestamp support should be surfaced in the optional-feature list"
        );
    } else {
        assert_eq!(wgsl.frame_cost.gpu_runtime.timestamped_pass_count, 0);
        assert!(
            !features.iter().any(|feature| feature == "timestamp_query"),
            "timestamp fallback should remain explicit when timestamp queries are unavailable"
        );
        assert!(
            wgsl.frame_cost
                .passes
                .iter()
                .all(|pass| pass.gpu_elapsed_micros.is_none())
        );
    }

    let cpu_color = cpu
        .attachments
        .decode_attachment("color")
        .expect("cpu color attachment");
    let wgsl_color = wgsl
        .attachments
        .decode_attachment("color")
        .expect("wgsl color attachment");
    assert_attachment_vec3_approx_eq(
        &cpu_color,
        &wgsl_color,
        0.02,
        "resident feature fallback parity",
    );
}

#[test]
fn wgsl_custom_pass_pipelines_record_cache_hits_after_warm_frame() {
    let (plan, ctx, input) = presentation_fixture(DispatchBackend::Wgsl);
    let cold = execute_plan(&ctx, &plan, &input).expect("cold wgsl frame");
    let warm = execute_plan(&ctx, &plan, &input).expect("warm wgsl frame");

    assert!(warm.frame_cost.gpu_runtime.pipeline_cache_hits > 0);
    assert!(
        warm.frame_cost.gpu_runtime.pipeline_cache_misses
            <= cold.frame_cost.gpu_runtime.pipeline_cache_misses
    );
}

#[test]
fn presentation_wgsl_selector_honors_supported_workgroup_override() {
    let _lock = workgroup_override_test_lock();
    let _guard = EnvVarGuard::set(WGSL_WORKGROUP_SIZE_OVERRIDE_ENV, "32");
    let mut adapter_limits = wgpu::Limits::downlevel_defaults();
    adapter_limits.max_compute_invocations_per_workgroup = 128;
    adapter_limits.max_compute_workgroup_size_x = 128;
    assert_eq!(
        select_presentation_workgroup_size(&adapter_limits).expect("select override"),
        32
    );
}

#[test]
fn presentation_wgsl_selector_prefers_retuned_resident_default() {
    let mut adapter_limits = wgpu::Limits::downlevel_defaults();
    adapter_limits.max_compute_invocations_per_workgroup = 128;
    adapter_limits.max_compute_workgroup_size_x = 128;
    assert_eq!(
        select_presentation_workgroup_size(&adapter_limits).expect("retuned workgroup selection"),
        64
    );
}

#[test]
fn presentation_wgsl_selector_rejects_incompatible_workgroup_override() {
    let _lock = workgroup_override_test_lock();
    let _guard = EnvVarGuard::set(WGSL_WORKGROUP_SIZE_OVERRIDE_ENV, "128");
    let mut adapter_limits = wgpu::Limits::downlevel_defaults();
    adapter_limits.max_compute_invocations_per_workgroup = 64;
    adapter_limits.max_compute_workgroup_size_x = 64;
    let err = select_presentation_workgroup_size(&adapter_limits)
        .expect_err("reject presentation incompatible size");
    assert!(
        err.to_string().contains("incompatible with adapter limits"),
        "unexpected error: {err}"
    );
}
