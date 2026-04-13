use smol_str::SmolStr;
use wrela::artifact_contract::{
    ArtifactCompatibilityRelation, ArtifactEvidenceCompatibility, ArtifactLogicalField,
    ArtifactLogicalSchema, ArtifactPolicyCompatibility, ArtifactSnapshotRelation,
    ArtifactTransitionRelation, ArtifactValidityPredicate, ArtifactValidityRule,
    SemanticArtifactContract, SemanticArtifactKind,
};
use wrela::artifact_key::ArtifactPolicyDigestMode;
use wrela::artifact_store::{
    ArtifactInstanceMetadata, ArtifactLookupRequest, ArtifactStore, StoredArtifact,
};
use wrela::query_exec::stable_region_snapshot_handle;
use wrela::query_plan::{BatchQueryKind, BatchQueryPlan, DispatchBackend};
use wrela::semantic_evidence::SemanticEvidenceSummary;
use wrela::state_advance::{ChangeClass, ChangeCompatibility};
use wrela::world_identity::SnapshotEpoch;

fn query_contract_with_validity(
    id: &str,
    name: &str,
    validity: ArtifactValidityRule,
    compatibility: ArtifactCompatibilityRelation,
    evidence_summary: SemanticEvidenceSummary,
) -> SemanticArtifactContract {
    SemanticArtifactContract {
        id: SmolStr::new(id),
        kind: SemanticArtifactKind::Query,
        logical_schema: ArtifactLogicalSchema {
            namespace: SmolStr::new("query"),
            name: SmolStr::new(name),
            fields: vec![ArtifactLogicalField::new("fixture", "true")],
        },
        compatibility,
        validity,
        producer: SmolStr::new("fixture.producer"),
        consumer: SmolStr::new("fixture.consumer"),
        deterministic: true,
        version: 1,
        transition: None,
        evidence_summary,
    }
}

fn stored_artifact(
    contract: SemanticArtifactContract,
    snapshot_name: &str,
    epoch: u64,
    policy_digest: Option<u64>,
    evidence_summary: SemanticEvidenceSummary,
) -> StoredArtifact<&'static str> {
    let snapshot = stable_region_snapshot_handle(&SmolStr::new(snapshot_name))
        .with_epoch(SnapshotEpoch(epoch));
    let reuse_key = wrela::artifact_key::ArtifactReuseKey::new(
        &snapshot,
        Some(contract.id.clone()),
        contract.logical_schema.describe(),
        contract.logical_schema.stable_hash(),
        policy_digest,
        contract.compatibility.policy.mode,
    );
    StoredArtifact {
        contract,
        metadata: ArtifactInstanceMetadata {
            snapshot,
            reuse_key,
            policy_digest,
            presentation_frame: None,
            layout_signature: None,
            history_compatibility_hash: None,
            evidence_summary,
        },
        payload: "payload",
    }
}

#[test]
fn support_summaries_reuse_across_compatible_snapshots_and_store_reports_entries() {
    let base_artifact =
        BatchQueryPlan::for_shape_query(BatchQueryKind::Trace, DispatchBackend::Cpu, None)
            .artifact_contracts
            .into_iter()
            .find(|artifact| {
                matches!(
                    artifact.schema,
                    wrela::query_plan::ArtifactSchema::SupportSummary { .. }
                )
            })
            .expect("support summary")
            .semantic_artifact_contract();
    let evidence = base_artifact.evidence_summary.clone();
    let compatibility = ArtifactCompatibilityRelation {
        snapshot: ArtifactSnapshotRelation::CaptureLineage,
        transition: ArtifactTransitionRelation {
            compatibility: Some(ChangeCompatibility::new(ChangeClass::Presentation)),
            requires_previous_snapshot: false,
        },
        policy: ArtifactPolicyCompatibility {
            mode: ArtifactPolicyDigestMode::CompatibleRange,
        },
        evidence: ArtifactEvidenceCompatibility {
            origin: evidence.origin,
            scope: evidence.scope,
        },
    };
    let contract = SemanticArtifactContract {
        validity: ArtifactValidityRule::all(vec![
            ArtifactValidityRule::predicate(
                ArtifactValidityPredicate::SnapshotLineageMatchesCurrent,
            ),
            ArtifactValidityRule::predicate(ArtifactValidityPredicate::CompatibleChange(
                ChangeCompatibility::new(ChangeClass::Presentation),
            )),
            ArtifactValidityRule::predicate(ArtifactValidityPredicate::PolicyDigestMatches),
            ArtifactValidityRule::predicate(ArtifactValidityPredicate::EvidenceSummaryMatches),
            ArtifactValidityRule::predicate(ArtifactValidityPredicate::MaxSnapshotAge(1)),
        ]),
        compatibility,
        ..base_artifact
    };
    let evidence_summary = evidence.clone();
    let mut store = ArtifactStore::default();
    store.insert(stored_artifact(
        contract.clone(),
        "artifact_store_region",
        1,
        Some(99),
        evidence_summary.clone(),
    ));

    let current_snapshot = stable_region_snapshot_handle(&SmolStr::new("artifact_store_region"))
        .with_epoch(SnapshotEpoch(2));
    let report = store.report();
    assert_eq!(report.entries, 1);
    assert_eq!(report.buckets.len(), 1);

    let (found, lookup) = store.lookup(&ArtifactLookupRequest {
        contract: contract.clone(),
        current_snapshot,
        previous_snapshot_epoch: None,
        change_class: Some(ChangeClass::Presentation),
        policy_digest: Some(99),
        presentation_frame: None,
        layout_signature: None,
        history_compatibility_hash: None,
        evidence_summary: Some(evidence_summary),
    });

    assert!(
        found.is_some(),
        "expected compatible support summary reuse: {lookup:?}"
    );
    assert_eq!(lookup.index_candidates, 1);
    assert!(
        store.invalidate_where(|entry| entry.contract.id == contract.id) > 0,
        "expected invalidation to remove the stored artifact"
    );
    assert_eq!(store.report().entries, 0);
}

