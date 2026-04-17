//! Owns CPU-side execution for world, shape, capture, and batch queries.
//! Does not own query planning, public contract selection, or GPU backend
//! execution.
//!
//! Key invariants:
//! - CPU execution remains the semantic oracle that other backends are checked
//!   against.
//! - observability and fallback counters must describe the path that actually
//!   ran, not the path the planner first proposed.
//! - support-bound and witness reuse may save work, but they must not widen the
//!   query contract beyond authored semantics.
//!
//! Primary entrypoints:
//! - `execute_world_query`
//! - `execute_capture_query`
//! - `execute_batch_query`
//!
//! Failure modes / common pitfalls:
//! - forgetting to update observability when a fallback or retry path runs
//!   makes closure reports lie.
//! - reusing cached support or witness data across incompatible execution
//!   policies can silently break oracle trust.

use crate::acceleration::cache::{CacheDisableReason, SupportBrickCache};
#[cfg(feature = "internal-learned-experiments")]
use crate::acceleration::learned::{
    build_cpu_oracle_dataset, maybe_export_learned_oracle_dataset, propose_cpu_oracle_step,
    verify_learned_step,
};
use crate::acceleration::{AccelerationForest, AccelerationLeafPayload, BoundDescriptorKind};
use crate::artifact_contract::ArtifactObserver;
use crate::execution_policy::QueryExecutionPolicy;
use crate::hir;
use crate::hir::body::{BinaryOp, Expr, Literal, UnaryOp};
use crate::kernel::ir::{
    KernelBatchItemContract, KernelBatchQueryPlan, KernelCaptureQueryPlan, KernelWorldQueryPlan,
};
use crate::kernel::{KernelStructValue, KernelValue};
use crate::kernel::{
    KernelValidationError, validate_batch_query_plan, validate_capture_query_plan,
    validate_world_query_plan,
};
use crate::portable;
use crate::query_exec::capture::{self, CaptureQueryBackend, execute_batch_item_contract};
use crate::query_exec::context::QueryExecContext;
use crate::query_exec::ids::stable_shape_capture_id;
use crate::query_exec::region::{select_region_exec_case, world_domain_mismatch_message};
use crate::query_exec::world::{
    NormalRole, WorldDistanceBackend, WorldMediumBackend, WorldNormalBackend, WorldQueryBackend,
    WorldRadianceBackend, WorldSurfaceBackend, WorldTraceBackend, execute_world_distance,
    execute_world_medium, execute_world_normal, execute_world_radiance, execute_world_ray,
    execute_world_surface, world_query_semantics, world_query_semantics_for_contract,
};
use crate::query_exec::{QueryExecutionObservability, QueryTraceSolverMode};
use crate::query_plan::{
    ArtifactContract, ArtifactSchema, BatchQueryKind, WorldQueryKind,
    batch_query_kind_for_contract_id, world_query_kind_for_contract_id,
};
use crate::query_solver::{
    CertificateReuseClass, RaySolverArtifactReuseResolution, RaySolverFallbackReason,
    RaySolverIntentDisposition, RaySolverMethod, RaySolverPlan, RayStepCertificate,
    RayStepCertificateMetadata, RayStepCertificateSubjectKind, RequiredGuaranteeClass,
    SemanticEvidence, StepCertificateKind,
};
use crate::scene_ir::{
    DistanceSemantics, FieldNode, RepeatKind, SceneArgExpr, SceneProfileExpr, SceneValueExpr,
    ShapeLeafRef, ShapeMergeProvenancePolicy, ShapeNode, ShapeProvenanceExpr,
    ShapeSubtractProvenancePolicy, SmoothKind, SupportClass, SupportNodeId, SupportNodeKindSummary,
    SupportPayload, TransformKind,
};
use crate::world_identity::{SnapshotCaptureKind, WorldSnapshotHandle};
use smol_str::SmolStr;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use thiserror::Error;
use wrela_runtime::{
    TypeId, Value as RuntimeValue, wr_affine_transform, wr_bend, wr_box, wr_box_frame,
    wr_capped_cone, wr_capsule, wr_capsule2, wr_circle2, wr_class_new, wr_class_set_slot, wr_cone,
    wr_cylinder, wr_displace, wr_ellipsoid, wr_field_intersection, wr_field_subtract,
    wr_field_sweep_coords, wr_field_union, wr_hex_prism, wr_instance_array,
    wr_instance_array_identity, wr_list_get, wr_list_len, wr_list_new_local, wr_list_push,
    wr_mat3_component, wr_mat3_from_columns, wr_mat4_component, wr_mat4_from_columns,
    wr_mirror_array, wr_mirror_array_identity, wr_plane, wr_polygon2, wr_polyline2, wr_quat_new,
    wr_radial_repeat, wr_radial_repeat_identity, wr_rect2, wr_repeat_grid, wr_repeat_grid_identity,
    wr_repeat_linear, wr_repeat_linear_identity, wr_rotate, wr_rounded_box, wr_rounded_rect2,
    wr_segment2, wr_slab, wr_smooth_intersection, wr_smooth_subtract, wr_smooth_union, wr_sphere,
    wr_taper, wr_torus, wr_transform_normal, wr_translate, wr_triangle_prism, wr_twist, wr_type_id,
    wr_uniform_scale, wr_vec_add, wr_vec_component, wr_vec_div, wr_vec_mul, wr_vec_sub,
    wr_vec2_new, wr_vec3_new, wr_vec4_new, wr_warp,
};

