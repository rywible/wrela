use super::*;

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
            .all(|artifact| artifact != "half_res_participants")
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
            .all(|artifact| artifact != "dynamic_resolution")
    );
    assert!(
        cpu.frame_cost
            .quality
            .active_degradations
            .iter()
            .any(|step| step == "reduce_internal_resolution")
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
        SnapshotEpoch(1),
        SnapshotEpoch(1),
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
    assert!(result.frame_cost.packet_scheduling_active);
    let primary_visibility = result
        .frame_cost
        .passes
        .iter()
        .find(|pass| pass.pass_kind == "primary_visibility")
        .expect("cpu primary visibility pass");
    assert!(
        result.frame_cost.tile_candidate_total_samples < primary_visibility.work_items,
        "candidate-table totals should measure cull-active samples, not the full resident pass"
    );
    assert!(primary_visibility.dispatch_count > 1);
}

#[test]
fn frame_cost_reports_view_distance_clipmap_reuse_update_and_fallback() {
    let (plan, ctx, input) = presentation_fixture(DispatchBackend::Cpu);

    let first = execute_plan(&ctx, &plan, &input).expect("cpu clipmap build");
    let first_clipmap = first
        .history
        .as_ref()
        .and_then(|history| history.clipmap.as_ref())
        .expect("initial clipmap artifact");
    assert_eq!(first_clipmap.build_count, 1);
    assert_eq!(first_clipmap.reuse_count, 0);
    assert_eq!(first_clipmap.update_count, 0);

    let mut stable_input = input.clone();
    stable_input.history = first.history.clone();
    stable_input.frame_state = frame_state_value_with_history(
        CanonicalCameraInput {
            position: [0.0, 0.0, 2.0],
            forward: [0.0, 0.0, -1.0],
            up: [0.0, 1.0, 0.0],
            vertical_fov_degrees: 75.0,
        },
        CanonicalCameraInput {
            position: [0.0, 0.0, 2.0],
            forward: [0.0, 0.0, -1.0],
            up: [0.0, 1.0, 0.0],
            vertical_fov_degrees: 75.0,
        },
        CanonicalViewportInput {
            width: 4,
            height: 4,
        },
        CanonicalViewportInput {
            width: 4,
            height: 4,
        },
        [0.0, 0.0],
        [0.0, 0.0],
        1,
        0,
        1.0 / 60.0,
        false,
        SnapshotEpoch(1),
        SnapshotEpoch(1),
    );
    let stable = execute_plan(&ctx, &plan, &stable_input).expect("cpu clipmap reuse");
    let stable_clipmap = stable
        .history
        .as_ref()
        .and_then(|history| history.clipmap.as_ref())
        .expect("stable clipmap artifact");
    assert_eq!(stable_clipmap.reuse_count, 1);
    assert_eq!(stable_clipmap.update_count, 0);
    assert!(
        stable
            .frame_cost
            .passes
            .iter()
            .find(|pass| pass.pass_kind == "view_distance_clipmap")
            .expect("clipmap pass")
            .notes
            .iter()
            .any(|note| note.contains("status=reused")),
        "expected clipmap reuse note, got {:?}",
        stable.frame_cost.passes
    );
    assert!(
        stable
            .frame_cost
            .active_acceleration_artifacts
            .iter()
            .any(|artifact| artifact == "view_distance_clipmap")
    );

    let mut moving_input = input.clone();
    moving_input.history = first.history.clone();
    moving_input.frame_state = frame_state_value_with_history(
        CanonicalCameraInput {
            position: [0.10, 0.0, 2.0],
            forward: [0.0, 0.0, -1.0],
            up: [0.0, 1.0, 0.0],
            vertical_fov_degrees: 75.0,
        },
        CanonicalCameraInput {
            position: [0.0, 0.0, 2.0],
            forward: [0.0, 0.0, -1.0],
            up: [0.0, 1.0, 0.0],
            vertical_fov_degrees: 75.0,
        },
        CanonicalViewportInput {
            width: 4,
            height: 4,
        },
        CanonicalViewportInput {
            width: 4,
            height: 4,
        },
        [0.0, 0.0],
        [0.0, 0.0],
        1,
        0,
        1.0 / 60.0,
        false,
        SnapshotEpoch(1),
        SnapshotEpoch(1),
    );
    let moving = execute_plan(&ctx, &plan, &moving_input).expect("cpu clipmap update");
    let moving_clipmap = moving
        .history
        .as_ref()
        .and_then(|history| history.clipmap.as_ref())
        .expect("moving clipmap artifact");
    assert_eq!(moving_clipmap.update_count, 1);
    assert_eq!(moving_clipmap.reuse_count, 0);
    assert!(
        moving
            .frame_cost
            .passes
            .iter()
            .find(|pass| pass.pass_kind == "view_distance_clipmap")
            .expect("clipmap pass")
            .notes
            .iter()
            .any(|note| note.contains("status=updated"))
    );

    let mut budget_input = input.clone();
    budget_input.history = first.history.clone();
    budget_input.execution_policy = presentation_execution_policy(1);
    budget_input.frame_state = frame_state_value_with_history(
        CanonicalCameraInput {
            position: [0.10, 0.0, 2.0],
            forward: [0.0, 0.0, -1.0],
            up: [0.0, 1.0, 0.0],
            vertical_fov_degrees: 75.0,
        },
        CanonicalCameraInput {
            position: [0.0, 0.0, 2.0],
            forward: [0.0, 0.0, -1.0],
            up: [0.0, 1.0, 0.0],
            vertical_fov_degrees: 75.0,
        },
        CanonicalViewportInput {
            width: 4,
            height: 4,
        },
        CanonicalViewportInput {
            width: 4,
            height: 4,
        },
        [0.0, 0.0],
        [0.0, 0.0],
        1,
        0,
        1.0 / 60.0,
        false,
        SnapshotEpoch(1),
        SnapshotEpoch(1),
    );
    let budget = execute_plan(&ctx, &plan, &budget_input).expect("cpu clipmap fallback");
    let budget_clipmap = budget
        .history
        .as_ref()
        .and_then(|history| history.clipmap.as_ref())
        .expect("budget clipmap artifact");
    assert_eq!(
        budget_clipmap.build_mode,
        wrela::acceleration::clipmap::ViewDistanceClipmapBuildMode::Fallback
    );
    assert!(
        budget_clipmap
            .fallback_reasons
            .iter()
            .any(|reason| reason == "upload-budget-exceeded")
    );
    assert!(
        wrela::presentation_exec::render_frame_cost_report(&budget.frame_cost)
            .contains("view_distance_clipmap")
    );
}

