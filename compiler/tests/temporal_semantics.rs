use smol_str::SmolStr;
use wrela::artifact_key::ArtifactPolicyDigestMode;
use wrela::query_exec::stable_region_snapshot_handle;
use wrela::query_plan::CaptureQueryPlan;
use wrela::query_plan::{
    ArtifactContract, BatchQueryKind, CaptureKind, CaptureQueryKind, DispatchBackend,
    QueryArtifactKind, QueryTransitionRecord, SceneSummary,
};
use wrela::state_advance::{
    ChangeClass, ChangeCompatibility, ChangeSummary, PresentationFrame, SimulationTick,
    SnapshotEpoch, StateAdvanceContract, StateAdvanceTransitionRecord, TemporalClock,
    TemporalValidityHorizon, TickInputBatch, TickInputEvent, TickInputKind,
    TransitionRejectionReason, WallClockStamp, WorldTransitionRecord,
};
use wrela_runtime::state_advance as runtime_state_advance;

#[test]
fn typed_clocks_and_change_lattice_keep_validity_and_compatibility_separate() {
    let simulation_tick = SimulationTick::new(41);
    let presentation_frame = PresentationFrame::new(8);
    let wall_clock = WallClockStamp::new(9_000);
    let current_clock = TemporalClock::new(
        SnapshotEpoch::new(12),
        simulation_tick,
        presentation_frame,
        wall_clock,
    );
    let previous_clock = TemporalClock::new(
        SnapshotEpoch::new(11),
        simulation_tick.next(),
        presentation_frame.next(),
        WallClockStamp::new(8_500),
    );

    assert_eq!(current_clock.snapshot_epoch.get(), 12);
    assert_eq!(current_clock.simulation_tick.get(), 41);
    assert_eq!(current_clock.presentation_frame.get(), 8);
    assert_eq!(current_clock.wall_clock.get(), 9_000);
    assert_eq!(
        ChangeClass::Presentation.join(ChangeClass::Topology),
        ChangeClass::Topology
    );
    assert_eq!(
        ChangeClass::Topology.meet(ChangeClass::Identity),
        ChangeClass::Topology
    );

    let compatibility = ChangeCompatibility::new(ChangeClass::Topology);
    let change = ChangeSummary::new(ChangeClass::Structural, "topology-adjacent change");
    let horizon = TemporalValidityHorizon::new(1, 4, 2, 16);

    assert!(compatibility.allows(change.class));
    assert!(!ChangeCompatibility::new(ChangeClass::Presentation).allows(change.class));
    assert_eq!(horizon.max_snapshot_age, 1);
    assert_eq!(horizon.max_wall_clock_age_ms, 16);

    let contract = StateAdvanceContract::new(
        current_clock,
        Some(previous_clock),
        horizon,
        change.detail.clone(),
        change.class,
        compatibility,
    );
    assert!(contract.is_transition_compatible());

    let summary: QueryTransitionRecord = contract.query_transition_summary();
    assert!(summary.accepted);
    assert_eq!(summary.current_clock.snapshot_epoch.get(), 12);
    assert!(summary.rejection.is_none());
}

#[test]
fn authoritative_transition_records_capture_acceptance_and_rejection_reason() {
    let contract = StateAdvanceContract::new(
        TemporalClock::new(
            SnapshotEpoch::new(4),
            SimulationTick::new(100),
            PresentationFrame::new(7),
            WallClockStamp::new(120),
        ),
        None,
        TemporalValidityHorizon::new(0, 0, 0, 0),
        "identity-affecting change",
        ChangeClass::Identity,
        ChangeCompatibility::new(ChangeClass::Topology),
    );

    let accepted = StateAdvanceTransitionRecord::accepted(contract.clone());
    assert!(accepted.accepted);
    assert!(accepted.rejection.is_none());

    let rejected = StateAdvanceTransitionRecord::rejected(
        contract,
        TransitionRejectionReason::ChangeCompatibilityExceeded,
    );
    assert!(!rejected.accepted);
    assert_eq!(
        rejected.rejection,
        Some(TransitionRejectionReason::ChangeCompatibilityExceeded)
    );
}

