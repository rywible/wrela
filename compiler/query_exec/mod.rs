mod capture;
pub(crate) mod cost;
pub(crate) mod gpu_dispatch;
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

use crate::gpu_runtime::GpuRuntimeMetrics;
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
use std::env;

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

pub const WGSL_WORKGROUP_SIZE_OVERRIDE_ENV: &str = "WRELA_WGSL_WORKGROUP_SIZE";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum QueryTraceSolverMode {
    Hybrid,
    DenseOnly,
}

pub const QUERY_WGSL_LEGAL_WORKGROUP_SIZES: [u32; 3] = [32, 64, 128];

pub fn validate_query_wgsl_workgroup_size(
    requested_workgroup_size: u32,
    adapter_limits: &wgpu::Limits,
) -> Result<u32, QueryExecError> {
    if !QUERY_WGSL_LEGAL_WORKGROUP_SIZES.contains(&requested_workgroup_size) {
        return Err(QueryExecError::Unsupported {
            message: format!(
                "native WGSL workgroup size {requested_workgroup_size} is not in the legal set {:?}",
                QUERY_WGSL_LEGAL_WORKGROUP_SIZES
            ),
        });
    }
    if requested_workgroup_size > adapter_limits.max_compute_workgroup_size_x
        || requested_workgroup_size > adapter_limits.max_compute_invocations_per_workgroup
    {
        return Err(QueryExecError::Unsupported {
            message: format!(
                "native WGSL workgroup size {requested_workgroup_size} is incompatible with adapter limits x={} invocations={}",
                adapter_limits.max_compute_workgroup_size_x,
                adapter_limits.max_compute_invocations_per_workgroup
            ),
        });
    }
    Ok(requested_workgroup_size)
}

fn supported_query_wgsl_workgroup_sizes(adapter_limits: &wgpu::Limits) -> Vec<u32> {
    QUERY_WGSL_LEGAL_WORKGROUP_SIZES
        .iter()
        .copied()
        .filter_map(|candidate| validate_query_wgsl_workgroup_size(candidate, adapter_limits).ok())
        .collect()
}

fn requested_query_wgsl_workgroup_size_override() -> Result<Option<u32>, QueryExecError> {
    env::var(WGSL_WORKGROUP_SIZE_OVERRIDE_ENV)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(|value| {
            value
                .trim()
                .parse::<u32>()
                .map_err(|_| QueryExecError::Unsupported {
                    message: format!(
                        "{WGSL_WORKGROUP_SIZE_OVERRIDE_ENV} must be an integer legal WGSL workgroup size in {:?}, found `{}`",
                        QUERY_WGSL_LEGAL_WORKGROUP_SIZES,
                        value.trim()
                    ),
                })
        })
        .transpose()
}

pub fn select_query_wgsl_workgroup_size(
    adapter_limits: &wgpu::Limits,
) -> Result<u32, QueryExecError> {
    let supported_sizes = supported_query_wgsl_workgroup_sizes(adapter_limits);
    if supported_sizes.is_empty() {
        return Err(QueryExecError::Unsupported {
            message: format!(
                "native WGSL adapter does not support any legal workgroup size in {:?}",
                QUERY_WGSL_LEGAL_WORKGROUP_SIZES
            ),
        });
    }
    if let Some(requested_workgroup_size) = requested_query_wgsl_workgroup_size_override()? {
        return validate_query_wgsl_workgroup_size(requested_workgroup_size, adapter_limits);
    }
    supported_sizes
        .last()
        .copied()
        .ok_or_else(|| QueryExecError::Unsupported {
            message: "native WGSL adapter reported no supported workgroup sizes".to_string(),
        })
}

impl QueryTraceSolverMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Hybrid => "hybrid",
            Self::DenseOnly => "dense-only",
        }
    }
}

pub fn legal_wgsl_workgroup_sizes() -> &'static [u32] {
    &QUERY_WGSL_LEGAL_WORKGROUP_SIZES
}

pub fn supported_wgsl_workgroup_sizes() -> Result<Vec<u32>, QueryExecError> {
    let native = wgsl::native_wgpu_context()?;
    Ok(QUERY_WGSL_LEGAL_WORKGROUP_SIZES
        .iter()
        .copied()
        .filter(|size| validate_query_wgsl_workgroup_size(*size, &native.adapter_limits).is_ok())
        .collect())
}

