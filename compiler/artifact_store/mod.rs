use crate::artifact_contract::{
    ArtifactSnapshotRelation, ArtifactUseSource, ArtifactValidityPredicate, ArtifactValidityRule,
    SemanticArtifactContract,
};
use crate::artifact_key::ArtifactReuseKey;
use crate::semantic_evidence::SemanticEvidenceSummary;
use crate::state_advance::ChangeClass;
use crate::world_identity::{SnapshotEpoch, WorldSnapshotHandle};
use smol_str::SmolStr;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactInstanceMetadata {
    pub snapshot: WorldSnapshotHandle,
    pub reuse_key: ArtifactReuseKey,
    pub policy_digest: Option<u64>,
    pub presentation_frame: Option<u32>,
    pub layout_signature: Option<u64>,
    pub history_compatibility_hash: Option<u64>,
    pub evidence_summary: SemanticEvidenceSummary,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredArtifact<T> {
    pub contract: SemanticArtifactContract,
    pub metadata: ArtifactInstanceMetadata,
    pub payload: T,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactLookupRequest {
    pub contract: SemanticArtifactContract,
    pub current_snapshot: WorldSnapshotHandle,
    pub previous_snapshot_epoch: Option<SnapshotEpoch>,
    pub change_class: Option<ChangeClass>,
    pub policy_digest: Option<u64>,
    pub presentation_frame: Option<u32>,
    pub layout_signature: Option<u64>,
    pub history_compatibility_hash: Option<u64>,
    pub evidence_summary: Option<SemanticEvidenceSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactLookupReport {
    pub index_candidates: usize,
    pub compatibility_rejections: Vec<SmolStr>,
    pub validity_reports: Vec<ArtifactValidityReport>,
}

impl ArtifactLookupReport {
    pub fn primary_rejection_reason(&self) -> Option<SmolStr> {
        self.validity_reports
            .iter()
            .find_map(|report| {
                report
                    .checks
                    .iter()
                    .find(|check| !check.accepted)
                    .map(|check| check.predicate.clone())
            })
            .or_else(|| self.compatibility_rejections.first().cloned())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactValidityReport {
    pub artifact_id: SmolStr,
    pub accepted: bool,
    pub checks: Vec<ArtifactValidityCheck>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactValidityCheck {
    pub predicate: SmolStr,
    pub accepted: bool,
    pub detail: SmolStr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactStoreReport {
    pub entries: usize,
    pub buckets: Vec<ArtifactStoreBucketReport>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactStoreBucketReport {
    pub contract_id: SmolStr,
    pub logical_schema: SmolStr,
    pub entry_count: usize,
}

#[derive(Debug, Clone)]
pub struct ArtifactStore<T> {
    entries: BTreeMap<ArtifactIndexKey, Vec<StoredArtifact<T>>>,
}

impl<T> Default for ArtifactStore<T> {
    fn default() -> Self {
        Self {
            entries: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct ArtifactIndexKey {
    contract_id: SmolStr,
    logical_schema: SmolStr,
}

impl<T> ArtifactStore<T> {
    pub fn insert(&mut self, artifact: StoredArtifact<T>) {
        self.entries
            .entry(index_key(&artifact.contract))
            .or_default()
            .push(artifact);
    }

    pub fn lookup<'a>(
        &'a self,
        request: &ArtifactLookupRequest,
    ) -> (Option<&'a StoredArtifact<T>>, ArtifactLookupReport) {
        let key = index_key(&request.contract);
        let candidates = self.entries.get(&key).map(Vec::as_slice).unwrap_or(&[]);
        let mut compatibility_rejections = Vec::new();
        let mut validity_reports = Vec::new();
        for candidate in candidates.iter().rev() {
            if let Some(reason) = compatibility_mismatch(candidate, request) {
                compatibility_rejections.push(reason);
                continue;
            }
            let validity = validate(candidate, request);
            let accepted = validity.accepted;
            validity_reports.push(validity);
            if accepted {
                return (
                    Some(candidate),
                    ArtifactLookupReport {
                        index_candidates: candidates.len(),
                        compatibility_rejections,
                        validity_reports,
                    },
                );
            }
        }
        (
            None,
            ArtifactLookupReport {
                index_candidates: candidates.len(),
                compatibility_rejections,
                validity_reports,
            },
        )
    }

    pub fn invalidate_where(
        &mut self,
        mut predicate: impl FnMut(&StoredArtifact<T>) -> bool,
    ) -> usize {
        let mut removed = 0usize;
        self.entries.retain(|_, bucket| {
            let before = bucket.len();
            bucket.retain(|entry| !predicate(entry));
            removed += before.saturating_sub(bucket.len());
            !bucket.is_empty()
        });
        removed
    }

    pub fn report(&self) -> ArtifactStoreReport {
        ArtifactStoreReport {
            entries: self.entries.values().map(Vec::len).sum(),
            buckets: self
                .entries
                .iter()
                .map(|(key, entries)| ArtifactStoreBucketReport {
                    contract_id: key.contract_id.clone(),
                    logical_schema: key.logical_schema.clone(),
                    entry_count: entries.len(),
                })
                .collect(),
        }
    }
}

pub fn store_backed_use(
    actor: impl Into<SmolStr>,
    artifact_id: impl Into<SmolStr>,
    required_validity: ArtifactValidityRule,
) -> crate::artifact_contract::ArtifactUse {
    crate::artifact_contract::ArtifactUse {
        actor: actor.into(),
        artifact_id: artifact_id.into(),
        kind: crate::artifact_contract::ArtifactUseKind::Load,
        source: ArtifactUseSource::ArtifactStore,
        required_validity: Some(required_validity),
    }
}

fn index_key(contract: &SemanticArtifactContract) -> ArtifactIndexKey {
    ArtifactIndexKey {
        contract_id: contract.id.clone(),
        logical_schema: contract.logical_schema.describe(),
    }
}

fn compatibility_mismatch<T>(
    candidate: &StoredArtifact<T>,
    request: &ArtifactLookupRequest,
) -> Option<SmolStr> {
    if let Some(reason) = contract_mismatch(&candidate.contract, &request.contract) {
        return Some(reason);
    }
    if candidate.contract.logical_schema != request.contract.logical_schema {
        return Some(SmolStr::new("logical-schema-mismatch"));
    }
    if candidate.contract.compatibility.policy.mode != request.contract.compatibility.policy.mode {
        return Some(SmolStr::new("policy-mode-mismatch"));
    }
    if candidate.contract.compatibility.evidence.scope
        != request.contract.compatibility.evidence.scope
        || candidate.contract.compatibility.evidence.origin
            != request.contract.compatibility.evidence.origin
    {
        return Some(SmolStr::new("evidence-compatibility-mismatch"));
    }
    match request.contract.compatibility.snapshot {
        ArtifactSnapshotRelation::ExactSnapshot => {
            if candidate.metadata.snapshot.snapshot_id() != request.current_snapshot.snapshot_id()
                || candidate.metadata.snapshot.epoch() != request.current_snapshot.epoch()
            {
                return Some(SmolStr::new("exact-snapshot-mismatch"));
            }
        }
        ArtifactSnapshotRelation::PreviousSnapshotEpoch => {
            let Some(previous_epoch) = request.previous_snapshot_epoch else {
                return Some(SmolStr::new("missing-previous-snapshot-epoch"));
            };
            if candidate.metadata.snapshot.snapshot_id() != request.current_snapshot.snapshot_id()
                || candidate.metadata.snapshot.epoch() != previous_epoch
            {
                return Some(SmolStr::new("previous-snapshot-mismatch"));
            }
        }
        ArtifactSnapshotRelation::CaptureLineage => {
            if candidate.metadata.snapshot.kind() != request.current_snapshot.kind()
                || candidate.metadata.snapshot.capture_name()
                    != request.current_snapshot.capture_name()
                || candidate.metadata.snapshot.root_entity().lineage_id()
                    != request.current_snapshot.root_entity().lineage_id()
            {
                return Some(SmolStr::new("capture-lineage-mismatch"));
            }
        }
    }
    None
}

fn contract_mismatch(
    candidate: &SemanticArtifactContract,
    request: &SemanticArtifactContract,
) -> Option<SmolStr> {
    if candidate.id != request.id {
        return Some(SmolStr::new("artifact-id-mismatch"));
    }
    if candidate.kind != request.kind {
        return Some(SmolStr::new("artifact-kind-mismatch"));
    }
    if candidate.logical_schema != request.logical_schema {
        return Some(SmolStr::new("logical-schema-mismatch"));
    }
    if candidate.version != request.version {
        return Some(SmolStr::new("artifact-version-mismatch"));
    }
    if candidate.compatibility != request.compatibility {
        return Some(SmolStr::new("artifact-compatibility-mismatch"));
    }
    if candidate.validity != request.validity {
        return Some(SmolStr::new("artifact-validity-mismatch"));
    }
    if candidate.producer != request.producer || candidate.consumer != request.consumer {
        return Some(SmolStr::new("artifact-actor-mismatch"));
    }
    if candidate.deterministic != request.deterministic {
        return Some(SmolStr::new("artifact-determinism-mismatch"));
    }
    if candidate.transition != request.transition {
        return Some(SmolStr::new("artifact-transition-contract-mismatch"));
    }
    if candidate.evidence_summary != request.evidence_summary {
        return Some(SmolStr::new("artifact-evidence-summary-mismatch"));
    }
    None
}

fn validate<T>(
    candidate: &StoredArtifact<T>,
    request: &ArtifactLookupRequest,
) -> ArtifactValidityReport {
    let mut checks = Vec::new();
    let accepted = evaluate_rule(&request.contract.validity, candidate, request, &mut checks);
    ArtifactValidityReport {
        artifact_id: candidate.contract.id.clone(),
        accepted,
        checks,
    }
}

fn evaluate_rule<T>(
    rule: &ArtifactValidityRule,
    candidate: &StoredArtifact<T>,
    request: &ArtifactLookupRequest,
    checks: &mut Vec<ArtifactValidityCheck>,
) -> bool {
    match rule {
        ArtifactValidityRule::Always => true,
        ArtifactValidityRule::All(rules) => rules
            .iter()
            .all(|rule| evaluate_rule(rule, candidate, request, checks)),
        ArtifactValidityRule::Any(rules) => rules
            .iter()
            .any(|rule| evaluate_rule(rule, candidate, request, checks)),
        ArtifactValidityRule::Predicate(predicate) => {
            let (accepted, detail) = evaluate_predicate(predicate, candidate, request);
            checks.push(ArtifactValidityCheck {
                predicate: SmolStr::new(format!("{predicate:?}")),
                accepted,
                detail,
            });
            accepted
        }
    }
}

fn evaluate_predicate<T>(
    predicate: &ArtifactValidityPredicate,
    candidate: &StoredArtifact<T>,
    request: &ArtifactLookupRequest,
) -> (bool, SmolStr) {
    match predicate {
        ArtifactValidityPredicate::CurrentSnapshotMatchesStored => {
            let accepted = candidate.metadata.snapshot.snapshot_id()
                == request.current_snapshot.snapshot_id()
                && candidate.metadata.snapshot.epoch() == request.current_snapshot.epoch();
            (
                accepted,
                SmolStr::new(format!(
                    "stored_epoch={} current_epoch={}",
                    candidate.metadata.snapshot.epoch().0,
                    request.current_snapshot.epoch().0
                )),
            )
        }
        ArtifactValidityPredicate::PreviousSnapshotMatchesStored => {
            let accepted = request
                .previous_snapshot_epoch
                .is_some_and(|previous_epoch| {
                    candidate.metadata.snapshot.snapshot_id()
                        == request.current_snapshot.snapshot_id()
                        && candidate.metadata.snapshot.epoch() == previous_epoch
                });
            (
                accepted,
                SmolStr::new(format!(
                    "stored_epoch={} requested_previous_epoch={}",
                    candidate.metadata.snapshot.epoch().0,
                    request
                        .previous_snapshot_epoch
                        .map(|epoch| epoch.0.to_string())
                        .unwrap_or_else(|| "none".to_string())
                )),
            )
        }
        ArtifactValidityPredicate::SnapshotLineageMatchesCurrent => {
            let accepted = candidate.metadata.snapshot.root_entity().lineage_id()
                == request.current_snapshot.root_entity().lineage_id();
            (
                accepted,
                SmolStr::new(format!(
                    "stored_lineage={} current_lineage={}",
                    candidate.metadata.snapshot.root_entity().lineage_id().0,
                    request.current_snapshot.root_entity().lineage_id().0
                )),
            )
        }
        ArtifactValidityPredicate::CompatibleChange(compatibility) => {
            let accepted = request
                .change_class
                .is_some_and(|change| compatibility.allows(change));
            (
                accepted,
                SmolStr::new(format!(
                    "requested_change={} allowed_max={:?}",
                    request
                        .change_class
                        .map(|value| format!("{value:?}"))
                        .unwrap_or_else(|| "none".to_string()),
                    compatibility.maximum
                )),
            )
        }
        ArtifactValidityPredicate::PolicyDigestMatches => (
            candidate.metadata.policy_digest == request.policy_digest,
            SmolStr::new(format!(
                "stored_policy_digest={} requested_policy_digest={}",
                display_optional_u64(candidate.metadata.policy_digest),
                display_optional_u64(request.policy_digest)
            )),
        ),
        ArtifactValidityPredicate::LayoutSignatureMatches => (
            candidate.metadata.layout_signature == request.layout_signature,
            SmolStr::new(format!(
                "stored_layout_signature={} requested_layout_signature={}",
                display_optional_u64(candidate.metadata.layout_signature),
                display_optional_u64(request.layout_signature)
            )),
        ),
        ArtifactValidityPredicate::HistoryCompatibilityMatches => (
            candidate.metadata.history_compatibility_hash == request.history_compatibility_hash,
            SmolStr::new(format!(
                "stored_history_hash={} requested_history_hash={}",
                display_optional_u64(candidate.metadata.history_compatibility_hash),
                display_optional_u64(request.history_compatibility_hash)
            )),
        ),
        ArtifactValidityPredicate::EvidenceSummaryMatches => (
            request
                .evidence_summary
                .as_ref()
                .is_some_and(|summary| summary == &candidate.metadata.evidence_summary),
            SmolStr::new(format!(
                "stored_evidence_scope={:?} requested_evidence_scope={}",
                candidate.metadata.evidence_summary.scope,
                request
                    .evidence_summary
                    .as_ref()
                    .map(|summary| format!("{:?}", summary.scope))
                    .unwrap_or_else(|| "none".to_string())
            )),
        ),
        ArtifactValidityPredicate::EvidenceScopeMatches(scope) => (
            request
                .evidence_summary
                .as_ref()
                .is_some_and(|summary| &summary.scope == scope),
            SmolStr::new(format!(
                "required_scope={scope:?} requested_scope={}",
                request
                    .evidence_summary
                    .as_ref()
                    .map(|summary| format!("{:?}", summary.scope))
                    .unwrap_or_else(|| "none".to_string())
            )),
        ),
        ArtifactValidityPredicate::MaxSnapshotAge(maximum) => {
            let age = request
                .current_snapshot
                .epoch()
                .0
                .saturating_sub(candidate.metadata.snapshot.epoch().0);
            (
                age <= *maximum,
                SmolStr::new(format!("snapshot_age={age} max_snapshot_age={maximum}")),
            )
        }
        ArtifactValidityPredicate::MaxPresentationFrameAge(maximum) => {
            let age = match (
                request.presentation_frame,
                candidate.metadata.presentation_frame,
            ) {
                (Some(current), Some(stored)) => current.saturating_sub(stored),
                _ => u32::MAX,
            };
            (
                age <= *maximum as u32,
                SmolStr::new(format!(
                    "presentation_frame_age={age} max_presentation_frame_age={maximum}"
                )),
            )
        }
    }
}

fn display_optional_u64(value: Option<u64>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "none".to_string())
}
