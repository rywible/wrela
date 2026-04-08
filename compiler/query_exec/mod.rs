mod capture;

pub mod context;
pub mod cpu;
pub mod ids;
pub mod mir;
pub mod region;
pub mod spec;
pub mod vgpu;
pub mod world;

use crate::kernel::KernelValue;
use crate::kernel::ir::{KernelBatchQueryPlan, KernelCaptureQueryPlan, KernelWorldQueryPlan};
use crate::kernel::{KernelBatchQueryTrace, interpret_batch_query};
use crate::query_plan::DispatchBackend;

pub use context::QueryExecContext;
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectQueryExecutionTrace {
    pub backend: DispatchBackend,
    pub executor: DirectQueryExecutor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectQueryExecutor {
    Cpu,
    VirtualGpu,
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
    let (value, executor) = match backend {
        DispatchBackend::VirtualGpu => (
            vgpu::execute_capture_query(ctx, plan, args)?,
            DirectQueryExecutor::VirtualGpu,
        ),
        DispatchBackend::Cpu | DispatchBackend::Auto => {
            (
                cpu::execute_capture_query(ctx, plan, args)?,
                DirectQueryExecutor::Cpu,
            )
        }
    };
    Ok((value, DirectQueryExecutionTrace { backend, executor }))
}

pub fn execute_world_query(
    ctx: &QueryExecContext,
    plan: &KernelWorldQueryPlan,
    args: &[KernelValue],
) -> Result<KernelValue, QueryExecError> {
    execute_world_query_with_trace_on(ctx, DispatchBackend::Cpu, plan, args).map(|(value, _)| value)
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
    backend: DispatchBackend,
    plan: &KernelWorldQueryPlan,
    args: &[KernelValue],
) -> Result<(KernelValue, DirectQueryExecutionTrace), QueryExecError> {
    let backend = resolve_direct_backend(backend);
    let (value, executor) = match backend {
        DispatchBackend::VirtualGpu => (
            vgpu::execute_world_query(ctx, plan, args)?,
            DirectQueryExecutor::VirtualGpu,
        ),
        DispatchBackend::Cpu | DispatchBackend::Auto => (
            cpu::execute_world_query(ctx, plan, args)?,
            DirectQueryExecutor::Cpu,
        ),
    };
    Ok((value, DirectQueryExecutionTrace { backend, executor }))
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
    let value = match backend {
        DispatchBackend::VirtualGpu => vgpu::execute_batch_query(ctx, plan, args, &plan_trace)?,
        DispatchBackend::Cpu | DispatchBackend::Auto => cpu::execute_batch_query(ctx, plan, args)?,
    };
    Ok((
        value,
        BatchQueryExecutionTrace {
            backend,
            plan_trace,
        },
    ))
}

fn resolve_direct_backend(backend: DispatchBackend) -> DispatchBackend {
    match backend {
        DispatchBackend::VirtualGpu => DispatchBackend::VirtualGpu,
        DispatchBackend::Cpu | DispatchBackend::Auto => DispatchBackend::Cpu,
    }
}

fn resolve_batch_backend(
    requested_backend: DispatchBackend,
    plan: &KernelBatchQueryPlan,
) -> DispatchBackend {
    match requested_backend {
        DispatchBackend::Cpu => DispatchBackend::Cpu,
        DispatchBackend::VirtualGpu => DispatchBackend::VirtualGpu,
        DispatchBackend::Auto => {
            if matches!(plan.backend, DispatchBackend::VirtualGpu)
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