pub fn selected_wgsl_workgroup_size() -> Result<u32, QueryExecError> {
    let native = wgsl::native_wgpu_context()?;
    select_query_wgsl_workgroup_size(&native.adapter_limits)
}

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
    pub cache_interval_advances: u32,
    pub cache_resident_shared_snapshot_artifacts: u32,
    pub cache_resident_observer_local_artifacts: u32,
    pub cache_upload_attempts: u32,
    pub cache_upload_rejections: u32,
    pub cache_budget_rejections: u32,
    pub cache_dense_fallback_rays: u32,
    pub accepted_relaxed_steps: u32,
    pub rejected_relaxed_steps: u32,
    pub solver_relaxed_attempts: u32,
    pub solver_relaxed_no_root_advances: u32,
    pub solver_relaxed_brackets: u32,
    pub solver_relaxed_unresolved: u32,
    pub solver_interval_attempts: u32,
    pub solver_interval_no_root_advances: u32,
    pub solver_interval_brackets: u32,
    pub solver_interval_unresolved: u32,
    pub solver_refinement_attempts: u32,
    pub solver_refinement_failures: u32,
    pub solver_repeat_attempts: u32,
    pub solver_repeat_supported: u32,
    pub solver_repeat_inapplicable: u32,
    pub solver_repeat_unsupported: u32,
    pub solver_repeat_unsupported_form: u32,
    pub solver_repeat_unsupported_bounds: u32,
    pub solver_repeat_cells_enumerated: u32,
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
    pub learned_step_selected: u32,
    pub learned_step_verified: u32,
    pub learned_step_rejected: u32,
    pub learned_step_bypassed: u32,
    pub learned_verifier_acceptances: u32,
    pub learned_verifier_fallbacks: u32,
    pub solver_certificate_failures: u32,
    pub solver_continuation_available: u32,
    pub solver_continuation_consumed: u32,
    pub solver_continuation_rejected: u32,
    pub solver_continuation_unavailable: u32,
    pub field_samples: u32,
    pub contract_validation_failures: u32,
    pub wgsl_world_helper_path: Option<SmolStr>,
    pub wgsl_layout_signature: Option<u64>,
    pub wgsl_bind_group_count: u32,
    pub wgsl_requested_max_storage_buffer_bytes: u64,
    pub wgsl_used_max_storage_buffer_bytes: u64,
    pub wgsl_selected_workgroup_size: u32,
    pub gpu_runtime: GpuRuntimeMetrics,
}

