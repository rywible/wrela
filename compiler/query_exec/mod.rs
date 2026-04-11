mod capture;
pub(crate) mod cost;
mod native_bridge;

pub mod context;
pub mod cpu;
pub mod ids;
pub mod mir;
pub mod region;
pub mod spec;
pub mod vgpu;
pub mod wgsl;
pub mod world;

use crate::kernel::KernelValue;
use crate::kernel::ir::{KernelBatchQueryPlan, KernelCaptureQueryPlan, KernelWorldQueryPlan};
use crate::kernel::{KernelBatchQueryTrace, interpret_batch_query};
use crate::query_contract::{
    self, QueryContractId, QueryFamilyId, QueryQuestionId, QuerySurfaceKind,
};
use crate::query_plan::DispatchBackend;
use crate::query_solver::RaySolverMethod;
use smol_str::SmolStr;

pub use context::QueryExecContext;
pub use cost::{
    CostFidelity, SemanticCostCause, SemanticCostCauseKind, SemanticCostReport, SemanticCostStage,
    SemanticCostUnit, SemanticQueryScope, SemanticStageKind, render_semantic_cost_report,
};
pub use cpu::QueryExecError;
pub use ids::{
    stable_field_scene_capture_id, stable_region_scene_capture_id, stable_shape_capture_id,
    stable_shape_scene_capture_id,
};
pub use region::{
    RegionExecCase, RegionShapeLists, build_region_exec_cases, executable_region_shape_lists,
    select_region_exec_case, world_domain_mismatch_message,
};
pub use world::{WorldQuerySemantics, world_query_semantics};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchQueryExecutionTrace {
    pub contract_id: QueryContractId,
    pub family: QueryFamilyId,
    pub question: QueryQuestionId,
    pub surface: QuerySurfaceKind,
    pub contract_version: u32,
    pub backend: DispatchBackend,
    pub plan_trace: KernelBatchQueryTrace,
    pub observability: QueryExecutionObservability,
    pub cost_report: SemanticCostReport,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectQueryExecutionTrace {
    pub contract_id: QueryContractId,
    pub family: QueryFamilyId,
    pub question: QueryQuestionId,
    pub surface: QuerySurfaceKind,
    pub contract_version: u32,
    pub backend: DispatchBackend,
    pub executor: DirectQueryExecutor,
    pub observability: QueryExecutionObservability,
    pub cost_report: SemanticCostReport,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QueryExecutionObservability {
    pub solver_plan_id: Option<SmolStr>,
    pub solver_methods: Vec<RaySolverMethod>,
    pub dispatch_count: u32,
    pub dispatch_items: u32,
    pub dispatch_workgroups_x: u32,
    pub dispatch_workgroups_y: u32,
    pub dispatch_workgroups_z: u32,
    pub screen_sample_count: u32,
    pub world_batch_item_count: u32,
    pub candidate_count: u32,
    pub candidates_before_pruning: u32,
    pub candidates_after_pruning: u32,
    pub branch_visits: u32,
    pub support_pruned_candidates: u32,
    pub artifact_loads: u32,
    pub opaque_fallbacks: u32,
    pub trace_steps: u32,
    pub trace_steps_max: u32,
    pub hit_count: u32,
    pub miss_count: u32,
    pub dense_compatibility_batches: u32,
    pub semantic_pruned_batches: u32,
    pub solver_analytic_hits: u32,
    pub solver_support_rejections: u32,
    pub solver_interval_skips: u32,
    pub solver_packet_tile_rejections: u32,
    pub solver_newton_refinements: u32,
    pub solver_lipschitz_steps: u32,
    pub solver_adaptive_epsilon_uses: u32,
    pub solver_dense_fallback_rays: u32,
    pub solver_generated_dense_fallback_rays: u32,
    pub solver_fallback_contract_dense: u32,
    pub solver_fallback_missing_facts: u32,
    pub solver_fallback_analytic_unsupported: u32,
    pub solver_fallback_verification_failed: u32,
    pub solver_fallback_unsupported_backend: u32,
    pub solver_certificate_failures: u32,
    pub solver_continuation_available: u32,
    pub solver_continuation_consumed: u32,
    pub solver_continuation_rejected: u32,
    pub solver_continuation_unavailable: u32,
    pub field_samples: u32,
    pub contract_validation_failures: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectQueryExecutor {
    Cpu,
    VirtualGpu,
    Wgsl,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct QueryTraceIdentity {
    contract_id: QueryContractId,
    family: QueryFamilyId,
    question: QueryQuestionId,
    surface: QuerySurfaceKind,
    contract_version: u32,
    supported_backends: query_contract::BackendSupport,
}

pub fn execute_capture_query(
    ctx: &QueryExecContext,
    plan: &KernelCaptureQueryPlan,
    args: &[KernelValue],
) -> Result<KernelValue, QueryExecError> {
    execute_capture_query_with_trace_on(ctx, DispatchBackend::Cpu, plan, args)
        .map(|(value, _)| value)
}

pub fn execute_capture_query_on(
    ctx: &QueryExecContext,
    backend: DispatchBackend,
    plan: &KernelCaptureQueryPlan,
    args: &[KernelValue],
) -> Result<KernelValue, QueryExecError> {
    execute_capture_query_with_trace_on(ctx, backend, plan, args).map(|(value, _)| value)
}

pub fn execute_capture_query_with_trace_on(
    ctx: &QueryExecContext,
    backend: DispatchBackend,
    plan: &KernelCaptureQueryPlan,
    args: &[KernelValue],
) -> Result<(KernelValue, DirectQueryExecutionTrace), QueryExecError> {
    let identity = trace_identity(plan.contract_id)?;
    let backend = resolve_direct_backend(backend);
    ensure_backend_supported(identity, backend)?;
    let (value, executor, observability) = match backend {
        DispatchBackend::VirtualGpu => {
            let (value, observability) =
                vgpu::execute_capture_query_with_observability(ctx, plan, args)?;
            (value, DirectQueryExecutor::VirtualGpu, observability)
        }
        DispatchBackend::Wgsl => {
            let (value, observability) =
                wgsl::execute_capture_query_with_observability(ctx, plan, args)?;
            (value, DirectQueryExecutor::Wgsl, observability)
        }
        DispatchBackend::Cpu | DispatchBackend::Auto => {
            let (value, observability) =
                cpu::execute_capture_query_with_observability(ctx, plan, args)?;
            (value, DirectQueryExecutor::Cpu, observability)
        }
    };
    Ok((
        value,
        DirectQueryExecutionTrace {
            contract_id: identity.contract_id,
            family: identity.family,
            question: identity.question,
            surface: identity.surface,
            contract_version: identity.contract_version,
            backend,
            executor,
            cost_report: cost::capture_cost_report(backend, plan, &observability),
            observability,
        },
    ))
}

pub fn execute_world_query(
    ctx: &QueryExecContext,
    plan: &KernelWorldQueryPlan,
    args: &[KernelValue],
) -> Result<KernelValue, QueryExecError> {
    execute_world_query_with_trace_on(ctx, plan.backend, plan, args).map(|(value, _)| value)
}

pub fn execute_world_query_on(
    ctx: &QueryExecContext,
    backend: DispatchBackend,
    plan: &KernelWorldQueryPlan,
    args: &[KernelValue],
) -> Result<KernelValue, QueryExecError> {
    execute_world_query_with_trace_on(ctx, backend, plan, args).map(|(value, _)| value)
}

pub fn execute_world_query_with_trace_on(
    ctx: &QueryExecContext,
    requested_backend: DispatchBackend,
    plan: &KernelWorldQueryPlan,
    args: &[KernelValue],
) -> Result<(KernelValue, DirectQueryExecutionTrace), QueryExecError> {
    let identity = trace_identity(plan.contract_id)?;
    let backend = resolve_world_backend(requested_backend, plan);
    ensure_backend_supported(identity, backend)?;
    let (value, executor, observability) = match backend {
        DispatchBackend::VirtualGpu => {
            let (value, observability) =
                vgpu::execute_world_query_with_observability(ctx, plan, args)?;
            (value, DirectQueryExecutor::VirtualGpu, observability)
        }
        DispatchBackend::Wgsl => {
            let (value, observability) =
                wgsl::execute_world_query_with_observability(ctx, plan, args)?;
            (value, DirectQueryExecutor::Wgsl, observability)
        }
        DispatchBackend::Cpu | DispatchBackend::Auto => {
            let (value, observability) =
                cpu::execute_world_query_with_observability(ctx, plan, args)?;
            (value, DirectQueryExecutor::Cpu, observability)
        }
    };
    Ok((
        value,
        DirectQueryExecutionTrace {
            contract_id: identity.contract_id,
            family: identity.family,
            question: identity.question,
            surface: identity.surface,
            contract_version: identity.contract_version,
            backend,
            executor,
            cost_report: cost::world_cost_report(backend, plan, &observability),
            observability,
        },
    ))
}

pub fn execute_batch_query(
    ctx: &QueryExecContext,
    plan: &KernelBatchQueryPlan,
    args: &[KernelValue],
) -> Result<KernelValue, QueryExecError> {
    execute_batch_query_on(ctx, plan.backend, plan, args)
}

pub fn execute_batch_query_on(
    ctx: &QueryExecContext,
    backend: DispatchBackend,
    plan: &KernelBatchQueryPlan,
    args: &[KernelValue],
) -> Result<KernelValue, QueryExecError> {
    execute_batch_query_with_trace_on(ctx, backend, plan, args).map(|(value, _)| value)
}

pub fn execute_batch_query_with_trace(
    ctx: &QueryExecContext,
    plan: &KernelBatchQueryPlan,
    args: &[KernelValue],
) -> Result<(KernelValue, BatchQueryExecutionTrace), QueryExecError> {
    execute_batch_query_with_trace_on(ctx, plan.backend, plan, args)
}

pub fn execute_batch_query_with_trace_on(
    ctx: &QueryExecContext,
    requested_backend: DispatchBackend,
    plan: &KernelBatchQueryPlan,
    args: &[KernelValue],
) -> Result<(KernelValue, BatchQueryExecutionTrace), QueryExecError> {
    let identity = trace_identity(plan.contract_id)?;
    let item_count = batch_query_item_count(plan, args)?;
    let plan_trace = interpret_batch_query(plan, item_count);
    let backend = resolve_batch_backend(requested_backend, plan);
    ensure_backend_supported(identity, backend)?;
    let (value, observability) = match backend {
        DispatchBackend::VirtualGpu => {
            vgpu::execute_batch_query_with_observability(ctx, plan, args, &plan_trace)?
        }
        DispatchBackend::Wgsl => {
            wgsl::execute_batch_query_with_observability(ctx, plan, args, &plan_trace)?
        }
        DispatchBackend::Cpu | DispatchBackend::Auto => {
            cpu::execute_batch_query_with_observability(ctx, plan, args)?
        }
    };
    let cost_report = cost::batch_cost_report(backend, plan, &plan_trace, &observability);
    Ok((
        value,
        BatchQueryExecutionTrace {
            contract_id: identity.contract_id,
            family: identity.family,
            question: identity.question,
            surface: identity.surface,
            contract_version: identity.contract_version,
            backend,
            plan_trace,
            cost_report,
            observability,
        },
    ))
}

fn trace_identity(contract_id: QueryContractId) -> Result<QueryTraceIdentity, QueryExecError> {
    let descriptor =
        query_contract::query_contract(contract_id).ok_or_else(|| QueryExecError::Unsupported {
            message: format!("missing query contract '{}'", contract_id.as_str()),
        })?;
    Ok(QueryTraceIdentity {
        contract_id: descriptor.id,
        family: descriptor.family,
        question: descriptor.question,
        surface: descriptor.surface,
        contract_version: descriptor.version,
        supported_backends: descriptor.supported_backends,
    })
}

fn ensure_backend_supported(
    identity: QueryTraceIdentity,
    backend: DispatchBackend,
) -> Result<(), QueryExecError> {
    if identity.supported_backends.supports(backend) {
        Ok(())
    } else {
        Err(QueryExecError::Unsupported {
            message: format!(
                "query contract '{}' v{} ({:?}/{:?}) does not support backend {:?}",
                identity.contract_id.as_str(),
                identity.contract_version,
                identity.surface,
                identity.question,
                backend
            ),
        })
    }
}

fn resolve_direct_backend(backend: DispatchBackend) -> DispatchBackend {
    match backend {
        DispatchBackend::VirtualGpu => DispatchBackend::VirtualGpu,
        DispatchBackend::Wgsl => DispatchBackend::Wgsl,
        DispatchBackend::Cpu | DispatchBackend::Auto => DispatchBackend::Cpu,
    }
}

fn resolve_world_backend(
    requested_backend: DispatchBackend,
    plan: &KernelWorldQueryPlan,
) -> DispatchBackend {
    match requested_backend {
        DispatchBackend::Cpu => DispatchBackend::Cpu,
        DispatchBackend::VirtualGpu => DispatchBackend::VirtualGpu,
        DispatchBackend::Wgsl => DispatchBackend::Wgsl,
        DispatchBackend::Auto => match plan.backend {
            DispatchBackend::Cpu | DispatchBackend::Auto => DispatchBackend::Cpu,
            DispatchBackend::VirtualGpu => DispatchBackend::VirtualGpu,
            DispatchBackend::Wgsl => DispatchBackend::Wgsl,
        },
    }
}

fn resolve_batch_backend(
    requested_backend: DispatchBackend,
    plan: &KernelBatchQueryPlan,
) -> DispatchBackend {
    match requested_backend {
        DispatchBackend::Cpu => DispatchBackend::Cpu,
        DispatchBackend::VirtualGpu => DispatchBackend::VirtualGpu,
        DispatchBackend::Wgsl => DispatchBackend::Wgsl,
        DispatchBackend::Auto => {
            if matches!(plan.backend, DispatchBackend::Wgsl) {
                DispatchBackend::Wgsl
            } else if matches!(plan.backend, DispatchBackend::VirtualGpu)
                || plan.requires_virtual_gpu_dispatch()
            {
                DispatchBackend::VirtualGpu
            } else {
                DispatchBackend::Cpu
            }
        }
    }
}

fn batch_query_item_count(
    plan: &KernelBatchQueryPlan,
    args: &[KernelValue],
) -> Result<u32, QueryExecError> {
    let item_index = if matches!(plan.capture_kind, crate::query_plan::CaptureKind::Region) {
        2
    } else {
        1
    };
    match args.get(item_index) {
        Some(KernelValue::Array(items)) => Ok(items.len() as u32),
        Some(other) => Err(QueryExecError::TypeMismatch {
            expected: "Array".to_string(),
            found: format!("{other:?}"),
        }),
        None => Err(QueryExecError::MissingCaptureTarget {
            kind: "batch query items",
        }),
    }
}