mod backend;
mod evaluator;
mod portable_eval;
mod queries;
mod support_bounds;
mod tracing;
mod values;

#[allow(unused_imports)]
use backend::*;
#[allow(unused_imports)]
use values::*;

pub(crate) use values::{
    combine_medium_values, default_hit, default_medium, default_payload, default_surface,
    hit_value, kernel_to_runtime, medium_value, normalize3, runtime_to_kernel_value,
};

fn snapshot_capture_kind(kind: crate::query_plan::CaptureKind) -> SnapshotCaptureKind {
    match kind {
        crate::query_plan::CaptureKind::Field => SnapshotCaptureKind::Field,
        crate::query_plan::CaptureKind::Shape => SnapshotCaptureKind::Shape,
        crate::query_plan::CaptureKind::Region => SnapshotCaptureKind::Region,
    }
}

fn ready_shared_cache_artifact_count(ctx: &QueryExecContext) -> u32 {
    let catalog = ctx.shared_acceleration.cache_catalog();
    let ready_shape_support = catalog
        .shape_support
        .values()
        .filter(|cache| cache.is_ready())
        .count();
    let ready_shape_distance = catalog
        .shape_distance
        .values()
        .filter(|cache| cache.is_ready())
        .count();
    let ready_world_support = catalog
        .world_support
        .values()
        .filter(|cache| cache.is_ready())
        .count();
    let ready_world_distance = catalog
        .world_distance
        .values()
        .filter(|cache| cache.is_ready())
        .count();
    (ready_shape_support + ready_shape_distance + ready_world_support + ready_world_distance) as u32
}

fn cache_disable_reason_is_budget_pressure(reason: CacheDisableReason) -> bool {
    matches!(
        reason,
        CacheDisableReason::MemoryBudgetExceeded
            | CacheDisableReason::BuildBudgetExhausted
            | CacheDisableReason::UploadBudgetExhausted
            | CacheDisableReason::InsufficientNarrowBandCoverage
    )
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct NormalEvaluation {
    normal: [f32; 3],
    role: NormalRole,
}

#[derive(Debug, Error, Clone, PartialEq)]
pub enum QueryExecError {
    #[error("query execution expected {expected}, found {found}")]
    TypeMismatch { expected: String, found: String },
    #[error("missing capture target for {kind}")]
    MissingCaptureTarget { kind: &'static str },
    #[error("unknown field capture '{name}'")]
    UnknownFieldCapture { name: SmolStr },
    #[error("unknown shape capture '{name}'")]
    UnknownShapeCapture { name: SmolStr },
    #[error("unknown region capture '{name}'")]
    UnknownRegionCapture { name: SmolStr },
    #[error(
        "snapshot epoch mismatch for {kind} capture '{name}': expected {expected}, found {found}"
    )]
    SnapshotEpochMismatch {
        kind: &'static str,
        name: SmolStr,
        expected: u32,
        found: u32,
    },
    #[error("missing scene field '{name}'")]
    MissingField { name: SmolStr },
    #[error("missing scene shape '{name}'")]
    MissingShape { name: SmolStr },
    #[error("missing region '{name}'")]
    MissingRegion { name: SmolStr },
    #[error("missing feature id {feature_id} in shape '{shape}'")]
    MissingFeature { shape: SmolStr, feature_id: u32 },
    #[error("portable function '{name}' was not found")]
    MissingFunction { name: SmolStr },
    #[error("unsupported query operation: {message}")]
    Unsupported { message: String },
}

