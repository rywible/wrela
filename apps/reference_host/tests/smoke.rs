use ciborium::value::Value;
use wrela::engine_frame::{EngineResourceId, EngineSubsystemKind};
use wrela::persistence::decompress_payload;
use wrela_reference_host::inspector::InspectorState;

fn write_project_fixture(name: &str, source: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "wrela_reference_host_{name}_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("fixture dir");
    let path = dir.join("main.wr");
    std::fs::write(&path, source).expect("fixture source");
    path
}

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
        for label in [
            "input.translate",
            "system.join",
            "residency.apply",
            "physics.integrate",
            "physics.broadphase",
            "physics.detect_contacts",
            "physics.solve_positions",
            "physics.solve_velocities",
            "physics.move_fsm",
            "audio.publish_snapshot",
            "audio.render_to_output",
        ] {
            let producer_span = report
                .timeline_spans
                .iter()
                .find(|span| span.label == label)
                .unwrap_or_else(|| panic!("missing producer span {label}"));
            assert!(
                present_span.started_micros >= producer_span.ended_micros,
                "presentation must run after {label}"
            );
        }
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
            "system.join",
            "residency.plan",
            "residency.apply",
            "physics.integrate",
            "physics.broadphase",
            "physics.detect_contacts",
            "physics.solve_positions",
            "physics.solve_velocities",
            "physics.move_fsm",
            "audio.publish_snapshot",
            "audio.render_to_output",
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
        let presentation = report
            .subsystem(EngineSubsystemKind::Presentation)
            .expect("presentation report");
        let audio_output_note = presentation
            .notes
            .iter()
            .find(|note| note.starts_with("audio_output mode="))
            .unwrap_or_else(|| {
                panic!(
                    "presentation report must document the live audio output bridge or null fallback: {:?}",
                    presentation.notes
                )
            });
        assert!(
            audio_output_note.contains("mode=device") || audio_output_note.contains("mode=null"),
            "audio output note must identify the live output path: {audio_output_note}"
        );
        assert!(
            audio_output_note.contains("voices=1"),
            "audio output bridge must read the same voice ledger published by audio: {audio_output_note}"
        );
        if audio_output_note.contains("renderer=callback") {
            assert!(
                audio_output_note.contains("mode=device"),
                "callback rendering should be tied to a live output device: {audio_output_note}"
            );
        } else {
            assert!(
                audio_output_note.contains("rendered_frames=256"),
                "null audio output bridge must render a full configured block: {audio_output_note}"
            );
        }
        if audio_output_note.contains("mode=null") {
            assert!(
                presentation
                    .notes
                    .iter()
                    .any(|note| note.starts_with("audio_output_fallback=")),
                "null audio output must document why the device stream is unavailable"
            );
        }
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

#[test]
fn reference_host_project_smoke_schedules_authored_system() {
    let project_path = write_project_fixture(
        "authored_system",
        r#"
resource Transform {
    x: F32
}

@phase(sim)
system IntegrateTransforms(@mut transform: Transform) -> Nothing {
    return
}

fn run() -> Integer {
    return 0
}
"#,
    );

    let reports = wrela_reference_host::run_headless_smoke_for_project(1, project_path)
        .expect("authored system should be scheduled by the reference host");
    assert!(
        reports
            .iter()
            .any(|report| report.subsystems.iter().any(
                |subsystem| subsystem.kind == wrela::engine_frame::EngineSubsystemKind::System
            )),
        "expected a system subsystem report"
    );
}

#[test]
fn headless_save_payload_contains_live_runtime_subsystem_records() {
    let project_path = write_project_fixture(
        "live_save_payload",
        r#"
fn run() -> Integer {
    return 0
}
"#,
    );
    let record =
        wrela_reference_host::run_headless_save_for_project(2, project_path).expect("save record");
    assert!(
        record.header.snapshot_epoch > 0,
        "save must be taken from the live state-advance output, not the initial snapshot"
    );
    let payload = decompress_payload(&record).expect("decode save payload");
    let type_ids = payload
        .ledger
        .iter()
        .map(|record| record.type_id.as_str())
        .collect::<Vec<_>>();
    for expected in [
        "RuntimeStateAdvance",
        "InputFrame",
        "PhysicsBodyState",
        "ResidentRegions",
        "AudioVoiceLedger",
    ] {
        assert!(
            type_ids.contains(&expected),
            "save payload missing runtime subsystem record `{expected}`: {type_ids:?}"
        );
    }

    let input = ledger_payload(&payload.ledger, "InputFrame");
    assert!(
        matches!(map_field(input, "actions"), Some(Value::Array(_))),
        "input actions must serialize action state details, not a count: {input:?}"
    );

    let physics = ledger_payload(&payload.ledger, "PhysicsBodyState");
    let physics_bodies = match map_field(physics, "bodies") {
        Some(Value::Array(bodies)) => bodies,
        other => panic!("physics bodies must be an array of body states, got {other:?}"),
    };
    assert!(
        physics_bodies.iter().any(|body| {
            matches!(map_field(body, "id"), Some(Value::Integer(_)))
                && matches!(map_field(body, "position"), Some(Value::Array(_)))
                && matches!(map_field(body, "velocity"), Some(Value::Array(_)))
        }),
        "physics save payload must include id/position/velocity body states: {physics_bodies:?}"
    );

    let residency = ledger_payload(&payload.ledger, "ResidentRegions");
    let resident_region_ids = match map_field(residency, "resident_region_ids") {
        Some(Value::Array(ids)) => ids,
        other => panic!("resident regions must be serialized as ids, got {other:?}"),
    };
    assert!(
        resident_region_ids
            .iter()
            .any(|id| matches!(id, Value::Text(value) if value == "reference_origin")),
        "resident region ids missing reference_origin: {resident_region_ids:?}"
    );

    let audio = ledger_payload(&payload.ledger, "AudioVoiceLedger");
    let voices = match map_field(audio, "voices") {
        Some(Value::Array(voices)) => voices,
        other => panic!("audio voices must be an array of plan/state fields, got {other:?}"),
    };
    assert!(
        voices.iter().any(|voice| {
            matches!(map_field(voice, "id"), Some(Value::Integer(_)))
                && matches!(
                    map_field(voice, "source_audio_signature"),
                    Some(Value::Integer(_))
                )
                && matches!(map_field(voice, "position"), Some(Value::Array(_)))
                && matches!(map_field(voice, "velocity"), Some(Value::Array(_)))
                && matches!(map_field(voice, "gain"), Some(Value::Float(_)))
                && matches!(map_field(voice, "gate"), Some(Value::Bool(_)))
        }),
        "audio save payload must include voice plan/state fields: {voices:?}"
    );
}

fn ledger_payload<'a>(
    ledger: &'a [wrela::persistence::SnapshotLedgerRecord],
    type_id: &str,
) -> &'a Value {
    &ledger
        .iter()
        .find(|record| record.type_id == type_id)
        .unwrap_or_else(|| panic!("missing ledger record {type_id}"))
        .payload
}

fn map_field<'a>(value: &'a Value, field: &str) -> Option<&'a Value> {
    match value {
        Value::Map(entries) => entries.iter().find_map(|(key, value)| match key {
            Value::Text(key) if key == field => Some(value),
            _ => None,
        }),
        _ => None,
    }
}
