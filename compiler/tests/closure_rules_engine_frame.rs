//! Parity tests for `collect_engine_frame_budget_findings` (RFC 0011 Phase 62.9).

use wrela::engine_frame::{
    ClosureRuleTable, EngineSubsystemKind, collect_engine_frame_budget_findings,
};
use wrela::perf_target::{
    PerfClosureEngineFrameBudget, PerfClosureEngineFrameStatusReport, PerfClosureLaneStatus,
};

fn sample_budget() -> PerfClosureEngineFrameBudget {
    PerfClosureEngineFrameBudget {
        frame_wall_time_median_ms: 8.33,
        frame_wall_time_p95_ms: 8.33,
        presentation_median_ms: 4.50,
        collision_median_ms: 2.50,
        state_advance_median_ms: 0.25,
        future_subsystem_reserve_ms: 1.00,
        max_queue_submit_count_per_frame: 2,
        max_hot_path_readback_bytes_per_frame: 0,
    }
}

#[test]
fn engine_frame_budget_findings_empty_when_within_budget() {
    let budget = sample_budget();
    let report = PerfClosureEngineFrameStatusReport {
        status: PerfClosureLaneStatus::Sampled,
        frame_wall_time_median_ms: Some(1.0),
        frame_wall_time_p95_ms: Some(2.0),
        cpu_critical_path_median_ms: None,
        gpu_critical_path_median_ms: None,
        presentation_median_ms: Some(1.0),
        collision_median_ms: Some(0.5),
        state_advance_median_ms: Some(0.1),
        future_subsystem_reserve_ms: Some(5.0),
        queue_submit_count: Some(1),
        hot_path_readback_bytes: Some(0),
        scene_reupload_bytes: None,
        active_degradations: Vec::new(),
        violations: Vec::new(),
        notes: Vec::new(),
        motion_to_photon_median_ms: None,
        motion_to_photon_budget_ms: None,
    };
    let findings = collect_engine_frame_budget_findings(&budget, &report);
    assert!(findings.is_empty());
}

#[test]
fn engine_frame_budget_findings_frame_wall_over_budget() {
    let budget = sample_budget();
    let report = PerfClosureEngineFrameStatusReport {
        status: PerfClosureLaneStatus::Sampled,
        frame_wall_time_median_ms: Some(20.0),
        frame_wall_time_p95_ms: Some(25.0),
        cpu_critical_path_median_ms: None,
        gpu_critical_path_median_ms: None,
        presentation_median_ms: Some(1.0),
        collision_median_ms: Some(0.5),
        state_advance_median_ms: Some(0.1),
        future_subsystem_reserve_ms: Some(5.0),
        queue_submit_count: Some(1),
        hot_path_readback_bytes: Some(0),
        scene_reupload_bytes: None,
        active_degradations: Vec::new(),
        violations: Vec::new(),
        notes: Vec::new(),
        motion_to_photon_median_ms: None,
        motion_to_photon_budget_ms: None,
    };
    let findings = collect_engine_frame_budget_findings(&budget, &report);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].focus, "frame_wall_time_budget");
}

#[test]
fn canonical_rules_cover_rfc0011_subsystems() {
    let table = ClosureRuleTable::with_canonical_engine_frame_rules();
    let covered = table.registered_subsystems();
    for kind in [
        EngineSubsystemKind::Input,
        EngineSubsystemKind::System,
        EngineSubsystemKind::Residency,
        EngineSubsystemKind::Physics,
        EngineSubsystemKind::Audio,
        EngineSubsystemKind::Save,
        EngineSubsystemKind::Presentation,
    ] {
        assert!(covered.contains(&kind), "missing {kind:?}");
    }
}

#[test]
fn canonical_rules_promote_rfc0011_runtime_violations() {
    let budget = sample_budget();
    let report = PerfClosureEngineFrameStatusReport {
        status: PerfClosureLaneStatus::Sampled,
        frame_wall_time_median_ms: None,
        frame_wall_time_p95_ms: None,
        cpu_critical_path_median_ms: None,
        gpu_critical_path_median_ms: None,
        presentation_median_ms: None,
        collision_median_ms: None,
        state_advance_median_ms: None,
        future_subsystem_reserve_ms: None,
        queue_submit_count: None,
        hot_path_readback_bytes: None,
        scene_reupload_bytes: None,
        active_degradations: Vec::new(),
        violations: vec![
            "physics.contact_readback_over_budget".to_string(),
            "audio.underrun".to_string(),
            "presentation.input_ring_overflow".to_string(),
            "presentation.fallback_to_vsync_fifo".to_string(),
            "save.write_failed".to_string(),
        ],
        notes: Vec::new(),
        motion_to_photon_median_ms: None,
        motion_to_photon_budget_ms: None,
    };
    let findings = collect_engine_frame_budget_findings(&budget, &report);
    let focuses = findings
        .iter()
        .map(|finding| finding.focus.as_str())
        .collect::<Vec<_>>();
    for expected in [
        "physics.contact_readback_over_budget",
        "audio.underrun",
        "presentation.input_ring_overflow",
        "presentation.fallback_to_vsync_fifo",
        "save.write_failed",
    ] {
        assert!(
            focuses.contains(&expected),
            "missing promoted finding {expected}; got {focuses:?}"
        );
    }
}