#[test]
fn history_like_artifacts_are_invalidated_on_incompatible_transitions() {
    let evidence = SemanticEvidenceSummary::contract_bound();
    let contract = query_contract_with_validity(
        "history-artifact",
        "history-slot",
        ArtifactValidityRule::all(vec![
            ArtifactValidityRule::predicate(
                ArtifactValidityPredicate::PreviousSnapshotMatchesStored,
            ),
            ArtifactValidityRule::predicate(ArtifactValidityPredicate::CompatibleChange(
                ChangeCompatibility::new(ChangeClass::Presentation),
            )),
            ArtifactValidityRule::predicate(ArtifactValidityPredicate::PolicyDigestMatches),
        ]),
        ArtifactCompatibilityRelation {
            snapshot: ArtifactSnapshotRelation::PreviousSnapshotEpoch,
            transition: ArtifactTransitionRelation {
                compatibility: Some(ChangeCompatibility::new(ChangeClass::Presentation)),
                requires_previous_snapshot: true,
            },
            policy: ArtifactPolicyCompatibility {
                mode: ArtifactPolicyDigestMode::CompatibleRange,
            },
            evidence: ArtifactEvidenceCompatibility {
                origin: evidence.origin,
                scope: evidence.scope,
            },
        },
        evidence.clone(),
    );
    let mut store = ArtifactStore::default();
    store.insert(stored_artifact(
        contract.clone(),
        "history_region",
        1,
        Some(7),
        evidence.clone(),
    ));

    let current_snapshot =
        stable_region_snapshot_handle(&SmolStr::new("history_region")).with_epoch(SnapshotEpoch(2));
    let (found, lookup) = store.lookup(&ArtifactLookupRequest {
        contract,
        current_snapshot,
        previous_snapshot_epoch: Some(SnapshotEpoch(1)),
        change_class: Some(ChangeClass::Topology),
        policy_digest: Some(7),
        presentation_frame: None,
        layout_signature: None,
        history_compatibility_hash: None,
        evidence_summary: Some(evidence),
    });

    assert!(found.is_none());
    assert!(
        lookup.validity_reports.iter().any(|report| report
            .checks
            .iter()
            .any(|check| { !check.accepted && check.predicate.contains("CompatibleChange") })),
        "expected incompatible transition rejection: {lookup:?}"
    );
}

#[test]
fn culling_tables_are_rejected_when_required_evidence_changes() {
    let evidence = SemanticEvidenceSummary::artifact_bound(false);
    let changed_evidence = evidence.with_artifact_binding("changed evidence");
    let contract = query_contract_with_validity(
        "culling-table",
        "culling-table",
        ArtifactValidityRule::all(vec![
            ArtifactValidityRule::predicate(
                ArtifactValidityPredicate::SnapshotLineageMatchesCurrent,
            ),
            ArtifactValidityRule::predicate(ArtifactValidityPredicate::PolicyDigestMatches),
            ArtifactValidityRule::predicate(ArtifactValidityPredicate::EvidenceSummaryMatches),
        ]),
        ArtifactCompatibilityRelation {
            snapshot: ArtifactSnapshotRelation::CaptureLineage,
            transition: ArtifactTransitionRelation {
                compatibility: None,
                requires_previous_snapshot: false,
            },
            policy: ArtifactPolicyCompatibility {
                mode: ArtifactPolicyDigestMode::CompatibleRange,
            },
            evidence: ArtifactEvidenceCompatibility {
                origin: evidence.origin,
                scope: evidence.scope,
            },
        },
        evidence.clone(),
    );
    let mut store = ArtifactStore::default();
    store.insert(stored_artifact(
        contract.clone(),
        "culling_region",
        1,
        Some(11),
        evidence,
    ));

    let current_snapshot =
        stable_region_snapshot_handle(&SmolStr::new("culling_region")).with_epoch(SnapshotEpoch(2));
    let (found, lookup) = store.lookup(&ArtifactLookupRequest {
        contract,
        current_snapshot,
        previous_snapshot_epoch: None,
        change_class: Some(ChangeClass::Presentation),
        policy_digest: Some(11),
        presentation_frame: None,
        layout_signature: None,
        history_compatibility_hash: None,
        evidence_summary: Some(changed_evidence),
    });

    assert!(found.is_none());
    assert!(
        lookup
            .validity_reports
            .iter()
            .any(|report| report.checks.iter().any(|check| {
                !check.accepted && check.predicate.contains("EvidenceSummaryMatches")
            })),
        "expected evidence mismatch rejection: {lookup:?}"
    );
}