pub fn execute_capture_query(
    ctx: &QueryExecContext,
    plan: &KernelCaptureQueryPlan,
    args: &[KernelValue],
) -> Result<KernelValue, QueryExecError> {
    execute_capture_query_with_observability(ctx, plan, args).map(|(value, _)| value)
}

pub fn execute_world_query(
    ctx: &QueryExecContext,
    plan: &KernelWorldQueryPlan,
    args: &[KernelValue],
) -> Result<KernelValue, QueryExecError> {
    let policy = QueryExecutionPolicy::conservative(plan.backend, None);
    execute_world_query_with_policy_with_observability(ctx, &policy, plan, args)
        .map(|(value, _)| value)
}

pub(crate) fn execute_batch_query(
    ctx: &QueryExecContext,
    plan: &KernelBatchQueryPlan,
    args: &[KernelValue],
) -> Result<KernelValue, QueryExecError> {
    execute_batch_query_with_observability(ctx, plan, args).map(|(value, _)| value)
}

pub(crate) fn execute_capture_query_with_observability(
    ctx: &QueryExecContext,
    plan: &KernelCaptureQueryPlan,
    args: &[KernelValue],
) -> Result<(KernelValue, QueryExecutionObservability), QueryExecError> {
    execute_capture_query_with_snapshot_observability(ctx, None, plan, args)
}

pub(crate) fn execute_capture_query_with_snapshot_observability(
    ctx: &QueryExecContext,
    snapshot: Option<&WorldSnapshotHandle>,
    plan: &KernelCaptureQueryPlan,
    args: &[KernelValue],
) -> Result<(KernelValue, QueryExecutionObservability), QueryExecError> {
    let ops = DirectQueryOps::new_with_snapshot(ctx, snapshot);
    ops.note_dispatch();
    if let Err(errors) = validate_capture_query_plan(plan) {
        ops.note_contract_validation_failure();
        return Err(validation_error("capture query", errors));
    }
    let value = capture::execute_capture_query(&ops, plan, args)?;
    Ok((value, ops.snapshot_observability()))
}

pub(crate) fn execute_world_query_with_observability(
    ctx: &QueryExecContext,
    plan: &KernelWorldQueryPlan,
    args: &[KernelValue],
) -> Result<(KernelValue, QueryExecutionObservability), QueryExecError> {
    let policy = QueryExecutionPolicy::conservative(plan.backend, None);
    execute_world_query_with_policy_with_observability(ctx, &policy, plan, args)
}

pub(crate) fn execute_world_query_with_policy_with_observability(
    ctx: &QueryExecContext,
    policy: &QueryExecutionPolicy,
    plan: &KernelWorldQueryPlan,
    args: &[KernelValue],
) -> Result<(KernelValue, QueryExecutionObservability), QueryExecError> {
    execute_world_query_with_policy_with_snapshot_observability(ctx, None, policy, plan, args)
}

pub(crate) fn execute_world_query_with_snapshot_observability(
    ctx: &QueryExecContext,
    snapshot: Option<&WorldSnapshotHandle>,
    plan: &KernelWorldQueryPlan,
    args: &[KernelValue],
) -> Result<(KernelValue, QueryExecutionObservability), QueryExecError> {
    let policy = QueryExecutionPolicy::conservative(plan.backend, None);
    execute_world_query_with_policy_with_snapshot_observability(ctx, snapshot, &policy, plan, args)
}

pub(crate) fn execute_world_query_with_policy_with_snapshot_observability(
    ctx: &QueryExecContext,
    snapshot: Option<&WorldSnapshotHandle>,
    _policy: &QueryExecutionPolicy,
    plan: &KernelWorldQueryPlan,
    args: &[KernelValue],
) -> Result<(KernelValue, QueryExecutionObservability), QueryExecError> {
    let evaluator = DirectQueryEvaluator::new_with_snapshot(ctx, snapshot);
    evaluator.note_dispatch();
    if let Err(errors) = validate_world_query_plan(plan) {
        evaluator.note_contract_validation_failure();
        return Err(validation_error("world query", errors));
    }
    let value = evaluator.execute_world_query(plan, args)?;
    Ok((value, evaluator.snapshot_observability()))
}

pub(crate) fn execute_batch_query_with_observability(
    ctx: &QueryExecContext,
    plan: &KernelBatchQueryPlan,
    args: &[KernelValue],
) -> Result<(KernelValue, QueryExecutionObservability), QueryExecError> {
    execute_batch_query_with_snapshot_observability(ctx, None, plan, args)
}