#[test]
fn world_transition_records_preserve_snapshot_identity_across_compiler_and_runtime() {
    let from_snapshot = stable_region_snapshot_handle(&SmolStr::new("transition_region"));
    let to_snapshot = from_snapshot.with_epoch(wrela::world_identity::SnapshotEpoch(2));
    let current_clock = TemporalClock::new(
        SnapshotEpoch::new(2),
        SimulationTick::new(100),
        PresentationFrame::new(7),
        WallClockStamp::new(120),
    );
    let inputs = TickInputBatch::new(
        current_clock.simulation_tick,
        vec![TickInputEvent::new(
            current_clock.simulation_tick,
            TickInputKind::Command,
            "player",
            "move",
        )],
    );
    let transition = WorldTransitionRecord::new(
        Some(from_snapshot.clone()),
        to_snapshot.clone(),
        Some(TemporalClock::new(
            SnapshotEpoch::new(1),
            SimulationTick::new(99),
            PresentationFrame::new(6),
            WallClockStamp::new(104),
        )),
        current_clock,
        inputs,
        Vec::new(),
    );

    assert_eq!(
        transition
            .from_snapshot
            .as_ref()
            .expect("from snapshot")
            .snapshot_id(),
        from_snapshot.snapshot_id()
    );
    assert_eq!(
        transition.to_snapshot.snapshot_id(),
        to_snapshot.snapshot_id()
    );
    assert_eq!(transition.to_snapshot.epoch(), to_snapshot.epoch());

    let runtime_transition = runtime_state_advance::WorldTransitionRecord::new(
        Some(runtime_state_advance::WorldSnapshotHandleRecord::new(
            "transition_region",
            7,
            runtime_state_advance::SnapshotEpoch::new(1),
        )),
        runtime_state_advance::WorldSnapshotHandleRecord::new(
            "transition_region",
            7,
            runtime_state_advance::SnapshotEpoch::new(2),
        ),
        Some(runtime_state_advance::TemporalClock::new(
            runtime_state_advance::SnapshotEpoch::new(1),
            runtime_state_advance::SimulationTick::new(99),
            runtime_state_advance::PresentationFrame::new(6),
            runtime_state_advance::WallClockStamp::new(104),
        )),
        runtime_state_advance::TemporalClock::new(
            runtime_state_advance::SnapshotEpoch::new(2),
            runtime_state_advance::SimulationTick::new(100),
            runtime_state_advance::PresentationFrame::new(7),
            runtime_state_advance::WallClockStamp::new(120),
        ),
        runtime_state_advance::TickInputBatch::new(
            runtime_state_advance::SimulationTick::new(100),
            Vec::new(),
        ),
        Vec::new(),
    );

    assert_eq!(
        runtime_transition
            .from_snapshot
            .as_ref()
            .expect("runtime from snapshot")
            .epoch
            .0,
        1
    );
    assert_eq!(runtime_transition.to_snapshot.epoch.0, 2);
}

#[test]
fn runtime_state_advance_mirror_tracks_the_same_clock_family() {
    let executor = runtime_state_advance::StateAdvanceExecutorContract {
        current_clock: runtime_state_advance::TemporalClock::new(
            runtime_state_advance::SnapshotEpoch::new(9),
            runtime_state_advance::SimulationTick::new(17),
            runtime_state_advance::PresentationFrame::new(4),
            runtime_state_advance::WallClockStamp::new(1_000),
        ),
        previous_clock: None,
        validity_horizon: runtime_state_advance::TemporalValidityHorizon::new(1, 1, 1, 1),
        change: runtime_state_advance::ChangeSummary::new(
            runtime_state_advance::ChangeClass::Presentation,
            "camera jitter",
        ),
        compatibility: runtime_state_advance::ChangeCompatibility::new(
            runtime_state_advance::ChangeClass::Topology,
        ),
    };

    assert!(executor.is_transition_compatible());

    let mirror = runtime_state_advance::StateAdvanceMirrorContract {
        executor,
        accepted: true,
    };

    assert!(mirror.accepted);
    assert_eq!(mirror.executor.current_clock.snapshot_epoch.0, 9);
}

