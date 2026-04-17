use super::core::{temporal_alias_source, temporal_disocclusion_source};
use super::*;

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
        SnapshotEpoch(1),
        SnapshotEpoch(1),
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
        SnapshotEpoch(2),
        SnapshotEpoch(1),
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
        SnapshotEpoch(1),
        SnapshotEpoch(1),
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
        SnapshotEpoch(2),
        SnapshotEpoch(1),
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
        SnapshotEpoch(1),
        SnapshotEpoch(1),
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
        SnapshotEpoch(2),
        SnapshotEpoch(1),
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
        SnapshotEpoch(1),
        SnapshotEpoch(1),
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
        SnapshotEpoch(2),
        SnapshotEpoch(1),
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
        SnapshotEpoch(1),
        SnapshotEpoch(1),
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
        SnapshotEpoch(2),
        SnapshotEpoch(1),
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
        SnapshotEpoch(1),
        SnapshotEpoch(1),
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
        SnapshotEpoch(2),
        SnapshotEpoch(1),
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
        SnapshotEpoch(1),
        SnapshotEpoch(1),
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
        SnapshotEpoch(2),
        SnapshotEpoch(1),
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
        SnapshotEpoch(1),
        SnapshotEpoch(1),
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
        SnapshotEpoch(2),
        SnapshotEpoch(1),
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
        SnapshotEpoch(1),
        SnapshotEpoch(1),
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
        SnapshotEpoch(1),
        SnapshotEpoch(1),
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
        SnapshotEpoch(2),
        SnapshotEpoch(1),
        wgsl_frame0.history.clone(),
    );
    let wgsl_with_history =
        execute_plan(&wgsl_ctx1, &wgsl_plan1, &wgsl_input1).expect("wgsl temporal reuse");
    assert_eq!(
        cpu_with_history.metrics.continuation_available_count,
        wgsl_with_history.metrics.continuation_available_count
    );
    assert_eq!(
        cpu_with_history.metrics.continuation_consumed_count,
        wgsl_with_history.metrics.continuation_consumed_count
    );
    assert_attachment_vec3_approx_eq(
        &with_history_color,
        &wgsl_with_history
            .attachments
            .decode_attachment("color")
            .unwrap(),
        2.0e-2,
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
        SnapshotEpoch(1),
        SnapshotEpoch(1),
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
        SnapshotEpoch(2),
        SnapshotEpoch(1),
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
        SnapshotEpoch(1),
        SnapshotEpoch(1),
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
        SnapshotEpoch(2),
        SnapshotEpoch(1),
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
        SnapshotEpoch(1),
        SnapshotEpoch(1),
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
