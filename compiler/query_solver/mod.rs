use crate::query_contract::{
    QueryCardinality, QueryContractId, QueryFamilyId, QueryItemKind, QueryQuestionId,
    QueryResultKind, QueryTargetKind, query_contract,
};
pub use crate::semantic_evidence::{
    AnalyticIntersectionStatus, EvidenceClass, EvidenceOrigin, EvidenceRefinementKind,
    EvidenceRefinementStep, EvidenceScope, FactAvailability, LipschitzStatus, PrimitiveFact,
    RepetitionFact, RuntimeBoundsEvidence, SemanticEvidence, SemanticEvidenceSummary,
    TemporalStability, TransformFact,
};
use smol_str::SmolStr;

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
    pub correctness: RaySolverCorrectnessPolicy,
    pub evidence: SemanticEvidence,
    pub portfolio: RaySolverPortfolio,
    pub fallback: RaySolverFallback,
    pub certificate: RaySolverCertificateShape,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RaySolverDiagnosticSummary {
    pub plan_id: SmolStr,
    pub methods: Vec<RaySolverMethod>,
    pub fallback: RaySolverFallbackKind,
    pub unavailable_facts: Vec<&'static str>,
    pub evidence_summary: SemanticEvidenceSummary,
}

impl RaySolverPlan {
    pub fn for_contract(
        contract_id: QueryContractId,
        evidence: Option<SemanticEvidence>,
    ) -> Option<Self> {
        if !is_ray_shaped_spatial_contract(contract_id) {
            return None;
        }
        let descriptor = query_contract(contract_id)?;
        let evidence =
            evidence.unwrap_or_else(|| SemanticEvidence::runtime_unknown("world.region.runtime"));
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
        Some(Self {
            id: SmolStr::new(format!("ray-solver:{}:v1", contract_id.as_str())),
            contract_id,
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
        })
    }

    pub fn diagnostic_summary(&self) -> RaySolverDiagnosticSummary {
        RaySolverDiagnosticSummary {
            plan_id: self.id.clone(),
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
