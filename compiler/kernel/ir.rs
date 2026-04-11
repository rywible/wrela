use crate::hir::{self, Type};
use crate::query_plan::{
    ArtifactContract, BatchItemContract, BatchQueryKind, CandidateRecordContract,
    CandidateStrategy, CaptureKind, CaptureQueryKind, CaptureQueryPlan, DerivedArtifact,
    DispatchBackend, DispatchRecordContract, HitContextContract, InternalKernelKind,
    ParticipantSelectionContract, PlanExecutor, PlanStage, PlanningObservability, PruningStrategy,
    QueryCardinality, QueryContractId, QueryFamilyId, QueryItemKind, QueryResultKind,
    QuerySurfaceKind, QueryTargetKind, ResultRecordContract, SceneDomainFlag, SceneSummary,
    WorldQueryKind, WorldQueryPlan, capture_query_kind_for_contract_id,
    world_query_kind_for_contract_id,
};
use crate::query_solver::RaySolverPlan;
use rowan::TextRange;
use smol_str::SmolStr;

#[derive(Debug, Clone, PartialEq)]
pub struct KernelModule {
    pub entry: SmolStr,
    pub functions: Vec<KernelFunction>,
}

impl KernelModule {
    pub fn function(&self, name: &str) -> Option<&KernelFunction> {
        self.functions.iter().find(|func| func.name == name)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct KernelFunction {
    pub name: SmolStr,
    pub params: Vec<KernelParam>,
    pub ret: Type,
    pub body: KernelBlock,
}

#[derive(Debug, Clone, PartialEq)]
pub struct KernelParam {
    pub name: SmolStr,
    pub ty: Type,
}

pub type KernelBlock = Vec<KernelStmt>;

#[derive(Debug, Clone, PartialEq)]
pub enum KernelStmt {
    Let {
        name: SmolStr,
        mutable: bool,
        ty: Type,
        value: KernelExpr,
        span: TextRange,
    },
    Assign {
        name: SmolStr,
        op: hir::AssignOp,
        value: KernelExpr,
        span: TextRange,
    },
    Expr {
        value: KernelExpr,
        span: TextRange,
    },
    IgnoreResult {
        value: KernelExpr,
        span: TextRange,
    },
    If {
        condition: KernelExpr,
        then_block: KernelBlock,
        else_block: KernelBlock,
        span: TextRange,
    },
    While {
        condition: KernelExpr,
        body: KernelBlock,
        span: TextRange,
    },
    Return {
        value: Option<KernelExpr>,
        span: TextRange,
    },
    Break {
        span: TextRange,
    },
    Continue {
        span: TextRange,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum KernelExpr {
    Literal {
        value: hir::Literal,
        ty: Type,
        span: TextRange,
    },
    Var {
        name: SmolStr,
        ty: Type,
        span: TextRange,
    },
    Unary {
        op: hir::UnaryOp,
        expr: Box<KernelExpr>,
        ty: Type,
        span: TextRange,
    },
    Binary {
        op: hir::BinaryOp,
        lhs: Box<KernelExpr>,
        rhs: Box<KernelExpr>,
        ty: Type,
        span: TextRange,
    },
    Crash {
        expr: Box<KernelExpr>,
        ty: Type,
        span: TextRange,
    },
    Call {
        target: SmolStr,
        args: Vec<KernelExpr>,
        ty: Type,
        span: TextRange,
    },
    Capture {
        target: SmolStr,
        ty: Type,
        span: TextRange,
    },
    DispatchBackend {
        backend: DispatchBackend,
        ty: Type,
        span: TextRange,
    },
    CaptureQuery {
        plan: KernelCaptureQueryPlan,
        args: Vec<KernelExpr>,
        ty: Type,
        span: TextRange,
    },
    WorldQuery {
        plan: KernelWorldQueryPlan,
        args: Vec<KernelExpr>,
        ty: Type,
        span: TextRange,
    },
    BatchQuery {
        plan: KernelBatchQueryPlan,
        args: Vec<KernelExpr>,
        ty: Type,
        span: TextRange,
    },
    Member {
        base: Box<KernelExpr>,
        member: SmolStr,
        ty: Type,
        span: TextRange,
    },
    Index {
        base: Box<KernelExpr>,
        index: Box<KernelExpr>,
        ty: Type,
        span: TextRange,
    },
    ArrayLiteral {
        items: Vec<KernelExpr>,
        ty: Type,
        span: TextRange,
    },
    StructLiteral {
        name: SmolStr,
        fields: Vec<(SmolStr, KernelExpr)>,
        ty: Type,
        span: TextRange,
    },
}

impl KernelExpr {
    pub fn ty(&self) -> &Type {
        match self {
            KernelExpr::Literal { ty, .. }
            | KernelExpr::Var { ty, .. }
            | KernelExpr::Unary { ty, .. }
            | KernelExpr::Binary { ty, .. }
            | KernelExpr::Crash { ty, .. }
            | KernelExpr::Call { ty, .. }
            | KernelExpr::Capture { ty, .. }
            | KernelExpr::DispatchBackend { ty, .. }
            | KernelExpr::CaptureQuery { ty, .. }
            | KernelExpr::WorldQuery { ty, .. }
            | KernelExpr::BatchQuery { ty, .. }
            | KernelExpr::Member { ty, .. }
            | KernelExpr::Index { ty, .. }
            | KernelExpr::ArrayLiteral { ty, .. }
            | KernelExpr::StructLiteral { ty, .. } => ty,
        }
    }

    pub fn span(&self) -> TextRange {
        match self {
            KernelExpr::Literal { span, .. }
            | KernelExpr::Var { span, .. }
            | KernelExpr::Unary { span, .. }
            | KernelExpr::Binary { span, .. }
            | KernelExpr::Crash { span, .. }
            | KernelExpr::Call { span, .. }
            | KernelExpr::Capture { span, .. }
            | KernelExpr::DispatchBackend { span, .. }
            | KernelExpr::CaptureQuery { span, .. }
            | KernelExpr::WorldQuery { span, .. }
            | KernelExpr::BatchQuery { span, .. }
            | KernelExpr::Member { span, .. }
            | KernelExpr::Index { span, .. }
            | KernelExpr::ArrayLiteral { span, .. }
            | KernelExpr::StructLiteral { span, .. } => *span,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedKernelDispatch {
    pub kernel: SmolStr,
    pub workgroups: [hir::Idx<hir::Expr>; 3],
    pub workgroup_size: [hir::Idx<hir::Expr>; 3],
    pub schedule: Option<hir::Idx<hir::Expr>>,
    pub kernel_args: Vec<hir::Idx<hir::Expr>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KernelDispatchSchedule {
    Deterministic,
    Reverse,
    Shuffle(u32),
    WorkgroupReverse,
    WorkgroupShuffle(u32),
    RoundRobinWorkgroups,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KernelDispatchGrid {
    pub workgroups: [u32; 3],
    pub workgroup_size: [u32; 3],
}

impl KernelDispatchGrid {
    pub fn total_size(&self) -> [u32; 3] {
        [
            self.workgroups[0].saturating_mul(self.workgroup_size[0]),
            self.workgroups[1].saturating_mul(self.workgroup_size[1]),
            self.workgroups[2].saturating_mul(self.workgroup_size[2]),
        ]
    }

    pub fn total_count(&self) -> Option<usize> {
        self.total_size().iter().try_fold(1usize, |acc, value| {
            acc.checked_mul(usize::try_from(*value).ok()?)
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedKernelDispatch {
    pub kernel: SmolStr,
    pub grid: KernelDispatchGrid,
    pub schedule: KernelDispatchSchedule,
    pub kernel_arg_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KernelPlanStage {
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

impl From<PlanStage> for KernelPlanStage {
    fn from(value: PlanStage) -> Self {
        match value {
            PlanStage::SelectBackend => Self::SelectBackend,
            PlanStage::LoadCapture => Self::LoadCapture,
            PlanStage::BeginVirtualGpuDispatch => Self::BeginVirtualGpuDispatch,
            PlanStage::LoadDerivedArtifact { artifact } => Self::LoadDerivedArtifact { artifact },
            PlanStage::IterateItems { item_kind } => Self::IterateItems { item_kind },
            PlanStage::GenerateCandidates { strategy } => Self::GenerateCandidates { strategy },
            PlanStage::PruneCandidates { strategy } => Self::PruneCandidates { strategy },
            PlanStage::LoadDomainFlags => Self::LoadDomainFlags,
            PlanStage::SelectParticipants { kind } => Self::SelectParticipants { kind },
            PlanStage::Execute { executor } => Self::Execute { executor },
            PlanStage::AssembleHitContext => Self::AssembleHitContext,
            PlanStage::AppendResult { result_kind } => Self::AppendResult { result_kind },
            PlanStage::EndVirtualGpuDispatch => Self::EndVirtualGpuDispatch,
        }
    }
}

impl From<&PlanStage> for KernelPlanStage {
    fn from(value: &PlanStage) -> Self {
        value.clone().into()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KernelBatchQueryPlan {
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
    pub candidate_strategy: CandidateStrategy,
    pub pruning_strategy: PruningStrategy,
    pub stages: Vec<KernelPlanStage>,
    pub derived_artifacts: Vec<DerivedArtifact>,
    pub dispatch_contract: DispatchRecordContract,
    pub candidate_contract: CandidateRecordContract,
    pub result_contract: ResultRecordContract,
    pub hit_context_contract: Option<HitContextContract>,
    pub participant_contract: Option<ParticipantSelectionContract>,
    pub domain_flags: Vec<SceneDomainFlag>,
    pub artifact_contracts: Vec<ArtifactContract>,
    pub item_contract: KernelBatchItemContract,
    pub ray_solver: Option<RaySolverPlan>,
    pub observability: PlanningObservability,
    pub preserves_local_hit_context: bool,
}

impl KernelBatchQueryPlan {
    pub fn requires_virtual_gpu_dispatch(&self) -> bool {
        self.stages
            .iter()
            .any(|stage| matches!(stage, KernelPlanStage::BeginVirtualGpuDispatch))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KernelCaptureQueryPlan {
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
    pub candidate_strategy: CandidateStrategy,
    pub pruning_strategy: PruningStrategy,
    pub stages: Vec<KernelPlanStage>,
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
pub struct KernelWorldQueryPlan {
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
    pub candidate_strategy: CandidateStrategy,
    pub pruning_strategy: PruningStrategy,
    pub stages: Vec<KernelPlanStage>,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KernelBatchItemContract {
    CaptureQuery {
        plan: KernelCaptureQueryPlan,
    },
    RayThenOcclusion {
        nearest_plan: KernelCaptureQueryPlan,
    },
    WorldQuery {
        plan: KernelWorldQueryPlan,
    },
}

impl From<&CaptureQueryPlan> for KernelCaptureQueryPlan {
    fn from(plan: &CaptureQueryPlan) -> Self {
        Self {
            contract_version: plan.contract_version,
            contract_id: plan.contract_id,
            family: plan.family,
            target: plan.target,
            cardinality: plan.cardinality,
            surface: plan.surface,
            helper_name: plan.helper_name.clone(),
            kind: capture_query_kind_for_contract_id(plan.contract_id)
                .expect("capture query plan contract id must resolve"),
            capture_kind: plan.capture_kind,
            result_kind: plan.result_kind,
            executor: plan.executor,
            scene: plan.scene.clone(),
            candidate_strategy: plan.candidate_contract.candidate_strategy,
            pruning_strategy: plan.candidate_contract.pruning_strategy,
            stages: plan.stages.iter().map(KernelPlanStage::from).collect(),
            derived_artifacts: plan.derived_artifacts.clone(),
            candidate_contract: plan.candidate_contract.clone(),
            result_contract: plan.result_contract.clone(),
            hit_context_contract: plan.hit_context_contract.clone(),
            participant_contract: plan.participant_contract.clone(),
            artifact_contracts: plan.artifact_contracts.clone(),
            observability: plan.observability.clone(),
            preserves_local_hit_context: plan.preserves_local_hit_context,
        }
    }
}

impl From<&WorldQueryPlan> for KernelWorldQueryPlan {
    fn from(plan: &WorldQueryPlan) -> Self {
        Self {
            contract_version: plan.contract_version,
            contract_id: plan.contract_id,
            family: plan.family,
            target: plan.target,
            cardinality: plan.cardinality,
            surface: plan.surface,
            helper_name: plan.helper_name.clone(),
            kind: world_query_kind_for_contract_id(plan.contract_id)
                .expect("world query plan contract id must resolve"),
            backend: plan.backend,
            result_kind: plan.result_kind,
            executor: plan.executor,
            candidate_strategy: plan.candidate_contract.candidate_strategy,
            pruning_strategy: plan.candidate_contract.pruning_strategy,
            stages: plan.stages.iter().map(KernelPlanStage::from).collect(),
            derived_artifacts: plan.derived_artifacts.clone(),
            dispatch_contract: plan.dispatch_contract.clone(),
            candidate_contract: plan.candidate_contract.clone(),
            result_contract: plan.result_contract.clone(),
            hit_context_contract: plan.hit_context_contract.clone(),
            participant_contract: plan.participant_contract.clone(),
            domain_flags: plan.domain_flags.clone(),
            artifact_contracts: plan.artifact_contracts.clone(),
            ray_solver: plan.ray_solver.clone(),
            observability: plan.observability.clone(),
            preserves_local_hit_context: plan.preserves_local_hit_context,
        }
    }
}

impl From<&BatchItemContract> for KernelBatchItemContract {
    fn from(value: &BatchItemContract) -> Self {
        match value {
            BatchItemContract::CaptureQuery { plan } => Self::CaptureQuery { plan: plan.into() },
            BatchItemContract::RayThenOcclusion { nearest_plan } => Self::RayThenOcclusion {
                nearest_plan: nearest_plan.into(),
            },
            BatchItemContract::WorldQuery { plan } => Self::WorldQuery { plan: plan.into() },
        }
    }
}
