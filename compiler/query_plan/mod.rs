use self::DispatchBackend::{Auto, Cpu, VirtualGpu};
use self::PlanExecutor::{
    FieldDistanceCapture, FieldNormalCapture, SceneMediumCapture, SceneRadianceCapture,
    SceneSurfaceCapture, SceneTraceCapture, ShapeDistanceCapture, ShapeNormalCapture,
    WorldDistanceCapture, WorldMediumCapture, WorldNormalCapture, WorldRadianceCapture,
    WorldSurfaceCapture, WorldTraceCapture,
};
use crate::scene_ir::{DistanceSemantics, SceneCaptureKind, SupportClass};
use smol_str::SmolStr;

pub type CaptureKind = SceneCaptureKind;

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
    Auto,
}

impl DispatchBackend {
    pub fn from_id(id: i32) -> Option<Self> {
        match id {
            0 => Some(Cpu),
            1 => Some(VirtualGpu),
            2 => Some(Auto),
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneSummary {
    pub name: Option<SmolStr>,
    pub semantics: DistanceSemantics,
    pub support_class: SupportClass,
    pub can_coarse_support_pruning: bool,
    pub opaque_boundary: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchQueryPlan {
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
    pub preserves_local_hit_context: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureQueryPlan {
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
    pub preserves_local_hit_context: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldQueryPlan {
    pub helper_name: SmolStr,
    pub kind: WorldQueryKind,
    pub result_kind: QueryResultKind,
    pub executor: PlanExecutor,
    pub stages: Vec<PlanStage>,
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
        if matches!(backend, VirtualGpu | Auto) {
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
        if matches!(backend, VirtualGpu | Auto) {
            stages.push(PlanStage::EndVirtualGpuDispatch);
        }
        Self {
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
        if matches!(backend, VirtualGpu | Auto) {
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
        if matches!(backend, VirtualGpu | Auto) {
            stages.push(PlanStage::EndVirtualGpuDispatch);
        }
        Self {
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
            preserves_local_hit_context,
        }
    }

    pub fn requires_virtual_gpu_scaffolding(&self) -> bool {
        self.stages
            .iter()
            .any(|stage| matches!(stage, PlanStage::BeginVirtualGpuDispatch))
    }

    pub fn candidate_strategy(&self) -> CandidateStrategy {
        self.candidate_strategy
    }

    pub fn pruning_strategy(&self) -> PruningStrategy {
        self.pruning_strategy
    }

    pub fn requests_culling_table(&self) -> bool {
        self.derived_artifacts
            .iter()
            .any(|artifact| matches!(artifact, DerivedArtifact::CullingTable { .. }))
    }

    pub fn has_opaque_pessimization_boundary(&self) -> bool {
        self.derived_artifacts
            .iter()
            .any(|artifact| matches!(artifact, DerivedArtifact::OpaquePessimizationBoundary))
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
        Ok(Self {
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
            preserves_local_hit_context,
        })
    }
}

impl CaptureQueryPlan {
    pub fn candidate_strategy(&self) -> CandidateStrategy {
        self.candidate_strategy
    }

    pub fn pruning_strategy(&self) -> PruningStrategy {
        self.pruning_strategy
    }

    pub fn requests_culling_table(&self) -> bool {
        self.derived_artifacts
            .iter()
            .any(|artifact| matches!(artifact, DerivedArtifact::CullingTable { .. }))
    }

    pub fn has_opaque_pessimization_boundary(&self) -> bool {
        self.derived_artifacts
            .iter()
            .any(|artifact| matches!(artifact, DerivedArtifact::OpaquePessimizationBoundary))
    }
}

impl WorldQueryPlan {
    pub fn for_query(kind: WorldQueryKind) -> Self {
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
        Self {
            helper_name: SmolStr::new(helper_name),
            kind,
            result_kind,
            executor,
            stages,
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
