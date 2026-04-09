use self::DispatchBackend::{Auto, Cpu, VirtualGpu, Wgsl};
use self::PlanExecutor::{
    FieldDistanceCapture, FieldNormalCapture, SceneMediumCapture, SceneRadianceCapture,
    SceneSurfaceCapture, SceneTraceCapture, ShapeDistanceCapture, ShapeNormalCapture,
    WorldDistanceCapture, WorldMediumCapture, WorldNormalCapture, WorldRadianceCapture,
    WorldSurfaceCapture, WorldTraceCapture,
};
use crate::scene_ir::{DistanceSemantics, SceneCaptureKind, SupportClass};
use smol_str::SmolStr;

pub type CaptureKind = SceneCaptureKind;
pub const QUERY_PLAN_CONTRACT_VERSION: u32 = 1;

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
pub enum DispatchBackend {
    Cpu,
    VirtualGpu,
    Wgsl,
    Auto,
}

impl DispatchBackend {
    pub fn from_id(id: i32) -> Option<Self> {
        match id {
            0 => Some(Cpu),
            1 => Some(VirtualGpu),
            2 => Some(Wgsl),
            3 => Some(Auto),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum InternalKernelKind {
    CaptureUpdate,
    FieldDistanceCapture,
    ShapeDistanceCapture,
    FieldNormalCapture,
    ShapeNormalCapture,
    ShapeTraceCapture,
    ShapeSurfaceCapture,
    ShapeOccludedCapture,
    SceneRadianceCapture,
    SceneMediumCapture,
    WorldDistanceCapture,
    WorldNormalCapture,
    WorldTraceCapture,
    WorldSurfaceCapture,
    WorldRadianceCapture,
    WorldMediumCapture,
    Culling,
    Bake,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum QueryItemKind {
    PointQuery,
    RayQuery,
    Hit3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum QueryResultKind {
    DistanceResult,
    NormalResult,
    Hit3,
    Surface,
    OcclusionResult,
    RadianceResult,
    MediumResult,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PlanExecutor {
    FieldDistanceCapture,
    ShapeDistanceCapture,
    FieldNormalCapture,
    ShapeNormalCapture,
    SceneTraceCapture,
    SceneSurfaceCapture,
    SceneRadianceCapture,
    SceneMediumCapture,
    WorldDistanceCapture,
    WorldNormalCapture,
    WorldTraceCapture,
    WorldSurfaceCapture,
    WorldRadianceCapture,
    WorldMediumCapture,
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
pub enum SceneDomainFlag {
    Material,
    Radiance,
    Media,
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
    pub helper_name: SmolStr,
    pub kind: WorldQueryKind,
    pub backend: DispatchBackend,
    pub result_kind: QueryResultKind,
    pub executor: PlanExecutor,
    pub stages: Vec<PlanStage>,
    pub dispatch_contract: DispatchRecordContract,
    pub result_contract: ResultRecordContract,
    pub hit_context_contract: Option<HitContextContract>,
    pub participant_contract: Option<ParticipantSelectionContract>,
    pub domain_flags: Vec<SceneDomainFlag>,
    pub artifact_contracts: Vec<ArtifactContract>,
    pub observability: PlanningObservability,
    pub preserves_local_hit_context: bool,
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
        let helper_name = match (kind, capture_kind) {
            (BatchQueryKind::Distance, CaptureKind::Field) => "__wr_field_distance_batch_queries",
            (BatchQueryKind::Distance, CaptureKind::Shape) => "__wr_shape_distance_batch_queries",
            (BatchQueryKind::Normal, CaptureKind::Field) => "__wr_field_normal_batch_queries",
            (BatchQueryKind::Normal, CaptureKind::Shape) => "__wr_shape_normal_batch_queries",
            _ => panic!("unsupported field batch query: {kind:?} on {capture_kind:?}"),
        };
        let (kernel, result_kind, executor) = match (kind, capture_kind) {
            (BatchQueryKind::Distance, CaptureKind::Field) => (
                InternalKernelKind::FieldDistanceCapture,
                QueryResultKind::DistanceResult,
                FieldDistanceCapture,
            ),
            (BatchQueryKind::Distance, CaptureKind::Shape) => (
                InternalKernelKind::ShapeDistanceCapture,
                QueryResultKind::DistanceResult,
                ShapeDistanceCapture,
            ),
            (BatchQueryKind::Normal, CaptureKind::Field) => (
                InternalKernelKind::FieldNormalCapture,
                QueryResultKind::NormalResult,
                FieldNormalCapture,
            ),
            (BatchQueryKind::Normal, CaptureKind::Shape) => (
                InternalKernelKind::ShapeNormalCapture,
                QueryResultKind::NormalResult,
                ShapeNormalCapture,
            ),
            _ => panic!("unsupported field batch query: {kind:?} on {capture_kind:?}"),
        };
        let candidate_strategy = candidate_strategy_for_field_query(capture_kind, scene.as_ref());
        let pruning_strategy =
            pruning_strategy_for_plan(kind, capture_kind, scene.as_ref(), candidate_strategy);
        let derived_artifacts = derive_artifacts(
            scene.as_ref(),
            capture_kind,
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
            item_kind: QueryItemKind::PointQuery,
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
            QueryItemKind::PointQuery,
            candidate_strategy,
            pruning_strategy,
            WinnerSelectionMode::Nearest,
            true,
        );
        let result_contract = build_result_contract(result_kind, false);
        let artifact_contracts = derive_artifact_contracts(
            &derived_artifacts,
            scene.as_ref(),
            QueryItemKind::PointQuery,
            result_kind,
            false,
            helper_name,
        );
        let item_contract = BatchItemContract::CaptureQuery {
            plan: CaptureQueryPlan::for_query(
                match kind {
                    BatchQueryKind::Distance => CaptureQueryKind::Distance,
                    BatchQueryKind::Normal => CaptureQueryKind::Normal,
                    _ => unreachable!(),
                },
                capture_kind,
                scene.clone(),
            )
            .expect("field batch item contract"),
        };
        Self {
            contract_version: QUERY_PLAN_CONTRACT_VERSION,
            helper_name: SmolStr::new(helper_name),
            kind,
            capture_kind,
            backend,
            kernel,
            item_kind: QueryItemKind::PointQuery,
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
                item_kind: QueryItemKind::PointQuery,
                result_kind,
            },
            candidate_contract,
            result_contract,
            hit_context_contract: None,
            participant_contract: None,
            domain_flags: Vec::new(),
            artifact_contracts,
            item_contract,
            observability: default_planning_observability(pruning_strategy),
            preserves_local_hit_context: false,
        }
    }

    pub fn for_shape_query(
        kind: BatchQueryKind,
        backend: DispatchBackend,
        scene: Option<SceneSummary>,
    ) -> Self {
        let helper_name = match kind {
            BatchQueryKind::Trace => "__wr_scene_trace_batch_queries",
            BatchQueryKind::Surface => "__wr_scene_surface_batch_queries",
            BatchQueryKind::Occluded => "__wr_scene_occluded_batch_queries",
            _ => panic!("unsupported shape batch query: {kind:?}"),
        };
        let (kernel, item_kind, result_kind, executor, preserves_local_hit_context) = match kind {
            BatchQueryKind::Trace => (
                InternalKernelKind::ShapeTraceCapture,
                QueryItemKind::RayQuery,
                QueryResultKind::Hit3,
                SceneTraceCapture,
                true,
            ),
            BatchQueryKind::Surface => (
                InternalKernelKind::ShapeSurfaceCapture,
                QueryItemKind::Hit3,
                QueryResultKind::Surface,
                SceneSurfaceCapture,
                false,
            ),
            BatchQueryKind::Occluded => (
                InternalKernelKind::ShapeOccludedCapture,
                QueryItemKind::RayQuery,
                QueryResultKind::OcclusionResult,
                SceneTraceCapture,
                true,
            ),
            _ => panic!("unsupported shape batch query: {kind:?}"),
        };
        let candidate_strategy = candidate_strategy_for_shape_query(kind, scene.as_ref());
        let pruning_strategy =
            pruning_strategy_for_plan(kind, CaptureKind::Shape, scene.as_ref(), candidate_strategy);
        let derived_artifacts = derive_artifacts(
            scene.as_ref(),
            CaptureKind::Shape,
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
        let winner_mode = match kind {
            BatchQueryKind::Surface => WinnerSelectionMode::SurfaceReuse,
            BatchQueryKind::Trace | BatchQueryKind::Occluded => WinnerSelectionMode::Nearest,
            _ => WinnerSelectionMode::None,
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
        let item_contract = match kind {
            BatchQueryKind::Trace => BatchItemContract::CaptureQuery {
                plan: CaptureQueryPlan::for_query(
                    CaptureQueryKind::Trace,
                    CaptureKind::Shape,
                    scene.clone(),
                )
                .expect("trace batch contract"),
            },
            BatchQueryKind::Surface => BatchItemContract::CaptureQuery {
                plan: CaptureQueryPlan::for_query(
                    CaptureQueryKind::Surface,
                    CaptureKind::Shape,
                    scene.clone(),
                )
                .expect("surface batch contract"),
            },
            BatchQueryKind::Occluded => BatchItemContract::TraceThenOcclusion {
                trace_plan: CaptureQueryPlan::for_query(
                    CaptureQueryKind::Trace,
                    CaptureKind::Shape,
                    scene.clone(),
                )
                .expect("occlusion trace contract"),
            },
            _ => unreachable!(),
        };
        Self {
            contract_version: QUERY_PLAN_CONTRACT_VERSION,
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
            domain_flags: Vec::new(),
            artifact_contracts,
            item_contract,
            observability: default_planning_observability(pruning_strategy),
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
        let (helper_name, result_kind, executor, preserves_local_hit_context) =
            match (kind, capture_kind) {
                (CaptureQueryKind::Distance, CaptureKind::Field) => (
                    "__wr_field_distance_capture",
                    QueryResultKind::DistanceResult,
                    FieldDistanceCapture,
                    false,
                ),
                (CaptureQueryKind::Distance, CaptureKind::Shape) => (
                    "__wr_shape_distance_capture",
                    QueryResultKind::DistanceResult,
                    ShapeDistanceCapture,
                    false,
                ),
                (CaptureQueryKind::Normal, CaptureKind::Field) => (
                    "__wr_field_normal_capture",
                    QueryResultKind::NormalResult,
                    FieldNormalCapture,
                    false,
                ),
                (CaptureQueryKind::Normal, CaptureKind::Shape) => (
                    "__wr_shape_normal_capture",
                    QueryResultKind::NormalResult,
                    ShapeNormalCapture,
                    false,
                ),
                (CaptureQueryKind::Radiance, CaptureKind::Shape) => (
                    "__wr_scene_radiance_capture",
                    QueryResultKind::RadianceResult,
                    SceneRadianceCapture,
                    false,
                ),
                (CaptureQueryKind::Medium, CaptureKind::Shape) => (
                    "__wr_scene_medium_capture",
                    QueryResultKind::MediumResult,
                    SceneMediumCapture,
                    false,
                ),
                (CaptureQueryKind::Trace, CaptureKind::Shape) => (
                    "__wr_scene_trace_capture",
                    QueryResultKind::Hit3,
                    SceneTraceCapture,
                    true,
                ),
                (CaptureQueryKind::Surface, CaptureKind::Shape) => (
                    "__wr_scene_surface_capture",
                    QueryResultKind::Surface,
                    SceneSurfaceCapture,
                    false,
                ),
                _ => {
                    return Err(
                        "capture query plan does not support the requested capture/kind pair",
                    );
                }
            };
        let candidate_strategy = match kind {
            CaptureQueryKind::Surface => CandidateStrategy::SurfaceHitReuse,
            CaptureQueryKind::Distance | CaptureQueryKind::Normal
                if matches!(capture_kind, CaptureKind::Field) =>
            {
                CandidateStrategy::DirectFieldCapture
            }
            _ => candidate_strategy_for_shape_query(BatchQueryKind::Trace, scene.as_ref()),
        };
        let pruning_strategy = match kind {
            CaptureQueryKind::Surface => PruningStrategy::None,
            CaptureQueryKind::Distance | CaptureQueryKind::Normal
                if matches!(capture_kind, CaptureKind::Field) =>
            {
                PruningStrategy::None
            }
            _ => pruning_strategy_for_plan(
                BatchQueryKind::Trace,
                capture_kind,
                scene.as_ref(),
                candidate_strategy,
            ),
        };
        let derived_artifacts = derive_artifacts(
            scene.as_ref(),
            capture_kind,
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
        if matches!(kind, CaptureQueryKind::Radiance | CaptureQueryKind::Medium) {
            stages.push(PlanStage::SelectParticipants { kind });
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
            match kind {
                CaptureQueryKind::Surface => QueryItemKind::Hit3,
                CaptureQueryKind::Trace => QueryItemKind::RayQuery,
                _ => QueryItemKind::PointQuery,
            },
            candidate_strategy,
            pruning_strategy,
            match kind {
                CaptureQueryKind::Surface => WinnerSelectionMode::SurfaceReuse,
                CaptureQueryKind::Trace => WinnerSelectionMode::Nearest,
                _ => WinnerSelectionMode::None,
            },
            true,
        );
        let result_contract = build_result_contract(result_kind, preserves_local_hit_context);
        let hit_context_contract = preserves_local_hit_context.then(hit_context_contract);
        let participant_contract = match kind {
            CaptureQueryKind::Radiance | CaptureQueryKind::Medium => {
                Some(build_participant_contract(kind))
            }
            _ => None,
        };
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
            observability: default_planning_observability(pruning_strategy),
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
        let (helper_name, result_kind, executor, preserves_local_hit_context) = match kind {
            WorldQueryKind::Distance => (
                "__wr_world_distance_capture",
                QueryResultKind::DistanceResult,
                WorldDistanceCapture,
                false,
            ),
            WorldQueryKind::Normal => (
                "__wr_world_normal_capture",
                QueryResultKind::NormalResult,
                WorldNormalCapture,
                false,
            ),
            WorldQueryKind::Radiance => (
                "__wr_world_radiance_capture",
                QueryResultKind::RadianceResult,
                WorldRadianceCapture,
                false,
            ),
            WorldQueryKind::Medium => (
                "__wr_world_medium_capture",
                QueryResultKind::MediumResult,
                WorldMediumCapture,
                false,
            ),
            WorldQueryKind::Trace => (
                "__wr_world_trace_capture",
                QueryResultKind::Hit3,
                WorldTraceCapture,
                true,
            ),
            WorldQueryKind::Surface => (
                "__wr_world_surface_capture",
                QueryResultKind::Surface,
                WorldSurfaceCapture,
                false,
            ),
        };
        let mut stages = vec![PlanStage::LoadDomainFlags];
        if matches!(kind, WorldQueryKind::Radiance) {
            stages.push(PlanStage::SelectParticipants {
                kind: CaptureQueryKind::Radiance,
            });
        }
        if matches!(kind, WorldQueryKind::Medium) {
            stages.push(PlanStage::SelectParticipants {
                kind: CaptureQueryKind::Medium,
            });
        }
        stages.push(PlanStage::Execute { executor });
        if preserves_local_hit_context {
            stages.push(PlanStage::AssembleHitContext);
        }
        stages.push(PlanStage::AppendResult { result_kind });
        let participant_contract = match kind {
            WorldQueryKind::Radiance => {
                Some(build_participant_contract(CaptureQueryKind::Radiance))
            }
            WorldQueryKind::Medium => Some(build_participant_contract(CaptureQueryKind::Medium)),
            _ => None,
        };
        let domain_flags = world_domain_flags(kind);
        Self {
            contract_version: QUERY_PLAN_CONTRACT_VERSION,
            helper_name: SmolStr::new(helper_name),
            kind,
            backend,
            result_kind,
            executor,
            stages,
            dispatch_contract: DispatchRecordContract {
                backend,
                kernel: match kind {
                    WorldQueryKind::Distance => InternalKernelKind::WorldDistanceCapture,
                    WorldQueryKind::Normal => InternalKernelKind::WorldNormalCapture,
                    WorldQueryKind::Trace => InternalKernelKind::WorldTraceCapture,
                    WorldQueryKind::Surface => InternalKernelKind::WorldSurfaceCapture,
                    WorldQueryKind::Radiance => InternalKernelKind::WorldRadianceCapture,
                    WorldQueryKind::Medium => InternalKernelKind::WorldMediumCapture,
                },
                item_kind: match kind {
                    WorldQueryKind::Surface => QueryItemKind::Hit3,
                    WorldQueryKind::Trace => QueryItemKind::RayQuery,
                    _ => QueryItemKind::PointQuery,
                },
                result_kind,
            },
            result_contract: build_result_contract(result_kind, preserves_local_hit_context),
            hit_context_contract: preserves_local_hit_context.then(hit_context_contract),
            participant_contract,
            domain_flags: domain_flags.clone(),
            artifact_contracts: vec![
                ArtifactContract {
                    id: SmolStr::new(format!("{}::dispatch", helper_name)),
                    schema: ArtifactSchema::DispatchRecord {
                        item_kind: match kind {
                            WorldQueryKind::Surface => QueryItemKind::Hit3,
                            WorldQueryKind::Trace => QueryItemKind::RayQuery,
                            _ => QueryItemKind::PointQuery,
                        },
                        result_kind,
                    },
                    producer: SmolStr::new(helper_name),
                    consumer: SmolStr::new(helper_name),
                    deterministic: true,
                    version: QUERY_PLAN_CONTRACT_VERSION,
                },
                ArtifactContract {
                    id: SmolStr::new(format!("{}::result", helper_name)),
                    schema: ArtifactSchema::HitResultBuffer {
                        result_kind,
                        preserves_local_hit_context,
                    },
                    producer: SmolStr::new(helper_name),
                    consumer: SmolStr::new(helper_name),
                    deterministic: true,
                    version: QUERY_PLAN_CONTRACT_VERSION,
                },
            ],
            observability: default_planning_observability(PruningStrategy::None),
            preserves_local_hit_context,
        }
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

fn build_participant_contract(kind: CaptureQueryKind) -> ParticipantSelectionContract {
    ParticipantSelectionContract {
        kind,
        provenance_aware: true,
        additive: matches!(kind, CaptureQueryKind::Radiance | CaptureQueryKind::Medium),
    }
}

fn world_domain_flags(kind: WorldQueryKind) -> Vec<SceneDomainFlag> {
    match kind {
        WorldQueryKind::Surface => vec![SceneDomainFlag::Material],
        WorldQueryKind::Radiance => vec![SceneDomainFlag::Radiance],
        WorldQueryKind::Medium => vec![SceneDomainFlag::Media],
        _ => Vec::new(),
    }
}

fn default_planning_observability(pruning_strategy: PruningStrategy) -> PlanningObservability {
    PlanningObservability {
        candidate_count: true,
        branch_visits: true,
        support_prune_effectiveness: !matches!(pruning_strategy, PruningStrategy::None),
        culling_hit_rate: matches!(pruning_strategy, PruningStrategy::CullingTable),
        artifact_sizes: true,
        dispatch_overhead: true,
    }
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
        assert!(!plan.preserves_local_hit_context);
        assert_eq!(
            plan.candidate_strategy(),
            CandidateStrategy::DirectFieldCapture
        );
        assert_eq!(plan.pruning_strategy(), PruningStrategy::None);
    }
}
