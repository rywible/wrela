use crate::acceleration::{AccelerationForest, AccelerationLeafPayload, BoundDescriptorKind};
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

fn snapshot_capture_kind(kind: crate::query_plan::CaptureKind) -> SnapshotCaptureKind {
    match kind {
        crate::query_plan::CaptureKind::Field => SnapshotCaptureKind::Field,
        crate::query_plan::CaptureKind::Shape => SnapshotCaptureKind::Shape,
        crate::query_plan::CaptureKind::Region => SnapshotCaptureKind::Region,
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct NormalEvaluation {
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
struct PortableVariable {
    value: KernelValue,
    mutable: bool,
}

#[derive(Debug, Clone, PartialEq)]
enum PortableFlow {
    None,
    Return(KernelValue),
    Break,
    Continue,
}

#[derive(Debug, Clone)]
struct ShapeWinner {
    distance: f32,
    feature_id: u32,
    leaf: Option<ShapeLeafRef>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct SupportBounds {
    min: [f32; 3],
    max: [f32; 3],
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct RaySupportInterval {
    start_t: f32,
    end_t: f32,
    starts_inside: bool,
    conservative: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum RaySupportProbe {
    Unavailable,
    Rejected,
    Interval(RaySupportInterval),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CpuChildSpan {
    start: usize,
    len: usize,
}

#[derive(Debug, Clone, PartialEq)]
struct CpuAccelerationNode<T> {
    bounds: Option<SupportBounds>,
    child_span: Option<CpuChildSpan>,
    leaf: Option<T>,
    leaf_count: u32,
}

#[derive(Debug, Clone, PartialEq)]
struct CpuAccelerationTree<T> {
    root: usize,
    nodes: Vec<CpuAccelerationNode<T>>,
    children: Vec<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct CpuPointTraversal {
    node_index: usize,
    lower_bound: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct CpuRayTraversal {
    node_index: usize,
    start_t: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct SupportSummaryParts {
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
enum AnalyticRayHit {
    Hit(RayStepCertificate),
    VerificationFailed,
    NotApplicable,
}

#[derive(Debug, Clone)]
enum RepeatAwareTraceOutcome {
    Finished(KernelValue),
    Inapplicable(KernelValue),
    Unsupported(RepeatAwareUnsupportedReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RepeatAwareUnsupportedReason {
    Form,
    Bounds,
}

#[derive(Debug, Clone)]
struct RepeatLinearTraversal<'scene> {
    inner: &'scene FieldNode,
    local_origin: [f32; 3],
    local_direction: [f32; 3],
    bounds: SupportBounds,
    period: [f32; 3],
}

#[derive(Debug, Clone)]
struct TraceLoopPolicy {
    subject: SmolStr,
    enabled_methods: Vec<RaySolverMethod>,
}

#[derive(Debug, Clone, Copy)]
struct TraceLoopState {
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
enum TraceStepDecision {
    Advance {
        certificate: RayStepCertificate,
        next_sample: Option<IntervalSample>,
    },
    Hit(RayStepCertificate),
    Miss,
}

#[derive(Debug, Clone, Copy)]
struct BracketRefinement {
    lo: IntervalSample,
    hi: IntervalSample,
}

#[derive(Debug, Clone, Copy)]
struct IntervalSample {
    t: f32,
    distance: f32,
    adaptive_epsilon: f32,
}

#[derive(Debug, Clone, Copy)]
enum IntervalProofOutcome {
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
enum AnalyticPrimitive {
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
struct AnalyticPrimitiveRay {
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
            enabled_methods: plan.diagnostic_summary().methods,
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

impl<'a> std::ops::Deref for DirectQueryEvaluator<'a> {
    type Target = DirectQueryOps<'a>;

    fn deref(&self) -> &Self::Target {
        &self.ops
    }
}

impl<'a> DirectQueryEvaluator<'a> {
    pub(crate) fn new(ctx: &'a QueryExecContext) -> Self {
        Self::new_with_snapshot_and_solver_mode(ctx, None, QueryTraceSolverMode::Hybrid)
    }

    pub(crate) fn new_with_snapshot(
        ctx: &'a QueryExecContext,
        snapshot: Option<&WorldSnapshotHandle>,
    ) -> Self {
        Self::new_with_snapshot_and_solver_mode(ctx, snapshot, QueryTraceSolverMode::Hybrid)
    }

    pub(crate) fn new_with_snapshot_and_solver_mode(
        ctx: &'a QueryExecContext,
        snapshot: Option<&WorldSnapshotHandle>,
        solver_mode: QueryTraceSolverMode,
    ) -> Self {
        let observability = Rc::new(RefCell::new(QueryExecutionObservability::default()));
        Self {
            ops: DirectQueryOps::with_observability_and_snapshot_and_solver_mode(
                ctx,
                snapshot,
                observability,
                solver_mode,
            ),
        }
    }

    pub(crate) fn snapshot_observability(&self) -> QueryExecutionObservability {
        self.ops.snapshot_observability()
    }

    pub(crate) fn note_dispatch(&self) {
        self.ops.note_dispatch();
    }
}

impl<'a> DirectQueryOps<'a> {
    pub(crate) fn new(ctx: &'a QueryExecContext) -> Self {
        Self::new_with_snapshot_and_solver_mode(ctx, None, QueryTraceSolverMode::Hybrid)
    }

    pub(crate) fn new_with_snapshot(
        ctx: &'a QueryExecContext,
        snapshot: Option<&WorldSnapshotHandle>,
    ) -> Self {
        Self::new_with_snapshot_and_solver_mode(ctx, snapshot, QueryTraceSolverMode::Hybrid)
    }

    pub(crate) fn new_with_snapshot_and_solver_mode(
        ctx: &'a QueryExecContext,
        snapshot: Option<&WorldSnapshotHandle>,
        solver_mode: QueryTraceSolverMode,
    ) -> Self {
        Self::with_observability_and_snapshot_and_solver_mode(
            ctx,
            snapshot,
            Rc::new(RefCell::new(QueryExecutionObservability::default())),
            solver_mode,
        )
    }

    pub(crate) fn with_observability(
        ctx: &'a QueryExecContext,
        observability: Rc<RefCell<QueryExecutionObservability>>,
    ) -> Self {
        Self::with_observability_and_snapshot_and_solver_mode(
            ctx,
            None,
            observability,
            QueryTraceSolverMode::Hybrid,
        )
    }

    pub(crate) fn with_observability_and_snapshot(
        ctx: &'a QueryExecContext,
        snapshot: Option<&WorldSnapshotHandle>,
        observability: Rc<RefCell<QueryExecutionObservability>>,
    ) -> Self {
        Self::with_observability_and_snapshot_and_solver_mode(
            ctx,
            snapshot,
            observability,
            QueryTraceSolverMode::Hybrid,
        )
    }

    pub(crate) fn with_observability_and_snapshot_and_solver_mode(
        ctx: &'a QueryExecContext,
        snapshot: Option<&WorldSnapshotHandle>,
        observability: Rc<RefCell<QueryExecutionObservability>>,
        solver_mode: QueryTraceSolverMode,
    ) -> Self {
        Self {
            ctx,
            snapshot: snapshot.cloned(),
            trace_solver_mode: solver_mode,
            observability,
            world_acceleration_cache: Rc::new(RefCell::new(HashMap::new())),
            shape_union_cache: Rc::new(RefCell::new(HashMap::new())),
        }
    }

    pub(crate) fn snapshot_observability(&self) -> QueryExecutionObservability {
        self.observability.borrow().clone()
    }

    pub(crate) fn context(&self) -> &'a QueryExecContext {
        self.ctx
    }

    fn authoritative_snapshot(&self, kind: SnapshotCaptureKind) -> Option<&WorldSnapshotHandle> {
        self.snapshot
            .as_ref()
            .filter(|snapshot| snapshot.kind() == kind)
    }

    fn ensure_snapshot_epoch(
        &self,
        kind: &'static str,
        name: &SmolStr,
        handle: &WorldSnapshotHandle,
        found_epoch: u32,
    ) -> Result<(), QueryExecError> {
        let expected = handle.portable_epoch();
        if expected == found_epoch {
            return Ok(());
        }
        Err(QueryExecError::SnapshotEpochMismatch {
            kind,
            name: name.clone(),
            expected,
            found: found_epoch,
        })
    }

    fn update_observability<F>(&self, update: F)
    where
        F: FnOnce(&mut QueryExecutionObservability),
    {
        let mut observability = self.observability.borrow_mut();
        update(&mut observability);
    }

    pub(crate) fn note_dispatch(&self) {
        self.update_observability(|observability| observability.dispatch_count += 1);
    }

    pub(crate) fn note_candidate_count(&self, count: u32) {
        self.update_observability(|observability| {
            observability.candidate_count += count;
            observability.candidates_after_pruning += count;
            observability.candidates_before_pruning += count;
        });
    }

    pub(crate) fn note_support_pruned_candidates(&self, count: u32) {
        self.update_observability(|observability| {
            observability.support_pruned_candidates += count;
            observability.candidates_before_pruning += count;
        });
    }

    pub(crate) fn note_batch_dispatch_shape(&self, items: u32, world_batch: bool) {
        self.note_batch_dispatch_grid(items, items.max(1), 1, 1, world_batch);
    }

    pub(crate) fn note_batch_dispatch_grid(
        &self,
        items: u32,
        workgroups_x: u32,
        workgroups_y: u32,
        workgroups_z: u32,
        world_batch: bool,
    ) {
        self.update_observability(|observability| {
            observability.dispatch_items += items;
            observability.dispatch_workgroups_x =
                observability.dispatch_workgroups_x.max(workgroups_x);
            observability.dispatch_workgroups_y =
                observability.dispatch_workgroups_y.max(workgroups_y);
            observability.dispatch_workgroups_z =
                observability.dispatch_workgroups_z.max(workgroups_z);
            if world_batch {
                observability.world_batch_item_count += items;
                observability.screen_sample_count += items;
            }
        });
    }

    pub(crate) fn note_batch_execution_mode(&self, semantic_pruned: bool) {
        self.update_observability(|observability| {
            if semantic_pruned {
                observability.semantic_pruned_batches += 1;
            } else {
                observability.dense_compatibility_batches += 1;
            }
        });
    }

    pub(crate) fn note_solver_plan(&self, plan: &RaySolverPlan) {
        self.note_solver_plan_with_methods(plan, &plan.diagnostic_summary().methods);
    }

    pub(crate) fn note_solver_plan_with_methods(
        &self,
        plan: &RaySolverPlan,
        methods: &[RaySolverMethod],
    ) {
        self.update_observability(|observability| {
            observability.solver_plan_id = Some(plan.id.clone());
            observability.solver_subject = Some(plan.subject.clone());
            for method in methods {
                if !observability.solver_methods.contains(method) {
                    observability.solver_methods.push(*method);
                }
            }
            observability.observer_continuation_seed_hits += plan
                .continuation_intents
                .iter()
                .filter(|intent| {
                    matches!(
                        intent.disposition,
                        crate::query_solver::RaySolverIntentDisposition::Used
                    )
                })
                .count() as u32;
        });
    }

    pub(crate) fn note_step_certificate(&self, certificate: &RayStepCertificate) {
        self.update_observability(|observability| {
            *observability
                .step_certificate_kinds
                .entry(certificate.kind)
                .or_insert(0) += 1;
            if !observability
                .step_certificate_metadata
                .iter()
                .any(|metadata| {
                    same_observability_certificate_metadata(metadata, &certificate.metadata)
                })
            {
                observability
                    .step_certificate_metadata
                    .push(certificate.metadata.clone());
            }
        });
    }

    pub(crate) fn note_acceleration_node_visit(&self) {
        self.update_observability(|observability| observability.acceleration_node_visits += 1);
    }

    pub(crate) fn note_shape_leaf_visit(&self) {
        self.update_observability(|observability| observability.shape_leaf_visits += 1);
    }

    pub(crate) fn note_acceleration_pruned_node(&self) {
        self.update_observability(|observability| observability.acceleration_pruned_nodes += 1);
    }

    pub(crate) fn note_union_cluster_visit(&self) {
        self.update_observability(|observability| observability.union_cluster_visits += 1);
    }

    pub(crate) fn note_ray_support_interval_rejection(&self) {
        self.update_observability(|observability| {
            observability.ray_support_interval_rejections += 1
        });
    }

    pub(crate) fn note_ray_support_entry_jump(&self) {
        self.update_observability(|observability| observability.ray_support_entry_jumps += 1);
    }

    pub(crate) fn note_repeat_cell_skip(&self) {
        self.update_observability(|observability| observability.repeat_cell_skips += 1);
    }

    pub(crate) fn note_solver_repeat_attempt(&self) {
        self.update_observability(|observability| {
            observability.solver_repeat_attempts += 1;
            if !observability
                .solver_methods
                .contains(&RaySolverMethod::RepeatAwareTraversal)
            {
                observability
                    .solver_methods
                    .push(RaySolverMethod::RepeatAwareTraversal);
            }
        });
    }

    pub(crate) fn note_solver_repeat_supported(&self) {
        self.update_observability(|observability| observability.solver_repeat_supported += 1);
    }

    pub(crate) fn note_solver_repeat_inapplicable(&self) {
        self.update_observability(|observability| observability.solver_repeat_inapplicable += 1);
    }

    pub(crate) fn note_solver_repeat_unsupported(&self) {
        self.update_observability(|observability| observability.solver_repeat_unsupported += 1);
    }

    fn note_solver_repeat_unsupported_reason(&self, reason: RepeatAwareUnsupportedReason) {
        self.update_observability(|observability| match reason {
            RepeatAwareUnsupportedReason::Form => observability.solver_repeat_unsupported_form += 1,
            RepeatAwareUnsupportedReason::Bounds => {
                observability.solver_repeat_unsupported_bounds += 1
            }
        });
    }

    pub(crate) fn note_solver_repeat_cells_enumerated(&self, count: u32) {
        self.update_observability(|observability| {
            observability.solver_repeat_cells_enumerated = observability
                .solver_repeat_cells_enumerated
                .saturating_add(count);
        });
    }

    pub(crate) fn note_cache_brick_visit(&self) {
        self.update_observability(|observability| observability.cache_brick_visits += 1);
    }

    pub(crate) fn note_cache_brick_hit(&self) {
        self.update_observability(|observability| observability.cache_brick_hits += 1);
    }

    pub(crate) fn note_cache_brick_miss(&self) {
        self.update_observability(|observability| observability.cache_brick_misses += 1);
    }

    pub(crate) fn note_accepted_relaxed_step(&self) {
        self.update_observability(|observability| observability.accepted_relaxed_steps += 1);
    }

    pub(crate) fn note_rejected_relaxed_step(&self) {
        self.update_observability(|observability| observability.rejected_relaxed_steps += 1);
    }

    pub(crate) fn note_solver_relaxed_attempt(&self) {
        self.update_observability(|observability| {
            observability.solver_relaxed_attempts += 1;
            if !observability
                .solver_methods
                .contains(&RaySolverMethod::LipschitzSafeStepping)
            {
                observability
                    .solver_methods
                    .push(RaySolverMethod::LipschitzSafeStepping);
            }
        });
    }

    pub(crate) fn note_solver_relaxed_no_root_advance(&self) {
        self.update_observability(|observability| {
            observability.solver_relaxed_no_root_advances += 1;
        });
    }

    pub(crate) fn note_solver_relaxed_bracket(&self) {
        self.update_observability(|observability| {
            observability.solver_relaxed_brackets += 1;
        });
    }

    pub(crate) fn note_solver_relaxed_unresolved(&self) {
        self.update_observability(|observability| {
            observability.solver_relaxed_unresolved += 1;
        });
    }

    pub(crate) fn note_analytic_transformed_hit(&self) {
        self.update_observability(|observability| observability.analytic_transformed_hits += 1);
    }

    pub(crate) fn note_interval_subdivision(&self) {
        self.update_observability(|observability| observability.interval_subdivisions += 1);
    }

    pub(crate) fn note_interval_proof_success(&self) {
        self.update_observability(|observability| observability.interval_proof_successes += 1);
    }

    pub(crate) fn note_solver_interval_attempt(&self) {
        self.update_observability(|observability| {
            observability.solver_interval_attempts += 1;
            if !observability
                .solver_methods
                .contains(&RaySolverMethod::IntervalNewtonIsolation)
            {
                observability
                    .solver_methods
                    .push(RaySolverMethod::IntervalNewtonIsolation);
            }
        });
    }

    pub(crate) fn note_solver_interval_no_root_advance(&self) {
        self.update_observability(|observability| {
            observability.solver_interval_no_root_advances += 1;
        });
    }

    pub(crate) fn note_solver_interval_bracket(&self) {
        self.update_observability(|observability| {
            observability.solver_interval_brackets += 1;
        });
    }

    pub(crate) fn note_solver_interval_unresolved(&self) {
        self.update_observability(|observability| {
            observability.solver_interval_unresolved += 1;
        });
    }

    pub(crate) fn note_solver_dense_fallback(&self, reason: RaySolverFallbackReason) {
        self.update_observability(|observability| {
            observability.solver_dense_fallback_rays += 1;
            match reason {
                RaySolverFallbackReason::ContractRequiresDenseOracle => {
                    observability.solver_fallback_contract_dense += 1;
                }
                RaySolverFallbackReason::MissingFieldFacts => {
                    observability.solver_fallback_missing_facts += 1;
                }
                RaySolverFallbackReason::AnalyticUnsupported => {
                    observability.solver_fallback_analytic_unsupported += 1;
                }
                RaySolverFallbackReason::VerificationFailed => {
                    observability.solver_fallback_verification_failed += 1;
                }
                RaySolverFallbackReason::UnsupportedBackend => {
                    observability.solver_fallback_unsupported_backend += 1;
                }
            }
        });
    }

    pub(crate) fn note_solver_dense_fallback_reasons(&self, reasons: &[RaySolverFallbackReason]) {
        if reasons.is_empty() {
            self.note_solver_dense_fallback(RaySolverFallbackReason::ContractRequiresDenseOracle);
            return;
        }
        self.update_observability(|observability| {
            observability.solver_dense_fallback_rays += 1;
            for reason in reasons {
                match reason {
                    RaySolverFallbackReason::ContractRequiresDenseOracle => {
                        observability.solver_fallback_contract_dense += 1;
                    }
                    RaySolverFallbackReason::MissingFieldFacts => {
                        observability.solver_fallback_missing_facts += 1;
                    }
                    RaySolverFallbackReason::AnalyticUnsupported => {
                        observability.solver_fallback_analytic_unsupported += 1;
                    }
                    RaySolverFallbackReason::VerificationFailed => {
                        observability.solver_fallback_verification_failed += 1;
                    }
                    RaySolverFallbackReason::UnsupportedBackend => {
                        observability.solver_fallback_unsupported_backend += 1;
                    }
                }
            }
        });
    }

    pub(crate) fn note_solver_generated_dense_fallback(&self, plan: &RaySolverPlan) {
        self.note_solver_plan(plan);
        self.update_observability(|observability| {
            observability.solver_generated_dense_fallback_rays += 1;
            observability.solver_fallback_unsupported_backend += 1;
        });
    }

    pub(crate) fn note_solver_analytic_hit(&self) {
        self.update_observability(|observability| {
            observability.solver_analytic_hits += 1;
            if !observability
                .solver_methods
                .contains(&RaySolverMethod::AnalyticPrimitiveIntersection)
            {
                observability
                    .solver_methods
                    .push(RaySolverMethod::AnalyticPrimitiveIntersection);
            }
        });
    }

    pub(crate) fn note_solver_support_rejection(&self) {
        self.update_observability(|observability| {
            observability.solver_support_rejections += 1;
            if !observability
                .solver_methods
                .contains(&RaySolverMethod::SupportBoundCandidateRejection)
            {
                observability
                    .solver_methods
                    .push(RaySolverMethod::SupportBoundCandidateRejection);
            }
        });
    }

    pub(crate) fn note_solver_lipschitz_step(&self) {
        self.update_observability(|observability| {
            observability.solver_lipschitz_steps += 1;
            if !observability
                .solver_methods
                .contains(&RaySolverMethod::LipschitzSafeStepping)
            {
                observability
                    .solver_methods
                    .push(RaySolverMethod::LipschitzSafeStepping);
            }
        });
    }

    pub(crate) fn note_solver_interval_skip(&self) {
        self.update_observability(|observability| {
            observability.solver_interval_skips += 1;
            if !observability
                .solver_methods
                .contains(&RaySolverMethod::IntervalNewtonIsolation)
            {
                observability
                    .solver_methods
                    .push(RaySolverMethod::IntervalNewtonIsolation);
            }
        });
    }

    pub(crate) fn note_solver_newton_refinement(&self) {
        self.update_observability(|observability| {
            observability.solver_newton_refinements += 1;
            if !observability
                .solver_methods
                .contains(&RaySolverMethod::SafeguardedNewtonRefinement)
            {
                observability
                    .solver_methods
                    .push(RaySolverMethod::SafeguardedNewtonRefinement);
            }
        });
    }

    pub(crate) fn note_solver_refinement_attempt(&self) {
        self.update_observability(|observability| {
            observability.solver_refinement_attempts += 1;
            if !observability
                .solver_methods
                .contains(&RaySolverMethod::SafeguardedNewtonRefinement)
            {
                observability
                    .solver_methods
                    .push(RaySolverMethod::SafeguardedNewtonRefinement);
            }
        });
    }

    pub(crate) fn note_solver_refinement_failure(&self) {
        self.update_observability(|observability| {
            observability.solver_refinement_failures += 1;
        });
    }

    pub(crate) fn note_solver_repeat_aware_traversal(&self) {
        self.update_observability(|observability| {
            if !observability
                .solver_methods
                .contains(&RaySolverMethod::RepeatAwareTraversal)
            {
                observability
                    .solver_methods
                    .push(RaySolverMethod::RepeatAwareTraversal);
            }
        });
    }

    pub(crate) fn note_solver_adaptive_epsilon(&self) {
        self.update_observability(|observability| {
            observability.solver_adaptive_epsilon_uses += 1;
        });
    }

    pub(crate) fn note_solver_certificate_failure(&self) {
        self.update_observability(|observability| {
            observability.solver_certificate_failures += 1;
        });
    }

    pub(crate) fn note_hit_result(&self, hit: bool, steps: u32) {
        self.update_observability(|observability| {
            if hit {
                observability.hit_count += 1;
            } else {
                observability.miss_count += 1;
            }
            observability.trace_steps_max = observability.trace_steps_max.max(steps);
        });
    }

    pub(crate) fn note_branch_visit(&self) {
        self.update_observability(|observability| observability.branch_visits += 1);
    }

    pub(crate) fn note_artifact_load(&self) {
        self.update_observability(|observability| observability.artifact_loads += 1);
    }

    pub(crate) fn note_opaque_fallback(&self) {
        self.update_observability(|observability| observability.opaque_fallbacks += 1);
    }

    pub(crate) fn note_trace_step(&self) {
        self.note_trace_steps(1);
    }

    pub(crate) fn note_trace_steps(&self, count: u32) {
        self.update_observability(|observability| {
            observability.trace_steps = observability.trace_steps.saturating_add(count);
            observability.trace_steps_max = observability.trace_steps_max.max(count);
        });
    }

    pub(crate) fn note_field_sample(&self) {
        self.update_observability(|observability| observability.field_samples += 1);
    }

    pub(crate) fn note_normal_role(&self, role: NormalRole) {
        self.update_observability(|observability| {
            observability.normal_role = Some(SmolStr::new(role.observability_tag()));
        });
    }

    pub(crate) fn note_contract_validation_failure(&self) {
        self.update_observability(|observability| observability.contract_validation_failures += 1);
    }

    pub(crate) fn execute_world_query(
        &self,
        plan: &KernelWorldQueryPlan,
        args: &[KernelValue],
    ) -> Result<KernelValue, QueryExecError> {
        let kind = world_kind_for_plan(plan)?;
        let semantics = world_query_semantics_for_contract(plan.contract_id);
        let capture = self.resolve_region_capture(args.first())?;
        let domain = expect_struct(args.get(1), "SceneDomain")?;
        let detail = self.validate_world_domain(&capture, domain, semantics.query_name)?;
        match kind {
            WorldQueryKind::Distance => {
                let point = expect_vec3(args.get(2), "point")?;
                Ok(KernelValue::F32(
                    self.eval_world_distance(&capture, detail, point)?,
                ))
            }
            WorldQueryKind::Normal => {
                let point = expect_vec3(args.get(2), "point")?;
                let mut backend = CpuWorldNormalBackend {
                    evaluator: self,
                    capture: &capture,
                    detail,
                    point,
                };
                Ok(KernelValue::Vec3(execute_world_normal(&mut backend)?))
            }
            WorldQueryKind::SupportSummary => self.support_summary_for_region(&capture, detail),
            WorldQueryKind::Nearest | WorldQueryKind::Trace => {
                let ray = expect_struct(args.get(2), "RayQuery")?;
                self.execute_world_ray_hit(plan, &capture, detail, ray, WorldQueryKind::Nearest)
            }
            WorldQueryKind::Occluded => {
                let ray = expect_struct(args.get(2), "RayQuery")?;
                let hit = self.execute_world_ray_hit(
                    plan,
                    &capture,
                    detail,
                    ray,
                    WorldQueryKind::Occluded,
                )?;
                let hit = expect_struct_ref(&hit, "Hit3")?;
                Ok(occlusion_result(
                    expect_struct_bool(hit, "hit")?,
                    expect_struct_f32(hit, "distance")?,
                    expect_struct_i32(hit, "steps")?,
                ))
            }
            WorldQueryKind::Surface => {
                let hit = expect_struct(args.get(2), "Hit3")?;
                let mut backend = CpuWorldSurfaceBackend {
                    evaluator: self,
                    capture: &capture,
                    detail,
                    domain,
                    hit: hit.clone(),
                    root_shape_id: expect_struct_u32(hit, "root_shape_id")?,
                    result: default_surface(),
                };
                execute_world_surface(&mut backend)?;
                Ok(backend.result)
            }
            WorldQueryKind::Radiance => {
                let sample = expect_struct(args.get(2), "PointDirectionQuery")?;
                let point = expect_struct_vec3(sample, "point")?;
                let direction = expect_struct_vec3(sample, "direction")?;
                let mut backend = CpuWorldRadianceBackend {
                    evaluator: self,
                    capture: &capture,
                    detail,
                    domain,
                    point,
                    direction,
                    result: [0.0, 0.0, 0.0],
                };
                execute_world_radiance(&mut backend)?;
                Ok(KernelValue::Vec3(backend.result))
            }
            WorldQueryKind::Medium => {
                let point = expect_vec3(args.get(2), "point")?;
                let mut backend = CpuWorldMediumBackend {
                    evaluator: self,
                    capture: &capture,
                    detail,
                    domain,
                    point,
                    density: 0.0,
                    emission: [0.0, 0.0, 0.0],
                    anisotropy: 0.0,
                };
                execute_world_medium(&mut backend)?;
                Ok(medium_value(
                    backend.density,
                    backend.emission,
                    backend.anisotropy,
                ))
            }
        }
    }

    pub(crate) fn validate_world_domain(
        &self,
        capture: &SmolStr,
        domain: &KernelStructValue,
        query_name: &str,
    ) -> Result<i32, QueryExecError> {
        let capture_scene_id = self.ctx.region_scene_id(capture);
        let domain_scene_id = expect_struct_u32(domain, "scene_id")?;
        if capture_scene_id != domain_scene_id {
            return Err(QueryExecError::Unsupported {
                message: world_domain_mismatch_message(query_name),
            });
        }
        let spatial = expect_struct_ref(struct_field(domain, "spatial")?, "SpatialDomainContract")?;
        expect_struct_i32(spatial, "geometry_detail")
    }

    pub(crate) fn world_domain_flag_enabled(
        &self,
        domain: &KernelStructValue,
        kind: WorldQueryKind,
    ) -> Result<bool, QueryExecError> {
        let Some(flag) = world_query_semantics(kind).domain_flag else {
            return Ok(true);
        };
        let (contract_field, contract_name) = match flag {
            "material" => ("surface", "SurfaceDomainContract"),
            "radiance" | "media" => ("participants", "ParticipantDomainContract"),
            _ => {
                return Err(QueryExecError::Unsupported {
                    message: format!("unknown SceneDomain flag '{flag}'"),
                });
            }
        };
        let contract = expect_struct_ref(struct_field(domain, contract_field)?, contract_name)?;
        expect_struct_bool(contract, flag)
    }

    pub(crate) fn execute_batch_query(
        &self,
        plan: &KernelBatchQueryPlan,
        args: &[KernelValue],
    ) -> Result<KernelValue, QueryExecError> {
        let kind = batch_kind_for_plan(plan)?;
        if plan.capture_kind == crate::query_plan::CaptureKind::Region {
            return self.execute_world_batch_query(plan, args);
        }
        let capture = match kind {
            BatchQueryKind::Distance | BatchQueryKind::Normal => {
                self.resolve_field_or_shape_capture(args.first())
            }
            BatchQueryKind::Nearest
            | BatchQueryKind::Trace
            | BatchQueryKind::Surface
            | BatchQueryKind::Occluded
            | BatchQueryKind::Radiance
            | BatchQueryKind::Medium => self.resolve_shape_capture(args.first()),
        }?;
        let items = expect_array(
            args.get(1),
            if matches!(kind, BatchQueryKind::Distance | BatchQueryKind::Normal) {
                "points"
            } else if matches!(kind, BatchQueryKind::Surface) {
                "hits"
            } else {
                "rays"
            },
        )?;
        self.note_candidate_count(items.len() as u32);
        self.note_batch_dispatch_shape(items.len() as u32, false);
        self.note_batch_execution_mode(!matches!(
            plan.pruning_strategy,
            crate::query_plan::PruningStrategy::None
                | crate::query_plan::PruningStrategy::ConservativeTraversal
        ));
        let mut out = Vec::with_capacity(items.len());
        let capture_value = args.first().cloned().unwrap_or_else(|| {
            self.ctx
                .snapshot_handle_for_kind(snapshot_capture_kind(plan.capture_kind), &capture)
                .expect("resolved capture must have a snapshot handle")
                .capture_value()
        });
        for item in items {
            out.push(execute_batch_item_contract(
                self,
                &plan.item_contract,
                Some(&capture_value),
                item,
            )?);
        }
        Ok(KernelValue::Array(out))
    }

    fn execute_world_batch_query(
        &self,
        plan: &KernelBatchQueryPlan,
        args: &[KernelValue],
    ) -> Result<KernelValue, QueryExecError> {
        let capture = self.resolve_region_capture(args.first())?;
        let domain = expect_struct(args.get(1), "SceneDomain")?.clone();
        let items = expect_array(args.get(2), "world batch items")?;
        self.note_batch_dispatch_shape(items.len() as u32, true);
        self.note_batch_execution_mode(!matches!(
            plan.pruning_strategy,
            crate::query_plan::PruningStrategy::None
                | crate::query_plan::PruningStrategy::ConservativeTraversal
        ));

        let KernelBatchItemContract::WorldQuery { plan: world_plan } = &plan.item_contract else {
            return Err(QueryExecError::Unsupported {
                message: "world-batch plans require a world-query item contract".to_string(),
            });
        };
        let capture_value = args.first().cloned().unwrap_or_else(|| {
            self.ctx
                .region_snapshot_handle(&capture)
                .expect("resolved region capture must have a snapshot handle")
                .capture_value()
        });
        let domain_value = KernelValue::Struct(domain);
        let mut out = Vec::with_capacity(items.len());
        for item in items {
            let world_args =
                build_world_batch_args(world_plan, &capture_value, &domain_value, item)?;
            let value = self.execute_world_query(world_plan, &world_args)?;
            out.push(wrap_world_batch_result(world_plan, value)?);
        }
        Ok(KernelValue::Array(out))
    }

    pub(crate) fn resolve_field_or_shape_capture(
        &self,
        capture: Option<&KernelValue>,
    ) -> Result<SmolStr, QueryExecError> {
        self.note_artifact_load();
        match capture {
            // Legacy compatibility: core execution now prefers typed capture structs, but
            // name-only captures remain accepted for older callers and tests.
            Some(KernelValue::Capture(name)) => {
                if self.ctx.field_names.contains(name) || self.ctx.shape_names.contains(name) {
                    Ok(name.clone())
                } else {
                    Err(QueryExecError::MissingCaptureTarget {
                        kind: "field-or-shape capture",
                    })
                }
            }
            Some(KernelValue::Struct(value)) if value.name.as_str() == "FieldCapture" => {
                let scene_id = expect_struct_u32(value, "scene_id")?;
                let epoch = expect_struct_u32(value, "epoch")?;
                let name = self
                    .ctx
                    .field_name_for_scene_id(scene_id)
                    .cloned()
                    .ok_or_else(|| QueryExecError::UnknownFieldCapture {
                        name: SmolStr::new(format!("{scene_id}")),
                    })?;
                let handle = self
                    .authoritative_snapshot(SnapshotCaptureKind::Field)
                    .filter(|handle| {
                        handle.capture_name() == &name && handle.portable_scene_id() == scene_id
                    })
                    .or_else(|| self.ctx.field_snapshot_handle(&name))
                    .expect("field scene index must point at a snapshot handle");
                self.ensure_snapshot_epoch("field", &name, handle, epoch)?;
                Ok(name)
            }
            Some(KernelValue::Struct(value)) if value.name.as_str() == "ShapeCapture" => {
                let scene_id = expect_struct_u32(value, "scene_id")?;
                let epoch = expect_struct_u32(value, "epoch")?;
                let root_feature_id = expect_struct_u32(value, "root_feature_id")?;
                let name = self
                    .ctx
                    .shape_name_for_scene_id(scene_id)
                    .cloned()
                    .ok_or_else(|| QueryExecError::UnknownShapeCapture {
                        name: SmolStr::new(format!("{scene_id}")),
                    })?;
                let handle = self
                    .authoritative_snapshot(SnapshotCaptureKind::Shape)
                    .filter(|handle| {
                        handle.capture_name() == &name
                            && handle.portable_scene_id() == scene_id
                            && handle.portable_root_feature_id() == root_feature_id
                    })
                    .or_else(|| self.ctx.shape_snapshot_handle(&name))
                    .expect("shape scene index must point at a snapshot handle");
                self.ensure_snapshot_epoch("shape", &name, handle, epoch)?;
                if handle.portable_root_feature_id() != root_feature_id {
                    return Err(QueryExecError::UnknownShapeCapture {
                        name: SmolStr::new(format!("{scene_id}:{root_feature_id}")),
                    });
                }
                Ok(name)
            }
            _ => Err(QueryExecError::MissingCaptureTarget {
                kind: "field-or-shape capture",
            }),
        }
    }

    fn execute_world_ray_hit(
        &self,
        plan: &KernelWorldQueryPlan,
        capture: &SmolStr,
        detail: i32,
        ray: &KernelStructValue,
        kind: WorldQueryKind,
    ) -> Result<KernelValue, QueryExecError> {
        let solver_plan = plan
            .ray_solver
            .as_ref()
            .ok_or_else(|| QueryExecError::Unsupported {
                message: format!(
                    "world ray contract '{}' is missing a RaySolverPlan",
                    plan.contract_id.as_str()
                ),
            })?;
        let origin = expect_struct_vec3(ray, "origin")?;
        let direction = expect_struct_vec3(ray, "direction")?;
        let max_distance = expect_struct_f32(ray, "max_distance")?;
        let min_step = expect_struct_f32(ray, "min_step")?;
        let hit_epsilon = expect_struct_f32(ray, "hit_epsilon")?;
        let max_steps = expect_struct_i32(ray, "max_steps")?;
        let mut backend = CpuWorldTraceBackend {
            evaluator: self,
            capture,
            detail,
            origin,
            direction,
            max_distance,
            min_step,
            hit_epsilon,
            max_steps,
            solver_plan,
            artifact_contracts: &plan.artifact_contracts,
            result: default_hit(origin),
            best_distance: f32::INFINITY,
        };
        execute_world_ray(
            &mut backend,
            kind,
            match kind {
                WorldQueryKind::Occluded => {
                    "occluded_world requires a capture created from a region declaration"
                }
                WorldQueryKind::Nearest => {
                    "nearest_world requires a capture created from a region declaration"
                }
                _ => "trace_world requires a capture created from a region declaration",
            },
        )?;
        if let Ok(hit) = expect_struct_ref(&backend.result, "Hit3") {
            self.note_hit_result(
                expect_struct_bool(hit, "hit").unwrap_or(false),
                expect_struct_i32(hit, "steps").unwrap_or_default().max(0) as u32,
            );
        }
        Ok(backend.result)
    }

    pub(crate) fn resolve_shape_capture(
        &self,
        capture: Option<&KernelValue>,
    ) -> Result<SmolStr, QueryExecError> {
        self.note_artifact_load();
        match capture {
            // Legacy compatibility: core execution now prefers typed capture structs, but
            // name-only captures remain accepted for older callers and tests.
            Some(KernelValue::Capture(name)) if self.ctx.shape_names.contains(name) => {
                Ok(name.clone())
            }
            Some(KernelValue::Struct(value)) if value.name.as_str() == "ShapeCapture" => {
                let scene_id = expect_struct_u32(value, "scene_id")?;
                let epoch = expect_struct_u32(value, "epoch")?;
                let root_feature_id = expect_struct_u32(value, "root_feature_id")?;
                let name = self
                    .ctx
                    .shape_name_for_root_feature_id(root_feature_id)
                    .cloned()
                    .ok_or_else(|| QueryExecError::UnknownShapeCapture {
                        name: SmolStr::new(format!("{root_feature_id}")),
                    })?;
                let handle = self
                    .authoritative_snapshot(SnapshotCaptureKind::Shape)
                    .filter(|handle| {
                        handle.capture_name() == &name
                            && handle.portable_scene_id() == scene_id
                            && handle.portable_root_feature_id() == root_feature_id
                    })
                    .or_else(|| self.ctx.shape_snapshot_handle(&name))
                    .expect("shape root-feature index must point at a snapshot handle");
                self.ensure_snapshot_epoch("shape", &name, handle, epoch)?;
                if handle.portable_scene_id() != scene_id {
                    return Err(QueryExecError::UnknownShapeCapture {
                        name: SmolStr::new(format!("{scene_id}:{root_feature_id}")),
                    });
                }
                Ok(name)
            }
            Some(KernelValue::Capture(name)) => {
                Err(QueryExecError::UnknownShapeCapture { name: name.clone() })
            }
            _ => Err(QueryExecError::MissingCaptureTarget {
                kind: "shape capture",
            }),
        }
    }

    pub(crate) fn resolve_region_capture(
        &self,
        capture: Option<&KernelValue>,
    ) -> Result<SmolStr, QueryExecError> {
        self.note_artifact_load();
        match capture {
            // Legacy compatibility: core execution now prefers typed capture structs, but
            // name-only captures remain accepted for older callers and tests.
            Some(KernelValue::Capture(name)) if self.ctx.regions_by_name.contains_key(name) => {
                Ok(name.clone())
            }
            Some(KernelValue::Struct(value)) if value.name.as_str() == "RegionCapture" => {
                let scene_id = expect_struct_u32(value, "scene_id")?;
                let epoch = expect_struct_u32(value, "epoch")?;
                let name = self
                    .ctx
                    .region_name_for_scene_id(scene_id)
                    .cloned()
                    .ok_or_else(|| QueryExecError::UnknownRegionCapture {
                        name: SmolStr::new(format!("{scene_id}")),
                    })?;
                let handle = self
                    .authoritative_snapshot(SnapshotCaptureKind::Region)
                    .filter(|handle| {
                        handle.capture_name() == &name && handle.portable_scene_id() == scene_id
                    })
                    .or_else(|| self.ctx.region_snapshot_handle(&name))
                    .expect("region scene index must point at a snapshot handle");
                self.ensure_snapshot_epoch("region", &name, handle, epoch)?;
                Ok(name)
            }
            Some(KernelValue::Capture(name)) => {
                Err(QueryExecError::UnknownRegionCapture { name: name.clone() })
            }
            _ => Err(QueryExecError::MissingCaptureTarget {
                kind: "region capture",
            }),
        }
    }

    pub(crate) fn eval_capture_distance(
        &self,
        capture: &SmolStr,
        point: [f32; 3],
        capture_kind: crate::query_plan::CaptureKind,
    ) -> Result<f32, QueryExecError> {
        match capture_kind {
            crate::query_plan::CaptureKind::Field => self.eval_field_distance(capture, point),
            crate::query_plan::CaptureKind::Shape => self.eval_shape_distance(capture, point),
            crate::query_plan::CaptureKind::Region => Err(QueryExecError::Unsupported {
                message: "region captures are only valid for world queries".to_string(),
            }),
        }
    }

    pub(crate) fn eval_capture_normal(
        &self,
        capture: &SmolStr,
        point: [f32; 3],
        capture_kind: crate::query_plan::CaptureKind,
    ) -> Result<[f32; 3], QueryExecError> {
        let evaluation = match capture_kind {
            crate::query_plan::CaptureKind::Field => {
                self.eval_field_normal_with_role(capture, point)?
            }
            crate::query_plan::CaptureKind::Shape => {
                self.eval_shape_normal_with_role(capture, point)?
            }
            crate::query_plan::CaptureKind::Region => {
                return Err(QueryExecError::Unsupported {
                    message: "region captures are only valid for world queries".to_string(),
                });
            }
        };
        self.note_normal_role(evaluation.role);
        Ok(evaluation.normal)
    }

    pub(crate) fn support_summary_for_capture(
        &self,
        capture: &SmolStr,
        capture_kind: crate::query_plan::CaptureKind,
    ) -> Result<KernelValue, QueryExecError> {
        self.note_artifact_load();
        let summary = match capture_kind {
            crate::query_plan::CaptureKind::Field => {
                let scene = self.field_scene(capture)?;
                let bounds = self.field_support_bounds(scene, scene.root_support_id)?;
                SupportSummaryParts {
                    support_class: scene.support_class,
                    semantics: scene.semantics,
                    has_bounds: bounds.is_some(),
                    opaque_boundary: scene.opaque_boundary,
                    can_coarse_support_prune: scene.can_coarse_support_pruning,
                    bounds: bounds.unwrap_or_else(empty_support_bounds),
                }
            }
            crate::query_plan::CaptureKind::Shape => {
                let scene = self.shape_scene(capture)?;
                let bounds = self.shape_support_bounds(scene, scene.root_support_id)?;
                SupportSummaryParts {
                    support_class: scene.support_class,
                    semantics: scene.semantics,
                    has_bounds: bounds.is_some(),
                    opaque_boundary: scene.opaque_boundary,
                    can_coarse_support_prune: scene.can_coarse_support_pruning,
                    bounds: bounds.unwrap_or_else(empty_support_bounds),
                }
            }
            crate::query_plan::CaptureKind::Region => {
                return Err(QueryExecError::Unsupported {
                    message: "region captures require support_summary_world".to_string(),
                });
            }
        };
        Ok(support_summary_value(summary))
    }

    pub(crate) fn support_summary_for_region(
        &self,
        capture: &SmolStr,
        detail: i32,
    ) -> Result<KernelValue, QueryExecError> {
        self.note_artifact_load();
        let shapes = self.resolve_world_shapes(capture, detail, None)?;
        let mut shape_summaries = Vec::with_capacity(shapes.len());
        for shape in shapes {
            let scene = self.shape_scene(&shape)?;
            let bounds = self.shape_support_bounds(scene, scene.root_support_id)?;
            shape_summaries.push(SupportSummaryParts {
                support_class: scene.support_class,
                semantics: scene.semantics,
                has_bounds: bounds.is_some(),
                opaque_boundary: scene.opaque_boundary,
                can_coarse_support_prune: scene.can_coarse_support_pruning,
                bounds: bounds.unwrap_or_else(empty_support_bounds),
            });
        }
        Ok(support_summary_value(merge_world_support_summaries(
            &shape_summaries,
        )))
    }

    pub(crate) fn eval_field_distance(
        &self,
        field: &SmolStr,
        point: [f32; 3],
    ) -> Result<f32, QueryExecError> {
        self.note_field_sample();
        let scene = self.field_scene(field)?;
        if scene.root.contains_opaque_leaf() {
            return self.eval_opaque_field_distance(field, point);
        }
        self.eval_field_node(&scene.root, point)
    }

    pub(crate) fn eval_field_normal(
        &self,
        field: &SmolStr,
        point: [f32; 3],
    ) -> Result<[f32; 3], QueryExecError> {
        Ok(self.eval_field_normal_with_role(field, point)?.normal)
    }

    pub(crate) fn eval_shape_distance(
        &self,
        shape: &SmolStr,
        point: [f32; 3],
    ) -> Result<f32, QueryExecError> {
        self.note_field_sample();
        let scene =
            self.ctx
                .scene
                .shapes
                .get(shape)
                .ok_or_else(|| QueryExecError::MissingShape {
                    name: shape.clone(),
                })?;
        if let ShapeNode::Union { items } = &scene.root
            && let Some(tree) = self.shape_root_union_tree(shape)?
        {
            self.note_union_cluster_visit();
            return self.eval_shape_union_tree(items, &tree, point);
        }
        self.eval_shape_node(&scene.root, point)
    }

    pub(crate) fn eval_shape_normal(
        &self,
        shape: &SmolStr,
        point: [f32; 3],
    ) -> Result<[f32; 3], QueryExecError> {
        Ok(self.eval_shape_normal_with_role(shape, point)?.normal)
    }

    fn eval_field_normal_with_role(
        &self,
        field: &SmolStr,
        point: [f32; 3],
    ) -> Result<NormalEvaluation, QueryExecError> {
        if let Some(normal) = self.try_certified_field_normal(field, point)? {
            return Ok(normal);
        }
        Ok(NormalEvaluation {
            normal: self.finite_difference_normal(point, |sample_point| {
                self.eval_field_distance(field, sample_point)
            })?,
            role: NormalRole::HeuristicShadingNormal,
        })
    }

    fn eval_shape_normal_with_role(
        &self,
        shape: &SmolStr,
        point: [f32; 3],
    ) -> Result<NormalEvaluation, QueryExecError> {
        if let Some(normal) = self.try_certified_shape_normal(shape, point)? {
            return Ok(normal);
        }
        Ok(NormalEvaluation {
            normal: self.finite_difference_normal(point, |sample_point| {
                self.eval_shape_distance(shape, sample_point)
            })?,
            role: NormalRole::HeuristicShadingNormal,
        })
    }

    fn finite_difference_normal<F>(
        &self,
        point: [f32; 3],
        mut sample: F,
    ) -> Result<[f32; 3], QueryExecError>
    where
        F: FnMut([f32; 3]) -> Result<f32, QueryExecError>,
    {
        let eps = 0.001f32;
        let dx = sample([point[0] + eps, point[1], point[2]])?
            - sample([point[0] - eps, point[1], point[2]])?;
        let dy = sample([point[0], point[1] + eps, point[2]])?
            - sample([point[0], point[1] - eps, point[2]])?;
        let dz = sample([point[0], point[1], point[2] + eps])?
            - sample([point[0], point[1], point[2] - eps])?;
        Ok(normalize3([dx, dy, dz]))
    }

    fn try_certified_field_normal(
        &self,
        field: &SmolStr,
        point: [f32; 3],
    ) -> Result<Option<NormalEvaluation>, QueryExecError> {
        let scene = self.field_scene(field)?;
        if scene.opaque_boundary
            || !matches!(
                scene.analysis.differential_support,
                crate::scene_ir::SceneDifferentialSupport::CertifiedGradient
            )
        {
            return Ok(None);
        }
        self.try_certified_field_normal_node(&scene.root, point)
    }

    fn try_certified_field_normal_node(
        &self,
        node: &FieldNode,
        point: [f32; 3],
    ) -> Result<Option<NormalEvaluation>, QueryExecError> {
        match node {
            FieldNode::Use { target } => self.try_certified_field_normal(target, point),
            FieldNode::Primitive { primitive, args } => match primitive {
                hir::FieldPrimitive::Sphere => Ok(Some(NormalEvaluation {
                    normal: normalize3(point),
                    role: NormalRole::CertifiedFieldGradient,
                })),
                hir::FieldPrimitive::Plane => {
                    let Some(normal) =
                        self.eval_scene_named_arg_opt(args.as_deref().unwrap_or(&[]), "normal")?
                    else {
                        return Ok(None);
                    };
                    let normal = expect_vec3(Some(&normal), "plane normal")?;
                    if dot3(normal, normal).sqrt() <= f32::EPSILON {
                        return Ok(None);
                    }
                    Ok(Some(NormalEvaluation {
                        normal: normalize3(normal),
                        role: NormalRole::CertifiedFieldGradient,
                    }))
                }
                hir::FieldPrimitive::Torus => {
                    let major_radius =
                        self.eval_scene_named_arg(args.as_deref().unwrap_or(&[]), "major_radius")?;
                    let minor_radius =
                        self.eval_scene_named_arg(args.as_deref().unwrap_or(&[]), "minor_radius")?;
                    let major_radius = expect_f32(Some(&major_radius), "torus major_radius")?;
                    let minor_radius = expect_f32(Some(&minor_radius), "torus minor_radius")?;
                    if minor_radius <= f32::EPSILON {
                        return Ok(None);
                    }
                    let radial = (point[0] * point[0] + point[2] * point[2]).sqrt();
                    if radial <= f32::EPSILON {
                        return Ok(None);
                    }
                    let tube = radial - major_radius;
                    let normal = [point[0] * tube / radial, point[1], point[2] * tube / radial];
                    if dot3(normal, normal).sqrt() <= f32::EPSILON {
                        return Ok(None);
                    }
                    Ok(Some(NormalEvaluation {
                        normal: normalize3(normal),
                        role: NormalRole::CertifiedFieldGradient,
                    }))
                }
                _ => Ok(None),
            },
            FieldNode::Transform { kind, param, inner } => {
                let Some(param) = param else {
                    return self.try_certified_field_normal_node(inner, point);
                };
                match kind {
                    TransformKind::Translate
                    | TransformKind::Rotate
                    | TransformKind::UniformScale => {
                        let local_point = self.eval_wrapped_point(*kind, param, point)?;
                        let Some(mut inner) =
                            self.try_certified_field_normal_node(inner, local_point)?
                        else {
                            return Ok(None);
                        };
                        let config = self.eval_scene_value_expr(param, &HashMap::new())?;
                        inner.normal = transform_certified_normal(*kind, &config, inner.normal)?;
                        Ok(Some(NormalEvaluation {
                            normal: normalize3(inner.normal),
                            role: inner.role,
                        }))
                    }
                    _ => Ok(None),
                }
            }
            FieldNode::Smooth {
                kind,
                smoothing,
                items,
            } => {
                let Some(first) = items.first() else {
                    return Ok(None);
                };
                let smoothing_value = smoothing
                    .as_ref()
                    .map(|expr| self.eval_scene_value_expr(expr, &HashMap::new()))
                    .transpose()?
                    .unwrap_or(KernelValue::F32(0.0));
                let smoothing = expect_f32(Some(&smoothing_value), "smoothing")?;
                if smoothing <= 0.0 {
                    return Ok(None);
                }
                let Some(first_normal) = self.try_certified_field_normal_node(first, point)? else {
                    return Ok(None);
                };
                let mut current_distance = self.eval_field_node(first, point)?;
                let mut current_normal = first_normal.normal;
                match kind {
                    SmoothKind::Union | SmoothKind::Intersection => {
                        for item in items.iter().skip(1) {
                            let Some(rhs_normal) =
                                self.try_certified_field_normal_node(item, point)?
                            else {
                                return Ok(None);
                            };
                            let rhs_distance = self.eval_field_node(item, point)?;
                            current_normal = smooth_blended_normal(
                                *kind,
                                smoothing,
                                current_distance,
                                current_normal,
                                rhs_distance,
                                rhs_normal.normal,
                            );
                            current_distance = match kind {
                                SmoothKind::Union => runtime_ternary_f32(
                                    smoothing,
                                    current_distance,
                                    rhs_distance,
                                    wr_smooth_union,
                                )?,
                                SmoothKind::Intersection => runtime_ternary_f32(
                                    smoothing,
                                    current_distance,
                                    rhs_distance,
                                    wr_smooth_intersection,
                                )?,
                                SmoothKind::Subtract => unreachable!(),
                            };
                        }
                    }
                    SmoothKind::Subtract => {
                        let Some(rhs) = items.get(1) else {
                            return Ok(None);
                        };
                        let Some(rhs_normal) = self.try_certified_field_normal_node(rhs, point)?
                        else {
                            return Ok(None);
                        };
                        let rhs_distance = self.eval_field_node(rhs, point)?;
                        current_normal = smooth_blended_normal(
                            *kind,
                            smoothing,
                            current_distance,
                            current_normal,
                            rhs_distance,
                            rhs_normal.normal,
                        );
                    }
                }
                Ok(Some(NormalEvaluation {
                    normal: normalize3(current_normal),
                    role: NormalRole::CertifiedFieldGradient,
                }))
            }
            FieldNode::Repeat { .. }
            | FieldNode::Union { .. }
            | FieldNode::Intersection { .. }
            | FieldNode::Subtract { .. }
            | FieldNode::Extrude { .. }
            | FieldNode::Revolve { .. }
            | FieldNode::Sweep { .. }
            | FieldNode::Loft { .. }
            | FieldNode::OpaqueLeaf => Ok(None),
        }
    }

    fn try_certified_shape_normal(
        &self,
        shape: &SmolStr,
        point: [f32; 3],
    ) -> Result<Option<NormalEvaluation>, QueryExecError> {
        let scene = self.shape_scene(shape)?;
        if scene.opaque_boundary
            || !matches!(
                scene.analysis.differential_support,
                crate::scene_ir::SceneDifferentialSupport::CertifiedGradient
            )
        {
            return Ok(None);
        }
        match &scene.root {
            ShapeNode::Use { target } => self.try_certified_shape_normal(target, point),
            ShapeNode::Leaf(leaf) => {
                let Some(mut field_normal) = self.try_certified_field_normal(&leaf.field, point)?
                else {
                    return Ok(None);
                };
                field_normal.role = NormalRole::FeatureNormal;
                Ok(Some(field_normal))
            }
            ShapeNode::Union { .. }
            | ShapeNode::Intersection { .. }
            | ShapeNode::Subtract { .. } => Ok(None),
        }
    }

    fn try_certified_world_normal(
        &self,
        capture: &SmolStr,
        detail: i32,
        point: [f32; 3],
    ) -> Result<Option<NormalEvaluation>, QueryExecError> {
        let shapes = self.resolve_world_shapes(capture, detail, None)?;
        // Keep the certified world path conservative: only single-shape regions
        // with a certifiable shape leaf can skip finite differences.
        let [shape] = shapes.as_slice() else {
            return Ok(None);
        };
        let result = self.try_certified_shape_normal(shape, point)?;
        if result.is_some() {
            self.note_interval_proof_success();
        }
        Ok(result)
    }

    pub(crate) fn eval_field_node(
        &self,
        node: &FieldNode,
        point: [f32; 3],
    ) -> Result<f32, QueryExecError> {
        self.note_branch_visit();
        self.note_acceleration_node_visit();
        match node {
            FieldNode::Use { target } => self.eval_field_distance(target, point),
            FieldNode::Primitive { primitive, args } => {
                self.eval_field_primitive(*primitive, args.as_deref().unwrap_or(&[]), point)
            }
            FieldNode::Union { items } => {
                self.note_union_cluster_visit();
                let mut current = 1_000_000.0f32;
                for item in items {
                    current = runtime_binary_f32(
                        current,
                        self.eval_field_node(item, point)?,
                        wr_field_union,
                    )?;
                }
                Ok(current)
            }
            FieldNode::Intersection { items } => {
                self.note_union_cluster_visit();
                let mut iter = items.iter();
                let Some(first) = iter.next() else {
                    return Ok(1_000_000.0);
                };
                let mut current = self.eval_field_node(first, point)?;
                for item in iter {
                    current = runtime_binary_f32(
                        current,
                        self.eval_field_node(item, point)?,
                        wr_field_intersection,
                    )?;
                }
                Ok(current)
            }
            FieldNode::Subtract { left, right } => Ok(runtime_binary_f32(
                self.eval_field_node(left, point)?,
                self.eval_field_node(right, point)?,
                wr_field_subtract,
            )?),
            FieldNode::Transform { kind, param, inner } => {
                let Some(param) = param else {
                    return self.eval_field_node(inner, point);
                };
                let local_point = self.eval_wrapped_point(*kind, param, point)?;
                let inner_distance = self.eval_field_node(inner, local_point)?;
                if matches!(kind, TransformKind::UniformScale) {
                    let scale = self.eval_scene_value_expr(param, &HashMap::new())?;
                    Ok(inner_distance * expect_abs_scalar(&scale)?)
                } else {
                    Ok(inner_distance)
                }
            }
            FieldNode::Repeat { kind, param, inner } => {
                let Some(param) = param else {
                    return self.eval_field_node(inner, point);
                };
                self.note_repeat_cell_skip();
                let local_point = self.eval_repeat_point(*kind, param, point)?;
                self.eval_field_node(inner, local_point)
            }
            FieldNode::Smooth {
                kind,
                smoothing,
                items,
            } => {
                let Some(first) = items.first() else {
                    return Ok(1_000_000.0);
                };
                let smoothing_value = smoothing
                    .as_ref()
                    .map(|expr| self.eval_scene_value_expr(expr, &HashMap::new()))
                    .transpose()?
                    .unwrap_or(KernelValue::F32(0.0));
                let smoothing = expect_f32(Some(&smoothing_value), "smoothing")?;
                let mut current = self.eval_field_node(first, point)?;
                match kind {
                    SmoothKind::Union => {
                        for item in items.iter().skip(1) {
                            current = runtime_ternary_f32(
                                smoothing,
                                current,
                                self.eval_field_node(item, point)?,
                                wr_smooth_union,
                            )?;
                        }
                    }
                    SmoothKind::Intersection => {
                        for item in items.iter().skip(1) {
                            current = runtime_ternary_f32(
                                smoothing,
                                current,
                                self.eval_field_node(item, point)?,
                                wr_smooth_intersection,
                            )?;
                        }
                    }
                    SmoothKind::Subtract => {
                        if items.len() >= 2 {
                            current = runtime_ternary_f32(
                                smoothing,
                                current,
                                self.eval_field_node(&items[1], point)?,
                                wr_smooth_subtract,
                            )?;
                        }
                    }
                }
                Ok(current)
            }
            FieldNode::OpaqueLeaf => Ok(1_000_000.0),
            FieldNode::Extrude { height, profile } => {
                let (Some(height), Some(profile)) = (height.as_ref(), profile.as_ref()) else {
                    return Ok(1_000_000.0);
                };
                let height_value = self.eval_scene_value_expr(height, &HashMap::new())?;
                let abs_height = expect_abs_scalar(&height_value)?;
                let half_height = abs_height * 0.5;
                let profile_distance = self.eval_profile_expr(profile, [point[0], point[2]])?;
                let axial = point[1].abs() - half_height;
                Ok(self.eval_profile_cap_distance(profile_distance, axial))
            }
            FieldNode::Revolve { profile } => {
                let Some(profile) = profile.as_ref() else {
                    return Ok(1_000_000.0);
                };
                let radial = (point[0] * point[0] + point[2] * point[2]).sqrt();
                self.eval_profile_expr(profile, [radial, point[1]])
            }
            FieldNode::Sweep { path, profile } => {
                let (Some(path), Some(profile)) = (path.as_ref(), profile.as_ref()) else {
                    return Ok(1_000_000.0);
                };
                let path_value = self.eval_scene_value_expr(path, &HashMap::new())?;
                let coords = runtime_binary_value(
                    path_value.clone(),
                    KernelValue::Vec3(point),
                    wr_field_sweep_coords,
                )?;
                let coords = expect_vec3(Some(&coords), "field_sweep_coords")?;
                let profile_distance = self.eval_profile_expr(profile, [coords[0], coords[1]])?;
                let path_length = length_of(&path_value)?;
                let axial = coords[2].abs() - path_length * 0.5;
                Ok(self.eval_profile_cap_distance(profile_distance, axial))
            }
            FieldNode::Loft { height, from, to } => {
                let (Some(height), Some(from), Some(to)) =
                    (height.as_ref(), from.as_ref(), to.as_ref())
                else {
                    return Ok(1_000_000.0);
                };
                let height_value = self.eval_scene_value_expr(height, &HashMap::new())?;
                let abs_height = expect_abs_scalar(&height_value)?;
                let half_height = abs_height * 0.5;
                let safe_height = abs_height.max(0.0001);
                let profile_point = [point[0], point[2]];
                let from_distance = self.eval_profile_expr(from, profile_point)?;
                let to_distance = self.eval_profile_expr(to, profile_point)?;
                let t = ((point[1] + half_height) / safe_height).clamp(0.0, 1.0);
                let mixed = from_distance + (to_distance - from_distance) * t;
                let axial = point[1].abs() - half_height;
                Ok(self.eval_profile_cap_distance(mixed, axial))
            }
        }
    }

    fn eval_shape_node(&self, node: &ShapeNode, point: [f32; 3]) -> Result<f32, QueryExecError> {
        self.note_branch_visit();
        self.note_acceleration_node_visit();
        self.note_cache_brick_visit();
        self.note_cache_brick_hit();
        match node {
            ShapeNode::Use { target } => self.eval_shape_distance(target, point),
            ShapeNode::Leaf(leaf) => {
                self.note_shape_leaf_visit();
                self.eval_field_distance(&leaf.field, point)
            }
            ShapeNode::Union { items } => {
                self.note_union_cluster_visit();
                let mut current = 1_000_000.0f32;
                for item in items {
                    current = runtime_binary_f32(
                        current,
                        self.eval_shape_node(item, point)?,
                        wr_field_union,
                    )?;
                }
                Ok(current)
            }
            ShapeNode::Intersection { items } => {
                self.note_union_cluster_visit();
                let mut iter = items.iter();
                let Some(first) = iter.next() else {
                    return Ok(1_000_000.0);
                };
                let mut current = self.eval_shape_node(first, point)?;
                for item in iter {
                    current = runtime_binary_f32(
                        current,
                        self.eval_shape_node(item, point)?,
                        wr_field_intersection,
                    )?;
                }
                Ok(current)
            }
            ShapeNode::Subtract { left, right } => Ok(runtime_binary_f32(
                self.eval_shape_node(left, point)?,
                self.eval_shape_node(right, point)?,
                wr_field_subtract,
            )?),
        }
    }

    fn eval_shape_union_tree(
        &self,
        items: &[ShapeNode],
        tree: &CpuAccelerationTree<usize>,
        point: [f32; 3],
    ) -> Result<f32, QueryExecError> {
        let mut best = f32::INFINITY;
        let mut stack = vec![CpuPointTraversal {
            node_index: tree.root,
            lower_bound: f32::NEG_INFINITY,
        }];

        while let Some(current) = pop_best_point_traversal(&mut stack) {
            self.note_acceleration_node_visit();
            if current.lower_bound > best {
                self.note_acceleration_pruned_node();
                self.note_support_pruned_candidates(tree.leaf_count(current.node_index));
                continue;
            }
            let Some(node) = tree.node(current.node_index) else {
                continue;
            };
            if let Some(child_index) = node.leaf {
                let distance = self.eval_shape_node(&items[child_index], point)?;
                if distance < best {
                    best = distance;
                }
                continue;
            }

            let mut pending = Vec::new();
            for child_index in tree.children_of(current.node_index) {
                let Some(child) = tree.node(*child_index) else {
                    continue;
                };
                let lower_bound = child
                    .bounds
                    .map(|bounds| support_box_lower_bound(bounds.min, bounds.max, point))
                    .transpose()?
                    .unwrap_or(f32::NEG_INFINITY);
                if lower_bound > best {
                    self.note_acceleration_pruned_node();
                    self.note_support_pruned_candidates(child.leaf_count);
                    continue;
                }
                pending.push(CpuPointTraversal {
                    node_index: *child_index,
                    lower_bound,
                });
            }
            push_ordered_point_traversals(&mut stack, pending);
        }

        if best.is_finite() {
            Ok(best)
        } else {
            Ok(1_000_000.0)
        }
    }

    pub(crate) fn eval_wrapped_point(
        &self,
        kind: TransformKind,
        param: &SceneValueExpr,
        point: [f32; 3],
    ) -> Result<[f32; 3], QueryExecError> {
        let config = self.eval_scene_value_expr(param, &HashMap::new())?;
        let point_value = KernelValue::Vec3(point);
        let value = match kind {
            TransformKind::Translate => runtime_binary_value(config, point_value, wr_translate)?,
            TransformKind::Rotate => runtime_binary_value(config, point_value, wr_rotate)?,
            TransformKind::UniformScale => {
                runtime_binary_value(config, point_value, wr_uniform_scale)?
            }
            TransformKind::AffineTransform => {
                runtime_binary_value(config, point_value, wr_affine_transform)?
            }
            TransformKind::Warp => runtime_binary_value(config, point_value, wr_warp)?,
            TransformKind::Bend => runtime_binary_value(config, point_value, wr_bend)?,
            TransformKind::Twist => runtime_binary_value(config, point_value, wr_twist)?,
            TransformKind::Taper => runtime_binary_value(config, point_value, wr_taper)?,
            TransformKind::Displace => runtime_binary_value(config, point_value, wr_displace)?,
        };
        expect_vec3(Some(&value), "wrapped point")
    }

    fn eval_wrapped_vector(
        &self,
        kind: TransformKind,
        param: &SceneValueExpr,
        vector: [f32; 3],
    ) -> Result<[f32; 3], QueryExecError> {
        let config = self.eval_scene_value_expr(param, &HashMap::new())?;
        let vector_value = KernelValue::Vec3(vector);
        let value = match kind {
            TransformKind::Translate => return Ok(vector),
            TransformKind::Rotate => runtime_binary_value(config, vector_value, wr_rotate)?,
            TransformKind::UniformScale => {
                runtime_binary_value(config, vector_value, wr_uniform_scale)?
            }
            TransformKind::AffineTransform
            | TransformKind::Warp
            | TransformKind::Bend
            | TransformKind::Twist
            | TransformKind::Taper
            | TransformKind::Displace => {
                return Err(QueryExecError::Unsupported {
                    message: format!("ray support vector wrapper is unavailable for {kind:?}"),
                });
            }
        };
        expect_vec3(Some(&value), "wrapped vector")
    }

    pub(crate) fn eval_repeat_point(
        &self,
        kind: RepeatKind,
        param: &SceneValueExpr,
        point: [f32; 3],
    ) -> Result<[f32; 3], QueryExecError> {
        let config = self.eval_scene_value_expr(param, &HashMap::new())?;
        let point_value = KernelValue::Vec3(point);
        let value = match kind {
            RepeatKind::RepeatLinear => {
                runtime_binary_value(config, point_value, wr_repeat_linear)?
            }
            RepeatKind::RepeatGrid => runtime_binary_value(config, point_value, wr_repeat_grid)?,
            RepeatKind::RadialRepeat => {
                runtime_binary_value(config, point_value, wr_radial_repeat)?
            }
            RepeatKind::MirrorArray => runtime_binary_value(config, point_value, wr_mirror_array)?,
            RepeatKind::InstanceArray => {
                runtime_binary_value(config, point_value, wr_instance_array)?
            }
        };
        expect_vec3(Some(&value), "repeat point")
    }

    pub(crate) fn eval_repeat_identity(
        &self,
        kind: RepeatKind,
        param: &SceneValueExpr,
        point: [f32; 3],
    ) -> Result<u32, QueryExecError> {
        let config = self.eval_scene_value_expr(param, &HashMap::new())?;
        let point_value = KernelValue::Vec3(point);
        let value = match kind {
            RepeatKind::RepeatLinear => {
                runtime_binary_value(config, point_value, wr_repeat_linear_identity)?
            }
            RepeatKind::RepeatGrid => {
                runtime_binary_value(config, point_value, wr_repeat_grid_identity)?
            }
            RepeatKind::RadialRepeat => {
                runtime_binary_value(config, point_value, wr_radial_repeat_identity)?
            }
            RepeatKind::MirrorArray => {
                runtime_binary_value(config, point_value, wr_mirror_array_identity)?
            }
            RepeatKind::InstanceArray => {
                runtime_binary_value(config, point_value, wr_instance_array_identity)?
            }
        };
        match value {
            KernelValue::U32(value) => Ok(value),
            KernelValue::I32(value) => Ok(value as u32),
            other => Err(QueryExecError::TypeMismatch {
                expected: "repeat identity: U32".to_string(),
                found: format!("{other:?}"),
            }),
        }
    }

    fn eval_profile_expr(
        &self,
        profile: &SceneProfileExpr,
        point: [f32; 2],
    ) -> Result<f32, QueryExecError> {
        match profile {
            SceneProfileExpr::Primitive { primitive, args } => {
                let point_value = KernelValue::Vec2(point);
                match primitive {
                    hir::ProfilePrimitive::Circle2 => {
                        let radius = self.eval_scene_named_arg(args, "radius")?;
                        runtime_binary_f32_from_values(point_value, radius, wr_circle2)
                    }
                    hir::ProfilePrimitive::Rect2 => {
                        let half = self.eval_scene_named_arg(args, "half")?;
                        runtime_binary_f32_from_values(point_value, half, wr_rect2)
                    }
                    hir::ProfilePrimitive::RoundedRect2 => {
                        let half = self.eval_scene_named_arg(args, "half")?;
                        let radius = self.eval_scene_named_arg(args, "radius")?;
                        runtime_ternary_f32_from_values(point_value, half, radius, wr_rounded_rect2)
                    }
                    hir::ProfilePrimitive::Capsule2 => {
                        let a = self.eval_scene_named_arg(args, "a")?;
                        let b = self.eval_scene_named_arg(args, "b")?;
                        let radius = self.eval_scene_named_arg(args, "radius")?;
                        runtime_quaternary_f32(point_value, a, b, radius, wr_capsule2)
                    }
                    hir::ProfilePrimitive::Segment2 => {
                        let a = self.eval_scene_named_arg(args, "a")?;
                        let b = self.eval_scene_named_arg(args, "b")?;
                        runtime_ternary_f32_from_values(point_value, a, b, wr_segment2)
                    }
                    hir::ProfilePrimitive::Polygon2 => {
                        let vertices = self.eval_scene_named_arg(args, "vertices")?;
                        polygon_profile_distance(point, &vertices, true)
                    }
                    hir::ProfilePrimitive::Polyline2 => {
                        let vertices = self.eval_scene_named_arg(args, "vertices")?;
                        polygon_profile_distance(point, &vertices, false)
                    }
                }
            }
        }
    }

    fn eval_profile_cap_distance(&self, profile_distance: f32, axial_distance: f32) -> f32 {
        let outside_x = profile_distance.max(0.0);
        let outside_y = axial_distance.max(0.0);
        let outside_len = (outside_x * outside_x + outside_y * outside_y).sqrt();
        let inside = profile_distance.max(axial_distance).min(0.0);
        inside + outside_len
    }

    fn eval_opaque_field_distance(
        &self,
        field: &SmolStr,
        point: [f32; 3],
    ) -> Result<f32, QueryExecError> {
        self.note_opaque_fallback();
        let scene = self.field_scene(field)?;
        let bounds_expr =
            scene
                .authored_bounds
                .as_ref()
                .ok_or_else(|| QueryExecError::Unsupported {
                    message: format!("opaque field '{field}' is missing authored bounds"),
                })?;
        let bounds_value = self.eval_scene_value_expr(bounds_expr, &HashMap::new())?;
        let bounds = expect_struct_ref(&bounds_value, "Bounds3")?;
        let min = expect_struct_vec3(bounds, "min")?;
        let max = expect_struct_vec3(bounds, "max")?;
        let center = [
            (min[0] + max[0]) * 0.5,
            (min[1] + max[1]) * 0.5,
            (min[2] + max[2]) * 0.5,
        ];
        let half = [
            (max[0] - min[0]) * 0.5,
            (max[1] - min[1]) * 0.5,
            (max[2] - min[2]) * 0.5,
        ];
        runtime_binary_f32_from_values(
            KernelValue::Vec3([
                point[0] - center[0],
                point[1] - center[1],
                point[2] - center[2],
            ]),
            KernelValue::Vec3(half),
            wr_box,
        )
    }

    fn eval_field_primitive(
        &self,
        primitive: hir::FieldPrimitive,
        args: &[SceneArgExpr],
        point: [f32; 3],
    ) -> Result<f32, QueryExecError> {
        let point = KernelValue::Vec3(point);
        match primitive {
            hir::FieldPrimitive::Sphere => {
                let radius = self.eval_scene_named_arg(args, "radius")?;
                runtime_binary_f32_from_values(point, radius, wr_sphere)
            }
            hir::FieldPrimitive::Box => {
                let half = self
                    .eval_scene_named_arg_opt(args, "half")?
                    .or_else(|| {
                        self.eval_scene_named_arg_opt(args, "half_size")
                            .ok()
                            .flatten()
                    })
                    .ok_or_else(|| QueryExecError::MissingCaptureTarget { kind: "box half" })?;
                runtime_binary_f32_from_values(point, half, wr_box)
            }
            hir::FieldPrimitive::Capsule => {
                let a = self.eval_scene_named_arg(args, "a")?;
                let b = self.eval_scene_named_arg(args, "b")?;
                let radius = self.eval_scene_named_arg(args, "radius")?;
                runtime_quaternary_f32(point, a, b, radius, wr_capsule)
            }
            hir::FieldPrimitive::Cylinder => {
                let radius = self.eval_scene_named_arg(args, "radius")?;
                let half_height = self.eval_scene_named_arg(args, "half_height")?;
                runtime_ternary_f32_from_values(point, radius, half_height, wr_cylinder)
            }
            hir::FieldPrimitive::Plane => {
                let normal = self.eval_scene_named_arg(args, "normal")?;
                let offset = self.eval_scene_named_arg(args, "offset")?;
                runtime_ternary_f32_from_values(point, normal, offset, wr_plane)
            }
            hir::FieldPrimitive::Torus => {
                let major_radius = self.eval_scene_named_arg(args, "major_radius")?;
                let minor_radius = self.eval_scene_named_arg(args, "minor_radius")?;
                runtime_ternary_f32_from_values(point, major_radius, minor_radius, wr_torus)
            }
            hir::FieldPrimitive::RoundedBox => {
                let half = self.eval_scene_named_arg(args, "half")?;
                let radius = self.eval_scene_named_arg(args, "radius")?;
                runtime_ternary_f32_from_values(point, half, radius, wr_rounded_box)
            }
            hir::FieldPrimitive::Ellipsoid => {
                let radii = self.eval_scene_named_arg(args, "radii")?;
                runtime_binary_f32_from_values(point, radii, wr_ellipsoid)
            }
            hir::FieldPrimitive::Cone => {
                let radius = self.eval_scene_named_arg(args, "radius")?;
                let half_height = self.eval_scene_named_arg(args, "half_height")?;
                runtime_ternary_f32_from_values(point, radius, half_height, wr_cone)
            }
            hir::FieldPrimitive::CappedCone => {
                let radius_bottom = self.eval_scene_named_arg(args, "radius_bottom")?;
                let radius_top = self.eval_scene_named_arg(args, "radius_top")?;
                let half_height = self.eval_scene_named_arg(args, "half_height")?;
                runtime_quaternary_f32(
                    point,
                    radius_bottom,
                    radius_top,
                    half_height,
                    wr_capped_cone,
                )
            }
            hir::FieldPrimitive::BoxFrame => {
                let half = self.eval_scene_named_arg(args, "half")?;
                let thickness = self.eval_scene_named_arg(args, "thickness")?;
                runtime_ternary_f32_from_values(point, half, thickness, wr_box_frame)
            }
            hir::FieldPrimitive::Slab => {
                let thickness = self.eval_scene_named_arg(args, "thickness")?;
                runtime_binary_f32_from_values(point, thickness, wr_slab)
            }
            hir::FieldPrimitive::TrianglePrism => {
                let half = self.eval_scene_named_arg(args, "half")?;
                let half_height = self.eval_scene_named_arg(args, "half_height")?;
                runtime_ternary_f32_from_values(point, half, half_height, wr_triangle_prism)
            }
            hir::FieldPrimitive::HexPrism => {
                let half = self.eval_scene_named_arg(args, "half")?;
                let half_height = self.eval_scene_named_arg(args, "half_height")?;
                runtime_ternary_f32_from_values(point, half, half_height, wr_hex_prism)
            }
        }
    }

    pub(crate) fn field_scene(
        &self,
        field: &SmolStr,
    ) -> Result<&crate::scene_ir::FieldScene, QueryExecError> {
        self.ctx
            .scene
            .fields
            .get(field)
            .ok_or_else(|| QueryExecError::MissingField {
                name: field.clone(),
            })
    }

    pub(crate) fn shape_scene(
        &self,
        shape: &SmolStr,
    ) -> Result<&crate::scene_ir::ShapeScene, QueryExecError> {
        self.ctx
            .scene
            .shapes
            .get(shape)
            .ok_or_else(|| QueryExecError::MissingShape {
                name: shape.clone(),
            })
    }

    fn eval_shape_winner(
        &self,
        shape: &SmolStr,
        point: [f32; 3],
    ) -> Result<ShapeWinner, QueryExecError> {
        let scene = self.shape_scene(shape)?;
        self.eval_shape_winner_node(shape, &scene.root, scene.provenance.as_ref(), point)
    }

    fn eval_shape_winner_node(
        &self,
        scene_name: &SmolStr,
        node: &ShapeNode,
        provenance: Option<&ShapeProvenanceExpr>,
        point: [f32; 3],
    ) -> Result<ShapeWinner, QueryExecError> {
        self.note_branch_visit();
        match node {
            ShapeNode::Use { target } => {
                let scene = self.shape_scene(target)?;
                self.eval_shape_winner_node(target, &scene.root, scene.provenance.as_ref(), point)
            }
            ShapeNode::Leaf(leaf) => {
                self.note_shape_leaf_visit();
                Ok(ShapeWinner {
                    distance: self.eval_field_distance(&leaf.field, point)?,
                    feature_id: leaf.feature_id,
                    leaf: Some(ShapeLeafRef {
                        scene: scene_name.clone(),
                        leaf: leaf.id,
                    }),
                })
            }
            ShapeNode::Union { items } => {
                let merge_policy = match provenance {
                    Some(ShapeProvenanceExpr::Union { provenance, .. }) => *provenance,
                    _ => ShapeMergeProvenancePolicy::Nearest,
                };
                let provenance_items = match provenance {
                    Some(ShapeProvenanceExpr::Union { items, .. }) => Some(items.as_slice()),
                    _ => None,
                };
                let mut iter = items.iter().enumerate();
                let Some((idx, first)) = iter.next() else {
                    return Ok(default_shape_winner());
                };
                let mut current = self.eval_shape_winner_node(
                    scene_name,
                    first,
                    provenance_items.and_then(|items| items.get(idx)),
                    point,
                )?;
                for (idx, item) in iter {
                    let next = self.eval_shape_winner_node(
                        scene_name,
                        item,
                        provenance_items.and_then(|items| items.get(idx)),
                        point,
                    )?;
                    match merge_policy {
                        ShapeMergeProvenancePolicy::Ordered => {
                            current.distance = runtime_binary_f32(
                                current.distance,
                                next.distance,
                                wr_field_union,
                            )?;
                        }
                        ShapeMergeProvenancePolicy::Nearest => {
                            if next.distance < current.distance {
                                current = next;
                            }
                        }
                    }
                }
                Ok(current)
            }
            ShapeNode::Intersection { items } => {
                let merge_policy = match provenance {
                    Some(ShapeProvenanceExpr::Intersection { provenance, .. }) => *provenance,
                    _ => ShapeMergeProvenancePolicy::Nearest,
                };
                let provenance_items = match provenance {
                    Some(ShapeProvenanceExpr::Intersection { items, .. }) => Some(items.as_slice()),
                    _ => None,
                };
                let mut iter = items.iter().enumerate();
                let Some((idx, first)) = iter.next() else {
                    return Ok(default_shape_winner());
                };
                let mut current = self.eval_shape_winner_node(
                    scene_name,
                    first,
                    provenance_items.and_then(|items| items.get(idx)),
                    point,
                )?;
                for (idx, item) in iter {
                    let next = self.eval_shape_winner_node(
                        scene_name,
                        item,
                        provenance_items.and_then(|items| items.get(idx)),
                        point,
                    )?;
                    match merge_policy {
                        ShapeMergeProvenancePolicy::Ordered => {
                            current.distance = runtime_binary_f32(
                                current.distance,
                                next.distance,
                                wr_field_intersection,
                            )?;
                        }
                        ShapeMergeProvenancePolicy::Nearest => {
                            if next.distance > current.distance {
                                current = next;
                            }
                        }
                    }
                }
                Ok(current)
            }
            ShapeNode::Subtract { left, right } => {
                let (subtract_policy, left_provenance, right_provenance) = match provenance {
                    Some(ShapeProvenanceExpr::Subtract {
                        provenance,
                        left,
                        right,
                    }) => (*provenance, Some(left.as_ref()), Some(right.as_ref())),
                    _ => (ShapeSubtractProvenancePolicy::Left, None, None),
                };
                let left = self.eval_shape_winner_node(scene_name, left, left_provenance, point)?;
                let right =
                    self.eval_shape_winner_node(scene_name, right, right_provenance, point)?;
                let neg_right = -right.distance;
                if left.distance >= neg_right {
                    Ok(left)
                } else {
                    Ok(ShapeWinner {
                        distance: neg_right,
                        feature_id: match subtract_policy {
                            ShapeSubtractProvenancePolicy::Left => left.feature_id,
                            ShapeSubtractProvenancePolicy::Right => right.feature_id,
                        },
                        leaf: match subtract_policy {
                            ShapeSubtractProvenancePolicy::Left => left.leaf,
                            ShapeSubtractProvenancePolicy::Right => right.leaf,
                        },
                    })
                }
            }
        }
    }

    pub(crate) fn eval_field_local_frame<'scene>(
        &'scene self,
        field: &SmolStr,
        point: [f32; 3],
    ) -> Result<FieldLocalFrame<'scene>, QueryExecError> {
        let scene = self.field_scene(field)?;
        self.eval_field_local_frame_node(field.clone(), &scene.root, point, 0, 0)
    }

    fn eval_field_local_frame_node<'scene>(
        &'scene self,
        field_name: SmolStr,
        node: &'scene FieldNode,
        point: [f32; 3],
        instance_id: u32,
        repeat_id: u32,
    ) -> Result<FieldLocalFrame<'scene>, QueryExecError> {
        self.note_acceleration_node_visit();
        match node {
            FieldNode::Use { target } => {
                let scene = self.field_scene(target)?;
                self.eval_field_local_frame_node(
                    target.clone(),
                    &scene.root,
                    point,
                    instance_id,
                    repeat_id,
                )
            }
            FieldNode::Transform { kind, param, inner } => {
                let Some(param) = param else {
                    return self.eval_field_local_frame_node(
                        field_name,
                        inner,
                        point,
                        instance_id,
                        repeat_id,
                    );
                };
                let local_point = self.eval_wrapped_point(*kind, param, point)?;
                self.eval_field_local_frame_node(
                    field_name,
                    inner,
                    local_point,
                    instance_id,
                    repeat_id,
                )
            }
            FieldNode::Repeat { kind, param, inner } => {
                let Some(param) = param else {
                    return self.eval_field_local_frame_node(
                        field_name,
                        inner,
                        point,
                        instance_id,
                        repeat_id,
                    );
                };
                self.note_repeat_cell_skip();
                let component = self.eval_repeat_identity(*kind, param, point)?;
                let local_point = self.eval_repeat_point(*kind, param, point)?;
                let (next_instance_id, next_repeat_id) = match kind {
                    RepeatKind::InstanceArray => {
                        (chain_identity_component(instance_id, component), repeat_id)
                    }
                    _ => (instance_id, chain_identity_component(repeat_id, component)),
                };
                self.eval_field_local_frame_node(
                    field_name,
                    inner,
                    local_point,
                    next_instance_id,
                    next_repeat_id,
                )
            }
            _ => Ok(FieldLocalFrame {
                field_name,
                node,
                point,
                instance_id,
                repeat_id,
            }),
        }
    }

    pub(crate) fn eval_field_local_normal(
        &self,
        field: &SmolStr,
        point: [f32; 3],
    ) -> Result<[f32; 3], QueryExecError> {
        let frame = self.eval_field_local_frame(field, point)?;
        let eps = 0.001f32;
        let sample = |sample_point: [f32; 3]| match frame.node {
            FieldNode::OpaqueLeaf => {
                self.eval_opaque_field_distance(&frame.field_name, sample_point)
            }
            _ => self.eval_field_node(frame.node, sample_point),
        };
        let dx = sample([frame.point[0] + eps, frame.point[1], frame.point[2]])?
            - sample([frame.point[0] - eps, frame.point[1], frame.point[2]])?;
        let dy = sample([frame.point[0], frame.point[1] + eps, frame.point[2]])?
            - sample([frame.point[0], frame.point[1] - eps, frame.point[2]])?;
        let dz = sample([frame.point[0], frame.point[1], frame.point[2] + eps])?
            - sample([frame.point[0], frame.point[1], frame.point[2] - eps])?;
        Ok(normalize3([dx, dy, dz]))
    }

    fn eval_shape_radiance_node(
        &self,
        node: &ShapeNode,
        point: [f32; 3],
        direction: [f32; 3],
    ) -> Result<[f32; 3], QueryExecError> {
        match node {
            ShapeNode::Use { target } => {
                let scene = self.shape_scene(target)?;
                self.eval_shape_radiance_node(&scene.root, point, direction)
            }
            ShapeNode::Leaf(leaf) => {
                let Some(radiance) = &leaf.radiance else {
                    return Ok([0.0, 0.0, 0.0]);
                };
                let local_frame = self.eval_field_local_frame(&leaf.field, point)?;
                let value = self.execute_portable_function(
                    radiance,
                    vec![
                        KernelValue::Vec3(local_frame.point),
                        KernelValue::Vec3(direction),
                        KernelValue::U32(leaf.feature_id),
                    ],
                )?;
                expect_vec3(Some(&value), "radiance")
            }
            ShapeNode::Union { items } | ShapeNode::Intersection { items } => {
                let mut total = [0.0, 0.0, 0.0];
                for item in items {
                    total = add3(
                        total,
                        self.eval_shape_radiance_node(item, point, direction)?,
                    );
                }
                Ok(total)
            }
            ShapeNode::Subtract { left, right } => Ok(add3(
                self.eval_shape_radiance_node(left, point, direction)?,
                self.eval_shape_radiance_node(right, point, direction)?,
            )),
        }
    }

    fn eval_shape_medium_node(
        &self,
        node: &ShapeNode,
        point: [f32; 3],
    ) -> Result<KernelValue, QueryExecError> {
        match node {
            ShapeNode::Use { target } => {
                let scene = self.shape_scene(target)?;
                self.eval_shape_medium_node(&scene.root, point)
            }
            ShapeNode::Leaf(leaf) => {
                let Some(volume) = &leaf.volume else {
                    return Ok(default_medium());
                };
                let local_frame = self.eval_field_local_frame(&leaf.field, point)?;
                let local_surface_distance =
                    self.eval_field_node(local_frame.node, local_frame.point)?;
                self.execute_portable_function(
                    volume,
                    vec![
                        KernelValue::Vec3(local_frame.point),
                        KernelValue::F32(local_surface_distance),
                    ],
                )
            }
            ShapeNode::Union { items } | ShapeNode::Intersection { items } => {
                let mut total = default_medium();
                for item in items {
                    total =
                        combine_medium_values(total, self.eval_shape_medium_node(item, point)?)?;
                }
                Ok(total)
            }
            ShapeNode::Subtract { left, right } => combine_medium_values(
                self.eval_shape_medium_node(left, point)?,
                self.eval_shape_medium_node(right, point)?,
            ),
        }
    }

    fn trace_shape_impl(
        &self,
        shape: &SmolStr,
        origin: [f32; 3],
        direction: [f32; 3],
        max_distance: f32,
        min_step: f32,
        hit_epsilon: f32,
        max_steps: i32,
    ) -> Result<KernelValue, QueryExecError> {
        self.trace_shape_with_policy(
            shape,
            origin,
            direction,
            0.0,
            max_distance,
            min_step,
            hit_epsilon,
            max_steps,
            &TraceLoopPolicy::dense_only(shape.clone()),
            None,
        )
    }

    fn trace_shape_dense_certificate_only(
        &self,
        shape: &SmolStr,
        origin: [f32; 3],
        direction: [f32; 3],
        start_travel: f32,
        max_distance: f32,
        min_step: f32,
        hit_epsilon: f32,
        max_steps: i32,
        policy: &TraceLoopPolicy,
        runtime_plan: Option<&RaySolverPlan>,
    ) -> Result<KernelValue, QueryExecError> {
        let mut travel = start_travel.max(0.0);
        let mut steps = 0i32;
        let mut dense_fallback_recorded = false;

        if start_travel > 0.0 {
            let certificate = self.support_entry_jump_certificate(policy, shape, 0.0, start_travel);
            self.note_step_certificate(&certificate);
        }

        while steps < max_steps && travel <= max_distance {
            self.note_trace_step();
            let point = [
                origin[0] + direction[0] * travel,
                origin[1] + direction[1] * travel,
                origin[2] + direction[2] * travel,
            ];
            let distance = self.eval_shape_distance(shape, point)?;
            let state = TraceLoopState {
                travel,
                distance,
                adaptive_epsilon: hit_epsilon,
                sample: IntervalSample {
                    t: travel,
                    distance,
                    adaptive_epsilon: hit_epsilon,
                },
                step_bound: distance.max(min_step),
                previous_distance: None,
                consecutive_small_steps: 0,
                non_improving_distance: false,
            };
            if distance <= hit_epsilon {
                if !dense_fallback_recorded && runtime_plan.is_some() {
                    self.note_solver_dense_fallback_reasons(
                        runtime_plan.expect("runtime plan").dense_fallback_reasons(),
                    );
                }
                let certificate = self.dense_distance_hit_certificate(policy, shape, &state);
                self.note_step_certificate(&certificate);
                return self.shape_hit_value(shape, travel, point, steps);
            }

            if !dense_fallback_recorded && runtime_plan.is_some() {
                self.note_solver_dense_fallback_reasons(
                    runtime_plan.expect("runtime plan").dense_fallback_reasons(),
                );
                dense_fallback_recorded = true;
            }
            let certificate = self.dense_distance_advance_certificate(policy, shape, &state);
            self.note_step_certificate(&certificate);
            travel = certificate.t_end;
            steps += 1;
        }

        Ok(default_hit(origin))
    }

    fn trace_shape_with_policy(
        &self,
        shape: &SmolStr,
        origin: [f32; 3],
        direction: [f32; 3],
        start_travel: f32,
        max_distance: f32,
        min_step: f32,
        hit_epsilon: f32,
        max_steps: i32,
        policy: &TraceLoopPolicy,
        runtime_plan: Option<&RaySolverPlan>,
    ) -> Result<KernelValue, QueryExecError> {
        if !self.ctx.shape_names.contains(shape) {
            return Ok(default_hit(origin));
        }
        if policy.is_dense_only() {
            return self.trace_shape_dense_certificate_only(
                shape,
                origin,
                direction,
                start_travel,
                max_distance,
                min_step,
                hit_epsilon,
                max_steps,
                policy,
                runtime_plan,
            );
        }
        let mut travel = start_travel.max(0.0);
        let mut steps = 0i32;
        let mut previous_distance = None;
        let mut consecutive_small_steps = 0u32;
        let mut dense_fallback_recorded = false;
        let mut cached_sample: Option<IntervalSample> = None;

        if start_travel > 0.0 {
            let certificate = self.support_entry_jump_certificate(policy, shape, 0.0, start_travel);
            self.note_step_certificate(&certificate);
        }

        while steps < max_steps && travel <= max_distance {
            self.note_trace_step();
            let sample = match cached_sample.take() {
                Some(sample) if (sample.t - travel).abs() <= f32::EPSILON => sample,
                _ => self.interval_sample(shape, origin, direction, travel, hit_epsilon)?,
            };
            let distance = sample.distance;
            let adaptive_epsilon = sample.adaptive_epsilon;
            let non_improving_distance = previous_distance.is_some_and(|previous| {
                distance >= previous - (adaptive_epsilon * 0.25).max(min_step * 0.05)
            });
            let state = TraceLoopState {
                travel,
                distance,
                adaptive_epsilon,
                sample,
                step_bound: distance.max(min_step),
                previous_distance,
                consecutive_small_steps,
                non_improving_distance,
            };
            let decision = self.next_shape_trace_certificate(
                shape,
                origin,
                direction,
                max_distance,
                min_step,
                hit_epsilon,
                &state,
                policy,
            )?;

            match decision {
                TraceStepDecision::Advance {
                    certificate,
                    next_sample,
                } => {
                    if !dense_fallback_recorded
                        && matches!(certificate.kind, StepCertificateKind::DenseDistanceBound)
                        && runtime_plan.is_some()
                    {
                        self.note_solver_dense_fallback_reasons(
                            runtime_plan.expect("runtime plan").dense_fallback_reasons(),
                        );
                        dense_fallback_recorded = true;
                    }
                    self.note_step_certificate(&certificate);
                    previous_distance = Some(distance);
                    let next_travel = certificate.t_end.max(travel);
                    if next_travel <= travel + f32::EPSILON {
                        self.note_solver_certificate_failure();
                        break;
                    }
                    let advance = next_travel - travel;
                    let stall_threshold = (min_step * 1.5).max(adaptive_epsilon * 8.0);
                    consecutive_small_steps = if advance <= stall_threshold {
                        consecutive_small_steps.saturating_add(1)
                    } else {
                        0
                    };
                    cached_sample =
                        next_sample.filter(|sample| (sample.t - next_travel).abs() <= f32::EPSILON);
                    travel = next_travel;
                    steps += 1;
                }
                TraceStepDecision::Hit(certificate) => {
                    if !dense_fallback_recorded
                        && matches!(certificate.kind, StepCertificateKind::DenseDistanceBound)
                        && runtime_plan.is_some()
                    {
                        self.note_solver_dense_fallback_reasons(
                            runtime_plan.expect("runtime plan").dense_fallback_reasons(),
                        );
                    }
                    self.note_step_certificate(&certificate);
                    if matches!(certificate.kind, StepCertificateKind::AnalyticHit) {
                        self.note_solver_analytic_hit();
                    }
                    let hit_point = [
                        origin[0] + direction[0] * certificate.t_end,
                        origin[1] + direction[1] * certificate.t_end,
                        origin[2] + direction[2] * certificate.t_end,
                    ];
                    return self.shape_hit_value(shape, certificate.t_end, hit_point, steps);
                }
                TraceStepDecision::Miss => {
                    break;
                }
            }
        }
        Ok(default_hit(origin))
    }

    pub(crate) fn solve_shape_ray(
        &self,
        solver_plan: &RaySolverPlan,
        artifact_contracts: &[ArtifactContract],
        shape: &SmolStr,
        origin: [f32; 3],
        direction: [f32; 3],
        start_travel: f32,
        max_distance: f32,
        min_step: f32,
        hit_epsilon: f32,
        max_steps: i32,
    ) -> Result<KernelValue, QueryExecError> {
        let runtime_plan =
            self.runtime_shape_solver_plan(solver_plan, artifact_contracts, shape)?;
        let effective_methods = match self.trace_solver_mode {
            QueryTraceSolverMode::Hybrid => runtime_plan.diagnostic_summary().methods,
            QueryTraceSolverMode::DenseOnly => vec![RaySolverMethod::DenseSphereTracing],
        };
        let policy = match self.trace_solver_mode {
            QueryTraceSolverMode::Hybrid => TraceLoopPolicy::from_solver_plan(&runtime_plan),
            QueryTraceSolverMode::DenseOnly => {
                TraceLoopPolicy::dense_only(runtime_plan.subject.clone())
            }
        };
        self.note_solver_plan_with_methods(&runtime_plan, &effective_methods);
        if policy.method_enabled(RaySolverMethod::RepeatAwareTraversal) {
            self.note_solver_repeat_attempt();
            match self.try_repeat_linear_shape_hit(
                shape,
                origin,
                direction,
                start_travel,
                max_distance,
                min_step,
                hit_epsilon,
                max_steps,
                &policy,
            )? {
                RepeatAwareTraceOutcome::Finished(hit) => {
                    self.note_solver_repeat_supported();
                    self.note_solver_repeat_aware_traversal();
                    return Ok(hit);
                }
                RepeatAwareTraceOutcome::Inapplicable(hit) => {
                    self.note_solver_repeat_inapplicable();
                    self.note_solver_repeat_aware_traversal();
                    return Ok(hit);
                }
                RepeatAwareTraceOutcome::Unsupported(reason) => {
                    self.note_solver_repeat_unsupported();
                    self.note_solver_repeat_unsupported_reason(reason);
                }
            }
        }
        self.trace_shape_with_policy(
            shape,
            origin,
            direction,
            start_travel,
            max_distance,
            min_step,
            hit_epsilon,
            max_steps,
            &policy,
            Some(&runtime_plan),
        )
    }

    fn runtime_shape_solver_plan(
        &self,
        solver_plan: &RaySolverPlan,
        artifact_contracts: &[ArtifactContract],
        shape: &SmolStr,
    ) -> Result<RaySolverPlan, QueryExecError> {
        let scene = self.shape_scene(shape)?;
        let evidence = match &scene.root {
            ShapeNode::Leaf(leaf) => {
                let field_evidence =
                    SemanticEvidence::for_field_scene(self.field_scene(&leaf.field)?)
                        .with_subject(shape.clone());
                let shape_evidence = SemanticEvidence::for_shape_scene(scene);
                field_evidence.refine_identity_with(
                    shape_evidence.identity.clone(),
                    "shape leaf identity overlay",
                )
            }
            _ => SemanticEvidence::for_shape_scene(scene),
        };
        let plan = RaySolverPlan::for_contract_with_subject(
            solver_plan.contract_id,
            shape.clone(),
            Some(evidence),
        )
        .ok_or_else(|| QueryExecError::Unsupported {
            message: format!(
                "contract '{}' is not a ray-shaped spatial solver contract",
                solver_plan.contract_id.as_str()
            ),
        })?;
        Ok(plan.with_artifact_reuse_resolution(
            Self::artifact_reuse_resolution_for_query_artifacts(artifact_contracts),
        ))
    }

    fn artifact_reuse_resolution_for_query_artifacts(
        artifact_contracts: &[ArtifactContract],
    ) -> RaySolverArtifactReuseResolution {
        let compatible_artifacts = artifact_contracts
            .iter()
            .filter_map(|artifact| match artifact.schema {
                ArtifactSchema::SupportSummary { .. } => Some("support-summary"),
                ArtifactSchema::CaptureCache { .. } => Some("capture-cache"),
                ArtifactSchema::CullingTable { .. } => Some("culling-table"),
                _ => None,
            })
            .collect::<Vec<_>>();
        if compatible_artifacts.is_empty() {
            return RaySolverArtifactReuseResolution {
                disposition: RaySolverIntentDisposition::Rejected,
                reasons: vec![SmolStr::new(
                    "query plan exposes no compatible solver artifacts for reuse",
                )],
            };
        }
        RaySolverArtifactReuseResolution {
            disposition: RaySolverIntentDisposition::Used,
            reasons: vec![
                SmolStr::new(format!(
                    "query plan requested compatible artifacts: {}",
                    compatible_artifacts.join(", ")
                )),
                SmolStr::new(
                    "artifact validity remains enforced by the query plan compatibility contract",
                ),
            ],
        }
    }

    fn next_shape_trace_certificate(
        &self,
        shape: &SmolStr,
        origin: [f32; 3],
        direction: [f32; 3],
        max_distance: f32,
        min_step: f32,
        hit_epsilon: f32,
        state: &TraceLoopState,
        policy: &TraceLoopPolicy,
    ) -> Result<TraceStepDecision, QueryExecError> {
        if policy.method_enabled(RaySolverMethod::AnalyticPrimitiveIntersection) {
            match self.try_analytic_primitive_hit(
                shape,
                origin,
                direction,
                state.travel,
                max_distance,
                hit_epsilon,
                policy,
            )? {
                AnalyticRayHit::Hit(certificate) => {
                    self.note_analytic_transformed_hit();
                    self.note_interval_proof_success();
                    return Ok(TraceStepDecision::Hit(certificate));
                }
                AnalyticRayHit::VerificationFailed => {
                    self.note_solver_certificate_failure();
                }
                AnalyticRayHit::NotApplicable => {}
            }
        }

        if state.distance <= state.adaptive_epsilon {
            self.note_interval_proof_success();
            return Ok(TraceStepDecision::Hit(
                self.dense_distance_hit_certificate(policy, shape, state),
            ));
        }

        if let Some(decision) = self.try_relaxed_or_interval_certificate(
            shape,
            origin,
            direction,
            max_distance,
            min_step,
            hit_epsilon,
            state,
            policy,
        )? {
            return Ok(decision);
        }

        Ok(TraceStepDecision::Advance {
            certificate: self.dense_distance_advance_certificate(policy, shape, state),
            next_sample: None,
        })
    }

    fn try_analytic_primitive_hit(
        &self,
        shape: &SmolStr,
        origin: [f32; 3],
        direction: [f32; 3],
        start_travel: f32,
        max_distance: f32,
        hit_epsilon: f32,
        policy: &TraceLoopPolicy,
    ) -> Result<AnalyticRayHit, QueryExecError> {
        let Some(primitive_ray) = self.analytic_primitive_for_shape(shape, origin, direction)?
        else {
            return Ok(AnalyticRayHit::NotApplicable);
        };
        let travel = match primitive_ray.solve(start_travel, max_distance) {
            Some(travel) => travel,
            None => return Ok(AnalyticRayHit::NotApplicable),
        };
        let point = [
            origin[0] + direction[0] * travel,
            origin[1] + direction[1] * travel,
            origin[2] + direction[2] * travel,
        ];
        let adaptive_epsilon =
            self.shape_adaptive_hit_epsilon(shape, travel, point, hit_epsilon)?;
        let residual = self.eval_shape_distance(shape, point)?.abs();
        if residual > adaptive_epsilon {
            return Ok(AnalyticRayHit::VerificationFailed);
        }
        let certificate = RayStepCertificate {
            kind: StepCertificateKind::AnalyticHit,
            metadata: self.certificate_metadata(
                RequiredGuaranteeClass::Exact,
                policy,
                shape,
                RayStepCertificateSubjectKind::Primitive,
                "analytic-primitive-hit",
                format!("hit_epsilon={hit_epsilon:.6}; adaptive_epsilon={adaptive_epsilon:.6}"),
                CertificateReuseClass::RenderingAndCollision,
                vec![
                    SmolStr::new("primitive arguments changed"),
                    SmolStr::new("safe transform chain changed"),
                    SmolStr::new("shape semantic identity changed"),
                ],
            ),
            t_start: start_travel.max(0.0),
            t_end: travel,
            no_hit_before_t_end: true,
            bracket: None,
            provenance: None,
        };
        self.note_trace_steps(1);
        Ok(AnalyticRayHit::Hit(certificate))
    }

    fn support_entry_jump_certificate(
        &self,
        policy: &TraceLoopPolicy,
        shape: &SmolStr,
        t_start: f32,
        t_end: f32,
    ) -> RayStepCertificate {
        RayStepCertificate {
            kind: StepCertificateKind::SupportEntryJump,
            metadata: self.certificate_metadata(
                RequiredGuaranteeClass::ConservativeNoFalseMiss,
                policy,
                shape,
                RayStepCertificateSubjectKind::SupportInterval,
                "support-entry-jump",
                "support-interval entry jump",
                CertificateReuseClass::RenderingAndCollision,
                vec![
                    SmolStr::new("support bounds changed"),
                    SmolStr::new("support semantics invalidated"),
                ],
            ),
            t_start,
            t_end,
            no_hit_before_t_end: true,
            bracket: None,
            provenance: None,
        }
    }

    fn dense_distance_hit_certificate(
        &self,
        policy: &TraceLoopPolicy,
        shape: &SmolStr,
        state: &TraceLoopState,
    ) -> RayStepCertificate {
        RayStepCertificate {
            kind: StepCertificateKind::DenseDistanceBound,
            metadata: self.certificate_metadata(
                RequiredGuaranteeClass::Exact,
                policy,
                shape,
                RayStepCertificateSubjectKind::Shape,
                "dense-distance-bound",
                format!(
                    "dense hit within adaptive epsilon {:.6}",
                    state.adaptive_epsilon
                ),
                CertificateReuseClass::RenderingAndCollision,
                vec![SmolStr::new("distance semantics changed")],
            ),
            t_start: state.travel,
            t_end: state.travel,
            no_hit_before_t_end: false,
            bracket: Some([state.travel, state.travel]),
            provenance: None,
        }
    }

    fn dense_distance_advance_certificate(
        &self,
        policy: &TraceLoopPolicy,
        shape: &SmolStr,
        state: &TraceLoopState,
    ) -> RayStepCertificate {
        RayStepCertificate {
            kind: StepCertificateKind::DenseDistanceBound,
            metadata: self.certificate_metadata(
                RequiredGuaranteeClass::Exact,
                policy,
                shape,
                RayStepCertificateSubjectKind::Shape,
                "dense-distance-bound",
                format!("dense step {:.6}", state.step_bound),
                CertificateReuseClass::RenderingAndCollision,
                vec![SmolStr::new("distance semantics changed")],
            ),
            t_start: state.travel,
            t_end: state.travel + state.step_bound,
            no_hit_before_t_end: true,
            bracket: None,
            provenance: None,
        }
    }

    fn certificate_metadata(
        &self,
        guarantee: RequiredGuaranteeClass,
        policy: &TraceLoopPolicy,
        shape: &SmolStr,
        subject_kind: RayStepCertificateSubjectKind,
        proof_family: impl Into<SmolStr>,
        tolerance_context: impl Into<SmolStr>,
        reusable_by: CertificateReuseClass,
        invalidation_reasons: Vec<SmolStr>,
    ) -> RayStepCertificateMetadata {
        RayStepCertificateMetadata {
            guarantee,
            proof_family: proof_family.into(),
            subject: if policy.subject.is_empty() {
                shape.clone()
            } else {
                policy.subject.clone()
            },
            subject_kind,
            tolerance_context: tolerance_context.into(),
            reusable_by,
            invalidation_reasons,
        }
    }

    fn analytic_primitive_for_shape(
        &self,
        shape: &SmolStr,
        origin: [f32; 3],
        direction: [f32; 3],
    ) -> Result<Option<AnalyticPrimitiveRay>, QueryExecError> {
        let scene = self.shape_scene(shape)?;
        let leaf = match &scene.root {
            ShapeNode::Leaf(leaf) => leaf,
            ShapeNode::Use { target } => {
                let target_scene = self.shape_scene(target)?;
                let ShapeNode::Leaf(leaf) = &target_scene.root else {
                    return Ok(None);
                };
                leaf
            }
            _ => return Ok(None),
        };
        let field = self.field_scene(&leaf.field)?;
        self.analytic_primitive_for_field_node(&field.root, origin, direction)
    }

    fn analytic_primitive_for_field_node(
        &self,
        node: &FieldNode,
        origin: [f32; 3],
        direction: [f32; 3],
    ) -> Result<Option<AnalyticPrimitiveRay>, QueryExecError> {
        match node {
            FieldNode::Use { target } => {
                let field = self.field_scene(target)?;
                self.analytic_primitive_for_field_node(&field.root, origin, direction)
            }
            FieldNode::Transform { kind, param, inner } => {
                let Some(param) = param else {
                    return self.analytic_primitive_for_field_node(inner, origin, direction);
                };
                match kind {
                    TransformKind::Translate
                    | TransformKind::Rotate
                    | TransformKind::UniformScale => {
                        let local_origin = self.eval_wrapped_point(*kind, param, origin)?;
                        let local_direction = self.eval_wrapped_vector(*kind, param, direction)?;
                        self.analytic_primitive_for_field_node(inner, local_origin, local_direction)
                    }
                    _ => Ok(None),
                }
            }
            FieldNode::Primitive { primitive, args } => {
                let primitive = match primitive {
                    hir::FieldPrimitive::Sphere => {
                        let radius = self
                            .eval_scene_named_arg_opt(args.as_deref().unwrap_or(&[]), "radius")?
                            .map(|value| expect_f32(Some(&value), "sphere radius"))
                            .transpose()?
                            .unwrap_or(1.0)
                            .abs();
                        AnalyticPrimitive::Sphere { radius }
                    }
                    hir::FieldPrimitive::Plane => {
                        let normal = self
                            .eval_scene_named_arg_opt(args.as_deref().unwrap_or(&[]), "normal")?
                            .map(|value| expect_vec3(Some(&value), "plane normal"))
                            .transpose()?
                            .unwrap_or([0.0, 1.0, 0.0]);
                        let offset = self
                            .eval_scene_named_arg_opt(args.as_deref().unwrap_or(&[]), "offset")?
                            .map(|value| expect_f32(Some(&value), "plane offset"))
                            .transpose()?
                            .unwrap_or(0.0);
                        AnalyticPrimitive::Plane { normal, offset }
                    }
                    hir::FieldPrimitive::Slab => {
                        let thickness = self
                            .eval_scene_named_arg_opt(args.as_deref().unwrap_or(&[]), "thickness")?
                            .map(|value| expect_f32(Some(&value), "slab thickness"))
                            .transpose()?
                            .unwrap_or(0.0)
                            .abs();
                        AnalyticPrimitive::Slab { thickness }
                    }
                    hir::FieldPrimitive::Box => {
                        let half = self
                            .eval_scene_named_arg_opt(args.as_deref().unwrap_or(&[]), "half")?
                            .map(|value| expect_vec3(Some(&value), "box half"))
                            .transpose()?
                            .unwrap_or([0.5, 0.5, 0.5]);
                        AnalyticPrimitive::Box { half }
                    }
                    hir::FieldPrimitive::Capsule => {
                        let a = self.eval_scene_named_arg(args.as_deref().unwrap_or(&[]), "a")?;
                        let b = self.eval_scene_named_arg(args.as_deref().unwrap_or(&[]), "b")?;
                        let radius =
                            self.eval_scene_named_arg(args.as_deref().unwrap_or(&[]), "radius")?;
                        AnalyticPrimitive::Capsule {
                            a: expect_vec3(Some(&a), "capsule a")?,
                            b: expect_vec3(Some(&b), "capsule b")?,
                            radius: expect_f32(Some(&radius), "capsule radius")?.abs(),
                        }
                    }
                    hir::FieldPrimitive::Cylinder => {
                        let radius =
                            self.eval_scene_named_arg(args.as_deref().unwrap_or(&[]), "radius")?;
                        let half_height = self
                            .eval_scene_named_arg(args.as_deref().unwrap_or(&[]), "half_height")?;
                        AnalyticPrimitive::Cylinder {
                            radius: expect_f32(Some(&radius), "cylinder radius")?.abs(),
                            half_height: expect_f32(Some(&half_height), "cylinder half height")?
                                .abs(),
                        }
                    }
                    _ => return Ok(None),
                };
                Ok(Some(AnalyticPrimitiveRay {
                    primitive,
                    local_origin: origin,
                    local_direction: direction,
                }))
            }
            _ => Ok(None),
        }
    }

    fn try_repeat_linear_shape_hit(
        &self,
        shape: &SmolStr,
        origin: [f32; 3],
        direction: [f32; 3],
        start_travel: f32,
        max_distance: f32,
        min_step: f32,
        hit_epsilon: f32,
        max_steps: i32,
        _policy: &TraceLoopPolicy,
    ) -> Result<RepeatAwareTraceOutcome, QueryExecError> {
        let scene = self.shape_scene(shape)?;
        let leaf = match &scene.root {
            ShapeNode::Leaf(leaf) => leaf,
            _ => {
                return Ok(RepeatAwareTraceOutcome::Unsupported(
                    RepeatAwareUnsupportedReason::Form,
                ));
            }
        };
        let field = self.field_scene(&leaf.field)?;
        let traversal = match &field.root {
            FieldNode::Repeat {
                kind: RepeatKind::RepeatLinear,
                param: Some(period_expr),
                inner,
            } => {
                let child_support = field
                    .support_node_record(field.root_support_id)
                    .and_then(|record| record.children.first().copied())
                    .ok_or(RepeatAwareUnsupportedReason::Bounds);
                let child_support = match child_support {
                    Ok(child_support) => child_support,
                    Err(reason) => return Ok(RepeatAwareTraceOutcome::Unsupported(reason)),
                };
                let Some(bounds) = self.field_support_bounds(field, child_support)? else {
                    return Ok(RepeatAwareTraceOutcome::Unsupported(
                        RepeatAwareUnsupportedReason::Bounds,
                    ));
                };
                let period_value = self.eval_scene_constant(period_expr)?;
                let period = expect_vec3(Some(&period_value), "repeat linear period")?;
                RepeatLinearTraversal {
                    inner: inner.as_ref(),
                    local_origin: origin,
                    local_direction: direction,
                    bounds,
                    period,
                }
            }
            FieldNode::Transform {
                kind,
                param: Some(param),
                inner,
            } if matches!(
                kind,
                TransformKind::Translate | TransformKind::AffineTransform
            ) =>
            {
                let Some((local_origin, local_direction)) =
                    self.repeat_aware_local_ray(*kind, param, origin, direction)?
                else {
                    return Ok(RepeatAwareTraceOutcome::Unsupported(
                        RepeatAwareUnsupportedReason::Form,
                    ));
                };
                let FieldNode::Repeat {
                    kind: RepeatKind::RepeatLinear,
                    param: Some(period_expr),
                    inner,
                } = inner.as_ref()
                else {
                    return Ok(RepeatAwareTraceOutcome::Unsupported(
                        RepeatAwareUnsupportedReason::Form,
                    ));
                };
                let repeat_support = field
                    .support_node_record(field.root_support_id)
                    .and_then(|record| record.children.first().copied())
                    .and_then(|repeat_id| {
                        field
                            .support_node_record(repeat_id)
                            .and_then(|record| record.children.first().copied())
                    });
                let Some(child_support) = repeat_support else {
                    return Ok(RepeatAwareTraceOutcome::Unsupported(
                        RepeatAwareUnsupportedReason::Bounds,
                    ));
                };
                let Some(bounds) = self.field_support_bounds(field, child_support)? else {
                    return Ok(RepeatAwareTraceOutcome::Unsupported(
                        RepeatAwareUnsupportedReason::Bounds,
                    ));
                };
                let period_value = self.eval_scene_constant(period_expr)?;
                let period = expect_vec3(Some(&period_value), "repeat linear period")?;
                RepeatLinearTraversal {
                    inner: inner.as_ref(),
                    local_origin,
                    local_direction,
                    bounds,
                    period,
                }
            }
            _ => {
                return Ok(RepeatAwareTraceOutcome::Unsupported(
                    RepeatAwareUnsupportedReason::Form,
                ));
            }
        };
        let Some(axis) = axis_aligned_repeat_axis(traversal.period) else {
            return Ok(RepeatAwareTraceOutcome::Unsupported(
                RepeatAwareUnsupportedReason::Form,
            ));
        };
        let cells = axis_aligned_repeat_linear_cells(
            traversal.bounds,
            axis,
            traversal.period[axis],
            traversal.local_origin,
            traversal.local_direction,
            start_travel.max(0.0),
            max_distance,
        );
        self.note_solver_repeat_cells_enumerated(cells.len().min(u32::MAX as usize) as u32);
        if cells.is_empty() {
            return Ok(RepeatAwareTraceOutcome::Inapplicable(default_hit(origin)));
        }

        for (offset, entry_t, exit_t) in cells {
            let local_origin = [
                traversal.local_origin[0] - offset[0],
                traversal.local_origin[1] - offset[1],
                traversal.local_origin[2] - offset[2],
            ];
            if let Some((travel, steps)) = self.trace_field_node_dense(
                traversal.inner,
                local_origin,
                traversal.local_direction,
                entry_t,
                exit_t,
                min_step,
                hit_epsilon,
                max_steps,
            )? {
                let point = [
                    origin[0] + direction[0] * travel,
                    origin[1] + direction[1] * travel,
                    origin[2] + direction[2] * travel,
                ];
                return self
                    .shape_hit_value(shape, travel, point, steps)
                    .map(RepeatAwareTraceOutcome::Finished);
            }
            self.note_repeat_cell_skip();
        }

        Ok(RepeatAwareTraceOutcome::Finished(default_hit(origin)))
    }

    fn repeat_aware_local_ray(
        &self,
        kind: TransformKind,
        param: &SceneValueExpr,
        origin: [f32; 3],
        direction: [f32; 3],
    ) -> Result<Option<([f32; 3], [f32; 3])>, QueryExecError> {
        let config = self.eval_scene_constant(param)?;
        match kind {
            TransformKind::Translate => {
                let translation = expect_vec3(Some(&config), "repeat-aware translation")?;
                Ok(Some((
                    [
                        origin[0] - translation[0],
                        origin[1] - translation[1],
                        origin[2] - translation[2],
                    ],
                    direction,
                )))
            }
            TransformKind::AffineTransform => {
                pure_translation_local_ray(&config, origin, direction)
            }
            _ => Ok(None),
        }
    }

    fn trace_field_node_dense(
        &self,
        node: &FieldNode,
        origin: [f32; 3],
        direction: [f32; 3],
        start_travel: f32,
        max_distance: f32,
        min_step: f32,
        hit_epsilon: f32,
        max_steps: i32,
    ) -> Result<Option<(f32, i32)>, QueryExecError> {
        let mut travel = start_travel.max(0.0);
        let mut steps = 0i32;
        while steps < max_steps && travel <= max_distance {
            self.note_trace_step();
            let point = [
                origin[0] + direction[0] * travel,
                origin[1] + direction[1] * travel,
                origin[2] + direction[2] * travel,
            ];
            let distance = self.eval_field_node(node, point)?;
            if distance <= hit_epsilon {
                return Ok(Some((travel, steps)));
            }
            travel += distance.max(min_step);
            steps += 1;
        }
        Ok(None)
    }

    fn try_relaxed_or_interval_certificate(
        &self,
        shape: &SmolStr,
        origin: [f32; 3],
        direction: [f32; 3],
        max_distance: f32,
        min_step: f32,
        hit_epsilon: f32,
        state: &TraceLoopState,
        policy: &TraceLoopPolicy,
    ) -> Result<Option<TraceStepDecision>, QueryExecError> {
        let allow_bracket = self.shape_is_exact(shape)?;
        let lipschitz_bound = ray_parameter_lipschitz_bound(direction);

        if allow_bracket && policy.method_enabled(RaySolverMethod::LipschitzSafeStepping) {
            let factor = relaxed_step_factor(state.previous_distance, state.distance);
            let candidate_end = (state.travel + state.step_bound * factor).min(max_distance);
            let required_relaxed_end = state.travel + (state.step_bound * 1.5);
            let relaxed_admission = state.previous_distance.is_some()
                && (state.non_improving_distance || state.consecutive_small_steps >= 1);
            if relaxed_admission && candidate_end + f32::EPSILON >= required_relaxed_end {
                self.note_solver_relaxed_attempt();
                match self.prove_shape_interval(
                    shape,
                    origin,
                    direction,
                    state.sample,
                    candidate_end,
                    None,
                    hit_epsilon,
                    lipschitz_bound,
                    allow_bracket,
                    0,
                    2,
                )? {
                    IntervalProofOutcome::NoRoot { end_t, end_sample } => {
                        self.note_solver_relaxed_no_root_advance();
                        self.note_solver_lipschitz_step();
                        self.note_interval_proof_success();
                        self.note_accepted_relaxed_step();
                        return Ok(Some(TraceStepDecision::Advance {
                            certificate: RayStepCertificate {
                                kind: StepCertificateKind::RelaxedConservativeJump,
                                metadata: self.certificate_metadata(
                                    RequiredGuaranteeClass::IntervalBounded,
                                    policy,
                                    shape,
                                    RayStepCertificateSubjectKind::Interval,
                                    "lipschitz-relaxed-proof",
                                    format!("relaxed_factor={factor:.2}; lipschitz_bound={lipschitz_bound:.6}"),
                                    CertificateReuseClass::RenderingAndCollision,
                                    vec![
                                        SmolStr::new("lipschitz evidence changed"),
                                        SmolStr::new("distance semantics changed"),
                                    ],
                                ),
                                t_start: state.travel,
                                t_end: end_t,
                                no_hit_before_t_end: true,
                                bracket: None,
                                provenance: None,
                            },
                            next_sample: Some(end_sample),
                        }));
                    }
                    IntervalProofOutcome::Bracket { bracket } => {
                        self.note_solver_relaxed_bracket();
                        if let Some(certificate) = self.refine_shape_bracket(
                            shape,
                            origin,
                            direction,
                            bracket,
                            hit_epsilon,
                            policy,
                        )? {
                            return Ok(Some(TraceStepDecision::Hit(certificate)));
                        }
                        self.note_rejected_relaxed_step();
                        self.note_solver_relaxed_unresolved();
                        return Ok(None);
                    }
                    IntervalProofOutcome::Unresolved => {
                        self.note_rejected_relaxed_step();
                        self.note_solver_relaxed_unresolved();
                        return Ok(None);
                    }
                }
            }
        }

        let stall_step_threshold = (min_step * 2.0).max(state.adaptive_epsilon * 8.0);
        let hard_ray = state.consecutive_small_steps >= 2
            || (state.non_improving_distance && state.step_bound <= stall_step_threshold);
        if policy.method_enabled(RaySolverMethod::IntervalNewtonIsolation) && hard_ray {
            let candidate_end = (state.travel + state.step_bound * 2.0).min(max_distance);
            if candidate_end > state.travel + f32::EPSILON {
                self.note_solver_interval_attempt();
                match self.prove_shape_interval(
                    shape,
                    origin,
                    direction,
                    state.sample,
                    candidate_end,
                    None,
                    hit_epsilon,
                    lipschitz_bound,
                    allow_bracket,
                    0,
                    4,
                )? {
                    IntervalProofOutcome::NoRoot { end_t, end_sample } => {
                        self.note_solver_interval_no_root_advance();
                        self.note_solver_interval_skip();
                        self.note_interval_proof_success();
                        return Ok(Some(TraceStepDecision::Advance {
                            certificate: RayStepCertificate {
                                kind: StepCertificateKind::IntervalNoRootProof,
                                metadata: self.certificate_metadata(
                                    RequiredGuaranteeClass::IntervalBounded,
                                    policy,
                                    shape,
                                    RayStepCertificateSubjectKind::Interval,
                                    "interval-no-root-proof",
                                    format!("lipschitz_bound={lipschitz_bound:.6}; hard_ray=true"),
                                    CertificateReuseClass::RenderingAndCollision,
                                    vec![
                                        SmolStr::new("interval evidence changed"),
                                        SmolStr::new("distance semantics changed"),
                                    ],
                                ),
                                t_start: state.travel,
                                t_end: end_t,
                                no_hit_before_t_end: true,
                                bracket: None,
                                provenance: None,
                            },
                            next_sample: Some(end_sample),
                        }));
                    }
                    IntervalProofOutcome::Bracket { bracket } => {
                        self.note_solver_interval_bracket();
                        if let Some(certificate) = self.refine_shape_bracket(
                            shape,
                            origin,
                            direction,
                            bracket,
                            hit_epsilon,
                            policy,
                        )? {
                            return Ok(Some(TraceStepDecision::Hit(certificate)));
                        }
                        self.note_solver_interval_unresolved();
                        return Ok(None);
                    }
                    IntervalProofOutcome::Unresolved => {
                        self.note_solver_interval_unresolved();
                        self.note_solver_certificate_failure();
                        return Ok(None);
                    }
                }
            }
        }

        Ok(None)
    }

    fn prove_shape_interval(
        &self,
        shape: &SmolStr,
        origin: [f32; 3],
        direction: [f32; 3],
        start_sample: IntervalSample,
        end_t: f32,
        end_sample: Option<IntervalSample>,
        hit_epsilon: f32,
        lipschitz_bound: f32,
        allow_bracket: bool,
        depth: u32,
        max_depth: u32,
    ) -> Result<IntervalProofOutcome, QueryExecError> {
        self.note_interval_subdivision();
        if end_t <= start_sample.t + f32::EPSILON {
            return Ok(IntervalProofOutcome::Unresolved);
        }
        let span = end_t - start_sample.t;
        let end_sample_value = match end_sample {
            Some(sample) => sample,
            None => self.interval_sample(shape, origin, direction, end_t, hit_epsilon)?,
        };
        if end_sample_value.distance - lipschitz_bound * span > end_sample_value.adaptive_epsilon {
            return Ok(IntervalProofOutcome::NoRoot {
                end_t,
                end_sample: end_sample_value,
            });
        }
        if allow_bracket
            && (end_sample_value.distance.abs() <= end_sample_value.adaptive_epsilon
                || end_sample_value.distance < 0.0)
        {
            return Ok(IntervalProofOutcome::Bracket {
                bracket: BracketRefinement {
                    lo: start_sample,
                    hi: end_sample_value,
                },
            });
        }

        let mid_t = 0.5 * (start_sample.t + end_t);
        let mid_sample = self.interval_sample(shape, origin, direction, mid_t, hit_epsilon)?;
        let radius = 0.5 * span;
        if mid_sample.distance - lipschitz_bound * radius > mid_sample.adaptive_epsilon {
            return Ok(IntervalProofOutcome::NoRoot {
                end_t,
                end_sample: end_sample_value,
            });
        }

        if depth >= max_depth || radius <= hit_epsilon * 2.0 {
            return Ok(IntervalProofOutcome::Unresolved);
        }

        let left = self.prove_shape_interval(
            shape,
            origin,
            direction,
            start_sample,
            mid_t,
            Some(mid_sample),
            hit_epsilon,
            lipschitz_bound,
            allow_bracket,
            depth + 1,
            max_depth,
        )?;
        match left {
            IntervalProofOutcome::Bracket { .. } => return Ok(left),
            IntervalProofOutcome::NoRoot { .. } => {}
            IntervalProofOutcome::Unresolved => return Ok(IntervalProofOutcome::Unresolved),
        }

        let right = self.prove_shape_interval(
            shape,
            origin,
            direction,
            mid_sample,
            end_t,
            Some(end_sample_value),
            hit_epsilon,
            lipschitz_bound,
            allow_bracket,
            depth + 1,
            max_depth,
        )?;
        match right {
            IntervalProofOutcome::Bracket { .. } => Ok(right),
            IntervalProofOutcome::NoRoot { end_sample, .. } => {
                Ok(IntervalProofOutcome::NoRoot { end_t, end_sample })
            }
            IntervalProofOutcome::Unresolved => Ok(IntervalProofOutcome::Unresolved),
        }
    }

    fn refine_shape_bracket(
        &self,
        shape: &SmolStr,
        origin: [f32; 3],
        direction: [f32; 3],
        bracket: BracketRefinement,
        hit_epsilon: f32,
        policy: &TraceLoopPolicy,
    ) -> Result<Option<RayStepCertificate>, QueryExecError> {
        if !policy.method_enabled(RaySolverMethod::SafeguardedNewtonRefinement) {
            return Ok(None);
        }
        self.note_solver_refinement_attempt();
        let mut lo = bracket.lo;
        let mut hi = bracket.hi;
        if hi.distance.abs() > hi.adaptive_epsilon && hi.distance > 0.0 {
            self.note_solver_refinement_failure();
            return Ok(None);
        }
        if hi.distance.abs() <= hi.adaptive_epsilon || (hi.t - lo.t).abs() <= hi.adaptive_epsilon {
            self.note_solver_newton_refinement();
            return Ok(Some(RayStepCertificate {
                kind: StepCertificateKind::RefinementBracket,
                metadata: self.certificate_metadata(
                    RequiredGuaranteeClass::IntervalBounded,
                    policy,
                    shape,
                    RayStepCertificateSubjectKind::Interval,
                    "safeguarded-newton-refinement",
                    format!("adaptive_epsilon={:.6}", hi.adaptive_epsilon),
                    CertificateReuseClass::RenderingAndCollision,
                    vec![
                        SmolStr::new("certified gradient changed"),
                        SmolStr::new("distance semantics changed"),
                    ],
                ),
                t_start: bracket.lo.t,
                t_end: hi.t,
                no_hit_before_t_end: true,
                bracket: Some([lo.t, hi.t]),
                provenance: None,
            }));
        }

        let mut current = hi;

        for _ in 0..8 {
            if current.distance.abs() <= current.adaptive_epsilon
                || (hi.t - lo.t).abs() <= current.adaptive_epsilon
            {
                self.note_solver_newton_refinement();
                return Ok(Some(RayStepCertificate {
                    kind: StepCertificateKind::RefinementBracket,
                    metadata: self.certificate_metadata(
                        RequiredGuaranteeClass::IntervalBounded,
                        policy,
                        shape,
                        RayStepCertificateSubjectKind::Interval,
                        "safeguarded-newton-refinement",
                        format!("adaptive_epsilon={:.6}", current.adaptive_epsilon),
                        CertificateReuseClass::RenderingAndCollision,
                        vec![
                            SmolStr::new("certified gradient changed"),
                            SmolStr::new("distance semantics changed"),
                        ],
                    ),
                    t_start: bracket.lo.t,
                    t_end: current.t,
                    no_hit_before_t_end: true,
                    bracket: Some([lo.t, hi.t]),
                    provenance: None,
                }));
            }

            if current.distance > 0.0 {
                lo = current;
            } else {
                hi = current;
            }

            let point = [
                origin[0] + direction[0] * current.t,
                origin[1] + direction[1] * current.t,
                origin[2] + direction[2] * current.t,
            ];
            let next = match self.certified_shape_directional_derivative(shape, point, direction)? {
                Some((derivative, _gradient_mag)) if derivative.abs() > 1e-5 => {
                    let candidate = current.t - current.distance / derivative;
                    if candidate > lo.t && candidate < hi.t {
                        candidate
                    } else {
                        0.5 * (lo.t + hi.t)
                    }
                }
                _ => {
                    self.note_solver_refinement_failure();
                    return Ok(None);
                }
            };
            current = self.interval_sample(shape, origin, direction, next, hit_epsilon)?;
        }

        if hi.distance.abs() <= hi.adaptive_epsilon {
            self.note_solver_newton_refinement();
            return Ok(Some(RayStepCertificate {
                kind: StepCertificateKind::RefinementBracket,
                metadata: self.certificate_metadata(
                    RequiredGuaranteeClass::IntervalBounded,
                    policy,
                    shape,
                    RayStepCertificateSubjectKind::Interval,
                    "safeguarded-newton-refinement",
                    format!("adaptive_epsilon={:.6}", hi.adaptive_epsilon),
                    CertificateReuseClass::RenderingAndCollision,
                    vec![
                        SmolStr::new("certified gradient changed"),
                        SmolStr::new("distance semantics changed"),
                    ],
                ),
                t_start: bracket.lo.t,
                t_end: hi.t,
                no_hit_before_t_end: true,
                bracket: Some([lo.t, hi.t]),
                provenance: None,
            }));
        }

        self.note_solver_refinement_failure();
        Ok(None)
    }

    fn interval_sample(
        &self,
        shape: &SmolStr,
        origin: [f32; 3],
        direction: [f32; 3],
        travel: f32,
        hit_epsilon: f32,
    ) -> Result<IntervalSample, QueryExecError> {
        let point = [
            origin[0] + direction[0] * travel,
            origin[1] + direction[1] * travel,
            origin[2] + direction[2] * travel,
        ];
        let distance = self.eval_shape_distance(shape, point)?;
        let adaptive_epsilon =
            self.shape_adaptive_hit_epsilon(shape, travel, point, hit_epsilon)?;
        Ok(IntervalSample {
            t: travel,
            distance,
            adaptive_epsilon,
        })
    }

    fn shape_is_exact(&self, shape: &SmolStr) -> Result<bool, QueryExecError> {
        Ok(matches!(
            self.shape_scene(shape)?.semantics,
            DistanceSemantics::ExactSignedDistance
        ))
    }

    fn shape_scale_hint(&self, shape: &SmolStr) -> Result<f32, QueryExecError> {
        let Some((min, max)) = self.shape_support_bounds_world(shape)? else {
            return Ok(1.0);
        };
        Ok((0..3)
            .map(|axis| (max[axis] - min[axis]).abs())
            .fold(1.0f32, f32::max)
            .max(1.0))
    }

    fn shape_adaptive_hit_epsilon(
        &self,
        shape: &SmolStr,
        travel: f32,
        point: [f32; 3],
        base: f32,
    ) -> Result<f32, QueryExecError> {
        let gradient_mag = self
            .try_certified_shape_normal(shape, point)?
            .map(|evaluation| dot3(evaluation.normal, evaluation.normal).sqrt())
            .unwrap_or(1.0);
        let epsilon = adaptive_hit_epsilon_with_gradient(
            base,
            travel,
            self.shape_scale_hint(shape)?,
            gradient_mag,
        );
        self.note_solver_adaptive_epsilon();
        Ok(epsilon)
    }

    fn certified_shape_directional_derivative(
        &self,
        shape: &SmolStr,
        point: [f32; 3],
        direction: [f32; 3],
    ) -> Result<Option<(f32, f32)>, QueryExecError> {
        let Some(evaluation) = self.try_certified_shape_normal(shape, point)? else {
            return Ok(None);
        };
        let gradient_mag = dot3(evaluation.normal, evaluation.normal).sqrt().max(1e-5);
        Ok(Some((dot3(evaluation.normal, direction), gradient_mag)))
    }

    fn shape_hit_value(
        &self,
        shape: &SmolStr,
        travel: f32,
        point: [f32; 3],
        steps: i32,
    ) -> Result<KernelValue, QueryExecError> {
        let normal = self.eval_shape_normal(shape, point)?;
        let winner = self.eval_shape_winner(shape, point)?;
        let feature_id = winner.feature_id;
        let (payload, local_position, local_normal, instance_id, repeat_id) = self
            .shape_leaf_from_winner(shape, feature_id, winner.leaf.as_ref())
            .map(|leaf| {
                let local_frame = self.eval_field_local_frame(&leaf.field, point)?;
                let local_normal = self.eval_field_local_normal(&leaf.field, point)?;
                let payload = self
                    .eval_payload_body(&leaf.payload)
                    .unwrap_or_else(|_| default_payload());
                Ok::<_, QueryExecError>((
                    payload,
                    local_frame.point,
                    local_normal,
                    local_frame.instance_id,
                    local_frame.repeat_id,
                ))
            })
            .transpose()?
            .unwrap_or_else(|| (default_payload(), point, normal, 0, 0));
        Ok(hit_value(
            true,
            travel,
            point,
            normal,
            local_position,
            local_normal,
            steps,
            feature_id,
            instance_id,
            repeat_id,
            stable_shape_capture_id(shape),
            payload,
        ))
    }

    pub(crate) fn surface_at(
        &self,
        shape: &SmolStr,
        hit: &KernelStructValue,
    ) -> Result<KernelValue, QueryExecError> {
        let feature_id = expect_struct_u32(hit, "feature_id")?;
        let Some(leaf) = self
            .ctx
            .shape_leaf_ref(shape, feature_id)
            .and_then(|leaf_ref| self.ctx.shape_leaf(&leaf_ref.scene, leaf_ref.leaf))
        else {
            return Ok(default_surface());
        };
        self.execute_portable_function(&leaf.material, vec![KernelValue::Struct(hit.clone())])
    }

    pub(crate) fn radiance_at(
        &self,
        shape: &SmolStr,
        point: [f32; 3],
        direction: [f32; 3],
    ) -> Result<KernelValue, QueryExecError> {
        let scene = self.shape_scene(shape)?;
        Ok(KernelValue::Vec3(self.eval_shape_radiance_node(
            &scene.root,
            point,
            direction,
        )?))
    }

    pub(crate) fn medium_at(
        &self,
        shape: &SmolStr,
        point: [f32; 3],
    ) -> Result<KernelValue, QueryExecError> {
        let scene = self.shape_scene(shape)?;
        self.eval_shape_medium_node(&scene.root, point)
    }

    pub(crate) fn eval_field_support_lower_bound(
        &self,
        field: &SmolStr,
        point: [f32; 3],
    ) -> Result<Option<f32>, QueryExecError> {
        let scene = self.field_scene(field)?;
        self.eval_support_lower_bound_for_field_scene(scene, point)
    }

    pub(crate) fn eval_shape_support_lower_bound(
        &self,
        shape: &SmolStr,
        point: [f32; 3],
    ) -> Result<Option<f32>, QueryExecError> {
        let scene = self.shape_scene(shape)?;
        self.eval_support_lower_bound_for_shape_scene(scene, point)
    }

    fn eval_support_lower_bound_for_field_scene(
        &self,
        scene: &crate::scene_ir::FieldScene,
        point: [f32; 3],
    ) -> Result<Option<f32>, QueryExecError> {
        if scene.opaque_boundary
            || !scene.can_coarse_support_pruning
            || matches!(
                scene.semantics,
                crate::scene_ir::DistanceSemantics::UnknownOpaque
            )
        {
            return Ok(None);
        }
        self.eval_field_support_record(scene, scene.root_support_id, point)
    }

    fn eval_support_lower_bound_for_shape_scene(
        &self,
        scene: &crate::scene_ir::ShapeScene,
        point: [f32; 3],
    ) -> Result<Option<f32>, QueryExecError> {
        if scene.opaque_boundary
            || !scene.can_coarse_support_pruning
            || matches!(
                scene.semantics,
                crate::scene_ir::DistanceSemantics::UnknownOpaque
            )
        {
            return Ok(None);
        }
        self.eval_shape_support_record(scene, scene.root_support_id, point)
    }

    fn eval_field_support_record(
        &self,
        scene: &crate::scene_ir::FieldScene,
        id: crate::scene_ir::SupportNodeId,
        point: [f32; 3],
    ) -> Result<Option<f32>, QueryExecError> {
        self.note_cache_brick_visit();
        let Some(record) = scene.support_node_record(id) else {
            self.note_cache_brick_miss();
            return Ok(None);
        };
        self.note_cache_brick_hit();
        self.note_artifact_load();
        match record.kind {
            crate::scene_ir::SupportNodeKindSummary::Unknown
            | crate::scene_ir::SupportNodeKindSummary::Unbounded => Ok(None),
            crate::scene_ir::SupportNodeKindSummary::Use => {
                let Some(target) = record.target.as_ref() else {
                    return Ok(None);
                };
                self.note_ray_support_entry_jump();
                self.eval_field_support_lower_bound(target, point)
            }
            crate::scene_ir::SupportNodeKindSummary::Aabb
            | crate::scene_ir::SupportNodeKindSummary::Sphere
            | crate::scene_ir::SupportNodeKindSummary::OpaqueBoundary => {
                self.eval_support_leaf_payload(record, point)
            }
            crate::scene_ir::SupportNodeKindSummary::Union => {
                self.eval_field_support_children(scene, &record.children, point, f32::min)
            }
            crate::scene_ir::SupportNodeKindSummary::Intersection => {
                self.eval_field_support_children(scene, &record.children, point, f32::max)
            }
            crate::scene_ir::SupportNodeKindSummary::Difference => {
                let Some(left) = record.children.first() else {
                    return Ok(None);
                };
                self.note_ray_support_entry_jump();
                self.eval_field_support_record(scene, *left, point)
            }
            crate::scene_ir::SupportNodeKindSummary::Transform(kind) => {
                let Some(crate::scene_ir::SupportPayload::Transform { param }) =
                    record.payload.as_ref()
                else {
                    return Ok(None);
                };
                let Some(param) = param.as_ref() else {
                    return Ok(None);
                };
                let Some(child) = record.children.first() else {
                    return Ok(None);
                };
                self.note_ray_support_entry_jump();
                match kind {
                    TransformKind::Translate | TransformKind::Rotate => {
                        let local_point = self.eval_wrapped_point(kind, param, point)?;
                        self.eval_field_support_record(scene, *child, local_point)
                    }
                    TransformKind::UniformScale => {
                        let config = self.eval_scene_value_expr(param, &HashMap::new())?;
                        let scale = expect_abs_scalar(&config)?;
                        let local_point = self.eval_wrapped_point(kind, param, point)?;
                        Ok(self
                            .eval_field_support_record(scene, *child, local_point)?
                            .map(|value| value * scale))
                    }
                    TransformKind::AffineTransform
                    | TransformKind::Warp
                    | TransformKind::Bend
                    | TransformKind::Twist
                    | TransformKind::Taper
                    | TransformKind::Displace => Ok(None),
                }
            }
            crate::scene_ir::SupportNodeKindSummary::Periodic(kind) => {
                let Some(crate::scene_ir::SupportPayload::Periodic { period }) =
                    record.payload.as_ref()
                else {
                    return Ok(None);
                };
                let Some(period) = period.as_ref() else {
                    return Ok(None);
                };
                let Some(child) = record.children.first() else {
                    return Ok(None);
                };
                self.note_repeat_cell_skip();
                self.note_ray_support_entry_jump();
                let local_point = self.eval_repeat_point(kind, period, point)?;
                self.eval_field_support_record(scene, *child, local_point)
            }
            crate::scene_ir::SupportNodeKindSummary::Repeat(kind) => {
                let Some(crate::scene_ir::SupportPayload::Repeat { param }) =
                    record.payload.as_ref()
                else {
                    return Ok(None);
                };
                let Some(param) = param.as_ref() else {
                    return Ok(None);
                };
                let Some(child) = record.children.first() else {
                    return Ok(None);
                };
                self.note_repeat_cell_skip();
                self.note_ray_support_entry_jump();
                let local_point = self.eval_repeat_point(kind, param, point)?;
                self.eval_field_support_record(scene, *child, local_point)
            }
        }
    }

    fn eval_shape_support_record(
        &self,
        scene: &crate::scene_ir::ShapeScene,
        id: crate::scene_ir::SupportNodeId,
        point: [f32; 3],
    ) -> Result<Option<f32>, QueryExecError> {
        self.note_cache_brick_visit();
        let Some(record) = scene.support_node_record(id) else {
            self.note_cache_brick_miss();
            return Ok(None);
        };
        self.note_cache_brick_hit();
        self.note_artifact_load();
        match record.kind {
            crate::scene_ir::SupportNodeKindSummary::Unknown
            | crate::scene_ir::SupportNodeKindSummary::Unbounded => Ok(None),
            crate::scene_ir::SupportNodeKindSummary::Use => {
                let Some(target) = record.target.as_ref() else {
                    return Ok(None);
                };
                if self.ctx.field_names.contains(target) {
                    self.note_ray_support_entry_jump();
                    self.eval_field_support_lower_bound(target, point)
                } else if self.ctx.shape_names.contains(target) {
                    self.note_ray_support_entry_jump();
                    self.eval_shape_support_lower_bound(target, point)
                } else {
                    Ok(None)
                }
            }
            crate::scene_ir::SupportNodeKindSummary::Aabb
            | crate::scene_ir::SupportNodeKindSummary::Sphere
            | crate::scene_ir::SupportNodeKindSummary::OpaqueBoundary => {
                self.eval_support_leaf_payload(record, point)
            }
            crate::scene_ir::SupportNodeKindSummary::Union => {
                self.eval_shape_support_children(scene, &record.children, point, f32::min)
            }
            crate::scene_ir::SupportNodeKindSummary::Intersection => {
                self.eval_shape_support_children(scene, &record.children, point, f32::max)
            }
            crate::scene_ir::SupportNodeKindSummary::Difference => {
                let Some(left) = record.children.first() else {
                    return Ok(None);
                };
                self.note_ray_support_entry_jump();
                self.eval_shape_support_record(scene, *left, point)
            }
            crate::scene_ir::SupportNodeKindSummary::Transform(kind) => {
                let Some(crate::scene_ir::SupportPayload::Transform { param }) =
                    record.payload.as_ref()
                else {
                    return Ok(None);
                };
                let Some(param) = param.as_ref() else {
                    return Ok(None);
                };
                let Some(child) = record.children.first() else {
                    return Ok(None);
                };
                match kind {
                    TransformKind::Translate | TransformKind::Rotate => {
                        let local_point = self.eval_wrapped_point(kind, param, point)?;
                        self.eval_shape_support_record(scene, *child, local_point)
                    }
                    TransformKind::UniformScale => {
                        let config = self.eval_scene_value_expr(param, &HashMap::new())?;
                        let scale = expect_abs_scalar(&config)?;
                        let local_point = self.eval_wrapped_point(kind, param, point)?;
                        Ok(self
                            .eval_shape_support_record(scene, *child, local_point)?
                            .map(|value| value * scale))
                    }
                    TransformKind::AffineTransform
                    | TransformKind::Warp
                    | TransformKind::Bend
                    | TransformKind::Twist
                    | TransformKind::Taper
                    | TransformKind::Displace => Ok(None),
                }
            }
            crate::scene_ir::SupportNodeKindSummary::Periodic(kind) => {
                let Some(crate::scene_ir::SupportPayload::Periodic { period }) =
                    record.payload.as_ref()
                else {
                    return Ok(None);
                };
                let Some(period) = period.as_ref() else {
                    return Ok(None);
                };
                let Some(child) = record.children.first() else {
                    return Ok(None);
                };
                self.note_repeat_cell_skip();
                self.note_ray_support_entry_jump();
                let local_point = self.eval_repeat_point(kind, period, point)?;
                self.eval_shape_support_record(scene, *child, local_point)
            }
            crate::scene_ir::SupportNodeKindSummary::Repeat(kind) => {
                let Some(crate::scene_ir::SupportPayload::Repeat { param }) =
                    record.payload.as_ref()
                else {
                    return Ok(None);
                };
                let Some(param) = param.as_ref() else {
                    return Ok(None);
                };
                let Some(child) = record.children.first() else {
                    return Ok(None);
                };
                self.note_repeat_cell_skip();
                self.note_ray_support_entry_jump();
                let local_point = self.eval_repeat_point(kind, param, point)?;
                self.eval_shape_support_record(scene, *child, local_point)
            }
        }
    }

    fn eval_support_leaf_payload(
        &self,
        record: &crate::scene_ir::SupportNodeRecord,
        point: [f32; 3],
    ) -> Result<Option<f32>, QueryExecError> {
        match record.payload.as_ref() {
            Some(crate::scene_ir::SupportPayload::Aabb { min, max }) => {
                let min = self.eval_scene_value_expr(min, &HashMap::new())?;
                let max = self.eval_scene_value_expr(max, &HashMap::new())?;
                support_box_lower_bound(
                    expect_vec3(Some(&min), "support min")?,
                    expect_vec3(Some(&max), "support max")?,
                    point,
                )
                .map(Some)
            }
            Some(crate::scene_ir::SupportPayload::Sphere { center, radius }) => {
                let center = self.eval_scene_value_expr(center, &HashMap::new())?;
                let radius = self.eval_scene_value_expr(radius, &HashMap::new())?;
                Ok(Some(support_sphere_lower_bound(
                    expect_vec3(Some(&center), "support center")?,
                    expect_f32(Some(&radius), "support radius")?.abs(),
                    point,
                )))
            }
            Some(crate::scene_ir::SupportPayload::OpaqueBoundary {
                bounds: Some(bounds),
            }) => {
                let bounds_value = self.eval_scene_value_expr(bounds, &HashMap::new())?;
                let bounds = expect_struct_ref(&bounds_value, "Bounds3")?;
                support_box_lower_bound(
                    expect_struct_vec3(bounds, "min")?,
                    expect_struct_vec3(bounds, "max")?,
                    point,
                )
                .map(Some)
            }
            _ => Ok(None),
        }
    }

    fn field_support_bounds(
        &self,
        scene: &crate::scene_ir::FieldScene,
        id: SupportNodeId,
    ) -> Result<Option<SupportBounds>, QueryExecError> {
        let Some(record) = scene.support_records.iter().find(|record| record.id == id) else {
            return Ok(None);
        };
        match record.kind {
            SupportNodeKindSummary::Unknown
            | SupportNodeKindSummary::Unbounded
            | SupportNodeKindSummary::Periodic(_) => Ok(None),
            SupportNodeKindSummary::Use => {
                let Some(target) = record.target.as_ref() else {
                    return Ok(None);
                };
                let target_scene = self.field_scene(target)?;
                self.field_support_bounds(target_scene, target_scene.root_support_id)
            }
            SupportNodeKindSummary::Aabb
            | SupportNodeKindSummary::Sphere
            | SupportNodeKindSummary::OpaqueBoundary => self.support_payload_bounds(record),
            SupportNodeKindSummary::Union => self.field_support_children_bounds(
                scene,
                &record.children,
                merge_union_support_bounds,
                false,
            ),
            SupportNodeKindSummary::Intersection => self.field_support_children_bounds(
                scene,
                &record.children,
                merge_intersection_support_bounds,
                true,
            ),
            SupportNodeKindSummary::Difference => record
                .children
                .first()
                .copied()
                .map(|child| self.field_support_bounds(scene, child))
                .unwrap_or(Ok(None)),
            SupportNodeKindSummary::Transform(kind) => {
                let Some(child) = record.children.first().copied() else {
                    return Ok(None);
                };
                let Some(bounds) = self.field_support_bounds(scene, child)? else {
                    return Ok(None);
                };
                let param = match record.payload.as_ref() {
                    Some(SupportPayload::Transform { param }) => param.as_ref(),
                    _ => None,
                };
                self.note_ray_support_entry_jump();
                self.transform_support_bounds(kind, param, bounds)
            }
            SupportNodeKindSummary::Repeat(kind) => {
                let Some(child) = record.children.first().copied() else {
                    return Ok(None);
                };
                let Some(bounds) = self.field_support_bounds(scene, child)? else {
                    return Ok(None);
                };
                let param = match record.payload.as_ref() {
                    Some(SupportPayload::Repeat { param }) => param.as_ref(),
                    _ => None,
                };
                self.repeat_support_bounds(kind, param, bounds)
            }
        }
    }

    fn shape_support_bounds(
        &self,
        scene: &crate::scene_ir::ShapeScene,
        id: SupportNodeId,
    ) -> Result<Option<SupportBounds>, QueryExecError> {
        let Some(record) = scene.support_records.iter().find(|record| record.id == id) else {
            return Ok(None);
        };
        match record.kind {
            SupportNodeKindSummary::Unknown
            | SupportNodeKindSummary::Unbounded
            | SupportNodeKindSummary::Periodic(_) => Ok(None),
            SupportNodeKindSummary::Use => {
                let Some(target) = record.target.as_ref() else {
                    return Ok(None);
                };
                if let Some(target_scene) = self.ctx.scene.shapes.get(target) {
                    self.shape_support_bounds(target_scene, target_scene.root_support_id)
                } else if let Some(target_scene) = self.ctx.scene.fields.get(target) {
                    self.field_support_bounds(target_scene, target_scene.root_support_id)
                } else {
                    Ok(None)
                }
            }
            SupportNodeKindSummary::Aabb
            | SupportNodeKindSummary::Sphere
            | SupportNodeKindSummary::OpaqueBoundary => self.support_payload_bounds(record),
            SupportNodeKindSummary::Union => self.shape_support_children_bounds(
                scene,
                &record.children,
                merge_union_support_bounds,
                false,
            ),
            SupportNodeKindSummary::Intersection => self.shape_support_children_bounds(
                scene,
                &record.children,
                merge_intersection_support_bounds,
                true,
            ),
            SupportNodeKindSummary::Difference => record
                .children
                .first()
                .copied()
                .map(|child| self.shape_support_bounds(scene, child))
                .unwrap_or(Ok(None)),
            SupportNodeKindSummary::Transform(kind) => {
                let Some(child) = record.children.first().copied() else {
                    return Ok(None);
                };
                let Some(bounds) = self.shape_support_bounds(scene, child)? else {
                    return Ok(None);
                };
                let param = match record.payload.as_ref() {
                    Some(SupportPayload::Transform { param }) => param.as_ref(),
                    _ => None,
                };
                self.transform_support_bounds(kind, param, bounds)
            }
            SupportNodeKindSummary::Repeat(kind) => {
                let Some(child) = record.children.first().copied() else {
                    return Ok(None);
                };
                let Some(bounds) = self.shape_support_bounds(scene, child)? else {
                    return Ok(None);
                };
                let param = match record.payload.as_ref() {
                    Some(SupportPayload::Repeat { param }) => param.as_ref(),
                    _ => None,
                };
                self.repeat_support_bounds(kind, param, bounds)
            }
        }
    }

    fn support_payload_bounds(
        &self,
        record: &crate::scene_ir::SupportNodeRecord,
    ) -> Result<Option<SupportBounds>, QueryExecError> {
        match record.payload.as_ref() {
            Some(SupportPayload::Aabb { min, max }) => {
                let min = self.eval_scene_constant(min)?;
                let max = self.eval_scene_constant(max)?;
                Ok(Some(SupportBounds {
                    min: expect_vec3(Some(&min), "support min")?,
                    max: expect_vec3(Some(&max), "support max")?,
                }))
            }
            Some(SupportPayload::Sphere { center, radius }) => {
                let center = self.eval_scene_constant(center)?;
                let radius = self.eval_scene_constant(radius)?;
                let center = expect_vec3(Some(&center), "support center")?;
                let radius = expect_f32(Some(&radius), "support radius")?.abs();
                Ok(Some(SupportBounds {
                    min: [center[0] - radius, center[1] - radius, center[2] - radius],
                    max: [center[0] + radius, center[1] + radius, center[2] + radius],
                }))
            }
            Some(SupportPayload::OpaqueBoundary {
                bounds: Some(bounds),
            }) => {
                let bounds_value = self.eval_scene_constant(bounds)?;
                let bounds = expect_struct_ref(&bounds_value, "Bounds3")?;
                Ok(Some(SupportBounds {
                    min: expect_struct_vec3(bounds, "min")?,
                    max: expect_struct_vec3(bounds, "max")?,
                }))
            }
            _ => Ok(None),
        }
    }

    pub(crate) fn shape_support_bounds_world(
        &self,
        shape: &SmolStr,
    ) -> Result<Option<([f32; 3], [f32; 3])>, QueryExecError> {
        let scene = self.shape_scene(shape)?;
        Ok(self
            .shape_support_bounds(scene, scene.root_support_id)?
            .map(|bounds| (bounds.min, bounds.max)))
    }

    fn shape_node_support_bounds(
        &self,
        node: &ShapeNode,
    ) -> Result<Option<SupportBounds>, QueryExecError> {
        match node {
            ShapeNode::Use { target } => {
                let scene = self.shape_scene(target)?;
                self.shape_support_bounds(scene, scene.root_support_id)
            }
            ShapeNode::Leaf(leaf) => {
                let field = self.field_scene(&leaf.field)?;
                self.field_support_bounds(field, field.root_support_id)
            }
            ShapeNode::Union { items } => {
                let mut result = None;
                for item in items {
                    let Some(bounds) = self.shape_node_support_bounds(item)? else {
                        return Ok(None);
                    };
                    result = Some(match result {
                        Some(current) => merge_union_support_bounds(current, bounds),
                        None => bounds,
                    });
                }
                Ok(result)
            }
            ShapeNode::Intersection { items } => {
                let mut result = None;
                for item in items {
                    let Some(bounds) = self.shape_node_support_bounds(item)? else {
                        return Ok(None);
                    };
                    result = Some(match result {
                        Some(current) => merge_intersection_support_bounds(current, bounds),
                        None => bounds,
                    });
                }
                Ok(result)
            }
            ShapeNode::Subtract { left, .. } => self.shape_node_support_bounds(left),
        }
    }

    fn world_acceleration_tree(
        &self,
        capture: &SmolStr,
        detail: i32,
    ) -> Result<Option<CpuAccelerationTree<SmolStr>>, QueryExecError> {
        if let Some(cached) = self
            .world_acceleration_cache
            .borrow()
            .get(&(capture.clone(), detail))
            .cloned()
        {
            return Ok(cached);
        }
        let tree = self
            .ctx
            .world_acceleration_forest(capture, detail)
            .and_then(|forest| {
                build_cpu_acceleration_tree_from_forest(forest, |payload| {
                    Some(payload.semantic_id.clone())
                })
            });
        self.world_acceleration_cache
            .borrow_mut()
            .insert((capture.clone(), detail), tree.clone());
        Ok(tree)
    }

    pub(crate) fn shape_root_union_candidate_bounds(
        &self,
        shape: &SmolStr,
    ) -> Result<Option<Vec<ShapeUnionAccelerationCandidate>>, QueryExecError> {
        const LARGE_UNION_THRESHOLD: usize = 4;
        let scene = self.shape_scene(shape)?;
        let ShapeNode::Union { items } = &scene.root else {
            return Ok(None);
        };
        if items.len() < LARGE_UNION_THRESHOLD {
            return Ok(None);
        }
        Ok(Some(
            items
                .iter()
                .enumerate()
                .map(|(index, item)| {
                    Ok(ShapeUnionAccelerationCandidate {
                        index,
                        bounds: self
                            .shape_node_support_bounds(item)?
                            .map(|bounds| (bounds.min, bounds.max)),
                    })
                })
                .collect::<Result<Vec<_>, QueryExecError>>()?,
        ))
    }

    fn shape_root_union_tree(
        &self,
        shape: &SmolStr,
    ) -> Result<Option<CpuAccelerationTree<usize>>, QueryExecError> {
        if let Some(cached) = self.shape_union_cache.borrow().get(shape).cloned() {
            return Ok(cached);
        }
        let tree = self
            .ctx
            .union_acceleration_forest(shape)
            .and_then(|forest| {
                build_cpu_acceleration_tree_from_forest(forest, |payload| {
                    payload
                        .feature_id
                        .as_ref()
                        .and_then(|index| index.parse::<usize>().ok())
                })
            });
        self.shape_union_cache
            .borrow_mut()
            .insert(shape.clone(), tree.clone());
        Ok(tree)
    }

    fn shape_ray_support_probe_world(
        &self,
        shape: &SmolStr,
        origin: [f32; 3],
        direction: [f32; 3],
    ) -> Result<RaySupportProbe, QueryExecError> {
        let direct = self.shape_ray_support_probe(shape, origin, direction)?;
        let Some((min, max)) = self.shape_support_bounds_world(shape)? else {
            return Ok(direct);
        };
        let bounds_probe =
            ray_support_interval_for_bounds(SupportBounds { min, max }, origin, direction);
        Ok(match direct {
            RaySupportProbe::Interval(_) => direct,
            RaySupportProbe::Rejected => match bounds_probe {
                RaySupportProbe::Interval(_) => bounds_probe,
                _ => direct,
            },
            RaySupportProbe::Unavailable => bounds_probe,
        })
    }

    fn field_ray_support_probe(
        &self,
        field: &SmolStr,
        origin: [f32; 3],
        direction: [f32; 3],
    ) -> Result<RaySupportProbe, QueryExecError> {
        let scene = self.field_scene(field)?;
        self.field_ray_support_probe_record(scene, scene.root_support_id, origin, direction)
    }

    fn field_ray_support_probe_record(
        &self,
        scene: &crate::scene_ir::FieldScene,
        id: SupportNodeId,
        origin: [f32; 3],
        direction: [f32; 3],
    ) -> Result<RaySupportProbe, QueryExecError> {
        let Some(record) = scene.support_node_record(id) else {
            return Ok(RaySupportProbe::Unavailable);
        };
        match record.kind {
            SupportNodeKindSummary::Unknown | SupportNodeKindSummary::Unbounded => {
                Ok(RaySupportProbe::Unavailable)
            }
            SupportNodeKindSummary::Use => {
                let Some(target) = record.target.as_ref() else {
                    return Ok(RaySupportProbe::Unavailable);
                };
                self.field_ray_support_probe(target, origin, direction)
            }
            SupportNodeKindSummary::Aabb
            | SupportNodeKindSummary::Sphere
            | SupportNodeKindSummary::OpaqueBoundary => {
                self.support_leaf_ray_support_probe(record, origin, direction)
            }
            SupportNodeKindSummary::Union => {
                let mut result = RaySupportProbe::Rejected;
                for child in &record.children {
                    result = merge_union_support_probe(
                        result,
                        self.field_ray_support_probe_record(scene, *child, origin, direction)?,
                    );
                    if matches!(result, RaySupportProbe::Unavailable) {
                        break;
                    }
                }
                Ok(result)
            }
            SupportNodeKindSummary::Intersection => {
                let mut result = None;
                for child in &record.children {
                    let child_probe =
                        self.field_ray_support_probe_record(scene, *child, origin, direction)?;
                    if matches!(child_probe, RaySupportProbe::Rejected) {
                        return Ok(RaySupportProbe::Rejected);
                    }
                    result = Some(match result {
                        Some(current) => merge_intersection_support_probe(current, child_probe),
                        None => child_probe,
                    });
                    if matches!(
                        result,
                        Some(RaySupportProbe::Rejected | RaySupportProbe::Unavailable)
                    ) {
                        break;
                    }
                }
                Ok(result.unwrap_or(RaySupportProbe::Unavailable))
            }
            SupportNodeKindSummary::Difference => {
                let Some(left) = record.children.first() else {
                    return Ok(RaySupportProbe::Unavailable);
                };
                self.field_ray_support_probe_record(scene, *left, origin, direction)
            }
            SupportNodeKindSummary::Transform(kind) => {
                let Some(SupportPayload::Transform { param }) = record.payload.as_ref() else {
                    return Ok(RaySupportProbe::Unavailable);
                };
                let Some(param) = param.as_ref() else {
                    return Ok(RaySupportProbe::Unavailable);
                };
                let Some(child) = record.children.first() else {
                    return Ok(RaySupportProbe::Unavailable);
                };
                let (origin, direction) = match kind {
                    TransformKind::Translate
                    | TransformKind::Rotate
                    | TransformKind::UniformScale => (
                        self.eval_wrapped_point(kind, param, origin)?,
                        self.eval_wrapped_vector(kind, param, direction)?,
                    ),
                    TransformKind::AffineTransform
                    | TransformKind::Warp
                    | TransformKind::Bend
                    | TransformKind::Twist
                    | TransformKind::Taper
                    | TransformKind::Displace => return Ok(RaySupportProbe::Unavailable),
                };
                self.field_ray_support_probe_record(scene, *child, origin, direction)
            }
            SupportNodeKindSummary::Periodic(kind) => {
                let Some(SupportPayload::Periodic { period }) = record.payload.as_ref() else {
                    return Ok(RaySupportProbe::Unavailable);
                };
                let Some(period) = period.as_ref() else {
                    return Ok(RaySupportProbe::Unavailable);
                };
                let Some(child) = record.children.first().copied() else {
                    return Ok(RaySupportProbe::Unavailable);
                };
                let Some(bounds) = self.field_support_bounds(scene, child)? else {
                    return Ok(RaySupportProbe::Unavailable);
                };
                let period = self.eval_scene_constant(period)?;
                match kind {
                    RepeatKind::RepeatLinear | RepeatKind::RepeatGrid => {
                        Ok(ray_support_interval_for_periodic_bounds(
                            bounds,
                            expect_vec3(Some(&period), "periodic support period")?,
                            origin,
                            direction,
                        ))
                    }
                    RepeatKind::RadialRepeat => Ok(ray_support_interval_for_radial_repeat_bounds(
                        bounds,
                        match &period {
                            KernelValue::Vec3(value) => value[0].abs(),
                            _ => expect_f32(Some(&period), "radial repeat period")?.abs(),
                        },
                        origin,
                        direction,
                    )),
                    RepeatKind::MirrorArray | RepeatKind::InstanceArray => {
                        Ok(RaySupportProbe::Unavailable)
                    }
                }
            }
            SupportNodeKindSummary::Repeat(kind) => {
                let Some(SupportPayload::Repeat { param }) = record.payload.as_ref() else {
                    return Ok(RaySupportProbe::Unavailable);
                };
                let Some(param) = param.as_ref() else {
                    return Ok(RaySupportProbe::Unavailable);
                };
                let Some(child) = record.children.first() else {
                    return Ok(RaySupportProbe::Unavailable);
                };
                match kind {
                    RepeatKind::MirrorArray => {
                        let config = self.eval_scene_constant(param)?;
                        let normal = expect_vec3(Some(&config), "mirror array support normal")?;
                        let direct =
                            self.field_ray_support_probe_record(scene, *child, origin, direction)?;
                        let (mirrored_origin, mirrored_direction) =
                            reflect_ray_across_plane(normal, origin, direction);
                        let mirrored = self.field_ray_support_probe_record(
                            scene,
                            *child,
                            mirrored_origin,
                            mirrored_direction,
                        )?;
                        Ok(merge_union_support_probe(direct, mirrored))
                    }
                    RepeatKind::InstanceArray => {
                        let config = self.eval_scene_constant(param)?;
                        let Some((origin, direction)) =
                            instance_array_local_ray(&config, origin, direction)?
                        else {
                            return Ok(RaySupportProbe::Unavailable);
                        };
                        self.field_ray_support_probe_record(scene, *child, origin, direction)
                    }
                    RepeatKind::RepeatLinear
                    | RepeatKind::RepeatGrid
                    | RepeatKind::RadialRepeat => Ok(RaySupportProbe::Unavailable),
                }
            }
        }
    }

    fn shape_ray_support_probe(
        &self,
        shape: &SmolStr,
        origin: [f32; 3],
        direction: [f32; 3],
    ) -> Result<RaySupportProbe, QueryExecError> {
        let scene = self.shape_scene(shape)?;
        self.shape_ray_support_probe_record(scene, scene.root_support_id, origin, direction)
    }

    fn shape_ray_support_probe_record(
        &self,
        scene: &crate::scene_ir::ShapeScene,
        id: SupportNodeId,
        origin: [f32; 3],
        direction: [f32; 3],
    ) -> Result<RaySupportProbe, QueryExecError> {
        let Some(record) = scene.support_node_record(id) else {
            return Ok(RaySupportProbe::Unavailable);
        };
        match record.kind {
            SupportNodeKindSummary::Unknown | SupportNodeKindSummary::Unbounded => {
                Ok(RaySupportProbe::Unavailable)
            }
            SupportNodeKindSummary::Use => {
                let Some(target) = record.target.as_ref() else {
                    return Ok(RaySupportProbe::Unavailable);
                };
                if self.ctx.shape_names.contains(target) {
                    self.shape_ray_support_probe(target, origin, direction)
                } else if self.ctx.field_names.contains(target) {
                    self.field_ray_support_probe(target, origin, direction)
                } else {
                    Ok(RaySupportProbe::Unavailable)
                }
            }
            SupportNodeKindSummary::Aabb
            | SupportNodeKindSummary::Sphere
            | SupportNodeKindSummary::OpaqueBoundary => {
                self.support_leaf_ray_support_probe(record, origin, direction)
            }
            SupportNodeKindSummary::Union => {
                let mut result = RaySupportProbe::Rejected;
                for child in &record.children {
                    result = merge_union_support_probe(
                        result,
                        self.shape_ray_support_probe_record(scene, *child, origin, direction)?,
                    );
                    if matches!(result, RaySupportProbe::Unavailable) {
                        break;
                    }
                }
                Ok(result)
            }
            SupportNodeKindSummary::Intersection => {
                let mut result = None;
                for child in &record.children {
                    let child_probe =
                        self.shape_ray_support_probe_record(scene, *child, origin, direction)?;
                    if matches!(child_probe, RaySupportProbe::Rejected) {
                        return Ok(RaySupportProbe::Rejected);
                    }
                    result = Some(match result {
                        Some(current) => merge_intersection_support_probe(current, child_probe),
                        None => child_probe,
                    });
                    if matches!(
                        result,
                        Some(RaySupportProbe::Rejected | RaySupportProbe::Unavailable)
                    ) {
                        break;
                    }
                }
                Ok(result.unwrap_or(RaySupportProbe::Unavailable))
            }
            SupportNodeKindSummary::Difference => {
                let Some(left) = record.children.first() else {
                    return Ok(RaySupportProbe::Unavailable);
                };
                self.shape_ray_support_probe_record(scene, *left, origin, direction)
            }
            SupportNodeKindSummary::Transform(kind) => {
                let Some(SupportPayload::Transform { param }) = record.payload.as_ref() else {
                    return Ok(RaySupportProbe::Unavailable);
                };
                let Some(param) = param.as_ref() else {
                    return Ok(RaySupportProbe::Unavailable);
                };
                let Some(child) = record.children.first() else {
                    return Ok(RaySupportProbe::Unavailable);
                };
                let (origin, direction) = match kind {
                    TransformKind::Translate
                    | TransformKind::Rotate
                    | TransformKind::UniformScale => (
                        self.eval_wrapped_point(kind, param, origin)?,
                        self.eval_wrapped_vector(kind, param, direction)?,
                    ),
                    TransformKind::AffineTransform
                    | TransformKind::Warp
                    | TransformKind::Bend
                    | TransformKind::Twist
                    | TransformKind::Taper
                    | TransformKind::Displace => return Ok(RaySupportProbe::Unavailable),
                };
                self.shape_ray_support_probe_record(scene, *child, origin, direction)
            }
            SupportNodeKindSummary::Periodic(kind) => {
                let Some(SupportPayload::Periodic { period }) = record.payload.as_ref() else {
                    return Ok(RaySupportProbe::Unavailable);
                };
                let Some(period) = period.as_ref() else {
                    return Ok(RaySupportProbe::Unavailable);
                };
                let Some(child) = record.children.first().copied() else {
                    return Ok(RaySupportProbe::Unavailable);
                };
                let Some(bounds) = self.shape_support_bounds(scene, child)? else {
                    return Ok(RaySupportProbe::Unavailable);
                };
                let period = self.eval_scene_constant(period)?;
                match kind {
                    RepeatKind::RepeatLinear | RepeatKind::RepeatGrid => {
                        Ok(ray_support_interval_for_periodic_bounds(
                            bounds,
                            expect_vec3(Some(&period), "periodic support period")?,
                            origin,
                            direction,
                        ))
                    }
                    RepeatKind::RadialRepeat => Ok(ray_support_interval_for_radial_repeat_bounds(
                        bounds,
                        match &period {
                            KernelValue::Vec3(value) => value[0].abs(),
                            _ => expect_f32(Some(&period), "radial repeat period")?.abs(),
                        },
                        origin,
                        direction,
                    )),
                    RepeatKind::MirrorArray | RepeatKind::InstanceArray => {
                        Ok(RaySupportProbe::Unavailable)
                    }
                }
            }
            SupportNodeKindSummary::Repeat(kind) => {
                let Some(SupportPayload::Repeat { param }) = record.payload.as_ref() else {
                    return Ok(RaySupportProbe::Unavailable);
                };
                let Some(param) = param.as_ref() else {
                    return Ok(RaySupportProbe::Unavailable);
                };
                let Some(child) = record.children.first() else {
                    return Ok(RaySupportProbe::Unavailable);
                };
                match kind {
                    RepeatKind::MirrorArray => {
                        let config = self.eval_scene_constant(param)?;
                        let normal = expect_vec3(Some(&config), "mirror array support normal")?;
                        let direct =
                            self.shape_ray_support_probe_record(scene, *child, origin, direction)?;
                        let (mirrored_origin, mirrored_direction) =
                            reflect_ray_across_plane(normal, origin, direction);
                        let mirrored = self.shape_ray_support_probe_record(
                            scene,
                            *child,
                            mirrored_origin,
                            mirrored_direction,
                        )?;
                        Ok(merge_union_support_probe(direct, mirrored))
                    }
                    RepeatKind::InstanceArray => {
                        let config = self.eval_scene_constant(param)?;
                        let Some((origin, direction)) =
                            instance_array_local_ray(&config, origin, direction)?
                        else {
                            return Ok(RaySupportProbe::Unavailable);
                        };
                        self.shape_ray_support_probe_record(scene, *child, origin, direction)
                    }
                    RepeatKind::RepeatLinear
                    | RepeatKind::RepeatGrid
                    | RepeatKind::RadialRepeat => Ok(RaySupportProbe::Unavailable),
                }
            }
        }
    }

    fn support_leaf_ray_support_probe(
        &self,
        record: &crate::scene_ir::SupportNodeRecord,
        origin: [f32; 3],
        direction: [f32; 3],
    ) -> Result<RaySupportProbe, QueryExecError> {
        match record.payload.as_ref() {
            Some(SupportPayload::Aabb { min, max }) => {
                let min = self.eval_scene_constant(min)?;
                let max = self.eval_scene_constant(max)?;
                Ok(ray_support_interval_for_bounds(
                    SupportBounds {
                        min: expect_vec3(Some(&min), "support min")?,
                        max: expect_vec3(Some(&max), "support max")?,
                    },
                    origin,
                    direction,
                ))
            }
            Some(SupportPayload::Sphere { center, radius }) => {
                let center = self.eval_scene_constant(center)?;
                let radius = self.eval_scene_constant(radius)?;
                Ok(ray_support_interval_for_sphere(
                    expect_vec3(Some(&center), "support center")?,
                    expect_f32(Some(&radius), "support radius")?.abs(),
                    origin,
                    direction,
                ))
            }
            Some(SupportPayload::OpaqueBoundary {
                bounds: Some(bounds),
            }) => {
                let bounds = self.eval_scene_constant(bounds)?;
                let bounds = expect_struct_ref(&bounds, "Bounds3")?;
                Ok(ray_support_interval_for_bounds(
                    SupportBounds {
                        min: expect_struct_vec3(bounds, "min")?,
                        max: expect_struct_vec3(bounds, "max")?,
                    },
                    origin,
                    direction,
                ))
            }
            _ => Ok(RaySupportProbe::Unavailable),
        }
    }

    pub(crate) fn region_shape_support_bounds(
        &self,
        capture: &SmolStr,
        detail: i32,
    ) -> Result<Vec<(SmolStr, [f32; 3], [f32; 3])>, QueryExecError> {
        let shapes = self.resolve_world_shapes(capture, detail, None)?;
        let mut bounds = Vec::new();
        for shape in shapes {
            if let Some((min, max)) = self.shape_support_bounds_world(&shape)? {
                bounds.push((shape, min, max));
            }
        }
        Ok(bounds)
    }

    fn field_support_children_bounds(
        &self,
        scene: &crate::scene_ir::FieldScene,
        children: &[SupportNodeId],
        merge: fn(SupportBounds, SupportBounds) -> SupportBounds,
        allow_partial: bool,
    ) -> Result<Option<SupportBounds>, QueryExecError> {
        let mut out = None;
        for child in children {
            match self.field_support_bounds(scene, *child)? {
                Some(bounds) => {
                    out = Some(match out {
                        Some(current) => merge(current, bounds),
                        None => bounds,
                    });
                }
                None if !allow_partial => return Ok(None),
                None => {}
            }
        }
        Ok(out)
    }

    fn shape_support_children_bounds(
        &self,
        scene: &crate::scene_ir::ShapeScene,
        children: &[SupportNodeId],
        merge: fn(SupportBounds, SupportBounds) -> SupportBounds,
        allow_partial: bool,
    ) -> Result<Option<SupportBounds>, QueryExecError> {
        let mut out = None;
        for child in children {
            match self.shape_support_bounds(scene, *child)? {
                Some(bounds) => {
                    out = Some(match out {
                        Some(current) => merge(current, bounds),
                        None => bounds,
                    });
                }
                None if !allow_partial => return Ok(None),
                None => {}
            }
        }
        Ok(out)
    }

    fn transform_support_bounds(
        &self,
        kind: TransformKind,
        param: Option<&SceneValueExpr>,
        bounds: SupportBounds,
    ) -> Result<Option<SupportBounds>, QueryExecError> {
        let Some(param) = param else {
            return Ok(Some(bounds));
        };
        let value = self.eval_scene_constant(param)?;
        match kind {
            TransformKind::Translate => {
                let offset = expect_vec3(Some(&value), "support translate")?;
                Ok(Some(SupportBounds {
                    min: add3(bounds.min, offset),
                    max: add3(bounds.max, offset),
                }))
            }
            TransformKind::UniformScale => {
                let scale = expect_f32(Some(&value), "support uniform scale")?;
                let scaled = SupportBounds {
                    min: mul3_scalar(bounds.min, scale),
                    max: mul3_scalar(bounds.max, scale),
                };
                Ok(Some(normalize_support_bounds(scaled)))
            }
            TransformKind::Rotate => {
                let rotation = eval_unary_value(UnaryOp::Neg, value.clone())?;
                let mut transformed = None;
                for corner in support_bounds_corners(bounds) {
                    let point = runtime_binary_value(
                        rotation.clone(),
                        KernelValue::Vec3(corner),
                        wr_rotate,
                    )?;
                    let point = expect_vec3(Some(&point), "support rotate corner")?;
                    let point_bounds = SupportBounds {
                        min: point,
                        max: point,
                    };
                    transformed = Some(match transformed {
                        Some(current) => merge_union_support_bounds(current, point_bounds),
                        None => point_bounds,
                    });
                }
                Ok(transformed)
            }
            TransformKind::AffineTransform => transform_value_support_bounds(&value, bounds),
            TransformKind::Warp
            | TransformKind::Bend
            | TransformKind::Twist
            | TransformKind::Taper
            | TransformKind::Displace => Ok(None),
        }
    }

    fn repeat_support_bounds(
        &self,
        kind: RepeatKind,
        param: Option<&SceneValueExpr>,
        bounds: SupportBounds,
    ) -> Result<Option<SupportBounds>, QueryExecError> {
        let Some(param) = param else {
            return Ok(Some(bounds));
        };
        let value = self.eval_scene_constant(param)?;
        match kind {
            RepeatKind::MirrorArray => {
                let normal = expect_vec3(Some(&value), "mirror array support normal")?;
                Ok(Some(merge_union_support_bounds(
                    bounds,
                    reflect_support_bounds(bounds, normal),
                )))
            }
            RepeatKind::InstanceArray => transform_value_support_bounds(&value, bounds),
            RepeatKind::RepeatLinear | RepeatKind::RepeatGrid | RepeatKind::RadialRepeat => {
                Ok(None)
            }
        }
    }

    fn eval_field_support_children(
        &self,
        scene: &crate::scene_ir::FieldScene,
        children: &[crate::scene_ir::SupportNodeId],
        point: [f32; 3],
        merge: fn(f32, f32) -> f32,
    ) -> Result<Option<f32>, QueryExecError> {
        let mut result = None;
        for child in children {
            let Some(value) = self.eval_field_support_record(scene, *child, point)? else {
                return Ok(None);
            };
            result = Some(match result {
                Some(current) => merge(current, value),
                None => value,
            });
        }
        Ok(result)
    }

    fn eval_shape_support_children(
        &self,
        scene: &crate::scene_ir::ShapeScene,
        children: &[crate::scene_ir::SupportNodeId],
        point: [f32; 3],
        merge: fn(f32, f32) -> f32,
    ) -> Result<Option<f32>, QueryExecError> {
        let mut result = None;
        for child in children {
            let Some(value) = self.eval_shape_support_record(scene, *child, point)? else {
                return Ok(None);
            };
            result = Some(match result {
                Some(current) => merge(current, value),
                None => value,
            });
        }
        Ok(result)
    }

    pub(crate) fn resolve_world_shapes(
        &self,
        capture: &SmolStr,
        detail: i32,
        root_shape_id: Option<u32>,
    ) -> Result<Vec<SmolStr>, QueryExecError> {
        self.note_artifact_load();
        let scene_id = self.ctx.region_scene_id(capture);
        let Some(region_case) = select_region_exec_case(&self.ctx.region_cases, scene_id) else {
            return Err(QueryExecError::MissingRegion {
                name: capture.clone(),
            });
        };
        region_case
            .shapes_for_detail(detail)
            .map(|shapes| match root_shape_id {
                Some(root_shape_id) => {
                    let selected = shapes
                        .iter()
                        .filter(|shape| self.ctx.shape_root_feature_id(shape) == root_shape_id)
                        .cloned()
                        .collect::<Vec<_>>();
                    self.note_support_pruned_candidates(
                        (shapes.len().saturating_sub(selected.len())) as u32,
                    );
                    selected
                }
                None => shapes.to_vec(),
            })
            .map_err(|message| QueryExecError::Unsupported {
                message: message.to_string(),
            })
    }

    pub(crate) fn eval_world_distance(
        &self,
        capture: &SmolStr,
        detail: i32,
        point: [f32; 3],
    ) -> Result<f32, QueryExecError> {
        let mut backend = CpuWorldDistanceBackend {
            evaluator: self,
            capture,
            detail,
            point,
            result: 1_000_000.0,
        };
        execute_world_distance(&mut backend)?;
        Ok(backend.result)
    }

    fn shape_leaf_from_winner(
        &self,
        shape: &SmolStr,
        feature_id: u32,
        leaf_ref: Option<&ShapeLeafRef>,
    ) -> Option<&crate::scene_ir::ShapeLeafScene> {
        leaf_ref
            .and_then(|leaf_ref| self.ctx.shape_leaf(&leaf_ref.scene, leaf_ref.leaf))
            .or_else(|| {
                self.ctx
                    .shape_leaf_ref(shape, feature_id)
                    .and_then(|leaf_ref| self.ctx.shape_leaf(&leaf_ref.scene, leaf_ref.leaf))
            })
    }

    pub(crate) fn eval_payload_body(
        &self,
        body: &hir::Body,
    ) -> Result<KernelValue, QueryExecError> {
        let mut scopes = vec![HashMap::new()];
        self.eval_portable_body_expr(body, &mut scopes)
    }

    fn eval_portable_body_expr(
        &self,
        body: &hir::Body,
        scopes: &mut Vec<HashMap<SmolStr, PortableVariable>>,
    ) -> Result<KernelValue, QueryExecError> {
        let (flow, last_value) =
            self.execute_portable_stmt_block(body, &body.root_stmts, scopes)?;
        match flow {
            PortableFlow::None => Ok(last_value),
            PortableFlow::Return(value) => Ok(value),
            PortableFlow::Break | PortableFlow::Continue => Err(QueryExecError::Unsupported {
                message: "loop control escaped a portable function body".to_string(),
            }),
        }
    }

    fn execute_portable_stmt_block(
        &self,
        body: &hir::Body,
        stmts: &[hir::Idx<hir::Stmt>],
        scopes: &mut Vec<HashMap<SmolStr, PortableVariable>>,
    ) -> Result<(PortableFlow, KernelValue), QueryExecError> {
        scopes.push(HashMap::new());
        let mut last_value = KernelValue::Nothing;
        for stmt in stmts {
            let (flow, value) = self.execute_portable_stmt(body, *stmt, scopes)?;
            if !matches!(flow, PortableFlow::None) {
                scopes.pop();
                return Ok((flow, value));
            }
            last_value = value;
        }
        scopes.pop();
        Ok((PortableFlow::None, last_value))
    }

    fn execute_portable_stmt(
        &self,
        body: &hir::Body,
        stmt_id: hir::Idx<hir::Stmt>,
        scopes: &mut Vec<HashMap<SmolStr, PortableVariable>>,
    ) -> Result<(PortableFlow, KernelValue), QueryExecError> {
        match &body.stmts[stmt_id] {
            hir::Stmt::Expr(expr) => Ok((
                PortableFlow::None,
                self.eval_portable_expr(body, *expr, scopes)?,
            )),
            hir::Stmt::IgnoreResult { expr } => {
                let _ = self.eval_portable_expr(body, *expr, scopes)?;
                Ok((PortableFlow::None, KernelValue::Nothing))
            }
            hir::Stmt::Let {
                name,
                value,
                mutable,
                ..
            } => {
                let value = self.eval_portable_expr(body, *value, scopes)?;
                scopes.last_mut().expect("portable scope").insert(
                    name.clone(),
                    PortableVariable {
                        value,
                        mutable: *mutable,
                    },
                );
                Ok((PortableFlow::None, KernelValue::Nothing))
            }
            hir::Stmt::Assign {
                name, op, value, ..
            } => {
                let value = self.eval_portable_expr(body, *value, scopes)?;
                self.assign_portable_local(name, *op, value, scopes)?;
                Ok((PortableFlow::None, KernelValue::Nothing))
            }
            hir::Stmt::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let condition = self.eval_portable_expr(body, *condition, scopes)?;
                match condition {
                    KernelValue::Bool(true) => {
                        self.execute_portable_stmt_block(body, then_branch, scopes)
                    }
                    KernelValue::Bool(false) => {
                        if let Some(else_branch) = else_branch {
                            self.execute_portable_stmt_block(body, else_branch, scopes)
                        } else {
                            Ok((PortableFlow::None, KernelValue::Nothing))
                        }
                    }
                    other => Err(QueryExecError::TypeMismatch {
                        expected: "Bool".to_string(),
                        found: value_label(&other),
                    }),
                }
            }
            hir::Stmt::While {
                condition,
                body: loop_body,
            } => {
                loop {
                    let condition = self.eval_portable_expr(body, *condition, scopes)?;
                    match condition {
                        KernelValue::Bool(true) => {
                            let (flow, _value) =
                                self.execute_portable_stmt_block(body, loop_body, scopes)?;
                            match flow {
                                PortableFlow::None | PortableFlow::Continue => {}
                                PortableFlow::Break => break,
                                PortableFlow::Return(value) => {
                                    return Ok((PortableFlow::Return(value.clone()), value));
                                }
                            }
                        }
                        KernelValue::Bool(false) => break,
                        other => {
                            return Err(QueryExecError::TypeMismatch {
                                expected: "Bool".to_string(),
                                found: value_label(&other),
                            });
                        }
                    }
                }
                Ok((PortableFlow::None, KernelValue::Nothing))
            }
            hir::Stmt::Return(Some(expr)) => {
                let value = self.eval_portable_expr(body, *expr, scopes)?;
                Ok((PortableFlow::Return(value.clone()), value))
            }
            hir::Stmt::Return(None) => Ok((
                PortableFlow::Return(KernelValue::Nothing),
                KernelValue::Nothing,
            )),
            hir::Stmt::Break => Ok((PortableFlow::Break, KernelValue::Nothing)),
            hir::Stmt::Continue => Ok((PortableFlow::Continue, KernelValue::Nothing)),
            other => Err(QueryExecError::Unsupported {
                message: format!(
                    "portable body statement '{other:?}' is not supported in query_exec::cpu"
                ),
            }),
        }
    }

    fn assign_portable_local(
        &self,
        name: &SmolStr,
        op: hir::AssignOp,
        value: KernelValue,
        scopes: &mut [HashMap<SmolStr, PortableVariable>],
    ) -> Result<(), QueryExecError> {
        for scope in scopes.iter_mut().rev() {
            if let Some(variable) = scope.get_mut(name) {
                if !variable.mutable {
                    return Err(QueryExecError::Unsupported {
                        message: format!("cannot assign to immutable local '{name}'"),
                    });
                }
                let next = match op {
                    hir::AssignOp::Assign => value,
                    hir::AssignOp::AddAssign => {
                        eval_binary_value(BinaryOp::Add, variable.value.clone(), value)?
                    }
                    hir::AssignOp::SubAssign => {
                        eval_binary_value(BinaryOp::Sub, variable.value.clone(), value)?
                    }
                    hir::AssignOp::MulAssign => {
                        eval_binary_value(BinaryOp::Mul, variable.value.clone(), value)?
                    }
                    hir::AssignOp::DivAssign => {
                        eval_binary_value(BinaryOp::Div, variable.value.clone(), value)?
                    }
                };
                variable.value = next;
                return Ok(());
            }
        }
        Err(QueryExecError::Unsupported {
            message: format!("portable body variable '{name}' is not available"),
        })
    }

    fn eval_portable_expr(
        &self,
        body: &hir::Body,
        expr_id: hir::Idx<Expr>,
        scopes: &[HashMap<SmolStr, PortableVariable>],
    ) -> Result<KernelValue, QueryExecError> {
        match &body.exprs[expr_id] {
            Expr::Literal(literal) => Ok(literal_to_kernel(literal)),
            Expr::Variable(name) => self
                .lookup_portable_local(name, scopes)
                .cloned()
                .ok_or_else(|| QueryExecError::Unsupported {
                    message: format!("portable body variable '{name}' is not available"),
                }),
            Expr::Unary { op, expr, .. } => {
                let value = self.eval_portable_expr(body, *expr, scopes)?;
                eval_unary_value(*op, value)
            }
            Expr::Binary { lhs, op, rhs, .. } => {
                let lhs = self.eval_portable_expr(body, *lhs, scopes)?;
                let rhs = self.eval_portable_expr(body, *rhs, scopes)?;
                eval_binary_value(*op, lhs, rhs)
            }
            Expr::Call {
                callee,
                args,
                type_args,
            } if type_args.is_empty() => {
                let Expr::Variable(name) = &body.exprs[*callee] else {
                    return Err(QueryExecError::Unsupported {
                        message: "portable body only supports named calls".to_string(),
                    });
                };
                let lowered = args
                    .iter()
                    .map(|arg| match arg {
                        hir::Arg::Positional { value, .. } | hir::Arg::Named { value, .. } => {
                            self.eval_portable_expr(body, *value, scopes)
                        }
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                self.eval_callable(name, lowered)
            }
            Expr::Member { object, member, .. } => {
                let base = self.eval_portable_expr(body, *object, scopes)?;
                eval_member_value(base, member)
            }
            Expr::Index { object, index, .. } => {
                let base = self.eval_portable_expr(body, *object, scopes)?;
                let index = self.eval_portable_expr(body, *index, scopes)?;
                eval_index_value(base, index)
            }
            Expr::List(items) => Ok(KernelValue::Array(
                items
                    .iter()
                    .map(|item| self.eval_portable_expr(body, *item, scopes))
                    .collect::<Result<Vec<_>, _>>()?,
            )),
            _ => Err(QueryExecError::Unsupported {
                message: "portable body expression is not supported in query_exec::cpu".to_string(),
            }),
        }
    }

    fn eval_scene_named_arg(
        &self,
        args: &[SceneArgExpr],
        name: &str,
    ) -> Result<KernelValue, QueryExecError> {
        self.eval_scene_named_arg_opt(args, name)?
            .ok_or_else(|| QueryExecError::Unsupported {
                message: format!("missing scene argument '{name}'"),
            })
    }

    fn eval_scene_named_arg_opt(
        &self,
        args: &[SceneArgExpr],
        name: &str,
    ) -> Result<Option<KernelValue>, QueryExecError> {
        args.iter()
            .find_map(|arg| match arg {
                SceneArgExpr::Named {
                    name: arg_name,
                    value,
                } if arg_name.as_str() == name => {
                    Some(self.eval_scene_value_expr(value, &HashMap::new()))
                }
                _ => None,
            })
            .transpose()
    }

    pub(crate) fn eval_scene_constant(
        &self,
        expr: &SceneValueExpr,
    ) -> Result<KernelValue, QueryExecError> {
        self.eval_scene_value_expr(expr, &HashMap::new())
    }

    fn eval_scene_value_expr(
        &self,
        expr: &SceneValueExpr,
        env: &HashMap<SmolStr, KernelValue>,
    ) -> Result<KernelValue, QueryExecError> {
        match expr {
            SceneValueExpr::Literal(literal) => Ok(literal_to_kernel(literal)),
            SceneValueExpr::List(items) => Ok(KernelValue::Array(
                items
                    .iter()
                    .map(|item| self.eval_scene_value_expr(item, env))
                    .collect::<Result<Vec<_>, _>>()?,
            )),
            SceneValueExpr::Unary { op, expr } => {
                let value = self.eval_scene_value_expr(expr, env)?;
                eval_unary_value(*op, value)
            }
            SceneValueExpr::Binary { lhs, op, rhs } => {
                let lhs = self.eval_scene_value_expr(lhs, env)?;
                let rhs = self.eval_scene_value_expr(rhs, env)?;
                eval_binary_value(*op, lhs, rhs)
            }
            SceneValueExpr::Call { callee, args } => {
                let lowered = args
                    .iter()
                    .map(|arg| match arg {
                        SceneArgExpr::Positional(value) | SceneArgExpr::Named { value, .. } => {
                            self.eval_scene_value_expr(value, env)
                        }
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                self.eval_callable(callee, lowered)
            }
        }
    }

    fn eval_callable(
        &self,
        name: &SmolStr,
        args: Vec<KernelValue>,
    ) -> Result<KernelValue, QueryExecError> {
        if let Some(builtin) = self.eval_builtin_or_value_constructor(name, &args)? {
            return Ok(builtin);
        }
        self.execute_portable_function(name, args)
    }

    fn eval_builtin_or_value_constructor(
        &self,
        name: &SmolStr,
        args: &[KernelValue],
    ) -> Result<Option<KernelValue>, QueryExecError> {
        if let Some(builtin) = eval_builtin_callable(name.as_str(), args)? {
            return Ok(Some(builtin));
        }
        if portable::builtin_record_is_constructible(name.as_str()) {
            let record = portable::builtin_record(name.as_str()).expect("constructible record");
            return Ok(Some(construct_builtin_record_value(record, args)?));
        }
        if let Some(field_names) = self.ctx.value_class_fields.get(name) {
            let fields = field_names
                .iter()
                .enumerate()
                .map(|(index, field)| {
                    let value =
                        args.get(index)
                            .cloned()
                            .ok_or_else(|| QueryExecError::Unsupported {
                                message: format!(
                                    "missing constructor arg {} for value '{}'",
                                    index, name
                                ),
                            })?;
                    Ok((field.clone(), value))
                })
                .collect::<Result<Vec<_>, QueryExecError>>()?;
            return Ok(Some(KernelValue::Struct(KernelStructValue {
                name: name.clone(),
                fields,
            })));
        }
        Ok(None)
    }

    pub(crate) fn execute_portable_function(
        &self,
        name: &SmolStr,
        args: Vec<KernelValue>,
    ) -> Result<KernelValue, QueryExecError> {
        let function = self
            .ctx
            .functions_by_name
            .get(name)
            .ok_or_else(|| QueryExecError::MissingFunction { name: name.clone() })?;
        if function.lane() != hir::FunctionLane::Portable {
            return Err(QueryExecError::Unsupported {
                message: format!(
                    "portable function execution cannot call non-portable function '{}'",
                    name
                ),
            });
        }
        let body = function
            .body
            .as_ref()
            .ok_or_else(|| QueryExecError::Unsupported {
                message: format!("portable function '{name}' does not have a body"),
            })?;
        if args.len() != function.params.len() {
            return Err(QueryExecError::Unsupported {
                message: format!(
                    "portable function '{}' expected {} arguments but received {}",
                    name,
                    function.params.len(),
                    args.len()
                ),
            });
        }
        let mut scopes = vec![HashMap::new()];
        for (param, value) in function.params.iter().zip(args) {
            scopes.last_mut().expect("portable scope").insert(
                param.name.clone(),
                PortableVariable {
                    value,
                    mutable: false,
                },
            );
        }
        self.eval_portable_body_expr(body, &mut scopes)
    }

    fn lookup_portable_local<'b>(
        &self,
        name: &SmolStr,
        scopes: &'b [HashMap<SmolStr, PortableVariable>],
    ) -> Option<&'b KernelValue> {
        scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).map(|variable| &variable.value))
    }
}

fn validation_error(label: &str, errors: Vec<KernelValidationError>) -> QueryExecError {
    QueryExecError::Unsupported {
        message: format!(
            "{label} failed contract validation: {}",
            errors
                .into_iter()
                .map(|error| error.message)
                .collect::<Vec<_>>()
                .join("; ")
        ),
    }
}

fn batch_kind_for_plan(plan: &KernelBatchQueryPlan) -> Result<BatchQueryKind, QueryExecError> {
    batch_query_kind_for_contract_id(plan.contract_id).ok_or_else(|| QueryExecError::Unsupported {
        message: format!(
            "missing batch query contract '{}'",
            plan.contract_id.as_str()
        ),
    })
}

fn build_world_batch_args(
    plan: &KernelWorldQueryPlan,
    capture: &KernelValue,
    domain: &KernelValue,
    item: &KernelValue,
) -> Result<Vec<KernelValue>, QueryExecError> {
    let mut args = vec![capture.clone(), domain.clone()];
    match world_kind_for_plan(plan)? {
        WorldQueryKind::Distance | WorldQueryKind::Normal | WorldQueryKind::Medium => {
            let point = expect_struct(Some(item), "PointQuery")?;
            args.push(KernelValue::Vec3(expect_struct_vec3(point, "point")?));
        }
        WorldQueryKind::Nearest | WorldQueryKind::Trace | WorldQueryKind::Occluded => {
            expect_struct(Some(item), "RayQuery")?;
            args.push(item.clone());
        }
        WorldQueryKind::Surface => {
            expect_struct(Some(item), "Hit3")?;
            args.push(item.clone());
        }
        WorldQueryKind::Radiance => {
            expect_struct(Some(item), "PointDirectionQuery")?;
            args.push(item.clone());
        }
        WorldQueryKind::SupportSummary => {}
    }
    Ok(args)
}

fn wrap_world_batch_result(
    plan: &KernelWorldQueryPlan,
    value: KernelValue,
) -> Result<KernelValue, QueryExecError> {
    match world_kind_for_plan(plan)? {
        WorldQueryKind::Distance => Ok(distance_result(expect_f32(Some(&value), "distance")?)),
        WorldQueryKind::Normal => Ok(normal_result(expect_vec3(Some(&value), "normal")?)),
        _ => Ok(value),
    }
}

fn world_kind_for_plan(plan: &KernelWorldQueryPlan) -> Result<WorldQueryKind, QueryExecError> {
    world_query_kind_for_contract_id(plan.contract_id).ok_or_else(|| QueryExecError::Unsupported {
        message: format!(
            "missing world query contract '{}'",
            plan.contract_id.as_str()
        ),
    })
}

impl CaptureQueryBackend for DirectQueryOps<'_> {
    fn resolve_field_or_shape_capture(
        &self,
        capture: Option<&KernelValue>,
    ) -> Result<SmolStr, QueryExecError> {
        DirectQueryOps::resolve_field_or_shape_capture(self, capture)
    }

    fn resolve_shape_capture(
        &self,
        capture: Option<&KernelValue>,
    ) -> Result<SmolStr, QueryExecError> {
        DirectQueryOps::resolve_shape_capture(self, capture)
    }

    fn capture_distance(
        &self,
        capture: &SmolStr,
        point: [f32; 3],
        capture_kind: crate::query_plan::CaptureKind,
    ) -> Result<f32, QueryExecError> {
        DirectQueryOps::eval_capture_distance(self, capture, point, capture_kind)
    }

    fn capture_normal(
        &self,
        capture: &SmolStr,
        point: [f32; 3],
        capture_kind: crate::query_plan::CaptureKind,
    ) -> Result<[f32; 3], QueryExecError> {
        DirectQueryOps::eval_capture_normal(self, capture, point, capture_kind)
    }

    fn support_summary(
        &self,
        capture: &SmolStr,
        capture_kind: crate::query_plan::CaptureKind,
    ) -> Result<KernelValue, QueryExecError> {
        DirectQueryOps::support_summary_for_capture(self, capture, capture_kind)
    }

    fn trace_shape(
        &self,
        shape: &SmolStr,
        origin: [f32; 3],
        direction: [f32; 3],
        max_distance: f32,
        min_step: f32,
        hit_epsilon: f32,
        max_steps: i32,
    ) -> Result<KernelValue, QueryExecError> {
        DirectQueryOps::trace_shape_impl(
            self,
            shape,
            origin,
            direction,
            max_distance,
            min_step,
            hit_epsilon,
            max_steps,
        )
    }

    fn surface_at(
        &self,
        shape: &SmolStr,
        hit: &KernelStructValue,
    ) -> Result<KernelValue, QueryExecError> {
        DirectQueryOps::surface_at(self, shape, hit)
    }

    fn radiance_at(
        &self,
        shape: &SmolStr,
        point: [f32; 3],
        direction: [f32; 3],
    ) -> Result<KernelValue, QueryExecError> {
        DirectQueryOps::radiance_at(self, shape, point, direction)
    }

    fn medium_at(&self, shape: &SmolStr, point: [f32; 3]) -> Result<KernelValue, QueryExecError> {
        DirectQueryOps::medium_at(self, shape, point)
    }
}

fn eval_builtin_callable(
    name: &str,
    args: &[KernelValue],
) -> Result<Option<KernelValue>, QueryExecError> {
    let value = match name {
        "i32" => Some(KernelValue::I32(expect_scalar_as_i32(args, "i32")?)),
        "u32" => Some(KernelValue::U32(expect_scalar_as_u32(args, "u32")?)),
        "f32" => Some(KernelValue::F32(expect_scalar_as_f32(args, "f32")?)),
        "vec2" => Some(KernelValue::Vec2([
            expect_scalar_as_f32_arg(args, 0, "vec2")?,
            expect_scalar_as_f32_arg(args, 1, "vec2")?,
        ])),
        "vec3" => Some(KernelValue::Vec3([
            expect_scalar_as_f32_arg(args, 0, "vec3")?,
            expect_scalar_as_f32_arg(args, 1, "vec3")?,
            expect_scalar_as_f32_arg(args, 2, "vec3")?,
        ])),
        "vec4" => Some(KernelValue::Vec4([
            expect_scalar_as_f32_arg(args, 0, "vec4")?,
            expect_scalar_as_f32_arg(args, 1, "vec4")?,
            expect_scalar_as_f32_arg(args, 2, "vec4")?,
            expect_scalar_as_f32_arg(args, 3, "vec4")?,
        ])),
        "quat" => Some(KernelValue::Quat([
            expect_scalar_as_f32_arg(args, 0, "quat")?,
            expect_scalar_as_f32_arg(args, 1, "quat")?,
            expect_scalar_as_f32_arg(args, 2, "quat")?,
            expect_scalar_as_f32_arg(args, 3, "quat")?,
        ])),
        "mat3_identity" => Some(KernelValue::Mat3([
            1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0,
        ])),
        "mat4_identity" => Some(KernelValue::Mat4(mat4_identity())),
        "mat3_cols" => Some(runtime_to_kernel_mat3(wr_mat3_from_columns(
            kernel_to_runtime(&args[0])?,
            kernel_to_runtime(&args[1])?,
            kernel_to_runtime(&args[2])?,
        ))?),
        "mat4_cols" => Some(runtime_to_kernel_mat4(wr_mat4_from_columns(
            kernel_to_runtime(&args[0])?,
            kernel_to_runtime(&args[1])?,
            kernel_to_runtime(&args[2])?,
            kernel_to_runtime(&args[3])?,
        ))?),
        "transform3_identity" => Some(transform3_identity_value()),
        "compose_transform3" => Some(compose_transform3_value(args)?),
        "inverse_transform3" => Some(inverse_transform3_value(args)?),
        "transform_normal" => Some(runtime_binary_builtin(args, wr_transform_normal)?),
        "field_sweep_coords" => Some(runtime_binary_builtin(args, wr_field_sweep_coords)?),
        "circle2" => Some(runtime_binary_builtin(args, wr_circle2)?),
        "rect2" => Some(runtime_binary_builtin(args, wr_rect2)?),
        "rounded_rect2" => Some(runtime_ternary_builtin(args, wr_rounded_rect2)?),
        "capsule2" => {
            let [point, a, b, radius] = args else {
                return Err(QueryExecError::Unsupported {
                    message: "capsule2 expects four arguments".to_string(),
                });
            };
            Some(runtime_to_kernel_value(wr_capsule2(
                kernel_to_runtime(point)?,
                kernel_to_runtime(a)?,
                kernel_to_runtime(b)?,
                kernel_to_runtime(radius)?,
            ))?)
        }
        "segment2" => Some(runtime_ternary_builtin(args, wr_segment2)?),
        "polygon2" => Some(runtime_binary_builtin(args, wr_polygon2)?),
        "polyline2" => Some(runtime_binary_builtin(args, wr_polyline2)?),
        "abs" => Some(unary_componentwise(args, "abs", |value| value.abs())?),
        "min" => Some(binary_componentwise(args, "min", |lhs, rhs| lhs.min(rhs))?),
        "max" => Some(binary_componentwise(args, "max", |lhs, rhs| lhs.max(rhs))?),
        "clamp" => Some(ternary_componentwise(args, "clamp", |value, lo, hi| {
            value.clamp(lo, hi)
        })?),
        "mix" => Some(ternary_componentwise(args, "mix", |a, b, t| {
            a + (b - a) * t
        })?),
        "sign" => Some(unary_componentwise(args, "sign", |value| {
            if value > 0.0 {
                1.0
            } else if value < 0.0 {
                -1.0
            } else {
                0.0
            }
        })?),
        "floor" => Some(unary_componentwise(args, "floor", |value| value.floor())?),
        "fract" => Some(unary_componentwise(args, "fract", |value| value.fract())?),
        "sin" => Some(unary_componentwise(args, "sin", |value| value.sin())?),
        "cos" => Some(unary_componentwise(args, "cos", |value| value.cos())?),
        "sqrt" => Some(unary_componentwise(args, "sqrt", |value| value.sqrt())?),
        "pow" => Some(binary_componentwise(args, "pow", |lhs, rhs| lhs.powf(rhs))?),
        "distance" => Some(distance_builtin(args)?),
        "dot" => Some(dot_builtin(args)?),
        "length" => Some(length_builtin(args)?),
        "normalize" => Some(normalize_builtin(args)?),
        "cross" => Some(cross_builtin(args)?),
        "reflect" => Some(reflect_builtin(args)?),
        other if portable::builtin_record_by_function(other).is_some() => {
            Some(construct_builtin_record(other, args)?)
        }
        _ => None,
    };
    Ok(value)
}

fn cpu_backend_with_world_shapes<B, F>(
    evaluator: &DirectQueryOps<'_>,
    capture: &SmolStr,
    detail: i32,
    root_shape_id: Option<u32>,
    backend: &mut B,
    mut emit_shapes: F,
) -> Result<(), QueryExecError>
where
    F: FnMut(&mut B, &[SmolStr]) -> Result<(), QueryExecError>,
{
    let shapes = evaluator.resolve_world_shapes(capture, detail, root_shape_id)?;
    emit_shapes(backend, &shapes)
}

fn cpu_backend_with_domain_flag<B, F>(
    evaluator: &DirectQueryOps<'_>,
    domain: &KernelStructValue,
    kind: WorldQueryKind,
    backend: &mut B,
    enabled: F,
) -> Result<(), QueryExecError>
where
    F: FnOnce(&mut B) -> Result<(), QueryExecError>,
{
    if evaluator.world_domain_flag_enabled(domain, kind)? {
        enabled(backend)?;
    }
    Ok(())
}

struct CpuWorldDistanceBackend<'a, 'ctx> {
    evaluator: &'a DirectQueryOps<'ctx>,
    capture: &'a SmolStr,
    detail: i32,
    point: [f32; 3],
    result: f32,
}

impl CpuWorldDistanceBackend<'_, '_> {
    fn traverse_world_hierarchically(
        &mut self,
        tree: &CpuAccelerationTree<SmolStr>,
    ) -> Result<(), QueryExecError> {
        let mut stack = vec![CpuPointTraversal {
            node_index: tree.root,
            lower_bound: f32::NEG_INFINITY,
        }];
        while let Some(current) = pop_best_point_traversal(&mut stack) {
            self.evaluator.note_acceleration_node_visit();
            if current.lower_bound > self.result {
                self.evaluator.note_acceleration_pruned_node();
                self.evaluator
                    .note_support_pruned_candidates(tree.leaf_count(current.node_index));
                continue;
            }
            let Some(node) = tree.node(current.node_index) else {
                continue;
            };
            if let Some(shape) = node.leaf.as_ref() {
                self.accumulate_world_distance_shape(shape)?;
                continue;
            }
            let mut pending = Vec::new();
            for child_index in tree.children_of(current.node_index) {
                let Some(child) = tree.node(*child_index) else {
                    continue;
                };
                let lower_bound = child
                    .bounds
                    .map(|bounds| support_box_lower_bound(bounds.min, bounds.max, self.point))
                    .transpose()?
                    .unwrap_or(f32::NEG_INFINITY);
                if lower_bound > self.result {
                    self.evaluator.note_acceleration_pruned_node();
                    self.evaluator
                        .note_support_pruned_candidates(child.leaf_count);
                    continue;
                }
                pending.push(CpuPointTraversal {
                    node_index: *child_index,
                    lower_bound,
                });
            }
            push_ordered_point_traversals(&mut stack, pending);
        }
        Ok(())
    }
}

impl WorldQueryBackend for CpuWorldDistanceBackend<'_, '_> {
    type Error = QueryExecError;

    fn with_world_shapes<F>(
        &mut self,
        _kind: WorldQueryKind,
        _invalid_message: &'static str,
        emit_shapes: F,
    ) -> Result<(), Self::Error>
    where
        F: FnMut(&mut Self, &[SmolStr]) -> Result<(), Self::Error>,
    {
        if let Some(tree) = self
            .evaluator
            .world_acceleration_tree(self.capture, self.detail)?
        {
            return self.traverse_world_hierarchically(&tree);
        }
        cpu_backend_with_world_shapes(
            self.evaluator,
            self.capture,
            self.detail,
            None,
            self,
            emit_shapes,
        )
    }

    fn with_domain_flag<F>(&mut self, _kind: WorldQueryKind, enabled: F) -> Result<(), Self::Error>
    where
        F: FnOnce(&mut Self) -> Result<(), Self::Error>,
    {
        enabled(self)
    }
}

impl WorldDistanceBackend for CpuWorldDistanceBackend<'_, '_> {
    type Error = QueryExecError;

    fn init_world_distance(&mut self) -> Result<(), Self::Error> {
        self.result = 1_000_000.0;
        Ok(())
    }

    fn accumulate_world_distance_shape(&mut self, shape: &SmolStr) -> Result<(), Self::Error> {
        if let Some(lower_bound) = self
            .evaluator
            .eval_shape_support_lower_bound(shape, self.point)?
            && lower_bound > self.result
        {
            self.evaluator.note_support_pruned_candidates(1);
            return Ok(());
        }
        self.evaluator.note_candidate_count(1);
        self.result = self
            .result
            .min(self.evaluator.eval_shape_distance(shape, self.point)?);
        Ok(())
    }
}

struct CpuWorldNormalBackend<'a, 'ctx> {
    evaluator: &'a DirectQueryOps<'ctx>,
    capture: &'a SmolStr,
    detail: i32,
    point: [f32; 3],
}

impl WorldNormalBackend for CpuWorldNormalBackend<'_, '_> {
    type Error = QueryExecError;
    type Point = [f32; 3];
    type Distance = f32;
    type Normal = [f32; 3];

    fn base_point(&mut self) -> Result<Self::Point, Self::Error> {
        Ok(self.point)
    }

    fn offset_point(
        &mut self,
        point: &Self::Point,
        axis: usize,
        delta: f32,
    ) -> Result<Self::Point, Self::Error> {
        let mut point = *point;
        point[axis] += delta;
        Ok(point)
    }

    fn sample_world_distance(&mut self, point: Self::Point) -> Result<Self::Distance, Self::Error> {
        self.evaluator
            .eval_world_distance(self.capture, self.detail, point)
    }

    fn subtract_distance(
        &mut self,
        positive: Self::Distance,
        negative: Self::Distance,
    ) -> Result<Self::Distance, Self::Error> {
        Ok(positive - negative)
    }

    fn compose_normal(
        &mut self,
        x: Self::Distance,
        y: Self::Distance,
        z: Self::Distance,
    ) -> Result<Self::Normal, Self::Error> {
        Ok([x, y, z])
    }

    fn normalize_normal(&mut self, normal: Self::Normal) -> Result<Self::Normal, Self::Error> {
        Ok(normalize3(normal))
    }

    fn certified_world_normal(
        &mut self,
    ) -> Result<Option<(Self::Normal, NormalRole)>, Self::Error> {
        self.evaluator
            .try_certified_world_normal(self.capture, self.detail, self.point)
            .map(|result| result.map(|evaluation| (evaluation.normal, evaluation.role)))
    }

    fn record_world_normal_role(&mut self, role: NormalRole) -> Result<(), Self::Error> {
        self.evaluator.note_normal_role(role);
        Ok(())
    }
}

struct CpuWorldTraceBackend<'a, 'ctx> {
    evaluator: &'a DirectQueryOps<'ctx>,
    capture: &'a SmolStr,
    detail: i32,
    origin: [f32; 3],
    direction: [f32; 3],
    max_distance: f32,
    min_step: f32,
    hit_epsilon: f32,
    max_steps: i32,
    solver_plan: &'a RaySolverPlan,
    artifact_contracts: &'a [ArtifactContract],
    result: KernelValue,
    best_distance: f32,
}

impl CpuWorldTraceBackend<'_, '_> {
    fn trace_world_hierarchically(
        &mut self,
        tree: &CpuAccelerationTree<SmolStr>,
    ) -> Result<(), QueryExecError> {
        let root_start_t = match tree.node(tree.root).and_then(|node| node.bounds) {
            Some(bounds) => {
                match ray_support_interval_for_bounds(bounds, self.origin, self.direction) {
                    RaySupportProbe::Rejected => {
                        self.evaluator.note_acceleration_pruned_node();
                        self.evaluator
                            .note_support_pruned_candidates(tree.leaf_count(tree.root));
                        self.evaluator.note_ray_support_interval_rejection();
                        return Ok(());
                    }
                    RaySupportProbe::Interval(interval) => {
                        if interval.end_t < 0.0 {
                            self.evaluator.note_acceleration_pruned_node();
                            self.evaluator
                                .note_support_pruned_candidates(tree.leaf_count(tree.root));
                            self.evaluator.note_ray_support_interval_rejection();
                            return Ok(());
                        }
                        if interval.start_t > 0.0 {
                            self.evaluator.note_ray_support_entry_jump();
                        }
                        interval.start_t.max(0.0)
                    }
                    RaySupportProbe::Unavailable => 0.0,
                }
            }
            None => 0.0,
        };
        let mut stack = vec![CpuRayTraversal {
            node_index: tree.root,
            start_t: root_start_t,
        }];
        while let Some(current) = pop_best_ray_traversal(&mut stack) {
            self.evaluator.note_acceleration_node_visit();
            if current.start_t > self.best_distance.min(self.max_distance) {
                self.evaluator.note_acceleration_pruned_node();
                self.evaluator
                    .note_support_pruned_candidates(tree.leaf_count(current.node_index));
                continue;
            }
            let Some(node) = tree.node(current.node_index) else {
                continue;
            };
            if let Some(shape) = node.leaf.as_ref() {
                let prune_distance = self.best_distance.min(self.max_distance);
                let mut start_travel = current.start_t;
                if node.bounds.is_none() {
                    match self.evaluator.shape_ray_support_probe_world(
                        shape,
                        self.origin,
                        self.direction,
                    )? {
                        RaySupportProbe::Unavailable => {}
                        RaySupportProbe::Rejected => {
                            self.evaluator.note_acceleration_pruned_node();
                            self.evaluator
                                .note_support_pruned_candidates(node.leaf_count);
                            self.evaluator.note_ray_support_interval_rejection();
                            continue;
                        }
                        RaySupportProbe::Interval(interval) => {
                            if interval.end_t < 0.0 || interval.start_t > prune_distance {
                                self.evaluator.note_acceleration_pruned_node();
                                self.evaluator
                                    .note_support_pruned_candidates(node.leaf_count);
                                self.evaluator.note_ray_support_interval_rejection();
                                continue;
                            }
                            if interval.start_t > start_travel {
                                self.evaluator.note_ray_support_entry_jump();
                            }
                            start_travel = start_travel.max(interval.start_t.max(0.0));
                        }
                    }
                }
                if start_travel > prune_distance {
                    self.evaluator.note_acceleration_pruned_node();
                    self.evaluator
                        .note_support_pruned_candidates(node.leaf_count);
                    continue;
                }
                self.evaluator.note_candidate_count(1);
                let hit = self.evaluator.solve_shape_ray(
                    self.solver_plan,
                    self.artifact_contracts,
                    shape,
                    self.origin,
                    self.direction,
                    start_travel,
                    self.max_distance,
                    self.min_step,
                    self.hit_epsilon,
                    self.max_steps,
                )?;
                let hit_ref = expect_struct_ref(&hit, "Hit3")?;
                if expect_struct_bool(hit_ref, "hit")? {
                    let distance = expect_struct_f32(hit_ref, "distance")?;
                    if distance < self.best_distance {
                        self.best_distance = distance;
                        self.result = hit;
                    }
                }
                continue;
            }

            let mut pending = Vec::new();
            for child_index in tree.children_of(current.node_index) {
                let Some(child) = tree.node(*child_index) else {
                    continue;
                };
                let start_t = match child.bounds {
                    Some(bounds) => {
                        match ray_support_interval_for_bounds(bounds, self.origin, self.direction) {
                            RaySupportProbe::Rejected => {
                                self.evaluator.note_acceleration_pruned_node();
                                self.evaluator
                                    .note_support_pruned_candidates(child.leaf_count);
                                self.evaluator.note_ray_support_interval_rejection();
                                continue;
                            }
                            RaySupportProbe::Interval(interval) => {
                                if interval.end_t < 0.0 {
                                    self.evaluator.note_acceleration_pruned_node();
                                    self.evaluator
                                        .note_support_pruned_candidates(child.leaf_count);
                                    self.evaluator.note_ray_support_interval_rejection();
                                    continue;
                                }
                                if interval.start_t > 0.0 {
                                    self.evaluator.note_ray_support_entry_jump();
                                }
                                interval.start_t.max(0.0)
                            }
                            RaySupportProbe::Unavailable => 0.0,
                        }
                    }
                    None => 0.0,
                };
                if start_t > self.best_distance.min(self.max_distance) {
                    self.evaluator.note_acceleration_pruned_node();
                    self.evaluator
                        .note_support_pruned_candidates(child.leaf_count);
                    continue;
                }
                pending.push(CpuRayTraversal {
                    node_index: *child_index,
                    start_t,
                });
            }
            push_ordered_ray_traversals(&mut stack, pending);
        }
        Ok(())
    }
}

impl WorldQueryBackend for CpuWorldTraceBackend<'_, '_> {
    type Error = QueryExecError;

    fn with_world_shapes<F>(
        &mut self,
        _kind: WorldQueryKind,
        _invalid_message: &'static str,
        emit_shapes: F,
    ) -> Result<(), Self::Error>
    where
        F: FnMut(&mut Self, &[SmolStr]) -> Result<(), Self::Error>,
    {
        if let Some(tree) = self
            .evaluator
            .world_acceleration_tree(self.capture, self.detail)?
        {
            return self.trace_world_hierarchically(&tree);
        }
        cpu_backend_with_world_shapes(
            self.evaluator,
            self.capture,
            self.detail,
            None,
            self,
            emit_shapes,
        )
    }

    fn with_domain_flag<F>(&mut self, _kind: WorldQueryKind, enabled: F) -> Result<(), Self::Error>
    where
        F: FnOnce(&mut Self) -> Result<(), Self::Error>,
    {
        enabled(self)
    }
}

impl WorldTraceBackend for CpuWorldTraceBackend<'_, '_> {
    type Error = QueryExecError;

    fn init_world_trace(&mut self) -> Result<(), Self::Error> {
        self.result = default_hit(self.origin);
        self.best_distance = f32::INFINITY;
        Ok(())
    }

    fn consider_world_trace_shape(&mut self, shape: &SmolStr) -> Result<(), Self::Error> {
        let prune_distance = self.best_distance.min(self.max_distance);
        let mut start_travel = 0.0f32;
        match self
            .evaluator
            .shape_ray_support_probe_world(shape, self.origin, self.direction)?
        {
            RaySupportProbe::Unavailable => {
                if let Some(lower_bound) = self
                    .evaluator
                    .eval_shape_support_lower_bound(shape, self.origin)?
                    && lower_bound > prune_distance
                {
                    self.evaluator.note_support_pruned_candidates(1);
                    self.evaluator.note_solver_support_rejection();
                    self.evaluator.note_ray_support_interval_rejection();
                    return Ok(());
                }
            }
            RaySupportProbe::Rejected => {
                self.evaluator.note_support_pruned_candidates(1);
                self.evaluator.note_solver_support_rejection();
                self.evaluator.note_ray_support_interval_rejection();
                return Ok(());
            }
            RaySupportProbe::Interval(interval) => {
                if interval.end_t < 0.0 || interval.start_t > prune_distance {
                    self.evaluator.note_support_pruned_candidates(1);
                    self.evaluator.note_solver_support_rejection();
                    self.evaluator.note_ray_support_interval_rejection();
                    return Ok(());
                }
                if interval.start_t > 0.0 {
                    self.evaluator.note_ray_support_entry_jump();
                    start_travel = interval.start_t;
                }
            }
        }
        self.evaluator.note_candidate_count(1);
        let hit = self.evaluator.solve_shape_ray(
            self.solver_plan,
            self.artifact_contracts,
            shape,
            self.origin,
            self.direction,
            start_travel,
            self.max_distance,
            self.min_step,
            self.hit_epsilon,
            self.max_steps,
        )?;
        let hit_ref = expect_struct_ref(&hit, "Hit3")?;
        if expect_struct_bool(hit_ref, "hit")? {
            let distance = expect_struct_f32(hit_ref, "distance")?;
            if distance < self.best_distance {
                self.best_distance = distance;
                self.result = hit;
            }
        }
        Ok(())
    }
}

struct CpuWorldSurfaceBackend<'a, 'ctx> {
    evaluator: &'a DirectQueryOps<'ctx>,
    capture: &'a SmolStr,
    detail: i32,
    domain: &'a KernelStructValue,
    hit: KernelStructValue,
    root_shape_id: u32,
    result: KernelValue,
}

impl WorldQueryBackend for CpuWorldSurfaceBackend<'_, '_> {
    type Error = QueryExecError;

    fn with_world_shapes<F>(
        &mut self,
        _kind: WorldQueryKind,
        _invalid_message: &'static str,
        emit_shapes: F,
    ) -> Result<(), Self::Error>
    where
        F: FnMut(&mut Self, &[SmolStr]) -> Result<(), Self::Error>,
    {
        cpu_backend_with_world_shapes(
            self.evaluator,
            self.capture,
            self.detail,
            Some(self.root_shape_id),
            self,
            emit_shapes,
        )
    }

    fn with_domain_flag<F>(&mut self, kind: WorldQueryKind, enabled: F) -> Result<(), Self::Error>
    where
        F: FnOnce(&mut Self) -> Result<(), Self::Error>,
    {
        cpu_backend_with_domain_flag(self.evaluator, self.domain, kind, self, enabled)
    }
}

impl WorldSurfaceBackend for CpuWorldSurfaceBackend<'_, '_> {
    type Error = QueryExecError;

    fn init_world_surface(&mut self) -> Result<(), Self::Error> {
        self.result = default_surface();
        Ok(())
    }

    fn consider_world_surface_shape(&mut self, shape: &SmolStr) -> Result<(), Self::Error> {
        self.evaluator.note_candidate_count(1);
        if self.evaluator.ctx.shape_root_feature_id(shape) == self.root_shape_id {
            self.result = self.evaluator.surface_at(shape, &self.hit)?;
        }
        Ok(())
    }
}

struct CpuWorldRadianceBackend<'a, 'ctx> {
    evaluator: &'a DirectQueryOps<'ctx>,
    capture: &'a SmolStr,
    detail: i32,
    domain: &'a KernelStructValue,
    point: [f32; 3],
    direction: [f32; 3],
    result: [f32; 3],
}

impl WorldQueryBackend for CpuWorldRadianceBackend<'_, '_> {
    type Error = QueryExecError;

    fn with_world_shapes<F>(
        &mut self,
        _kind: WorldQueryKind,
        _invalid_message: &'static str,
        emit_shapes: F,
    ) -> Result<(), Self::Error>
    where
        F: FnMut(&mut Self, &[SmolStr]) -> Result<(), Self::Error>,
    {
        cpu_backend_with_world_shapes(
            self.evaluator,
            self.capture,
            self.detail,
            None,
            self,
            emit_shapes,
        )
    }

    fn with_domain_flag<F>(&mut self, kind: WorldQueryKind, enabled: F) -> Result<(), Self::Error>
    where
        F: FnOnce(&mut Self) -> Result<(), Self::Error>,
    {
        cpu_backend_with_domain_flag(self.evaluator, self.domain, kind, self, enabled)
    }
}

impl WorldRadianceBackend for CpuWorldRadianceBackend<'_, '_> {
    type Error = QueryExecError;

    fn init_world_radiance(&mut self) -> Result<(), Self::Error> {
        self.result = [0.0, 0.0, 0.0];
        Ok(())
    }

    fn accumulate_world_radiance_shape(&mut self, shape: &SmolStr) -> Result<(), Self::Error> {
        self.evaluator.note_candidate_count(1);
        let KernelValue::Vec3(next) =
            self.evaluator
                .radiance_at(shape, self.point, self.direction)?
        else {
            return Ok(());
        };
        self.result = [
            self.result[0] + next[0],
            self.result[1] + next[1],
            self.result[2] + next[2],
        ];
        Ok(())
    }
}

struct CpuWorldMediumBackend<'a, 'ctx> {
    evaluator: &'a DirectQueryOps<'ctx>,
    capture: &'a SmolStr,
    detail: i32,
    domain: &'a KernelStructValue,
    point: [f32; 3],
    density: f32,
    emission: [f32; 3],
    anisotropy: f32,
}

impl WorldQueryBackend for CpuWorldMediumBackend<'_, '_> {
    type Error = QueryExecError;

    fn with_world_shapes<F>(
        &mut self,
        _kind: WorldQueryKind,
        _invalid_message: &'static str,
        emit_shapes: F,
    ) -> Result<(), Self::Error>
    where
        F: FnMut(&mut Self, &[SmolStr]) -> Result<(), Self::Error>,
    {
        cpu_backend_with_world_shapes(
            self.evaluator,
            self.capture,
            self.detail,
            None,
            self,
            emit_shapes,
        )
    }

    fn with_domain_flag<F>(&mut self, kind: WorldQueryKind, enabled: F) -> Result<(), Self::Error>
    where
        F: FnOnce(&mut Self) -> Result<(), Self::Error>,
    {
        cpu_backend_with_domain_flag(self.evaluator, self.domain, kind, self, enabled)
    }
}

impl WorldMediumBackend for CpuWorldMediumBackend<'_, '_> {
    type Error = QueryExecError;

    fn init_world_medium(&mut self) -> Result<(), Self::Error> {
        self.density = 0.0;
        self.emission = [0.0, 0.0, 0.0];
        self.anisotropy = 0.0;
        Ok(())
    }

    fn accumulate_world_medium_shape(&mut self, shape: &SmolStr) -> Result<(), Self::Error> {
        self.evaluator.note_candidate_count(1);
        let KernelValue::Struct(next) = self.evaluator.medium_at(shape, self.point)? else {
            return Ok(());
        };
        let next_density = expect_struct_f32(&next, "density")?;
        let next_emission = expect_struct_vec3(&next, "emission")?;
        let next_anisotropy = expect_struct_f32(&next, "anisotropy")?;
        let density = self.density + next_density;
        let anisotropy = if density > 0.0 {
            (self.anisotropy * self.density + next_anisotropy * next_density) / density
        } else {
            0.0
        };
        self.density = density;
        self.emission = [
            self.emission[0] + next_emission[0],
            self.emission[1] + next_emission[1],
            self.emission[2] + next_emission[2],
        ];
        self.anisotropy = anisotropy;
        Ok(())
    }
}

fn construct_builtin_record(
    name: &str,
    args: &[KernelValue],
) -> Result<KernelValue, QueryExecError> {
    let Some(record) = portable::builtin_record_by_function(name) else {
        return Err(QueryExecError::Unsupported {
            message: format!("unknown builtin record constructor '{name}'"),
        });
    };
    construct_builtin_record_value(record, args)
}

fn default_actor_handle() -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("ActorHandle"),
        fields: vec![
            (SmolStr::new("id"), KernelValue::U32(0)),
            (SmolStr::new("generation"), KernelValue::U32(0)),
        ],
    })
}

pub(crate) fn default_payload() -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("Payload"),
        fields: vec![
            (SmolStr::new("entity_id"), KernelValue::U32(0)),
            (SmolStr::new("material_id"), KernelValue::U32(0)),
            (SmolStr::new("actor"), default_actor_handle()),
        ],
    })
}

pub(crate) fn default_surface() -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("Surface"),
        fields: vec![
            (SmolStr::new("albedo"), KernelValue::Vec3([0.0, 0.0, 0.0])),
            (SmolStr::new("roughness"), KernelValue::F32(0.0)),
            (SmolStr::new("metalness"), KernelValue::F32(0.0)),
            (SmolStr::new("clearcoat"), KernelValue::F32(0.0)),
            (SmolStr::new("clearcoat_roughness"), KernelValue::F32(0.0)),
            (SmolStr::new("sheen"), KernelValue::F32(0.0)),
            (SmolStr::new("emissive"), KernelValue::Vec3([0.0, 0.0, 0.0])),
        ],
    })
}

pub(crate) fn default_medium() -> KernelValue {
    medium_value(0.0, [0.0, 0.0, 0.0], 0.0)
}

fn default_builtin_record_value(name: &str) -> Result<KernelValue, QueryExecError> {
    Ok(match name {
        "ActorHandle" => default_actor_handle(),
        "Payload" => default_payload(),
        "Surface" => default_surface(),
        "Medium" => default_medium(),
        "Transform3" => transform3_identity_value(),
        "Hit3" => default_hit([0.0, 0.0, 0.0]),
        other => {
            let record =
                portable::builtin_record(other).ok_or_else(|| QueryExecError::Unsupported {
                    message: format!("unknown builtin record constructor '{other}'"),
                })?;
            let fields = record
                .fields
                .iter()
                .map(|field| {
                    Ok((
                        SmolStr::new(field.name),
                        default_builtin_field_value(field.ty)?,
                    ))
                })
                .collect::<Result<Vec<_>, QueryExecError>>()?;
            KernelValue::Struct(KernelStructValue {
                name: SmolStr::new(record.name),
                fields,
            })
        }
    })
}

fn default_builtin_field_value(
    ty: portable::PortableBuiltinType,
) -> Result<KernelValue, QueryExecError> {
    use portable::PortableBuiltinAtom as Atom;
    use portable::PortableBuiltinType::{Atom as BuiltinAtom, Named as BuiltinNamed};

    match ty {
        BuiltinAtom(Atom::Bool) => Ok(KernelValue::Bool(false)),
        BuiltinAtom(Atom::I32) => Ok(KernelValue::I32(0)),
        BuiltinAtom(Atom::U32) => Ok(KernelValue::U32(0)),
        BuiltinAtom(Atom::F32) => Ok(KernelValue::F32(0.0)),
        BuiltinAtom(Atom::Vec2) => Ok(KernelValue::Vec2([0.0, 0.0])),
        BuiltinAtom(Atom::Vec3) => Ok(KernelValue::Vec3([0.0, 0.0, 0.0])),
        BuiltinAtom(Atom::Vec4) => Ok(KernelValue::Vec4([0.0, 0.0, 0.0, 0.0])),
        BuiltinAtom(Atom::Quat) => Ok(KernelValue::Quat([0.0, 0.0, 0.0, 0.0])),
        BuiltinAtom(Atom::Mat3) => Ok(KernelValue::Mat3([0.0; 9])),
        BuiltinAtom(Atom::Mat4) => Ok(KernelValue::Mat4([0.0; 16])),
        BuiltinNamed(name) => default_builtin_record_value(name),
    }
}

fn construct_builtin_record_value(
    record: &portable::PortableBuiltinRecord,
    args: &[KernelValue],
) -> Result<KernelValue, QueryExecError> {
    let fields = record
        .fields
        .iter()
        .enumerate()
        .map(|(index, field)| {
            Ok((
                SmolStr::new(field.name),
                match args.get(index) {
                    Some(value) => value.clone(),
                    None => default_builtin_field_value(field.ty)?,
                },
            ))
        })
        .collect::<Result<Vec<_>, QueryExecError>>()?;
    Ok(KernelValue::Struct(KernelStructValue {
        name: SmolStr::new(record.name),
        fields,
    }))
}

pub(crate) fn medium_value(density: f32, emission: [f32; 3], anisotropy: f32) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("Medium"),
        fields: vec![
            (SmolStr::new("density"), KernelValue::F32(density)),
            (SmolStr::new("emission"), KernelValue::Vec3(emission)),
            (SmolStr::new("anisotropy"), KernelValue::F32(anisotropy)),
        ],
    })
}

fn polygon_profile_distance(
    point: [f32; 2],
    vertices: &KernelValue,
    closed: bool,
) -> Result<f32, QueryExecError> {
    let KernelValue::Array(items) = vertices else {
        return Err(QueryExecError::TypeMismatch {
            expected: "Array<Vec2>".to_string(),
            found: format!("{vertices:?}"),
        });
    };
    let vertices = items
        .iter()
        .map(|value| match value {
            KernelValue::Vec2(value) => Ok(*value),
            other => Err(QueryExecError::TypeMismatch {
                expected: "Vec2".to_string(),
                found: format!("{other:?}"),
            }),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let min_len = if closed { 3 } else { 2 };
    if vertices.len() < min_len {
        return Err(QueryExecError::Unsupported {
            message: format!(
                "{} expects at least {min_len} vertices",
                if closed { "polygon2" } else { "polyline2" }
            ),
        });
    }
    let mut best = f32::INFINITY;
    if closed {
        let mut inside = false;
        for index in 0..vertices.len() {
            let a = vertices[index];
            let b = vertices[(index + 1) % vertices.len()];
            best = best.min(segment_distance_2d(point, a, b));
            let crosses = ((a[1] > point[1]) != (b[1] > point[1]))
                && (point[0]
                    < (b[0] - a[0]) * (point[1] - a[1]) / (b[1] - a[1] + f32::EPSILON) + a[0]);
            if crosses {
                inside = !inside;
            }
        }
        Ok(if inside { -best } else { best })
    } else {
        for pair in vertices.windows(2) {
            best = best.min(segment_distance_2d(point, pair[0], pair[1]));
        }
        Ok(best)
    }
}

fn segment_distance_2d(point: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let edge = [b[0] - a[0], b[1] - a[1]];
    let ap = [point[0] - a[0], point[1] - a[1]];
    let denom = edge[0] * edge[0] + edge[1] * edge[1];
    let t = if denom == 0.0 {
        0.0
    } else {
        ((ap[0] * edge[0] + ap[1] * edge[1]) / denom).clamp(0.0, 1.0)
    };
    let closest = [a[0] + edge[0] * t, a[1] + edge[1] * t];
    let delta = [point[0] - closest[0], point[1] - closest[1]];
    (delta[0] * delta[0] + delta[1] * delta[1]).sqrt()
}

fn distance_result(distance: f32) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("DistanceResult"),
        fields: vec![(SmolStr::new("distance"), KernelValue::F32(distance))],
    })
}

fn normal_result(normal: [f32; 3]) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("NormalResult"),
        fields: vec![(SmolStr::new("normal"), KernelValue::Vec3(normal))],
    })
}

fn occlusion_result(occluded: bool, distance: f32, steps: i32) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("OcclusionResult"),
        fields: vec![
            (SmolStr::new("occluded"), KernelValue::Bool(occluded)),
            (SmolStr::new("distance"), KernelValue::F32(distance)),
            (SmolStr::new("steps"), KernelValue::I32(steps)),
        ],
    })
}

fn transform3_identity_value() -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("Transform3"),
        fields: vec![
            (SmolStr::new("matrix"), KernelValue::Mat4(mat4_identity())),
            (SmolStr::new("inverse"), KernelValue::Mat4(mat4_identity())),
        ],
    })
}

pub(crate) fn hit_value(
    hit: bool,
    distance: f32,
    position: [f32; 3],
    normal: [f32; 3],
    local_position: [f32; 3],
    local_normal: [f32; 3],
    steps: i32,
    feature_id: u32,
    instance_id: u32,
    repeat_id: u32,
    root_shape_id: u32,
    payload: KernelValue,
) -> KernelValue {
    let shading_frame = stable_surface_frame(position, normal);
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("Hit3"),
        fields: vec![
            (SmolStr::new("hit"), KernelValue::Bool(hit)),
            (SmolStr::new("distance"), KernelValue::F32(distance)),
            (SmolStr::new("position"), KernelValue::Vec3(position)),
            (SmolStr::new("normal"), KernelValue::Vec3(normal)),
            (
                SmolStr::new("local_position"),
                KernelValue::Vec3(local_position),
            ),
            (
                SmolStr::new("local_normal"),
                KernelValue::Vec3(local_normal),
            ),
            (SmolStr::new("shading_frame"), shading_frame),
            (SmolStr::new("steps"), KernelValue::I32(steps)),
            (SmolStr::new("feature_id"), KernelValue::U32(feature_id)),
            (SmolStr::new("instance_id"), KernelValue::U32(instance_id)),
            (SmolStr::new("repeat_id"), KernelValue::U32(repeat_id)),
            (
                SmolStr::new("root_shape_id"),
                KernelValue::U32(root_shape_id),
            ),
            (SmolStr::new("payload"), payload),
        ],
    })
}

pub(crate) fn default_hit(origin: [f32; 3]) -> KernelValue {
    hit_value(
        false,
        0.0,
        origin,
        [0.0, 0.0, 1.0],
        origin,
        [0.0, 0.0, 1.0],
        0,
        0,
        0,
        0,
        0,
        default_payload(),
    )
}

fn mat4_identity() -> [f32; 16] {
    [
        1.0, 0.0, 0.0, 0.0, //
        0.0, 1.0, 0.0, 0.0, //
        0.0, 0.0, 1.0, 0.0, //
        0.0, 0.0, 0.0, 1.0,
    ]
}

fn compose_transform3_value(args: &[KernelValue]) -> Result<KernelValue, QueryExecError> {
    let [left, right] = args else {
        return Err(QueryExecError::Unsupported {
            message: "compose_transform3 expects two arguments".to_string(),
        });
    };
    let left = expect_struct_ref(left, "Transform3")?;
    let right = expect_struct_ref(right, "Transform3")?;
    let left_matrix = expect_struct_mat4(left, "matrix")?;
    let left_inverse = expect_struct_mat4(left, "inverse")?;
    let right_matrix = expect_struct_mat4(right, "matrix")?;
    let right_inverse = expect_struct_mat4(right, "inverse")?;
    Ok(KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("Transform3"),
        fields: vec![
            (
                SmolStr::new("matrix"),
                KernelValue::Mat4(mul_mat4(left_matrix, right_matrix)),
            ),
            (
                SmolStr::new("inverse"),
                KernelValue::Mat4(mul_mat4(right_inverse, left_inverse)),
            ),
        ],
    }))
}

fn inverse_transform3_value(args: &[KernelValue]) -> Result<KernelValue, QueryExecError> {
    let [transform] = args else {
        return Err(QueryExecError::Unsupported {
            message: "inverse_transform3 expects one argument".to_string(),
        });
    };
    let transform = expect_struct_ref(transform, "Transform3")?;
    Ok(KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("Transform3"),
        fields: vec![
            (
                SmolStr::new("matrix"),
                KernelValue::Mat4(expect_struct_mat4(transform, "inverse")?),
            ),
            (
                SmolStr::new("inverse"),
                KernelValue::Mat4(expect_struct_mat4(transform, "matrix")?),
            ),
        ],
    }))
}

fn mul_mat4(left: [f32; 16], right: [f32; 16]) -> [f32; 16] {
    let mut out = [0.0; 16];
    for column in 0..4 {
        for row in 0..4 {
            out[column * 4 + row] = left[row] * right[column * 4]
                + left[4 + row] * right[column * 4 + 1]
                + left[8 + row] * right[column * 4 + 2]
                + left[12 + row] * right[column * 4 + 3];
        }
    }
    out
}

fn unary_componentwise(
    args: &[KernelValue],
    name: &str,
    f: impl Fn(f32) -> f32,
) -> Result<KernelValue, QueryExecError> {
    let [value] = args else {
        return Err(QueryExecError::Unsupported {
            message: format!("{name} expects one argument"),
        });
    };
    map_components(value, name, |value, _| f(value))
}

fn binary_componentwise(
    args: &[KernelValue],
    name: &str,
    f: impl Fn(f32, f32) -> f32,
) -> Result<KernelValue, QueryExecError> {
    let [lhs, rhs] = args else {
        return Err(QueryExecError::Unsupported {
            message: format!("{name} expects two arguments"),
        });
    };
    map_pair_components(lhs, rhs, name, |lhs, rhs, _| f(lhs, rhs))
}

fn ternary_componentwise(
    args: &[KernelValue],
    name: &str,
    f: impl Fn(f32, f32, f32) -> f32,
) -> Result<KernelValue, QueryExecError> {
    let [a, b, c] = args else {
        return Err(QueryExecError::Unsupported {
            message: format!("{name} expects three arguments"),
        });
    };
    map_triple_components(a, b, c, name, |a, b, c, _| f(a, b, c))
}

fn distance_builtin(args: &[KernelValue]) -> Result<KernelValue, QueryExecError> {
    let [lhs, rhs] = args else {
        return Err(QueryExecError::Unsupported {
            message: "distance expects two arguments".to_string(),
        });
    };
    let lhs = kernel_components(lhs, "distance")?;
    let rhs = broadcast_components(rhs, lhs.len(), "distance")?;
    let sum = lhs
        .iter()
        .zip(rhs.iter())
        .map(|(lhs, rhs)| {
            let delta = lhs - rhs;
            delta * delta
        })
        .sum::<f32>();
    Ok(KernelValue::F32(sum.sqrt()))
}

fn dot_builtin(args: &[KernelValue]) -> Result<KernelValue, QueryExecError> {
    let [lhs, rhs] = args else {
        return Err(QueryExecError::Unsupported {
            message: "dot expects two arguments".to_string(),
        });
    };
    let lhs = kernel_components(lhs, "dot")?;
    let rhs = broadcast_components(rhs, lhs.len(), "dot")?;
    Ok(KernelValue::F32(
        lhs.iter().zip(rhs.iter()).map(|(lhs, rhs)| lhs * rhs).sum(),
    ))
}

fn length_builtin(args: &[KernelValue]) -> Result<KernelValue, QueryExecError> {
    let [value] = args else {
        return Err(QueryExecError::Unsupported {
            message: "length expects one argument".to_string(),
        });
    };
    let components = kernel_components(value, "length")?;
    let len_sq = components
        .iter()
        .map(|component| component * component)
        .sum::<f32>();
    Ok(KernelValue::F32(len_sq.sqrt()))
}

fn normalize_builtin(args: &[KernelValue]) -> Result<KernelValue, QueryExecError> {
    let [value] = args else {
        return Err(QueryExecError::Unsupported {
            message: "normalize expects one argument".to_string(),
        });
    };
    let components = kernel_components(value, "normalize")?;
    let len_sq = components
        .iter()
        .map(|component| component * component)
        .sum::<f32>();
    if len_sq == 0.0 {
        return same_kind_from_components(value, &vec![0.0; components.len()], "normalize");
    }
    let len = len_sq.sqrt();
    let normalized = components
        .into_iter()
        .map(|component| component / len)
        .collect::<Vec<_>>();
    same_kind_from_components(value, &normalized, "normalize")
}

fn cross_builtin(args: &[KernelValue]) -> Result<KernelValue, QueryExecError> {
    let [lhs, rhs] = args else {
        return Err(QueryExecError::Unsupported {
            message: "cross expects two arguments".to_string(),
        });
    };
    let lhs = expect_vec3_like(lhs, "cross")?;
    let rhs = expect_vec3_like(rhs, "cross")?;
    Ok(KernelValue::Vec3([
        lhs[1] * rhs[2] - lhs[2] * rhs[1],
        lhs[2] * rhs[0] - lhs[0] * rhs[2],
        lhs[0] * rhs[1] - lhs[1] * rhs[0],
    ]))
}

fn reflect_builtin(args: &[KernelValue]) -> Result<KernelValue, QueryExecError> {
    let [incident, normal] = args else {
        return Err(QueryExecError::Unsupported {
            message: "reflect expects two arguments".to_string(),
        });
    };
    let incident_components = kernel_components(incident, "reflect")?;
    let normal_components = broadcast_components(normal, incident_components.len(), "reflect")?;
    let dot = incident_components
        .iter()
        .zip(normal_components.iter())
        .map(|(lhs, rhs)| lhs * rhs)
        .sum::<f32>();
    let reflected = incident_components
        .iter()
        .zip(normal_components.iter())
        .map(|(incident, normal)| incident - 2.0 * dot * normal)
        .collect::<Vec<_>>();
    same_kind_from_components(incident, &reflected, "reflect")
}

fn map_components(
    value: &KernelValue,
    name: &str,
    f: impl Fn(f32, usize) -> f32,
) -> Result<KernelValue, QueryExecError> {
    let components = kernel_components(value, name)?;
    let mapped = components
        .iter()
        .enumerate()
        .map(|(index, value)| f(*value, index))
        .collect::<Vec<_>>();
    same_kind_from_components(value, &mapped, name)
}

fn map_pair_components(
    lhs: &KernelValue,
    rhs: &KernelValue,
    name: &str,
    f: impl Fn(f32, f32, usize) -> f32,
) -> Result<KernelValue, QueryExecError> {
    let lhs_components = kernel_components(lhs, name)?;
    let rhs_components = broadcast_components(rhs, lhs_components.len(), name)?;
    let mapped = lhs_components
        .iter()
        .zip(rhs_components.iter())
        .enumerate()
        .map(|(index, (lhs, rhs))| f(*lhs, *rhs, index))
        .collect::<Vec<_>>();
    same_kind_from_components(lhs, &mapped, name)
}

fn map_triple_components(
    a: &KernelValue,
    b: &KernelValue,
    c: &KernelValue,
    name: &str,
    f: impl Fn(f32, f32, f32, usize) -> f32,
) -> Result<KernelValue, QueryExecError> {
    let a_components = kernel_components(a, name)?;
    let b_components = broadcast_components(b, a_components.len(), name)?;
    let c_components = broadcast_components(c, a_components.len(), name)?;
    let mapped = a_components
        .iter()
        .zip(b_components.iter())
        .zip(c_components.iter())
        .enumerate()
        .map(|(index, ((a, b), c))| f(*a, *b, *c, index))
        .collect::<Vec<_>>();
    same_kind_from_components(a, &mapped, name)
}

fn kernel_components(value: &KernelValue, name: &str) -> Result<Vec<f32>, QueryExecError> {
    match value {
        KernelValue::I32(value) => Ok(vec![*value as f32]),
        KernelValue::U32(value) => Ok(vec![*value as f32]),
        KernelValue::F32(value) => Ok(vec![*value]),
        KernelValue::Vec2(value) => Ok(value.to_vec()),
        KernelValue::Vec3(value) => Ok(value.to_vec()),
        KernelValue::Vec4(value) | KernelValue::Quat(value) => Ok(value.to_vec()),
        other => Err(QueryExecError::TypeMismatch {
            expected: format!("{name}: scalar or vector"),
            found: format!("{other:?}"),
        }),
    }
}

fn broadcast_components(
    value: &KernelValue,
    target_len: usize,
    name: &str,
) -> Result<Vec<f32>, QueryExecError> {
    let components = kernel_components(value, name)?;
    if components.len() == target_len {
        return Ok(components);
    }
    if components.len() == 1 {
        return Ok(vec![components[0]; target_len]);
    }
    Err(QueryExecError::TypeMismatch {
        expected: format!("{name}: broadcastable to {target_len} lanes"),
        found: format!("{value:?}"),
    })
}

fn same_kind_from_components(
    prototype: &KernelValue,
    components: &[f32],
    name: &str,
) -> Result<KernelValue, QueryExecError> {
    match prototype {
        KernelValue::I32(_) => Ok(KernelValue::I32(components[0] as i32)),
        KernelValue::U32(_) => Ok(KernelValue::U32(components[0].max(0.0) as u32)),
        KernelValue::F32(_) => Ok(KernelValue::F32(components[0])),
        KernelValue::Vec2(_) => Ok(KernelValue::Vec2([components[0], components[1]])),
        KernelValue::Vec3(_) => Ok(KernelValue::Vec3([
            components[0],
            components[1],
            components[2],
        ])),
        KernelValue::Vec4(_) => Ok(KernelValue::Vec4([
            components[0],
            components[1],
            components[2],
            components[3],
        ])),
        KernelValue::Quat(_) => Ok(KernelValue::Quat([
            components[0],
            components[1],
            components[2],
            components[3],
        ])),
        other => Err(QueryExecError::TypeMismatch {
            expected: format!("{name}: scalar or vector"),
            found: format!("{other:?}"),
        }),
    }
}

fn expect_vec3_like(value: &KernelValue, name: &str) -> Result<[f32; 3], QueryExecError> {
    match value {
        KernelValue::Vec3(value) => Ok(*value),
        other => Err(QueryExecError::TypeMismatch {
            expected: format!("{name}: Vec3"),
            found: format!("{other:?}"),
        }),
    }
}

fn literal_to_kernel(literal: &Literal) -> KernelValue {
    match literal {
        Literal::Integer(value) => KernelValue::I32(*value as i32),
        Literal::Float(value) => KernelValue::F32(*value as f32),
        Literal::Boolean(value) => KernelValue::Bool(*value),
        Literal::Nil => KernelValue::Nothing,
        Literal::String(_) => KernelValue::Nothing,
    }
}

fn eval_unary_value(op: UnaryOp, value: KernelValue) -> Result<KernelValue, QueryExecError> {
    match (op, value) {
        (UnaryOp::Neg, KernelValue::I32(value)) => Ok(KernelValue::I32(-value)),
        (UnaryOp::Neg, KernelValue::F32(value)) => Ok(KernelValue::F32(-value)),
        (UnaryOp::Neg, KernelValue::Vec2([x, y])) => Ok(KernelValue::Vec2([-x, -y])),
        (UnaryOp::Neg, KernelValue::Vec3([x, y, z])) => Ok(KernelValue::Vec3([-x, -y, -z])),
        (UnaryOp::Neg, KernelValue::Vec4([x, y, z, w])) => Ok(KernelValue::Vec4([-x, -y, -z, -w])),
        (UnaryOp::Neg, KernelValue::Quat([x, y, z, w])) => Ok(KernelValue::Quat([-x, -y, -z, -w])),
        (UnaryOp::Neg, KernelValue::Mat3(values)) => Ok(KernelValue::Mat3([
            -values[0], -values[1], -values[2], -values[3], -values[4], -values[5], -values[6],
            -values[7], -values[8],
        ])),
        (UnaryOp::Neg, KernelValue::Mat4(values)) => Ok(KernelValue::Mat4([
            -values[0],
            -values[1],
            -values[2],
            -values[3],
            -values[4],
            -values[5],
            -values[6],
            -values[7],
            -values[8],
            -values[9],
            -values[10],
            -values[11],
            -values[12],
            -values[13],
            -values[14],
            -values[15],
        ])),
        (UnaryOp::Not, KernelValue::Bool(value)) => Ok(KernelValue::Bool(!value)),
        (UnaryOp::BitNot, KernelValue::I32(value)) => Ok(KernelValue::I32(!value)),
        (UnaryOp::BitNot, KernelValue::U32(value)) => Ok(KernelValue::U32(!value)),
        (_, value) => Err(QueryExecError::Unsupported {
            message: format!("unary op {op:?} does not support {value:?}"),
        }),
    }
}

fn eval_binary_value(
    op: BinaryOp,
    lhs: KernelValue,
    rhs: KernelValue,
) -> Result<KernelValue, QueryExecError> {
    match (op, lhs, rhs) {
        (BinaryOp::Add, KernelValue::I32(lhs), KernelValue::I32(rhs)) => {
            Ok(KernelValue::I32(lhs.saturating_add(rhs)))
        }
        (BinaryOp::Sub, KernelValue::I32(lhs), KernelValue::I32(rhs)) => {
            Ok(KernelValue::I32(lhs.saturating_sub(rhs)))
        }
        (BinaryOp::Mul, KernelValue::I32(lhs), KernelValue::I32(rhs)) => {
            Ok(KernelValue::I32(lhs.saturating_mul(rhs)))
        }
        (BinaryOp::Div, KernelValue::I32(lhs), KernelValue::I32(rhs)) => {
            Ok(KernelValue::I32(lhs.checked_div(rhs).unwrap_or(0)))
        }
        (BinaryOp::Eq, KernelValue::I32(lhs), KernelValue::I32(rhs)) => {
            Ok(KernelValue::Bool(lhs == rhs))
        }
        (BinaryOp::Eq, KernelValue::U32(lhs), KernelValue::U32(rhs)) => {
            Ok(KernelValue::Bool(lhs == rhs))
        }
        (BinaryOp::Eq, KernelValue::F32(lhs), KernelValue::F32(rhs)) => {
            Ok(KernelValue::Bool((lhs - rhs).abs() < f32::EPSILON))
        }
        (BinaryOp::Eq, KernelValue::Bool(lhs), KernelValue::Bool(rhs)) => {
            Ok(KernelValue::Bool(lhs == rhs))
        }
        (BinaryOp::Ne, lhs, rhs) => {
            let KernelValue::Bool(eq) = eval_binary_value(BinaryOp::Eq, lhs, rhs)? else {
                return Err(QueryExecError::Unsupported {
                    message: "binary Ne expected boolean equality result".to_string(),
                });
            };
            Ok(KernelValue::Bool(!eq))
        }
        (BinaryOp::And, KernelValue::Bool(lhs), KernelValue::Bool(rhs)) => {
            Ok(KernelValue::Bool(lhs && rhs))
        }
        (BinaryOp::Or, KernelValue::Bool(lhs), KernelValue::Bool(rhs)) => {
            Ok(KernelValue::Bool(lhs || rhs))
        }
        (BinaryOp::Lt, KernelValue::I32(lhs), KernelValue::I32(rhs)) => {
            Ok(KernelValue::Bool(lhs < rhs))
        }
        (BinaryOp::Le, KernelValue::I32(lhs), KernelValue::I32(rhs)) => {
            Ok(KernelValue::Bool(lhs <= rhs))
        }
        (BinaryOp::Gt, KernelValue::I32(lhs), KernelValue::I32(rhs)) => {
            Ok(KernelValue::Bool(lhs > rhs))
        }
        (BinaryOp::Ge, KernelValue::I32(lhs), KernelValue::I32(rhs)) => {
            Ok(KernelValue::Bool(lhs >= rhs))
        }
        (BinaryOp::Lt, KernelValue::F32(lhs), KernelValue::F32(rhs)) => {
            Ok(KernelValue::Bool(lhs < rhs))
        }
        (BinaryOp::Le, KernelValue::F32(lhs), KernelValue::F32(rhs)) => {
            Ok(KernelValue::Bool(lhs <= rhs))
        }
        (BinaryOp::Gt, KernelValue::F32(lhs), KernelValue::F32(rhs)) => {
            Ok(KernelValue::Bool(lhs > rhs))
        }
        (BinaryOp::Ge, KernelValue::F32(lhs), KernelValue::F32(rhs)) => {
            Ok(KernelValue::Bool(lhs >= rhs))
        }
        (BinaryOp::Add, KernelValue::F32(lhs), KernelValue::F32(rhs)) => {
            Ok(KernelValue::F32(lhs + rhs))
        }
        (BinaryOp::Sub, KernelValue::F32(lhs), KernelValue::F32(rhs)) => {
            Ok(KernelValue::F32(lhs - rhs))
        }
        (BinaryOp::Mul, KernelValue::F32(lhs), KernelValue::F32(rhs)) => {
            Ok(KernelValue::F32(lhs * rhs))
        }
        (BinaryOp::Div, KernelValue::F32(lhs), KernelValue::F32(rhs)) => {
            Ok(KernelValue::F32(lhs / rhs))
        }
        (BinaryOp::Add, KernelValue::Vec2(lhs), KernelValue::Vec2(rhs)) => Ok(
            runtime_binary_value(KernelValue::Vec2(lhs), KernelValue::Vec2(rhs), wr_vec_add)?,
        ),
        (BinaryOp::Sub, KernelValue::Vec2(lhs), KernelValue::Vec2(rhs)) => Ok(
            runtime_binary_value(KernelValue::Vec2(lhs), KernelValue::Vec2(rhs), wr_vec_sub)?,
        ),
        (BinaryOp::Mul, KernelValue::Vec2(lhs), KernelValue::Vec2(rhs)) => Ok(
            runtime_binary_value(KernelValue::Vec2(lhs), KernelValue::Vec2(rhs), wr_vec_mul)?,
        ),
        (BinaryOp::Div, KernelValue::Vec2(lhs), KernelValue::Vec2(rhs)) => Ok(
            runtime_binary_value(KernelValue::Vec2(lhs), KernelValue::Vec2(rhs), wr_vec_div)?,
        ),
        (BinaryOp::Add, KernelValue::Vec3(lhs), KernelValue::Vec3(rhs)) => Ok(
            runtime_binary_value(KernelValue::Vec3(lhs), KernelValue::Vec3(rhs), wr_vec_add)?,
        ),
        (BinaryOp::Sub, KernelValue::Vec3(lhs), KernelValue::Vec3(rhs)) => Ok(
            runtime_binary_value(KernelValue::Vec3(lhs), KernelValue::Vec3(rhs), wr_vec_sub)?,
        ),
        (BinaryOp::Mul, KernelValue::Vec3(lhs), KernelValue::Vec3(rhs)) => Ok(
            runtime_binary_value(KernelValue::Vec3(lhs), KernelValue::Vec3(rhs), wr_vec_mul)?,
        ),
        (BinaryOp::Div, KernelValue::Vec3(lhs), KernelValue::Vec3(rhs)) => Ok(
            runtime_binary_value(KernelValue::Vec3(lhs), KernelValue::Vec3(rhs), wr_vec_div)?,
        ),
        (BinaryOp::Add, KernelValue::Vec4(lhs), KernelValue::Vec4(rhs)) => Ok(
            runtime_binary_value(KernelValue::Vec4(lhs), KernelValue::Vec4(rhs), wr_vec_add)?,
        ),
        (BinaryOp::Sub, KernelValue::Vec4(lhs), KernelValue::Vec4(rhs)) => Ok(
            runtime_binary_value(KernelValue::Vec4(lhs), KernelValue::Vec4(rhs), wr_vec_sub)?,
        ),
        (BinaryOp::Mul, KernelValue::Vec4(lhs), KernelValue::Vec4(rhs)) => Ok(
            runtime_binary_value(KernelValue::Vec4(lhs), KernelValue::Vec4(rhs), wr_vec_mul)?,
        ),
        (BinaryOp::Div, KernelValue::Vec4(lhs), KernelValue::Vec4(rhs)) => Ok(
            runtime_binary_value(KernelValue::Vec4(lhs), KernelValue::Vec4(rhs), wr_vec_div)?,
        ),
        (op @ (BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div), lhs, rhs)
            if is_componentwise_numeric(&lhs) && is_componentwise_numeric(&rhs) =>
        {
            eval_componentwise_binary(op, lhs, rhs)
        }
        (op, lhs, rhs) => Err(QueryExecError::Unsupported {
            message: format!("binary op {op:?} does not support {lhs:?} and {rhs:?}"),
        }),
    }
}

fn is_componentwise_numeric(value: &KernelValue) -> bool {
    matches!(
        value,
        KernelValue::F32(_)
            | KernelValue::Vec2(_)
            | KernelValue::Vec3(_)
            | KernelValue::Vec4(_)
            | KernelValue::Quat(_)
    )
}

fn eval_componentwise_binary(
    op: BinaryOp,
    lhs: KernelValue,
    rhs: KernelValue,
) -> Result<KernelValue, QueryExecError> {
    let lhs_lane_count = kernel_components(&lhs, "componentwise binary")?.len();
    let rhs_lane_count = kernel_components(&rhs, "componentwise binary")?.len();
    let target_len = lhs_lane_count.max(rhs_lane_count);
    let lhs_components = broadcast_components(&lhs, target_len, "componentwise binary")?;
    let rhs_components = broadcast_components(&rhs, target_len, "componentwise binary")?;
    let mapped = lhs_components
        .iter()
        .zip(rhs_components.iter())
        .map(|(lhs, rhs)| match op {
            BinaryOp::Add => lhs + rhs,
            BinaryOp::Sub => lhs - rhs,
            BinaryOp::Mul => lhs * rhs,
            BinaryOp::Div => lhs / rhs,
            _ => unreachable!("componentwise helper only handles arithmetic"),
        })
        .collect::<Vec<_>>();
    let prototype = if lhs_lane_count >= rhs_lane_count {
        &lhs
    } else {
        &rhs
    };
    same_kind_from_components(prototype, &mapped, "componentwise binary")
}

fn eval_member_value(base: KernelValue, member: &SmolStr) -> Result<KernelValue, QueryExecError> {
    match base {
        KernelValue::Struct(value) => value
            .fields
            .iter()
            .find(|(name, _)| name == member)
            .map(|(_, value)| value.clone())
            .ok_or_else(|| QueryExecError::Unsupported {
                message: format!(
                    "struct '{}' does not contain member '{}'",
                    value.name, member
                ),
            }),
        KernelValue::Vec2(value) => vector_member(&value, member, "xy"),
        KernelValue::Vec3(value) => vector_member(&value, member, "xyz"),
        KernelValue::Vec4(value) | KernelValue::Quat(value) => {
            vector_member(&value, member, "xyzw")
        }
        other => Err(QueryExecError::Unsupported {
            message: format!("member access is not implemented for {other:?}"),
        }),
    }
}

fn eval_index_value(base: KernelValue, index: KernelValue) -> Result<KernelValue, QueryExecError> {
    let index = match index {
        KernelValue::I32(value) if value >= 0 => value as usize,
        KernelValue::U32(value) => value as usize,
        other => {
            return Err(QueryExecError::TypeMismatch {
                expected: "array/vector index".to_string(),
                found: format!("{other:?}"),
            });
        }
    };
    match base {
        KernelValue::Array(items) => {
            items
                .get(index)
                .cloned()
                .ok_or_else(|| QueryExecError::Unsupported {
                    message: format!("index {index} is out of bounds"),
                })
        }
        KernelValue::Vec2(values) => {
            values
                .get(index)
                .copied()
                .map(KernelValue::F32)
                .ok_or_else(|| QueryExecError::Unsupported {
                    message: format!("index {index} is out of bounds"),
                })
        }
        KernelValue::Vec3(values) => {
            values
                .get(index)
                .copied()
                .map(KernelValue::F32)
                .ok_or_else(|| QueryExecError::Unsupported {
                    message: format!("index {index} is out of bounds"),
                })
        }
        KernelValue::Vec4(values) | KernelValue::Quat(values) => values
            .get(index)
            .copied()
            .map(KernelValue::F32)
            .ok_or_else(|| QueryExecError::Unsupported {
                message: format!("index {index} is out of bounds"),
            }),
        other => Err(QueryExecError::Unsupported {
            message: format!("indexing is not implemented for {other:?}"),
        }),
    }
}

fn vector_member<const N: usize>(
    values: &[f32; N],
    member: &SmolStr,
    alphabet: &str,
) -> Result<KernelValue, QueryExecError> {
    let Some(index) = alphabet.find(member.as_str()) else {
        return Err(QueryExecError::Unsupported {
            message: format!("unknown vector member '{member}'"),
        });
    };
    values
        .get(index)
        .copied()
        .map(KernelValue::F32)
        .ok_or_else(|| QueryExecError::Unsupported {
            message: format!("unknown vector member '{member}'"),
        })
}

fn value_label(value: &KernelValue) -> String {
    match value {
        KernelValue::Nothing => "Nothing".to_string(),
        KernelValue::Bool(_) => "Bool".to_string(),
        KernelValue::I32(_) => "I32".to_string(),
        KernelValue::U32(_) => "U32".to_string(),
        KernelValue::F32(_) => "F32".to_string(),
        KernelValue::Vec2(_) => "Vec2".to_string(),
        KernelValue::Vec3(_) => "Vec3".to_string(),
        KernelValue::Vec4(_) => "Vec4".to_string(),
        KernelValue::Mat3(_) => "Mat3".to_string(),
        KernelValue::Mat4(_) => "Mat4".to_string(),
        KernelValue::Quat(_) => "Quat".to_string(),
        KernelValue::Array(_) => "Array".to_string(),
        KernelValue::Struct(value) => value.name.to_string(),
        KernelValue::Capture(name) => format!("Capture({name})"),
        KernelValue::DispatchBackend(_) => "DispatchBackend".to_string(),
        KernelValue::GpuBuffer(_) => "GpuBuffer".to_string(),
        KernelValue::GpuAtomicI32(_) => "GpuAtomicI32".to_string(),
        KernelValue::GpuAtomicU32(_) => "GpuAtomicU32".to_string(),
    }
}

fn default_shape_winner() -> ShapeWinner {
    ShapeWinner {
        distance: 1_000_000.0,
        feature_id: 0,
        leaf: None,
    }
}

fn chain_identity_component(current: u32, component: u32) -> u32 {
    if component == 0 {
        return current;
    }
    if current == 0 {
        return component;
    }
    let mixed = (current ^ component).wrapping_mul(16_777_619);
    if mixed == 0 { 1 } else { mixed }
}

fn add3(lhs: [f32; 3], rhs: [f32; 3]) -> [f32; 3] {
    [lhs[0] + rhs[0], lhs[1] + rhs[1], lhs[2] + rhs[2]]
}

fn mul3_scalar(value: [f32; 3], scalar: f32) -> [f32; 3] {
    [value[0] * scalar, value[1] * scalar, value[2] * scalar]
}

fn transform_certified_normal(
    kind: TransformKind,
    config: &KernelValue,
    normal: [f32; 3],
) -> Result<[f32; 3], QueryExecError> {
    match kind {
        TransformKind::Translate | TransformKind::UniformScale => Ok(normal),
        TransformKind::Rotate => {
            let rotation = match config {
                KernelValue::F32(angle) => KernelValue::F32(-angle),
                KernelValue::Vec3(rotation) => {
                    KernelValue::Vec3([-rotation[0], -rotation[1], -rotation[2]])
                }
                other => {
                    return Err(QueryExecError::TypeMismatch {
                        expected: "rotate normal parameter: Float or Vec3".to_string(),
                        found: value_label(other),
                    });
                }
            };
            let transformed = runtime_binary_value(rotation, KernelValue::Vec3(normal), wr_rotate)?;
            expect_vec3(Some(&transformed), "transformed normal")
        }
        other => Err(QueryExecError::Unsupported {
            message: format!("certified normal transform does not support {other:?}"),
        }),
    }
}

fn smooth_blended_normal(
    kind: SmoothKind,
    smoothing: f32,
    left_distance: f32,
    left_normal: [f32; 3],
    right_distance: f32,
    right_normal: [f32; 3],
) -> [f32; 3] {
    if smoothing <= 0.0 {
        return left_normal;
    }
    let h = (0.5 + 0.5 * (right_distance - left_distance) / smoothing).clamp(0.0, 1.0);
    let rhs = match kind {
        SmoothKind::Subtract => mul3_scalar(right_normal, -1.0),
        SmoothKind::Union | SmoothKind::Intersection => right_normal,
    };
    normalize3(add3(mul3_scalar(left_normal, h), mul3_scalar(rhs, 1.0 - h)))
}

fn empty_support_bounds() -> SupportBounds {
    SupportBounds {
        min: [0.0, 0.0, 0.0],
        max: [0.0, 0.0, 0.0],
    }
}

fn normalize_support_bounds(bounds: SupportBounds) -> SupportBounds {
    SupportBounds {
        min: [
            bounds.min[0].min(bounds.max[0]),
            bounds.min[1].min(bounds.max[1]),
            bounds.min[2].min(bounds.max[2]),
        ],
        max: [
            bounds.min[0].max(bounds.max[0]),
            bounds.min[1].max(bounds.max[1]),
            bounds.min[2].max(bounds.max[2]),
        ],
    }
}

fn merge_union_support_bounds(lhs: SupportBounds, rhs: SupportBounds) -> SupportBounds {
    SupportBounds {
        min: [
            lhs.min[0].min(rhs.min[0]),
            lhs.min[1].min(rhs.min[1]),
            lhs.min[2].min(rhs.min[2]),
        ],
        max: [
            lhs.max[0].max(rhs.max[0]),
            lhs.max[1].max(rhs.max[1]),
            lhs.max[2].max(rhs.max[2]),
        ],
    }
}

fn merge_intersection_support_bounds(lhs: SupportBounds, rhs: SupportBounds) -> SupportBounds {
    normalize_support_bounds(SupportBounds {
        min: [
            lhs.min[0].max(rhs.min[0]),
            lhs.min[1].max(rhs.min[1]),
            lhs.min[2].max(rhs.min[2]),
        ],
        max: [
            lhs.max[0].min(rhs.max[0]),
            lhs.max[1].min(rhs.max[1]),
            lhs.max[2].min(rhs.max[2]),
        ],
    })
}

fn ray_support_interval_for_bounds(
    bounds: SupportBounds,
    origin: [f32; 3],
    direction: [f32; 3],
) -> RaySupportProbe {
    let mut start_t = f32::NEG_INFINITY;
    let mut end_t = f32::INFINITY;
    for axis in 0..3 {
        let direction_component = direction[axis];
        if direction_component.abs() <= f32::EPSILON {
            if origin[axis] < bounds.min[axis] || origin[axis] > bounds.max[axis] {
                return RaySupportProbe::Rejected;
            }
            continue;
        }
        let inv_direction = 1.0 / direction_component;
        let mut entry_t = (bounds.min[axis] - origin[axis]) * inv_direction;
        let mut exit_t = (bounds.max[axis] - origin[axis]) * inv_direction;
        if entry_t > exit_t {
            std::mem::swap(&mut entry_t, &mut exit_t);
        }
        start_t = start_t.max(entry_t);
        end_t = end_t.min(exit_t);
        if start_t > end_t {
            return RaySupportProbe::Rejected;
        }
    }
    RaySupportProbe::Interval(RaySupportInterval {
        start_t,
        end_t,
        starts_inside: start_t <= 0.0 && end_t >= 0.0,
        conservative: true,
    })
}

fn ray_support_interval_for_sphere(
    center: [f32; 3],
    radius: f32,
    origin: [f32; 3],
    direction: [f32; 3],
) -> RaySupportProbe {
    let oc = [
        origin[0] - center[0],
        origin[1] - center[1],
        origin[2] - center[2],
    ];
    let a = dot3(direction, direction);
    let c = dot3(oc, oc) - radius * radius;
    if a <= f32::EPSILON {
        return if c <= 0.0 {
            RaySupportProbe::Interval(RaySupportInterval {
                start_t: f32::NEG_INFINITY,
                end_t: f32::INFINITY,
                starts_inside: true,
                conservative: true,
            })
        } else {
            RaySupportProbe::Rejected
        };
    }
    let b = dot3(oc, direction);
    let discriminant = b * b - a * c;
    if discriminant < 0.0 {
        return RaySupportProbe::Rejected;
    }
    let sqrt_discriminant = discriminant.sqrt();
    let mut start_t = (-b - sqrt_discriminant) / a;
    let mut end_t = (-b + sqrt_discriminant) / a;
    if start_t > end_t {
        std::mem::swap(&mut start_t, &mut end_t);
    }
    RaySupportProbe::Interval(RaySupportInterval {
        start_t,
        end_t,
        starts_inside: c <= 0.0,
        conservative: true,
    })
}

fn ray_support_interval_for_periodic_bounds(
    bounds: SupportBounds,
    period: [f32; 3],
    origin: [f32; 3],
    direction: [f32; 3],
) -> RaySupportProbe {
    let bounds = normalize_support_bounds(bounds);
    let mut start_t = f32::NEG_INFINITY;
    for axis in 0..3 {
        let Some(axis_start_t) = periodic_axis_start_t(
            bounds.min[axis],
            bounds.max[axis],
            period[axis].abs(),
            origin[axis],
            direction[axis],
        ) else {
            return RaySupportProbe::Rejected;
        };
        start_t = start_t.max(axis_start_t);
    }
    RaySupportProbe::Interval(RaySupportInterval {
        start_t,
        end_t: f32::INFINITY,
        starts_inside: start_t <= 0.0,
        conservative: true,
    })
}

fn periodic_axis_start_t(
    min: f32,
    max: f32,
    period: f32,
    origin: f32,
    direction: f32,
) -> Option<f32> {
    if period <= f32::EPSILON {
        return axis_interval_start_t(min, max, origin, direction);
    }
    let width = (max - min).abs();
    if width >= period - f32::EPSILON {
        return Some(f32::NEG_INFINITY);
    }
    let midpoint = (min + max) * 0.5;
    let local_origin = wrap_periodic_coordinate(origin, period, midpoint);
    if direction.abs() <= f32::EPSILON {
        return (local_origin >= min && local_origin <= max).then_some(f32::NEG_INFINITY);
    }
    if direction > 0.0 {
        if local_origin < min {
            Some((min - local_origin) / direction)
        } else if local_origin <= max {
            Some(f32::NEG_INFINITY)
        } else {
            Some((min + period - local_origin) / direction)
        }
    } else if local_origin > max {
        Some((max - local_origin) / direction)
    } else if local_origin >= min {
        Some(f32::NEG_INFINITY)
    } else {
        Some((max - period - local_origin) / direction)
    }
}

fn wrap_periodic_coordinate(coord: f32, period: f32, midpoint: f32) -> f32 {
    coord - period * ((coord - midpoint) / period).round()
}

fn axis_interval_start_t(min: f32, max: f32, origin: f32, direction: f32) -> Option<f32> {
    if direction.abs() <= f32::EPSILON {
        return (origin >= min && origin <= max).then_some(f32::NEG_INFINITY);
    }
    let mut start_t = (min - origin) / direction;
    let mut end_t = (max - origin) / direction;
    if start_t > end_t {
        std::mem::swap(&mut start_t, &mut end_t);
    }
    (start_t <= end_t).then_some(start_t)
}

fn ray_support_interval_for_radial_repeat_bounds(
    bounds: SupportBounds,
    period: f32,
    origin: [f32; 3],
    direction: [f32; 3],
) -> RaySupportProbe {
    if period <= f32::EPSILON {
        return RaySupportProbe::Unavailable;
    }
    let bounds = normalize_support_bounds(bounds);
    let max_radius = support_bounds_corners(bounds)
        .iter()
        .map(|corner| (corner[0] * corner[0] + corner[2] * corner[2]).sqrt())
        .fold(0.0f32, f32::max);
    if max_radius <= f32::EPSILON {
        let collapsed = SupportBounds {
            min: [0.0, bounds.min[1], 0.0],
            max: [0.0, bounds.max[1], 0.0],
        };
        return ray_support_interval_for_bounds(collapsed, origin, direction);
    }
    ray_support_interval_for_bounds(
        SupportBounds {
            min: [-max_radius, bounds.min[1], -max_radius],
            max: [max_radius, bounds.max[1], max_radius],
        },
        origin,
        direction,
    )
}

fn merge_union_support_probe(lhs: RaySupportProbe, rhs: RaySupportProbe) -> RaySupportProbe {
    match (lhs, rhs) {
        (RaySupportProbe::Unavailable, _) | (_, RaySupportProbe::Unavailable) => {
            RaySupportProbe::Unavailable
        }
        (RaySupportProbe::Rejected, probe) | (probe, RaySupportProbe::Rejected) => probe,
        (RaySupportProbe::Interval(lhs), RaySupportProbe::Interval(rhs)) => {
            RaySupportProbe::Interval(RaySupportInterval {
                start_t: lhs.start_t.min(rhs.start_t),
                end_t: lhs.end_t.max(rhs.end_t),
                starts_inside: lhs.starts_inside || rhs.starts_inside,
                conservative: lhs.conservative || rhs.conservative,
            })
        }
    }
}

fn merge_intersection_support_probe(lhs: RaySupportProbe, rhs: RaySupportProbe) -> RaySupportProbe {
    match (lhs, rhs) {
        (RaySupportProbe::Rejected, _) | (_, RaySupportProbe::Rejected) => {
            RaySupportProbe::Rejected
        }
        (RaySupportProbe::Unavailable, _) | (_, RaySupportProbe::Unavailable) => {
            RaySupportProbe::Unavailable
        }
        (RaySupportProbe::Interval(lhs), RaySupportProbe::Interval(rhs)) => {
            let start_t = lhs.start_t.max(rhs.start_t);
            let end_t = lhs.end_t.min(rhs.end_t);
            if start_t > end_t {
                RaySupportProbe::Rejected
            } else {
                RaySupportProbe::Interval(RaySupportInterval {
                    start_t,
                    end_t,
                    starts_inside: lhs.starts_inside && rhs.starts_inside,
                    conservative: lhs.conservative || rhs.conservative,
                })
            }
        }
    }
}

fn reflect_support_bounds(bounds: SupportBounds, normal: [f32; 3]) -> SupportBounds {
    let mut reflected = None;
    for corner in support_bounds_corners(bounds) {
        let point = reflect_point_across_plane(normal, corner);
        let point_bounds = SupportBounds {
            min: point,
            max: point,
        };
        reflected = Some(match reflected {
            Some(current) => merge_union_support_bounds(current, point_bounds),
            None => point_bounds,
        });
    }
    reflected.unwrap_or(bounds)
}

fn transform_value_support_bounds(
    value: &KernelValue,
    bounds: SupportBounds,
) -> Result<Option<SupportBounds>, QueryExecError> {
    match value {
        KernelValue::Vec3(offset) => Ok(Some(SupportBounds {
            min: add3(bounds.min, *offset),
            max: add3(bounds.max, *offset),
        })),
        KernelValue::Struct(transform) if transform.name.as_str() == "Transform3" => {
            let matrix = expect_struct_mat4(transform, "matrix")?;
            let mut transformed = None;
            for corner in support_bounds_corners(bounds) {
                let point = mat4_mul_point(matrix, corner);
                let point_bounds = SupportBounds {
                    min: point,
                    max: point,
                };
                transformed = Some(match transformed {
                    Some(current) => merge_union_support_bounds(current, point_bounds),
                    None => point_bounds,
                });
            }
            Ok(transformed)
        }
        _ => Ok(None),
    }
}

fn support_bounds_corners(bounds: SupportBounds) -> [[f32; 3]; 8] {
    [
        [bounds.min[0], bounds.min[1], bounds.min[2]],
        [bounds.min[0], bounds.min[1], bounds.max[2]],
        [bounds.min[0], bounds.max[1], bounds.min[2]],
        [bounds.min[0], bounds.max[1], bounds.max[2]],
        [bounds.max[0], bounds.min[1], bounds.min[2]],
        [bounds.max[0], bounds.min[1], bounds.max[2]],
        [bounds.max[0], bounds.max[1], bounds.min[2]],
        [bounds.max[0], bounds.max[1], bounds.max[2]],
    ]
}

fn reflect_point_across_plane(normal: [f32; 3], point: [f32; 3]) -> [f32; 3] {
    let unit = normalize3(normal);
    let distance = dot3(point, unit);
    [
        point[0] - 2.0 * distance * unit[0],
        point[1] - 2.0 * distance * unit[1],
        point[2] - 2.0 * distance * unit[2],
    ]
}

fn reflect_vector_across_plane(normal: [f32; 3], vector: [f32; 3]) -> [f32; 3] {
    let unit = normalize3(normal);
    let distance = dot3(vector, unit);
    [
        vector[0] - 2.0 * distance * unit[0],
        vector[1] - 2.0 * distance * unit[1],
        vector[2] - 2.0 * distance * unit[2],
    ]
}

fn reflect_ray_across_plane(
    normal: [f32; 3],
    origin: [f32; 3],
    direction: [f32; 3],
) -> ([f32; 3], [f32; 3]) {
    (
        reflect_point_across_plane(normal, origin),
        reflect_vector_across_plane(normal, direction),
    )
}

fn instance_array_local_ray(
    config: &KernelValue,
    origin: [f32; 3],
    direction: [f32; 3],
) -> Result<Option<([f32; 3], [f32; 3])>, QueryExecError> {
    match config {
        KernelValue::Vec3(translation) => Ok(Some((
            [
                origin[0] - translation[0],
                origin[1] - translation[1],
                origin[2] - translation[2],
            ],
            direction,
        ))),
        KernelValue::Struct(transform) if transform.name.as_str() == "Transform3" => {
            let inverse = expect_struct_mat4(transform, "inverse")?;
            Ok(Some((
                mat4_mul_point(inverse, origin),
                mat4_mul_vector(inverse, direction),
            )))
        }
        _ => Ok(None),
    }
}

fn pure_translation_local_ray(
    config: &KernelValue,
    origin: [f32; 3],
    direction: [f32; 3],
) -> Result<Option<([f32; 3], [f32; 3])>, QueryExecError> {
    match config {
        KernelValue::Vec3(translation) => Ok(Some((
            [
                origin[0] - translation[0],
                origin[1] - translation[1],
                origin[2] - translation[2],
            ],
            direction,
        ))),
        KernelValue::Struct(transform) if transform.name.as_str() == "Transform3" => {
            let matrix = expect_struct_mat4(transform, "matrix")?;
            let inverse = expect_struct_mat4(transform, "inverse")?;
            if !transform3_is_pure_translation(matrix, inverse) {
                return Ok(None);
            }
            Ok(Some((
                [
                    origin[0] + inverse[12],
                    origin[1] + inverse[13],
                    origin[2] + inverse[14],
                ],
                direction,
            )))
        }
        _ => Ok(None),
    }
}

fn transform3_is_pure_translation(matrix: [f32; 16], inverse: [f32; 16]) -> bool {
    const EPSILON: f32 = 1e-5;
    let identity_linear = [
        (0, 1.0),
        (1, 0.0),
        (2, 0.0),
        (3, 0.0),
        (4, 0.0),
        (5, 1.0),
        (6, 0.0),
        (7, 0.0),
        (8, 0.0),
        (9, 0.0),
        (10, 1.0),
        (11, 0.0),
        (15, 1.0),
    ];
    let linear_ok = identity_linear.iter().all(|(index, expected)| {
        (matrix[*index] - *expected).abs() <= EPSILON
            && (inverse[*index] - *expected).abs() <= EPSILON
    });
    let inverse_translation_ok = (matrix[12] + inverse[12]).abs() <= EPSILON
        && (matrix[13] + inverse[13]).abs() <= EPSILON
        && (matrix[14] + inverse[14]).abs() <= EPSILON;
    linear_ok && inverse_translation_ok
}

fn mat4_mul_point(matrix: [f32; 16], point: [f32; 3]) -> [f32; 3] {
    let x = matrix[0] * point[0] + matrix[4] * point[1] + matrix[8] * point[2] + matrix[12];
    let y = matrix[1] * point[0] + matrix[5] * point[1] + matrix[9] * point[2] + matrix[13];
    let z = matrix[2] * point[0] + matrix[6] * point[1] + matrix[10] * point[2] + matrix[14];
    let w = matrix[3] * point[0] + matrix[7] * point[1] + matrix[11] * point[2] + matrix[15];
    if w.abs() > f32::EPSILON {
        [x / w, y / w, z / w]
    } else {
        [x, y, z]
    }
}

fn mat4_mul_vector(matrix: [f32; 16], vector: [f32; 3]) -> [f32; 3] {
    [
        matrix[0] * vector[0] + matrix[4] * vector[1] + matrix[8] * vector[2],
        matrix[1] * vector[0] + matrix[5] * vector[1] + matrix[9] * vector[2],
        matrix[2] * vector[0] + matrix[6] * vector[1] + matrix[10] * vector[2],
    ]
}

fn merge_world_support_summaries(items: &[SupportSummaryParts]) -> SupportSummaryParts {
    if items.is_empty() {
        return SupportSummaryParts {
            support_class: SupportClass::Unknown,
            semantics: DistanceSemantics::ConservativeLowerBound,
            has_bounds: false,
            opaque_boundary: false,
            can_coarse_support_prune: false,
            bounds: empty_support_bounds(),
        };
    }

    let support_class = if items
        .iter()
        .any(|item| matches!(item.support_class, SupportClass::Unbounded))
    {
        SupportClass::Unbounded
    } else if items
        .iter()
        .any(|item| matches!(item.support_class, SupportClass::Periodic))
    {
        SupportClass::Periodic
    } else if items
        .iter()
        .any(|item| matches!(item.support_class, SupportClass::Unknown))
    {
        SupportClass::Unknown
    } else {
        SupportClass::Bounded
    };
    let semantics = if items
        .iter()
        .any(|item| matches!(item.semantics, DistanceSemantics::UnknownOpaque))
    {
        DistanceSemantics::UnknownOpaque
    } else if items.len() == 1 {
        items[0].semantics
    } else {
        DistanceSemantics::ConservativeLowerBound
    };
    let has_bounds = items.iter().all(|item| item.has_bounds);
    let bounds = if has_bounds {
        items
            .iter()
            .map(|item| item.bounds)
            .reduce(merge_union_support_bounds)
            .unwrap_or_else(empty_support_bounds)
    } else {
        empty_support_bounds()
    };
    let opaque_boundary = items.iter().any(|item| item.opaque_boundary);
    let can_coarse_support_prune = !opaque_boundary
        && matches!(support_class, SupportClass::Bounded)
        && items.iter().all(|item| item.can_coarse_support_prune);
    SupportSummaryParts {
        support_class,
        semantics,
        has_bounds,
        opaque_boundary,
        can_coarse_support_prune,
        bounds,
    }
}

fn support_class_code(class: SupportClass) -> u32 {
    match class {
        SupportClass::Unknown => 0,
        SupportClass::Bounded => 1,
        SupportClass::Periodic => 2,
        SupportClass::Unbounded => 3,
    }
}

fn distance_semantics_code(semantics: DistanceSemantics) -> u32 {
    match semantics {
        DistanceSemantics::ExactSignedDistance => 0,
        DistanceSemantics::ConservativeLowerBound => 1,
        DistanceSemantics::UnknownOpaque => 2,
    }
}

fn support_summary_value(summary: SupportSummaryParts) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("SupportSummaryResult"),
        fields: vec![
            (
                SmolStr::new("support_class"),
                KernelValue::U32(support_class_code(summary.support_class)),
            ),
            (
                SmolStr::new("semantics"),
                KernelValue::U32(distance_semantics_code(summary.semantics)),
            ),
            (
                SmolStr::new("has_bounds"),
                KernelValue::Bool(summary.has_bounds),
            ),
            (
                SmolStr::new("opaque_boundary"),
                KernelValue::Bool(summary.opaque_boundary),
            ),
            (
                SmolStr::new("can_coarse_support_prune"),
                KernelValue::Bool(summary.can_coarse_support_prune),
            ),
            (SmolStr::new("min"), KernelValue::Vec3(summary.bounds.min)),
            (SmolStr::new("max"), KernelValue::Vec3(summary.bounds.max)),
        ],
    })
}

fn dot3(lhs: [f32; 3], rhs: [f32; 3]) -> f32 {
    lhs[0] * rhs[0] + lhs[1] * rhs[1] + lhs[2] * rhs[2]
}

fn same_observability_certificate_metadata(
    lhs: &RayStepCertificateMetadata,
    rhs: &RayStepCertificateMetadata,
) -> bool {
    lhs.guarantee == rhs.guarantee
        && lhs.proof_family == rhs.proof_family
        && lhs.subject == rhs.subject
        && lhs.subject_kind == rhs.subject_kind
        && lhs.reusable_by == rhs.reusable_by
        && lhs.invalidation_reasons == rhs.invalidation_reasons
}

impl AnalyticPrimitiveRay {
    fn solve(&self, start_t: f32, max_t: f32) -> Option<f32> {
        match self.primitive {
            AnalyticPrimitive::Sphere { radius } => solve_ray_sphere(
                self.local_origin,
                self.local_direction,
                [0.0, 0.0, 0.0],
                radius,
                start_t,
                max_t,
            ),
            AnalyticPrimitive::Plane { normal, offset } => solve_ray_plane(
                self.local_origin,
                self.local_direction,
                normal,
                offset,
                start_t,
                max_t,
            ),
            AnalyticPrimitive::Slab { thickness } => solve_ray_slab(
                self.local_origin,
                self.local_direction,
                thickness,
                start_t,
                max_t,
            ),
            AnalyticPrimitive::Box { half } => solve_ray_aabb(
                self.local_origin,
                self.local_direction,
                [-half[0].abs(), -half[1].abs(), -half[2].abs()],
                [half[0].abs(), half[1].abs(), half[2].abs()],
                start_t,
                max_t,
            ),
            AnalyticPrimitive::Capsule { a, b, radius } => solve_ray_capsule(
                self.local_origin,
                self.local_direction,
                a,
                b,
                radius,
                start_t,
                max_t,
            ),
            AnalyticPrimitive::Cylinder {
                radius,
                half_height,
            } => solve_ray_cylinder(
                self.local_origin,
                self.local_direction,
                radius,
                half_height,
                start_t,
                max_t,
            ),
        }
    }
}

fn solve_ray_sphere(
    origin: [f32; 3],
    direction: [f32; 3],
    center: [f32; 3],
    radius: f32,
    start_t: f32,
    max_t: f32,
) -> Option<f32> {
    let oc = [
        origin[0] - center[0],
        origin[1] - center[1],
        origin[2] - center[2],
    ];
    let a = dot3(direction, direction);
    if a <= f32::EPSILON {
        return None;
    }
    let b = 2.0 * dot3(oc, direction);
    let c = dot3(oc, oc) - radius * radius;
    let discriminant = b * b - 4.0 * a * c;
    if discriminant < 0.0 {
        return None;
    }
    let root = discriminant.sqrt();
    let inv = 1.0 / (2.0 * a);
    select_first_valid_t([(-b - root) * inv, (-b + root) * inv], start_t, max_t)
}

fn solve_ray_plane(
    origin: [f32; 3],
    direction: [f32; 3],
    normal: [f32; 3],
    offset: f32,
    start_t: f32,
    max_t: f32,
) -> Option<f32> {
    let normal_len = dot3(normal, normal).sqrt();
    if normal_len <= f32::EPSILON {
        return None;
    }
    let unit = [
        normal[0] / normal_len,
        normal[1] / normal_len,
        normal[2] / normal_len,
    ];
    let denom = dot3(unit, direction);
    if denom.abs() <= f32::EPSILON {
        return None;
    }
    let t = -(dot3(origin, unit) + offset) / denom;
    (t >= start_t && t <= max_t).then_some(t)
}

fn solve_ray_slab(
    origin: [f32; 3],
    direction: [f32; 3],
    thickness: f32,
    start_t: f32,
    max_t: f32,
) -> Option<f32> {
    let half = thickness.abs() * 0.5;
    if direction[1].abs() <= f32::EPSILON {
        return None;
    }
    select_first_valid_t(
        [
            (half - origin[1]) / direction[1],
            (-half - origin[1]) / direction[1],
        ],
        start_t,
        max_t,
    )
}

fn solve_ray_aabb(
    origin: [f32; 3],
    direction: [f32; 3],
    min: [f32; 3],
    max: [f32; 3],
    start_t: f32,
    max_t: f32,
) -> Option<f32> {
    let mut entry = f32::NEG_INFINITY;
    let mut exit = f32::INFINITY;
    for axis in 0..3 {
        if direction[axis].abs() <= f32::EPSILON {
            if origin[axis] < min[axis] || origin[axis] > max[axis] {
                return None;
            }
            continue;
        }
        let inv = 1.0 / direction[axis];
        let mut axis_entry = (min[axis] - origin[axis]) * inv;
        let mut axis_exit = (max[axis] - origin[axis]) * inv;
        if axis_entry > axis_exit {
            std::mem::swap(&mut axis_entry, &mut axis_exit);
        }
        entry = entry.max(axis_entry);
        exit = exit.min(axis_exit);
        if entry > exit {
            return None;
        }
    }
    let t = if entry >= start_t { entry } else { exit };
    (t >= start_t && t <= max_t).then_some(t)
}

fn solve_ray_cylinder(
    origin: [f32; 3],
    direction: [f32; 3],
    radius: f32,
    half_height: f32,
    start_t: f32,
    max_t: f32,
) -> Option<f32> {
    let mut candidates = Vec::new();
    let a = direction[0] * direction[0] + direction[2] * direction[2];
    let b = origin[0] * direction[0] + origin[2] * direction[2];
    let c = origin[0] * origin[0] + origin[2] * origin[2] - radius * radius;
    if a > f32::EPSILON {
        let discriminant = b * b - a * c;
        if discriminant >= 0.0 {
            let root = discriminant.sqrt();
            for t in [(-b - root) / a, (-b + root) / a] {
                let y = origin[1] + direction[1] * t;
                if y.abs() <= half_height + 1e-4 {
                    candidates.push(t);
                }
            }
        }
    }
    if direction[1].abs() > f32::EPSILON {
        for y in [-half_height, half_height] {
            let t = (y - origin[1]) / direction[1];
            let x = origin[0] + direction[0] * t;
            let z = origin[2] + direction[2] * t;
            if x * x + z * z <= radius * radius + 1e-4 {
                candidates.push(t);
            }
        }
    }
    select_first_valid_t(candidates, start_t, max_t)
}

fn solve_ray_capsule(
    origin: [f32; 3],
    direction: [f32; 3],
    a: [f32; 3],
    b: [f32; 3],
    radius: f32,
    start_t: f32,
    max_t: f32,
) -> Option<f32> {
    let ba = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let oa = [origin[0] - a[0], origin[1] - a[1], origin[2] - a[2]];
    let baba = dot3(ba, ba);
    let bard = dot3(ba, direction);
    let baoa = dot3(ba, oa);
    let rdoa = dot3(direction, oa);
    let oaoa = dot3(oa, oa);
    let a_coef = baba - bard * bard;
    let b_coef = baba * rdoa - baoa * bard;
    let c_coef = baba * oaoa - baoa * baoa - radius * radius * baba;
    let mut candidates = Vec::new();
    if a_coef.abs() > f32::EPSILON {
        let discriminant = b_coef * b_coef - a_coef * c_coef;
        if discriminant >= 0.0 {
            let root = discriminant.sqrt();
            for t in [(-b_coef - root) / a_coef, (-b_coef + root) / a_coef] {
                let y = baoa + t * bard;
                if y >= 0.0 && y <= baba {
                    candidates.push(t);
                }
            }
        }
    }
    candidates.extend(
        [
            solve_ray_sphere(origin, direction, a, radius, start_t, max_t),
            solve_ray_sphere(origin, direction, b, radius, start_t, max_t),
        ]
        .into_iter()
        .flatten(),
    );
    select_first_valid_t(candidates, start_t, max_t)
}

fn select_first_valid_t(
    candidates: impl IntoIterator<Item = f32>,
    start_t: f32,
    max_t: f32,
) -> Option<f32> {
    candidates
        .into_iter()
        .filter(|t| t.is_finite() && *t >= start_t && *t <= max_t)
        .min_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal))
}

fn ray_aabb_interval(
    origin: [f32; 3],
    direction: [f32; 3],
    min: [f32; 3],
    max: [f32; 3],
    start_t: f32,
    max_t: f32,
) -> Option<(f32, f32)> {
    let mut entry = start_t.max(0.0);
    let mut exit = max_t;
    for axis in 0..3 {
        if direction[axis].abs() <= f32::EPSILON {
            if origin[axis] < min[axis] || origin[axis] > max[axis] {
                return None;
            }
            continue;
        }
        let inv = 1.0 / direction[axis];
        let mut axis_entry = (min[axis] - origin[axis]) * inv;
        let mut axis_exit = (max[axis] - origin[axis]) * inv;
        if axis_entry > axis_exit {
            std::mem::swap(&mut axis_entry, &mut axis_exit);
        }
        entry = entry.max(axis_entry);
        exit = exit.min(axis_exit);
        if entry > exit {
            return None;
        }
    }
    (entry <= exit).then_some((entry, exit))
}

fn axis_aligned_repeat_axis(period: [f32; 3]) -> Option<usize> {
    const AXIS_EPSILON: f32 = 1e-5;
    let active = (0..3)
        .filter(|axis| period[*axis].abs() > AXIS_EPSILON)
        .collect::<Vec<_>>();
    (active.len() == 1).then_some(active[0])
}

fn axis_aligned_repeat_linear_cells(
    bounds: SupportBounds,
    axis: usize,
    period: f32,
    origin: [f32; 3],
    direction: [f32; 3],
    start_t: f32,
    max_t: f32,
) -> Vec<([f32; 3], f32, f32)> {
    const MAX_REPEAT_CELLS: i32 = 256;
    const SUPPORT_EPSILON: f32 = 1e-4;

    let period_abs = period.abs();
    if period_abs <= f32::EPSILON || axis > 2 {
        return Vec::new();
    }

    let width = (bounds.max[axis] - bounds.min[axis]).abs();
    if width > period_abs + SUPPORT_EPSILON {
        return Vec::new();
    }

    let axis_start = origin[axis] + direction[axis] * start_t;
    let axis_end = origin[axis] + direction[axis] * max_t;
    let ray_min = axis_start.min(axis_end);
    let ray_max = axis_start.max(axis_end);
    let cell_min = ((ray_min - bounds.max[axis]) / period_abs).floor() as i32 - 1;
    let cell_max = ((ray_max - bounds.min[axis]) / period_abs).ceil() as i32 + 1;
    if cell_max < cell_min || cell_max - cell_min > MAX_REPEAT_CELLS {
        return Vec::new();
    }

    let mut cells = Vec::new();
    for cell in cell_min..=cell_max {
        let offset_value = cell as f32 * period_abs;
        let mut offset = [0.0; 3];
        offset[axis] = offset_value;
        let mut shifted_min = bounds.min;
        let mut shifted_max = bounds.max;
        shifted_min[axis] += offset_value;
        shifted_max[axis] += offset_value;
        if let Some((entry_t, exit_t)) =
            ray_aabb_interval(origin, direction, shifted_min, shifted_max, start_t, max_t)
        {
            cells.push((offset, entry_t, exit_t));
        }
    }

    cells.sort_by(|left, right| {
        left.1
            .partial_cmp(&right.1)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    cells
}

fn ray_parameter_lipschitz_bound(direction: [f32; 3]) -> f32 {
    dot3(direction, direction).sqrt().max(1e-4)
}

fn relaxed_step_factor(previous_distance: Option<f32>, current_distance: f32) -> f32 {
    match previous_distance {
        Some(previous) if current_distance >= previous => 2.0,
        Some(previous) if current_distance >= previous * 0.5 => 1.5,
        Some(_) => 1.2,
        None => 1.25,
    }
}

fn adaptive_hit_epsilon(base: f32, travel: f32, scale: f32) -> f32 {
    base.max(travel.abs() * 0.000_01)
        .max(scale.abs() * 0.000_001)
}

fn adaptive_hit_epsilon_with_gradient(
    base: f32,
    travel: f32,
    scale: f32,
    gradient_mag: f32,
) -> f32 {
    let gradient_term = if gradient_mag > f32::EPSILON {
        1.0 / gradient_mag
    } else {
        2.0
    };
    adaptive_hit_epsilon(base, travel, scale)
        .max(base * (1.0 + scale.min(8.0) * 0.01 + gradient_term.min(4.0) * 0.02))
}

fn support_box_lower_bound(
    min: [f32; 3],
    max: [f32; 3],
    point: [f32; 3],
) -> Result<f32, QueryExecError> {
    let center = [
        (min[0] + max[0]) * 0.5,
        (min[1] + max[1]) * 0.5,
        (min[2] + max[2]) * 0.5,
    ];
    let half = [
        (max[0] - min[0]).abs() * 0.5,
        (max[1] - min[1]).abs() * 0.5,
        (max[2] - min[2]).abs() * 0.5,
    ];
    runtime_binary_f32_from_values(
        KernelValue::Vec3([
            point[0] - center[0],
            point[1] - center[1],
            point[2] - center[2],
        ]),
        KernelValue::Vec3(half),
        wr_box,
    )
}

fn support_sphere_lower_bound(center: [f32; 3], radius: f32, point: [f32; 3]) -> f32 {
    let dx = point[0] - center[0];
    let dy = point[1] - center[1];
    let dz = point[2] - center[2];
    (dx * dx + dy * dy + dz * dz).sqrt() - radius
}

fn cross3(lhs: [f32; 3], rhs: [f32; 3]) -> [f32; 3] {
    [
        lhs[1] * rhs[2] - lhs[2] * rhs[1],
        lhs[2] * rhs[0] - lhs[0] * rhs[2],
        lhs[0] * rhs[1] - lhs[1] * rhs[0],
    ]
}

pub(crate) fn normalize3(value: [f32; 3]) -> [f32; 3] {
    let len = (value[0] * value[0] + value[1] * value[1] + value[2] * value[2]).sqrt();
    if len <= f32::EPSILON {
        [0.0, 0.0, 1.0]
    } else {
        [value[0] / len, value[1] / len, value[2] / len]
    }
}

fn stable_surface_frame(position: [f32; 3], normal: [f32; 3]) -> KernelValue {
    let unit_normal = normalize3(normal);
    let world_up = [0.0, 1.0, 0.0];
    let world_right = [1.0, 0.0, 0.0];
    let tangent_seed = cross3(world_up, unit_normal);
    let tangent = if tangent_seed == [0.0, 0.0, 0.0] {
        normalize3(cross3(world_right, unit_normal))
    } else {
        normalize3(tangent_seed)
    };
    let bitangent = cross3(unit_normal, tangent);
    let inverse_translation = [
        -dot3(tangent, position),
        -dot3(bitangent, position),
        -dot3(unit_normal, position),
        1.0,
    ];
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("Transform3"),
        fields: vec![
            (
                SmolStr::new("matrix"),
                KernelValue::Mat4([
                    tangent[0],
                    tangent[1],
                    tangent[2],
                    0.0,
                    bitangent[0],
                    bitangent[1],
                    bitangent[2],
                    0.0,
                    unit_normal[0],
                    unit_normal[1],
                    unit_normal[2],
                    0.0,
                    position[0],
                    position[1],
                    position[2],
                    1.0,
                ]),
            ),
            (
                SmolStr::new("inverse"),
                KernelValue::Mat4([
                    tangent[0],
                    bitangent[0],
                    unit_normal[0],
                    0.0,
                    tangent[1],
                    bitangent[1],
                    unit_normal[1],
                    0.0,
                    tangent[2],
                    bitangent[2],
                    unit_normal[2],
                    0.0,
                    inverse_translation[0],
                    inverse_translation[1],
                    inverse_translation[2],
                    inverse_translation[3],
                ]),
            ),
        ],
    })
}

fn length_of(value: &KernelValue) -> Result<f32, QueryExecError> {
    let components = kernel_components(value, "length")?;
    Ok((components
        .iter()
        .map(|component| component * component)
        .sum::<f32>())
    .sqrt())
}

pub(crate) fn combine_medium_values(
    current: KernelValue,
    next: KernelValue,
) -> Result<KernelValue, QueryExecError> {
    let current = expect_struct_ref(&current, "Medium")?;
    let next = expect_struct_ref(&next, "Medium")?;
    let current_density = expect_struct_f32(current, "density")?;
    let current_emission = expect_struct_vec3(current, "emission")?;
    let current_anisotropy = expect_struct_f32(current, "anisotropy")?;
    let next_density = expect_struct_f32(next, "density")?;
    let next_emission = expect_struct_vec3(next, "emission")?;
    let next_anisotropy = expect_struct_f32(next, "anisotropy")?;
    let density = current_density + next_density;
    let emission = add3(current_emission, next_emission);
    let anisotropy = if density > 0.0 {
        (current_anisotropy * current_density + next_anisotropy * next_density) / density
    } else {
        0.0
    };
    Ok(medium_value(density, emission, anisotropy))
}

pub(crate) fn kernel_to_runtime(value: &KernelValue) -> Result<RuntimeValue, QueryExecError> {
    match value {
        KernelValue::Nothing => Ok(RuntimeValue::nil()),
        KernelValue::Bool(value) => Ok(RuntimeValue::from_bool(*value)),
        KernelValue::I32(value) => Ok(RuntimeValue::from_int(*value as i64)),
        KernelValue::U32(value) => Ok(RuntimeValue::from_int(*value as i64)),
        KernelValue::F32(value) => Ok(RuntimeValue::from_float(*value as f64)),
        KernelValue::Vec2([x, y]) => Ok(wr_vec2_new(
            RuntimeValue::from_float(*x as f64),
            RuntimeValue::from_float(*y as f64),
        )),
        KernelValue::Vec3([x, y, z]) => Ok(wr_vec3_new(
            RuntimeValue::from_float(*x as f64),
            RuntimeValue::from_float(*y as f64),
            RuntimeValue::from_float(*z as f64),
        )),
        KernelValue::Vec4([x, y, z, w]) => Ok(wr_vec4_new(
            RuntimeValue::from_float(*x as f64),
            RuntimeValue::from_float(*y as f64),
            RuntimeValue::from_float(*z as f64),
            RuntimeValue::from_float(*w as f64),
        )),
        KernelValue::Quat([x, y, z, w]) => Ok(wr_quat_new(
            RuntimeValue::from_float(*x as f64),
            RuntimeValue::from_float(*y as f64),
            RuntimeValue::from_float(*z as f64),
            RuntimeValue::from_float(*w as f64),
        )),
        KernelValue::Mat3(values) => Ok(wr_mat3_from_columns(
            kernel_to_runtime(&KernelValue::Vec3([values[0], values[1], values[2]]))?,
            kernel_to_runtime(&KernelValue::Vec3([values[3], values[4], values[5]]))?,
            kernel_to_runtime(&KernelValue::Vec3([values[6], values[7], values[8]]))?,
        )),
        KernelValue::Mat4(values) => Ok(wr_mat4_from_columns(
            kernel_to_runtime(&KernelValue::Vec4([
                values[0], values[1], values[2], values[3],
            ]))?,
            kernel_to_runtime(&KernelValue::Vec4([
                values[4], values[5], values[6], values[7],
            ]))?,
            kernel_to_runtime(&KernelValue::Vec4([
                values[8], values[9], values[10], values[11],
            ]))?,
            kernel_to_runtime(&KernelValue::Vec4([
                values[12], values[13], values[14], values[15],
            ]))?,
        )),
        KernelValue::Struct(value) => kernel_struct_to_runtime(value),
        KernelValue::Array(items) => {
            let list = wr_list_new_local(0);
            for item in items {
                wr_list_push(list, kernel_to_runtime(item)?);
            }
            Ok(list)
        }
        KernelValue::Capture(_)
        | KernelValue::DispatchBackend(_)
        | KernelValue::GpuBuffer(_)
        | KernelValue::GpuAtomicI32(_)
        | KernelValue::GpuAtomicU32(_) => Err(QueryExecError::Unsupported {
            message: format!("cannot convert runtime math value from {value:?}"),
        }),
    }
}

fn kernel_struct_to_runtime(value: &KernelStructValue) -> Result<RuntimeValue, QueryExecError> {
    let names = value
        .fields
        .iter()
        .map(|(name, _)| name.as_bytes().as_ptr())
        .collect::<Vec<_>>();
    let lens = value
        .fields
        .iter()
        .map(|(name, _)| name.len())
        .collect::<Vec<_>>();
    let obj = wr_class_new(
        TypeId::UserBase as u32,
        names.as_ptr(),
        lens.as_ptr(),
        names.len(),
    );
    for (index, (_, field_value)) in value.fields.iter().enumerate() {
        wr_class_set_slot(
            obj,
            std::ptr::null(),
            0,
            index,
            kernel_to_runtime(field_value)?,
        );
    }
    Ok(obj)
}

pub(crate) fn runtime_to_kernel_value(value: RuntimeValue) -> Result<KernelValue, QueryExecError> {
    match wr_type_id(value) as u32 {
        id if id == TypeId::Nil as u32 => Ok(KernelValue::Nothing),
        id if id == TypeId::Boolean as u32 => Ok(KernelValue::Bool(value.as_bool())),
        id if id == TypeId::Integer as u32 => Ok(KernelValue::I32(value.as_int() as i32)),
        id if id == TypeId::Float as u32 => Ok(KernelValue::F32(value.as_float() as f32)),
        id if id == TypeId::List as u32 => {
            let len = wr_list_len(value).as_int();
            let mut items = Vec::with_capacity(len.max(0) as usize);
            for index in 0..len {
                items.push(runtime_to_kernel_value(wr_list_get(value, index as usize))?);
            }
            Ok(KernelValue::Array(items))
        }
        id if id == TypeId::Vec2 as u32 => Ok(KernelValue::Vec2([
            component_as_f32(wr_vec_component(value, RuntimeValue::from_int(0)))?,
            component_as_f32(wr_vec_component(value, RuntimeValue::from_int(1)))?,
        ])),
        id if id == TypeId::Vec3 as u32 => Ok(KernelValue::Vec3([
            component_as_f32(wr_vec_component(value, RuntimeValue::from_int(0)))?,
            component_as_f32(wr_vec_component(value, RuntimeValue::from_int(1)))?,
            component_as_f32(wr_vec_component(value, RuntimeValue::from_int(2)))?,
        ])),
        id if id == TypeId::Vec4 as u32 => Ok(KernelValue::Vec4([
            component_as_f32(wr_vec_component(value, RuntimeValue::from_int(0)))?,
            component_as_f32(wr_vec_component(value, RuntimeValue::from_int(1)))?,
            component_as_f32(wr_vec_component(value, RuntimeValue::from_int(2)))?,
            component_as_f32(wr_vec_component(value, RuntimeValue::from_int(3)))?,
        ])),
        id if id == TypeId::Quat as u32 => Ok(KernelValue::Quat([
            component_as_f32(wr_vec_component(value, RuntimeValue::from_int(0)))?,
            component_as_f32(wr_vec_component(value, RuntimeValue::from_int(1)))?,
            component_as_f32(wr_vec_component(value, RuntimeValue::from_int(2)))?,
            component_as_f32(wr_vec_component(value, RuntimeValue::from_int(3)))?,
        ])),
        id if id == TypeId::Mat3 as u32 => runtime_to_kernel_mat3(value),
        id if id == TypeId::Mat4 as u32 => runtime_to_kernel_mat4(value),
        other => Err(QueryExecError::Unsupported {
            message: format!("runtime object conversion is not implemented for type id {other}"),
        }),
    }
}

fn runtime_to_kernel_mat3(value: RuntimeValue) -> Result<KernelValue, QueryExecError> {
    Ok(KernelValue::Mat3([
        component_as_f32(wr_mat3_component(value, RuntimeValue::from_int(0)))?,
        component_as_f32(wr_mat3_component(value, RuntimeValue::from_int(1)))?,
        component_as_f32(wr_mat3_component(value, RuntimeValue::from_int(2)))?,
        component_as_f32(wr_mat3_component(value, RuntimeValue::from_int(3)))?,
        component_as_f32(wr_mat3_component(value, RuntimeValue::from_int(4)))?,
        component_as_f32(wr_mat3_component(value, RuntimeValue::from_int(5)))?,
        component_as_f32(wr_mat3_component(value, RuntimeValue::from_int(6)))?,
        component_as_f32(wr_mat3_component(value, RuntimeValue::from_int(7)))?,
        component_as_f32(wr_mat3_component(value, RuntimeValue::from_int(8)))?,
    ]))
}

fn runtime_to_kernel_mat4(value: RuntimeValue) -> Result<KernelValue, QueryExecError> {
    Ok(KernelValue::Mat4([
        component_as_f32(wr_mat4_component(value, RuntimeValue::from_int(0)))?,
        component_as_f32(wr_mat4_component(value, RuntimeValue::from_int(1)))?,
        component_as_f32(wr_mat4_component(value, RuntimeValue::from_int(2)))?,
        component_as_f32(wr_mat4_component(value, RuntimeValue::from_int(3)))?,
        component_as_f32(wr_mat4_component(value, RuntimeValue::from_int(4)))?,
        component_as_f32(wr_mat4_component(value, RuntimeValue::from_int(5)))?,
        component_as_f32(wr_mat4_component(value, RuntimeValue::from_int(6)))?,
        component_as_f32(wr_mat4_component(value, RuntimeValue::from_int(7)))?,
        component_as_f32(wr_mat4_component(value, RuntimeValue::from_int(8)))?,
        component_as_f32(wr_mat4_component(value, RuntimeValue::from_int(9)))?,
        component_as_f32(wr_mat4_component(value, RuntimeValue::from_int(10)))?,
        component_as_f32(wr_mat4_component(value, RuntimeValue::from_int(11)))?,
        component_as_f32(wr_mat4_component(value, RuntimeValue::from_int(12)))?,
        component_as_f32(wr_mat4_component(value, RuntimeValue::from_int(13)))?,
        component_as_f32(wr_mat4_component(value, RuntimeValue::from_int(14)))?,
        component_as_f32(wr_mat4_component(value, RuntimeValue::from_int(15)))?,
    ]))
}

fn component_as_f32(value: RuntimeValue) -> Result<f32, QueryExecError> {
    if value.is_float() {
        Ok(value.as_float() as f32)
    } else {
        Ok(value.as_int() as f32)
    }
}

fn runtime_unary_builtin(
    args: &[KernelValue],
    f: extern "C" fn(RuntimeValue) -> RuntimeValue,
) -> Result<KernelValue, QueryExecError> {
    let [value] = args else {
        return Err(QueryExecError::Unsupported {
            message: "builtin expected one argument".to_string(),
        });
    };
    runtime_to_kernel_value(f(kernel_to_runtime(value)?))
}

fn runtime_binary_builtin(
    args: &[KernelValue],
    f: extern "C" fn(RuntimeValue, RuntimeValue) -> RuntimeValue,
) -> Result<KernelValue, QueryExecError> {
    let [lhs, rhs] = args else {
        return Err(QueryExecError::Unsupported {
            message: "builtin expected two arguments".to_string(),
        });
    };
    runtime_to_kernel_value(f(kernel_to_runtime(lhs)?, kernel_to_runtime(rhs)?))
}

fn runtime_ternary_builtin(
    args: &[KernelValue],
    f: extern "C" fn(RuntimeValue, RuntimeValue, RuntimeValue) -> RuntimeValue,
) -> Result<KernelValue, QueryExecError> {
    let [a, b, c] = args else {
        return Err(QueryExecError::Unsupported {
            message: "builtin expected three arguments".to_string(),
        });
    };
    runtime_to_kernel_value(f(
        kernel_to_runtime(a)?,
        kernel_to_runtime(b)?,
        kernel_to_runtime(c)?,
    ))
}

fn runtime_binary_value(
    lhs: KernelValue,
    rhs: KernelValue,
    f: extern "C" fn(RuntimeValue, RuntimeValue) -> RuntimeValue,
) -> Result<KernelValue, QueryExecError> {
    runtime_to_kernel_value(f(kernel_to_runtime(&lhs)?, kernel_to_runtime(&rhs)?))
}

fn runtime_binary_f32(
    lhs: f32,
    rhs: f32,
    f: extern "C" fn(RuntimeValue, RuntimeValue) -> RuntimeValue,
) -> Result<f32, QueryExecError> {
    match runtime_to_kernel_value(f(
        RuntimeValue::from_float(lhs as f64),
        RuntimeValue::from_float(rhs as f64),
    ))? {
        KernelValue::F32(value) => Ok(value),
        value => Err(QueryExecError::TypeMismatch {
            expected: "F32".to_string(),
            found: format!("{value:?}"),
        }),
    }
}

fn runtime_binary_f32_from_values(
    lhs: KernelValue,
    rhs: KernelValue,
    f: extern "C" fn(RuntimeValue, RuntimeValue) -> RuntimeValue,
) -> Result<f32, QueryExecError> {
    match runtime_binary_value(lhs, rhs, f)? {
        KernelValue::F32(value) => Ok(value),
        value => Err(QueryExecError::TypeMismatch {
            expected: "F32".to_string(),
            found: format!("{value:?}"),
        }),
    }
}

fn runtime_ternary_f32_from_values(
    a: KernelValue,
    b: KernelValue,
    c: KernelValue,
    f: extern "C" fn(RuntimeValue, RuntimeValue, RuntimeValue) -> RuntimeValue,
) -> Result<f32, QueryExecError> {
    match runtime_to_kernel_value(f(
        kernel_to_runtime(&a)?,
        kernel_to_runtime(&b)?,
        kernel_to_runtime(&c)?,
    ))? {
        KernelValue::F32(value) => Ok(value),
        value => Err(QueryExecError::TypeMismatch {
            expected: "F32".to_string(),
            found: format!("{value:?}"),
        }),
    }
}

fn runtime_ternary_f32(
    a: f32,
    b: f32,
    c: f32,
    f: extern "C" fn(RuntimeValue, RuntimeValue, RuntimeValue) -> RuntimeValue,
) -> Result<f32, QueryExecError> {
    runtime_ternary_f32_from_values(
        KernelValue::F32(a),
        KernelValue::F32(b),
        KernelValue::F32(c),
        f,
    )
}

fn runtime_quaternary_f32(
    a: KernelValue,
    b: KernelValue,
    c: KernelValue,
    d: KernelValue,
    f: extern "C" fn(RuntimeValue, RuntimeValue, RuntimeValue, RuntimeValue) -> RuntimeValue,
) -> Result<f32, QueryExecError> {
    match runtime_to_kernel_value(f(
        kernel_to_runtime(&a)?,
        kernel_to_runtime(&b)?,
        kernel_to_runtime(&c)?,
        kernel_to_runtime(&d)?,
    ))? {
        KernelValue::F32(value) => Ok(value),
        value => Err(QueryExecError::TypeMismatch {
            expected: "F32".to_string(),
            found: format!("{value:?}"),
        }),
    }
}

fn expect_array<'a>(
    value: Option<&'a KernelValue>,
    label: &str,
) -> Result<&'a [KernelValue], QueryExecError> {
    match value {
        Some(KernelValue::Array(items)) => Ok(items.as_slice()),
        Some(other) => Err(QueryExecError::TypeMismatch {
            expected: format!("{label}: Array"),
            found: format!("{other:?}"),
        }),
        None => Err(QueryExecError::TypeMismatch {
            expected: format!("{label}: Array"),
            found: "missing value".to_string(),
        }),
    }
}

fn expect_struct<'a>(
    value: Option<&'a KernelValue>,
    name: &str,
) -> Result<&'a KernelStructValue, QueryExecError> {
    match value {
        Some(KernelValue::Struct(value)) if value.name.as_str() == name => Ok(value),
        Some(other) => Err(QueryExecError::TypeMismatch {
            expected: format!("{name}"),
            found: format!("{other:?}"),
        }),
        None => Err(QueryExecError::TypeMismatch {
            expected: name.to_string(),
            found: "missing value".to_string(),
        }),
    }
}

fn expect_struct_ref<'a>(
    value: &'a KernelValue,
    name: &str,
) -> Result<&'a KernelStructValue, QueryExecError> {
    expect_struct(Some(value), name)
}

fn expect_vec3(value: Option<&KernelValue>, label: &str) -> Result<[f32; 3], QueryExecError> {
    match value {
        Some(KernelValue::Vec3(value)) => Ok(*value),
        Some(other) => Err(QueryExecError::TypeMismatch {
            expected: format!("{label}: Vec3"),
            found: format!("{other:?}"),
        }),
        None => Err(QueryExecError::TypeMismatch {
            expected: format!("{label}: Vec3"),
            found: "missing value".to_string(),
        }),
    }
}

fn expect_f32(value: Option<&KernelValue>, label: &str) -> Result<f32, QueryExecError> {
    match value {
        Some(KernelValue::F32(value)) => Ok(*value),
        Some(KernelValue::I32(value)) => Ok(*value as f32),
        Some(KernelValue::U32(value)) => Ok(*value as f32),
        Some(other) => Err(QueryExecError::TypeMismatch {
            expected: format!("{label}: F32"),
            found: format!("{other:?}"),
        }),
        None => Err(QueryExecError::TypeMismatch {
            expected: format!("{label}: F32"),
            found: "missing value".to_string(),
        }),
    }
}

fn expect_i32(value: Option<&KernelValue>, label: &str) -> Result<i32, QueryExecError> {
    match value {
        Some(KernelValue::I32(value)) => Ok(*value),
        Some(KernelValue::U32(value)) => Ok(*value as i32),
        Some(other) => Err(QueryExecError::TypeMismatch {
            expected: format!("{label}: I32"),
            found: format!("{other:?}"),
        }),
        None => Err(QueryExecError::TypeMismatch {
            expected: format!("{label}: I32"),
            found: "missing value".to_string(),
        }),
    }
}

fn expect_abs_scalar(value: &KernelValue) -> Result<f32, QueryExecError> {
    match value {
        KernelValue::F32(value) => Ok(value.abs()),
        KernelValue::I32(value) => Ok((*value as f32).abs()),
        KernelValue::U32(value) => Ok(*value as f32),
        other => Err(QueryExecError::TypeMismatch {
            expected: "scalar".to_string(),
            found: format!("{other:?}"),
        }),
    }
}

fn expect_struct_bool(value: &KernelStructValue, field: &str) -> Result<bool, QueryExecError> {
    match struct_field(value, field)? {
        KernelValue::Bool(value) => Ok(*value),
        other => Err(QueryExecError::TypeMismatch {
            expected: format!("{field}: Bool"),
            found: format!("{other:?}"),
        }),
    }
}

fn expect_struct_f32(value: &KernelStructValue, field: &str) -> Result<f32, QueryExecError> {
    expect_f32(Some(struct_field(value, field)?), field)
}

fn expect_struct_i32(value: &KernelStructValue, field: &str) -> Result<i32, QueryExecError> {
    expect_i32(Some(struct_field(value, field)?), field)
}

fn expect_struct_u32(value: &KernelStructValue, field: &str) -> Result<u32, QueryExecError> {
    match struct_field(value, field)? {
        KernelValue::U32(value) => Ok(*value),
        KernelValue::I32(value) if *value >= 0 => Ok(*value as u32),
        other => Err(QueryExecError::TypeMismatch {
            expected: format!("{field}: U32"),
            found: format!("{other:?}"),
        }),
    }
}

fn expect_struct_vec3(value: &KernelStructValue, field: &str) -> Result<[f32; 3], QueryExecError> {
    match struct_field(value, field)? {
        KernelValue::Vec3(value) => Ok(*value),
        other => Err(QueryExecError::TypeMismatch {
            expected: format!("{field}: Vec3"),
            found: format!("{other:?}"),
        }),
    }
}

fn expect_struct_mat4(value: &KernelStructValue, field: &str) -> Result<[f32; 16], QueryExecError> {
    match struct_field(value, field)? {
        KernelValue::Mat4(value) => Ok(*value),
        other => Err(QueryExecError::TypeMismatch {
            expected: format!("{field}: Mat4"),
            found: format!("{other:?}"),
        }),
    }
}

fn struct_field<'a>(
    value: &'a KernelStructValue,
    field: &str,
) -> Result<&'a KernelValue, QueryExecError> {
    value
        .fields
        .iter()
        .find(|(name, _)| name.as_str() == field)
        .map(|(_, value)| value)
        .ok_or_else(|| QueryExecError::Unsupported {
            message: format!("missing struct field '{field}' on '{}'", value.name),
        })
}

fn expect_scalar_as_i32(args: &[KernelValue], name: &str) -> Result<i32, QueryExecError> {
    expect_i32(args.first(), name)
}

fn expect_scalar_as_u32(args: &[KernelValue], name: &str) -> Result<u32, QueryExecError> {
    match args.first() {
        Some(KernelValue::U32(value)) => Ok(*value),
        Some(KernelValue::I32(value)) if *value >= 0 => Ok(*value as u32),
        Some(other) => Err(QueryExecError::TypeMismatch {
            expected: format!("{name}: U32"),
            found: format!("{other:?}"),
        }),
        None => Err(QueryExecError::TypeMismatch {
            expected: format!("{name}: U32"),
            found: "missing value".to_string(),
        }),
    }
}

fn expect_scalar_as_f32(args: &[KernelValue], name: &str) -> Result<f32, QueryExecError> {
    expect_f32(args.first(), name)
}

fn expect_scalar_as_f32_arg(
    args: &[KernelValue],
    index: usize,
    name: &str,
) -> Result<f32, QueryExecError> {
    expect_f32(args.get(index), name)
}
