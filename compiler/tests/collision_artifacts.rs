use smol_str::SmolStr;
use wrela::artifact_key::ArtifactReuseKey;
use wrela::artifact_store::{
    ArtifactInstanceMetadata, ArtifactLookupRequest, ArtifactStore, StoredArtifact,
};
use wrela::collision_exec::cpu::{
    CollisionArtifactPayload, CollisionContinuationSeed, CollisionStoredWitness,
};
use wrela::collision_plan::{
    CollisionArtifactBinding, CollisionArtifactKind, CollisionPlan, CollisionQueryKind,
    collision_history_compatibility_hash,
};
use wrela::query_exec::stable_region_snapshot_handle;
use wrela::state_advance::ChangeClass;
use wrela::world_identity::SnapshotEpoch;

fn policy_digest(policy: wrela::collision_contract::CollisionExecutionPolicy) -> u64 {
    let backend_tag = [match policy.backend_preference {
        wrela::query_contract::DispatchBackend::Cpu => 0,
        wrela::query_contract::DispatchBackend::VirtualGpu => 1,
        wrela::query_contract::DispatchBackend::Wgsl => 2,
        wrela::query_contract::DispatchBackend::Auto => 3,
    }];
    wrela::query_exec::ids::stable_semantic_id(&[
        &policy.required_guarantee.id().to_le_bytes(),
        &policy.selected_method.id().to_le_bytes(),
        &backend_tag,
    ])
}

fn artifact_by_kind(
    plan: &CollisionPlan,
    kind: CollisionArtifactKind,
) -> &CollisionArtifactBinding {
    plan.artifacts
        .iter()
        .find(|artifact| artifact.kind == kind)
        .expect("artifact kind")
}

#[test]
fn transition_reuse_keys_match_previous_epoch_contract_and_policy() {
    let plan = CollisionPlan::for_query(CollisionQueryKind::SphereSweepTransition);
    let witness = artifact_by_kind(&plan, CollisionArtifactKind::WitnessCache);
    let previous_snapshot =
        stable_region_snapshot_handle(&SmolStr::new("collision_artifact_region"))
            .with_epoch(SnapshotEpoch(1));
    let current_snapshot =
        stable_region_snapshot_handle(&SmolStr::new("collision_artifact_region"))
            .with_epoch(SnapshotEpoch(2));
    let previous_key = ArtifactReuseKey::new(
        &previous_snapshot,
        Some(witness.id.clone()),
        witness.contract.logical_schema.describe(),
        witness.contract.logical_schema.stable_hash(),
        Some(policy_digest(plan.policy)),
        witness.contract.compatibility.policy.mode,
    );
    let current_key = ArtifactReuseKey::new(
        &current_snapshot,
        Some(witness.id.clone()),
        witness.contract.logical_schema.describe(),
        witness.contract.logical_schema.stable_hash(),
        Some(policy_digest(plan.policy)),
        witness.contract.compatibility.policy.mode,
    );
    assert!(current_key.transition_compatible_with(&previous_key, SnapshotEpoch(1)));
}