#[test]
fn wgsl_frame_cost_reports_tile_candidates_packets_and_workgroup_size() {
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
        DispatchBackend::Wgsl,
        viewport,
        camera,
        camera,
        0,
        0,
        false,
        SnapshotEpoch(1),
        SnapshotEpoch(1),
        None,
    );

    let result = execute_plan(&ctx, &plan, &input).expect("wgsl culling execution");
    assert!(!result.frame_cost.packet_scheduling_active);
    assert!(matches!(
        result.frame_cost.selected_workgroup_size,
        32 | 64 | 128
    ));
    assert!(result.frame_cost.gpu_runtime.queue_submit_count > 0);
    assert_eq!(
        result.frame_cost.gpu_runtime.timestamps_supported,
        result.frame_cost.gpu_runtime.timestamped_pass_count > 0
    );
    assert!(result.frame_cost.gpu_runtime.transient_bind_group_creations > 0);
    assert!(result.frame_cost.gpu_runtime.transient_buffer_creations > 0);
    assert!(
        result.frame_cost.tile_candidate_total_samples
            >= result.frame_cost.tile_candidate_active_samples
    );
    assert_eq!(
        result.frame_cost.tile_candidate_total_samples
            - result.frame_cost.tile_candidate_active_samples,
        result.frame_cost.tile_candidate_reduction
    );
    assert!(
        result
            .frame_cost
            .active_acceleration_artifacts
            .iter()
            .any(|artifact| artifact == "tile_candidate_table")
    );
    assert!(result.frame_cost.tile_candidate_packet_count > 0);
    assert!(result.frame_cost.packet_compaction_ratio > 0.0);
    let report = wrela::presentation_exec::render_frame_cost_report(&result.frame_cost);
    assert!(report.contains("tile_candidate_total_samples="));
    assert!(report.contains("tile_candidate_effectiveness="));
    assert!(report.contains("tile_candidate_packet_count="));
    assert!(report.contains("packet_compaction_ratio="));
    assert!(report.contains("packet_scheduling_active="));
    assert!(report.contains("selected_workgroup_size="));
    assert!(report.contains("transient_bind_group_creations="));
    assert!(report.contains("frame_timing cpu_time_total_micros="));
    assert!(report.contains("timestamps_supported="));
    assert!(report.contains("gpu_runtime timestamped_pass_count="));
    assert!(result.frame_cost.tile_candidate_effectiveness >= 0.0);
    assert!(result.frame_cost.tile_candidate_effectiveness <= 1.0);
    assert!(result.frame_cost.packet_compaction_ratio >= 0.0);
    assert!(result.frame_cost.packet_compaction_ratio <= 1.0);
    assert!(result.frame_cost.passes.iter().all(|pass| {
        pass.notes
            .iter()
            .all(|note| !note.starts_with("gpu_elapsed_micros="))
    }));
    let primary_visibility = result
        .frame_cost
        .passes
        .iter()
        .find(|pass| pass.pass_kind == "primary_visibility")
        .expect("wgsl primary visibility pass");
    assert!(
        primary_visibility
            .notes
            .iter()
            .any(|note| note == "packet_scheduling active=false reason=resident_primary_path")
    );
    assert!(
        result.frame_cost.tile_candidate_total_samples < primary_visibility.work_items,
        "candidate-table totals should measure cull-active samples, not the full resident pass"
    );
    assert!(primary_visibility.dispatch_count > 1);
}

