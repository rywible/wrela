use self::DispatchBackend::{Auto, VirtualGpu, Wgsl};
use crate::query_contract::{
    self, ParticipantContractKind, QueryContractDescriptor, QueryExecutionBinding,
    QueryObservabilityProfile, QueryPlannerRecipeKind,
};
use crate::scene_ir::{DistanceSemantics, SupportClass};
use smol_str::SmolStr;

pub use crate::query_contract::{
    CaptureKind, DispatchBackend, InternalKernelKind, PlanExecutor,
    QUERY_CONTRACT_VERSION as QUERY_PLAN_CONTRACT_VERSION, QueryContractId, QueryFamilyId,
    QueryItemKind, QueryResultKind, QuerySurfaceKind, SceneDomainFlag,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BatchQueryKind {
    Distance,
    Normal,
    Trace,
    Surface,
    Occluded,
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
    Radiance,
    Medium,
    Trace,
    Surface,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ShapeBatchPlanKind {
    Trace,
    Surface,
    Occluded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum WorldQueryKind {
    Distance,
    Normal,
    Radiance,
    Medium,
    Trace,
    Surface,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CandidateStrategy {
    DirectFieldCapture,
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanningObservability {
    pub candidate_count: bool,
    pub branch_visits: bool,
    pub support_prune_effectiveness: bool,
    pub culling_hit_rate: bool,
    pub artifact_sizes: bool,
    pub dispatch_overhead: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BatchItemContract {
    CaptureQuery { plan: CaptureQueryPlan },
    TraceThenOcclusion { trace_plan: CaptureQueryPlan },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneSummary {
    pub name: Option<SmolStr>,
    pub semantics: DistanceSemantics,
    pub support_class: SupportClass,
    pub can_coarse_support_pruning: bool,
    pub opaque_boundary: bool,
    pub semantic_root: u32,
    pub support_root: u32,
    pub node_count: u32,
    pub support_node_count: u32,
    pub leaf_count: u32,
    pub identity_source_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchQueryPlan {
    pub contract_version: u32,
    pub contract_id: QueryContractId,
    pub family: QueryFamilyId,
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
    pub observability: PlanningObservability,
    pub preserves_local_hit_context: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureQueryPlan {
    pub contract_version: u32,
    pub contract_id: QueryContractId,
    pub family: QueryFamilyId,
    pub surface: QuerySurfaceKind,
    pub helper_name: SmolStr,
    pub kind: CaptureQueryKind,
    pub capture_kind: CaptureKind,
    pub result_kind: QueryResultKind,
    pub executor: PlanExecutor,
    pub scene: Option<SceneSummary>,
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
    pub surface: QuerySurfaceKind,
    pub helper_name: SmolStr,
    pub kind: WorldQueryKind,
    pub backend: DispatchBackend,
    pub result_kind: QueryResultKind,
    pub executor: PlanExecutor,
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
        (BatchQueryKind::Normal, CaptureKind::Field) => {
            Some(query_contract::SPATIAL_NORMAL_BATCH_FIELD)
        }
        (BatchQueryKind::Normal, CaptureKind::Shape) => {
            Some(query_contract::SPATIAL_NORMAL_BATCH_SHAPE)
        }
        (BatchQueryKind::Trace, CaptureKind::Shape) => {
            Some(query_contract::SPATIAL_TRACE_BATCH_SHAPE)
        }
        (BatchQueryKind::Surface, CaptureKind::Shape) => {
            Some(query_contract::SURFACE_SAMPLE_BATCH_SHAPE)
        }
        (BatchQueryKind::Occluded, CaptureKind::Shape) => {
            Some(query_contract::SPATIAL_OCCLUDED_BATCH_SHAPE)
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
        (CaptureQueryKind::Radiance, CaptureKind::Shape) => {
            Some(query_contract::PARTICIPANTS_RADIANCE_CAPTURE_SHAPE)
        }
        (CaptureQueryKind::Medium, CaptureKind::Shape) => {
            Some(query_contract::PARTICIPANTS_MEDIUM_CAPTURE_SHAPE)
        }
        (CaptureQueryKind::Trace, CaptureKind::Shape) => {
            Some(query_contract::SPATIAL_TRACE_CAPTURE_SHAPE)
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
        WorldQueryKind::Radiance => query_contract::PARTICIPANTS_RADIANCE_WORLD,
        WorldQueryKind::Medium => query_contract::PARTICIPANTS_MEDIUM_WORLD,
        WorldQueryKind::Trace => query_contract::SPATIAL_TRACE_WORLD,
        WorldQueryKind::Surface => query_contract::SURFACE_SAMPLE_WORLD,
    }
}

fn query_contract_bundle(
    contract_id: QueryContractId,
) -> (
    &'static QueryContractDescriptor,
    &'static QueryExecutionBinding,
) {
    query_contract::query_contract_bundle(contract_id).unwrap_or_else(|| {
        panic!(
            "missing query contract bundle for '{}'",
            contract_id.as_str()
        )
    })
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

fn batch_query_kind_for_recipe(recipe: QueryPlannerRecipeKind) -> BatchQueryKind {
    match recipe {
        QueryPlannerRecipeKind::SpatialDistanceBatchField
        | QueryPlannerRecipeKind::SpatialDistanceBatchShape => BatchQueryKind::Distance,
        QueryPlannerRecipeKind::SpatialNormalBatchField
        | QueryPlannerRecipeKind::SpatialNormalBatchShape => BatchQueryKind::Normal,
        QueryPlannerRecipeKind::SpatialTraceBatchShape => BatchQueryKind::Trace,
        QueryPlannerRecipeKind::SpatialOccludedBatchShape => BatchQueryKind::Occluded,
        QueryPlannerRecipeKind::SurfaceSampleBatchShape => BatchQueryKind::Surface,
        other => panic!("unexpected batch planner recipe: {other:?}"),
    }
}

fn capture_query_kind_for_recipe(recipe: QueryPlannerRecipeKind) -> CaptureQueryKind {
    match recipe {
        QueryPlannerRecipeKind::SpatialDistanceCaptureField
        | QueryPlannerRecipeKind::SpatialDistanceCaptureShape => CaptureQueryKind::Distance,
        QueryPlannerRecipeKind::SpatialNormalCaptureField
        | QueryPlannerRecipeKind::SpatialNormalCaptureShape => CaptureQueryKind::Normal,
        QueryPlannerRecipeKind::SpatialTraceCaptureShape => CaptureQueryKind::Trace,
        QueryPlannerRecipeKind::SurfaceSampleCaptureShape => CaptureQueryKind::Surface,
        QueryPlannerRecipeKind::ParticipantsRadianceCaptureShape => CaptureQueryKind::Radiance,
        QueryPlannerRecipeKind::ParticipantsMediumCaptureShape => CaptureQueryKind::Medium,
        other => panic!("unexpected capture planner recipe: {other:?}"),
    }
}

fn world_query_kind_for_recipe(recipe: QueryPlannerRecipeKind) -> WorldQueryKind {
    match recipe {
        QueryPlannerRecipeKind::SpatialDistanceWorld => WorldQueryKind::Distance,
        QueryPlannerRecipeKind::SpatialNormalWorld => WorldQueryKind::Normal,
        QueryPlannerRecipeKind::SpatialTraceWorld => WorldQueryKind::Trace,
        QueryPlannerRecipeKind::SurfaceSampleWorld => WorldQueryKind::Surface,
        QueryPlannerRecipeKind::ParticipantsRadianceWorld => WorldQueryKind::Radiance,
        QueryPlannerRecipeKind::ParticipantsMediumWorld => WorldQueryKind::Medium,
        other => panic!("unexpected world planner recipe: {other:?}"),
    }
}

impl BatchQueryPlan {
    pub fn new<K>(capture_kind: CaptureKind, kind: K) -> Self
    where
        K: Into<BatchQueryKind>,
    {
        let kind = kind.into();
        match kind {
            BatchQueryKind::Distance | BatchQueryKind::Normal => {
                Self::for_field_query(kind, capture_kind, DispatchBackend::Auto, None)
            }
            BatchQueryKind::Trace | BatchQueryKind::Surface | BatchQueryKind::Occluded => {
                Self::for_shape_query(kind, DispatchBackend::Auto, None)
            }
        }
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
        let (descriptor, binding) = query_contract_bundle(contract_id);
        let helper_name = helper_name(binding);
        let kernel = bound_kernel(binding);
        let result_kind = descriptor.result_kind;
        let executor = binding.default_executor;
        let planner_kind = batch_query_kind_for_recipe(binding.planner_recipe);
        debug_assert_eq!(descriptor.family, QueryFamilyId::Spatial);
        debug_assert_eq!(descriptor.surface, QuerySurfaceKind::CaptureBatch);
        debug_assert_eq!(descriptor.capture_kind, capture_kind);
        debug_assert_eq!(descriptor.item_kind, QueryItemKind::PointQuery);
        let candidate_strategy =
            candidate_strategy_for_field_query(descriptor.capture_kind, scene.as_ref());
        let pruning_strategy = pruning_strategy_for_plan(
            planner_kind,
            descriptor.capture_kind,
            scene.as_ref(),
            candidate_strategy,
        );
        let derived_artifacts = derive_artifacts(
            scene.as_ref(),
            descriptor.capture_kind,
            candidate_strategy,
            pruning_strategy,
        );
        let mut stages = vec![PlanStage::SelectBackend];
        if matches!(backend, VirtualGpu | Wgsl | Auto) {
            stages.push(PlanStage::BeginVirtualGpuDispatch);
        }
        stages.push(PlanStage::LoadCapture);
        stages.extend(load_artifact_stages(&derived_artifacts));
        stages.push(PlanStage::IterateItems {
            item_kind: descriptor.item_kind,
        });
        stages.push(PlanStage::GenerateCandidates {
            strategy: candidate_strategy,
        });
        stages.push(PlanStage::PruneCandidates {
            strategy: pruning_strategy,
        });
        stages.push(PlanStage::Execute { executor });
        stages.push(PlanStage::AppendResult { result_kind });
        if matches!(backend, VirtualGpu | Wgsl | Auto) {
            stages.push(PlanStage::EndVirtualGpuDispatch);
        }
        let candidate_contract = build_candidate_contract(
            CandidateSource::CaptureScene,
            descriptor.item_kind,
            candidate_strategy,
            pruning_strategy,
            WinnerSelectionMode::Nearest,
            true,
        );
        let result_contract = build_result_contract(result_kind, false);
        let artifact_contracts = derive_artifact_contracts(
            &derived_artifacts,
            scene.as_ref(),
            descriptor.item_kind,
            result_kind,
            false,
            helper_name,
        );
        let item_query_kind = match binding.planner_recipe {
            QueryPlannerRecipeKind::SpatialDistanceBatchField
            | QueryPlannerRecipeKind::SpatialDistanceBatchShape => CaptureQueryKind::Distance,
            QueryPlannerRecipeKind::SpatialNormalBatchField
            | QueryPlannerRecipeKind::SpatialNormalBatchShape => CaptureQueryKind::Normal,
            other => panic!("unexpected field batch planner recipe: {other:?}"),
        };
        let item_contract = BatchItemContract::CaptureQuery {
            plan: CaptureQueryPlan::for_query(item_query_kind, capture_kind, scene.clone())
                .expect("field batch item contract"),
        };
        Self {
            contract_version: QUERY_PLAN_CONTRACT_VERSION,
            contract_id,
            family: descriptor.family,
            surface: descriptor.surface,
            helper_name: SmolStr::new(helper_name),
            kind,
            capture_kind,
            backend,
            kernel,
            item_kind: descriptor.item_kind,
            result_kind,
            executor,
            scene,
            candidate_strategy,
            pruning_strategy,
            stages,
            derived_artifacts,
            dispatch_contract: DispatchRecordContract {
                backend,
                kernel,
                item_kind: descriptor.item_kind,
                result_kind,
            },
            candidate_contract,
            result_contract,
            hit_context_contract: None,
            participant_contract: None,
            domain_flags: descriptor.required_domain_flags.to_vec(),
            artifact_contracts,
            item_contract,
            observability: planning_observability(descriptor.observability, pruning_strategy),
            preserves_local_hit_context: descriptor.preserves_local_hit_context,
        }
    }

    pub fn for_shape_query(
        kind: BatchQueryKind,
        backend: DispatchBackend,
        scene: Option<SceneSummary>,
    ) -> Self {
        let contract_id = batch_query_contract_id(kind, CaptureKind::Shape)
            .unwrap_or_else(|| panic!("unsupported shape batch query: {kind:?}"));
        let (descriptor, binding) = query_contract_bundle(contract_id);
        let helper_name = helper_name(binding);
        let kernel = bound_kernel(binding);
        let item_kind = descriptor.item_kind;
        let result_kind = descriptor.result_kind;
        let executor = binding.default_executor;
        let preserves_local_hit_context = descriptor.preserves_local_hit_context;
        let planner_kind = batch_query_kind_for_recipe(binding.planner_recipe);
        debug_assert_eq!(descriptor.surface, QuerySurfaceKind::CaptureBatch);
        debug_assert_eq!(descriptor.capture_kind, CaptureKind::Shape);
        let candidate_strategy = candidate_strategy_for_shape_query(planner_kind, scene.as_ref());
        let pruning_strategy = pruning_strategy_for_plan(
            planner_kind,
            descriptor.capture_kind,
            scene.as_ref(),
            candidate_strategy,
        );
        let derived_artifacts = derive_artifacts(
            scene.as_ref(),
            descriptor.capture_kind,
            candidate_strategy,
            pruning_strategy,
        );
        let mut stages = vec![PlanStage::SelectBackend];
        if matches!(backend, VirtualGpu | Wgsl | Auto) {
            stages.push(PlanStage::BeginVirtualGpuDispatch);
        }
        stages.push(PlanStage::LoadCapture);
        stages.extend(load_artifact_stages(&derived_artifacts));
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
        let winner_mode = match binding.planner_recipe {
            QueryPlannerRecipeKind::SurfaceSampleBatchShape => WinnerSelectionMode::SurfaceReuse,
            QueryPlannerRecipeKind::SpatialTraceBatchShape
            | QueryPlannerRecipeKind::SpatialOccludedBatchShape => WinnerSelectionMode::Nearest,
            other => panic!("unexpected shape batch planner recipe: {other:?}"),
        };
        let candidate_contract = build_candidate_contract(
            CandidateSource::CaptureScene,
            item_kind,
            candidate_strategy,
            pruning_strategy,
            winner_mode,
            true,
        );
        let result_contract = build_result_contract(result_kind, preserves_local_hit_context);
        let hit_context_contract = preserves_local_hit_context.then(hit_context_contract);
        let artifact_contracts = derive_artifact_contracts(
            &derived_artifacts,
            scene.as_ref(),
            item_kind,
            result_kind,
            preserves_local_hit_context,
            helper_name,
        );
        let item_contract = match binding.planner_recipe {
            QueryPlannerRecipeKind::SpatialTraceBatchShape => BatchItemContract::CaptureQuery {
                plan: CaptureQueryPlan::for_query(
                    CaptureQueryKind::Trace,
                    CaptureKind::Shape,
                    scene.clone(),
                )
                .expect("trace batch contract"),
            },
            QueryPlannerRecipeKind::SurfaceSampleBatchShape => BatchItemContract::CaptureQuery {
                plan: CaptureQueryPlan::for_query(
                    CaptureQueryKind::Surface,
                    CaptureKind::Shape,
                    scene.clone(),
                )
                .expect("surface batch contract"),
            },
            QueryPlannerRecipeKind::SpatialOccludedBatchShape => {
                BatchItemContract::TraceThenOcclusion {
                    trace_plan: CaptureQueryPlan::for_query(
                        CaptureQueryKind::Trace,
                        CaptureKind::Shape,
                        scene.clone(),
                    )
                    .expect("occlusion trace contract"),
                }
            }
            other => panic!("unexpected shape batch planner recipe: {other:?}"),
        };
        Self {
            contract_version: QUERY_PLAN_CONTRACT_VERSION,
            contract_id,
            family: descriptor.family,
            surface: descriptor.surface,
            helper_name: SmolStr::new(helper_name),
            kind,
            capture_kind: CaptureKind::Shape,
            backend,
            kernel,
            item_kind,
            result_kind,
            executor,
            scene,
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
            participant_contract: None,
            domain_flags: descriptor.required_domain_flags.to_vec(),
            artifact_contracts,
            item_contract,
            observability: planning_observability(descriptor.observability, pruning_strategy),
            preserves_local_hit_context,
        }
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
    pub fn for_query(
        kind: CaptureQueryKind,
        capture_kind: CaptureKind,
        scene: Option<SceneSummary>,
    ) -> Result<Self, &'static str> {
        let Some(contract_id) = capture_query_contract_id(kind, capture_kind) else {
            return Err("capture query plan does not support the requested capture/kind pair");
        };
        let (descriptor, binding) = query_contract_bundle(contract_id);
        let helper_name = helper_name(binding);
        let result_kind = descriptor.result_kind;
        let executor = binding.default_executor;
        let preserves_local_hit_context = descriptor.preserves_local_hit_context;
        let planner_kind = capture_query_kind_for_recipe(binding.planner_recipe);
        debug_assert_eq!(descriptor.surface, QuerySurfaceKind::CaptureScalar);
        debug_assert_eq!(descriptor.capture_kind, capture_kind);
        let candidate_strategy = match planner_kind {
            CaptureQueryKind::Surface => CandidateStrategy::SurfaceHitReuse,
            CaptureQueryKind::Distance | CaptureQueryKind::Normal
                if matches!(descriptor.capture_kind, CaptureKind::Field) =>
            {
                CandidateStrategy::DirectFieldCapture
            }
            _ => candidate_strategy_for_shape_query(BatchQueryKind::Trace, scene.as_ref()),
        };
        let pruning_strategy = match planner_kind {
            CaptureQueryKind::Surface => PruningStrategy::None,
            CaptureQueryKind::Distance | CaptureQueryKind::Normal
                if matches!(descriptor.capture_kind, CaptureKind::Field) =>
            {
                PruningStrategy::None
            }
            _ => pruning_strategy_for_plan(
                BatchQueryKind::Trace,
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
        stages.push(PlanStage::GenerateCandidates {
            strategy: candidate_strategy,
        });
        stages.push(PlanStage::PruneCandidates {
            strategy: pruning_strategy,
        });
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
            if matches!(planner_kind, CaptureQueryKind::Surface) {
                CandidateSource::SurfaceHit
            } else {
                CandidateSource::CaptureScene
            },
            descriptor.item_kind,
            candidate_strategy,
            pruning_strategy,
            match binding.planner_recipe {
                QueryPlannerRecipeKind::SurfaceSampleCaptureShape => {
                    WinnerSelectionMode::SurfaceReuse
                }
                QueryPlannerRecipeKind::SpatialTraceCaptureShape => WinnerSelectionMode::Nearest,
                QueryPlannerRecipeKind::SpatialDistanceCaptureField
                | QueryPlannerRecipeKind::SpatialDistanceCaptureShape
                | QueryPlannerRecipeKind::SpatialNormalCaptureField
                | QueryPlannerRecipeKind::SpatialNormalCaptureShape
                | QueryPlannerRecipeKind::ParticipantsRadianceCaptureShape
                | QueryPlannerRecipeKind::ParticipantsMediumCaptureShape => {
                    WinnerSelectionMode::None
                }
                other => panic!("unexpected capture planner recipe: {other:?}"),
            },
            true,
        );
        let result_contract = build_result_contract(result_kind, preserves_local_hit_context);
        let hit_context_contract = preserves_local_hit_context.then(hit_context_contract);
        let participant_contract = descriptor.participant_kind.map(build_participant_contract);
        let artifact_contracts = derive_artifact_contracts(
            &derived_artifacts,
            scene.as_ref(),
            candidate_contract.item_kind,
            result_kind,
            preserves_local_hit_context,
            helper_name,
        );
        Ok(Self {
            contract_version: QUERY_PLAN_CONTRACT_VERSION,
            contract_id,
            family: descriptor.family,
            surface: descriptor.surface,
            helper_name: SmolStr::new(helper_name),
            kind,
            capture_kind,
            result_kind,
            executor,
            scene,
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

    pub fn for_query_with_backend(kind: WorldQueryKind, backend: DispatchBackend) -> Self {
        let contract_id = world_query_contract_id(kind);
        let (descriptor, binding) = query_contract_bundle(contract_id);
        let helper_name = helper_name(binding);
        let result_kind = descriptor.result_kind;
        let executor = binding.default_executor;
        let preserves_local_hit_context = descriptor.preserves_local_hit_context;
        let item_kind = descriptor.item_kind;
        let planner_kind = world_query_kind_for_recipe(binding.planner_recipe);
        debug_assert_eq!(descriptor.surface, QuerySurfaceKind::WorldScalar);
        debug_assert_eq!(descriptor.capture_kind, CaptureKind::Region);
        let candidate_strategy = world_candidate_strategy(planner_kind);
        let pruning_strategy = world_pruning_strategy(planner_kind, candidate_strategy);
        let derived_artifacts = derive_world_artifacts(candidate_strategy, pruning_strategy);
        let mut stages = vec![PlanStage::SelectBackend, PlanStage::LoadCapture];
        stages.extend(load_artifact_stages(&derived_artifacts));
        stages.push(PlanStage::LoadDomainFlags);
        stages.push(PlanStage::GenerateCandidates {
            strategy: candidate_strategy,
        });
        stages.push(PlanStage::PruneCandidates {
            strategy: pruning_strategy,
        });
        if let Some(ParticipantContractKind::Radiance) = descriptor.participant_kind {
            stages.push(PlanStage::SelectParticipants {
                kind: CaptureQueryKind::Radiance,
            });
        }
        if let Some(ParticipantContractKind::Medium) = descriptor.participant_kind {
            stages.push(PlanStage::SelectParticipants {
                kind: CaptureQueryKind::Medium,
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
            match binding.planner_recipe {
                QueryPlannerRecipeKind::SurfaceSampleWorld => WinnerSelectionMode::SurfaceReuse,
                QueryPlannerRecipeKind::ParticipantsRadianceWorld
                | QueryPlannerRecipeKind::ParticipantsMediumWorld => WinnerSelectionMode::Ordered,
                QueryPlannerRecipeKind::SpatialDistanceWorld
                | QueryPlannerRecipeKind::SpatialNormalWorld
                | QueryPlannerRecipeKind::SpatialTraceWorld => WinnerSelectionMode::Nearest,
                other => panic!("unexpected world planner recipe: {other:?}"),
            },
            true,
        );
        Self {
            contract_version: QUERY_PLAN_CONTRACT_VERSION,
            contract_id,
            family: descriptor.family,
            surface: descriptor.surface,
            helper_name: SmolStr::new(helper_name),
            kind,
            backend,
            result_kind,
            executor,
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
                dispatch_contract.item_kind,
                result_kind,
                preserves_local_hit_context,
                helper_name,
            ),
            observability: planning_observability(descriptor.observability, pruning_strategy),
            preserves_local_hit_context,
        }
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
        artifact_sizes: profile.artifact_sizes,
        dispatch_overhead: profile.dispatch_overhead,
    }
}

fn world_candidate_strategy(kind: WorldQueryKind) -> CandidateStrategy {
    match kind {
        WorldQueryKind::Surface => CandidateStrategy::SurfaceHitReuse,
        WorldQueryKind::Radiance | WorldQueryKind::Medium => {
            CandidateStrategy::ShapeBranchTraversal
        }
        WorldQueryKind::Distance | WorldQueryKind::Normal | WorldQueryKind::Trace => {
            CandidateStrategy::SupportAcceleratedShapeTraversal
        }
    }
}

fn world_pruning_strategy(
    kind: WorldQueryKind,
    candidate_strategy: CandidateStrategy,
) -> PruningStrategy {
    match kind {
        WorldQueryKind::Surface => PruningStrategy::None,
        WorldQueryKind::Radiance | WorldQueryKind::Medium => PruningStrategy::ConservativeTraversal,
        WorldQueryKind::Distance | WorldQueryKind::Normal | WorldQueryKind::Trace => {
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

fn derive_artifact_contracts(
    artifacts: &[DerivedArtifact],
    scene: Option<&SceneSummary>,
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
        CaptureKind::Shape => candidate_strategy_for_shape_query(BatchQueryKind::Trace, scene),
        CaptureKind::Region => CandidateStrategy::OpaqueFallback,
    }
}

fn candidate_strategy_for_shape_query(
    kind: BatchQueryKind,
    scene: Option<&SceneSummary>,
) -> CandidateStrategy {
    match kind {
        BatchQueryKind::Surface => CandidateStrategy::SurfaceHitReuse,
        BatchQueryKind::Trace | BatchQueryKind::Occluded => {
            let Some(scene) = scene else {
                return CandidateStrategy::ShapeBranchTraversal;
            };
            if scene.opaque_boundary {
                CandidateStrategy::OpaqueFallback
            } else if scene.can_coarse_support_pruning
                && matches!(
                    scene.support_class,
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
        BatchQueryKind::Trace
        | BatchQueryKind::Occluded
        | BatchQueryKind::Distance
        | BatchQueryKind::Normal => {
            let Some(scene) = scene else {
                return PruningStrategy::ConservativeTraversal;
            };
            if matches!(
                candidate_strategy,
                CandidateStrategy::SupportAcceleratedShapeTraversal
            ) {
                return PruningStrategy::CullingTable;
            }
            if matches!(candidate_strategy, CandidateStrategy::ShapeBranchTraversal)
                && scene.can_coarse_support_pruning
                && matches!(scene.support_class, SupportClass::Bounded)
            {
                return PruningStrategy::SupportLowerBound;
            }
            PruningStrategy::ConservativeTraversal
        }
        BatchQueryKind::Surface => PruningStrategy::None,
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
