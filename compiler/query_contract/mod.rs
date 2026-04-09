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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum QueryItemKind {
    Unit,
    PointQuery,
    RayQuery,
    Hit3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum QueryResultKind {
    DistanceResult,
    NormalResult,
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
    Trace,
    Sample,
    Radiance,
    Medium,
    Occluded,
    Summary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum QuerySurfaceKind {
    CaptureScalar,
    WorldScalar,
    CaptureBatch,
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
    SpatialNormalCaptureField,
    SpatialNormalCaptureShape,
    SpatialNormalWorld,
    SpatialNormalBatchField,
    SpatialNormalBatchShape,
    SpatialTraceCaptureShape,
    SpatialTraceBatchShape,
    SpatialTraceWorld,
    SpatialOccludedBatchShape,
    SurfaceSampleCaptureShape,
    SurfaceSampleBatchShape,
    SurfaceSampleWorld,
    ParticipantsRadianceCaptureShape,
    ParticipantsRadianceWorld,
    ParticipantsMediumCaptureShape,
    ParticipantsMediumWorld,
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
    pub artifact_sizes: bool,
    pub dispatch_overhead: bool,
}

impl QueryObservabilityProfile {
    pub const fn spatial() -> Self {
        Self {
            candidate_count: true,
            branch_visits: true,
            support_prune_effectiveness: true,
            culling_hit_rate: true,
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
pub struct QueryExecutionBinding {
    pub contract_id: QueryContractId,
    pub planner_recipe: QueryPlannerRecipeKind,
    pub default_executor: PlanExecutor,
    pub default_kernel: Option<InternalKernelKind>,
    pub helper_name: Option<&'static str>,
    pub legacy_builtin_name: &'static str,
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
pub const SPATIAL_NORMAL_CAPTURE_FIELD: QueryContractId =
    QueryContractId::new("spatial.normal.capture.field");
pub const SPATIAL_NORMAL_CAPTURE_SHAPE: QueryContractId =
    QueryContractId::new("spatial.normal.capture.shape");
pub const SPATIAL_NORMAL_WORLD: QueryContractId = QueryContractId::new("spatial.normal.world");
pub const SPATIAL_NORMAL_BATCH_FIELD: QueryContractId =
    QueryContractId::new("spatial.normal.batch.field");
pub const SPATIAL_NORMAL_BATCH_SHAPE: QueryContractId =
    QueryContractId::new("spatial.normal.batch.shape");
pub const SPATIAL_TRACE_CAPTURE_SHAPE: QueryContractId =
    QueryContractId::new("spatial.trace.capture.shape");
pub const SPATIAL_TRACE_BATCH_SHAPE: QueryContractId =
    QueryContractId::new("spatial.trace.batch.shape");
pub const SPATIAL_TRACE_WORLD: QueryContractId = QueryContractId::new("spatial.trace.world");
pub const SPATIAL_OCCLUDED_BATCH_SHAPE: QueryContractId =
    QueryContractId::new("spatial.occluded.batch.shape");
pub const SURFACE_SAMPLE_CAPTURE_SHAPE: QueryContractId =
    QueryContractId::new("surface.sample.capture.shape");
pub const SURFACE_SAMPLE_BATCH_SHAPE: QueryContractId =
    QueryContractId::new("surface.sample.batch.shape");
pub const SURFACE_SAMPLE_WORLD: QueryContractId = QueryContractId::new("surface.sample.world");
pub const PARTICIPANTS_RADIANCE_CAPTURE_SHAPE: QueryContractId =
    QueryContractId::new("participants.radiance.capture.shape");
pub const PARTICIPANTS_RADIANCE_WORLD: QueryContractId =
    QueryContractId::new("participants.radiance.world");
pub const PARTICIPANTS_MEDIUM_CAPTURE_SHAPE: QueryContractId =
    QueryContractId::new("participants.medium.capture.shape");
pub const PARTICIPANTS_MEDIUM_WORLD: QueryContractId =
    QueryContractId::new("participants.medium.world");

const NO_DOMAIN_FLAGS: &[SceneDomainFlag] = &[];
const MATERIAL_DOMAIN_FLAGS: &[SceneDomainFlag] = &[SceneDomainFlag::Material];
const RADIANCE_DOMAIN_FLAGS: &[SceneDomainFlag] = &[SceneDomainFlag::Radiance];
const MEDIA_DOMAIN_FLAGS: &[SceneDomainFlag] = &[SceneDomainFlag::Media];

const QUERY_CONTRACTS: [QueryContractDescriptor; 21] = [
    QueryContractDescriptor {
        id: SPATIAL_DISTANCE_CAPTURE_FIELD,
        version: QUERY_CONTRACT_VERSION,
        family: QueryFamilyId::Spatial,
        question: QueryQuestionId::Distance,
        surface: QuerySurfaceKind::CaptureScalar,
        capture_kind: CaptureKind::Field,
        item_kind: QueryItemKind::PointQuery,
        result_kind: QueryResultKind::DistanceResult,
        domain_contract: None,
        required_domain_flags: NO_DOMAIN_FLAGS,
        preserves_local_hit_context: false,
        participant_kind: None,
        supported_backends: BackendSupport::all(),
        observability: QueryObservabilityProfile::spatial(),
    },
    QueryContractDescriptor {
        id: SPATIAL_DISTANCE_CAPTURE_SHAPE,
        version: QUERY_CONTRACT_VERSION,
        family: QueryFamilyId::Spatial,
        question: QueryQuestionId::Distance,
        surface: QuerySurfaceKind::CaptureScalar,
        capture_kind: CaptureKind::Shape,
        item_kind: QueryItemKind::PointQuery,
        result_kind: QueryResultKind::DistanceResult,
        domain_contract: None,
        required_domain_flags: NO_DOMAIN_FLAGS,
        preserves_local_hit_context: false,
        participant_kind: None,
        supported_backends: BackendSupport::all(),
        observability: QueryObservabilityProfile::spatial(),
    },
    QueryContractDescriptor {
        id: SPATIAL_DISTANCE_WORLD,
        version: QUERY_CONTRACT_VERSION,
        family: QueryFamilyId::Spatial,
        question: QueryQuestionId::Distance,
        surface: QuerySurfaceKind::WorldScalar,
        capture_kind: CaptureKind::Region,
        item_kind: QueryItemKind::PointQuery,
        result_kind: QueryResultKind::DistanceResult,
        domain_contract: Some(DomainContractKind::SceneDomain),
        required_domain_flags: NO_DOMAIN_FLAGS,
        preserves_local_hit_context: false,
        participant_kind: None,
        supported_backends: BackendSupport::all(),
        observability: QueryObservabilityProfile::spatial(),
    },
    QueryContractDescriptor {
        id: SPATIAL_DISTANCE_BATCH_FIELD,
        version: QUERY_CONTRACT_VERSION,
        family: QueryFamilyId::Spatial,
        question: QueryQuestionId::Distance,
        surface: QuerySurfaceKind::CaptureBatch,
        capture_kind: CaptureKind::Field,
        item_kind: QueryItemKind::PointQuery,
        result_kind: QueryResultKind::DistanceResult,
        domain_contract: None,
        required_domain_flags: NO_DOMAIN_FLAGS,
        preserves_local_hit_context: false,
        participant_kind: None,
        supported_backends: BackendSupport::all(),
        observability: QueryObservabilityProfile::spatial(),
    },
    QueryContractDescriptor {
        id: SPATIAL_DISTANCE_BATCH_SHAPE,
        version: QUERY_CONTRACT_VERSION,
        family: QueryFamilyId::Spatial,
        question: QueryQuestionId::Distance,
        surface: QuerySurfaceKind::CaptureBatch,
        capture_kind: CaptureKind::Shape,
        item_kind: QueryItemKind::PointQuery,
        result_kind: QueryResultKind::DistanceResult,
        domain_contract: None,
        required_domain_flags: NO_DOMAIN_FLAGS,
        preserves_local_hit_context: false,
        participant_kind: None,
        supported_backends: BackendSupport::all(),
        observability: QueryObservabilityProfile::spatial(),
    },
    QueryContractDescriptor {
        id: SPATIAL_NORMAL_CAPTURE_FIELD,
        version: QUERY_CONTRACT_VERSION,
        family: QueryFamilyId::Spatial,
        question: QueryQuestionId::Normal,
        surface: QuerySurfaceKind::CaptureScalar,
        capture_kind: CaptureKind::Field,
        item_kind: QueryItemKind::PointQuery,
        result_kind: QueryResultKind::NormalResult,
        domain_contract: None,
        required_domain_flags: NO_DOMAIN_FLAGS,
        preserves_local_hit_context: false,
        participant_kind: None,
        supported_backends: BackendSupport::all(),
        observability: QueryObservabilityProfile::spatial(),
    },
    QueryContractDescriptor {
        id: SPATIAL_NORMAL_CAPTURE_SHAPE,
        version: QUERY_CONTRACT_VERSION,
        family: QueryFamilyId::Spatial,
        question: QueryQuestionId::Normal,
        surface: QuerySurfaceKind::CaptureScalar,
        capture_kind: CaptureKind::Shape,
        item_kind: QueryItemKind::PointQuery,
        result_kind: QueryResultKind::NormalResult,
        domain_contract: None,
        required_domain_flags: NO_DOMAIN_FLAGS,
        preserves_local_hit_context: false,
        participant_kind: None,
        supported_backends: BackendSupport::all(),
        observability: QueryObservabilityProfile::spatial(),
    },
    QueryContractDescriptor {
        id: SPATIAL_NORMAL_WORLD,
        version: QUERY_CONTRACT_VERSION,
        family: QueryFamilyId::Spatial,
        question: QueryQuestionId::Normal,
        surface: QuerySurfaceKind::WorldScalar,
        capture_kind: CaptureKind::Region,
        item_kind: QueryItemKind::PointQuery,
        result_kind: QueryResultKind::NormalResult,
        domain_contract: Some(DomainContractKind::SceneDomain),
        required_domain_flags: NO_DOMAIN_FLAGS,
        preserves_local_hit_context: false,
        participant_kind: None,
        supported_backends: BackendSupport::all(),
        observability: QueryObservabilityProfile::spatial(),
    },
    QueryContractDescriptor {
        id: SPATIAL_NORMAL_BATCH_FIELD,
        version: QUERY_CONTRACT_VERSION,
        family: QueryFamilyId::Spatial,
        question: QueryQuestionId::Normal,
        surface: QuerySurfaceKind::CaptureBatch,
        capture_kind: CaptureKind::Field,
        item_kind: QueryItemKind::PointQuery,
        result_kind: QueryResultKind::NormalResult,
        domain_contract: None,
        required_domain_flags: NO_DOMAIN_FLAGS,
        preserves_local_hit_context: false,
        participant_kind: None,
        supported_backends: BackendSupport::all(),
        observability: QueryObservabilityProfile::spatial(),
    },
    QueryContractDescriptor {
        id: SPATIAL_NORMAL_BATCH_SHAPE,
        version: QUERY_CONTRACT_VERSION,
        family: QueryFamilyId::Spatial,
        question: QueryQuestionId::Normal,
        surface: QuerySurfaceKind::CaptureBatch,
        capture_kind: CaptureKind::Shape,
        item_kind: QueryItemKind::PointQuery,
        result_kind: QueryResultKind::NormalResult,
        domain_contract: None,
        required_domain_flags: NO_DOMAIN_FLAGS,
        preserves_local_hit_context: false,
        participant_kind: None,
        supported_backends: BackendSupport::all(),
        observability: QueryObservabilityProfile::spatial(),
    },
    QueryContractDescriptor {
        id: SPATIAL_TRACE_CAPTURE_SHAPE,
        version: QUERY_CONTRACT_VERSION,
        family: QueryFamilyId::Spatial,
        question: QueryQuestionId::Trace,
        surface: QuerySurfaceKind::CaptureScalar,
        capture_kind: CaptureKind::Shape,
        item_kind: QueryItemKind::RayQuery,
        result_kind: QueryResultKind::Hit3,
        domain_contract: None,
        required_domain_flags: NO_DOMAIN_FLAGS,
        preserves_local_hit_context: true,
        participant_kind: None,
        supported_backends: BackendSupport::all(),
        observability: QueryObservabilityProfile::spatial(),
    },
    QueryContractDescriptor {
        id: SPATIAL_TRACE_BATCH_SHAPE,
        version: QUERY_CONTRACT_VERSION,
        family: QueryFamilyId::Spatial,
        question: QueryQuestionId::Trace,
        surface: QuerySurfaceKind::CaptureBatch,
        capture_kind: CaptureKind::Shape,
        item_kind: QueryItemKind::RayQuery,
        result_kind: QueryResultKind::Hit3,
        domain_contract: None,
        required_domain_flags: NO_DOMAIN_FLAGS,
        preserves_local_hit_context: true,
        participant_kind: None,
        supported_backends: BackendSupport::all(),
        observability: QueryObservabilityProfile::spatial(),
    },
    QueryContractDescriptor {
        id: SPATIAL_TRACE_WORLD,
        version: QUERY_CONTRACT_VERSION,
        family: QueryFamilyId::Spatial,
        question: QueryQuestionId::Trace,
        surface: QuerySurfaceKind::WorldScalar,
        capture_kind: CaptureKind::Region,
        item_kind: QueryItemKind::RayQuery,
        result_kind: QueryResultKind::Hit3,
        domain_contract: Some(DomainContractKind::SceneDomain),
        required_domain_flags: NO_DOMAIN_FLAGS,
        preserves_local_hit_context: true,
        participant_kind: None,
        supported_backends: BackendSupport::all(),
        observability: QueryObservabilityProfile::spatial(),
    },
    QueryContractDescriptor {
        id: SPATIAL_OCCLUDED_BATCH_SHAPE,
        version: QUERY_CONTRACT_VERSION,
        family: QueryFamilyId::Spatial,
        question: QueryQuestionId::Occluded,
        surface: QuerySurfaceKind::CaptureBatch,
        capture_kind: CaptureKind::Shape,
        item_kind: QueryItemKind::RayQuery,
        result_kind: QueryResultKind::OcclusionResult,
        domain_contract: None,
        required_domain_flags: NO_DOMAIN_FLAGS,
        preserves_local_hit_context: true,
        participant_kind: None,
        supported_backends: BackendSupport::all(),
        observability: QueryObservabilityProfile::spatial(),
    },
    QueryContractDescriptor {
        id: SURFACE_SAMPLE_CAPTURE_SHAPE,
        version: QUERY_CONTRACT_VERSION,
        family: QueryFamilyId::Surface,
        question: QueryQuestionId::Sample,
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
        id: SURFACE_SAMPLE_WORLD,
        version: QUERY_CONTRACT_VERSION,
        family: QueryFamilyId::Surface,
        question: QueryQuestionId::Sample,
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
        surface: QuerySurfaceKind::CaptureScalar,
        capture_kind: CaptureKind::Shape,
        item_kind: QueryItemKind::PointQuery,
        result_kind: QueryResultKind::RadianceResult,
        domain_contract: None,
        required_domain_flags: NO_DOMAIN_FLAGS,
        preserves_local_hit_context: false,
        participant_kind: Some(ParticipantContractKind::Radiance),
        supported_backends: BackendSupport::all(),
        observability: QueryObservabilityProfile::spatial(),
    },
    QueryContractDescriptor {
        id: PARTICIPANTS_RADIANCE_WORLD,
        version: QUERY_CONTRACT_VERSION,
        family: QueryFamilyId::Participants,
        question: QueryQuestionId::Radiance,
        surface: QuerySurfaceKind::WorldScalar,
        capture_kind: CaptureKind::Region,
        item_kind: QueryItemKind::PointQuery,
        result_kind: QueryResultKind::RadianceResult,
        domain_contract: Some(DomainContractKind::SceneDomain),
        required_domain_flags: RADIANCE_DOMAIN_FLAGS,
        preserves_local_hit_context: false,
        participant_kind: Some(ParticipantContractKind::Radiance),
        supported_backends: BackendSupport::all(),
        observability: QueryObservabilityProfile::spatial(),
    },
    QueryContractDescriptor {
        id: PARTICIPANTS_MEDIUM_CAPTURE_SHAPE,
        version: QUERY_CONTRACT_VERSION,
        family: QueryFamilyId::Participants,
        question: QueryQuestionId::Medium,
        surface: QuerySurfaceKind::CaptureScalar,
        capture_kind: CaptureKind::Shape,
        item_kind: QueryItemKind::PointQuery,
        result_kind: QueryResultKind::MediumResult,
        domain_contract: None,
        required_domain_flags: NO_DOMAIN_FLAGS,
        preserves_local_hit_context: false,
        participant_kind: Some(ParticipantContractKind::Medium),
        supported_backends: BackendSupport::all(),
        observability: QueryObservabilityProfile::spatial(),
    },
    QueryContractDescriptor {
        id: PARTICIPANTS_MEDIUM_WORLD,
        version: QUERY_CONTRACT_VERSION,
        family: QueryFamilyId::Participants,
        question: QueryQuestionId::Medium,
        surface: QuerySurfaceKind::WorldScalar,
        capture_kind: CaptureKind::Region,
        item_kind: QueryItemKind::PointQuery,
        result_kind: QueryResultKind::MediumResult,
        domain_contract: Some(DomainContractKind::SceneDomain),
        required_domain_flags: MEDIA_DOMAIN_FLAGS,
        preserves_local_hit_context: false,
        participant_kind: Some(ParticipantContractKind::Medium),
        supported_backends: BackendSupport::all(),
        observability: QueryObservabilityProfile::spatial(),
    },
];

const QUERY_EXECUTION_BINDINGS: [QueryExecutionBinding; 21] = [
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
        contract_id: SPATIAL_TRACE_CAPTURE_SHAPE,
        planner_recipe: QueryPlannerRecipeKind::SpatialTraceCaptureShape,
        default_executor: PlanExecutor::SceneTraceCapture,
        default_kernel: Some(InternalKernelKind::ShapeTraceCapture),
        helper_name: Some("__wr_scene_trace_capture"),
        legacy_builtin_name: "trace_shape",
    },
    QueryExecutionBinding {
        contract_id: SPATIAL_TRACE_BATCH_SHAPE,
        planner_recipe: QueryPlannerRecipeKind::SpatialTraceBatchShape,
        default_executor: PlanExecutor::SceneTraceCapture,
        default_kernel: Some(InternalKernelKind::ShapeTraceCapture),
        helper_name: Some("__wr_scene_trace_batch_queries"),
        legacy_builtin_name: "trace_shape_batch",
    },
    QueryExecutionBinding {
        contract_id: SPATIAL_TRACE_WORLD,
        planner_recipe: QueryPlannerRecipeKind::SpatialTraceWorld,
        default_executor: PlanExecutor::WorldTraceCapture,
        default_kernel: Some(InternalKernelKind::WorldTraceCapture),
        helper_name: Some("__wr_world_trace_capture"),
        legacy_builtin_name: "trace_world",
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
];

pub fn query_contracts() -> &'static [QueryContractDescriptor] {
    &QUERY_CONTRACTS
}

pub fn query_execution_bindings() -> &'static [QueryExecutionBinding] {
    &QUERY_EXECUTION_BINDINGS
}

pub fn query_contract(id: QueryContractId) -> Option<&'static QueryContractDescriptor> {
    QUERY_CONTRACTS
        .iter()
        .find(|descriptor| descriptor.id == id)
}

pub fn query_execution_binding(id: QueryContractId) -> Option<&'static QueryExecutionBinding> {
    QUERY_EXECUTION_BINDINGS
        .iter()
        .find(|binding| binding.contract_id == id)
}

pub fn query_contract_bundle(
    id: QueryContractId,
) -> Option<(
    &'static QueryContractDescriptor,
    &'static QueryExecutionBinding,
)> {
    Some((query_contract(id)?, query_execution_binding(id)?))
}