#[test]
fn wgsl_workgroup_selection_rejects_illegal_or_incompatible_sizes() {
    let _lock = workgroup_override_test_lock();
    let mut limits = wgpu::Limits::downlevel_defaults();
    limits.max_compute_workgroup_size_x = 128;
    limits.max_compute_invocations_per_workgroup = 128;

    assert_eq!(
        wrela::presentation_exec::select_presentation_workgroup_size(&limits)
            .expect("presentation workgroup selection"),
        64
    );
    assert_eq!(
        wrela::presentation_exec::validate_presentation_workgroup_size(64, &limits)
            .expect("presentation workgroup validation"),
        64
    );

    limits.max_compute_workgroup_size_x = 32;
    limits.max_compute_invocations_per_workgroup = 64;
    assert!(wrela::presentation_exec::validate_presentation_workgroup_size(64, &limits).is_err());
    assert!(wrela::presentation_exec::select_presentation_workgroup_size(&limits).is_ok());

    limits.max_compute_workgroup_size_x = 16;
    limits.max_compute_invocations_per_workgroup = 16;
    assert!(wrela::presentation_exec::select_presentation_workgroup_size(&limits).is_err());
}

#[test]
fn frame_cost_reports_legal_degradations_from_contract_not_active_state() {
    let (plan, ctx, mut input) = presentation_fixture(DispatchBackend::Cpu);
    let mut quality = plan.frame.quality.initial_state();
    quality.internal_resolution_scale = 0.5;
    quality.active_degradations = vec![QualityDegradationStep::ReduceInternalResolution];
    input.quality_override = Some(quality);

    let result = execute_plan(&ctx, &plan, &input).expect("cpu contract separation");

    let legal_degradations = plan
        .frame
        .quality
        .degradation_order
        .iter()
        .map(|step| wrela::presentation_exec::cost::quality_degradation_name(*step).to_string())
        .collect::<Vec<_>>();
    assert_eq!(result.frame_cost.legal_degradations, legal_degradations);
    assert!(
        result
            .frame_cost
            .active_acceleration_artifacts
            .iter()
            .all(|artifact| artifact != "dynamic_resolution")
    );
    assert!(
        result
            .frame_cost
            .active_acceleration_artifacts
            .iter()
            .all(|artifact| artifact != "half_res_participants")
    );
    assert!(
        result
            .frame_cost
            .active_acceleration_artifacts
            .iter()
            .all(|artifact| artifact != "reduced_radiance_queries")
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

#[test]
fn adaptive_controller_ignores_pipeline_cache_miss_frames() {
    let (mut plan, ctx, input) = presentation_fixture(DispatchBackend::Cpu);
    let mut contract = RealtimeQualityContract::named(RealtimeQualityTier::Realtime60);
    contract.target_fps = 100_000;
    plan.frame.quality = contract.clone();

    let mut report = execute_plan(&ctx, &plan, &input)
        .expect("frame execution")
        .frame_cost;
    report.gpu_runtime.pipeline_cache_misses = 1;

    let mut controller = AdaptivePresentationController::new(contract).with_window(1);
    assert!(!controller.observe_frame(&report));
    assert_eq!(controller.quality().internal_resolution_scale, 1.0);
    assert!(controller.quality().active_degradations.is_empty());
}