pub(crate) fn execute_batch_query_with_snapshot_observability(
    ctx: &QueryExecContext,
    snapshot: Option<&WorldSnapshotHandle>,
    plan: &KernelBatchQueryPlan,
    args: &[KernelValue],
) -> Result<(KernelValue, QueryExecutionObservability), QueryExecError> {
    execute_batch_query_with_solver_mode_with_snapshot_observability(
        ctx,
        snapshot,
        plan,
        args,
        QueryTraceSolverMode::Hybrid,
    )
}

pub(crate) fn execute_batch_query_with_solver_mode_with_snapshot_observability(
    ctx: &QueryExecContext,
    snapshot: Option<&WorldSnapshotHandle>,
    plan: &KernelBatchQueryPlan,
    args: &[KernelValue],
    solver_mode: QueryTraceSolverMode,
) -> Result<(KernelValue, QueryExecutionObservability), QueryExecError> {
    let evaluator =
        DirectQueryEvaluator::new_with_snapshot_and_solver_mode(ctx, snapshot, solver_mode);
    evaluator.note_dispatch();
    if let Err(errors) = validate_batch_query_plan(plan) {
        evaluator.note_contract_validation_failure();
        return Err(validation_error("batch query", errors));
    }
    let value = evaluator.execute_batch_query(plan, args)?;
    Ok((value, evaluator.snapshot_observability()))
}

pub(crate) fn resolve_batch_capture(
    ctx: &QueryExecContext,
    plan: &KernelBatchQueryPlan,
    capture: Option<&KernelValue>,
) -> Result<SmolStr, QueryExecError> {
    let evaluator = DirectQueryEvaluator::new(ctx);
    match batch_kind_for_plan(plan)? {
        BatchQueryKind::Distance | BatchQueryKind::Normal => {
            evaluator.resolve_field_or_shape_capture(capture)
        }
        BatchQueryKind::Nearest
        | BatchQueryKind::Trace
        | BatchQueryKind::Surface
        | BatchQueryKind::Occluded
        | BatchQueryKind::Radiance
        | BatchQueryKind::Medium => evaluator.resolve_shape_capture(capture),
    }
}

pub(crate) struct DirectQueryOps<'a> {
    ctx: &'a QueryExecContext,
    snapshot: Option<WorldSnapshotHandle>,
    trace_solver_mode: QueryTraceSolverMode,
    observability: Rc<RefCell<QueryExecutionObservability>>,
    world_acceleration_cache:
        Rc<RefCell<HashMap<(SmolStr, i32), Option<CpuAccelerationTree<SmolStr>>>>>,
    shape_union_cache: Rc<RefCell<HashMap<SmolStr, Option<CpuAccelerationTree<usize>>>>>,
}

pub(crate) struct DirectQueryEvaluator<'a> {
    ops: DirectQueryOps<'a>,
}

