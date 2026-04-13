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
