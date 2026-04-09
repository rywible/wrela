mod capture;
mod cost;
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
use crate::query_plan::DispatchBackend;

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
    pub backend: DispatchBackend,
    pub plan_trace: KernelBatchQueryTrace,
    pub observability: QueryExecutionObservability,
    pub cost_report: SemanticCostReport,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectQueryExecutionTrace {
    pub backend: DispatchBackend,
    pub executor: DirectQueryExecutor,
    pub observability: QueryExecutionObservability,
    pub cost_report: SemanticCostReport,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QueryExecutionObservability {
    pub dispatch_count: u32,
    pub candidate_count: u32,
    pub branch_visits: u32,
    pub support_pruned_candidates: u32,
    pub artifact_loads: u32,
    pub opaque_fallbacks: u32,
    pub trace_steps: u32,
    pub field_samples: u32,
    pub contract_validation_failures: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectQueryExecutor {
    Cpu,
    VirtualGpu,
    Wgsl,
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
    let backend = resolve_direct_backend(backend);
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
    let backend = resolve_world_backend(requested_backend, plan);
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
    let item_count = batch_query_item_count(args)?;
    let plan_trace = interpret_batch_query(plan, item_count);
    let backend = resolve_batch_backend(requested_backend, plan);
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
            backend,
            plan_trace,
            cost_report,
            observability,
        },
    ))
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

fn batch_query_item_count(args: &[KernelValue]) -> Result<u32, QueryExecError> {
    match args.get(1) {
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