#[derive(Debug, Clone)]
pub(crate) struct PortableVariable {
    value: KernelValue,
    mutable: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum PortableFlow {
    None,
    Return(KernelValue),
    Break,
    Continue,
}

#[derive(Debug, Clone)]
pub(crate) struct ShapeWinner {
    distance: f32,
    feature_id: u32,
    leaf: Option<ShapeLeafRef>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SupportBounds {
    min: [f32; 3],
    max: [f32; 3],
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct RaySupportInterval {
    start_t: f32,
    end_t: f32,
    starts_inside: bool,
    conservative: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum RaySupportProbe {
    Unavailable,
    Rejected,
    Interval(RaySupportInterval),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CpuChildSpan {
    start: usize,
    len: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CpuAccelerationNode<T> {
    bounds: Option<SupportBounds>,
    child_span: Option<CpuChildSpan>,
    leaf: Option<T>,
    leaf_count: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CpuAccelerationTree<T> {
    root: usize,
    nodes: Vec<CpuAccelerationNode<T>>,
    children: Vec<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct CpuPointTraversal {
    node_index: usize,
    lower_bound: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct CpuRayTraversal {
    node_index: usize,
    start_t: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SupportSummaryParts {
    support_class: SupportClass,
    semantics: DistanceSemantics,
    has_bounds: bool,
    opaque_boundary: bool,
    can_coarse_support_prune: bool,
    bounds: SupportBounds,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ShapeUnionAccelerationCandidate {
    pub index: usize,
    pub bounds: Option<([f32; 3], [f32; 3])>,
}

impl<T> CpuAccelerationTree<T> {
    fn node(&self, index: usize) -> Option<&CpuAccelerationNode<T>> {
        self.nodes.get(index)
    }

    fn children_of(&self, index: usize) -> &[usize] {
        let Some(node) = self.node(index) else {
            return &[];
        };
        let Some(span) = node.child_span else {
            return &[];
        };
        &self.children[span.start..span.start + span.len]
    }

    fn leaf_count(&self, index: usize) -> u32 {
        self.node(index).map(|node| node.leaf_count).unwrap_or(0)
    }
}

fn build_cpu_acceleration_tree_from_forest<T>(
    forest: &AccelerationForest,
    parse_leaf: impl Fn(&AccelerationLeafPayload) -> Option<T>,
) -> Option<CpuAccelerationTree<T>>
where
    T: Clone,
{
    let root_id = forest.root_nodes().first()?;
    let node_lookup = forest
        .nodes
        .iter()
        .map(|node| (node.id.clone(), node))
        .collect::<HashMap<_, _>>();
    let mut built = HashMap::<SmolStr, (usize, u32)>::new();
    let mut nodes = Vec::new();
    let mut children = Vec::new();
    let root = build_cpu_acceleration_subtree_from_forest(
        root_id,
        &node_lookup,
        &parse_leaf,
        &mut built,
        &mut nodes,
        &mut children,
    )?
    .0;
    Some(CpuAccelerationTree {
        root,
        nodes,
        children,
    })
}

fn build_cpu_acceleration_subtree_from_forest<T>(
    id: &SmolStr,
    node_lookup: &HashMap<SmolStr, &crate::acceleration::AccelerationNode>,
    parse_leaf: &impl Fn(&AccelerationLeafPayload) -> Option<T>,
    built: &mut HashMap<SmolStr, (usize, u32)>,
    nodes: &mut Vec<CpuAccelerationNode<T>>,
    children: &mut Vec<usize>,
) -> Option<(usize, u32)>
where
    T: Clone,
{
    if let Some(existing) = built.get(id).copied() {
        return Some(existing);
    }
    let source = node_lookup.get(id)?;
    let mut built_children = Vec::new();
    let mut leaf_count = 0u32;
    for child_id in &source.child_ids {
        let (child_index, child_leaf_count) = build_cpu_acceleration_subtree_from_forest(
            child_id,
            node_lookup,
            parse_leaf,
            built,
            nodes,
            children,
        )?;
        built_children.push(child_index);
        leaf_count += child_leaf_count;
    }
    let leaf = source.leaf_payload.as_ref().and_then(parse_leaf);
    if leaf.is_some() {
        leaf_count = leaf_count.max(1);
    }
    let child_span = if built_children.is_empty() {
        None
    } else {
        let start = children.len();
        children.extend(built_children);
        Some(CpuChildSpan {
            start,
            len: source.child_ids.len(),
        })
    };
    let index = nodes.len();
    nodes.push(CpuAccelerationNode {
        bounds: forest_support_bounds(source),
        child_span,
        leaf,
        leaf_count,
    });
    built.insert(id.clone(), (index, leaf_count));
    Some((index, leaf_count))
}

fn forest_support_bounds(node: &crate::acceleration::AccelerationNode) -> Option<SupportBounds> {
    node.bounds.iter().find_map(|bound| {
        if !matches!(bound.kind, BoundDescriptorKind::AxisAlignedBounds) {
            return None;
        }
        parse_support_bounds_summary(&bound.summary)
    })
}

fn parse_support_bounds_summary(summary: &str) -> Option<SupportBounds> {
    let (min, max) = summary.split_once("|max=")?;
    let min = min.strip_prefix("min=")?;
    Some(SupportBounds {
        min: parse_summary_vec3(min)?,
        max: parse_summary_vec3(max)?,
    })
}

fn parse_summary_vec3(summary: &str) -> Option<[f32; 3]> {
    let parts = summary
        .split(',')
        .map(|part| part.trim().parse::<f32>().ok())
        .collect::<Option<Vec<_>>>()?;
    let [x, y, z] = parts.try_into().ok()?;
    Some([x, y, z])
}

fn pop_best_point_traversal(stack: &mut Vec<CpuPointTraversal>) -> Option<CpuPointTraversal> {
    let best_index = stack
        .iter()
        .enumerate()
        .min_by(|(_, left), (_, right)| {
            left.lower_bound
                .partial_cmp(&right.lower_bound)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(index, _)| index)?;
    Some(stack.swap_remove(best_index))
}

fn push_ordered_point_traversals(
    stack: &mut Vec<CpuPointTraversal>,
    mut items: Vec<CpuPointTraversal>,
) {
    items.sort_by(|left, right| {
        left.lower_bound
            .partial_cmp(&right.lower_bound)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    stack.extend(items.into_iter().rev());
}

fn pop_best_ray_traversal(stack: &mut Vec<CpuRayTraversal>) -> Option<CpuRayTraversal> {
    let best_index = stack
        .iter()
        .enumerate()
        .min_by(|(_, left), (_, right)| {
            left.start_t
                .partial_cmp(&right.start_t)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(index, _)| index)?;
    Some(stack.swap_remove(best_index))
}

fn push_ordered_ray_traversals(stack: &mut Vec<CpuRayTraversal>, mut items: Vec<CpuRayTraversal>) {
    items.sort_by(|left, right| {
        left.start_t
            .partial_cmp(&right.start_t)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    stack.extend(items.into_iter().rev());
}

#[derive(Debug, Clone)]
pub(crate) enum AnalyticRayHit {
    Hit(RayStepCertificate),
    VerificationFailed,
    NotApplicable,
}

#[derive(Debug, Clone)]
pub(crate) enum RepeatAwareTraceOutcome {
    Finished(KernelValue),
    Inapplicable(KernelValue),
    Unsupported(RepeatAwareUnsupportedReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RepeatAwareUnsupportedReason {
    Form,
    Bounds,
}

#[derive(Debug, Clone)]
pub(crate) struct RepeatLinearTraversal<'scene> {
    inner: &'scene FieldNode,
    local_origin: [f32; 3],
    local_direction: [f32; 3],
    bounds: SupportBounds,
    period: [f32; 3],
}

#[derive(Debug, Clone)]
pub(crate) struct TraceLoopPolicy {
    subject: SmolStr,
    enabled_methods: Vec<RaySolverMethod>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct TraceLoopState {
    travel: f32,
    distance: f32,
    adaptive_epsilon: f32,
    sample: IntervalSample,
    step_bound: f32,
    previous_distance: Option<f32>,
    consecutive_small_steps: u32,
    non_improving_distance: bool,
}

#[derive(Debug, Clone)]
pub(crate) enum TraceStepDecision {
    Advance {
        certificate: RayStepCertificate,
        next_sample: Option<IntervalSample>,
    },
    Hit(RayStepCertificate),
    Miss,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct BracketRefinement {
    lo: IntervalSample,
    hi: IntervalSample,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct IntervalSample {
    t: f32,
    distance: f32,
    adaptive_epsilon: f32,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum IntervalProofOutcome {
    NoRoot {
        end_t: f32,
        end_sample: IntervalSample,
    },
    Bracket {
        bracket: BracketRefinement,
    },
    Unresolved,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum AnalyticPrimitive {
    Sphere {
        radius: f32,
    },
    Plane {
        normal: [f32; 3],
        offset: f32,
    },
    Slab {
        thickness: f32,
    },
    Box {
        half: [f32; 3],
    },
    Capsule {
        a: [f32; 3],
        b: [f32; 3],
        radius: f32,
    },
    Cylinder {
        radius: f32,
        half_height: f32,
    },
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct AnalyticPrimitiveRay {
    primitive: AnalyticPrimitive,
    local_origin: [f32; 3],
    local_direction: [f32; 3],
}

impl TraceLoopPolicy {
    fn dense_only(subject: impl Into<SmolStr>) -> Self {
        Self {
            subject: subject.into(),
            enabled_methods: vec![RaySolverMethod::DenseSphereTracing],
        }
    }

    fn from_solver_plan(plan: &RaySolverPlan) -> Self {
        Self {
            subject: plan.subject.clone(),
            enabled_methods: plan.runtime_methods_for_observer(ArtifactObserver::Query),
        }
    }

    fn method_enabled(&self, method: RaySolverMethod) -> bool {
        self.enabled_methods.contains(&method)
    }

    fn is_dense_only(&self) -> bool {
        self.enabled_methods.len() == 1
            && self.enabled_methods[0] == RaySolverMethod::DenseSphereTracing
    }
}

#[derive(Debug, Clone)]
pub(crate) struct FieldLocalFrame<'a> {
    pub(crate) field_name: SmolStr,
    pub(crate) node: &'a FieldNode,
    pub(crate) point: [f32; 3],
    pub(crate) instance_id: u32,
    pub(crate) repeat_id: u32,
}