#[test]
fn store_rejects_artifacts_when_contract_version_changes_even_if_ids_match() {
    let base_artifact =
        BatchQueryPlan::for_shape_query(BatchQueryKind::Trace, DispatchBackend::Cpu, None)
            .artifact_contracts
            .into_iter()
            .find(|artifact| {
                matches!(
                    artifact.schema,
                    wrela::query_plan::ArtifactSchema::SupportSummary { .. }
                )
            })
            .expect("support summary")
            .semantic_artifact_contract();
    let mut stored_contract = base_artifact.clone();
    stored_contract.version = 1;
    let mut requested_contract = base_artifact;
    requested_contract.version = 2;
    let evidence = requested_contract.evidence_summary.clone();

    let mut store = ArtifactStore::default();
    store.insert(stored_artifact(
        stored_contract,
        "versioned_region",
        1,
        Some(17),
        evidence.clone(),
    ));

    let current_snapshot = stable_region_snapshot_handle(&SmolStr::new("versioned_region"))
        .with_epoch(SnapshotEpoch(1));
    let (found, lookup) = store.lookup(&ArtifactLookupRequest {
        contract: requested_contract,
        current_snapshot,
        previous_snapshot_epoch: None,
        change_class: Some(ChangeClass::Presentation),
        policy_digest: Some(17),
        presentation_frame: None,
        layout_signature: None,
        history_compatibility_hash: None,
        evidence_summary: Some(evidence),
    });

    assert!(found.is_none());
    assert!(
        lookup
            .compatibility_rejections
            .iter()
            .any(|reason| reason == "artifact-version-mismatch"),
        "expected version mismatch rejection: {lookup:?}"
    );
}

#[test]
fn store_rejects_artifacts_when_requested_validity_changes() {
    let evidence = SemanticEvidenceSummary::contract_bound();
    let compatibility = ArtifactCompatibilityRelation {
        snapshot: ArtifactSnapshotRelation::ExactSnapshot,
        transition: ArtifactTransitionRelation {
            compatibility: None,
            requires_previous_snapshot: false,
        },
        policy: ArtifactPolicyCompatibility {
            mode: ArtifactPolicyDigestMode::Exact,
        },
        evidence: ArtifactEvidenceCompatibility {
            origin: evidence.origin,
            scope: evidence.scope,
        },
    };
    let stored_contract = query_contract_with_validity(
        "validity-artifact",
        "validity-artifact",
        ArtifactValidityRule::Always,
        compatibility.clone(),
        evidence.clone(),
    );
    let requested_contract = query_contract_with_validity(
        "validity-artifact",
        "validity-artifact",
        ArtifactValidityRule::predicate(ArtifactValidityPredicate::PolicyDigestMatches),
        compatibility,
        evidence.clone(),
    );
    let mut store = ArtifactStore::default();
    store.insert(stored_artifact(
        stored_contract,
        "validity_region",
        1,
        Some(3),
        evidence.clone(),
    ));

    let current_snapshot = stable_region_snapshot_handle(&SmolStr::new("validity_region"))
        .with_epoch(SnapshotEpoch(1));
    let (found, lookup) = store.lookup(&ArtifactLookupRequest {
        contract: requested_contract,
        current_snapshot,
        previous_snapshot_epoch: None,
        change_class: None,
        policy_digest: Some(3),
        presentation_frame: None,
        layout_signature: None,
        history_compatibility_hash: None,
        evidence_summary: Some(evidence),
    });

    assert!(found.is_none());
    assert!(
        lookup
            .compatibility_rejections
            .iter()
            .any(|reason| reason == "artifact-validity-mismatch"),
        "expected validity mismatch rejection: {lookup:?}"
    );
}
