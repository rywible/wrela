use crate::hir;
use crate::query_contract::{
    QueryCardinality, QueryContractId, QueryFamilyId, QueryItemKind, QueryQuestionId,
    QueryResultKind, QueryTargetKind, query_contract,
};
use crate::scene_ir::{
    DistanceSemantics, FieldNodeKindSummary, FieldScene, RepeatKind, SceneIdentitySourceKind,
    ShapeNodeKindSummary, ShapeScene, SupportClass, SupportNodeKindSummary, TransformKind,
};
use smol_str::SmolStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FactAvailability {
    Available,
    Unavailable,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LipschitzStatus {
    ExactKnown,
    ConservativeKnown,
    Unknown,
    Unavailable,
}

impl LipschitzStatus {
    pub const fn availability(self) -> FactAvailability {
        match self {
            Self::ExactKnown | Self::ConservativeKnown => FactAvailability::Available,
            Self::Unknown => FactAvailability::Unknown,
            Self::Unavailable => FactAvailability::Unavailable,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrimitiveFact {
    None,
    Single(hir::FieldPrimitive),
    Composite,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TransformFact {
    None,
    Rigid,
    UniformScale,
    AffineOrWarp,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RepetitionFact {
    None,
    Repeat(RepeatKind),
    IdentityAffecting(RepeatKind),
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AnalyticIntersectionStatus {
    Available,
    CandidateOnly,
    Unavailable,
    Unknown,
}

impl AnalyticIntersectionStatus {
    pub const fn availability(self) -> FactAvailability {
        match self {
            Self::Available | Self::CandidateOnly => FactAvailability::Available,
            Self::Unavailable => FactAvailability::Unavailable,
            Self::Unknown => FactAvailability::Unknown,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupportFacts {
    pub support_class: SupportClass,
    pub semantics: DistanceSemantics,
    pub conservative_bounds: FactAvailability,
    pub lower_bound_pruning: FactAvailability,
    pub can_coarse_prune: bool,
    pub opaque_boundary: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProvenanceFacts {
    pub hit3_identity_required: bool,
    pub stable_feature_id: bool,
    pub stable_instance_id: bool,
    pub stable_repeat_id: bool,
    pub payload_required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldFacts {
    pub subject: SmolStr,
    pub support: SupportFacts,
    pub primitive: PrimitiveFact,
    pub transform: TransformFact,
    pub repetition: RepetitionFact,
    pub derivative: FactAvailability,
    pub lipschitz: LipschitzStatus,
    pub analytic_intersection: AnalyticIntersectionStatus,
    pub interval_bounds: FactAvailability,
    pub provenance: ProvenanceFacts,
}

impl FieldFacts {
    pub fn runtime_unknown(subject: impl Into<SmolStr>) -> Self {
        Self {
            subject: subject.into(),
            support: SupportFacts {
                support_class: SupportClass::Unknown,
                semantics: DistanceSemantics::ConservativeLowerBound,
                conservative_bounds: FactAvailability::Unknown,
                lower_bound_pruning: FactAvailability::Unknown,
                can_coarse_prune: false,
                opaque_boundary: false,
            },
            primitive: PrimitiveFact::Unknown,
            transform: TransformFact::Unknown,
            repetition: RepetitionFact::Unknown,
            derivative: FactAvailability::Unknown,
            lipschitz: LipschitzStatus::Unknown,
            analytic_intersection: AnalyticIntersectionStatus::Unknown,
            interval_bounds: FactAvailability::Unknown,
            provenance: ProvenanceFacts {
                hit3_identity_required: false,
                stable_feature_id: false,
                stable_instance_id: false,
                stable_repeat_id: false,
                payload_required: false,
            },
        }
    }

    pub fn unavailable(subject: impl Into<SmolStr>) -> Self {
        Self {
            subject: subject.into(),
            support: SupportFacts {
                support_class: SupportClass::Unknown,
                semantics: DistanceSemantics::UnknownOpaque,
                conservative_bounds: FactAvailability::Unknown,
                lower_bound_pruning: FactAvailability::Unavailable,
                can_coarse_prune: false,
                opaque_boundary: true,
            },
            primitive: PrimitiveFact::Unknown,
            transform: TransformFact::Unknown,
            repetition: RepetitionFact::Unknown,
            derivative: FactAvailability::Unavailable,
            lipschitz: LipschitzStatus::Unknown,
            analytic_intersection: AnalyticIntersectionStatus::Unavailable,
            interval_bounds: FactAvailability::Unavailable,
            provenance: ProvenanceFacts {
                hit3_identity_required: false,
                stable_feature_id: false,
                stable_instance_id: false,
                stable_repeat_id: false,
                payload_required: false,
            },
        }
    }

    pub fn for_field_scene(scene: &FieldScene) -> Self {
        let primitive = primitive_fact_for_field(scene);
        let transform = transform_fact_for_field(scene);
        let repetition = repetition_fact_for_field(scene);
        let derivative = derivative_availability(primitive, transform, repetition);
        let lipschitz = lipschitz_for_scene(scene.semantics, transform, repetition);
        let analytic_intersection = analytic_status(primitive, transform, repetition);
        Self {
            subject: scene.name.clone(),
            support: support_facts(
                scene.support_class,
                scene.semantics,
                scene.can_coarse_support_pruning,
                scene.opaque_boundary,
                scene
                    .support_records
                    .iter()
                    .any(|record| supports_bounds(&record.kind)),
            ),
            primitive,
            transform,
            repetition,
            derivative,
            lipschitz,
            analytic_intersection,
            interval_bounds: FactAvailability::Unavailable,
            provenance: ProvenanceFacts {
                hit3_identity_required: false,
                stable_feature_id: false,
                stable_instance_id: false,
                stable_repeat_id: !scene.identity_sources.is_empty(),
                payload_required: false,
            },
        }
    }

    pub fn for_shape_scene(scene: &ShapeScene) -> Self {
        let primitive = primitive_fact_for_shape(scene);
        let transform = transform_fact_for_shape(scene);
        let repetition = repetition_fact_for_shape(scene);
        let derivative = derivative_availability(primitive, transform, repetition);
        let lipschitz = lipschitz_for_scene(scene.semantics, transform, repetition);
        let analytic_intersection = analytic_status(primitive, transform, repetition);
        let stable_repeat_id = scene.leaves.values().any(|leaf| {
            // Shape leaves inherit repeat/instance identity from their field-local frame.
            matches!(
                leaf.field_semantics,
                DistanceSemantics::ExactSignedDistance | DistanceSemantics::ConservativeLowerBound
            )
        });
        Self {
            subject: scene.name.clone(),
            support: support_facts(
                scene.support_class,
                scene.semantics,
                scene.can_coarse_support_pruning,
                scene.opaque_boundary,
                scene
                    .support_records
                    .iter()
                    .any(|record| supports_bounds(&record.kind)),
            ),
            primitive,
            transform,
            repetition,
            derivative,
            lipschitz,
            analytic_intersection,
            interval_bounds: FactAvailability::Unavailable,
            provenance: ProvenanceFacts {
                hit3_identity_required: true,
                stable_feature_id: !scene.leaves.is_empty(),
                stable_instance_id: stable_repeat_id,
                stable_repeat_id,
                payload_required: true,
            },
        }
    }

    pub fn unavailable_labels(&self) -> Vec<&'static str> {
        let mut labels = Vec::new();
        if matches!(
            self.support.conservative_bounds,
            FactAvailability::Unavailable
        ) {
            labels.push("support.conservative_bounds");
        }
        if matches!(
            self.support.lower_bound_pruning,
            FactAvailability::Unavailable
        ) {
            labels.push("support.lower_bound_pruning");
        }
        if matches!(self.derivative, FactAvailability::Unavailable) {
            labels.push("derivative");
        }
        if matches!(self.lipschitz, LipschitzStatus::Unavailable) {
            labels.push("lipschitz");
        }
        if matches!(
            self.analytic_intersection,
            AnalyticIntersectionStatus::Unavailable
        ) {
            labels.push("analytic");
        }
        if matches!(self.interval_bounds, FactAvailability::Unavailable) {
            labels.push("interval");
        }
        labels
    }
}

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
    pub facts: FieldFacts,
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
}

impl RaySolverPlan {
    pub fn for_contract(contract_id: QueryContractId, facts: Option<FieldFacts>) -> Option<Self> {
        if !is_ray_shaped_spatial_contract(contract_id) {
            return None;
        }
        let descriptor = query_contract(contract_id)?;
        let facts = facts.unwrap_or_else(|| FieldFacts::runtime_unknown("world.region.runtime"));
        let mut entries = vec![
            RaySolverPortfolioEntry {
                method: RaySolverMethod::DenseSphereTracing,
                status: RaySolverMethodStatus::Enabled,
                reason: SmolStr::new("dense CPU semantics remain the oracle fallback"),
            },
            RaySolverPortfolioEntry {
                method: RaySolverMethod::SupportBoundCandidateRejection,
                status: match facts.support.lower_bound_pruning {
                    FactAvailability::Available => RaySolverMethodStatus::Enabled,
                    FactAvailability::Unknown => RaySolverMethodStatus::Available,
                    FactAvailability::Unavailable => RaySolverMethodStatus::Unavailable,
                },
                reason: SmolStr::new(match facts.support.lower_bound_pruning {
                    FactAvailability::Available => {
                        "conservative support lower bounds may prune candidates"
                    }
                    FactAvailability::Unknown => {
                        "runtime support facts may enable candidate rejection"
                    }
                    FactAvailability::Unavailable => {
                        "support lower bounds are unavailable for this plan"
                    }
                }),
            },
            RaySolverPortfolioEntry {
                method: RaySolverMethod::AnalyticPrimitiveIntersection,
                status: match facts.analytic_intersection {
                    AnalyticIntersectionStatus::Available
                    | AnalyticIntersectionStatus::CandidateOnly => RaySolverMethodStatus::Enabled,
                    AnalyticIntersectionStatus::Unavailable => RaySolverMethodStatus::Unavailable,
                    AnalyticIntersectionStatus::Unknown => RaySolverMethodStatus::Available,
                },
                reason: SmolStr::new("analytic primitive hits must verify against dense semantics"),
            },
            RaySolverPortfolioEntry {
                method: RaySolverMethod::LipschitzSafeStepping,
                status: match facts.lipschitz {
                    LipschitzStatus::ExactKnown | LipschitzStatus::ConservativeKnown => {
                        RaySolverMethodStatus::Enabled
                    }
                    LipschitzStatus::Unknown => RaySolverMethodStatus::Available,
                    LipschitzStatus::Unavailable => RaySolverMethodStatus::Unavailable,
                },
                reason: SmolStr::new("Lipschitz facts choose conservative ray steps"),
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
            facts.analytic_intersection,
            AnalyticIntersectionStatus::Unavailable
        ) {
            reasons.push(RaySolverFallbackReason::AnalyticUnsupported);
        }
        if matches!(
            facts.support.lower_bound_pruning,
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
            facts,
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
            unavailable_facts: self.facts.unavailable_labels(),
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

fn support_facts(
    support_class: SupportClass,
    semantics: DistanceSemantics,
    can_coarse_prune: bool,
    opaque_boundary: bool,
    has_bounds: bool,
) -> SupportFacts {
    SupportFacts {
        support_class,
        semantics,
        conservative_bounds: if has_bounds {
            FactAvailability::Available
        } else if matches!(
            support_class,
            SupportClass::Unknown | SupportClass::Unbounded
        ) {
            FactAvailability::Unknown
        } else {
            FactAvailability::Unavailable
        },
        lower_bound_pruning: if can_coarse_prune && has_bounds && !opaque_boundary {
            FactAvailability::Available
        } else if opaque_boundary {
            FactAvailability::Unavailable
        } else {
            FactAvailability::Unknown
        },
        can_coarse_prune,
        opaque_boundary,
    }
}

fn supports_bounds(kind: &SupportNodeKindSummary) -> bool {
    matches!(
        kind,
        &SupportNodeKindSummary::Aabb
            | &SupportNodeKindSummary::Sphere
            | &SupportNodeKindSummary::OpaqueBoundary
    )
}

fn primitive_fact_for_field(scene: &FieldScene) -> PrimitiveFact {
    let mut primitives = scene
        .node_records
        .iter()
        .filter_map(|record| match record.kind {
            FieldNodeKindSummary::Primitive(primitive) => Some(primitive),
            _ => None,
        });
    let Some(first) = primitives.next() else {
        return PrimitiveFact::None;
    };
    if primitives.next().is_some() {
        PrimitiveFact::Composite
    } else {
        PrimitiveFact::Single(first)
    }
}

fn primitive_fact_for_shape(scene: &ShapeScene) -> PrimitiveFact {
    let mut primitives = scene.leaves.values().filter_map(|leaf| {
        matches!(leaf.field_semantics, DistanceSemantics::ExactSignedDistance)
            .then_some(PrimitiveFact::Unknown)
    });
    match (scene.node_records.as_slice(), primitives.next()) {
        ([record], Some(_)) if record.kind == ShapeNodeKindSummary::Leaf => PrimitiveFact::Unknown,
        _ if scene.leaves.len() > 1 => PrimitiveFact::Composite,
        _ => PrimitiveFact::Unknown,
    }
}

fn transform_fact_for_field(scene: &FieldScene) -> TransformFact {
    summarize_transforms(
        scene
            .node_records
            .iter()
            .filter_map(|record| match record.kind {
                FieldNodeKindSummary::Transform(kind) => Some(kind),
                _ => None,
            }),
    )
}

fn transform_fact_for_shape(_scene: &ShapeScene) -> TransformFact {
    TransformFact::Unknown
}

fn summarize_transforms<I>(transforms: I) -> TransformFact
where
    I: IntoIterator<Item = TransformKind>,
{
    let mut fact = TransformFact::None;
    for kind in transforms {
        fact = match kind {
            TransformKind::Translate | TransformKind::Rotate => match fact {
                TransformFact::None | TransformFact::Rigid => TransformFact::Rigid,
                other => other,
            },
            TransformKind::UniformScale => match fact {
                TransformFact::None | TransformFact::Rigid | TransformFact::UniformScale => {
                    TransformFact::UniformScale
                }
                other => other,
            },
            TransformKind::AffineTransform
            | TransformKind::Warp
            | TransformKind::Bend
            | TransformKind::Twist
            | TransformKind::Taper
            | TransformKind::Displace => TransformFact::AffineOrWarp,
        };
    }
    fact
}

fn repetition_fact_for_field(scene: &FieldScene) -> RepetitionFact {
    if let Some(identity) = scene.identity_sources.first() {
        return match identity.kind {
            SceneIdentitySourceKind::Repeat | SceneIdentitySourceKind::Instance => {
                RepetitionFact::IdentityAffecting(identity.repeat_kind)
            }
        };
    }
    scene
        .node_records
        .iter()
        .find_map(|record| match record.kind {
            FieldNodeKindSummary::Repeat(kind) => Some(RepetitionFact::Repeat(kind)),
            _ => None,
        })
        .unwrap_or(RepetitionFact::None)
}

fn repetition_fact_for_shape(_scene: &ShapeScene) -> RepetitionFact {
    RepetitionFact::Unknown
}

fn derivative_availability(
    primitive: PrimitiveFact,
    transform: TransformFact,
    repetition: RepetitionFact,
) -> FactAvailability {
    if matches!(primitive, PrimitiveFact::Single(_))
        && matches!(
            transform,
            TransformFact::None | TransformFact::Rigid | TransformFact::UniformScale
        )
        && matches!(repetition, RepetitionFact::None)
    {
        FactAvailability::Available
    } else {
        FactAvailability::Unavailable
    }
}

fn lipschitz_for_scene(
    semantics: DistanceSemantics,
    transform: TransformFact,
    repetition: RepetitionFact,
) -> LipschitzStatus {
    if !matches!(repetition, RepetitionFact::None) {
        return LipschitzStatus::Unknown;
    }
    match (semantics, transform) {
        (DistanceSemantics::ExactSignedDistance, TransformFact::None | TransformFact::Rigid) => {
            LipschitzStatus::ExactKnown
        }
        (
            DistanceSemantics::ExactSignedDistance | DistanceSemantics::ConservativeLowerBound,
            TransformFact::UniformScale,
        ) => LipschitzStatus::ConservativeKnown,
        (DistanceSemantics::UnknownOpaque, _) => LipschitzStatus::Unavailable,
        _ => LipschitzStatus::Unknown,
    }
}

fn analytic_status(
    primitive: PrimitiveFact,
    transform: TransformFact,
    repetition: RepetitionFact,
) -> AnalyticIntersectionStatus {
    if matches!(repetition, RepetitionFact::None)
        && matches!(transform, TransformFact::None | TransformFact::Rigid)
    {
        match primitive {
            PrimitiveFact::Single(
                hir::FieldPrimitive::Sphere
                | hir::FieldPrimitive::Plane
                | hir::FieldPrimitive::Slab,
            ) => AnalyticIntersectionStatus::CandidateOnly,
            PrimitiveFact::Single(_) | PrimitiveFact::Composite | PrimitiveFact::None => {
                AnalyticIntersectionStatus::Unavailable
            }
            PrimitiveFact::Unknown => AnalyticIntersectionStatus::Unknown,
        }
    } else {
        AnalyticIntersectionStatus::Unavailable
    }
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