#[test]
fn witness_reuse_validity_accepts_presentation_change_and_rejects_topology_change() {
    let plan = CollisionPlan::for_query(CollisionQueryKind::SphereSweepTransition);
    let witness = artifact_by_kind(&plan, CollisionArtifactKind::WitnessCache);
    let continuation = artifact_by_kind(&plan, CollisionArtifactKind::ContinuationSeed);
    let previous_snapshot =
        stable_region_snapshot_handle(&SmolStr::new("collision_history_region"))
            .with_epoch(SnapshotEpoch(1));
    let current_snapshot = stable_region_snapshot_handle(&SmolStr::new("collision_history_region"))
        .with_epoch(SnapshotEpoch(2));
    let policy_digest = policy_digest(plan.policy);
    let conservative_flavor =
        wrela::collision_contract::CollisionContactNormalFlavor::ConservativeUpperBound;
    let gradient_flavor = wrela::collision_contract::CollisionContactNormalFlavor::SurfaceGradient;
    let history_hash = collision_history_compatibility_hash(
        plan.contract_id,
        CollisionArtifactKind::WitnessCache,
        Some(conservative_flavor),
    );
    let mut store = ArtifactStore::<CollisionArtifactPayload>::default();
    store.insert(StoredArtifact {
        contract: witness.contract.clone(),
        metadata: ArtifactInstanceMetadata {
            snapshot: previous_snapshot.clone(),
            reuse_key: ArtifactReuseKey::new(
                &previous_snapshot,
                Some(witness.id.clone()),
                witness.contract.logical_schema.describe(),
                witness.contract.logical_schema.stable_hash(),
                Some(policy_digest),
                witness.contract.compatibility.policy.mode,
            ),
            policy_digest: Some(policy_digest),
            presentation_frame: None,
            layout_signature: None,
            history_compatibility_hash: Some(history_hash),
            evidence_summary: witness.contract.evidence_summary.clone(),
        },
        payload: CollisionArtifactPayload::WitnessCache(CollisionStoredWitness {
            hit: true,
            contact_fraction_upper_bound: Some(0.3125),
            normal_flavor: conservative_flavor,
        }),
    });
    store.insert(StoredArtifact {
        contract: continuation.contract.clone(),
        metadata: ArtifactInstanceMetadata {
            snapshot: previous_snapshot.clone(),
            reuse_key: ArtifactReuseKey::new(
                &previous_snapshot,
                Some(continuation.id.clone()),
                continuation.contract.logical_schema.describe(),
                continuation.contract.logical_schema.stable_hash(),
                Some(policy_digest),
                continuation.contract.compatibility.policy.mode,
            ),
            policy_digest: Some(policy_digest),
            presentation_frame: None,
            layout_signature: None,
            history_compatibility_hash: Some(collision_history_compatibility_hash(
                plan.contract_id,
                CollisionArtifactKind::ContinuationSeed,
                Some(conservative_flavor),
            )),
            evidence_summary: continuation.contract.evidence_summary.clone(),
        },
        payload: CollisionArtifactPayload::ContinuationSeed(CollisionContinuationSeed {
            fraction_hint: 0.3125,
            no_hit_certificate: true,
            normal_flavor: conservative_flavor,
        }),
    });

    let accepted_request = ArtifactLookupRequest {
        contract: witness.contract.clone(),
        reuse_key: None,
        current_snapshot: current_snapshot.clone(),
        previous_snapshot_epoch: Some(SnapshotEpoch(1)),
        change_class: Some(ChangeClass::Presentation),
        policy_digest: Some(policy_digest),
        presentation_frame: None,
        layout_signature: None,
        history_compatibility_hash: Some(history_hash),
        evidence_summary: Some(witness.contract.evidence_summary.clone()),
    };
    let (artifact, report) = store.lookup(&accepted_request);
    assert!(
        artifact.is_some(),
        "expected accepted reuse lookup: {report:?}"
    );

    let flavor_mismatch_request = ArtifactLookupRequest {
        history_compatibility_hash: Some(collision_history_compatibility_hash(
            plan.contract_id,
            CollisionArtifactKind::WitnessCache,
            Some(gradient_flavor),
        )),
        ..accepted_request.clone()
    };
    let (artifact, report) = store.lookup(&flavor_mismatch_request);
    assert!(artifact.is_none());
    assert_eq!(
        report.primary_rejection_reason().as_deref(),
        Some("HistoryCompatibilityMatches")
    );

    let continuation_request = ArtifactLookupRequest {
        contract: continuation.contract.clone(),
        history_compatibility_hash: Some(collision_history_compatibility_hash(
            plan.contract_id,
            CollisionArtifactKind::ContinuationSeed,
            Some(conservative_flavor),
        )),
        evidence_summary: Some(continuation.contract.evidence_summary.clone()),
        ..accepted_request.clone()
    };
    let (artifact, report) = store.lookup(&continuation_request);
    assert!(
        artifact.is_some(),
        "expected continuation reuse lookup: {report:?}"
    );

    let continuation_mismatch_request = ArtifactLookupRequest {
        contract: continuation.contract.clone(),
        history_compatibility_hash: Some(collision_history_compatibility_hash(
            plan.contract_id,
            CollisionArtifactKind::ContinuationSeed,
            Some(gradient_flavor),
        )),
        evidence_summary: Some(continuation.contract.evidence_summary.clone()),
        ..accepted_request
    };
    let (artifact, report) = store.lookup(&continuation_mismatch_request);
    assert!(artifact.is_none());
    assert_eq!(
        report.primary_rejection_reason().as_deref(),
        Some("HistoryCompatibilityMatches")
    );

    let rejected_request = ArtifactLookupRequest {
        change_class: Some(ChangeClass::Topology),
        ..continuation_request
    };
    let (artifact, report) = store.lookup(&rejected_request);
    assert!(artifact.is_none());
    assert_eq!(
        report.primary_rejection_reason().as_deref(),
        Some("CompatibleChange(ChangeCompatibility { maximum: Presentation })")
    );
}
