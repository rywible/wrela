use self::DispatchBackend::{Auto, VirtualGpu, Wgsl};
use crate::artifact_key::{ArtifactPolicyDigestMode, ArtifactReuseKey};
use crate::query_contract::{
    self, ParticipantContractKind, QueryContractDescriptor, QueryExecutionBinding,
    QueryObservabilityProfile, QueryQuestionId,
};
use crate::query_solver::{RaySolverPlan, is_ray_shaped_spatial_contract};
use crate::scene_ir::{DistanceSemantics, SupportClass};
use crate::semantic_evidence::{
    EvidenceOrigin, EvidenceRefinementKind as SemanticEvidenceRefinementKind,
    EvidenceRefinementStep, EvidenceScope, FactAvailability,
};
use crate::world_identity::WorldSnapshotHandle;
use smol_str::SmolStr;

pub use crate::query_contract::{
    CaptureKind, DispatchBackend, InternalKernelKind, PlanExecutor,
    QUERY_CONTRACT_VERSION as QUERY_PLAN_CONTRACT_VERSION, QueryCardinality, QueryContractId,
    QueryFamilyId, QueryItemKind, QueryResultKind, QuerySurfaceKind, QueryTargetKind,
    SceneDomainFlag,
};
pub use crate::semantic_evidence::{
    EvidenceOrigin as SemanticEvidenceOrigin, EvidenceRefinementKind,
    EvidenceRefinementStep as SemanticEvidenceRefinementStep,
    EvidenceScope as SemanticEvidenceScope, SemanticEvidenceSummary,
};

pub fn semantic_evidence_origin_name(origin: EvidenceOrigin) -> &'static str {
    match origin {
        EvidenceOrigin::StaticCompiled => "static-compiled",
        EvidenceOrigin::RuntimeObserved => "runtime-observed",
        EvidenceOrigin::ArtifactDerived => "artifact-derived",
        EvidenceOrigin::ImportedCompatibility => "imported-compatibility",
    }
}

pub fn semantic_evidence_scope_name(scope: EvidenceScope) -> &'static str {
    match scope {
        EvidenceScope::CompileInvariant => "compile-invariant",
        EvidenceScope::TransitionCompatible => "transition-compatible",
        EvidenceScope::SnapshotLocal => "snapshot-local",
        EvidenceScope::ArtifactBound => "artifact-bound",
    }
}

pub fn semantic_evidence_refinement_step_name(step: &EvidenceRefinementStep) -> &'static str {
    match step.kind {
        SemanticEvidenceRefinementKind::WarpWeakening => "warp-weakening",
        SemanticEvidenceRefinementKind::RuntimeBounds => "runtime-bounds",
        SemanticEvidenceRefinementKind::RuntimeObservation => "runtime-observation",
        SemanticEvidenceRefinementKind::IdentityOverlay => "identity-overlay",
        SemanticEvidenceRefinementKind::ArtifactBinding => "artifact-binding",
        SemanticEvidenceRefinementKind::ImportedCompatibility => "imported-compatibility",
    }
}