#[test]
fn query_artifact_descriptors_report_planner_facing_kinds() {
    let opaque_trace = CaptureQueryPlan::for_query(
        CaptureQueryKind::Trace,
        CaptureKind::Shape,
        Some(SceneSummary {
            name: Some("opaque-scene".into()),
            opaque_boundary: true,
            ..Default::default()
        }),
    )
    .expect("opaque trace plan");

    assert!(
        opaque_trace
            .artifact_contracts
            .iter()
            .any(|artifact| artifact.query_artifact_kind()
                == QueryArtifactKind::OpaquePessimizationBoundary)
    );

    let batch_plan = wrela::query_plan::BatchQueryPlan::for_shape_query(
        BatchQueryKind::Trace,
        DispatchBackend::Cpu,
        None,
    );
    let artifact = batch_plan
        .artifact_contracts
        .iter()
        .find(|artifact| artifact.query_artifact_kind() == QueryArtifactKind::DispatchRecord)
        .expect("dispatch artifact");
    let previous_transition = StateAdvanceContract::new(
        wrela::state_advance::TemporalClock::new(
            SnapshotEpoch::new(12),
            SimulationTick::new(41),
            PresentationFrame::new(8),
            WallClockStamp::new(9_000),
        ),
        None,
        wrela::state_advance::TemporalValidityHorizon::new(1, 1, 1, 1),
        "previous transition",
        ChangeClass::Presentation,
        ChangeCompatibility::new(ChangeClass::Topology),
    );
    let transition = StateAdvanceContract::new(
        wrela::state_advance::TemporalClock::new(
            SnapshotEpoch::new(13),
            SimulationTick::new(42),
            PresentationFrame::new(9),
            WallClockStamp::new(9_100),
        ),
        Some(previous_transition.current_clock),
        wrela::state_advance::TemporalValidityHorizon::new(1, 1, 1, 1),
        "planner transition",
        ChangeClass::Presentation,
        ChangeCompatibility::new(ChangeClass::Topology),
    );
    let artifact = ArtifactContract {
        transition: Some(transition.clone()),
        ..artifact.clone()
    };
    let descriptor = artifact.query_artifact_descriptor();

    assert_eq!(descriptor.kind, QueryArtifactKind::DispatchRecord);
    assert_eq!(descriptor.id, artifact.id);
    assert_eq!(
        descriptor.version,
        wrela::query_plan::QUERY_PLAN_CONTRACT_VERSION
    );
    assert_eq!(descriptor.transition, Some(transition));
    assert!(
        artifact
            .logical_artifact_schema()
            .starts_with("query-artifact::")
    );
}

#[test]
fn artifact_transition_contracts_affect_reuse_compatibility() {
    let batch_plan = wrela::query_plan::BatchQueryPlan::for_shape_query(
        BatchQueryKind::Trace,
        DispatchBackend::Cpu,
        None,
    );
    let base_artifact = batch_plan
        .artifact_contracts
        .iter()
        .find(|artifact| artifact.query_artifact_kind() == QueryArtifactKind::DispatchRecord)
        .expect("dispatch artifact")
        .clone();
    let snapshot = stable_region_snapshot_handle(&SmolStr::new("artifact_transition_region"));
    let artifact_a = ArtifactContract {
        transition: Some(StateAdvanceContract::new(
            TemporalClock::new(
                SnapshotEpoch::new(2),
                SimulationTick::new(10),
                PresentationFrame::new(3),
                WallClockStamp::new(50),
            ),
            Some(TemporalClock::new(
                SnapshotEpoch::new(1),
                SimulationTick::new(9),
                PresentationFrame::new(2),
                WallClockStamp::new(34),
            )),
            TemporalValidityHorizon::new(1, 1, 1, 16),
            "artifact transition a",
            ChangeClass::Presentation,
            ChangeCompatibility::new(ChangeClass::Topology),
        )),
        ..base_artifact.clone()
    };
    let artifact_b = ArtifactContract {
        transition: Some(StateAdvanceContract::new(
            TemporalClock::new(
                SnapshotEpoch::new(3),
                SimulationTick::new(11),
                PresentationFrame::new(4),
                WallClockStamp::new(66),
            ),
            Some(TemporalClock::new(
                SnapshotEpoch::new(2),
                SimulationTick::new(10),
                PresentationFrame::new(3),
                WallClockStamp::new(50),
            )),
            TemporalValidityHorizon::new(1, 1, 1, 16),
            "artifact transition b",
            ChangeClass::Topology,
            ChangeCompatibility::new(ChangeClass::Topology),
        )),
        ..base_artifact
    };

    let key_a = artifact_a.reuse_key(&snapshot, None, ArtifactPolicyDigestMode::CompatibleRange);
    let key_b = artifact_b.reuse_key(&snapshot, None, ArtifactPolicyDigestMode::CompatibleRange);

    assert_ne!(
        artifact_a.compatibility_hash(),
        artifact_b.compatibility_hash()
    );
    assert!(!key_a.compatible_contract_with(&key_b));
}
