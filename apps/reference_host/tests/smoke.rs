use wrela::engine_frame::{EngineResourceId, EngineSubsystemKind};
use wrela_reference_host::inspector::InspectorState;

#[test]
fn reference_host_headless_smoke_produces_inspectable_reports() {
    let frames = std::env::var("WRELA_REF_HOST_SMOKE_SECS")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .map(|secs| secs.saturating_mul(60))
        .unwrap_or(8);
    let reports = wrela_reference_host::run_headless_smoke(frames).expect("smoke");
    assert!(!reports.is_empty());
    for (frame_idx, report) in reports.iter().enumerate() {
        let inspector = InspectorState::from_report(report);
        assert_eq!(inspector.rows.len(), report.subsystems.len());
        assert!(report.violations.is_empty());
        let present_span = report
            .timeline_spans
            .iter()
            .find(|span| span.label == "presentation.swapchain_present")
            .expect("presentation present span");
        let save_span = report
            .timeline_spans
            .iter()
            .find(|span| span.label == "save.publish")
            .expect("save publish span");
        assert!(
            save_span.started_micros >= present_span.ended_micros,
            "save must run after presentation in the reference host"
        );
        assert!(
            report
                .timeline_spans
                .iter()
                .any(|span| span.label == "presentation.swapchain_acquire"),
            "missing presentation-owned swapchain acquire span"
        );
        assert!(
            report
                .timeline_spans
                .iter()
                .any(|span| span.label == "presentation.swapchain_present"),
            "missing presentation-owned swapchain present span"
        );
        assert!(
            report
                .timeline_spans
                .iter()
                .filter(|span| span.label == "presentation.swapchain_present")
                .all(|span| !span.queue_submission),
            "present observation must not be attributed as a queue submit"
        );
        for kind in [
            EngineSubsystemKind::Input,
            EngineSubsystemKind::System,
            EngineSubsystemKind::Residency,
            EngineSubsystemKind::Physics,
            EngineSubsystemKind::Audio,
            EngineSubsystemKind::Save,
            EngineSubsystemKind::Presentation,
        ] {
            assert!(
                report.subsystem(kind.clone()).is_some(),
                "missing {kind:?} subsystem report"
            );
        }
        for label in [
            "input.translate",
            "system.begin_tick",
            "system.pre_sim.ReferenceHostObserveInput",
            "system.commit_records",
            "residency.plan",
            "residency.apply",
            "physics.xpbd",
            "audio.publish_snapshot",
            "save.publish",
        ] {
            assert!(
                report.timeline_spans.iter().any(|span| span.label == label),
                "missing live subsystem span {label}"
            );
        }
        assert_eq!(
            report
                .subsystem(EngineSubsystemKind::System)
                .expect("system report")
                .work_items,
            1
        );
        assert!(
            report
                .subsystem(EngineSubsystemKind::Residency)
                .expect("residency report")
                .work_items
                > 0
        );
        assert!(
            report
                .subsystem(EngineSubsystemKind::Physics)
                .expect("physics report")
                .work_items
                > 0
        );
        assert_eq!(
            report
                .subsystem(EngineSubsystemKind::Audio)
                .expect("audio report")
                .work_items,
            1
        );
        assert_eq!(
            report
                .subsystem(EngineSubsystemKind::Save)
                .expect("save report")
                .work_items,
            if frame_idx == 0 { 1 } else { 0 },
            "save is an explicit one-shot request in smoke"
        );
        for resource in [
            "input frame",
            "resident region",
            "physics body state",
            "audio voice ledger",
        ] {
            assert!(
                report.resource_ledger.states.iter().any(|state| {
                    match (resource, &state.resource) {
                        ("input frame", EngineResourceId::InputFrame { .. }) => true,
                        ("resident region", EngineResourceId::ResidentRegion { .. }) => true,
                        ("physics body state", EngineResourceId::PhysicsBodyState { .. }) => true,
                        ("audio voice ledger", EngineResourceId::AudioVoiceLedger { .. }) => true,
                        _ => false,
                    }
                }),
                "missing {resource} resource state"
            );
        }
        let has_save_record = report
            .resource_ledger
            .states
            .iter()
            .any(|state| matches!(state.resource, EngineResourceId::SaveRecord { .. }));
        assert_eq!(
            has_save_record,
            frame_idx == 0,
            "save record resource should exist only for the requested save frame"
        );
    }
}
