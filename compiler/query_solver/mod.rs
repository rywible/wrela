pub use crate::acceleration::AccelerationRejectionClass;
use crate::query_contract::{
    QueryCardinality, QueryContractId, QueryFamilyId, QueryItemKind, QueryQuestionId,
    QueryResultKind, QueryTargetKind, query_contract,
};
pub use crate::query_contract::{RequiredGuaranteeClass, SelectedMethodClass};
use crate::scene_ir::SupportClass;
pub use crate::semantic_evidence::{
    AnalyticIntersectionStatus, EvidenceClass, EvidenceOrigin, EvidenceRefinementKind,
    EvidenceRefinementStep, EvidenceScope, FactAvailability, LipschitzStatus, PrimitiveFact,
    RepetitionFact, RuntimeBoundsEvidence, SemanticEvidence, SemanticEvidenceSummary,
    TemporalStability, TransformFact,
};
use smol_str::SmolStr;
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RaySolverMethod {
    DenseSphereTracing,
    SupportBoundCandidateRejection,
    AnalyticPrimitiveIntersection,
    LipschitzSafeStepping,
    IntervalNewtonIsolation,
    SafeguardedNewtonRefinement,
    AffineArithmeticBounds,
    RepeatAwareTraversal,
    TilePacketSolving,
    NeighborFrameContinuation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RaySolverMethodStatus {
    Enabled,
    Available,
    Reserved,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RaySolverPortfolioEntry {
    pub method: RaySolverMethod,
    pub status: RaySolverMethodStatus,
    pub reason: SmolStr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RaySolverPortfolio {
    pub entries: Vec<RaySolverPortfolioEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RaySolverMixedSelection {
    pub subject: SmolStr,
    pub candidate_class: SmolStr,
    pub method: RaySolverMethod,
    pub required_guarantee: RequiredGuaranteeClass,
    pub selected_method_class: SelectedMethodClass,
    pub evidence_policy_summary: SmolStr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RaySolverIntentDisposition {
    Used,
    Rejected,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RaySolverArtifactReuseIntent {
    pub selection: RaySolverMixedSelection,
    pub disposition: RaySolverIntentDisposition,
    pub reasons: Vec<SmolStr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RaySolverContinuationIntent {
    pub selection: RaySolverMixedSelection,
    pub disposition: RaySolverIntentDisposition,
    pub reasons: Vec<SmolStr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RaySolverArtifactReuseResolution {
    pub disposition: RaySolverIntentDisposition,
    pub reasons: Vec<SmolStr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RaySolverContinuationResolution {
    pub disposition: RaySolverIntentDisposition,
    pub reasons: Vec<SmolStr>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RaySolverFallbackKind {
    ExactDenseSphereTracing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RaySolverFallbackReason {
    ContractRequiresDenseOracle,
    MissingFieldFacts,
    AnalyticUnsupported,
    VerificationFailed,
    UnsupportedBackend,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RaySolverFallback {
    pub kind: RaySolverFallbackKind,
    pub reasons: Vec<RaySolverFallbackReason>,
    pub preserves_contract: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RaySolverCorrectnessPolicy {
    pub contract_id: QueryContractId,
    pub question: QueryQuestionId,
    pub target: QueryTargetKind,
    pub cardinality: QueryCardinality,
    pub result_kind: QueryResultKind,
    pub preserve_hit3_identity: bool,
    pub preserve_payload: bool,
    pub conservative_miss_policy: bool,
    pub dense_cpu_oracle_required: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RaySolverHitBracketStatus {
    Available,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RaySolverNoCloserHitProof {
    Available,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RaySolverCertificateShape {
    pub method: RaySolverMethod,
    pub hit_or_miss_recorded: bool,
    pub hit_bracket: RaySolverHitBracketStatus,
    pub no_closer_hit_proof: RaySolverNoCloserHitProof,
    pub fallback_reason: Option<RaySolverFallbackReason>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RaySolverPlan {
    pub id: SmolStr,
    pub contract_id: QueryContractId,
    pub subject: SmolStr,
    pub correctness: RaySolverCorrectnessPolicy,
    pub evidence: SemanticEvidence,
    pub portfolio: RaySolverPortfolio,
    pub mixed_selections: Vec<RaySolverMixedSelection>,
    pub artifact_reuse_intents: Vec<RaySolverArtifactReuseIntent>,
    pub continuation_intents: Vec<RaySolverContinuationIntent>,
    pub fallback: RaySolverFallback,
    pub certificate: RaySolverCertificateShape,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RaySolverDiagnosticSummary {
    pub plan_id: SmolStr,
    pub subject: SmolStr,
    pub methods: Vec<RaySolverMethod>,
    pub acceleration_rejection_classes: Vec<AccelerationRejectionClass>,
    pub mixed_selections: Vec<RaySolverMixedSelection>,
    pub artifact_reuse_intents: Vec<RaySolverArtifactReuseIntent>,
    pub continuation_intents: Vec<RaySolverContinuationIntent>,
    pub fallback: RaySolverFallbackKind,
    pub unavailable_facts: Vec<&'static str>,
    pub evidence_summary: SemanticEvidenceSummary,
}

impl RaySolverPlan {
    pub fn for_contract(
        contract_id: QueryContractId,
        evidence: Option<SemanticEvidence>,
    ) -> Option<Self> {
        Self::for_contract_with_subject(contract_id, contract_id.as_str(), evidence)
    }

    pub fn for_contract_with_subject(
        contract_id: QueryContractId,
        subject: impl Into<SmolStr>,
        evidence: Option<SemanticEvidence>,
    ) -> Option<Self> {
        if !is_ray_shaped_spatial_contract(contract_id) {
            return None;
        }
        let descriptor = query_contract(contract_id)?;
        let subject = subject.into();
        let evidence =
            evidence.unwrap_or_else(|| SemanticEvidence::runtime_unknown("world.region.runtime"));
        let evidence_summary = evidence.summary();
        let mut entries = vec![
            RaySolverPortfolioEntry {
                method: RaySolverMethod::DenseSphereTracing,
                status: RaySolverMethodStatus::Enabled,
                reason: SmolStr::new("dense CPU semantics remain the oracle fallback"),
            },
            RaySolverPortfolioEntry {
                method: RaySolverMethod::SupportBoundCandidateRejection,
                status: support_method_status(&evidence),
                reason: SmolStr::new(match evidence.support.lower_bound_pruning {
                    FactAvailability::Available => {
                        "conservative support lower bounds may prune candidates"
                    }
                    FactAvailability::Unknown => {
                        "runtime support evidence may enable candidate rejection"
                    }
                    FactAvailability::Unavailable => {
                        "support lower bounds are unavailable for this plan"
                    }
                }),
            },
            RaySolverPortfolioEntry {
                method: RaySolverMethod::AnalyticPrimitiveIntersection,
                status: analytic_method_status(&evidence),
                reason: SmolStr::new("analytic primitive hits must verify against dense semantics"),
            },
            RaySolverPortfolioEntry {
                method: RaySolverMethod::LipschitzSafeStepping,
                status: lipschitz_method_status(&evidence),
                reason: SmolStr::new("Lipschitz evidence chooses conservative ray steps"),
            },
        ];
        entries.extend([
            reserved_entry(RaySolverMethod::IntervalNewtonIsolation),
            reserved_entry(RaySolverMethod::SafeguardedNewtonRefinement),
            reserved_entry(RaySolverMethod::AffineArithmeticBounds),
            reserved_entry(RaySolverMethod::RepeatAwareTraversal),
            reserved_entry(RaySolverMethod::TilePacketSolving),
            reserved_entry(RaySolverMethod::NeighborFrameContinuation),
        ]);
        let mut reasons = vec![RaySolverFallbackReason::ContractRequiresDenseOracle];
        if matches!(
            evidence.distance.analytic_intersection,
            AnalyticIntersectionStatus::Unavailable
        ) {
            reasons.push(RaySolverFallbackReason::AnalyticUnsupported);
        }
        if matches!(
            evidence.support.lower_bound_pruning,
            FactAvailability::Unavailable
        ) {
            reasons.push(RaySolverFallbackReason::MissingFieldFacts);
        }
        let mixed_selections = entries
            .iter()
            .filter_map(|entry| mixed_selection_for_entry(contract_id, &evidence_summary, entry))
            .collect();
        let artifact_reuse_intents = vec![artifact_reuse_intent(contract_id, &evidence_summary)];
        let continuation_intents = vec![continuation_intent(contract_id, &evidence_summary)];
        let plan = Self {
            id: SmolStr::new(format!("ray-solver:{}:v1", contract_id.as_str())),
            contract_id,
            subject: subject.clone(),
            correctness: RaySolverCorrectnessPolicy {
                contract_id,
                question: descriptor.question,
                target: descriptor.target,
                cardinality: descriptor.cardinality,
                result_kind: descriptor.result_kind,
                preserve_hit3_identity: descriptor.result_kind == QueryResultKind::Hit3
                    || descriptor.preserves_local_hit_context,
                preserve_payload: descriptor.preserves_local_hit_context,
                conservative_miss_policy: true,
                dense_cpu_oracle_required: true,
            },
            evidence,
            portfolio: RaySolverPortfolio { entries },
            mixed_selections,
            artifact_reuse_intents,
            continuation_intents,
            fallback: RaySolverFallback {
                kind: RaySolverFallbackKind::ExactDenseSphereTracing,
                reasons,
                preserves_contract: true,
            },
            certificate: RaySolverCertificateShape {
                method: RaySolverMethod::DenseSphereTracing,
                hit_or_miss_recorded: true,
                hit_bracket: RaySolverHitBracketStatus::Unavailable,
                no_closer_hit_proof: RaySolverNoCloserHitProof::Unavailable,
                fallback_reason: Some(RaySolverFallbackReason::ContractRequiresDenseOracle),
            },
        };
        Some(plan.with_subject(subject))
    }

    pub fn diagnostic_summary(&self) -> RaySolverDiagnosticSummary {
        RaySolverDiagnosticSummary {
            plan_id: self.id.clone(),
            subject: self.subject.clone(),
            methods: self
                .portfolio
                .entries
                .iter()
                .filter(|entry| {
                    matches!(
                        entry.status,
                        RaySolverMethodStatus::Enabled | RaySolverMethodStatus::Available
                    )
                })
                .map(|entry| entry.method)
                .collect(),
            acceleration_rejection_classes: major_acceleration_rejection_classes(&self.evidence),
            mixed_selections: self.mixed_selections.clone(),
            artifact_reuse_intents: self.artifact_reuse_intents.clone(),
            continuation_intents: self.continuation_intents.clone(),
            fallback: self.fallback.kind,
            unavailable_facts: self.evidence.unavailable_labels(),
            evidence_summary: self.evidence.summary(),
        }
    }

    pub fn method_enabled(&self, method: RaySolverMethod) -> bool {
        self.portfolio.entries.iter().any(|entry| {
            entry.method == method && matches!(entry.status, RaySolverMethodStatus::Enabled)
        })
    }

    pub fn dense_fallback_reasons(&self) -> &[RaySolverFallbackReason] {
        &self.fallback.reasons
    }

    pub fn mixed_selections(&self) -> &[RaySolverMixedSelection] {
        &self.mixed_selections
    }

    pub fn artifact_reuse_intents(&self) -> &[RaySolverArtifactReuseIntent] {
        &self.artifact_reuse_intents
    }

    pub fn continuation_intents(&self) -> &[RaySolverContinuationIntent] {
        &self.continuation_intents
    }

    pub fn with_artifact_reuse_resolution(
        &self,
        resolution: RaySolverArtifactReuseResolution,
    ) -> Self {
        let mut plan = self.clone();
        for intent in &mut plan.artifact_reuse_intents {
            intent.disposition = resolution.disposition;
            intent.reasons = resolution.reasons.clone();
        }
        plan
    }

    pub fn with_continuation_resolution(
        &self,
        resolution: RaySolverContinuationResolution,
    ) -> Self {
        let mut plan = self.clone();
        for intent in &mut plan.continuation_intents {
            intent.disposition = resolution.disposition;
            intent.reasons = resolution.reasons.clone();
        }
        plan
    }

    pub fn with_subject(&self, subject: impl Into<SmolStr>) -> Self {
        let subject = subject.into();
        let mut plan = self.clone();
        plan.subject = subject.clone();
        for selection in &mut plan.mixed_selections {
            selection.subject = subject.clone();
            selection.evidence_policy_summary =
                rewrite_selection_subject(&selection.evidence_policy_summary, &subject);
        }
        for intent in &mut plan.artifact_reuse_intents {
            intent.selection.subject = subject.clone();
            intent.selection.evidence_policy_summary =
                rewrite_selection_subject(&intent.selection.evidence_policy_summary, &subject);
        }
        for intent in &mut plan.continuation_intents {
            intent.selection.subject = subject.clone();
            intent.selection.evidence_policy_summary =
                rewrite_selection_subject(&intent.selection.evidence_policy_summary, &subject);
        }
        plan
    }
}

pub fn is_ray_shaped_spatial_contract(contract_id: QueryContractId) -> bool {
    let Some(descriptor) = query_contract(contract_id) else {
        return false;
    };
    descriptor.family == QueryFamilyId::Spatial
        && matches!(
            descriptor.question,
            QueryQuestionId::Nearest | QueryQuestionId::Occluded
        )
        && descriptor.target == QueryTargetKind::World
        && descriptor.item_kind == QueryItemKind::RayQuery
        && matches!(
            descriptor.result_kind,
            QueryResultKind::Hit3 | QueryResultKind::OcclusionResult
        )
}

fn reserved_entry(method: RaySolverMethod) -> RaySolverPortfolioEntry {
    RaySolverPortfolioEntry {
        method,
        status: RaySolverMethodStatus::Reserved,
        reason: SmolStr::new("reserved for later query-owned solver work"),
    }
}

fn support_method_status(evidence: &SemanticEvidence) -> RaySolverMethodStatus {
    match evidence.support.lower_bound_pruning {
        FactAvailability::Available
            if compile_trust(
                &evidence.support.provenance.origin,
                &evidence.support.provenance.scope,
            ) =>
        {
            RaySolverMethodStatus::Enabled
        }
        FactAvailability::Available | FactAvailability::Unknown => RaySolverMethodStatus::Available,
        FactAvailability::Unavailable => RaySolverMethodStatus::Unavailable,
    }
}

fn analytic_method_status(evidence: &SemanticEvidence) -> RaySolverMethodStatus {
    match evidence.distance.analytic_intersection {
        AnalyticIntersectionStatus::Available | AnalyticIntersectionStatus::CandidateOnly
            if compile_trust(
                &evidence.distance.provenance.origin,
                &evidence.distance.provenance.scope,
            ) =>
        {
            RaySolverMethodStatus::Enabled
        }
        AnalyticIntersectionStatus::Available
        | AnalyticIntersectionStatus::CandidateOnly
        | AnalyticIntersectionStatus::Unknown => RaySolverMethodStatus::Available,
        AnalyticIntersectionStatus::Unavailable => RaySolverMethodStatus::Unavailable,
    }
}

fn lipschitz_method_status(evidence: &SemanticEvidence) -> RaySolverMethodStatus {
    match evidence.distance.lipschitz {
        LipschitzStatus::ExactKnown | LipschitzStatus::ConservativeKnown
            if compile_trust(
                &evidence.distance.provenance.origin,
                &evidence.distance.provenance.scope,
            ) =>
        {
            RaySolverMethodStatus::Enabled
        }
        LipschitzStatus::ExactKnown
        | LipschitzStatus::ConservativeKnown
        | LipschitzStatus::Unknown => RaySolverMethodStatus::Available,
        LipschitzStatus::Unavailable => RaySolverMethodStatus::Unavailable,
    }
}

fn compile_trust(origin: &EvidenceOrigin, scope: &EvidenceScope) -> bool {
    matches!(origin, EvidenceOrigin::StaticCompiled)
        && matches!(scope, EvidenceScope::CompileInvariant)
}

fn major_acceleration_rejection_classes(
    evidence: &SemanticEvidence,
) -> Vec<AccelerationRejectionClass> {
    let mut classes = BTreeSet::new();
    if matches!(
        evidence.distance.provenance.origin,
        EvidenceOrigin::ArtifactDerived
    ) || matches!(
        evidence.support.provenance.origin,
        EvidenceOrigin::ArtifactDerived
    ) || matches!(
        evidence.differential.provenance.origin,
        EvidenceOrigin::ArtifactDerived
    ) || matches!(
        evidence.identity.provenance.origin,
        EvidenceOrigin::ArtifactDerived
    ) || matches!(
        evidence.temporal.provenance.origin,
        EvidenceOrigin::ArtifactDerived
    ) {
        classes.insert(AccelerationRejectionClass::ArtifactInvalid);
    }
    if evidence.support.opaque_boundary {
        classes.insert(AccelerationRejectionClass::OpaqueBoundary);
    }
    if matches!(
        evidence.support.lower_bound_pruning,
        FactAvailability::Unavailable
    ) || matches!(
        evidence.support.conservative_bounds,
        FactAvailability::Unavailable
    ) || (matches!(
        evidence.support.support_class,
        SupportClass::Unknown | SupportClass::Unbounded | SupportClass::Periodic
    ) && !matches!(
        evidence.support.conservative_bounds,
        FactAvailability::Available
    )) {
        classes.insert(AccelerationRejectionClass::UnboundedSupport);
    }
    if matches!(
        evidence.differential.transform,
        TransformFact::AffineOrWarp | TransformFact::Unknown
    ) {
        classes.insert(AccelerationRejectionClass::UnsupportedTransform);
    }
    if matches!(
        evidence.differential.repetition,
        RepetitionFact::Repeat(_) | RepetitionFact::IdentityAffecting(_) | RepetitionFact::Unknown
    ) && !matches!(
        evidence.support.lower_bound_pruning,
        FactAvailability::Available
    ) {
        classes.insert(AccelerationRejectionClass::UnsupportedRepeatForm);
    }
    if matches!(
        evidence.distance.interval_bounds,
        FactAvailability::Unavailable
    ) || matches!(
        evidence.distance.analytic_intersection,
        AnalyticIntersectionStatus::Unavailable
    ) || matches!(evidence.distance.lipschitz, LipschitzStatus::Unavailable)
    {
        classes.insert(AccelerationRejectionClass::ArtifactUnavailable);
    }
    classes.into_iter().collect()
}

fn mixed_selection_for_entry(
    contract_id: QueryContractId,
    evidence_summary: &SemanticEvidenceSummary,
    entry: &RaySolverPortfolioEntry,
) -> Option<RaySolverMixedSelection> {
    if !matches!(
        entry.status,
        RaySolverMethodStatus::Enabled | RaySolverMethodStatus::Available
    ) {
        return None;
    }
    let required_guarantee = required_guarantee_for_method(entry.method);
    let selected_method_class = selected_method_class_for_method(entry.method);
    Some(RaySolverMixedSelection {
        subject: SmolStr::new(contract_id.as_str()),
        candidate_class: SmolStr::new(candidate_class_for_method(entry.method)),
        method: entry.method,
        required_guarantee,
        selected_method_class,
        evidence_policy_summary: SmolStr::new(format!(
            "subject={} candidate_class={} method={} guarantee={} class={} evidence-origin={:?} evidence-scope={:?} support-pruning={:?} analytic={:?} lipschitz={:?}; {}",
            contract_id.as_str(),
            candidate_class_for_method(entry.method),
            ray_solver_method_name(entry.method),
            required_guarantee.name(),
            selected_method_class.name(),
            evidence_summary.origin,
            evidence_summary.scope,
            evidence_summary.support.lower_bound_pruning,
            evidence_summary.distance.analytic_intersection,
            evidence_summary.distance.lipschitz,
            entry.reason.as_str()
        )),
    })
}

fn artifact_reuse_intent(
    contract_id: QueryContractId,
    evidence_summary: &SemanticEvidenceSummary,
) -> RaySolverArtifactReuseIntent {
    let selection = RaySolverMixedSelection {
        subject: SmolStr::new(contract_id.as_str()),
        candidate_class: SmolStr::new("artifact-reuse"),
        method: RaySolverMethod::SupportBoundCandidateRejection,
        required_guarantee: RequiredGuaranteeClass::ConservativeNoFalseMiss,
        selected_method_class: SelectedMethodClass::ConservativeSolver,
        evidence_policy_summary: SmolStr::new(format!(
            "subject={} artifact reuse candidate; evidence-origin={:?} evidence-scope={:?} support-pruning={:?} analytic={:?}",
            contract_id.as_str(),
            evidence_summary.origin,
            evidence_summary.scope,
            evidence_summary.support.lower_bound_pruning,
            evidence_summary.distance.analytic_intersection
        )),
    };
    let has_artifact_binding = matches!(evidence_summary.origin, EvidenceOrigin::ArtifactDerived)
        || matches!(evidence_summary.scope, EvidenceScope::ArtifactBound);
    let has_reusable_evidence = matches!(
        evidence_summary.support.lower_bound_pruning,
        FactAvailability::Available | FactAvailability::Unknown
    ) || matches!(
        evidence_summary.distance.analytic_intersection,
        AnalyticIntersectionStatus::Available
            | AnalyticIntersectionStatus::CandidateOnly
            | AnalyticIntersectionStatus::Unknown
    );
    let (disposition, reasons) = if has_artifact_binding {
        (
            RaySolverIntentDisposition::Rejected,
            vec![
                SmolStr::new("artifact-derived evidence must be revalidated before reuse is legal"),
                SmolStr::new(format!(
                    "artifact provenance origin={:?} scope={:?} requires a fresh compatibility check",
                    evidence_summary.origin, evidence_summary.scope
                )),
            ],
        )
    } else if has_reusable_evidence {
        (
            RaySolverIntentDisposition::Unavailable,
            vec![
                SmolStr::new(
                    "compatible runtime artifact instance is required before reuse becomes legal",
                ),
                SmolStr::new(format!(
                    "evidence origin={:?} scope={:?} constrains which artifacts may be reused",
                    evidence_summary.origin, evidence_summary.scope
                )),
            ],
        )
    } else {
        (
            RaySolverIntentDisposition::Rejected,
            vec![
                SmolStr::new("evidence does not justify artifact reuse for this contract"),
                SmolStr::new(format!(
                    "support-pruning={:?} analytic={:?}",
                    evidence_summary.support.lower_bound_pruning,
                    evidence_summary.distance.analytic_intersection
                )),
            ],
        )
    };
    RaySolverArtifactReuseIntent {
        selection,
        disposition,
        reasons,
    }
}

fn continuation_intent(
    contract_id: QueryContractId,
    evidence_summary: &SemanticEvidenceSummary,
) -> RaySolverContinuationIntent {
    let selection = RaySolverMixedSelection {
        subject: SmolStr::new(contract_id.as_str()),
        candidate_class: SmolStr::new("temporal-continuation"),
        method: RaySolverMethod::NeighborFrameContinuation,
        required_guarantee: RequiredGuaranteeClass::ConservativeNoFalseMiss,
        selected_method_class: SelectedMethodClass::HeuristicSolver,
        evidence_policy_summary: SmolStr::new(format!(
            "subject={} continuation candidate; temporal-stability={:?} change-class={:?} stationary={:?} rigid-over-interval={:?}",
            contract_id.as_str(),
            evidence_summary.temporal.stability,
            evidence_summary.temporal.change_class,
            evidence_summary.temporal.stationary,
            evidence_summary.temporal.rigid_over_interval
        )),
    };
    let has_continuation_evidence = matches!(
        evidence_summary.temporal.stability,
        TemporalStability::CompileInvariant
            | TemporalStability::TransitionCompatible
            | TemporalStability::SnapshotLocal
            | TemporalStability::ArtifactBound
    ) && matches!(
        evidence_summary.temporal.rigid_over_interval,
        FactAvailability::Available | FactAvailability::Unknown
    );
    let (disposition, reasons) = if has_continuation_evidence {
        (
            RaySolverIntentDisposition::Unavailable,
            vec![
                SmolStr::new(
                    "compatible runtime transition context is required before continuation is legal",
                ),
                SmolStr::new(format!(
                    "temporal stability={:?} change class={:?} constrains which prior frames may continue",
                    evidence_summary.temporal.stability, evidence_summary.temporal.change_class
                )),
            ],
        )
    } else {
        (
            RaySolverIntentDisposition::Rejected,
            vec![
                SmolStr::new("temporal evidence does not justify continuation for this contract"),
                SmolStr::new(format!(
                    "stability={:?} stationary={:?} rigid-over-interval={:?} topology-stable={:?}",
                    evidence_summary.temporal.stability,
                    evidence_summary.temporal.stationary,
                    evidence_summary.temporal.rigid_over_interval,
                    evidence_summary.temporal.topology_stable
                )),
            ],
        )
    };
    RaySolverContinuationIntent {
        selection,
        disposition,
        reasons,
    }
}

fn required_guarantee_for_method(method: RaySolverMethod) -> RequiredGuaranteeClass {
    match method {
        RaySolverMethod::DenseSphereTracing => RequiredGuaranteeClass::Exact,
        RaySolverMethod::SupportBoundCandidateRejection => {
            RequiredGuaranteeClass::ConservativeNoFalseMiss
        }
        RaySolverMethod::AnalyticPrimitiveIntersection => RequiredGuaranteeClass::Exact,
        RaySolverMethod::LipschitzSafeStepping => RequiredGuaranteeClass::IntervalBounded,
        RaySolverMethod::IntervalNewtonIsolation
        | RaySolverMethod::SafeguardedNewtonRefinement
        | RaySolverMethod::AffineArithmeticBounds => RequiredGuaranteeClass::IntervalBounded,
        RaySolverMethod::RepeatAwareTraversal
        | RaySolverMethod::TilePacketSolving
        | RaySolverMethod::NeighborFrameContinuation => RequiredGuaranteeClass::BestEffort,
    }
}

fn selected_method_class_for_method(method: RaySolverMethod) -> SelectedMethodClass {
    match method {
        RaySolverMethod::DenseSphereTracing | RaySolverMethod::AnalyticPrimitiveIntersection => {
            SelectedMethodClass::ExactOracle
        }
        RaySolverMethod::SupportBoundCandidateRejection => SelectedMethodClass::ConservativeSolver,
        RaySolverMethod::LipschitzSafeStepping
        | RaySolverMethod::IntervalNewtonIsolation
        | RaySolverMethod::SafeguardedNewtonRefinement
        | RaySolverMethod::AffineArithmeticBounds => SelectedMethodClass::IntervalSolver,
        RaySolverMethod::RepeatAwareTraversal
        | RaySolverMethod::TilePacketSolving
        | RaySolverMethod::NeighborFrameContinuation => SelectedMethodClass::HeuristicSolver,
    }
}

fn candidate_class_for_method(method: RaySolverMethod) -> &'static str {
    match method {
        RaySolverMethod::DenseSphereTracing => "dense-oracle",
        RaySolverMethod::SupportBoundCandidateRejection => "support-bounded-candidates",
        RaySolverMethod::AnalyticPrimitiveIntersection => "analytic-primitive-candidates",
        RaySolverMethod::LipschitzSafeStepping => "lipschitz-safe-candidates",
        RaySolverMethod::IntervalNewtonIsolation => "interval-isolation-candidates",
        RaySolverMethod::SafeguardedNewtonRefinement => "newton-refinement-candidates",
        RaySolverMethod::AffineArithmeticBounds => "affine-bounds-candidates",
        RaySolverMethod::RepeatAwareTraversal => "repeat-aware-candidates",
        RaySolverMethod::TilePacketSolving => "tile-packet-candidates",
        RaySolverMethod::NeighborFrameContinuation => "temporal-continuation",
    }
}

fn rewrite_selection_subject(summary: &SmolStr, subject: &SmolStr) -> SmolStr {
    let Some(rest) = summary.as_str().strip_prefix("subject=") else {
        return summary.clone();
    };
    let rewritten = match rest.split_once(' ') {
        Some((_, suffix)) => format!("subject={} {}", subject, suffix),
        None => format!("subject={}", subject),
    };
    SmolStr::new(rewritten)
}

pub fn ray_solver_method_name(method: RaySolverMethod) -> &'static str {
    match method {
        RaySolverMethod::DenseSphereTracing => "dense-sphere-tracing",
        RaySolverMethod::SupportBoundCandidateRejection => "support-bound-candidate-rejection",
        RaySolverMethod::AnalyticPrimitiveIntersection => "analytic-primitive-intersection",
        RaySolverMethod::LipschitzSafeStepping => "lipschitz-safe-stepping",
        RaySolverMethod::IntervalNewtonIsolation => "interval-newton-isolation",
        RaySolverMethod::SafeguardedNewtonRefinement => "safeguarded-newton-refinement",
        RaySolverMethod::AffineArithmeticBounds => "affine-arithmetic-bounds",
        RaySolverMethod::RepeatAwareTraversal => "repeat-aware-traversal",
        RaySolverMethod::TilePacketSolving => "tile-packet-solving",
        RaySolverMethod::NeighborFrameContinuation => "neighbor-frame-continuation",
    }
}

pub fn ray_solver_fallback_name(kind: RaySolverFallbackKind) -> &'static str {
    match kind {
        RaySolverFallbackKind::ExactDenseSphereTracing => "exact-dense-sphere-tracing",
    }
}
