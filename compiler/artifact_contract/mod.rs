#[cfg(feature = "internal-learned-experiments")]
use crate::acceleration::learned::{LearnedMethodPolicy, learned_method_policy_rejection};
use crate::artifact_key::ArtifactPolicyDigestMode;
use crate::semantic_evidence::{EvidenceOrigin, EvidenceScope, SemanticEvidenceSummary};
use crate::state_advance::{ChangeCompatibility, QueryTransitionContract};
use smol_str::SmolStr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SemanticArtifactKind {
    Query,
    PresentationAttachment,
    PresentationHistory,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactLogicalSchema {
    pub namespace: SmolStr,
    pub name: SmolStr,
    pub fields: Vec<ArtifactLogicalField>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactLogicalField {
    pub name: SmolStr,
    pub value: SmolStr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactCompatibilityRelation {
    pub snapshot: ArtifactSnapshotRelation,
    pub transition: ArtifactTransitionRelation,
    pub policy: ArtifactPolicyCompatibility,
    pub evidence: ArtifactEvidenceCompatibility,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactSnapshotRelation {
    ExactSnapshot,
    PreviousSnapshotEpoch,
    CaptureLineage,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactTransitionRelation {
    pub compatibility: Option<ChangeCompatibility>,
    pub requires_previous_snapshot: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactPolicyCompatibility {
    pub mode: ArtifactPolicyDigestMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactEvidenceCompatibility {
    pub origin: EvidenceOrigin,
    pub scope: EvidenceScope,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactObserver {
    Query,
    Presentation,
    Collision,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactResidency {
    SharedSnapshot,
    ObserverLocal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccelerationArtifactKind {
    SharedAccelerationForest,
    SharedUnionSubtreeForest,
    DistanceBrickCache,
    SupportBrickCache,
    RayCandidateTable,
    TileCandidateTable,
    ViewDistanceClipmap,
    ContinuationSeedTable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccelerationArtifactMetadata {
    pub kind: AccelerationArtifactKind,
    pub observer: ArtifactObserver,
    pub residency: ArtifactResidency,
    pub usage_site: SmolStr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtifactValidityRule {
    Always,
    All(Vec<ArtifactValidityRule>),
    Any(Vec<ArtifactValidityRule>),
    Predicate(ArtifactValidityPredicate),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtifactValidityPredicate {
    CurrentSnapshotMatchesStored,
    PreviousSnapshotMatchesStored,
    SnapshotLineageMatchesCurrent,
    CompatibleChange(ChangeCompatibility),
    PolicyDigestMatches,
    LayoutSignatureMatches,
    HistoryCompatibilityMatches,
    EvidenceSummaryMatches,
    EvidenceScopeMatches(EvidenceScope),
    MaxSnapshotAge(u64),
    MaxPresentationFrameAge(u64),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticArtifactContract {
    pub id: SmolStr,
    pub kind: SemanticArtifactKind,
    pub logical_schema: ArtifactLogicalSchema,
    pub compatibility: ArtifactCompatibilityRelation,
    pub acceleration: Option<AccelerationArtifactMetadata>,
    pub validity: ArtifactValidityRule,
    pub producer: SmolStr,
    pub consumer: SmolStr,
    pub deterministic: bool,
    pub version: u32,
    pub transition: Option<QueryTransitionContract>,
    pub evidence_summary: SemanticEvidenceSummary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactUseKind {
    Load,
    Produce,
    Preserve,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactUseSource {
    Plan,
    ArtifactStore,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactUse {
    pub actor: SmolStr,
    pub artifact_id: SmolStr,
    pub kind: ArtifactUseKind,
    pub source: ArtifactUseSource,
    pub required_validity: Option<ArtifactValidityRule>,
}

impl ArtifactLogicalSchema {
    pub fn describe(&self) -> SmolStr {
        let fields = self
            .fields
            .iter()
            .map(|field| format!("{}={}", field.name, field.value))
            .collect::<Vec<_>>()
            .join(",");
        SmolStr::new(format!(
            "{}::{}({fields})",
            self.namespace.as_str(),
            self.name.as_str()
        ))
    }

    pub fn stable_hash(&self) -> u64 {
        let encoded = self.describe();
        crate::query_exec::ids::stable_semantic_id(&[encoded.as_bytes()])
    }
}

impl ArtifactLogicalField {
    pub fn new(name: impl Into<SmolStr>, value: impl Into<SmolStr>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
        }
    }
}

impl ArtifactCompatibilityRelation {
    pub fn exact_snapshot(
        policy_mode: ArtifactPolicyDigestMode,
        evidence: SemanticEvidenceSummary,
    ) -> Self {
        Self {
            snapshot: ArtifactSnapshotRelation::ExactSnapshot,
            transition: ArtifactTransitionRelation {
                compatibility: None,
                requires_previous_snapshot: false,
            },
            policy: ArtifactPolicyCompatibility { mode: policy_mode },
            evidence: ArtifactEvidenceCompatibility {
                origin: evidence.origin,
                scope: evidence.scope,
            },
        }
    }
}

impl ArtifactValidityRule {
    pub fn all(predicates: impl Into<Vec<ArtifactValidityRule>>) -> Self {
        let predicates = predicates.into();
        if predicates.is_empty() {
            Self::Always
        } else {
            Self::All(predicates)
        }
    }

    pub const fn predicate(predicate: ArtifactValidityPredicate) -> Self {
        Self::Predicate(predicate)
    }

    pub const fn is_explicit(&self) -> bool {
        !matches!(self, Self::Always)
    }
}

pub fn artifact_use_kind_name(kind: ArtifactUseKind) -> &'static str {
    match kind {
        ArtifactUseKind::Load => "load",
        ArtifactUseKind::Produce => "produce",
        ArtifactUseKind::Preserve => "preserve",
    }
}

pub fn artifact_use_source_name(source: ArtifactUseSource) -> &'static str {
    match source {
        ArtifactUseSource::Plan => "plan",
        ArtifactUseSource::ArtifactStore => "artifact-store",
    }
}

pub fn snapshot_relation_name(value: ArtifactSnapshotRelation) -> &'static str {
    match value {
        ArtifactSnapshotRelation::ExactSnapshot => "exact-snapshot",
        ArtifactSnapshotRelation::PreviousSnapshotEpoch => "previous-snapshot-epoch",
        ArtifactSnapshotRelation::CaptureLineage => "capture-lineage",
    }
}

pub fn artifact_observer_name(observer: ArtifactObserver) -> &'static str {
    match observer {
        ArtifactObserver::Query => "query",
        ArtifactObserver::Presentation => "presentation",
        ArtifactObserver::Collision => "collision",
    }
}

pub fn artifact_residency_name(residency: ArtifactResidency) -> &'static str {
    match residency {
        ArtifactResidency::SharedSnapshot => "shared_snapshot",
        ArtifactResidency::ObserverLocal => "observer_local",
    }
}

pub fn acceleration_artifact_kind_name(kind: AccelerationArtifactKind) -> &'static str {
    match kind {
        AccelerationArtifactKind::SharedAccelerationForest => "shared_acceleration_forest",
        AccelerationArtifactKind::SharedUnionSubtreeForest => "shared_union_subtree_forest",
        AccelerationArtifactKind::DistanceBrickCache => "distance_brick_cache",
        AccelerationArtifactKind::SupportBrickCache => "support_brick_cache",
        AccelerationArtifactKind::RayCandidateTable => "ray_candidate_table",
        AccelerationArtifactKind::TileCandidateTable => "tile_candidate_table",
        AccelerationArtifactKind::ViewDistanceClipmap => "view_distance_clipmap",
        AccelerationArtifactKind::ContinuationSeedTable => "continuation_seed_table",
    }
}

pub const fn acceleration_artifact_expected_residency(
    kind: AccelerationArtifactKind,
) -> ArtifactResidency {
    match kind {
        AccelerationArtifactKind::SharedAccelerationForest
        | AccelerationArtifactKind::SharedUnionSubtreeForest
        | AccelerationArtifactKind::DistanceBrickCache
        | AccelerationArtifactKind::SupportBrickCache => ArtifactResidency::SharedSnapshot,
        AccelerationArtifactKind::RayCandidateTable
        | AccelerationArtifactKind::TileCandidateTable
        | AccelerationArtifactKind::ViewDistanceClipmap
        | AccelerationArtifactKind::ContinuationSeedTable => ArtifactResidency::ObserverLocal,
    }
}

pub const fn acceleration_artifact_allows_observer(
    kind: AccelerationArtifactKind,
    observer: ArtifactObserver,
) -> bool {
    match kind {
        AccelerationArtifactKind::SharedAccelerationForest
        | AccelerationArtifactKind::SharedUnionSubtreeForest
        | AccelerationArtifactKind::DistanceBrickCache
        | AccelerationArtifactKind::SupportBrickCache => true,
        AccelerationArtifactKind::RayCandidateTable => {
            matches!(
                observer,
                ArtifactObserver::Query | ArtifactObserver::Collision
            )
        }
        AccelerationArtifactKind::TileCandidateTable
        | AccelerationArtifactKind::ViewDistanceClipmap => {
            matches!(observer, ArtifactObserver::Presentation)
        }
        AccelerationArtifactKind::ContinuationSeedTable => {
            matches!(
                observer,
                ArtifactObserver::Presentation | ArtifactObserver::Collision
            )
        }
    }
}

pub fn validate_acceleration_artifact_contract(contract: &SemanticArtifactContract) -> Vec<String> {
    let Some(acceleration) = contract.acceleration.as_ref() else {
        return Vec::new();
    };
    let mut errors = Vec::new();
    let expected_residency = acceleration_artifact_expected_residency(acceleration.kind);
    if acceleration.residency != expected_residency {
        errors.push(format!(
            "artifact '{}' declares residency '{}' for '{}' but expected '{}'",
            contract.id,
            artifact_residency_name(acceleration.residency),
            acceleration_artifact_kind_name(acceleration.kind),
            artifact_residency_name(expected_residency)
        ));
    }
    if !acceleration_artifact_allows_observer(acceleration.kind, acceleration.observer) {
        errors.push(format!(
            "artifact '{}' kind '{}' is not valid for observer '{}'",
            contract.id,
            acceleration_artifact_kind_name(acceleration.kind),
            artifact_observer_name(acceleration.observer)
        ));
    }
    if acceleration.usage_site.trim().is_empty() {
        errors.push(format!(
            "artifact '{}' acceleration usage_site must not be empty",
            contract.id
        ));
    }

    match acceleration.residency {
        ArtifactResidency::SharedSnapshot => {
            if contract.compatibility.snapshot != ArtifactSnapshotRelation::ExactSnapshot {
                errors.push(format!(
                    "artifact '{}' shared snapshot acceleration must use exact-snapshot compatibility",
                    contract.id
                ));
            }
            if contract.compatibility.transition.requires_previous_snapshot {
                errors.push(format!(
                    "artifact '{}' shared snapshot acceleration must not require a previous snapshot",
                    contract.id
                ));
            }
        }
        ArtifactResidency::ObserverLocal => match acceleration.kind {
            AccelerationArtifactKind::ContinuationSeedTable => {
                if !matches!(
                    contract.compatibility.snapshot,
                    ArtifactSnapshotRelation::ExactSnapshot
                        | ArtifactSnapshotRelation::PreviousSnapshotEpoch
                ) {
                    errors.push(format!(
                        "artifact '{}' continuation seed tables must be exact-snapshot or previous-snapshot scoped",
                        contract.id
                    ));
                }
            }
            _ => {
                if contract.compatibility.snapshot != ArtifactSnapshotRelation::ExactSnapshot {
                    errors.push(format!(
                        "artifact '{}' observer-local acceleration must use exact-snapshot compatibility",
                        contract.id
                    ));
                }
            }
        },
    }

    errors
}

#[cfg(feature = "internal-learned-experiments")]
pub fn validate_learned_method_policy(
    observer: ArtifactObserver,
    policy: LearnedMethodPolicy,
) -> Vec<String> {
    learned_method_policy_rejection(observer, policy)
        .map(|reason| vec![reason.to_string()])
        .unwrap_or_default()
}
