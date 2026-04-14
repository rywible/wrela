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
use crate::query_solver::{RaySolverMethod, RayStepCertificateMetadata, StepCertificateKind};
use crate::world_identity::{SnapshotEpoch, SnapshotIdentityReport, WorldSnapshotHandle};
use smol_str::SmolStr;
use std::collections::BTreeMap;

pub use crate::execution_policy::{
    QueryExecutionPolicy, RayBudgetPolicy, RequiredGuaranteeClass, SelectedMethodClass,
};
pub use context::QueryExecContext;
pub use cost::{
    CostFidelity, SemanticCostCause, SemanticCostCauseKind, SemanticCostReport, SemanticCostStage,
    SemanticCostUnit, SemanticQueryScope, SemanticStageKind, render_semantic_cost_report,
};
pub use cpu::QueryExecError;
pub use ids::{
    stable_field_scene_capture_id, stable_field_snapshot_handle, stable_region_scene_capture_id,
    stable_region_snapshot_handle, stable_shape_capture_id, stable_shape_scene_capture_id,
    stable_shape_snapshot_handle,
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
    pub snapshot: Option<SnapshotIdentityReport>,
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
    pub snapshot: Option<SnapshotIdentityReport>,
    pub observability: QueryExecutionObservability,
    pub cost_report: SemanticCostReport,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QueryExecutionObservability {
    pub solver_plan_id: Option<SmolStr>,
    pub solver_subject: Option<SmolStr>,
    pub normal_role: Option<SmolStr>,
    pub solver_methods: Vec<RaySolverMethod>,
    pub step_certificate_kinds: BTreeMap<StepCertificateKind, u32>,
    pub step_certificate_metadata: Vec<RayStepCertificateMetadata>,
    pub acceleration_node_visits: u32,
    pub shape_leaf_visits: u32,
    pub acceleration_pruned_nodes: u32,
    pub union_cluster_visits: u32,
    pub ray_support_interval_rejections: u32,
    pub ray_support_entry_jumps: u32,
    pub repeat_cell_skips: u32,
    pub cache_brick_visits: u32,
    pub cache_brick_hits: u32,
    pub cache_brick_misses: u32,
    pub accepted_relaxed_steps: u32,
    pub rejected_relaxed_steps: u32,
    pub analytic_transformed_hits: u32,
    pub interval_subdivisions: u32,
    pub interval_proof_successes: u32,
    pub observer_continuation_seed_hits: u32,
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
    execute_capture_query_with_snapshot_on(ctx, backend, None, plan, args)
}

pub fn execute_capture_query_with_snapshot_on(
    ctx: &QueryExecContext,
    backend: DispatchBackend,
    snapshot: Option<&WorldSnapshotHandle>,
    plan: &KernelCaptureQueryPlan,
    args: &[KernelValue],
) -> Result<(KernelValue, DirectQueryExecutionTrace), QueryExecError> {
    let identity = trace_identity(plan.contract_id)?;
    let backend = resolve_direct_backend(backend);
    ensure_backend_supported(identity, backend)?;
    let snapshot_report = trace_snapshot_report(snapshot, ctx, args);
    let (value, executor, observability) = match backend {
        DispatchBackend::VirtualGpu => {
            let (value, observability) =
                vgpu::execute_capture_query_with_snapshot_observability(ctx, snapshot, plan, args)?;
            (value, DirectQueryExecutor::VirtualGpu, observability)
        }
        DispatchBackend::Wgsl => {
            let (value, observability) =
                wgsl::execute_capture_query_with_snapshot_observability(ctx, snapshot, plan, args)?;
            (value, DirectQueryExecutor::Wgsl, observability)
        }
        DispatchBackend::Cpu | DispatchBackend::Auto => {
            let (value, observability) =
                cpu::execute_capture_query_with_snapshot_observability(ctx, snapshot, plan, args)?;
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
            snapshot: snapshot_report,
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
    let policy = QueryExecutionPolicy::conservative(requested_backend, None);
    execute_world_query_with_policy_with_trace_on(ctx, requested_backend, &policy, None, plan, args)
}

pub fn execute_world_query_with_policy(
    ctx: &QueryExecContext,
    policy: &QueryExecutionPolicy,
    plan: &KernelWorldQueryPlan,
    args: &[KernelValue],
) -> Result<KernelValue, QueryExecError> {
    execute_world_query_with_policy_with_trace_on(
        ctx,
        policy.backend_preference,
        policy,
        None,
        plan,
        args,
    )
    .map(|(value, _)| value)
}

pub fn execute_world_query_on_with_policy(
    ctx: &QueryExecContext,
    requested_backend: DispatchBackend,
    policy: &QueryExecutionPolicy,
    plan: &KernelWorldQueryPlan,
    args: &[KernelValue],
) -> Result<KernelValue, QueryExecError> {
    execute_world_query_with_policy_with_trace_on(ctx, requested_backend, policy, None, plan, args)
        .map(|(value, _)| value)
}

pub fn execute_world_query_with_policy_with_trace_on(
    ctx: &QueryExecContext,
    requested_backend: DispatchBackend,
    policy: &QueryExecutionPolicy,
    snapshot: Option<&WorldSnapshotHandle>,
    plan: &KernelWorldQueryPlan,
    args: &[KernelValue],
) -> Result<(KernelValue, DirectQueryExecutionTrace), QueryExecError> {
    execute_world_query_with_policy_with_snapshot_on(
        ctx,
        requested_backend,
        snapshot,
        policy,
        plan,
        args,
    )
}

pub fn execute_world_query_with_snapshot_on(
    ctx: &QueryExecContext,
    requested_backend: DispatchBackend,
    snapshot: Option<&WorldSnapshotHandle>,
    plan: &KernelWorldQueryPlan,
    args: &[KernelValue],
) -> Result<(KernelValue, DirectQueryExecutionTrace), QueryExecError> {
    let policy = QueryExecutionPolicy::conservative(requested_backend, None);
    execute_world_query_with_policy_with_snapshot_on(
        ctx,
        requested_backend,
        snapshot,
        &policy,
        plan,
        args,
    )
}

pub fn execute_world_query_with_policy_with_snapshot_on(
    ctx: &QueryExecContext,
    requested_backend: DispatchBackend,
    snapshot: Option<&WorldSnapshotHandle>,
    policy: &QueryExecutionPolicy,
    plan: &KernelWorldQueryPlan,
    args: &[KernelValue],
) -> Result<(KernelValue, DirectQueryExecutionTrace), QueryExecError> {
    let identity = trace_identity(plan.contract_id)?;
    let backend = resolve_world_backend(requested_backend, plan, policy);
    ensure_backend_supported(identity, backend)?;
    ensure_world_policy_legal(backend, policy)?;
    let snapshot_report = trace_snapshot_report(snapshot, ctx, args);
    let (value, executor, observability) = match backend {
        DispatchBackend::VirtualGpu => {
            let (value, observability) =
                vgpu::execute_world_query_with_policy_with_snapshot_observability(
                    ctx, snapshot, policy, plan, args,
                )?;
            (value, DirectQueryExecutor::VirtualGpu, observability)
        }
        DispatchBackend::Wgsl => {
            let (value, observability) =
                wgsl::execute_world_query_with_policy_with_snapshot_observability(
                    ctx, snapshot, policy, plan, args,
                )?;
            (value, DirectQueryExecutor::Wgsl, observability)
        }
        DispatchBackend::Cpu | DispatchBackend::Auto => {
            let (value, observability) =
                cpu::execute_world_query_with_policy_with_snapshot_observability(
                    ctx, snapshot, policy, plan, args,
                )?;
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
            snapshot: snapshot_report,
            cost_report: cost::world_cost_report(backend, policy, plan, &observability),
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
    execute_batch_query_with_snapshot_on(ctx, requested_backend, None, plan, args)
}

pub fn execute_batch_query_with_snapshot_on(
    ctx: &QueryExecContext,
    requested_backend: DispatchBackend,
    snapshot: Option<&WorldSnapshotHandle>,
    plan: &KernelBatchQueryPlan,
    args: &[KernelValue],
) -> Result<(KernelValue, BatchQueryExecutionTrace), QueryExecError> {
    let identity = trace_identity(plan.contract_id)?;
    let item_count = batch_query_item_count(plan, args)?;
    let plan_trace = interpret_batch_query(plan, item_count);
    let backend = resolve_batch_backend(requested_backend, plan);
    ensure_backend_supported(identity, backend)?;
    let snapshot_report = trace_snapshot_report(snapshot, ctx, args);
    let (value, observability) = match backend {
        DispatchBackend::VirtualGpu => vgpu::execute_batch_query_with_snapshot_observability(
            ctx,
            snapshot,
            plan,
            args,
            &plan_trace,
        )?,
        DispatchBackend::Wgsl => wgsl::execute_batch_query_with_snapshot_observability(
            ctx,
            snapshot,
            plan,
            args,
            &plan_trace,
        )?,
        DispatchBackend::Cpu | DispatchBackend::Auto => {
            cpu::execute_batch_query_with_snapshot_observability(ctx, snapshot, plan, args)?
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
            snapshot: snapshot_report,
            plan_trace,
            cost_report,
            observability,
        },
    ))
}

fn trace_snapshot_report(
    snapshot: Option<&WorldSnapshotHandle>,
    ctx: &QueryExecContext,
    args: &[KernelValue],
) -> Option<SnapshotIdentityReport> {
    if let Some(snapshot) = snapshot {
        return Some(snapshot.report());
    }
    let capture = args.first()?;
    match capture {
        KernelValue::Capture(name) => ctx.snapshot_report_for_capture_name(name),
        KernelValue::Struct(value) if value.name.as_str() == "FieldCapture" => value
            .fields
            .iter()
            .find(|(field, _)| field.as_str() == "scene_id")
            .and_then(|(_, value)| match value {
                KernelValue::U32(scene_id) => ctx.field_name_for_scene_id(*scene_id),
                _ => None,
            })
            .and_then(|name| {
                report_for_struct_capture(
                    ctx.field_snapshot_handle(name),
                    expect_struct_u32_field(value, "epoch")
                        .map(|epoch| SnapshotEpoch(u64::from(epoch))),
                )
            }),
        KernelValue::Struct(value) if value.name.as_str() == "ShapeCapture" => value
            .fields
            .iter()
            .find(|(field, _)| field.as_str() == "root_feature_id")
            .and_then(|(_, value)| match value {
                KernelValue::U32(root_feature_id) => {
                    ctx.shape_name_for_root_feature_id(*root_feature_id)
                }
                _ => None,
            })
            .and_then(|name| {
                report_for_struct_capture(
                    ctx.shape_snapshot_handle(name),
                    expect_struct_u32_field(value, "epoch")
                        .map(|epoch| SnapshotEpoch(u64::from(epoch))),
                )
            }),
        KernelValue::Struct(value) if value.name.as_str() == "RegionCapture" => value
            .fields
            .iter()
            .find(|(field, _)| field.as_str() == "scene_id")
            .and_then(|(_, value)| match value {
                KernelValue::U32(scene_id) => ctx.region_name_for_scene_id(*scene_id),
                _ => None,
            })
            .and_then(|name| {
                report_for_struct_capture(
                    ctx.region_snapshot_handle(name),
                    expect_struct_u32_field(value, "epoch")
                        .map(|epoch| SnapshotEpoch(u64::from(epoch))),
                )
            }),
        _ => None,
    }
}

fn report_for_struct_capture(
    snapshot: Option<&WorldSnapshotHandle>,
    epoch: Option<SnapshotEpoch>,
) -> Option<SnapshotIdentityReport> {
    let snapshot = snapshot?;
    Some(
        epoch
            .map(|epoch| snapshot.with_epoch(epoch))
            .unwrap_or_else(|| snapshot.clone())
            .report(),
    )
}

fn expect_struct_u32_field(
    value: &crate::kernel::KernelStructValue,
    field_name: &str,
) -> Option<u32> {
    value.fields.iter().find_map(|(field, value)| {
        (field.as_str() == field_name)
            .then_some(value)
            .and_then(|value| match value {
                KernelValue::U32(value) => Some(*value),
                _ => None,
            })
    })
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
    policy: &QueryExecutionPolicy,
) -> DispatchBackend {
    match requested_backend {
        DispatchBackend::Cpu => DispatchBackend::Cpu,
        DispatchBackend::VirtualGpu => DispatchBackend::VirtualGpu,
        DispatchBackend::Wgsl => DispatchBackend::Wgsl,
        DispatchBackend::Auto => match policy.backend_preference {
            DispatchBackend::Cpu => DispatchBackend::Cpu,
            DispatchBackend::VirtualGpu => DispatchBackend::VirtualGpu,
            DispatchBackend::Wgsl => DispatchBackend::Wgsl,
            DispatchBackend::Auto => match plan.backend {
                DispatchBackend::Cpu | DispatchBackend::Auto => DispatchBackend::Cpu,
                DispatchBackend::VirtualGpu => DispatchBackend::VirtualGpu,
                DispatchBackend::Wgsl => DispatchBackend::Wgsl,
            },
        },
    }
}

fn ensure_world_policy_legal(
    backend: DispatchBackend,
    policy: &QueryExecutionPolicy,
) -> Result<(), QueryExecError> {
    if matches!(backend, DispatchBackend::Cpu | DispatchBackend::Auto) {
        return Ok(());
    }
    if matches!(policy.required_guarantee, RequiredGuaranteeClass::Exact)
        || matches!(policy.selected_method, SelectedMethodClass::ExactOracle)
    {
        return Err(QueryExecError::Unsupported {
            message: format!(
                "{backend:?} backend cannot satisfy execution policy required_guarantee={} selected_method={}",
                policy.required_guarantee.name(),
                policy.selected_method.name()
            ),
        });
    }
    Ok(())
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