impl QueryExecutionObservability {
    pub fn merge_from(&mut self, other: &Self) {
        if self.solver_plan_id.is_none() {
            self.solver_plan_id = other.solver_plan_id.clone();
        }
        if self.solver_subject.is_none() {
            self.solver_subject = other.solver_subject.clone();
        }
        if self.normal_role.is_none() {
            self.normal_role = other.normal_role.clone();
        }
        for method in &other.solver_methods {
            if !self.solver_methods.contains(method) {
                self.solver_methods.push(*method);
            }
        }
        for (kind, count) in &other.step_certificate_kinds {
            *self.step_certificate_kinds.entry(*kind).or_default() = self
                .step_certificate_kinds
                .get(kind)
                .copied()
                .unwrap_or_default()
                .saturating_add(*count);
        }
        for metadata in &other.step_certificate_metadata {
            if !self.step_certificate_metadata.contains(metadata) {
                self.step_certificate_metadata.push(metadata.clone());
            }
        }
        self.acceleration_node_visits = self
            .acceleration_node_visits
            .saturating_add(other.acceleration_node_visits);
        self.shape_leaf_visits = self
            .shape_leaf_visits
            .saturating_add(other.shape_leaf_visits);
        self.acceleration_pruned_nodes = self
            .acceleration_pruned_nodes
            .saturating_add(other.acceleration_pruned_nodes);
        self.union_cluster_visits = self
            .union_cluster_visits
            .saturating_add(other.union_cluster_visits);
        self.ray_support_interval_rejections = self
            .ray_support_interval_rejections
            .saturating_add(other.ray_support_interval_rejections);
        self.ray_support_entry_jumps = self
            .ray_support_entry_jumps
            .saturating_add(other.ray_support_entry_jumps);
        self.repeat_cell_skips = self
            .repeat_cell_skips
            .saturating_add(other.repeat_cell_skips);
        self.cache_brick_visits = self
            .cache_brick_visits
            .saturating_add(other.cache_brick_visits);
        self.cache_brick_hits = self.cache_brick_hits.saturating_add(other.cache_brick_hits);
        self.cache_brick_misses = self
            .cache_brick_misses
            .saturating_add(other.cache_brick_misses);
        self.cache_interval_advances = self
            .cache_interval_advances
            .saturating_add(other.cache_interval_advances);
        self.cache_resident_shared_snapshot_artifacts = self
            .cache_resident_shared_snapshot_artifacts
            .saturating_add(other.cache_resident_shared_snapshot_artifacts);
        self.cache_resident_observer_local_artifacts = self
            .cache_resident_observer_local_artifacts
            .saturating_add(other.cache_resident_observer_local_artifacts);
        self.cache_upload_attempts = self
            .cache_upload_attempts
            .saturating_add(other.cache_upload_attempts);
        self.cache_upload_rejections = self
            .cache_upload_rejections
            .saturating_add(other.cache_upload_rejections);
        self.cache_budget_rejections = self
            .cache_budget_rejections
            .saturating_add(other.cache_budget_rejections);
        self.cache_dense_fallback_rays = self
            .cache_dense_fallback_rays
            .saturating_add(other.cache_dense_fallback_rays);
        self.accepted_relaxed_steps = self
            .accepted_relaxed_steps
            .saturating_add(other.accepted_relaxed_steps);
        self.rejected_relaxed_steps = self
            .rejected_relaxed_steps
            .saturating_add(other.rejected_relaxed_steps);
        self.solver_relaxed_attempts = self
            .solver_relaxed_attempts
            .saturating_add(other.solver_relaxed_attempts);
        self.solver_relaxed_no_root_advances = self
            .solver_relaxed_no_root_advances
            .saturating_add(other.solver_relaxed_no_root_advances);
        self.solver_relaxed_brackets = self
            .solver_relaxed_brackets
            .saturating_add(other.solver_relaxed_brackets);
        self.solver_relaxed_unresolved = self
            .solver_relaxed_unresolved
            .saturating_add(other.solver_relaxed_unresolved);
        self.solver_interval_attempts = self
            .solver_interval_attempts
            .saturating_add(other.solver_interval_attempts);
        self.solver_interval_no_root_advances = self
            .solver_interval_no_root_advances
            .saturating_add(other.solver_interval_no_root_advances);
        self.solver_interval_brackets = self
            .solver_interval_brackets
            .saturating_add(other.solver_interval_brackets);
        self.solver_interval_unresolved = self
            .solver_interval_unresolved
            .saturating_add(other.solver_interval_unresolved);
        self.solver_refinement_attempts = self
            .solver_refinement_attempts
            .saturating_add(other.solver_refinement_attempts);
        self.solver_refinement_failures = self
            .solver_refinement_failures
            .saturating_add(other.solver_refinement_failures);
        self.solver_repeat_attempts = self
            .solver_repeat_attempts
            .saturating_add(other.solver_repeat_attempts);
        self.solver_repeat_supported = self
            .solver_repeat_supported
            .saturating_add(other.solver_repeat_supported);
        self.solver_repeat_inapplicable = self
            .solver_repeat_inapplicable
            .saturating_add(other.solver_repeat_inapplicable);
        self.solver_repeat_unsupported = self
            .solver_repeat_unsupported
            .saturating_add(other.solver_repeat_unsupported);
        self.solver_repeat_unsupported_form = self
            .solver_repeat_unsupported_form
            .saturating_add(other.solver_repeat_unsupported_form);
        self.solver_repeat_unsupported_bounds = self
            .solver_repeat_unsupported_bounds
            .saturating_add(other.solver_repeat_unsupported_bounds);
        self.solver_repeat_cells_enumerated = self
            .solver_repeat_cells_enumerated
            .saturating_add(other.solver_repeat_cells_enumerated);
        self.analytic_transformed_hits = self
            .analytic_transformed_hits
            .saturating_add(other.analytic_transformed_hits);
        self.interval_subdivisions = self
            .interval_subdivisions
            .saturating_add(other.interval_subdivisions);
        self.interval_proof_successes = self
            .interval_proof_successes
            .saturating_add(other.interval_proof_successes);
        self.observer_continuation_seed_hits = self
            .observer_continuation_seed_hits
            .saturating_add(other.observer_continuation_seed_hits);
        self.dispatch_count = self.dispatch_count.saturating_add(other.dispatch_count);
        self.dispatch_items = self.dispatch_items.saturating_add(other.dispatch_items);
        self.dispatch_workgroups_x = self.dispatch_workgroups_x.max(other.dispatch_workgroups_x);
        self.dispatch_workgroups_y = self.dispatch_workgroups_y.max(other.dispatch_workgroups_y);
        self.dispatch_workgroups_z = self.dispatch_workgroups_z.max(other.dispatch_workgroups_z);
        self.screen_sample_count = self
            .screen_sample_count
            .saturating_add(other.screen_sample_count);
        self.world_batch_item_count = self
            .world_batch_item_count
            .saturating_add(other.world_batch_item_count);
        self.candidate_count = self.candidate_count.saturating_add(other.candidate_count);
        self.candidates_before_pruning = self
            .candidates_before_pruning
            .saturating_add(other.candidates_before_pruning);
        self.candidates_after_pruning = self
            .candidates_after_pruning
            .saturating_add(other.candidates_after_pruning);
        self.branch_visits = self.branch_visits.saturating_add(other.branch_visits);
        self.support_pruned_candidates = self
            .support_pruned_candidates
            .saturating_add(other.support_pruned_candidates);
        self.artifact_loads = self.artifact_loads.saturating_add(other.artifact_loads);
        self.opaque_fallbacks = self.opaque_fallbacks.saturating_add(other.opaque_fallbacks);
        self.trace_steps = self.trace_steps.saturating_add(other.trace_steps);
        self.trace_steps_max = self.trace_steps_max.max(other.trace_steps_max);
        self.hit_count = self.hit_count.saturating_add(other.hit_count);
        self.miss_count = self.miss_count.saturating_add(other.miss_count);
        self.dense_compatibility_batches = self
            .dense_compatibility_batches
            .saturating_add(other.dense_compatibility_batches);
        self.semantic_pruned_batches = self
            .semantic_pruned_batches
            .saturating_add(other.semantic_pruned_batches);
        self.solver_analytic_hits = self
            .solver_analytic_hits
            .saturating_add(other.solver_analytic_hits);
        self.solver_support_rejections = self
            .solver_support_rejections
            .saturating_add(other.solver_support_rejections);
        self.solver_interval_skips = self
            .solver_interval_skips
            .saturating_add(other.solver_interval_skips);
        self.solver_packet_tile_rejections = self
            .solver_packet_tile_rejections
            .saturating_add(other.solver_packet_tile_rejections);
        self.solver_newton_refinements = self
            .solver_newton_refinements
            .saturating_add(other.solver_newton_refinements);
        self.solver_lipschitz_steps = self
            .solver_lipschitz_steps
            .saturating_add(other.solver_lipschitz_steps);
        self.solver_adaptive_epsilon_uses = self
            .solver_adaptive_epsilon_uses
            .saturating_add(other.solver_adaptive_epsilon_uses);
        self.solver_dense_fallback_rays = self
            .solver_dense_fallback_rays
            .saturating_add(other.solver_dense_fallback_rays);
        self.solver_generated_dense_fallback_rays = self
            .solver_generated_dense_fallback_rays
            .saturating_add(other.solver_generated_dense_fallback_rays);
        self.solver_fallback_contract_dense = self
            .solver_fallback_contract_dense
            .saturating_add(other.solver_fallback_contract_dense);
        self.solver_fallback_missing_facts = self
            .solver_fallback_missing_facts
            .saturating_add(other.solver_fallback_missing_facts);
        self.solver_fallback_analytic_unsupported = self
            .solver_fallback_analytic_unsupported
            .saturating_add(other.solver_fallback_analytic_unsupported);
        self.solver_fallback_verification_failed = self
            .solver_fallback_verification_failed
            .saturating_add(other.solver_fallback_verification_failed);
        self.solver_fallback_unsupported_backend = self
            .solver_fallback_unsupported_backend
            .saturating_add(other.solver_fallback_unsupported_backend);
        self.learned_step_selected = self
            .learned_step_selected
            .saturating_add(other.learned_step_selected);
        self.learned_step_verified = self
            .learned_step_verified
            .saturating_add(other.learned_step_verified);
        self.learned_step_rejected = self
            .learned_step_rejected
            .saturating_add(other.learned_step_rejected);
        self.learned_step_bypassed = self
            .learned_step_bypassed
            .saturating_add(other.learned_step_bypassed);
        self.learned_verifier_acceptances = self
            .learned_verifier_acceptances
            .saturating_add(other.learned_verifier_acceptances);
        self.learned_verifier_fallbacks = self
            .learned_verifier_fallbacks
            .saturating_add(other.learned_verifier_fallbacks);
        self.solver_certificate_failures = self
            .solver_certificate_failures
            .saturating_add(other.solver_certificate_failures);
        self.solver_continuation_available = self
            .solver_continuation_available
            .saturating_add(other.solver_continuation_available);
        self.solver_continuation_consumed = self
            .solver_continuation_consumed
            .saturating_add(other.solver_continuation_consumed);
        self.solver_continuation_rejected = self
            .solver_continuation_rejected
            .saturating_add(other.solver_continuation_rejected);
        self.solver_continuation_unavailable = self
            .solver_continuation_unavailable
            .saturating_add(other.solver_continuation_unavailable);
        self.field_samples = self.field_samples.saturating_add(other.field_samples);
        self.contract_validation_failures = self
            .contract_validation_failures
            .saturating_add(other.contract_validation_failures);
        if self.wgsl_world_helper_path.is_none() {
            self.wgsl_world_helper_path = other.wgsl_world_helper_path.clone();
        }
        if self.wgsl_layout_signature.is_none() {
            self.wgsl_layout_signature = other.wgsl_layout_signature;
        }
        self.wgsl_bind_group_count = self.wgsl_bind_group_count.max(other.wgsl_bind_group_count);
        self.wgsl_requested_max_storage_buffer_bytes = self
            .wgsl_requested_max_storage_buffer_bytes
            .max(other.wgsl_requested_max_storage_buffer_bytes);
        self.wgsl_used_max_storage_buffer_bytes = self
            .wgsl_used_max_storage_buffer_bytes
            .max(other.wgsl_used_max_storage_buffer_bytes);
        self.wgsl_selected_workgroup_size = self
            .wgsl_selected_workgroup_size
            .max(other.wgsl_selected_workgroup_size);
        self.gpu_runtime.merge_from(&other.gpu_runtime);
    }

    pub fn learned_verifier_acceptance_rate(&self) -> Option<f32> {
        if self.learned_step_verified == 0 {
            None
        } else {
            Some(self.learned_verifier_acceptances as f32 / self.learned_step_verified as f32)
        }
    }

    pub fn learned_verifier_fallback_rate(&self) -> Option<f32> {
        if self.learned_step_selected == 0 {
            None
        } else {
            Some(self.learned_verifier_fallbacks as f32 / self.learned_step_selected as f32)
        }
    }
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
    execute_batch_query_with_solver_mode_with_snapshot_on(
        ctx,
        requested_backend,
        snapshot,
        plan,
        args,
        QueryTraceSolverMode::Hybrid,
    )
}

pub fn execute_batch_query_with_solver_mode_with_snapshot_on(
    ctx: &QueryExecContext,
    requested_backend: DispatchBackend,
    snapshot: Option<&WorldSnapshotHandle>,
    plan: &KernelBatchQueryPlan,
    args: &[KernelValue],
    solver_mode: QueryTraceSolverMode,
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
            cpu::execute_batch_query_with_solver_mode_with_snapshot_observability(
                ctx,
                snapshot,
                plan,
                args,
                solver_mode,
            )?
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
