use crate::hir;
use crate::scene_ir::{
    DistanceSemantics, FieldNodeKindSummary, FieldScene, RepeatKind, SceneDifferentialSupport,
    SceneIdentitySourceKind, ShapeNodeKindSummary, ShapeScene, SupportClass,
    SupportNodeKindSummary, TransformKind,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EvidenceOrigin {
    StaticCompiled,
    RuntimeObserved,
    ArtifactDerived,
    ImportedCompatibility,
}

impl EvidenceOrigin {
    pub const fn default_scope(self) -> EvidenceScope {
        match self {
            Self::StaticCompiled => EvidenceScope::CompileInvariant,
            Self::RuntimeObserved => EvidenceScope::SnapshotLocal,
            Self::ArtifactDerived => EvidenceScope::ArtifactBound,
            Self::ImportedCompatibility => EvidenceScope::TransitionCompatible,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EvidenceScope {
    CompileInvariant,
    TransitionCompatible,
    SnapshotLocal,
    ArtifactBound,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EvidenceClass {
    Distance,
    Support,
    Differential,
    Identity,
    Temporal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EvidenceRefinementKind {
    WarpWeakening,
    RuntimeBounds,
    RuntimeObservation,
    IdentityOverlay,
    ArtifactBinding,
    ImportedCompatibility,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceRefinementStep {
    pub class: EvidenceClass,
    pub kind: EvidenceRefinementKind,
    pub detail: SmolStr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceProvenance {
    pub origin: EvidenceOrigin,
    pub scope: EvidenceScope,
    pub refinement_path: Vec<EvidenceRefinementStep>,
}

impl EvidenceProvenance {
    pub fn new(origin: EvidenceOrigin, scope: EvidenceScope) -> Self {
        Self {
            origin,
            scope,
            refinement_path: Vec::new(),
        }
    }

    pub fn static_compiled() -> Self {
        Self::new(
            EvidenceOrigin::StaticCompiled,
            EvidenceScope::CompileInvariant,
        )
    }

    pub fn runtime_observed() -> Self {
        Self::new(
            EvidenceOrigin::RuntimeObserved,
            EvidenceScope::SnapshotLocal,
        )
    }

    pub fn with_step(
        &self,
        class: EvidenceClass,
        kind: EvidenceRefinementKind,
        detail: impl Into<SmolStr>,
    ) -> Self {
        let mut out = self.clone();
        out.refinement_path.push(EvidenceRefinementStep {
            class,
            kind,
            detail: detail.into(),
        });
        out
    }

    pub fn retag(
        &self,
        origin: EvidenceOrigin,
        scope: EvidenceScope,
        class: EvidenceClass,
        kind: EvidenceRefinementKind,
        detail: impl Into<SmolStr>,
    ) -> Self {
        let mut out = self.with_step(class, kind, detail);
        out.origin = origin;
        out.scope = scope;
        out
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DistanceEvidence {
    pub semantics: DistanceSemantics,
    pub lipschitz: LipschitzStatus,
    pub interval_bounds: FactAvailability,
    pub analytic_intersection: AnalyticIntersectionStatus,
    pub provenance: EvidenceProvenance,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupportEvidence {
    pub support_class: SupportClass,
    pub semantics: DistanceSemantics,
    pub conservative_bounds: FactAvailability,
    pub lower_bound_pruning: FactAvailability,
    pub can_coarse_prune: bool,
    pub opaque_boundary: bool,
    pub provenance: EvidenceProvenance,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DifferentialEvidence {
    pub derivative: FactAvailability,
    pub primitive: PrimitiveFact,
    pub transform: TransformFact,
    pub repetition: RepetitionFact,
    pub provenance: EvidenceProvenance,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityEvidence {
    pub stable_feature_id: bool,
    pub stable_instance_id: bool,
    pub stable_repeat_id: bool,
    pub provenance: EvidenceProvenance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TemporalStability {
    CompileInvariant,
    TransitionCompatible,
    SnapshotLocal,
    ArtifactBound,
    Unknown,
}

impl TemporalStability {
    pub const fn for_scope(scope: EvidenceScope) -> Self {
        match scope {
            EvidenceScope::CompileInvariant => Self::CompileInvariant,
            EvidenceScope::TransitionCompatible => Self::TransitionCompatible,
            EvidenceScope::SnapshotLocal => Self::SnapshotLocal,
            EvidenceScope::ArtifactBound => Self::ArtifactBound,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TemporalChangeClass {
    Stable,
    CameraMotion,
    ViewportShift,
    TopologyShift,
    IdentityShift,
    HistoryReset,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemporalEvidence {
    pub stability: TemporalStability,
    pub change_class: TemporalChangeClass,
    pub stationary: FactAvailability,
    pub rigid_over_interval: FactAvailability,
    pub topology_stable: FactAvailability,
    pub bounded_velocity: FactAvailability,
    pub provenance: EvidenceProvenance,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticEvidence {
    pub subject: SmolStr,
    pub distance: DistanceEvidence,
    pub support: SupportEvidence,
    pub differential: DifferentialEvidence,
    pub identity: IdentityEvidence,
    pub temporal: TemporalEvidence,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DistanceEvidenceSummary {
    pub semantics: DistanceSemantics,
    pub lipschitz: LipschitzStatus,
    pub interval_bounds: FactAvailability,
    pub analytic_intersection: AnalyticIntersectionStatus,
    pub origin: EvidenceOrigin,
    pub scope: EvidenceScope,
    pub refinement_path: Vec<EvidenceRefinementStep>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupportEvidenceSummary {
    pub support_class: SupportClass,
    pub semantics: DistanceSemantics,
    pub conservative_bounds: FactAvailability,
    pub lower_bound_pruning: FactAvailability,
    pub can_coarse_prune: bool,
    pub opaque_boundary: bool,
    pub origin: EvidenceOrigin,
    pub scope: EvidenceScope,
    pub refinement_path: Vec<EvidenceRefinementStep>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DifferentialEvidenceSummary {
    pub derivative: FactAvailability,
    pub primitive: PrimitiveFact,
    pub transform: TransformFact,
    pub repetition: RepetitionFact,
    pub origin: EvidenceOrigin,
    pub scope: EvidenceScope,
    pub refinement_path: Vec<EvidenceRefinementStep>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityEvidenceSummary {
    pub stable_feature_id: bool,
    pub stable_instance_id: bool,
    pub stable_repeat_id: bool,
    pub origin: EvidenceOrigin,
    pub scope: EvidenceScope,
    pub refinement_path: Vec<EvidenceRefinementStep>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemporalEvidenceSummary {
    pub stability: TemporalStability,
    pub change_class: TemporalChangeClass,
    pub stationary: FactAvailability,
    pub rigid_over_interval: FactAvailability,
    pub topology_stable: FactAvailability,
    pub bounded_velocity: FactAvailability,
    pub origin: EvidenceOrigin,
    pub scope: EvidenceScope,
    pub refinement_path: Vec<EvidenceRefinementStep>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticEvidenceSummary {
    pub subject: SmolStr,
    pub distance: DistanceEvidenceSummary,
    pub support: SupportEvidenceSummary,
    pub differential: DifferentialEvidenceSummary,
    pub identity: IdentityEvidenceSummary,
    pub temporal: TemporalEvidenceSummary,
    pub origin: EvidenceOrigin,
    pub scope: EvidenceScope,
    pub refinement_path: Vec<EvidenceRefinementStep>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeBoundsEvidence {
    pub lower_bound_pruning: FactAvailability,
    pub interval_bounds: FactAvailability,
    pub lipschitz: Option<LipschitzStatus>,
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

impl SemanticEvidence {
    pub fn from_summary(summary: &SemanticEvidenceSummary) -> Self {
        Self {
            subject: summary.subject.clone(),
            distance: DistanceEvidence {
                semantics: summary.distance.semantics,
                lipschitz: summary.distance.lipschitz,
                interval_bounds: summary.distance.interval_bounds,
                analytic_intersection: summary.distance.analytic_intersection,
                provenance: EvidenceProvenance {
                    origin: summary.distance.origin,
                    scope: summary.distance.scope,
                    refinement_path: summary.distance.refinement_path.clone(),
                },
            },
            support: SupportEvidence {
                support_class: summary.support.support_class,
                semantics: summary.support.semantics,
                conservative_bounds: summary.support.conservative_bounds,
                lower_bound_pruning: summary.support.lower_bound_pruning,
                can_coarse_prune: summary.support.can_coarse_prune,
                opaque_boundary: summary.support.opaque_boundary,
                provenance: EvidenceProvenance {
                    origin: summary.support.origin,
                    scope: summary.support.scope,
                    refinement_path: summary.support.refinement_path.clone(),
                },
            },
            differential: DifferentialEvidence {
                derivative: summary.differential.derivative,
                primitive: summary.differential.primitive,
                transform: summary.differential.transform,
                repetition: summary.differential.repetition,
                provenance: EvidenceProvenance {
                    origin: summary.differential.origin,
                    scope: summary.differential.scope,
                    refinement_path: summary.differential.refinement_path.clone(),
                },
            },
            identity: IdentityEvidence {
                stable_feature_id: summary.identity.stable_feature_id,
                stable_instance_id: summary.identity.stable_instance_id,
                stable_repeat_id: summary.identity.stable_repeat_id,
                provenance: EvidenceProvenance {
                    origin: summary.identity.origin,
                    scope: summary.identity.scope,
                    refinement_path: summary.identity.refinement_path.clone(),
                },
            },
            temporal: TemporalEvidence {
                stability: summary.temporal.stability,
                change_class: summary.temporal.change_class,
                stationary: summary.temporal.stationary,
                rigid_over_interval: summary.temporal.rigid_over_interval,
                topology_stable: summary.temporal.topology_stable,
                bounded_velocity: summary.temporal.bounded_velocity,
                provenance: EvidenceProvenance {
                    origin: summary.temporal.origin,
                    scope: summary.temporal.scope,
                    refinement_path: summary.temporal.refinement_path.clone(),
                },
            },
        }
    }

    pub fn runtime_unknown(subject: impl Into<SmolStr>) -> Self {
        let subject = subject.into();
        let provenance = EvidenceProvenance::runtime_observed().with_step(
            EvidenceClass::Temporal,
            EvidenceRefinementKind::RuntimeObservation,
            "runtime planner placeholder",
        );
        Self {
            subject,
            distance: DistanceEvidence {
                semantics: DistanceSemantics::ConservativeLowerBound,
                lipschitz: LipschitzStatus::Unknown,
                interval_bounds: FactAvailability::Unknown,
                analytic_intersection: AnalyticIntersectionStatus::Unknown,
                provenance: provenance.clone(),
            },
            support: SupportEvidence {
                support_class: SupportClass::Unknown,
                semantics: DistanceSemantics::ConservativeLowerBound,
                conservative_bounds: FactAvailability::Unknown,
                lower_bound_pruning: FactAvailability::Unknown,
                can_coarse_prune: false,
                opaque_boundary: false,
                provenance: provenance.clone(),
            },
            differential: DifferentialEvidence {
                derivative: FactAvailability::Unknown,
                primitive: PrimitiveFact::Unknown,
                transform: TransformFact::Unknown,
                repetition: RepetitionFact::Unknown,
                provenance: provenance.clone(),
            },
            identity: IdentityEvidence {
                stable_feature_id: false,
                stable_instance_id: false,
                stable_repeat_id: false,
                provenance: provenance.clone(),
            },
            temporal: TemporalEvidence {
                stability: TemporalStability::SnapshotLocal,
                change_class: TemporalChangeClass::Unknown,
                stationary: FactAvailability::Unknown,
                rigid_over_interval: FactAvailability::Unknown,
                topology_stable: FactAvailability::Unknown,
                bounded_velocity: FactAvailability::Unknown,
                provenance,
            },
        }
    }

    pub fn unavailable(subject: impl Into<SmolStr>) -> Self {
        let subject = subject.into();
        let provenance = EvidenceProvenance::static_compiled().with_step(
            EvidenceClass::Support,
            EvidenceRefinementKind::WarpWeakening,
            "opaque or unavailable scene evidence",
        );
        Self {
            subject,
            distance: DistanceEvidence {
                semantics: DistanceSemantics::UnknownOpaque,
                lipschitz: LipschitzStatus::Unavailable,
                interval_bounds: FactAvailability::Unavailable,
                analytic_intersection: AnalyticIntersectionStatus::Unavailable,
                provenance: provenance.clone(),
            },
            support: SupportEvidence {
                support_class: SupportClass::Unknown,
                semantics: DistanceSemantics::UnknownOpaque,
                conservative_bounds: FactAvailability::Unknown,
                lower_bound_pruning: FactAvailability::Unavailable,
                can_coarse_prune: false,
                opaque_boundary: true,
                provenance: provenance.clone(),
            },
            differential: DifferentialEvidence {
                derivative: FactAvailability::Unavailable,
                primitive: PrimitiveFact::Unknown,
                transform: TransformFact::Unknown,
                repetition: RepetitionFact::Unknown,
                provenance: provenance.clone(),
            },
            identity: IdentityEvidence {
                stable_feature_id: false,
                stable_instance_id: false,
                stable_repeat_id: false,
                provenance: provenance.clone(),
            },
            temporal: TemporalEvidence {
                stability: TemporalStability::CompileInvariant,
                change_class: TemporalChangeClass::Unknown,
                stationary: FactAvailability::Unknown,
                rigid_over_interval: FactAvailability::Unknown,
                topology_stable: FactAvailability::Unknown,
                bounded_velocity: FactAvailability::Unknown,
                provenance,
            },
        }
    }

    pub fn for_field_scene(scene: &FieldScene) -> Self {
        let primitive = primitive_fact_for_field(scene);
        let transform = transform_fact_for_field(scene);
        let repetition = repetition_fact_for_field(scene);
        let derivative = derivative_availability_for_scene(scene.analysis.differential_support);
        let lipschitz = lipschitz_for_scene(scene.semantics, transform, repetition);
        let analytic_intersection = analytic_status(primitive, transform, repetition);
        let provenance = EvidenceProvenance::static_compiled();
        Self {
            subject: scene.name.clone(),
            distance: DistanceEvidence {
                semantics: scene.semantics,
                lipschitz,
                interval_bounds: FactAvailability::Unavailable,
                analytic_intersection,
                provenance: provenance.clone(),
            },
            support: support_evidence(
                scene.support_class,
                scene.semantics,
                scene.can_coarse_support_pruning,
                scene.opaque_boundary,
                scene
                    .support_records
                    .iter()
                    .any(|record| supports_bounds(&record.kind)),
                provenance.clone(),
            ),
            differential: DifferentialEvidence {
                derivative,
                primitive,
                transform,
                repetition,
                provenance: provenance.clone(),
            },
            identity: IdentityEvidence {
                stable_feature_id: false,
                stable_instance_id: false,
                stable_repeat_id: !scene.identity_sources.is_empty(),
                provenance: provenance.clone(),
            },
            temporal: TemporalEvidence {
                stability: TemporalStability::CompileInvariant,
                change_class: TemporalChangeClass::Stable,
                stationary: FactAvailability::Available,
                rigid_over_interval: FactAvailability::Available,
                topology_stable: FactAvailability::Available,
                bounded_velocity: FactAvailability::Available,
                provenance,
            },
        }
    }

    pub fn for_shape_scene(scene: &ShapeScene) -> Self {
        let primitive = primitive_fact_for_shape(scene);
        let transform = transform_fact_for_shape(scene);
        let repetition = repetition_fact_for_shape(scene);
        let derivative = derivative_availability_for_scene(scene.analysis.differential_support);
        let lipschitz = lipschitz_for_scene(scene.semantics, transform, repetition);
        let analytic_intersection = analytic_status(primitive, transform, repetition);
        let stable_repeat_id = scene.leaves.values().any(|leaf| {
            matches!(
                leaf.field_semantics,
                DistanceSemantics::ExactSignedDistance | DistanceSemantics::ConservativeLowerBound
            )
        });
        let provenance = EvidenceProvenance::static_compiled();
        Self {
            subject: scene.name.clone(),
            distance: DistanceEvidence {
                semantics: scene.semantics,
                lipschitz,
                interval_bounds: FactAvailability::Unavailable,
                analytic_intersection,
                provenance: provenance.clone(),
            },
            support: support_evidence(
                scene.support_class,
                scene.semantics,
                scene.can_coarse_support_pruning,
                scene.opaque_boundary,
                scene
                    .support_records
                    .iter()
                    .any(|record| supports_bounds(&record.kind)),
                provenance.clone(),
            ),
            differential: DifferentialEvidence {
                derivative,
                primitive,
                transform,
                repetition,
                provenance: provenance.clone(),
            },
            identity: IdentityEvidence {
                stable_feature_id: !scene.leaves.is_empty(),
                stable_instance_id: stable_repeat_id,
                stable_repeat_id,
                provenance: provenance.clone(),
            },
            temporal: TemporalEvidence {
                stability: TemporalStability::CompileInvariant,
                change_class: TemporalChangeClass::Stable,
                stationary: FactAvailability::Available,
                rigid_over_interval: FactAvailability::Available,
                topology_stable: FactAvailability::Available,
                bounded_velocity: FactAvailability::Available,
                provenance,
            },
        }
    }

    pub fn with_subject(&self, subject: impl Into<SmolStr>) -> Self {
        let mut out = self.clone();
        out.subject = subject.into();
        out
    }

    pub fn refine_identity_with(
        &self,
        identity: IdentityEvidence,
        detail: impl Into<SmolStr>,
    ) -> Self {
        let detail = detail.into();
        let mut out = self.clone();
        out.identity = IdentityEvidence {
            stable_feature_id: self.identity.stable_feature_id || identity.stable_feature_id,
            stable_instance_id: self.identity.stable_instance_id || identity.stable_instance_id,
            stable_repeat_id: self.identity.stable_repeat_id || identity.stable_repeat_id,
            provenance: identity.provenance.with_step(
                EvidenceClass::Identity,
                EvidenceRefinementKind::IdentityOverlay,
                detail,
            ),
        };
        out
    }

    pub fn weaken_for_warp(&self, detail: impl Into<SmolStr>) -> Self {
        let detail = detail.into();
        let mut out = self.clone();
        out.distance.semantics = match out.distance.semantics {
            DistanceSemantics::ExactSignedDistance => DistanceSemantics::ConservativeLowerBound,
            other => other,
        };
        out.distance.lipschitz = match out.distance.lipschitz {
            LipschitzStatus::ExactKnown | LipschitzStatus::ConservativeKnown => {
                LipschitzStatus::Unknown
            }
            other => other,
        };
        out.distance.analytic_intersection = AnalyticIntersectionStatus::Unavailable;
        out.distance.provenance = out.distance.provenance.with_step(
            EvidenceClass::Distance,
            EvidenceRefinementKind::WarpWeakening,
            detail.clone(),
        );
        out.support.lower_bound_pruning = FactAvailability::Unknown;
        out.support.can_coarse_prune = false;
        out.support.provenance = out.support.provenance.with_step(
            EvidenceClass::Support,
            EvidenceRefinementKind::WarpWeakening,
            detail.clone(),
        );
        out.differential.derivative = FactAvailability::Unavailable;
        out.differential.transform = TransformFact::AffineOrWarp;
        out.differential.provenance = out.differential.provenance.with_step(
            EvidenceClass::Differential,
            EvidenceRefinementKind::WarpWeakening,
            detail,
        );
        out
    }

    pub fn refine_with_runtime_bounds(
        &self,
        bounds: RuntimeBoundsEvidence,
        detail: impl Into<SmolStr>,
    ) -> Self {
        let detail = detail.into();
        let mut out = self.clone();
        out.support.lower_bound_pruning =
            refine_availability(self.support.lower_bound_pruning, bounds.lower_bound_pruning);
        out.support.provenance = out.support.provenance.retag(
            EvidenceOrigin::RuntimeObserved,
            EvidenceScope::SnapshotLocal,
            EvidenceClass::Support,
            EvidenceRefinementKind::RuntimeBounds,
            detail.clone(),
        );
        out.distance.interval_bounds =
            refine_availability(self.distance.interval_bounds, bounds.interval_bounds);
        if let (LipschitzStatus::Unknown, Some(refined)) =
            (self.distance.lipschitz, bounds.lipschitz)
        {
            out.distance.lipschitz = refined;
        }
        out.distance.provenance = out.distance.provenance.retag(
            EvidenceOrigin::RuntimeObserved,
            EvidenceScope::SnapshotLocal,
            EvidenceClass::Distance,
            EvidenceRefinementKind::RuntimeBounds,
            detail.clone(),
        );
        out.temporal.stability = TemporalStability::SnapshotLocal;
        out.temporal.change_class = self.temporal.change_class;
        out.temporal.provenance = out.temporal.provenance.retag(
            EvidenceOrigin::RuntimeObserved,
            EvidenceScope::SnapshotLocal,
            EvidenceClass::Temporal,
            EvidenceRefinementKind::RuntimeBounds,
            detail,
        );
        out
    }

    pub fn imported_compatibility(&self, detail: impl Into<SmolStr>) -> Self {
        self.retag_all(
            EvidenceOrigin::ImportedCompatibility,
            EvidenceScope::TransitionCompatible,
            EvidenceRefinementKind::ImportedCompatibility,
            detail,
        )
    }

    pub fn refine_with_temporal_stability(
        &self,
        stationary: FactAvailability,
        rigid_over_interval: FactAvailability,
        topology_stable: FactAvailability,
        bounded_velocity: FactAvailability,
        detail: impl Into<SmolStr>,
    ) -> Self {
        let detail = detail.into();
        let mut out = self.clone();
        out.temporal.stability = TemporalStability::TransitionCompatible;
        out.temporal.stationary = refine_availability(self.temporal.stationary, stationary);
        out.temporal.rigid_over_interval =
            refine_availability(self.temporal.rigid_over_interval, rigid_over_interval);
        out.temporal.topology_stable =
            refine_availability(self.temporal.topology_stable, topology_stable);
        out.temporal.bounded_velocity =
            refine_availability(self.temporal.bounded_velocity, bounded_velocity);
        out.temporal.provenance = out.temporal.provenance.retag(
            EvidenceOrigin::RuntimeObserved,
            EvidenceScope::TransitionCompatible,
            EvidenceClass::Temporal,
            EvidenceRefinementKind::RuntimeObservation,
            detail,
        );
        out
    }

    pub fn artifact_bound(&self, detail: impl Into<SmolStr>) -> Self {
        self.retag_all(
            EvidenceOrigin::ArtifactDerived,
            EvidenceScope::ArtifactBound,
            EvidenceRefinementKind::ArtifactBinding,
            detail,
        )
    }

    fn retag_all(
        &self,
        origin: EvidenceOrigin,
        scope: EvidenceScope,
        kind: EvidenceRefinementKind,
        detail: impl Into<SmolStr>,
    ) -> Self {
        let detail = detail.into();
        let mut out = self.clone();
        out.distance.provenance = out.distance.provenance.retag(
            origin,
            scope,
            EvidenceClass::Distance,
            kind,
            detail.clone(),
        );
        out.support.provenance = out.support.provenance.retag(
            origin,
            scope,
            EvidenceClass::Support,
            kind,
            detail.clone(),
        );
        out.differential.provenance = out.differential.provenance.retag(
            origin,
            scope,
            EvidenceClass::Differential,
            kind,
            detail.clone(),
        );
        out.identity.provenance = out.identity.provenance.retag(
            origin,
            scope,
            EvidenceClass::Identity,
            kind,
            detail.clone(),
        );
        out.temporal.stability = TemporalStability::for_scope(scope);
        out.temporal.change_class = self.temporal.change_class;
        out.temporal.provenance =
            out.temporal
                .provenance
                .retag(origin, scope, EvidenceClass::Temporal, kind, detail);
        out
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
        if matches!(self.differential.derivative, FactAvailability::Unavailable) {
            labels.push("derivative");
        }
        if matches!(self.distance.lipschitz, LipschitzStatus::Unavailable) {
            labels.push("lipschitz");
        }
        if matches!(
            self.distance.analytic_intersection,
            AnalyticIntersectionStatus::Unavailable
        ) {
            labels.push("analytic");
        }
        if matches!(self.distance.interval_bounds, FactAvailability::Unavailable) {
            labels.push("interval");
        }
        labels
    }

    pub fn summary(&self) -> SemanticEvidenceSummary {
        SemanticEvidenceSummary {
            subject: self.subject.clone(),
            distance: DistanceEvidenceSummary {
                semantics: self.distance.semantics,
                lipschitz: self.distance.lipschitz,
                interval_bounds: self.distance.interval_bounds,
                analytic_intersection: self.distance.analytic_intersection,
                origin: self.distance.provenance.origin,
                scope: self.distance.provenance.scope,
                refinement_path: self.distance.provenance.refinement_path.clone(),
            },
            support: SupportEvidenceSummary {
                support_class: self.support.support_class,
                semantics: self.support.semantics,
                conservative_bounds: self.support.conservative_bounds,
                lower_bound_pruning: self.support.lower_bound_pruning,
                can_coarse_prune: self.support.can_coarse_prune,
                opaque_boundary: self.support.opaque_boundary,
                origin: self.support.provenance.origin,
                scope: self.support.provenance.scope,
                refinement_path: self.support.provenance.refinement_path.clone(),
            },
            differential: DifferentialEvidenceSummary {
                derivative: self.differential.derivative,
                primitive: self.differential.primitive,
                transform: self.differential.transform,
                repetition: self.differential.repetition,
                origin: self.differential.provenance.origin,
                scope: self.differential.provenance.scope,
                refinement_path: self.differential.provenance.refinement_path.clone(),
            },
            identity: IdentityEvidenceSummary {
                stable_feature_id: self.identity.stable_feature_id,
                stable_instance_id: self.identity.stable_instance_id,
                stable_repeat_id: self.identity.stable_repeat_id,
                origin: self.identity.provenance.origin,
                scope: self.identity.provenance.scope,
                refinement_path: self.identity.provenance.refinement_path.clone(),
            },
            temporal: TemporalEvidenceSummary {
                stability: self.temporal.stability,
                change_class: self.temporal.change_class,
                stationary: self.temporal.stationary,
                rigid_over_interval: self.temporal.rigid_over_interval,
                topology_stable: self.temporal.topology_stable,
                bounded_velocity: self.temporal.bounded_velocity,
                origin: self.temporal.provenance.origin,
                scope: self.temporal.provenance.scope,
                refinement_path: self.temporal.provenance.refinement_path.clone(),
            },
            origin: self.aggregate_origin(),
            scope: self.aggregate_scope(),
            refinement_path: self.refinement_path(),
        }
    }

    pub fn aggregate_origin(&self) -> EvidenceOrigin {
        [
            self.distance.provenance.origin,
            self.support.provenance.origin,
            self.differential.provenance.origin,
            self.identity.provenance.origin,
            self.temporal.provenance.origin,
        ]
        .into_iter()
        .max()
        .unwrap_or(EvidenceOrigin::StaticCompiled)
    }

    pub fn aggregate_scope(&self) -> EvidenceScope {
        [
            self.distance.provenance.scope,
            self.support.provenance.scope,
            self.differential.provenance.scope,
            self.identity.provenance.scope,
            self.temporal.provenance.scope,
        ]
        .into_iter()
        .max()
        .unwrap_or(EvidenceScope::CompileInvariant)
    }

    pub fn refinement_path(&self) -> Vec<EvidenceRefinementStep> {
        let mut aggregated = Vec::new();
        for step in [
            &self.distance.provenance.refinement_path,
            &self.support.provenance.refinement_path,
            &self.differential.provenance.refinement_path,
            &self.identity.provenance.refinement_path,
            &self.temporal.provenance.refinement_path,
        ]
        .into_iter()
        .flat_map(|steps| steps.iter())
        {
            if aggregated.iter().any(|existing: &EvidenceRefinementStep| {
                existing.kind == step.kind && existing.detail == step.detail
            }) {
                continue;
            }
            aggregated.push(step.clone());
        }
        aggregated
    }
}

impl SemanticEvidenceSummary {
    pub fn runtime_unknown(subject: impl Into<SmolStr>) -> Self {
        SemanticEvidence::runtime_unknown(subject).summary()
    }

    pub fn contract_bound() -> Self {
        SemanticEvidence::runtime_unknown("contract.bound")
            .imported_compatibility("contract-bound placeholder")
            .summary()
    }

    pub fn compiled_scene(class_detail: impl Into<SmolStr>, opaque_boundary: bool) -> Self {
        let detail = class_detail.into();
        let evidence = if opaque_boundary {
            SemanticEvidence::unavailable(detail.clone())
        } else {
            SemanticEvidence::runtime_unknown(detail.clone()).retag_all(
                EvidenceOrigin::StaticCompiled,
                EvidenceScope::CompileInvariant,
                EvidenceRefinementKind::RuntimeObservation,
                "scene-derived summary",
            )
        };
        evidence.summary()
    }

    pub fn artifact_bound(opaque_boundary: bool) -> Self {
        let evidence = if opaque_boundary {
            SemanticEvidence::unavailable("artifact.bound")
        } else {
            SemanticEvidence::runtime_unknown("artifact.bound")
        }
        .artifact_bound("artifact-bound summary");
        evidence.summary()
    }

    pub fn with_artifact_binding(&self, detail: impl Into<SmolStr>) -> Self {
        SemanticEvidence::from_summary(self)
            .artifact_bound(detail)
            .summary()
    }
}

fn support_evidence(
    support_class: SupportClass,
    semantics: DistanceSemantics,
    can_coarse_prune: bool,
    opaque_boundary: bool,
    has_bounds: bool,
    provenance: EvidenceProvenance,
) -> SupportEvidence {
    SupportEvidence {
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
        provenance,
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

fn derivative_availability_for_scene(support: SceneDifferentialSupport) -> FactAvailability {
    match support {
        SceneDifferentialSupport::CertifiedGradient => FactAvailability::Available,
        SceneDifferentialSupport::FiniteDifferenceFallback => FactAvailability::Unavailable,
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

fn refine_availability(current: FactAvailability, observed: FactAvailability) -> FactAvailability {
    match current {
        FactAvailability::Available | FactAvailability::Unavailable => current,
        FactAvailability::Unknown => observed,
    }
}
