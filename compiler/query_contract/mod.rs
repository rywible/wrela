pub use crate::execution_policy::{
    QueryExecutionPolicy, RayBudgetPolicy, RequiredGuaranteeClass, SelectedMethodClass,
};
use crate::scene_ir::SceneCaptureKind;
use std::fmt;

pub type CaptureKind = SceneCaptureKind;
pub const QUERY_CONTRACT_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DispatchBackend {
    Cpu,
    VirtualGpu,
    Wgsl,
    Auto,
}

impl DispatchBackend {
    pub fn from_id(id: i32) -> Option<Self> {
        match id {
            0 => Some(Self::Cpu),
            1 => Some(Self::VirtualGpu),
            2 => Some(Self::Wgsl),
            3 => Some(Self::Auto),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum InternalKernelKind {
    CaptureUpdate,
    FieldDistanceCapture,
    ShapeDistanceCapture,
    FieldNormalCapture,
    ShapeNormalCapture,
    FieldSupportSummaryCapture,
    ShapeSupportSummaryCapture,
    ShapeTraceCapture,
    ShapeSurfaceCapture,
    ShapeOccludedCapture,
    SceneRadianceCapture,
    SceneMediumCapture,
    WorldDistanceCapture,
    WorldNormalCapture,
    WorldSupportSummaryCapture,
    WorldTraceCapture,
    WorldSurfaceCapture,
    WorldRadianceCapture,
    WorldMediumCapture,
    Culling,
    Bake,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum QueryItemKind {
    Unit,
    PointQuery,
    PointDirectionQuery,
    RayQuery,
    Hit3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum QueryResultKind {
    DistanceResult,
    NormalResult,
    SupportSummaryResult,
    Hit3,
    Surface,
    OcclusionResult,
    RadianceResult,
    MediumResult,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PlanExecutor {
    FieldDistanceCapture,
    ShapeDistanceCapture,
    FieldNormalCapture,
    ShapeNormalCapture,
    FieldSupportSummaryCapture,
    ShapeSupportSummaryCapture,
    SceneTraceCapture,
    SceneSurfaceCapture,
    SceneRadianceCapture,
    SceneMediumCapture,
    WorldDistanceCapture,
    WorldNormalCapture,
    WorldSupportSummaryCapture,
    WorldTraceCapture,
    WorldSurfaceCapture,
    WorldRadianceCapture,
    WorldMediumCapture,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SceneDomainFlag {
    Material,
    Radiance,
    Media,
}

pub fn scene_domain_flag_name(flag: SceneDomainFlag) -> &'static str {
    match flag {
        SceneDomainFlag::Material => "material",
        SceneDomainFlag::Radiance => "radiance",
        SceneDomainFlag::Media => "media",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct QueryContractId(&'static str);

impl QueryContractId {
    pub const fn new(value: &'static str) -> Self {
        Self(value)
    }

    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

impl fmt::Display for QueryContractId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum QueryFamilyId {
    Spatial,
    Surface,
    Participants,
    Support,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum QueryQuestionId {
    Distance,
    Normal,
    Nearest,
    /// Compatibility-only question id for legacy trace contract aliases.
    Trace,
    Sample,
    Radiance,
    Medium,
    Occluded,
    Summary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum QueryTargetKind {
    Capture,
    World,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum QueryCardinality {
    Scalar,
    Batch,
}

/// Compatibility adapter for older compiler layers that still talk about a
/// combined query surface. Public contract reporting should prefer the
/// orthogonal `QueryTargetKind` and `QueryCardinality` axes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum QuerySurfaceKind {
    CaptureScalar,
    WorldScalar,
    CaptureBatch,
    WorldBatch,
}

impl QuerySurfaceKind {
    pub const fn from_axes(target: QueryTargetKind, cardinality: QueryCardinality) -> Self {
        match (target, cardinality) {
            (QueryTargetKind::Capture, QueryCardinality::Scalar) => Self::CaptureScalar,
            (QueryTargetKind::World, QueryCardinality::Scalar) => Self::WorldScalar,
            (QueryTargetKind::Capture, QueryCardinality::Batch) => Self::CaptureBatch,
            (QueryTargetKind::World, QueryCardinality::Batch) => Self::WorldBatch,
        }
    }

    pub const fn target(self) -> QueryTargetKind {
        match self {
            Self::CaptureScalar | Self::CaptureBatch => QueryTargetKind::Capture,
            Self::WorldScalar | Self::WorldBatch => QueryTargetKind::World,
        }
    }

    pub const fn cardinality(self) -> QueryCardinality {
        match self {
            Self::CaptureScalar | Self::WorldScalar => QueryCardinality::Scalar,
            Self::CaptureBatch | Self::WorldBatch => QueryCardinality::Batch,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum QueryFamilyCallSurface {
    Scalar,
    Batch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueryFamilyMember {
    pub family: QueryFamilyId,
    pub question: QueryQuestionId,
    pub call_surface: QueryFamilyCallSurface,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DomainContractKind {
    SceneDomain,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ParticipantContractKind {
    Radiance,
    Medium,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum QueryPlannerRecipeKind {
    SpatialDistanceCaptureField,
    SpatialDistanceCaptureShape,
    SpatialDistanceWorld,
    SpatialDistanceBatchField,
    SpatialDistanceBatchShape,
    SpatialDistanceBatchWorld,
    SpatialNormalCaptureField,
    SpatialNormalCaptureShape,
    SpatialNormalWorld,
    SpatialNormalBatchField,
    SpatialNormalBatchShape,
    SpatialNormalBatchWorld,
    SupportSummaryCaptureField,
    SupportSummaryCaptureShape,
    SupportSummaryWorld,
    SpatialNearestCaptureShape,
    SpatialNearestBatchShape,
    SpatialNearestBatchWorld,
    SpatialNearestWorld,
    SpatialTraceCaptureShape,
    SpatialTraceBatchShape,
    SpatialTraceWorld,
    SpatialOccludedCaptureShape,
    SpatialOccludedBatchShape,
    SpatialOccludedBatchWorld,
    SpatialOccludedWorld,
    SurfaceSampleCaptureShape,
    SurfaceSampleBatchShape,
    SurfaceSampleBatchWorld,
    SurfaceSampleWorld,
    ParticipantsRadianceCaptureShape,
    ParticipantsRadianceWorld,
    ParticipantsRadianceBatchWorld,
    ParticipantsMediumCaptureShape,
    ParticipantsMediumWorld,
    ParticipantsMediumBatchWorld,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackendSupport {
    pub cpu: bool,
    pub virtual_gpu: bool,
    pub wgsl: bool,
}

impl BackendSupport {
    pub const fn all() -> Self {
        Self {
            cpu: true,
            virtual_gpu: true,
            wgsl: true,
        }
    }

    pub const fn cpu_and_virtual_gpu() -> Self {
        Self {
            cpu: true,
            virtual_gpu: true,
            wgsl: false,
        }
    }

    pub const fn supports(self, backend: DispatchBackend) -> bool {
        match backend {
            DispatchBackend::Cpu => self.cpu,
            DispatchBackend::VirtualGpu => self.virtual_gpu,
            DispatchBackend::Wgsl => self.wgsl,
            DispatchBackend::Auto => self.cpu || self.virtual_gpu || self.wgsl,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueryObservabilityProfile {
    pub candidate_count: bool,
    pub branch_visits: bool,
    pub support_prune_effectiveness: bool,
    pub culling_hit_rate: bool,
    pub trace_steps: bool,
    pub field_samples: bool,
    pub artifact_sizes: bool,
    pub dispatch_overhead: bool,
}

impl QueryObservabilityProfile {
    pub const fn support_summary() -> Self {
        Self {
            candidate_count: false,
            branch_visits: false,
            support_prune_effectiveness: false,
            culling_hit_rate: false,
            trace_steps: false,
            field_samples: false,
            artifact_sizes: true,
            dispatch_overhead: true,
        }
    }

    pub const fn spatial_point() -> Self {
        Self {
            candidate_count: true,
            branch_visits: true,
            support_prune_effectiveness: true,
            culling_hit_rate: true,
            trace_steps: false,
            field_samples: true,
            artifact_sizes: true,
            dispatch_overhead: true,
        }
    }

    pub const fn spatial_ray() -> Self {
        Self {
            candidate_count: true,
            branch_visits: true,
            support_prune_effectiveness: true,
            culling_hit_rate: true,
            trace_steps: true,
            field_samples: true,
            artifact_sizes: true,
            dispatch_overhead: true,
        }
    }

    pub const fn point_sample() -> Self {
        Self {
            candidate_count: true,
            branch_visits: true,
            support_prune_effectiveness: false,
            culling_hit_rate: false,
            trace_steps: false,
            field_samples: true,
            artifact_sizes: true,
            dispatch_overhead: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueryContractDescriptor {
    pub id: QueryContractId,
    pub version: u32,
    pub family: QueryFamilyId,
    pub question: QueryQuestionId,
    pub target: QueryTargetKind,
    pub cardinality: QueryCardinality,
    pub surface: QuerySurfaceKind,
    pub capture_kind: CaptureKind,
    pub item_kind: QueryItemKind,
    pub result_kind: QueryResultKind,
    pub domain_contract: Option<DomainContractKind>,
    pub required_domain_flags: &'static [SceneDomainFlag],
    pub preserves_local_hit_context: bool,
    pub participant_kind: Option<ParticipantContractKind>,
    pub supported_backends: BackendSupport,
    pub observability: QueryObservabilityProfile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct QueryExecutionBinding {
    pub contract_id: QueryContractId,
    pub planner_recipe: QueryPlannerRecipeKind,
    pub default_executor: PlanExecutor,
    pub default_kernel: Option<InternalKernelKind>,
    pub helper_name: Option<&'static str>,
    pub legacy_builtin_name: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueryContractAlias {
    pub alias_id: QueryContractId,
    pub canonical_id: QueryContractId,
    pub reason: &'static str,
}

pub const SPATIAL_DISTANCE_CAPTURE_FIELD: QueryContractId =
    QueryContractId::new("spatial.distance.capture.field");
pub const SPATIAL_DISTANCE_CAPTURE_SHAPE: QueryContractId =
    QueryContractId::new("spatial.distance.capture.shape");
pub const SPATIAL_DISTANCE_WORLD: QueryContractId = QueryContractId::new("spatial.distance.world");
pub const SPATIAL_DISTANCE_BATCH_FIELD: QueryContractId =
    QueryContractId::new("spatial.distance.batch.field");
pub const SPATIAL_DISTANCE_BATCH_SHAPE: QueryContractId =
    QueryContractId::new("spatial.distance.batch.shape");
pub const SPATIAL_DISTANCE_BATCH_WORLD: QueryContractId =
    QueryContractId::new("spatial.distance.batch.world");
pub const SPATIAL_NORMAL_CAPTURE_FIELD: QueryContractId =
    QueryContractId::new("spatial.normal.capture.field");
pub const SPATIAL_NORMAL_CAPTURE_SHAPE: QueryContractId =
    QueryContractId::new("spatial.normal.capture.shape");
pub const SPATIAL_NORMAL_WORLD: QueryContractId = QueryContractId::new("spatial.normal.world");
pub const SPATIAL_NORMAL_BATCH_FIELD: QueryContractId =
    QueryContractId::new("spatial.normal.batch.field");
pub const SPATIAL_NORMAL_BATCH_SHAPE: QueryContractId =
    QueryContractId::new("spatial.normal.batch.shape");
pub const SPATIAL_NORMAL_BATCH_WORLD: QueryContractId =
    QueryContractId::new("spatial.normal.batch.world");
pub const SUPPORT_SUMMARY_CAPTURE_FIELD: QueryContractId =
    QueryContractId::new("support.summary.capture.field");
pub const SUPPORT_SUMMARY_CAPTURE_SHAPE: QueryContractId =
    QueryContractId::new("support.summary.capture.shape");
pub const SUPPORT_SUMMARY_WORLD: QueryContractId = QueryContractId::new("support.summary.world");
pub const SPATIAL_NEAREST_CAPTURE_SHAPE: QueryContractId =
    QueryContractId::new("spatial.nearest.capture.shape");
pub const SPATIAL_NEAREST_BATCH_SHAPE: QueryContractId =
    QueryContractId::new("spatial.nearest.batch.shape");
pub const SPATIAL_NEAREST_BATCH_WORLD: QueryContractId =
    QueryContractId::new("spatial.nearest.batch.world");
pub const SPATIAL_NEAREST_WORLD: QueryContractId = QueryContractId::new("spatial.nearest.world");
pub const LEGACY_SPATIAL_TRACE_CAPTURE_SHAPE: QueryContractId =
    QueryContractId::new("spatial.trace.capture.shape");
pub const LEGACY_SPATIAL_TRACE_BATCH_SHAPE: QueryContractId =
    QueryContractId::new("spatial.trace.batch.shape");
pub const LEGACY_SPATIAL_TRACE_BATCH_WORLD: QueryContractId =
    QueryContractId::new("spatial.trace.batch.world");
pub const LEGACY_SPATIAL_TRACE_WORLD: QueryContractId = QueryContractId::new("spatial.trace.world");
pub const SPATIAL_TRACE_CAPTURE_SHAPE: QueryContractId = SPATIAL_NEAREST_CAPTURE_SHAPE;
pub const SPATIAL_TRACE_BATCH_SHAPE: QueryContractId = SPATIAL_NEAREST_BATCH_SHAPE;
pub const SPATIAL_TRACE_BATCH_WORLD: QueryContractId = SPATIAL_NEAREST_BATCH_WORLD;
pub const SPATIAL_TRACE_WORLD: QueryContractId = SPATIAL_NEAREST_WORLD;
pub const SPATIAL_OCCLUDED_CAPTURE_SHAPE: QueryContractId =
    QueryContractId::new("spatial.occluded.capture.shape");
pub const SPATIAL_OCCLUDED_BATCH_SHAPE: QueryContractId =
    QueryContractId::new("spatial.occluded.batch.shape");
pub const SPATIAL_OCCLUDED_BATCH_WORLD: QueryContractId =
    QueryContractId::new("spatial.occluded.batch.world");
pub const SPATIAL_OCCLUDED_WORLD: QueryContractId = QueryContractId::new("spatial.occluded.world");
pub const SURFACE_SAMPLE_CAPTURE_SHAPE: QueryContractId =
    QueryContractId::new("surface.sample.capture.shape");
pub const SURFACE_SAMPLE_BATCH_SHAPE: QueryContractId =
    QueryContractId::new("surface.sample.batch.shape");
pub const SURFACE_SAMPLE_BATCH_WORLD: QueryContractId =
    QueryContractId::new("surface.sample.batch.world");
pub const SURFACE_SAMPLE_WORLD: QueryContractId = QueryContractId::new("surface.sample.world");
pub const PARTICIPANTS_RADIANCE_CAPTURE_SHAPE: QueryContractId =
    QueryContractId::new("participants.radiance.capture.shape");
pub const PARTICIPANTS_RADIANCE_WORLD: QueryContractId =
    QueryContractId::new("participants.radiance.world");
pub const PARTICIPANTS_RADIANCE_BATCH_WORLD: QueryContractId =
    QueryContractId::new("participants.radiance.batch.world");
pub const PARTICIPANTS_MEDIUM_CAPTURE_SHAPE: QueryContractId =
    QueryContractId::new("participants.medium.capture.shape");
pub const PARTICIPANTS_MEDIUM_WORLD: QueryContractId =
    QueryContractId::new("participants.medium.world");
pub const PARTICIPANTS_MEDIUM_BATCH_WORLD: QueryContractId =
    QueryContractId::new("participants.medium.batch.world");

const NO_DOMAIN_FLAGS: &[SceneDomainFlag] = &[];
const MATERIAL_DOMAIN_FLAGS: &[SceneDomainFlag] = &[SceneDomainFlag::Material];
const RADIANCE_DOMAIN_FLAGS: &[SceneDomainFlag] = &[SceneDomainFlag::Radiance];
const MEDIA_DOMAIN_FLAGS: &[SceneDomainFlag] = &[SceneDomainFlag::Media];

const QUERY_CONTRACTS: [QueryContractDescriptor; 33] = [
    QueryContractDescriptor {
        id: SPATIAL_DISTANCE_CAPTURE_FIELD,
        version: QUERY_CONTRACT_VERSION,
        family: QueryFamilyId::Spatial,
        question: QueryQuestionId::Distance,
        target: QueryTargetKind::Capture,
        cardinality: QueryCardinality::Scalar,
        surface: QuerySurfaceKind::CaptureScalar,
        capture_kind: CaptureKind::Field,
        item_kind: QueryItemKind::PointQuery,
        result_kind: QueryResultKind::DistanceResult,
        domain_contract: None,
        required_domain_flags: NO_DOMAIN_FLAGS,
        preserves_local_hit_context: false,
        participant_kind: None,
        supported_backends: BackendSupport::all(),
        observability: QueryObservabilityProfile::spatial_point(),
    },
    QueryContractDescriptor {
        id: SPATIAL_DISTANCE_CAPTURE_SHAPE,
        version: QUERY_CONTRACT_VERSION,
        family: QueryFamilyId::Spatial,
        question: QueryQuestionId::Distance,
        target: QueryTargetKind::Capture,
        cardinality: QueryCardinality::Scalar,
        surface: QuerySurfaceKind::CaptureScalar,
        capture_kind: CaptureKind::Shape,
        item_kind: QueryItemKind::PointQuery,
        result_kind: QueryResultKind::DistanceResult,
        domain_contract: None,
        required_domain_flags: NO_DOMAIN_FLAGS,
        preserves_local_hit_context: false,
        participant_kind: None,
        supported_backends: BackendSupport::all(),
        observability: QueryObservabilityProfile::spatial_point(),
    },
    QueryContractDescriptor {
        id: SPATIAL_DISTANCE_WORLD,
        version: QUERY_CONTRACT_VERSION,
        family: QueryFamilyId::Spatial,
        question: QueryQuestionId::Distance,
        target: QueryTargetKind::World,
        cardinality: QueryCardinality::Scalar,
        surface: QuerySurfaceKind::WorldScalar,
        capture_kind: CaptureKind::Region,
        item_kind: QueryItemKind::PointQuery,
        result_kind: QueryResultKind::DistanceResult,
        domain_contract: Some(DomainContractKind::SceneDomain),
        required_domain_flags: NO_DOMAIN_FLAGS,
        preserves_local_hit_context: false,
        participant_kind: None,
        supported_backends: BackendSupport::all(),
        observability: QueryObservabilityProfile::spatial_point(),
    },
    QueryContractDescriptor {
        id: SPATIAL_DISTANCE_BATCH_FIELD,
        version: QUERY_CONTRACT_VERSION,
        family: QueryFamilyId::Spatial,
        question: QueryQuestionId::Distance,
        target: QueryTargetKind::Capture,
        cardinality: QueryCardinality::Batch,
        surface: QuerySurfaceKind::CaptureBatch,
        capture_kind: CaptureKind::Field,
        item_kind: QueryItemKind::PointQuery,
        result_kind: QueryResultKind::DistanceResult,
        domain_contract: None,
        required_domain_flags: NO_DOMAIN_FLAGS,
        preserves_local_hit_context: false,
        participant_kind: None,
        supported_backends: BackendSupport::all(),
        observability: QueryObservabilityProfile::spatial_point(),
    },
    QueryContractDescriptor {
        id: SPATIAL_DISTANCE_BATCH_SHAPE,
        version: QUERY_CONTRACT_VERSION,
        family: QueryFamilyId::Spatial,
        question: QueryQuestionId::Distance,
        target: QueryTargetKind::Capture,
        cardinality: QueryCardinality::Batch,
        surface: QuerySurfaceKind::CaptureBatch,
        capture_kind: CaptureKind::Shape,
        item_kind: QueryItemKind::PointQuery,
        result_kind: QueryResultKind::DistanceResult,
        domain_contract: None,
        required_domain_flags: NO_DOMAIN_FLAGS,
        preserves_local_hit_context: false,
        participant_kind: None,
        supported_backends: BackendSupport::all(),
        observability: QueryObservabilityProfile::spatial_point(),
    },
    QueryContractDescriptor {
        id: SPATIAL_DISTANCE_BATCH_WORLD,
        version: QUERY_CONTRACT_VERSION,
        family: QueryFamilyId::Spatial,
        question: QueryQuestionId::Distance,
        target: QueryTargetKind::World,
        cardinality: QueryCardinality::Batch,
        surface: QuerySurfaceKind::WorldBatch,
        capture_kind: CaptureKind::Region,
        item_kind: QueryItemKind::PointQuery,
        result_kind: QueryResultKind::DistanceResult,
        domain_contract: Some(DomainContractKind::SceneDomain),
        required_domain_flags: NO_DOMAIN_FLAGS,
        preserves_local_hit_context: false,
        participant_kind: None,
        supported_backends: BackendSupport::all(),
        observability: QueryObservabilityProfile::spatial_point(),
    },
    QueryContractDescriptor {
        id: SPATIAL_NORMAL_CAPTURE_FIELD,
        version: QUERY_CONTRACT_VERSION,
        family: QueryFamilyId::Spatial,
        question: QueryQuestionId::Normal,
        target: QueryTargetKind::Capture,
        cardinality: QueryCardinality::Scalar,
        surface: QuerySurfaceKind::CaptureScalar,
        capture_kind: CaptureKind::Field,
        item_kind: QueryItemKind::PointQuery,
        result_kind: QueryResultKind::NormalResult,
        domain_contract: None,
        required_domain_flags: NO_DOMAIN_FLAGS,
        preserves_local_hit_context: false,
        participant_kind: None,
        supported_backends: BackendSupport::all(),
        observability: QueryObservabilityProfile::spatial_point(),
    },
    QueryContractDescriptor {
        id: SPATIAL_NORMAL_CAPTURE_SHAPE,
        version: QUERY_CONTRACT_VERSION,
        family: QueryFamilyId::Spatial,
        question: QueryQuestionId::Normal,
        target: QueryTargetKind::Capture,
        cardinality: QueryCardinality::Scalar,
        surface: QuerySurfaceKind::CaptureScalar,
        capture_kind: CaptureKind::Shape,
        item_kind: QueryItemKind::PointQuery,
        result_kind: QueryResultKind::NormalResult,
        domain_contract: None,
        required_domain_flags: NO_DOMAIN_FLAGS,
        preserves_local_hit_context: false,
        participant_kind: None,
        supported_backends: BackendSupport::all(),
        observability: QueryObservabilityProfile::spatial_point(),
    },
    QueryContractDescriptor {
        id: SPATIAL_NORMAL_WORLD,
        version: QUERY_CONTRACT_VERSION,
        family: QueryFamilyId::Spatial,
        question: QueryQuestionId::Normal,
        target: QueryTargetKind::World,
        cardinality: QueryCardinality::Scalar,
        surface: QuerySurfaceKind::WorldScalar,
        capture_kind: CaptureKind::Region,
        item_kind: QueryItemKind::PointQuery,
        result_kind: QueryResultKind::NormalResult,
        domain_contract: Some(DomainContractKind::SceneDomain),
        required_domain_flags: NO_DOMAIN_FLAGS,
        preserves_local_hit_context: false,
        participant_kind: None,
        supported_backends: BackendSupport::all(),
        observability: QueryObservabilityProfile::spatial_point(),
    },
    QueryContractDescriptor {
        id: SPATIAL_NORMAL_BATCH_FIELD,
        version: QUERY_CONTRACT_VERSION,
        family: QueryFamilyId::Spatial,
        question: QueryQuestionId::Normal,
        target: QueryTargetKind::Capture,
        cardinality: QueryCardinality::Batch,
        surface: QuerySurfaceKind::CaptureBatch,
        capture_kind: CaptureKind::Field,
        item_kind: QueryItemKind::PointQuery,
        result_kind: QueryResultKind::NormalResult,
        domain_contract: None,
        required_domain_flags: NO_DOMAIN_FLAGS,
        preserves_local_hit_context: false,
        participant_kind: None,
        supported_backends: BackendSupport::all(),
        observability: QueryObservabilityProfile::spatial_point(),
    },
    QueryContractDescriptor {
        id: SPATIAL_NORMAL_BATCH_SHAPE,
        version: QUERY_CONTRACT_VERSION,
        family: QueryFamilyId::Spatial,
        question: QueryQuestionId::Normal,
        target: QueryTargetKind::Capture,
        cardinality: QueryCardinality::Batch,
        surface: QuerySurfaceKind::CaptureBatch,
        capture_kind: CaptureKind::Shape,
        item_kind: QueryItemKind::PointQuery,
        result_kind: QueryResultKind::NormalResult,
        domain_contract: None,
        required_domain_flags: NO_DOMAIN_FLAGS,
        preserves_local_hit_context: false,
        participant_kind: None,
        supported_backends: BackendSupport::all(),
        observability: QueryObservabilityProfile::spatial_point(),
    },
    QueryContractDescriptor {
        id: SPATIAL_NORMAL_BATCH_WORLD,
        version: QUERY_CONTRACT_VERSION,
        family: QueryFamilyId::Spatial,
        question: QueryQuestionId::Normal,
        target: QueryTargetKind::World,
        cardinality: QueryCardinality::Batch,
        surface: QuerySurfaceKind::WorldBatch,
        capture_kind: CaptureKind::Region,
        item_kind: QueryItemKind::PointQuery,
        result_kind: QueryResultKind::NormalResult,
        domain_contract: Some(DomainContractKind::SceneDomain),
        required_domain_flags: NO_DOMAIN_FLAGS,
        preserves_local_hit_context: false,
        participant_kind: None,
        supported_backends: BackendSupport::all(),
        observability: QueryObservabilityProfile::spatial_point(),
    },
    QueryContractDescriptor {
        id: SUPPORT_SUMMARY_CAPTURE_FIELD,
        version: QUERY_CONTRACT_VERSION,
        family: QueryFamilyId::Support,
        question: QueryQuestionId::Summary,
        target: QueryTargetKind::Capture,
        cardinality: QueryCardinality::Scalar,
        surface: QuerySurfaceKind::CaptureScalar,
        capture_kind: CaptureKind::Field,
        item_kind: QueryItemKind::Unit,
        result_kind: QueryResultKind::SupportSummaryResult,
        domain_contract: None,
        required_domain_flags: NO_DOMAIN_FLAGS,
        preserves_local_hit_context: false,
        participant_kind: None,
        supported_backends: BackendSupport::cpu_and_virtual_gpu(),
        observability: QueryObservabilityProfile::support_summary(),
    },
    QueryContractDescriptor {
        id: SUPPORT_SUMMARY_CAPTURE_SHAPE,
        version: QUERY_CONTRACT_VERSION,
        family: QueryFamilyId::Support,
        question: QueryQuestionId::Summary,
        target: QueryTargetKind::Capture,
        cardinality: QueryCardinality::Scalar,
        surface: QuerySurfaceKind::CaptureScalar,
        capture_kind: CaptureKind::Shape,
        item_kind: QueryItemKind::Unit,
        result_kind: QueryResultKind::SupportSummaryResult,
        domain_contract: None,
        required_domain_flags: NO_DOMAIN_FLAGS,
        preserves_local_hit_context: false,
        participant_kind: None,
        supported_backends: BackendSupport::cpu_and_virtual_gpu(),
        observability: QueryObservabilityProfile::support_summary(),
    },
    QueryContractDescriptor {
        id: SUPPORT_SUMMARY_WORLD,
        version: QUERY_CONTRACT_VERSION,
        family: QueryFamilyId::Support,
        question: QueryQuestionId::Summary,
        target: QueryTargetKind::World,
        cardinality: QueryCardinality::Scalar,
        surface: QuerySurfaceKind::WorldScalar,
        capture_kind: CaptureKind::Region,
        item_kind: QueryItemKind::Unit,
        result_kind: QueryResultKind::SupportSummaryResult,
        domain_contract: Some(DomainContractKind::SceneDomain),
        required_domain_flags: NO_DOMAIN_FLAGS,
        preserves_local_hit_context: false,
        participant_kind: None,
        supported_backends: BackendSupport::cpu_and_virtual_gpu(),
        observability: QueryObservabilityProfile::support_summary(),
    },
    QueryContractDescriptor {
        id: SPATIAL_NEAREST_CAPTURE_SHAPE,
        version: QUERY_CONTRACT_VERSION,
        family: QueryFamilyId::Spatial,
        question: QueryQuestionId::Nearest,
        target: QueryTargetKind::Capture,
        cardinality: QueryCardinality::Scalar,
        surface: QuerySurfaceKind::CaptureScalar,
        capture_kind: CaptureKind::Shape,
        item_kind: QueryItemKind::RayQuery,
        result_kind: QueryResultKind::Hit3,
        domain_contract: None,
        required_domain_flags: NO_DOMAIN_FLAGS,
        preserves_local_hit_context: true,
        participant_kind: None,
        supported_backends: BackendSupport::all(),
        observability: QueryObservabilityProfile::spatial_ray(),
    },
    QueryContractDescriptor {
        id: SPATIAL_NEAREST_BATCH_SHAPE,
        version: QUERY_CONTRACT_VERSION,
        family: QueryFamilyId::Spatial,
        question: QueryQuestionId::Nearest,
        target: QueryTargetKind::Capture,
        cardinality: QueryCardinality::Batch,
        surface: QuerySurfaceKind::CaptureBatch,
        capture_kind: CaptureKind::Shape,
        item_kind: QueryItemKind::RayQuery,
        result_kind: QueryResultKind::Hit3,
        domain_contract: None,
        required_domain_flags: NO_DOMAIN_FLAGS,
        preserves_local_hit_context: true,
        participant_kind: None,
        supported_backends: BackendSupport::all(),
        observability: QueryObservabilityProfile::spatial_ray(),
    },
    QueryContractDescriptor {
        id: SPATIAL_NEAREST_BATCH_WORLD,
        version: QUERY_CONTRACT_VERSION,
        family: QueryFamilyId::Spatial,
        question: QueryQuestionId::Nearest,
        target: QueryTargetKind::World,
        cardinality: QueryCardinality::Batch,
        surface: QuerySurfaceKind::WorldBatch,
        capture_kind: CaptureKind::Region,
        item_kind: QueryItemKind::RayQuery,
        result_kind: QueryResultKind::Hit3,
        domain_contract: Some(DomainContractKind::SceneDomain),
        required_domain_flags: NO_DOMAIN_FLAGS,
        preserves_local_hit_context: true,
        participant_kind: None,
        supported_backends: BackendSupport::all(),
        observability: QueryObservabilityProfile::spatial_ray(),
    },
    QueryContractDescriptor {
        id: SPATIAL_NEAREST_WORLD,
        version: QUERY_CONTRACT_VERSION,
        family: QueryFamilyId::Spatial,
        question: QueryQuestionId::Nearest,
        target: QueryTargetKind::World,
        cardinality: QueryCardinality::Scalar,
        surface: QuerySurfaceKind::WorldScalar,
        capture_kind: CaptureKind::Region,
        item_kind: QueryItemKind::RayQuery,
        result_kind: QueryResultKind::Hit3,
        domain_contract: Some(DomainContractKind::SceneDomain),
        required_domain_flags: NO_DOMAIN_FLAGS,
        preserves_local_hit_context: true,
        participant_kind: None,
        supported_backends: BackendSupport::all(),
        observability: QueryObservabilityProfile::spatial_ray(),
    },
    QueryContractDescriptor {
        id: SPATIAL_OCCLUDED_CAPTURE_SHAPE,
        version: QUERY_CONTRACT_VERSION,
        family: QueryFamilyId::Spatial,
        question: QueryQuestionId::Occluded,
        target: QueryTargetKind::Capture,
        cardinality: QueryCardinality::Scalar,
        surface: QuerySurfaceKind::CaptureScalar,
        capture_kind: CaptureKind::Shape,
        item_kind: QueryItemKind::RayQuery,
        result_kind: QueryResultKind::OcclusionResult,
        domain_contract: None,
        required_domain_flags: NO_DOMAIN_FLAGS,
        preserves_local_hit_context: true,
        participant_kind: None,
        supported_backends: BackendSupport::all(),
        observability: QueryObservabilityProfile::spatial_ray(),
    },
    QueryContractDescriptor {
        id: SPATIAL_OCCLUDED_BATCH_SHAPE,
        version: QUERY_CONTRACT_VERSION,
        family: QueryFamilyId::Spatial,
        question: QueryQuestionId::Occluded,
        target: QueryTargetKind::Capture,
        cardinality: QueryCardinality::Batch,
        surface: QuerySurfaceKind::CaptureBatch,
        capture_kind: CaptureKind::Shape,
        item_kind: QueryItemKind::RayQuery,
        result_kind: QueryResultKind::OcclusionResult,
        domain_contract: None,
        required_domain_flags: NO_DOMAIN_FLAGS,
        preserves_local_hit_context: true,
        participant_kind: None,
        supported_backends: BackendSupport::all(),
        observability: QueryObservabilityProfile::spatial_ray(),
    },
    QueryContractDescriptor {
        id: SPATIAL_OCCLUDED_BATCH_WORLD,
        version: QUERY_CONTRACT_VERSION,
        family: QueryFamilyId::Spatial,
        question: QueryQuestionId::Occluded,
        target: QueryTargetKind::World,
        cardinality: QueryCardinality::Batch,
        surface: QuerySurfaceKind::WorldBatch,
        capture_kind: CaptureKind::Region,
        item_kind: QueryItemKind::RayQuery,
        result_kind: QueryResultKind::OcclusionResult,
        domain_contract: Some(DomainContractKind::SceneDomain),
        required_domain_flags: NO_DOMAIN_FLAGS,
        preserves_local_hit_context: true,
        participant_kind: None,
        supported_backends: BackendSupport::all(),
        observability: QueryObservabilityProfile::spatial_ray(),
    },
    QueryContractDescriptor {
        id: SPATIAL_OCCLUDED_WORLD,
        version: QUERY_CONTRACT_VERSION,
        family: QueryFamilyId::Spatial,
        question: QueryQuestionId::Occluded,
        target: QueryTargetKind::World,
        cardinality: QueryCardinality::Scalar,
        surface: QuerySurfaceKind::WorldScalar,
        capture_kind: CaptureKind::Region,
        item_kind: QueryItemKind::RayQuery,
        result_kind: QueryResultKind::OcclusionResult,
        domain_contract: Some(DomainContractKind::SceneDomain),
        required_domain_flags: NO_DOMAIN_FLAGS,
        preserves_local_hit_context: true,
        participant_kind: None,
        supported_backends: BackendSupport::all(),
        observability: QueryObservabilityProfile::spatial_ray(),
    },
    QueryContractDescriptor {
        id: SURFACE_SAMPLE_CAPTURE_SHAPE,
        version: QUERY_CONTRACT_VERSION,
        family: QueryFamilyId::Surface,
        question: QueryQuestionId::Sample,
        target: QueryTargetKind::Capture,
        cardinality: QueryCardinality::Scalar,
        surface: QuerySurfaceKind::CaptureScalar,
        capture_kind: CaptureKind::Shape,
        item_kind: QueryItemKind::Hit3,
        result_kind: QueryResultKind::Surface,
        domain_contract: None,
        required_domain_flags: NO_DOMAIN_FLAGS,
        preserves_local_hit_context: false,
        participant_kind: None,
        supported_backends: BackendSupport::all(),
        observability: QueryObservabilityProfile::point_sample(),
    },
    QueryContractDescriptor {
        id: SURFACE_SAMPLE_BATCH_SHAPE,
        version: QUERY_CONTRACT_VERSION,
        family: QueryFamilyId::Surface,
        question: QueryQuestionId::Sample,
        target: QueryTargetKind::Capture,
        cardinality: QueryCardinality::Batch,
        surface: QuerySurfaceKind::CaptureBatch,
        capture_kind: CaptureKind::Shape,
        item_kind: QueryItemKind::Hit3,
        result_kind: QueryResultKind::Surface,
        domain_contract: None,
        required_domain_flags: NO_DOMAIN_FLAGS,
        preserves_local_hit_context: false,
        participant_kind: None,
        supported_backends: BackendSupport::all(),
        observability: QueryObservabilityProfile::point_sample(),
    },
    QueryContractDescriptor {
        id: SURFACE_SAMPLE_BATCH_WORLD,
        version: QUERY_CONTRACT_VERSION,
        family: QueryFamilyId::Surface,
        question: QueryQuestionId::Sample,
        target: QueryTargetKind::World,
        cardinality: QueryCardinality::Batch,
        surface: QuerySurfaceKind::WorldBatch,
        capture_kind: CaptureKind::Region,
        item_kind: QueryItemKind::Hit3,
        result_kind: QueryResultKind::Surface,
        domain_contract: Some(DomainContractKind::SceneDomain),
        required_domain_flags: MATERIAL_DOMAIN_FLAGS,
        preserves_local_hit_context: false,
        participant_kind: None,
        supported_backends: BackendSupport::all(),
        observability: QueryObservabilityProfile::point_sample(),
    },
    QueryContractDescriptor {
        id: SURFACE_SAMPLE_WORLD,
        version: QUERY_CONTRACT_VERSION,
        family: QueryFamilyId::Surface,
        question: QueryQuestionId::Sample,
        target: QueryTargetKind::World,
        cardinality: QueryCardinality::Scalar,
        surface: QuerySurfaceKind::WorldScalar,
        capture_kind: CaptureKind::Region,
        item_kind: QueryItemKind::Hit3,
        result_kind: QueryResultKind::Surface,
        domain_contract: Some(DomainContractKind::SceneDomain),
        required_domain_flags: MATERIAL_DOMAIN_FLAGS,
        preserves_local_hit_context: false,
        participant_kind: None,
        supported_backends: BackendSupport::all(),
        observability: QueryObservabilityProfile::point_sample(),
    },
    QueryContractDescriptor {
        id: PARTICIPANTS_RADIANCE_CAPTURE_SHAPE,
        version: QUERY_CONTRACT_VERSION,
        family: QueryFamilyId::Participants,
        question: QueryQuestionId::Radiance,
        target: QueryTargetKind::Capture,
        cardinality: QueryCardinality::Scalar,
        surface: QuerySurfaceKind::CaptureScalar,
        capture_kind: CaptureKind::Shape,
        item_kind: QueryItemKind::PointDirectionQuery,
        result_kind: QueryResultKind::RadianceResult,
        domain_contract: None,
        required_domain_flags: NO_DOMAIN_FLAGS,
        preserves_local_hit_context: false,
        participant_kind: Some(ParticipantContractKind::Radiance),
        supported_backends: BackendSupport::all(),
        observability: QueryObservabilityProfile::spatial_point(),
    },
    QueryContractDescriptor {
        id: PARTICIPANTS_RADIANCE_WORLD,
        version: QUERY_CONTRACT_VERSION,
        family: QueryFamilyId::Participants,
        question: QueryQuestionId::Radiance,
        target: QueryTargetKind::World,
        cardinality: QueryCardinality::Scalar,
        surface: QuerySurfaceKind::WorldScalar,
        capture_kind: CaptureKind::Region,
        item_kind: QueryItemKind::PointDirectionQuery,
        result_kind: QueryResultKind::RadianceResult,
        domain_contract: Some(DomainContractKind::SceneDomain),
        required_domain_flags: RADIANCE_DOMAIN_FLAGS,
        preserves_local_hit_context: false,
        participant_kind: Some(ParticipantContractKind::Radiance),
        supported_backends: BackendSupport::all(),
        observability: QueryObservabilityProfile::spatial_point(),
    },
    QueryContractDescriptor {
        id: PARTICIPANTS_RADIANCE_BATCH_WORLD,
        version: QUERY_CONTRACT_VERSION,
        family: QueryFamilyId::Participants,
        question: QueryQuestionId::Radiance,
        target: QueryTargetKind::World,
        cardinality: QueryCardinality::Batch,
        surface: QuerySurfaceKind::WorldBatch,
        capture_kind: CaptureKind::Region,
        item_kind: QueryItemKind::PointDirectionQuery,
        result_kind: QueryResultKind::RadianceResult,
        domain_contract: Some(DomainContractKind::SceneDomain),
        required_domain_flags: RADIANCE_DOMAIN_FLAGS,
        preserves_local_hit_context: false,
        participant_kind: Some(ParticipantContractKind::Radiance),
        supported_backends: BackendSupport::all(),
        observability: QueryObservabilityProfile::spatial_point(),
    },
    QueryContractDescriptor {
        id: PARTICIPANTS_MEDIUM_CAPTURE_SHAPE,
        version: QUERY_CONTRACT_VERSION,
        family: QueryFamilyId::Participants,
        question: QueryQuestionId::Medium,
        target: QueryTargetKind::Capture,
        cardinality: QueryCardinality::Scalar,
        surface: QuerySurfaceKind::CaptureScalar,
        capture_kind: CaptureKind::Shape,
        item_kind: QueryItemKind::PointQuery,
        result_kind: QueryResultKind::MediumResult,
        domain_contract: None,
        required_domain_flags: NO_DOMAIN_FLAGS,
        preserves_local_hit_context: false,
        participant_kind: Some(ParticipantContractKind::Medium),
        supported_backends: BackendSupport::all(),
        observability: QueryObservabilityProfile::spatial_point(),
    },
    QueryContractDescriptor {
        id: PARTICIPANTS_MEDIUM_WORLD,
        version: QUERY_CONTRACT_VERSION,
        family: QueryFamilyId::Participants,
        question: QueryQuestionId::Medium,
        target: QueryTargetKind::World,
        cardinality: QueryCardinality::Scalar,
        surface: QuerySurfaceKind::WorldScalar,
        capture_kind: CaptureKind::Region,
        item_kind: QueryItemKind::PointQuery,
        result_kind: QueryResultKind::MediumResult,
        domain_contract: Some(DomainContractKind::SceneDomain),
        required_domain_flags: MEDIA_DOMAIN_FLAGS,
        preserves_local_hit_context: false,
        participant_kind: Some(ParticipantContractKind::Medium),
        supported_backends: BackendSupport::all(),
        observability: QueryObservabilityProfile::spatial_point(),
    },
    QueryContractDescriptor {
        id: PARTICIPANTS_MEDIUM_BATCH_WORLD,
        version: QUERY_CONTRACT_VERSION,
        family: QueryFamilyId::Participants,
        question: QueryQuestionId::Medium,
        target: QueryTargetKind::World,
        cardinality: QueryCardinality::Batch,
        surface: QuerySurfaceKind::WorldBatch,
        capture_kind: CaptureKind::Region,
        item_kind: QueryItemKind::PointQuery,
        result_kind: QueryResultKind::MediumResult,
        domain_contract: Some(DomainContractKind::SceneDomain),
        required_domain_flags: MEDIA_DOMAIN_FLAGS,
        preserves_local_hit_context: false,
        participant_kind: Some(ParticipantContractKind::Medium),
        supported_backends: BackendSupport::all(),
        observability: QueryObservabilityProfile::spatial_point(),
    },
];

const QUERY_EXECUTION_BINDINGS: [QueryExecutionBinding; 33] = [
    QueryExecutionBinding {
        contract_id: SPATIAL_DISTANCE_CAPTURE_FIELD,
        planner_recipe: QueryPlannerRecipeKind::SpatialDistanceCaptureField,
        default_executor: PlanExecutor::FieldDistanceCapture,
        default_kernel: Some(InternalKernelKind::FieldDistanceCapture),
        helper_name: Some("__wr_field_distance_capture"),
        legacy_builtin_name: "distance_at",
    },
    QueryExecutionBinding {
        contract_id: SPATIAL_DISTANCE_CAPTURE_SHAPE,
        planner_recipe: QueryPlannerRecipeKind::SpatialDistanceCaptureShape,
        default_executor: PlanExecutor::ShapeDistanceCapture,
        default_kernel: Some(InternalKernelKind::ShapeDistanceCapture),
        helper_name: Some("__wr_shape_distance_capture"),
        legacy_builtin_name: "distance_at",
    },
    QueryExecutionBinding {
        contract_id: SPATIAL_DISTANCE_WORLD,
        planner_recipe: QueryPlannerRecipeKind::SpatialDistanceWorld,
        default_executor: PlanExecutor::WorldDistanceCapture,
        default_kernel: Some(InternalKernelKind::WorldDistanceCapture),
        helper_name: Some("__wr_world_distance_capture"),
        legacy_builtin_name: "distance_world",
    },
    QueryExecutionBinding {
        contract_id: SPATIAL_DISTANCE_BATCH_FIELD,
        planner_recipe: QueryPlannerRecipeKind::SpatialDistanceBatchField,
        default_executor: PlanExecutor::FieldDistanceCapture,
        default_kernel: Some(InternalKernelKind::FieldDistanceCapture),
        helper_name: Some("__wr_field_distance_batch_queries"),
        legacy_builtin_name: "distance_at_batch",
    },
    QueryExecutionBinding {
        contract_id: SPATIAL_DISTANCE_BATCH_SHAPE,
        planner_recipe: QueryPlannerRecipeKind::SpatialDistanceBatchShape,
        default_executor: PlanExecutor::ShapeDistanceCapture,
        default_kernel: Some(InternalKernelKind::ShapeDistanceCapture),
        helper_name: Some("__wr_shape_distance_batch_queries"),
        legacy_builtin_name: "distance_at_batch",
    },
    QueryExecutionBinding {
        contract_id: SPATIAL_DISTANCE_BATCH_WORLD,
        planner_recipe: QueryPlannerRecipeKind::SpatialDistanceBatchWorld,
        default_executor: PlanExecutor::WorldDistanceCapture,
        default_kernel: Some(InternalKernelKind::WorldDistanceCapture),
        helper_name: Some("__wr_world_distance_batch_queries"),
        legacy_builtin_name: "distance_world_batch",
    },
    QueryExecutionBinding {
        contract_id: SPATIAL_NORMAL_CAPTURE_FIELD,
        planner_recipe: QueryPlannerRecipeKind::SpatialNormalCaptureField,
        default_executor: PlanExecutor::FieldNormalCapture,
        default_kernel: Some(InternalKernelKind::FieldNormalCapture),
        helper_name: Some("__wr_field_normal_capture"),
        legacy_builtin_name: "normal_at",
    },
    QueryExecutionBinding {
        contract_id: SPATIAL_NORMAL_CAPTURE_SHAPE,
        planner_recipe: QueryPlannerRecipeKind::SpatialNormalCaptureShape,
        default_executor: PlanExecutor::ShapeNormalCapture,
        default_kernel: Some(InternalKernelKind::ShapeNormalCapture),
        helper_name: Some("__wr_shape_normal_capture"),
        legacy_builtin_name: "normal_at",
    },
    QueryExecutionBinding {
        contract_id: SPATIAL_NORMAL_WORLD,
        planner_recipe: QueryPlannerRecipeKind::SpatialNormalWorld,
        default_executor: PlanExecutor::WorldNormalCapture,
        default_kernel: Some(InternalKernelKind::WorldNormalCapture),
        helper_name: Some("__wr_world_normal_capture"),
        legacy_builtin_name: "normal_world",
    },
    QueryExecutionBinding {
        contract_id: SPATIAL_NORMAL_BATCH_FIELD,
        planner_recipe: QueryPlannerRecipeKind::SpatialNormalBatchField,
        default_executor: PlanExecutor::FieldNormalCapture,
        default_kernel: Some(InternalKernelKind::FieldNormalCapture),
        helper_name: Some("__wr_field_normal_batch_queries"),
        legacy_builtin_name: "normal_at_batch",
    },
    QueryExecutionBinding {
        contract_id: SPATIAL_NORMAL_BATCH_SHAPE,
        planner_recipe: QueryPlannerRecipeKind::SpatialNormalBatchShape,
        default_executor: PlanExecutor::ShapeNormalCapture,
        default_kernel: Some(InternalKernelKind::ShapeNormalCapture),
        helper_name: Some("__wr_shape_normal_batch_queries"),
        legacy_builtin_name: "normal_at_batch",
    },
    QueryExecutionBinding {
        contract_id: SPATIAL_NORMAL_BATCH_WORLD,
        planner_recipe: QueryPlannerRecipeKind::SpatialNormalBatchWorld,
        default_executor: PlanExecutor::WorldNormalCapture,
        default_kernel: Some(InternalKernelKind::WorldNormalCapture),
        helper_name: Some("__wr_world_normal_batch_queries"),
        legacy_builtin_name: "normal_world_batch",
    },
    QueryExecutionBinding {
        contract_id: SUPPORT_SUMMARY_CAPTURE_FIELD,
        planner_recipe: QueryPlannerRecipeKind::SupportSummaryCaptureField,
        default_executor: PlanExecutor::FieldSupportSummaryCapture,
        default_kernel: Some(InternalKernelKind::FieldSupportSummaryCapture),
        helper_name: Some("__wr_field_support_summary_capture"),
        legacy_builtin_name: "support_summary",
    },
    QueryExecutionBinding {
        contract_id: SUPPORT_SUMMARY_CAPTURE_SHAPE,
        planner_recipe: QueryPlannerRecipeKind::SupportSummaryCaptureShape,
        default_executor: PlanExecutor::ShapeSupportSummaryCapture,
        default_kernel: Some(InternalKernelKind::ShapeSupportSummaryCapture),
        helper_name: Some("__wr_shape_support_summary_capture"),
        legacy_builtin_name: "support_summary",
    },
    QueryExecutionBinding {
        contract_id: SUPPORT_SUMMARY_WORLD,
        planner_recipe: QueryPlannerRecipeKind::SupportSummaryWorld,
        default_executor: PlanExecutor::WorldSupportSummaryCapture,
        default_kernel: Some(InternalKernelKind::WorldSupportSummaryCapture),
        helper_name: Some("__wr_world_support_summary_capture"),
        legacy_builtin_name: "support_summary_world",
    },
    QueryExecutionBinding {
        contract_id: SPATIAL_NEAREST_CAPTURE_SHAPE,
        planner_recipe: QueryPlannerRecipeKind::SpatialNearestCaptureShape,
        default_executor: PlanExecutor::SceneTraceCapture,
        default_kernel: Some(InternalKernelKind::ShapeTraceCapture),
        helper_name: Some("__wr_scene_trace_capture"),
        legacy_builtin_name: "trace_shape",
    },
    QueryExecutionBinding {
        contract_id: SPATIAL_NEAREST_BATCH_SHAPE,
        planner_recipe: QueryPlannerRecipeKind::SpatialNearestBatchShape,
        default_executor: PlanExecutor::SceneTraceCapture,
        default_kernel: Some(InternalKernelKind::ShapeTraceCapture),
        helper_name: Some("__wr_scene_trace_batch_queries"),
        legacy_builtin_name: "trace_shape_batch",
    },
    QueryExecutionBinding {
        contract_id: SPATIAL_NEAREST_BATCH_WORLD,
        planner_recipe: QueryPlannerRecipeKind::SpatialNearestBatchWorld,
        default_executor: PlanExecutor::WorldTraceCapture,
        default_kernel: Some(InternalKernelKind::WorldTraceCapture),
        helper_name: Some("__wr_world_trace_batch_queries"),
        legacy_builtin_name: "trace_world_batch",
    },
    QueryExecutionBinding {
        contract_id: SPATIAL_NEAREST_WORLD,
        planner_recipe: QueryPlannerRecipeKind::SpatialNearestWorld,
        default_executor: PlanExecutor::WorldTraceCapture,
        default_kernel: Some(InternalKernelKind::WorldTraceCapture),
        helper_name: Some("__wr_world_trace_capture"),
        legacy_builtin_name: "trace_world",
    },
    QueryExecutionBinding {
        contract_id: SPATIAL_OCCLUDED_CAPTURE_SHAPE,
        planner_recipe: QueryPlannerRecipeKind::SpatialOccludedCaptureShape,
        default_executor: PlanExecutor::SceneTraceCapture,
        default_kernel: Some(InternalKernelKind::ShapeOccludedCapture),
        helper_name: Some("__wr_scene_occluded_capture"),
        legacy_builtin_name: "occluded",
    },
    QueryExecutionBinding {
        contract_id: SPATIAL_OCCLUDED_BATCH_SHAPE,
        planner_recipe: QueryPlannerRecipeKind::SpatialOccludedBatchShape,
        default_executor: PlanExecutor::SceneTraceCapture,
        default_kernel: Some(InternalKernelKind::ShapeOccludedCapture),
        helper_name: Some("__wr_scene_occluded_batch_queries"),
        legacy_builtin_name: "occluded_batch",
    },
    QueryExecutionBinding {
        contract_id: SPATIAL_OCCLUDED_BATCH_WORLD,
        planner_recipe: QueryPlannerRecipeKind::SpatialOccludedBatchWorld,
        default_executor: PlanExecutor::WorldTraceCapture,
        default_kernel: Some(InternalKernelKind::WorldTraceCapture),
        helper_name: Some("__wr_world_occluded_batch_queries"),
        legacy_builtin_name: "occluded_world_batch",
    },
    QueryExecutionBinding {
        contract_id: SPATIAL_OCCLUDED_WORLD,
        planner_recipe: QueryPlannerRecipeKind::SpatialOccludedWorld,
        default_executor: PlanExecutor::WorldTraceCapture,
        default_kernel: Some(InternalKernelKind::WorldTraceCapture),
        helper_name: Some("__wr_world_occluded_capture"),
        legacy_builtin_name: "occluded_world",
    },
    QueryExecutionBinding {
        contract_id: SURFACE_SAMPLE_CAPTURE_SHAPE,
        planner_recipe: QueryPlannerRecipeKind::SurfaceSampleCaptureShape,
        default_executor: PlanExecutor::SceneSurfaceCapture,
        default_kernel: Some(InternalKernelKind::ShapeSurfaceCapture),
        helper_name: Some("__wr_scene_surface_capture"),
        legacy_builtin_name: "surface_at",
    },
    QueryExecutionBinding {
        contract_id: SURFACE_SAMPLE_BATCH_SHAPE,
        planner_recipe: QueryPlannerRecipeKind::SurfaceSampleBatchShape,
        default_executor: PlanExecutor::SceneSurfaceCapture,
        default_kernel: Some(InternalKernelKind::ShapeSurfaceCapture),
        helper_name: Some("__wr_scene_surface_batch_queries"),
        legacy_builtin_name: "surface_at_batch",
    },
    QueryExecutionBinding {
        contract_id: SURFACE_SAMPLE_BATCH_WORLD,
        planner_recipe: QueryPlannerRecipeKind::SurfaceSampleBatchWorld,
        default_executor: PlanExecutor::WorldSurfaceCapture,
        default_kernel: Some(InternalKernelKind::WorldSurfaceCapture),
        helper_name: Some("__wr_world_surface_batch_queries"),
        legacy_builtin_name: "surface_world_batch",
    },
    QueryExecutionBinding {
        contract_id: SURFACE_SAMPLE_WORLD,
        planner_recipe: QueryPlannerRecipeKind::SurfaceSampleWorld,
        default_executor: PlanExecutor::WorldSurfaceCapture,
        default_kernel: Some(InternalKernelKind::WorldSurfaceCapture),
        helper_name: Some("__wr_world_surface_capture"),
        legacy_builtin_name: "surface_world",
    },
    QueryExecutionBinding {
        contract_id: PARTICIPANTS_RADIANCE_CAPTURE_SHAPE,
        planner_recipe: QueryPlannerRecipeKind::ParticipantsRadianceCaptureShape,
        default_executor: PlanExecutor::SceneRadianceCapture,
        default_kernel: Some(InternalKernelKind::SceneRadianceCapture),
        helper_name: Some("__wr_scene_radiance_capture"),
        legacy_builtin_name: "radiance_at",
    },
    QueryExecutionBinding {
        contract_id: PARTICIPANTS_RADIANCE_WORLD,
        planner_recipe: QueryPlannerRecipeKind::ParticipantsRadianceWorld,
        default_executor: PlanExecutor::WorldRadianceCapture,
        default_kernel: Some(InternalKernelKind::WorldRadianceCapture),
        helper_name: Some("__wr_world_radiance_capture"),
        legacy_builtin_name: "radiance_world",
    },
    QueryExecutionBinding {
        contract_id: PARTICIPANTS_RADIANCE_BATCH_WORLD,
        planner_recipe: QueryPlannerRecipeKind::ParticipantsRadianceBatchWorld,
        default_executor: PlanExecutor::WorldRadianceCapture,
        default_kernel: Some(InternalKernelKind::WorldRadianceCapture),
        helper_name: Some("__wr_world_radiance_batch_queries"),
        legacy_builtin_name: "radiance_world_batch",
    },
    QueryExecutionBinding {
        contract_id: PARTICIPANTS_MEDIUM_CAPTURE_SHAPE,
        planner_recipe: QueryPlannerRecipeKind::ParticipantsMediumCaptureShape,
        default_executor: PlanExecutor::SceneMediumCapture,
        default_kernel: Some(InternalKernelKind::SceneMediumCapture),
        helper_name: Some("__wr_scene_medium_capture"),
        legacy_builtin_name: "medium_at",
    },
    QueryExecutionBinding {
        contract_id: PARTICIPANTS_MEDIUM_WORLD,
        planner_recipe: QueryPlannerRecipeKind::ParticipantsMediumWorld,
        default_executor: PlanExecutor::WorldMediumCapture,
        default_kernel: Some(InternalKernelKind::WorldMediumCapture),
        helper_name: Some("__wr_world_medium_capture"),
        legacy_builtin_name: "medium_world",
    },
    QueryExecutionBinding {
        contract_id: PARTICIPANTS_MEDIUM_BATCH_WORLD,
        planner_recipe: QueryPlannerRecipeKind::ParticipantsMediumBatchWorld,
        default_executor: PlanExecutor::WorldMediumCapture,
        default_kernel: Some(InternalKernelKind::WorldMediumCapture),
        helper_name: Some("__wr_world_medium_batch_queries"),
        legacy_builtin_name: "medium_world_batch",
    },
];

const QUERY_CONTRACT_ALIASES: [QueryContractAlias; 4] = [
    QueryContractAlias {
        alias_id: LEGACY_SPATIAL_TRACE_CAPTURE_SHAPE,
        canonical_id: SPATIAL_NEAREST_CAPTURE_SHAPE,
        reason: "trace is the legacy name for spatial.nearest",
    },
    QueryContractAlias {
        alias_id: LEGACY_SPATIAL_TRACE_BATCH_SHAPE,
        canonical_id: SPATIAL_NEAREST_BATCH_SHAPE,
        reason: "trace is the legacy name for spatial.nearest",
    },
    QueryContractAlias {
        alias_id: LEGACY_SPATIAL_TRACE_BATCH_WORLD,
        canonical_id: SPATIAL_NEAREST_BATCH_WORLD,
        reason: "trace is the legacy name for spatial.nearest",
    },
    QueryContractAlias {
        alias_id: LEGACY_SPATIAL_TRACE_WORLD,
        canonical_id: SPATIAL_NEAREST_WORLD,
        reason: "trace is the legacy name for spatial.nearest",
    },
];

pub fn query_family_namespace(name: &str) -> Option<QueryFamilyId> {
    match name {
        "spatial" => Some(QueryFamilyId::Spatial),
        "surface" => Some(QueryFamilyId::Surface),
        "participants" => Some(QueryFamilyId::Participants),
        "support" => Some(QueryFamilyId::Support),
        _ => None,
    }
}

pub fn query_family_name(family: QueryFamilyId) -> &'static str {
    match family {
        QueryFamilyId::Spatial => "spatial",
        QueryFamilyId::Surface => "surface",
        QueryFamilyId::Participants => "participants",
        QueryFamilyId::Support => "support",
    }
}

pub fn query_question_name(question: QueryQuestionId) -> &'static str {
    match question {
        QueryQuestionId::Distance => "distance",
        QueryQuestionId::Normal => "normal",
        QueryQuestionId::Nearest => "nearest",
        QueryQuestionId::Trace => "trace",
        QueryQuestionId::Sample => "sample",
        QueryQuestionId::Radiance => "radiance",
        QueryQuestionId::Medium => "medium",
        QueryQuestionId::Occluded => "occluded",
        QueryQuestionId::Summary => "summary",
    }
}

pub fn query_surface_name(surface: QuerySurfaceKind) -> &'static str {
    match surface {
        QuerySurfaceKind::CaptureScalar => "capture",
        QuerySurfaceKind::WorldScalar => "world",
        QuerySurfaceKind::CaptureBatch => "batch",
        QuerySurfaceKind::WorldBatch => "batch.world",
    }
}

pub fn query_target_name(target: QueryTargetKind) -> &'static str {
    match target {
        QueryTargetKind::Capture => "capture",
        QueryTargetKind::World => "world",
    }
}

pub fn query_cardinality_name(cardinality: QueryCardinality) -> &'static str {
    match cardinality {
        QueryCardinality::Scalar => "scalar",
        QueryCardinality::Batch => "batch",
    }
}

pub fn query_capture_kind_name(capture_kind: CaptureKind) -> &'static str {
    match capture_kind {
        CaptureKind::Field => "field",
        CaptureKind::Shape => "shape",
        CaptureKind::Region => "region",
    }
}

pub fn query_item_kind_name(kind: QueryItemKind) -> &'static str {
    match kind {
        QueryItemKind::Unit => "unit",
        QueryItemKind::PointQuery => "point",
        QueryItemKind::PointDirectionQuery => "point_direction",
        QueryItemKind::RayQuery => "ray",
        QueryItemKind::Hit3 => "hit",
    }
}

pub fn query_result_kind_name(kind: QueryResultKind) -> &'static str {
    match kind {
        QueryResultKind::DistanceResult => "distance_result",
        QueryResultKind::NormalResult => "normal_result",
        QueryResultKind::SupportSummaryResult => "support_summary_result",
        QueryResultKind::Hit3 => "hit",
        QueryResultKind::Surface => "surface",
        QueryResultKind::OcclusionResult => "occlusion_result",
        QueryResultKind::RadianceResult => "radiance_result",
        QueryResultKind::MediumResult => "medium",
    }
}

pub fn query_backend_support_names(support: BackendSupport) -> Vec<&'static str> {
    let mut names = Vec::new();
    if support.cpu {
        names.push("cpu");
    }
    if support.virtual_gpu {
        names.push("virtual_gpu");
    }
    if support.wgsl {
        names.push("wgsl");
    }
    names
}

pub fn query_family_member(family: QueryFamilyId, member: &str) -> Option<QueryFamilyMember> {
    let (question, call_surface) = match (family, member) {
        (QueryFamilyId::Spatial, "distance") => {
            (QueryQuestionId::Distance, QueryFamilyCallSurface::Scalar)
        }
        (QueryFamilyId::Spatial, "normal") => {
            (QueryQuestionId::Normal, QueryFamilyCallSurface::Scalar)
        }
        (QueryFamilyId::Spatial, "nearest") => {
            (QueryQuestionId::Nearest, QueryFamilyCallSurface::Scalar)
        }
        (QueryFamilyId::Spatial, "occluded") => {
            (QueryQuestionId::Occluded, QueryFamilyCallSurface::Scalar)
        }
        (QueryFamilyId::Surface, "sample") => {
            (QueryQuestionId::Sample, QueryFamilyCallSurface::Scalar)
        }
        (QueryFamilyId::Participants, "radiance") => {
            (QueryQuestionId::Radiance, QueryFamilyCallSurface::Scalar)
        }
        (QueryFamilyId::Participants, "medium") => {
            (QueryQuestionId::Medium, QueryFamilyCallSurface::Scalar)
        }
        (QueryFamilyId::Support, "summary") => {
            (QueryQuestionId::Summary, QueryFamilyCallSurface::Scalar)
        }
        (QueryFamilyId::Spatial, "distance_batch") => {
            (QueryQuestionId::Distance, QueryFamilyCallSurface::Batch)
        }
        (QueryFamilyId::Spatial, "normal_batch") => {
            (QueryQuestionId::Normal, QueryFamilyCallSurface::Batch)
        }
        (QueryFamilyId::Spatial, "nearest_batch") => {
            (QueryQuestionId::Nearest, QueryFamilyCallSurface::Batch)
        }
        (QueryFamilyId::Spatial, "occluded_batch") => {
            (QueryQuestionId::Occluded, QueryFamilyCallSurface::Batch)
        }
        (QueryFamilyId::Surface, "sample_batch") => {
            (QueryQuestionId::Sample, QueryFamilyCallSurface::Batch)
        }
        (QueryFamilyId::Participants, "radiance_batch") => {
            (QueryQuestionId::Radiance, QueryFamilyCallSurface::Batch)
        }
        (QueryFamilyId::Participants, "medium_batch") => {
            (QueryQuestionId::Medium, QueryFamilyCallSurface::Batch)
        }
        _ => return None,
    };
    Some(QueryFamilyMember {
        family,
        question,
        call_surface,
    })
}

pub fn query_family_member_name(descriptor: &QueryContractDescriptor) -> &'static str {
    match (
        descriptor.family,
        descriptor.question,
        descriptor.cardinality,
    ) {
        (QueryFamilyId::Spatial, QueryQuestionId::Distance, QueryCardinality::Batch) => {
            "distance_batch"
        }
        (QueryFamilyId::Spatial, QueryQuestionId::Normal, QueryCardinality::Batch) => {
            "normal_batch"
        }
        (QueryFamilyId::Spatial, QueryQuestionId::Nearest, QueryCardinality::Batch) => {
            "nearest_batch"
        }
        (QueryFamilyId::Spatial, QueryQuestionId::Occluded, QueryCardinality::Batch) => {
            "occluded_batch"
        }
        (QueryFamilyId::Surface, QueryQuestionId::Sample, QueryCardinality::Batch) => {
            "sample_batch"
        }
        (QueryFamilyId::Participants, QueryQuestionId::Radiance, QueryCardinality::Batch) => {
            "radiance_batch"
        }
        (QueryFamilyId::Participants, QueryQuestionId::Medium, QueryCardinality::Batch) => {
            "medium_batch"
        }
        (_, question, _) => query_question_name(question),
    }
}

pub fn query_contract_for_family_member(
    family: QueryFamilyId,
    member: &str,
    surface: QuerySurfaceKind,
    capture_kind: CaptureKind,
) -> Option<&'static QueryContractDescriptor> {
    query_contract_bundle_for_family_member_internal(family, member, surface, capture_kind)
        .map(|(descriptor, _binding)| descriptor)
}

pub(crate) fn query_contract_bundle_for_family_member_internal(
    family: QueryFamilyId,
    member: &str,
    surface: QuerySurfaceKind,
    capture_kind: CaptureKind,
) -> Option<(
    &'static QueryContractDescriptor,
    &'static QueryExecutionBinding,
)> {
    let member = query_family_member(family, member)?;
    if member.call_surface == QueryFamilyCallSurface::Scalar
        && surface.cardinality() == QueryCardinality::Batch
    {
        return None;
    }
    if member.call_surface == QueryFamilyCallSurface::Batch
        && surface.cardinality() != QueryCardinality::Batch
    {
        return None;
    }
    let descriptor = QUERY_CONTRACTS.iter().find(|descriptor| {
        descriptor.family == member.family
            && descriptor.question == member.question
            && descriptor.surface == surface
            && descriptor.capture_kind == capture_kind
    })?;
    Some((descriptor, query_execution_binding(descriptor.id)?))
}

pub fn query_contracts() -> &'static [QueryContractDescriptor] {
    &QUERY_CONTRACTS
}

pub(crate) fn query_execution_bindings() -> &'static [QueryExecutionBinding] {
    &QUERY_EXECUTION_BINDINGS
}

pub fn query_contract_aliases() -> &'static [QueryContractAlias] {
    &QUERY_CONTRACT_ALIASES
}

pub fn canonical_query_contract_id(id: QueryContractId) -> QueryContractId {
    QUERY_CONTRACT_ALIASES
        .iter()
        .find_map(|alias| (alias.alias_id == id).then_some(alias.canonical_id))
        .unwrap_or(id)
}

pub fn query_contract(id: QueryContractId) -> Option<&'static QueryContractDescriptor> {
    let id = canonical_query_contract_id(id);
    QUERY_CONTRACTS
        .iter()
        .find(|descriptor| descriptor.id == id)
}

pub fn query_legacy_builtin_name(id: QueryContractId) -> Option<&'static str> {
    query_execution_binding(id).map(|binding| binding.legacy_builtin_name)
}

pub(crate) fn query_execution_binding(
    id: QueryContractId,
) -> Option<&'static QueryExecutionBinding> {
    let id = canonical_query_contract_id(id);
    QUERY_EXECUTION_BINDINGS
        .iter()
        .find(|binding| binding.contract_id == id)
}

pub(crate) fn query_contract_bundle(
    id: QueryContractId,
) -> Option<(
    &'static QueryContractDescriptor,
    &'static QueryExecutionBinding,
)> {
    Some((query_contract(id)?, query_execution_binding(id)?))
}

pub(crate) fn query_contract_bundle_for_legacy_builtin(
    legacy_builtin_name: &str,
    surface: QuerySurfaceKind,
    capture_kind: CaptureKind,
) -> Option<(
    &'static QueryContractDescriptor,
    &'static QueryExecutionBinding,
)> {
    QUERY_EXECUTION_BINDINGS
        .iter()
        .filter(|binding| binding.legacy_builtin_name == legacy_builtin_name)
        .find_map(|binding| {
            let descriptor = query_contract(binding.contract_id)?;
            (descriptor.surface == surface && descriptor.capture_kind == capture_kind)
                .then_some((descriptor, binding))
        })
}

pub(crate) fn query_contract_bundle_for_legacy_builtin_capture_candidates(
    legacy_builtin_name: &str,
    surface: QuerySurfaceKind,
    capture_kinds: &[CaptureKind],
) -> Option<(
    &'static QueryContractDescriptor,
    &'static QueryExecutionBinding,
)> {
    capture_kinds.iter().find_map(|capture_kind| {
        query_contract_bundle_for_legacy_builtin(legacy_builtin_name, surface, *capture_kind)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execution_bindings_cover_every_descriptor_internally() {
        assert_eq!(query_contracts().len(), query_execution_bindings().len());

        for descriptor in query_contracts() {
            let binding = query_execution_binding(descriptor.id)
                .expect("every descriptor should have an execution binding");
            assert_eq!(binding.contract_id, descriptor.id);
            assert!(!binding.legacy_builtin_name.is_empty());
            assert!(binding.helper_name.is_some());
            assert!(query_contract_bundle(descriptor.id).is_some());
        }
    }
}