pub fn semantic_evidence_summary_from_evidence(
    evidence: &crate::semantic_evidence::SemanticEvidence,
) -> SemanticEvidenceSummary {
    evidence.summary()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BatchQueryKind {
    Distance,
    Normal,
    Nearest,
    Trace,
    Surface,
    Occluded,
    Radiance,
    Medium,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FieldBatchPlanKind {
    Distance,
    Normal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CaptureQueryKind {
    Distance,
    Normal,
    SupportSummary,
    Radiance,
    Medium,
    Nearest,
    Trace,
    Surface,
    Occluded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ShapeBatchPlanKind {
    Nearest,
    Trace,
    Surface,
    Occluded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum WorldQueryKind {
    Distance,
    Normal,
    SupportSummary,
    Radiance,
    Medium,
    Nearest,
    Trace,
    Surface,
    Occluded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CandidateStrategy {
    DirectFieldCapture,
    SemanticSupportSummary,
    ShapeBranchTraversal,
    SupportAcceleratedShapeTraversal,
    SurfaceHitReuse,
    OpaqueFallback,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PruningStrategy {
    None,
    ConservativeTraversal,
    SupportLowerBound,
    CullingTable,
    OpaquePessimizationBoundary,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DerivedArtifact {
    SupportSummary {
        semantics: DistanceSemantics,
        support_class: SupportClass,
        can_coarse_support_pruning: bool,
    },
    CaptureCache {
        capture_kind: CaptureKind,
    },
    CullingTable {
        candidate_strategy: CandidateStrategy,
        pruning_strategy: PruningStrategy,
    },
    OpaquePessimizationBoundary,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanStage {
    SelectBackend,
    LoadCapture,
    BeginVirtualGpuDispatch,
    LoadDerivedArtifact { artifact: DerivedArtifact },
    IterateItems { item_kind: QueryItemKind },
    GenerateCandidates { strategy: CandidateStrategy },
    PruneCandidates { strategy: PruningStrategy },
    LoadDomainFlags,
    SelectParticipants { kind: CaptureQueryKind },
    Execute { executor: PlanExecutor },
    AssembleHitContext,
    AppendResult { result_kind: QueryResultKind },
    EndVirtualGpuDispatch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CandidateSource {
    CaptureScene,
    WorldRegionShapes,
    SurfaceHit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum WinnerSelectionMode {
    None,
    Nearest,
    Ordered,
    SurfaceReuse,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateRecordContract {
    pub source: CandidateSource,
    pub item_kind: QueryItemKind,
    pub candidate_strategy: CandidateStrategy,
    pub pruning_strategy: PruningStrategy,
    pub winner_mode: WinnerSelectionMode,
    pub stable_leaf_identity: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultRecordContract {
    pub result_kind: QueryResultKind,
    pub preserves_local_hit_context: bool,
    pub stable_feature_id: bool,
    pub stable_instance_id: bool,
    pub stable_repeat_id: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HitContextContract {
    pub world_position: bool,
    pub world_normal: bool,
    pub local_position: bool,
    pub local_normal: bool,
    pub shading_frame: bool,
    pub payload: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParticipantSelectionContract {
    pub kind: CaptureQueryKind,
    pub provenance_aware: bool,
    pub additive: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DispatchRecordContract {
    pub backend: DispatchBackend,
    pub kernel: InternalKernelKind,
    pub item_kind: QueryItemKind,
    pub result_kind: QueryResultKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtifactSchema {
    SupportSummary {
        semantics: DistanceSemantics,
        support_class: SupportClass,
        can_coarse_support_pruning: bool,
        semantic_root: u32,
        support_root: u32,
        node_count: u32,
        support_node_count: u32,
        leaf_count: u32,
        identity_source_count: u32,
    },
    CaptureCache {
        capture_kind: CaptureKind,
        semantic_root: u32,
    },
    CullingTable {
        candidate_strategy: CandidateStrategy,
        pruning_strategy: PruningStrategy,
        support_class: SupportClass,
        semantics: DistanceSemantics,
        support_root: u32,
        support_node_count: u32,
        leaf_count: u32,
        identity_source_count: u32,
    },
    DispatchRecord {
        item_kind: QueryItemKind,
        result_kind: QueryResultKind,
    },
    HitResultBuffer {
        result_kind: QueryResultKind,
        preserves_local_hit_context: bool,
    },
    OpaquePessimizationBoundary {
        support_root: u32,
        support_node_count: u32,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactContract {
    pub id: SmolStr,
    pub schema: ArtifactSchema,
    pub producer: SmolStr,
    pub consumer: SmolStr,
    pub deterministic: bool,
    pub version: u32,
    pub evidence_summary: SemanticEvidenceSummary,
}

impl ArtifactContract {
    pub fn logical_artifact_schema(&self) -> SmolStr {
        SmolStr::new(format!("query-artifact::{}", self.id))
    }

    pub fn compatibility_hash(&self) -> u64 {
        let schema = format!("{:?}", self.schema);
        let evidence_summary = format!("{:?}", self.evidence_summary);
        crate::query_exec::ids::stable_semantic_id(&[
            self.id.as_bytes(),
            schema.as_bytes(),
            self.producer.as_bytes(),
            self.consumer.as_bytes(),
            evidence_summary.as_bytes(),
            &self.version.to_le_bytes(),
        ])
    }

    pub fn reuse_key(
        &self,
        snapshot: &WorldSnapshotHandle,
        policy_digest: Option<u64>,
        policy_mode: ArtifactPolicyDigestMode,
    ) -> ArtifactReuseKey {
        ArtifactReuseKey::new(
            snapshot,
            Some(self.id.clone()),
            self.logical_artifact_schema(),
            self.compatibility_hash(),
            policy_digest,
            policy_mode,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanningObservability {
    pub candidate_count: bool,
    pub branch_visits: bool,
    pub support_prune_effectiveness: bool,
    pub culling_hit_rate: bool,
    pub trace_steps: bool,
    pub field_samples: bool,
    pub artifact_sizes: bool,
    pub dispatch_overhead: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BatchItemContract {
    CaptureQuery { plan: CaptureQueryPlan },
    RayThenOcclusion { nearest_plan: CaptureQueryPlan },
    WorldQuery { plan: WorldQueryPlan },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneSummary {
    pub name: Option<SmolStr>,
    pub semantics: DistanceSemantics,
    pub support_class: SupportClass,
    pub can_coarse_support_pruning: bool,
    pub opaque_boundary: bool,
    pub evidence_summary: SemanticEvidenceSummary,
    pub semantic_root: u32,
    pub support_root: u32,
    pub node_count: u32,
    pub support_node_count: u32,
    pub leaf_count: u32,
    pub identity_source_count: u32,
}

impl Default for SceneSummary {
    fn default() -> Self {
        Self {
            name: None,
            semantics: DistanceSemantics::ConservativeLowerBound,
            support_class: SupportClass::Unknown,
            can_coarse_support_pruning: false,
            opaque_boundary: false,
            evidence_summary: SemanticEvidenceSummary::contract_bound(),
            semantic_root: 0,
            support_root: 0,
            node_count: 0,
            support_node_count: 0,
            leaf_count: 0,
            identity_source_count: 0,
        }
    }
}

impl SceneSummary {
    fn effective_evidence_summary(&self) -> SemanticEvidenceSummary {
        let mut summary = self.evidence_summary.clone();
        let has_bounds = matches!(
            self.support_class,
            SupportClass::Bounded | SupportClass::Periodic
        );
        summary.distance.semantics = self.semantics;
        summary.support.support_class = self.support_class;
        summary.support.semantics = self.semantics;
        summary.support.can_coarse_prune = self.can_coarse_support_pruning;
        summary.support.opaque_boundary = self.opaque_boundary;
        summary.support.conservative_bounds = if has_bounds {
            FactAvailability::Available
        } else if matches!(
            self.support_class,
            SupportClass::Unknown | SupportClass::Unbounded
        ) {
            FactAvailability::Unknown
        } else {
            FactAvailability::Unavailable
        };
        summary.support.lower_bound_pruning =
            if self.can_coarse_support_pruning && has_bounds && !self.opaque_boundary {
                FactAvailability::Available
            } else if self.opaque_boundary {
                FactAvailability::Unavailable
            } else {
                FactAvailability::Unknown
            };
        summary
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchQueryPlan {
    pub contract_version: u32,
    pub contract_id: QueryContractId,
    pub family: QueryFamilyId,
    pub target: QueryTargetKind,
    pub cardinality: QueryCardinality,
    pub surface: QuerySurfaceKind,
    pub helper_name: SmolStr,
    pub kind: BatchQueryKind,
    pub capture_kind: CaptureKind,
    pub backend: DispatchBackend,
    pub kernel: InternalKernelKind,
    pub item_kind: QueryItemKind,
    pub result_kind: QueryResultKind,
    pub executor: PlanExecutor,
    pub scene: Option<SceneSummary>,
    pub evidence_summary: SemanticEvidenceSummary,
    pub candidate_strategy: CandidateStrategy,
    pub pruning_strategy: PruningStrategy,
    pub stages: Vec<PlanStage>,
    pub derived_artifacts: Vec<DerivedArtifact>,
    pub dispatch_contract: DispatchRecordContract,
    pub candidate_contract: CandidateRecordContract,
    pub result_contract: ResultRecordContract,
    pub hit_context_contract: Option<HitContextContract>,
    pub participant_contract: Option<ParticipantSelectionContract>,
    pub domain_flags: Vec<SceneDomainFlag>,
    pub artifact_contracts: Vec<ArtifactContract>,
    pub item_contract: BatchItemContract,
    pub ray_solver: Option<RaySolverPlan>,
    pub observability: PlanningObservability,
    pub preserves_local_hit_context: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureQueryPlan {
    pub contract_version: u32,
    pub contract_id: QueryContractId,
    pub family: QueryFamilyId,
    pub target: QueryTargetKind,
    pub cardinality: QueryCardinality,
    pub surface: QuerySurfaceKind,
    pub helper_name: SmolStr,
    pub kind: CaptureQueryKind,
    pub capture_kind: CaptureKind,
    pub result_kind: QueryResultKind,
    pub executor: PlanExecutor,
    pub scene: Option<SceneSummary>,
    pub evidence_summary: SemanticEvidenceSummary,
    pub candidate_strategy: CandidateStrategy,
    pub pruning_strategy: PruningStrategy,
    pub stages: Vec<PlanStage>,
    pub derived_artifacts: Vec<DerivedArtifact>,
    pub candidate_contract: CandidateRecordContract,
    pub result_contract: ResultRecordContract,
    pub hit_context_contract: Option<HitContextContract>,
    pub participant_contract: Option<ParticipantSelectionContract>,
    pub artifact_contracts: Vec<ArtifactContract>,
    pub observability: PlanningObservability,
    pub preserves_local_hit_context: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldQueryPlan {
    pub contract_version: u32,
    pub contract_id: QueryContractId,
    pub family: QueryFamilyId,
    pub target: QueryTargetKind,
    pub cardinality: QueryCardinality,
    pub surface: QuerySurfaceKind,
    pub helper_name: SmolStr,
    pub kind: WorldQueryKind,
    pub backend: DispatchBackend,
    pub result_kind: QueryResultKind,
    pub executor: PlanExecutor,
    pub evidence_summary: SemanticEvidenceSummary,
    pub candidate_strategy: CandidateStrategy,
    pub pruning_strategy: PruningStrategy,
    pub stages: Vec<PlanStage>,
    pub derived_artifacts: Vec<DerivedArtifact>,
    pub dispatch_contract: DispatchRecordContract,
    pub candidate_contract: CandidateRecordContract,
    pub result_contract: ResultRecordContract,
    pub hit_context_contract: Option<HitContextContract>,
    pub participant_contract: Option<ParticipantSelectionContract>,
    pub domain_flags: Vec<SceneDomainFlag>,
    pub artifact_contracts: Vec<ArtifactContract>,
    pub ray_solver: Option<RaySolverPlan>,
    pub observability: PlanningObservability,
    pub preserves_local_hit_context: bool,
}

pub(crate) fn batch_query_contract_id(
    kind: BatchQueryKind,
    capture_kind: CaptureKind,
) -> Option<QueryContractId> {
    match (kind, capture_kind) {
        (BatchQueryKind::Distance, CaptureKind::Field) => {
            Some(query_contract::SPATIAL_DISTANCE_BATCH_FIELD)
        }
        (BatchQueryKind::Distance, CaptureKind::Shape) => {
            Some(query_contract::SPATIAL_DISTANCE_BATCH_SHAPE)
        }
        (BatchQueryKind::Distance, CaptureKind::Region) => {
            Some(query_contract::SPATIAL_DISTANCE_BATCH_WORLD)
        }
        (BatchQueryKind::Normal, CaptureKind::Field) => {
            Some(query_contract::SPATIAL_NORMAL_BATCH_FIELD)
        }
        (BatchQueryKind::Normal, CaptureKind::Shape) => {
            Some(query_contract::SPATIAL_NORMAL_BATCH_SHAPE)
        }
        (BatchQueryKind::Normal, CaptureKind::Region) => {
            Some(query_contract::SPATIAL_NORMAL_BATCH_WORLD)
        }
        (BatchQueryKind::Nearest | BatchQueryKind::Trace, CaptureKind::Shape) => {
            Some(query_contract::SPATIAL_NEAREST_BATCH_SHAPE)
        }
        (BatchQueryKind::Nearest | BatchQueryKind::Trace, CaptureKind::Region) => {
            Some(query_contract::SPATIAL_NEAREST_BATCH_WORLD)
        }
        (BatchQueryKind::Surface, CaptureKind::Shape) => {
            Some(query_contract::SURFACE_SAMPLE_BATCH_SHAPE)
        }
        (BatchQueryKind::Surface, CaptureKind::Region) => {
            Some(query_contract::SURFACE_SAMPLE_BATCH_WORLD)
        }
        (BatchQueryKind::Occluded, CaptureKind::Shape) => {
            Some(query_contract::SPATIAL_OCCLUDED_BATCH_SHAPE)
        }
        (BatchQueryKind::Occluded, CaptureKind::Region) => {
            Some(query_contract::SPATIAL_OCCLUDED_BATCH_WORLD)
        }
        (BatchQueryKind::Radiance, CaptureKind::Region) => {
            Some(query_contract::PARTICIPANTS_RADIANCE_BATCH_WORLD)
        }
        (BatchQueryKind::Medium, CaptureKind::Region) => {
            Some(query_contract::PARTICIPANTS_MEDIUM_BATCH_WORLD)
        }
        _ => None,
    }
}

pub(crate) fn capture_query_contract_id(
    kind: CaptureQueryKind,
    capture_kind: CaptureKind,
) -> Option<QueryContractId> {
    match (kind, capture_kind) {
        (CaptureQueryKind::Distance, CaptureKind::Field) => {
            Some(query_contract::SPATIAL_DISTANCE_CAPTURE_FIELD)
        }
        (CaptureQueryKind::Distance, CaptureKind::Shape) => {
            Some(query_contract::SPATIAL_DISTANCE_CAPTURE_SHAPE)
        }
        (CaptureQueryKind::Normal, CaptureKind::Field) => {
            Some(query_contract::SPATIAL_NORMAL_CAPTURE_FIELD)
        }
        (CaptureQueryKind::Normal, CaptureKind::Shape) => {
            Some(query_contract::SPATIAL_NORMAL_CAPTURE_SHAPE)
        }
        (CaptureQueryKind::SupportSummary, CaptureKind::Field) => {
            Some(query_contract::SUPPORT_SUMMARY_CAPTURE_FIELD)
        }
        (CaptureQueryKind::SupportSummary, CaptureKind::Shape) => {
            Some(query_contract::SUPPORT_SUMMARY_CAPTURE_SHAPE)
        }
        (CaptureQueryKind::Radiance, CaptureKind::Shape) => {
            Some(query_contract::PARTICIPANTS_RADIANCE_CAPTURE_SHAPE)
        }
        (CaptureQueryKind::Medium, CaptureKind::Shape) => {
            Some(query_contract::PARTICIPANTS_MEDIUM_CAPTURE_SHAPE)
        }
        (CaptureQueryKind::Nearest | CaptureQueryKind::Trace, CaptureKind::Shape) => {
            Some(query_contract::SPATIAL_NEAREST_CAPTURE_SHAPE)
        }
        (CaptureQueryKind::Occluded, CaptureKind::Shape) => {
            Some(query_contract::SPATIAL_OCCLUDED_CAPTURE_SHAPE)
        }
        (CaptureQueryKind::Surface, CaptureKind::Shape) => {
            Some(query_contract::SURFACE_SAMPLE_CAPTURE_SHAPE)
        }
        _ => None,
    }
}

pub(crate) fn world_query_contract_id(kind: WorldQueryKind) -> QueryContractId {
    match kind {
        WorldQueryKind::Distance => query_contract::SPATIAL_DISTANCE_WORLD,
        WorldQueryKind::Normal => query_contract::SPATIAL_NORMAL_WORLD,
        WorldQueryKind::SupportSummary => query_contract::SUPPORT_SUMMARY_WORLD,
        WorldQueryKind::Radiance => query_contract::PARTICIPANTS_RADIANCE_WORLD,
        WorldQueryKind::Medium => query_contract::PARTICIPANTS_MEDIUM_WORLD,
        WorldQueryKind::Nearest | WorldQueryKind::Trace => query_contract::SPATIAL_NEAREST_WORLD,
        WorldQueryKind::Surface => query_contract::SURFACE_SAMPLE_WORLD,
        WorldQueryKind::Occluded => query_contract::SPATIAL_OCCLUDED_WORLD,
    }
}

fn world_scalar_contract_for_batch_descriptor(
    descriptor: &QueryContractDescriptor,
) -> Option<QueryContractId> {
    let kind = match (descriptor.family, descriptor.question) {
        (QueryFamilyId::Spatial, QueryQuestionId::Distance) => WorldQueryKind::Distance,
        (QueryFamilyId::Spatial, QueryQuestionId::Normal) => WorldQueryKind::Normal,
        (QueryFamilyId::Spatial, QueryQuestionId::Nearest) => WorldQueryKind::Nearest,
        (QueryFamilyId::Spatial, QueryQuestionId::Occluded) => WorldQueryKind::Occluded,
        (QueryFamilyId::Surface, QueryQuestionId::Sample) => WorldQueryKind::Surface,
        (QueryFamilyId::Participants, QueryQuestionId::Radiance) => WorldQueryKind::Radiance,
        (QueryFamilyId::Participants, QueryQuestionId::Medium) => WorldQueryKind::Medium,
        _ => return None,
    };
    Some(world_query_contract_id(kind))
}

fn helper_name(binding: &QueryExecutionBinding) -> &'static str {
    binding.helper_name.unwrap_or_else(|| {
        panic!(
            "query contract '{}' is missing a helper binding",
            binding.contract_id.as_str()
        )
    })
}

fn bound_kernel(binding: &QueryExecutionBinding) -> InternalKernelKind {
    binding.default_kernel.unwrap_or_else(|| {
        panic!(
            "query contract '{}' is missing a default kernel binding",
            binding.contract_id.as_str()
        )
    })
}

fn participant_selection_kind(kind: ParticipantContractKind) -> CaptureQueryKind {
    match kind {
        ParticipantContractKind::Radiance => CaptureQueryKind::Radiance,
        ParticipantContractKind::Medium => CaptureQueryKind::Medium,
    }
}

pub(crate) fn batch_query_kind_for_descriptor(
    descriptor: &QueryContractDescriptor,
) -> Option<BatchQueryKind> {
    if descriptor.cardinality != QueryCardinality::Batch {
        return None;
    }
    match (descriptor.family, descriptor.question) {
        (QueryFamilyId::Spatial, QueryQuestionId::Distance) => Some(BatchQueryKind::Distance),
        (QueryFamilyId::Spatial, QueryQuestionId::Normal) => Some(BatchQueryKind::Normal),
        (QueryFamilyId::Spatial, QueryQuestionId::Nearest) => Some(BatchQueryKind::Nearest),
        (QueryFamilyId::Spatial, QueryQuestionId::Occluded) => Some(BatchQueryKind::Occluded),
        (QueryFamilyId::Surface, QueryQuestionId::Sample) => Some(BatchQueryKind::Surface),
        (QueryFamilyId::Participants, QueryQuestionId::Radiance) => Some(BatchQueryKind::Radiance),
        (QueryFamilyId::Participants, QueryQuestionId::Medium) => Some(BatchQueryKind::Medium),
        _ => None,
    }
}

pub(crate) fn batch_query_kind_for_contract_id(
    contract_id: QueryContractId,
) -> Option<BatchQueryKind> {
    query_contract::query_contract(contract_id).and_then(batch_query_kind_for_descriptor)
}

pub(crate) fn capture_query_kind_for_descriptor(
    descriptor: &QueryContractDescriptor,
) -> Option<CaptureQueryKind> {
    if descriptor.target != QueryTargetKind::Capture
        || descriptor.cardinality != QueryCardinality::Scalar
    {
        return None;
    }
    match (descriptor.family, descriptor.question) {
        (QueryFamilyId::Spatial, QueryQuestionId::Distance) => Some(CaptureQueryKind::Distance),
        (QueryFamilyId::Spatial, QueryQuestionId::Normal) => Some(CaptureQueryKind::Normal),
        (QueryFamilyId::Spatial, QueryQuestionId::Nearest) => Some(CaptureQueryKind::Nearest),
        (QueryFamilyId::Spatial, QueryQuestionId::Occluded) => Some(CaptureQueryKind::Occluded),
        (QueryFamilyId::Surface, QueryQuestionId::Sample) => Some(CaptureQueryKind::Surface),
        (QueryFamilyId::Participants, QueryQuestionId::Radiance) => {
            Some(CaptureQueryKind::Radiance)
        }
        (QueryFamilyId::Participants, QueryQuestionId::Medium) => Some(CaptureQueryKind::Medium),
        (QueryFamilyId::Support, QueryQuestionId::Summary) => {
            Some(CaptureQueryKind::SupportSummary)
        }
        _ => None,
    }
}

pub(crate) fn capture_query_kind_for_contract_id(
    contract_id: QueryContractId,
) -> Option<CaptureQueryKind> {
    query_contract::query_contract(contract_id).and_then(capture_query_kind_for_descriptor)
}

pub(crate) fn world_query_kind_for_descriptor(
    descriptor: &QueryContractDescriptor,
) -> Option<WorldQueryKind> {
    if descriptor.target != QueryTargetKind::World
        || descriptor.cardinality != QueryCardinality::Scalar
    {
        return None;
    }
    match (descriptor.family, descriptor.question) {
        (QueryFamilyId::Spatial, QueryQuestionId::Distance) => Some(WorldQueryKind::Distance),
        (QueryFamilyId::Spatial, QueryQuestionId::Normal) => Some(WorldQueryKind::Normal),
        (QueryFamilyId::Spatial, QueryQuestionId::Nearest) => Some(WorldQueryKind::Nearest),
        (QueryFamilyId::Spatial, QueryQuestionId::Occluded) => Some(WorldQueryKind::Occluded),
        (QueryFamilyId::Surface, QueryQuestionId::Sample) => Some(WorldQueryKind::Surface),
        (QueryFamilyId::Participants, QueryQuestionId::Radiance) => Some(WorldQueryKind::Radiance),
        (QueryFamilyId::Participants, QueryQuestionId::Medium) => Some(WorldQueryKind::Medium),
        (QueryFamilyId::Support, QueryQuestionId::Summary) => Some(WorldQueryKind::SupportSummary),
        _ => None,
    }
}

pub(crate) fn world_query_kind_for_contract_id(
    contract_id: QueryContractId,
) -> Option<WorldQueryKind> {
    query_contract::query_contract(contract_id).and_then(world_query_kind_for_descriptor)
}

fn batch_item_contract_for_descriptor(
    descriptor: &QueryContractDescriptor,
    scene: Option<SceneSummary>,
) -> Result<BatchItemContract, &'static str> {
    if descriptor.target == QueryTargetKind::World {
        let Some(contract_id) = world_scalar_contract_for_batch_descriptor(descriptor) else {
            return Err("missing world scalar contract for world-batch item");
        };
        return Ok(BatchItemContract::WorldQuery {
            plan: WorldQueryPlan::for_contract_with_backend(contract_id, DispatchBackend::Auto)?,
        });
    }

    if descriptor.result_kind == QueryResultKind::OcclusionResult {
        let Some(trace_contract) = query_contract::query_contracts().iter().find(|candidate| {
            candidate.surface == QuerySurfaceKind::CaptureScalar
                && candidate.capture_kind == descriptor.capture_kind
                && candidate.question == query_contract::QueryQuestionId::Nearest
                && candidate.item_kind == descriptor.item_kind
                && candidate.result_kind == QueryResultKind::Hit3
        }) else {
            return Err("missing nearest capture contract for occlusion batch");
        };
        return Ok(BatchItemContract::RayThenOcclusion {
            nearest_plan: CaptureQueryPlan::for_contract(trace_contract.id, scene)?,
        });
    }

    let Some(capture_contract) = query_contract::query_contracts().iter().find(|candidate| {
        candidate.surface == QuerySurfaceKind::CaptureScalar
            && candidate.family == descriptor.family
            && candidate.question == descriptor.question
            && candidate.capture_kind == descriptor.capture_kind
            && candidate.item_kind == descriptor.item_kind
            && candidate.result_kind == descriptor.result_kind
    }) else {
        return Err("missing capture scalar contract for batch item");
    };
    Ok(BatchItemContract::CaptureQuery {
        plan: CaptureQueryPlan::for_contract(capture_contract.id, scene)?,
    })
}

impl BatchQueryPlan {
    pub fn new<K>(capture_kind: CaptureKind, kind: K) -> Self
    where
        K: Into<BatchQueryKind>,
    {
        let kind = kind.into();
        match kind {
            BatchQueryKind::Distance | BatchQueryKind::Normal => {
                if matches!(capture_kind, CaptureKind::Region) {
                    Self::for_world_query(kind, DispatchBackend::Auto)
                } else {
                    Self::for_field_query(kind, capture_kind, DispatchBackend::Auto, None)
                }
            }
            BatchQueryKind::Nearest
            | BatchQueryKind::Trace
            | BatchQueryKind::Surface
            | BatchQueryKind::Occluded => {
                if matches!(capture_kind, CaptureKind::Region) {
                    Self::for_world_query(kind, DispatchBackend::Auto)
                } else {
                    Self::for_shape_query(kind, DispatchBackend::Auto, None)
                }
            }
            BatchQueryKind::Radiance | BatchQueryKind::Medium => {
                Self::for_world_query(kind, DispatchBackend::Auto)
            }
        }
    }

    pub fn for_contract(
        contract_id: QueryContractId,
        backend: DispatchBackend,
        scene: Option<SceneSummary>,
    ) -> Result<Self, &'static str> {
        let Some((descriptor, binding)) = query_contract::query_contract_bundle(contract_id) else {
            return Err("missing query contract bundle");
        };
        Self::for_descriptor(descriptor, binding, backend, scene)
    }

    pub(crate) fn for_descriptor(
        descriptor: &'static QueryContractDescriptor,
        binding: &'static QueryExecutionBinding,
        backend: DispatchBackend,
        scene: Option<SceneSummary>,
    ) -> Result<Self, &'static str> {
        if descriptor.id != binding.contract_id {
            return Err("query descriptor and execution binding ids do not match");
        }
        if descriptor.cardinality != QueryCardinality::Batch {
            return Err("batch query plans require batch contracts");
        }
        if descriptor.target == QueryTargetKind::World
            && descriptor.capture_kind != CaptureKind::Region
        {
            return Err("world-batch query plans require region captures");
        }
        if descriptor.target == QueryTargetKind::Capture
            && descriptor.capture_kind == CaptureKind::Region
        {
            return Err("capture-batch query plans do not support region captures");
        }

        let helper_name = helper_name(binding);
        let kernel = bound_kernel(binding);
        let kind = batch_query_kind_for_descriptor(descriptor)
            .ok_or("batch query descriptor does not map to a batch question kind")?;
        let capture_kind = descriptor.capture_kind;
        let item_kind = descriptor.item_kind;
        let result_kind = descriptor.result_kind;
        let executor = binding.default_executor;
        let preserves_local_hit_context = descriptor.preserves_local_hit_context;
        let candidate_strategy = if descriptor.target == QueryTargetKind::World {
            world_candidate_strategy(batch_kind_to_world_kind(kind))
        } else if matches!(
            descriptor.result_kind,
            QueryResultKind::DistanceResult | QueryResultKind::NormalResult
        ) {
            candidate_strategy_for_field_query(capture_kind, scene.as_ref())
        } else {
            candidate_strategy_for_shape_query(kind, scene.as_ref())
        };
        let pruning_strategy = if descriptor.target == QueryTargetKind::World {
            world_pruning_strategy(batch_kind_to_world_kind(kind), candidate_strategy)
        } else {
            pruning_strategy_for_plan(kind, capture_kind, scene.as_ref(), candidate_strategy)
        };
        let derived_artifacts = if descriptor.target == QueryTargetKind::World {
            derive_world_artifacts(candidate_strategy, pruning_strategy)
        } else {
            derive_artifacts(
                scene.as_ref(),
                capture_kind,
                candidate_strategy,
                pruning_strategy,
            )
        };
        let mut stages = vec![PlanStage::SelectBackend];
        if matches!(backend, VirtualGpu | Wgsl | Auto) {
            stages.push(PlanStage::BeginVirtualGpuDispatch);
        }
        stages.push(PlanStage::LoadCapture);
        stages.extend(load_artifact_stages(&derived_artifacts));
        if descriptor.target == QueryTargetKind::World {
            stages.push(PlanStage::LoadDomainFlags);
        }
        stages.push(PlanStage::IterateItems { item_kind });
        stages.push(PlanStage::GenerateCandidates {
            strategy: candidate_strategy,
        });
        stages.push(PlanStage::PruneCandidates {
            strategy: pruning_strategy,
        });
        stages.push(PlanStage::Execute { executor });
        if preserves_local_hit_context {
            stages.push(PlanStage::AssembleHitContext);
        }
        stages.push(PlanStage::AppendResult { result_kind });
        if matches!(backend, VirtualGpu | Wgsl | Auto) {
            stages.push(PlanStage::EndVirtualGpuDispatch);
        }

        let winner_mode = match result_kind {
            QueryResultKind::SupportSummaryResult => WinnerSelectionMode::None,
            QueryResultKind::Surface => WinnerSelectionMode::SurfaceReuse,
            QueryResultKind::Hit3 | QueryResultKind::OcclusionResult => {
                WinnerSelectionMode::Nearest
            }
            QueryResultKind::DistanceResult | QueryResultKind::NormalResult => {
                WinnerSelectionMode::Nearest
            }
            QueryResultKind::RadianceResult | QueryResultKind::MediumResult => {
                WinnerSelectionMode::Ordered
            }
        };
        let candidate_contract = build_candidate_contract(
            if descriptor.target == QueryTargetKind::World {
                CandidateSource::WorldRegionShapes
            } else {
                CandidateSource::CaptureScene
            },
            item_kind,
            candidate_strategy,
            pruning_strategy,
            winner_mode,
            true,
        );
        let result_contract = build_result_contract(result_kind, preserves_local_hit_context);
        let hit_context_contract = preserves_local_hit_context.then(hit_context_contract);
        let evidence_summary = scene
            .as_ref()
            .map(SceneSummary::effective_evidence_summary)
            .unwrap_or_else(|| default_evidence_summary(descriptor.id));
        let artifact_contracts = derive_artifact_contracts(
            &derived_artifacts,
            scene.as_ref(),
            &evidence_summary,
            item_kind,
            result_kind,
            preserves_local_hit_context,
            helper_name,
        );
        let item_contract = batch_item_contract_for_descriptor(descriptor, scene.clone())?;
        let ray_solver = ray_solver_for_descriptor(descriptor, &evidence_summary);

        Ok(Self {
            contract_version: QUERY_PLAN_CONTRACT_VERSION,
            contract_id: descriptor.id,
            family: descriptor.family,
            target: descriptor.target,
            cardinality: descriptor.cardinality,
            surface: descriptor.surface,
            helper_name: SmolStr::new(helper_name),
            kind,
            capture_kind,
            backend,
            kernel,
            item_kind,
            result_kind,
            executor,
            scene,
            evidence_summary,
            candidate_strategy,
            pruning_strategy,
            stages,
            derived_artifacts,
            dispatch_contract: DispatchRecordContract {
                backend,
                kernel,
                item_kind,
                result_kind,
            },
            candidate_contract,
            result_contract,
            hit_context_contract,
            participant_contract: descriptor.participant_kind.map(build_participant_contract),
            domain_flags: descriptor.required_domain_flags.to_vec(),
            artifact_contracts,
            item_contract,
            ray_solver,
            observability: planning_observability(descriptor.observability, pruning_strategy),
            preserves_local_hit_context,
        })
    }

    pub fn for_field_query(
        kind: BatchQueryKind,
        capture_kind: CaptureKind,
        backend: DispatchBackend,
        scene: Option<SceneSummary>,
    ) -> Self {
        let contract_id = batch_query_contract_id(kind, capture_kind).unwrap_or_else(|| {
            panic!("unsupported field batch query: {kind:?} on {capture_kind:?}")
        });
        Self::for_contract(contract_id, backend, scene).expect("field batch contract plan")
    }

    pub fn for_shape_query(
        kind: BatchQueryKind,
        backend: DispatchBackend,
        scene: Option<SceneSummary>,
    ) -> Self {
        let contract_id = batch_query_contract_id(kind, CaptureKind::Shape)
            .unwrap_or_else(|| panic!("unsupported shape batch query: {kind:?}"));
        Self::for_contract(contract_id, backend, scene).expect("shape batch contract plan")
    }

    pub fn for_world_query(kind: BatchQueryKind, backend: DispatchBackend) -> Self {
        let contract_id = batch_query_contract_id(kind, CaptureKind::Region)
            .unwrap_or_else(|| panic!("unsupported world batch query: {kind:?}"));
        Self::for_contract(contract_id, backend, None).expect("world batch contract plan")
    }

    pub fn requires_virtual_gpu_scaffolding(&self) -> bool {
        self.stages
            .iter()
            .any(|stage| matches!(stage, PlanStage::BeginVirtualGpuDispatch))
    }

    pub fn candidate_strategy(&self) -> CandidateStrategy {
        self.candidate_contract.candidate_strategy
    }

    pub fn pruning_strategy(&self) -> PruningStrategy {
        self.candidate_contract.pruning_strategy
    }

    pub fn requests_culling_table(&self) -> bool {
        self.artifact_contracts
            .iter()
            .any(|artifact| matches!(artifact.schema, ArtifactSchema::CullingTable { .. }))
    }

    pub fn has_opaque_pessimization_boundary(&self) -> bool {
        self.artifact_contracts.iter().any(|artifact| {
            matches!(
                artifact.schema,
                ArtifactSchema::OpaquePessimizationBoundary { .. }
            )
        })
    }

    pub fn from_field_plan_kind(kind: FieldBatchPlanKind, capture_kind: CaptureKind) -> Self {
        Self::new(
            capture_kind,
            match kind {
                FieldBatchPlanKind::Distance => BatchQueryKind::Distance,
                FieldBatchPlanKind::Normal => BatchQueryKind::Normal,
            },
        )
    }

    pub fn from_shape_plan_kind(kind: ShapeBatchPlanKind) -> Self {
        Self::new(
            CaptureKind::Shape,
            match kind {
                ShapeBatchPlanKind::Nearest => BatchQueryKind::Nearest,
                ShapeBatchPlanKind::Trace => BatchQueryKind::Trace,
                ShapeBatchPlanKind::Surface => BatchQueryKind::Surface,
                ShapeBatchPlanKind::Occluded => BatchQueryKind::Occluded,
            },
        )
    }

    pub fn for_field(
        kind: FieldBatchPlanKind,
        capture_kind: CaptureKind,
    ) -> Result<Self, &'static str> {
        if matches!(capture_kind, CaptureKind::Region) {
            return Err("field batch plans require field or shape captures");
        }
        Ok(Self::from_field_plan_kind(kind, capture_kind))
    }

    pub fn for_shape(kind: ShapeBatchPlanKind) -> Self {
        Self::from_shape_plan_kind(kind)
    }
}

impl CaptureQueryPlan {
    pub fn for_contract(
        contract_id: QueryContractId,
        scene: Option<SceneSummary>,
    ) -> Result<Self, &'static str> {
        let Some((descriptor, binding)) = query_contract::query_contract_bundle(contract_id) else {
            return Err("missing query contract bundle");
        };
        Self::for_descriptor(descriptor, binding, scene)
    }

    pub(crate) fn for_descriptor(
        descriptor: &'static QueryContractDescriptor,
        binding: &'static QueryExecutionBinding,
        scene: Option<SceneSummary>,
    ) -> Result<Self, &'static str> {
        if descriptor.id != binding.contract_id {
            return Err("query descriptor and execution binding ids do not match");
        }
        if descriptor.surface != QuerySurfaceKind::CaptureScalar {
            return Err("capture query plans require capture-scalar contracts");
        }

        let helper_name = helper_name(binding);
        let result_kind = descriptor.result_kind;
        let executor = binding.default_executor;
        let preserves_local_hit_context = descriptor.preserves_local_hit_context;
        let kind = capture_query_kind_for_descriptor(descriptor)
            .ok_or("capture query descriptor does not map to a capture question kind")?;
        let capture_kind = descriptor.capture_kind;
        let candidate_strategy = match kind {
            CaptureQueryKind::SupportSummary => CandidateStrategy::SemanticSupportSummary,
            CaptureQueryKind::Surface => CandidateStrategy::SurfaceHitReuse,
            CaptureQueryKind::Distance | CaptureQueryKind::Normal
                if matches!(descriptor.capture_kind, CaptureKind::Field) =>
            {
                CandidateStrategy::DirectFieldCapture
            }
            _ => candidate_strategy_for_shape_query(BatchQueryKind::Nearest, scene.as_ref()),
        };
        let pruning_strategy = match kind {
            CaptureQueryKind::SupportSummary => PruningStrategy::None,
            CaptureQueryKind::Surface => PruningStrategy::None,
            CaptureQueryKind::Distance | CaptureQueryKind::Normal
                if matches!(descriptor.capture_kind, CaptureKind::Field) =>
            {
                PruningStrategy::None
            }
            _ => pruning_strategy_for_plan(
                BatchQueryKind::Nearest,
                descriptor.capture_kind,
                scene.as_ref(),
                candidate_strategy,
            ),
        };
        let derived_artifacts = derive_artifacts(
            scene.as_ref(),
            descriptor.capture_kind,
            candidate_strategy,
            pruning_strategy,
        );
        let mut stages = vec![PlanStage::LoadCapture];
        stages.extend(load_artifact_stages(&derived_artifacts));
        if !matches!(kind, CaptureQueryKind::SupportSummary) {
            stages.push(PlanStage::GenerateCandidates {
                strategy: candidate_strategy,
            });
            stages.push(PlanStage::PruneCandidates {
                strategy: pruning_strategy,
            });
        }
        if let Some(participant_kind) = descriptor.participant_kind {
            stages.push(PlanStage::SelectParticipants {
                kind: participant_selection_kind(participant_kind),
            });
        }
        stages.push(PlanStage::Execute { executor });
        if preserves_local_hit_context {
            stages.push(PlanStage::AssembleHitContext);
        }
        stages.push(PlanStage::AppendResult { result_kind });
        let candidate_contract = build_candidate_contract(
            if matches!(kind, CaptureQueryKind::Surface) {
                CandidateSource::SurfaceHit
            } else {
                CandidateSource::CaptureScene
            },
            descriptor.item_kind,
            candidate_strategy,
            pruning_strategy,
            match result_kind {
                QueryResultKind::Surface => WinnerSelectionMode::SurfaceReuse,
                QueryResultKind::Hit3 | QueryResultKind::OcclusionResult => {
                    WinnerSelectionMode::Nearest
                }
                _ => WinnerSelectionMode::None,
            },
            true,
        );
        let result_contract = build_result_contract(result_kind, preserves_local_hit_context);
        let hit_context_contract = preserves_local_hit_context.then(hit_context_contract);
        let participant_contract = descriptor.participant_kind.map(build_participant_contract);
        let evidence_summary = scene
            .as_ref()
            .map(SceneSummary::effective_evidence_summary)
            .unwrap_or_else(|| default_evidence_summary(descriptor.id));
        let artifact_contracts = derive_artifact_contracts(
            &derived_artifacts,
            scene.as_ref(),
            &evidence_summary,
            candidate_contract.item_kind,
            result_kind,
            preserves_local_hit_context,
            helper_name,
        );
        Ok(Self {
            contract_version: QUERY_PLAN_CONTRACT_VERSION,
            contract_id: descriptor.id,
            family: descriptor.family,
            target: descriptor.target,
            cardinality: descriptor.cardinality,
            surface: descriptor.surface,
            helper_name: SmolStr::new(helper_name),
            kind,
            capture_kind,
            result_kind,
            executor,
            scene,
            evidence_summary,
            candidate_strategy,
            pruning_strategy,
            stages,
            derived_artifacts,
            candidate_contract,
            result_contract,
            hit_context_contract,
            participant_contract,
            artifact_contracts,
            observability: planning_observability(descriptor.observability, pruning_strategy),
            preserves_local_hit_context,
        })
    }

    pub fn for_query(
        kind: CaptureQueryKind,
        capture_kind: CaptureKind,
        scene: Option<SceneSummary>,
    ) -> Result<Self, &'static str> {
        let Some(contract_id) = capture_query_contract_id(kind, capture_kind) else {
            return Err("capture query plan does not support the requested capture/kind pair");
        };
        Self::for_contract(contract_id, scene)
    }
}

impl CaptureQueryPlan {
    pub fn candidate_strategy(&self) -> CandidateStrategy {
        self.candidate_contract.candidate_strategy
    }

    pub fn pruning_strategy(&self) -> PruningStrategy {
        self.candidate_contract.pruning_strategy
    }

    pub fn requests_culling_table(&self) -> bool {
        self.artifact_contracts
            .iter()
            .any(|artifact| matches!(artifact.schema, ArtifactSchema::CullingTable { .. }))
    }

    pub fn has_opaque_pessimization_boundary(&self) -> bool {
        self.artifact_contracts.iter().any(|artifact| {
            matches!(
                artifact.schema,
                ArtifactSchema::OpaquePessimizationBoundary { .. }
            )
        })
    }
}

impl WorldQueryPlan {
    pub fn for_query(kind: WorldQueryKind) -> Self {
        Self::for_query_with_backend(kind, DispatchBackend::Auto)
    }

    pub fn for_contract(contract_id: QueryContractId) -> Result<Self, &'static str> {
        Self::for_contract_with_backend(contract_id, DispatchBackend::Auto)
    }

    pub fn for_contract_with_backend(
        contract_id: QueryContractId,
        backend: DispatchBackend,
    ) -> Result<Self, &'static str> {
        let Some((descriptor, binding)) = query_contract::query_contract_bundle(contract_id) else {
            return Err("missing query contract bundle");
        };
        Self::for_descriptor(descriptor, binding, backend)
    }

    pub(crate) fn for_descriptor(
        descriptor: &'static QueryContractDescriptor,
        binding: &'static QueryExecutionBinding,
        backend: DispatchBackend,
    ) -> Result<Self, &'static str> {
        if descriptor.id != binding.contract_id {
            return Err("query descriptor and execution binding ids do not match");
        }
        if descriptor.surface != QuerySurfaceKind::WorldScalar {
            return Err("world query plans require world-scalar contracts");
        }
        if descriptor.capture_kind != CaptureKind::Region {
            return Err("world query plans require region captures");
        }

        let helper_name = helper_name(binding);
        let result_kind = descriptor.result_kind;
        let executor = binding.default_executor;
        let preserves_local_hit_context = descriptor.preserves_local_hit_context;
        let item_kind = descriptor.item_kind;
        let kind = world_query_kind_for_descriptor(descriptor)
            .ok_or("world query descriptor does not map to a world question kind")?;
        let candidate_strategy = world_candidate_strategy(kind);
        let pruning_strategy = world_pruning_strategy(kind, candidate_strategy);
        let derived_artifacts = derive_world_artifacts(candidate_strategy, pruning_strategy);
        let mut stages = vec![PlanStage::SelectBackend, PlanStage::LoadCapture];
        stages.extend(load_artifact_stages(&derived_artifacts));
        stages.push(PlanStage::LoadDomainFlags);
        if !matches!(kind, WorldQueryKind::SupportSummary) {
            stages.push(PlanStage::GenerateCandidates {
                strategy: candidate_strategy,
            });
            stages.push(PlanStage::PruneCandidates {
                strategy: pruning_strategy,
            });
        }
        if let Some(participant_kind) = descriptor.participant_kind {
            stages.push(PlanStage::SelectParticipants {
                kind: participant_selection_kind(participant_kind),
            });
        }
        stages.push(PlanStage::Execute { executor });
        if preserves_local_hit_context {
            stages.push(PlanStage::AssembleHitContext);
        }
        stages.push(PlanStage::AppendResult { result_kind });
        let participant_contract = descriptor.participant_kind.map(build_participant_contract);
        let domain_flags = descriptor.required_domain_flags.to_vec();
        let dispatch_contract = DispatchRecordContract {
            backend,
            kernel: bound_kernel(binding),
            item_kind,
            result_kind,
        };
        let candidate_contract = build_candidate_contract(
            CandidateSource::WorldRegionShapes,
            item_kind,
            candidate_strategy,
            pruning_strategy,
            match result_kind {
                QueryResultKind::SupportSummaryResult => WinnerSelectionMode::None,
                QueryResultKind::Surface => WinnerSelectionMode::SurfaceReuse,
                QueryResultKind::RadianceResult | QueryResultKind::MediumResult => {
                    WinnerSelectionMode::Ordered
                }
                _ => WinnerSelectionMode::Nearest,
            },
            true,
        );
        let evidence_summary = default_evidence_summary(descriptor.id);
        Ok(Self {
            contract_version: QUERY_PLAN_CONTRACT_VERSION,
            contract_id: descriptor.id,
            family: descriptor.family,
            target: descriptor.target,
            cardinality: descriptor.cardinality,
            surface: descriptor.surface,
            helper_name: SmolStr::new(helper_name),
            kind,
            backend,
            result_kind,
            executor,
            evidence_summary: evidence_summary.clone(),
            candidate_strategy,
            pruning_strategy,
            stages,
            derived_artifacts: derived_artifacts.clone(),
            dispatch_contract: dispatch_contract.clone(),
            candidate_contract,
            result_contract: build_result_contract(result_kind, preserves_local_hit_context),
            hit_context_contract: preserves_local_hit_context.then(hit_context_contract),
            participant_contract,
            domain_flags: domain_flags.clone(),
            artifact_contracts: derive_artifact_contracts(
                &derived_artifacts,
                None,
                &evidence_summary,
                dispatch_contract.item_kind,
                result_kind,
                preserves_local_hit_context,
                helper_name,
            ),
            ray_solver: ray_solver_for_descriptor(descriptor, &evidence_summary),
            observability: planning_observability(descriptor.observability, pruning_strategy),
            preserves_local_hit_context,
        })
    }

    pub fn for_query_with_backend(kind: WorldQueryKind, backend: DispatchBackend) -> Self {
        let contract_id = world_query_contract_id(kind);
        Self::for_contract_with_backend(contract_id, backend).expect("world contract plan")
    }

    pub fn candidate_strategy(&self) -> CandidateStrategy {
        self.candidate_contract.candidate_strategy
    }

    pub fn pruning_strategy(&self) -> PruningStrategy {
        self.candidate_contract.pruning_strategy
    }

    pub fn requests_culling_table(&self) -> bool {
        self.artifact_contracts
            .iter()
            .any(|artifact| matches!(artifact.schema, ArtifactSchema::CullingTable { .. }))
    }
}

impl From<FieldBatchPlanKind> for BatchQueryKind {
    fn from(value: FieldBatchPlanKind) -> Self {
        match value {
            FieldBatchPlanKind::Distance => BatchQueryKind::Distance,
            FieldBatchPlanKind::Normal => BatchQueryKind::Normal,
        }
    }
}

impl From<ShapeBatchPlanKind> for BatchQueryKind {
    fn from(value: ShapeBatchPlanKind) -> Self {
        match value {
            ShapeBatchPlanKind::Nearest => BatchQueryKind::Nearest,
            ShapeBatchPlanKind::Trace => BatchQueryKind::Trace,
            ShapeBatchPlanKind::Surface => BatchQueryKind::Surface,
            ShapeBatchPlanKind::Occluded => BatchQueryKind::Occluded,
        }
    }
}

fn load_artifact_stages(artifacts: &[DerivedArtifact]) -> Vec<PlanStage> {
    artifacts
        .iter()
        .cloned()
        .map(|artifact| PlanStage::LoadDerivedArtifact { artifact })
        .collect()
}

fn derive_artifacts(
    scene: Option<&SceneSummary>,
    capture_kind: CaptureKind,
    candidate_strategy: CandidateStrategy,
    pruning_strategy: PruningStrategy,
) -> Vec<DerivedArtifact> {
    let (semantics, support_class, can_coarse_support_pruning) = scene
        .map(|summary| {
            (
                summary.semantics,
                summary.support_class,
                summary.can_coarse_support_pruning,
            )
        })
        .unwrap_or((
            DistanceSemantics::ConservativeLowerBound,
            SupportClass::Unknown,
            false,
        ));
    let mut artifacts = vec![
        DerivedArtifact::SupportSummary {
            semantics,
            support_class,
            can_coarse_support_pruning,
        },
        DerivedArtifact::CaptureCache { capture_kind },
    ];
    if matches!(pruning_strategy, PruningStrategy::CullingTable)
        || matches!(
            candidate_strategy,
            CandidateStrategy::SupportAcceleratedShapeTraversal
        )
    {
        artifacts.push(DerivedArtifact::CullingTable {
            candidate_strategy,
            pruning_strategy,
        });
    }
    if scene.is_some_and(|summary| summary.opaque_boundary) {
        artifacts.push(DerivedArtifact::OpaquePessimizationBoundary);
    }
    artifacts
}

fn build_candidate_contract(
    source: CandidateSource,
    item_kind: QueryItemKind,
    candidate_strategy: CandidateStrategy,
    pruning_strategy: PruningStrategy,
    winner_mode: WinnerSelectionMode,
    stable_leaf_identity: bool,
) -> CandidateRecordContract {
    CandidateRecordContract {
        source,
        item_kind,
        candidate_strategy,
        pruning_strategy,
        winner_mode,
        stable_leaf_identity,
    }
}

fn build_result_contract(
    result_kind: QueryResultKind,
    preserves_local_hit_context: bool,
) -> ResultRecordContract {
    ResultRecordContract {
        result_kind,
        preserves_local_hit_context,
        stable_feature_id: matches!(
            result_kind,
            QueryResultKind::Hit3 | QueryResultKind::Surface | QueryResultKind::RadianceResult
        ),
        stable_instance_id: preserves_local_hit_context,
        stable_repeat_id: preserves_local_hit_context,
    }
}

fn hit_context_contract() -> HitContextContract {
    HitContextContract {
        world_position: true,
        world_normal: true,
        local_position: true,
        local_normal: true,
        shading_frame: true,
        payload: true,
    }
}

fn build_participant_contract(kind: ParticipantContractKind) -> ParticipantSelectionContract {
    let kind = participant_selection_kind(kind);
    ParticipantSelectionContract {
        kind,
        provenance_aware: true,
        additive: matches!(kind, CaptureQueryKind::Radiance | CaptureQueryKind::Medium),
    }
}

fn planning_observability(
    profile: QueryObservabilityProfile,
    pruning_strategy: PruningStrategy,
) -> PlanningObservability {
    PlanningObservability {
        candidate_count: profile.candidate_count,
        branch_visits: profile.branch_visits,
        support_prune_effectiveness: profile.support_prune_effectiveness
            && !matches!(pruning_strategy, PruningStrategy::None),
        culling_hit_rate: profile.culling_hit_rate
            && matches!(pruning_strategy, PruningStrategy::CullingTable),
        trace_steps: profile.trace_steps,
        field_samples: profile.field_samples,
        artifact_sizes: profile.artifact_sizes,
        dispatch_overhead: profile.dispatch_overhead,
    }
}

fn world_candidate_strategy(kind: WorldQueryKind) -> CandidateStrategy {
    match kind {
        WorldQueryKind::SupportSummary => CandidateStrategy::SemanticSupportSummary,
        WorldQueryKind::Surface => CandidateStrategy::SurfaceHitReuse,
        WorldQueryKind::Radiance | WorldQueryKind::Medium => {
            CandidateStrategy::ShapeBranchTraversal
        }
        WorldQueryKind::Distance
        | WorldQueryKind::Normal
        | WorldQueryKind::Nearest
        | WorldQueryKind::Trace
        | WorldQueryKind::Occluded => CandidateStrategy::SupportAcceleratedShapeTraversal,
    }
}

fn batch_kind_to_world_kind(kind: BatchQueryKind) -> WorldQueryKind {
    match kind {
        BatchQueryKind::Distance => WorldQueryKind::Distance,
        BatchQueryKind::Normal => WorldQueryKind::Normal,
        BatchQueryKind::Nearest | BatchQueryKind::Trace => WorldQueryKind::Nearest,
        BatchQueryKind::Surface => WorldQueryKind::Surface,
        BatchQueryKind::Occluded => WorldQueryKind::Occluded,
        BatchQueryKind::Radiance => WorldQueryKind::Radiance,
        BatchQueryKind::Medium => WorldQueryKind::Medium,
    }
}

fn world_pruning_strategy(
    kind: WorldQueryKind,
    candidate_strategy: CandidateStrategy,
) -> PruningStrategy {
    match kind {
        WorldQueryKind::SupportSummary => PruningStrategy::None,
        WorldQueryKind::Surface => PruningStrategy::None,
        WorldQueryKind::Radiance | WorldQueryKind::Medium => PruningStrategy::ConservativeTraversal,
        WorldQueryKind::Distance
        | WorldQueryKind::Normal
        | WorldQueryKind::Nearest
        | WorldQueryKind::Trace
        | WorldQueryKind::Occluded => {
            if matches!(
                candidate_strategy,
                CandidateStrategy::SupportAcceleratedShapeTraversal
            ) {
                PruningStrategy::SupportLowerBound
            } else {
                PruningStrategy::ConservativeTraversal
            }
        }
    }
}

fn derive_world_artifacts(
    candidate_strategy: CandidateStrategy,
    pruning_strategy: PruningStrategy,
) -> Vec<DerivedArtifact> {
    let mut artifacts = vec![DerivedArtifact::CaptureCache {
        capture_kind: CaptureKind::Region,
    }];
    if matches!(
        candidate_strategy,
        CandidateStrategy::SupportAcceleratedShapeTraversal
    ) {
        artifacts.push(DerivedArtifact::CullingTable {
            candidate_strategy,
            pruning_strategy,
        });
    }
    artifacts
}

fn default_evidence_summary(contract_id: QueryContractId) -> SemanticEvidenceSummary {
    SemanticEvidenceSummary::runtime_unknown(format!("{}::runtime", contract_id.as_str()))
}

fn ray_solver_for_descriptor(
    descriptor: &QueryContractDescriptor,
    evidence_summary: &SemanticEvidenceSummary,
) -> Option<RaySolverPlan> {
    is_ray_shaped_spatial_contract(descriptor.id)
        .then(|| {
            RaySolverPlan::for_contract(
                descriptor.id,
                Some(crate::semantic_evidence::SemanticEvidence::from_summary(
                    evidence_summary,
                )),
            )
        })
        .flatten()
}

fn derive_artifact_contracts(
    artifacts: &[DerivedArtifact],
    scene: Option<&SceneSummary>,
    evidence_summary: &SemanticEvidenceSummary,
    item_kind: QueryItemKind,
    result_kind: QueryResultKind,
    preserves_local_hit_context: bool,
    producer: &str,
) -> Vec<ArtifactContract> {
    let mut out = artifacts
        .iter()
        .enumerate()
        .map(|(index, artifact)| ArtifactContract {
            id: SmolStr::new(format!("{producer}::artifact::{index}")),
            evidence_summary: evidence_summary
                .with_artifact_binding(format!("{producer}::artifact::{index}")),
            schema: match artifact {
                DerivedArtifact::SupportSummary {
                    semantics,
                    support_class,
                    can_coarse_support_pruning,
                } => ArtifactSchema::SupportSummary {
                    semantics: *semantics,
                    support_class: *support_class,
                    can_coarse_support_pruning: *can_coarse_support_pruning,
                    semantic_root: scene
                        .map(|summary| summary.semantic_root)
                        .unwrap_or_default(),
                    support_root: scene
                        .map(|summary| summary.support_root)
                        .unwrap_or_default(),
                    node_count: scene.map(|summary| summary.node_count).unwrap_or_default(),
                    support_node_count: scene
                        .map(|summary| summary.support_node_count)
                        .unwrap_or_default(),
                    leaf_count: scene.map(|summary| summary.leaf_count).unwrap_or_default(),
                    identity_source_count: scene
                        .map(|summary| summary.identity_source_count)
                        .unwrap_or_default(),
                },
                DerivedArtifact::CaptureCache { capture_kind } => ArtifactSchema::CaptureCache {
                    capture_kind: *capture_kind,
                    semantic_root: scene
                        .map(|summary| summary.semantic_root)
                        .unwrap_or_default(),
                },
                DerivedArtifact::CullingTable {
                    candidate_strategy,
                    pruning_strategy,
                } => ArtifactSchema::CullingTable {
                    candidate_strategy: *candidate_strategy,
                    pruning_strategy: *pruning_strategy,
                    support_class: scene
                        .map(|summary| summary.support_class)
                        .unwrap_or(SupportClass::Unknown),
                    semantics: scene
                        .map(|summary| summary.semantics)
                        .unwrap_or(DistanceSemantics::ConservativeLowerBound),
                    support_root: scene
                        .map(|summary| summary.support_root)
                        .unwrap_or_default(),
                    support_node_count: scene
                        .map(|summary| summary.support_node_count)
                        .unwrap_or_default(),
                    leaf_count: scene.map(|summary| summary.leaf_count).unwrap_or_default(),
                    identity_source_count: scene
                        .map(|summary| summary.identity_source_count)
                        .unwrap_or_default(),
                },
                DerivedArtifact::OpaquePessimizationBoundary => {
                    ArtifactSchema::OpaquePessimizationBoundary {
                        support_root: scene
                            .map(|summary| summary.support_root)
                            .unwrap_or_default(),
                        support_node_count: scene
                            .map(|summary| summary.support_node_count)
                            .unwrap_or_default(),
                    }
                }
            },
            producer: SmolStr::new(producer),
            consumer: SmolStr::new(producer),
            deterministic: true,
            version: QUERY_PLAN_CONTRACT_VERSION,
        })
        .collect::<Vec<_>>();
    out.push(ArtifactContract {
        id: SmolStr::new(format!("{producer}::dispatch")),
        evidence_summary: evidence_summary.with_artifact_binding(format!("{producer}::dispatch")),
        schema: ArtifactSchema::DispatchRecord {
            item_kind,
            result_kind,
        },
        producer: SmolStr::new(producer),
        consumer: SmolStr::new(producer),
        deterministic: true,
        version: QUERY_PLAN_CONTRACT_VERSION,
    });
    out.push(ArtifactContract {
        id: SmolStr::new(format!("{producer}::result")),
        evidence_summary: evidence_summary.with_artifact_binding(format!("{producer}::result")),
        schema: ArtifactSchema::HitResultBuffer {
            result_kind,
            preserves_local_hit_context,
        },
        producer: SmolStr::new(producer),
        consumer: SmolStr::new(producer),
        deterministic: true,
        version: QUERY_PLAN_CONTRACT_VERSION,
    });
    out
}

fn candidate_strategy_for_field_query(
    capture_kind: CaptureKind,
    scene: Option<&SceneSummary>,
) -> CandidateStrategy {
    match capture_kind {
        CaptureKind::Field => CandidateStrategy::DirectFieldCapture,
        CaptureKind::Shape => candidate_strategy_for_shape_query(BatchQueryKind::Nearest, scene),
        CaptureKind::Region => CandidateStrategy::OpaqueFallback,
    }
}

fn candidate_strategy_for_shape_query(
    kind: BatchQueryKind,
    scene: Option<&SceneSummary>,
) -> CandidateStrategy {
    match kind {
        BatchQueryKind::Surface => CandidateStrategy::SurfaceHitReuse,
        BatchQueryKind::Nearest | BatchQueryKind::Trace | BatchQueryKind::Occluded => {
            let Some(scene) = scene else {
                return CandidateStrategy::ShapeBranchTraversal;
            };
            let evidence_summary = scene.effective_evidence_summary();
            if evidence_summary.support.opaque_boundary {
                CandidateStrategy::OpaqueFallback
            } else if evidence_summary.support.can_coarse_prune
                && matches!(
                    evidence_summary.support.support_class,
                    SupportClass::Bounded | SupportClass::Periodic
                )
            {
                CandidateStrategy::SupportAcceleratedShapeTraversal
            } else {
                CandidateStrategy::ShapeBranchTraversal
            }
        }
        _ => CandidateStrategy::DirectFieldCapture,
    }
}

fn pruning_strategy_for_plan(
    kind: BatchQueryKind,
    capture_kind: CaptureKind,
    scene: Option<&SceneSummary>,
    candidate_strategy: CandidateStrategy,
) -> PruningStrategy {
    if matches!(candidate_strategy, CandidateStrategy::OpaqueFallback) {
        return PruningStrategy::OpaquePessimizationBoundary;
    }
    if matches!(capture_kind, CaptureKind::Field) {
        return PruningStrategy::None;
    }
    match kind {
        BatchQueryKind::Nearest
        | BatchQueryKind::Trace
        | BatchQueryKind::Occluded
        | BatchQueryKind::Distance
        | BatchQueryKind::Normal => {
            let Some(scene) = scene else {
                return PruningStrategy::ConservativeTraversal;
            };
            let evidence_summary = scene.effective_evidence_summary();
            if matches!(
                candidate_strategy,
                CandidateStrategy::SupportAcceleratedShapeTraversal
            ) {
                return PruningStrategy::CullingTable;
            }
            if matches!(candidate_strategy, CandidateStrategy::ShapeBranchTraversal)
                && evidence_summary.support.can_coarse_prune
                && matches!(
                    evidence_summary.support.support_class,
                    SupportClass::Bounded
                )
            {
                return PruningStrategy::SupportLowerBound;
            }
            PruningStrategy::ConservativeTraversal
        }
        BatchQueryKind::Surface | BatchQueryKind::Radiance | BatchQueryKind::Medium => {
            PruningStrategy::None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene_ir::{DistanceSemantics, SupportClass};

    #[test]
    fn batch_query_plans_are_deterministic() {
        let summary = SceneSummary {
            name: Some(SmolStr::new("sphere_field")),
            semantics: DistanceSemantics::ExactSignedDistance,
            support_class: SupportClass::Bounded,
            can_coarse_support_pruning: true,
            opaque_boundary: false,
            semantic_root: 1,
            support_root: 1,
            node_count: 1,
            support_node_count: 1,
            leaf_count: 1,
            identity_source_count: 0,
            ..Default::default()
        };
        let left = BatchQueryPlan::for_shape_query(
            BatchQueryKind::Trace,
            DispatchBackend::Cpu,
            Some(summary.clone()),
        );
        let right = BatchQueryPlan::for_shape_query(
            BatchQueryKind::Trace,
            DispatchBackend::Cpu,
            Some(summary),
        );
        assert_eq!(left, right);
        assert_eq!(
            left.contract_id,
            crate::query_contract::SPATIAL_TRACE_BATCH_SHAPE
        );
        assert_eq!(left.family, QueryFamilyId::Spatial);
        assert_eq!(left.surface, QuerySurfaceKind::CaptureBatch);
    }

    #[test]
    fn opaque_shape_trace_plan_requests_pessimization_boundary() {
        let plan = BatchQueryPlan::for_shape_query(
            BatchQueryKind::Trace,
            DispatchBackend::VirtualGpu,
            Some(SceneSummary {
                name: Some(SmolStr::new("scene_shape")),
                semantics: DistanceSemantics::UnknownOpaque,
                support_class: SupportClass::Bounded,
                can_coarse_support_pruning: false,
                opaque_boundary: true,
                semantic_root: 1,
                support_root: 1,
                node_count: 1,
                support_node_count: 1,
                leaf_count: 1,
                identity_source_count: 0,
                ..Default::default()
            }),
        );
        assert_eq!(plan.kernel, InternalKernelKind::ShapeTraceCapture);
        assert_eq!(
            plan.contract_id,
            crate::query_contract::SPATIAL_TRACE_BATCH_SHAPE
        );
        assert_eq!(plan.family, QueryFamilyId::Spatial);
        assert_eq!(plan.surface, QuerySurfaceKind::CaptureBatch);
        assert!(plan.requires_virtual_gpu_scaffolding());
        assert!(plan.preserves_local_hit_context);
        assert_eq!(plan.candidate_strategy(), CandidateStrategy::OpaqueFallback);
        assert_eq!(
            plan.pruning_strategy(),
            PruningStrategy::OpaquePessimizationBoundary
        );
        assert!(plan.has_opaque_pessimization_boundary());
    }

    #[test]
    fn field_normal_plan_uses_field_normal_executor_without_scene_traversal() {
        let plan = BatchQueryPlan::for_field_query(
            BatchQueryKind::Normal,
            CaptureKind::Field,
            DispatchBackend::Auto,
            None,
        );
        assert_eq!(plan.executor, PlanExecutor::FieldNormalCapture);
        assert_eq!(plan.kernel, InternalKernelKind::FieldNormalCapture);
        assert_eq!(plan.result_kind, QueryResultKind::NormalResult);
        assert_eq!(plan.item_kind, QueryItemKind::PointQuery);
        assert_eq!(
            plan.contract_id,
            crate::query_contract::SPATIAL_NORMAL_BATCH_FIELD
        );
        assert_eq!(plan.family, QueryFamilyId::Spatial);
        assert_eq!(plan.surface, QuerySurfaceKind::CaptureBatch);
        assert!(!plan.preserves_local_hit_context);
        assert_eq!(
            plan.candidate_strategy(),
            CandidateStrategy::DirectFieldCapture
        );
        assert_eq!(plan.pruning_strategy(), PruningStrategy::None);
    }
}
