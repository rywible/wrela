use crate::kernel::interp::KernelBatchQueryTrace;
use crate::kernel::{
    KernelBatchItemContract, KernelBatchQueryPlan, KernelCaptureQueryPlan, KernelWorldQueryPlan,
};
use crate::kernel::{KernelStructValue, KernelValue};
use crate::kernel::{
    KernelValidationError, validate_batch_query_plan, validate_capture_query_plan,
    validate_world_query_plan,
};
use crate::query_exec::capture::{self, CaptureQueryBackend, execute_batch_item_contract};
use crate::query_exec::cpu::{DirectQueryOps, default_hit, default_surface, medium_value};
use crate::query_exec::world::{
    WorldDistanceBackend, WorldMediumBackend, WorldNormalBackend, WorldQueryBackend,
    WorldRadianceBackend, WorldSurfaceBackend, WorldTraceBackend, execute_world_distance,
    execute_world_medium, execute_world_normal, execute_world_radiance, execute_world_ray,
    execute_world_surface,
};
use crate::query_exec::{QueryExecContext, QueryExecError, QueryExecutionObservability};
use crate::execution_policy::QueryExecutionPolicy;
use crate::query_plan::{
    CaptureKind, PruningStrategy, WorldQueryKind, world_query_kind_for_contract_id,
};
use crate::query_solver::{RaySolverFallbackReason, RaySolverMethod, RaySolverPlan};
use crate::scene_ir::{
    ShapeLeafRef, ShapeMergeProvenancePolicy, ShapeNode, ShapeProvenanceExpr,
    ShapeSubtractProvenancePolicy,
};
use smol_str::SmolStr;

trait QueryContractRuntime {
    fn resolve_field_or_shape_capture(
        &self,
        capture: Option<&KernelValue>,
    ) -> Result<SmolStr, QueryExecError>;
    fn resolve_shape_capture(
        &self,
        capture: Option<&KernelValue>,
    ) -> Result<SmolStr, QueryExecError>;
    fn resolve_region_capture(
        &self,
        capture: Option<&KernelValue>,
    ) -> Result<SmolStr, QueryExecError>;
    fn validate_world_domain(
        &self,
        capture: &SmolStr,
        domain: &KernelStructValue,
        query_name: &'static str,
    ) -> Result<i32, QueryExecError>;
    fn resolve_world_shapes(
        &self,
        capture: &SmolStr,
        detail: i32,
        root_shape_id: Option<u32>,
    ) -> Result<Vec<SmolStr>, QueryExecError>;
    fn world_domain_flag_enabled(
        &self,
        domain: &KernelStructValue,
        kind: WorldQueryKind,
    ) -> Result<bool, QueryExecError>;
    fn capture_distance(
        &self,
        capture: &SmolStr,
        point: [f32; 3],
        capture_kind: crate::query_plan::CaptureKind,
    ) -> Result<f32, QueryExecError>;
    fn capture_normal(
        &self,
        capture: &SmolStr,
        point: [f32; 3],
        capture_kind: crate::query_plan::CaptureKind,
    ) -> Result<[f32; 3], QueryExecError>;
    fn support_summary(
        &self,
        capture: &SmolStr,
        capture_kind: crate::query_plan::CaptureKind,
    ) -> Result<KernelValue, QueryExecError>;
    fn support_summary_for_region(
        &self,
        capture: &SmolStr,
        detail: i32,
    ) -> Result<KernelValue, QueryExecError>;
    fn eval_shape_distance(&self, shape: &SmolStr, point: [f32; 3]) -> Result<f32, QueryExecError>;
    fn eval_shape_support_lower_bound(
        &self,
        shape: &SmolStr,
        point: [f32; 3],
    ) -> Result<Option<f32>, QueryExecError>;
    fn trace_shape(
        &self,
        shape: &SmolStr,
        origin: [f32; 3],
        direction: [f32; 3],
        max_distance: f32,
        min_step: f32,
        hit_epsilon: f32,
        max_steps: i32,
    ) -> Result<KernelValue, QueryExecError>;
    fn surface_at(
        &self,
        shape: &SmolStr,
        hit: &KernelStructValue,
    ) -> Result<KernelValue, QueryExecError>;
    fn radiance_at(
        &self,
        shape: &SmolStr,
        point: [f32; 3],
        direction: [f32; 3],
    ) -> Result<KernelValue, QueryExecError>;
    fn medium_at(&self, shape: &SmolStr, point: [f32; 3]) -> Result<KernelValue, QueryExecError>;
    fn snapshot_observability(&self) -> QueryExecutionObservability;
    fn note_candidate_count(&self, count: u32);
    fn note_support_pruned_candidates(&self, count: u32);
    fn note_dispatch(&self);
    fn note_batch_dispatch_shape(&self, items: u32, world_batch: bool);
    fn note_batch_execution_mode(&self, semantic_pruned: bool);
    fn note_solver_plan(&self, plan: &RaySolverPlan);
    fn note_solver_dense_fallback_reasons(&self, reasons: &[RaySolverFallbackReason]);
    fn note_solver_support_rejection(&self);
    fn note_solver_lipschitz_step(&self);
    fn note_hit_result(&self, hit: bool, steps: u32);
    fn note_contract_validation_failure(&self);
}

#[derive(Debug, Clone)]
struct VirtualGpuShapeWinner {
    distance: f32,
    feature_id: u32,
    leaf: Option<ShapeLeafRef>,
}

struct VirtualGpuRuntime<'a> {
    ops: DirectQueryOps<'a>,
}

impl<'a> VirtualGpuRuntime<'a> {
    fn new(ctx: &'a QueryExecContext) -> Self {
        Self::new_with_snapshot(ctx, None)
    }

    fn new_with_snapshot(
        ctx: &'a QueryExecContext,
        snapshot: Option<&crate::world_identity::WorldSnapshotHandle>,
    ) -> Self {
        Self {
            ops: DirectQueryOps::new_with_snapshot(ctx, snapshot),
        }
    }

    fn eval_shape_distance_node(
        &self,
        node: &ShapeNode,
        point: [f32; 3],
    ) -> Result<f32, QueryExecError> {
        self.ops.note_branch_visit();
        match node {
            ShapeNode::Use { target } => self.eval_shape_distance(target, point),
            ShapeNode::Leaf(leaf) => self.ops.eval_field_distance(&leaf.field, point),
            ShapeNode::Union { items } => {
                let mut current = 1_000_000.0f32;
                for item in items {
                    current = current.min(self.eval_shape_distance_node(item, point)?);
                }
                Ok(current)
            }
            ShapeNode::Intersection { items } => {
                let mut iter = items.iter();
                let Some(first) = iter.next() else {
                    return Ok(1_000_000.0);
                };
                let mut current = self.eval_shape_distance_node(first, point)?;
                for item in iter {
                    current = current.max(self.eval_shape_distance_node(item, point)?);
                }
                Ok(current)
            }
            ShapeNode::Subtract { left, right } => Ok(self
                .eval_shape_distance_node(left, point)?
                .max(-self.eval_shape_distance_node(right, point)?)),
        }
    }

    fn eval_shape_winner(
        &self,
        shape: &SmolStr,
        point: [f32; 3],
    ) -> Result<VirtualGpuShapeWinner, QueryExecError> {
        let scene = self.ops.shape_scene(shape)?;
        self.eval_shape_winner_node(shape, &scene.root, scene.provenance.as_ref(), point)
    }

    fn eval_shape_winner_node(
        &self,
        scene_name: &SmolStr,
        node: &ShapeNode,
        provenance: Option<&ShapeProvenanceExpr>,
        point: [f32; 3],
    ) -> Result<VirtualGpuShapeWinner, QueryExecError> {
        self.ops.note_branch_visit();
        match node {
            ShapeNode::Use { target } => {
                let scene = self.ops.shape_scene(target)?;
                self.eval_shape_winner_node(target, &scene.root, scene.provenance.as_ref(), point)
            }
            ShapeNode::Leaf(leaf) => Ok(VirtualGpuShapeWinner {
                distance: self.ops.eval_field_distance(&leaf.field, point)?,
                feature_id: leaf.feature_id,
                leaf: Some(ShapeLeafRef {
                    scene: scene_name.clone(),
                    leaf: leaf.id,
                }),
            }),
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
                let Some((index, first)) = iter.next() else {
                    return Ok(VirtualGpuShapeWinner {
                        distance: 1_000_000.0,
                        feature_id: 0,
                        leaf: None,
                    });
                };
                let mut current = self.eval_shape_winner_node(
                    scene_name,
                    first,
                    provenance_items.and_then(|items| items.get(index)),
                    point,
                )?;
                for (index, item) in iter {
                    let next = self.eval_shape_winner_node(
                        scene_name,
                        item,
                        provenance_items.and_then(|items| items.get(index)),
                        point,
                    )?;
                    match merge_policy {
                        ShapeMergeProvenancePolicy::Ordered => {
                            current.distance = current.distance.min(next.distance);
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
                let Some((index, first)) = iter.next() else {
                    return Ok(VirtualGpuShapeWinner {
                        distance: 1_000_000.0,
                        feature_id: 0,
                        leaf: None,
                    });
                };
                let mut current = self.eval_shape_winner_node(
                    scene_name,
                    first,
                    provenance_items.and_then(|items| items.get(index)),
                    point,
                )?;
                for (index, item) in iter {
                    let next = self.eval_shape_winner_node(
                        scene_name,
                        item,
                        provenance_items.and_then(|items| items.get(index)),
                        point,
                    )?;
                    match merge_policy {
                        ShapeMergeProvenancePolicy::Ordered => {
                            current.distance = current.distance.max(next.distance);
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
                    Ok(VirtualGpuShapeWinner {
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

    fn eval_shape_radiance_node(
        &self,
        node: &ShapeNode,
        point: [f32; 3],
        direction: [f32; 3],
    ) -> Result<[f32; 3], QueryExecError> {
        match node {
            ShapeNode::Use { target } => {
                let scene = self.ops.shape_scene(target)?;
                self.eval_shape_radiance_node(&scene.root, point, direction)
            }
            ShapeNode::Leaf(leaf) => {
                let Some(radiance) = &leaf.radiance else {
                    return Ok([0.0, 0.0, 0.0]);
                };
                let local_frame = self.ops.eval_field_local_frame(&leaf.field, point)?;
                let value = self.ops.execute_portable_function(
                    radiance,
                    vec![
                        KernelValue::Vec3(local_frame.point),
                        KernelValue::Vec3(direction),
                        KernelValue::U32(leaf.feature_id),
                    ],
                )?;
                match value {
                    KernelValue::Vec3(value) => Ok(value),
                    other => Err(QueryExecError::TypeMismatch {
                        expected: "Vec3".to_string(),
                        found: format!("{other:?}"),
                    }),
                }
            }
            ShapeNode::Union { items } | ShapeNode::Intersection { items } => {
                let mut total = [0.0, 0.0, 0.0];
                for item in items {
                    let next = self.eval_shape_radiance_node(item, point, direction)?;
                    total = [total[0] + next[0], total[1] + next[1], total[2] + next[2]];
                }
                Ok(total)
            }
            ShapeNode::Subtract { left, right } => {
                let left = self.eval_shape_radiance_node(left, point, direction)?;
                let right = self.eval_shape_radiance_node(right, point, direction)?;
                Ok([left[0] + right[0], left[1] + right[1], left[2] + right[2]])
            }
        }
    }

    fn eval_shape_medium_node(
        &self,
        node: &ShapeNode,
        point: [f32; 3],
    ) -> Result<KernelValue, QueryExecError> {
        match node {
            ShapeNode::Use { target } => {
                let scene = self.ops.shape_scene(target)?;
                self.eval_shape_medium_node(&scene.root, point)
            }
            ShapeNode::Leaf(leaf) => {
                let Some(volume) = &leaf.volume else {
                    return Ok(crate::query_exec::cpu::default_medium());
                };
                let local_frame = self.ops.eval_field_local_frame(&leaf.field, point)?;
                let local_surface_distance = self
                    .ops
                    .eval_field_node(local_frame.node, local_frame.point)?;
                self.ops.execute_portable_function(
                    volume,
                    vec![
                        KernelValue::Vec3(local_frame.point),
                        KernelValue::F32(local_surface_distance),
                    ],
                )
            }
            ShapeNode::Union { items } | ShapeNode::Intersection { items } => {
                let mut total = crate::query_exec::cpu::default_medium();
                for item in items {
                    total = crate::query_exec::cpu::combine_medium_values(
                        total,
                        self.eval_shape_medium_node(item, point)?,
                    )?;
                }
                Ok(total)
            }
            ShapeNode::Subtract { left, right } => crate::query_exec::cpu::combine_medium_values(
                self.eval_shape_medium_node(left, point)?,
                self.eval_shape_medium_node(right, point)?,
            ),
        }
    }
}

impl QueryContractRuntime for VirtualGpuRuntime<'_> {
    fn resolve_field_or_shape_capture(
        &self,
        capture: Option<&KernelValue>,
    ) -> Result<SmolStr, QueryExecError> {
        self.ops.resolve_field_or_shape_capture(capture)
    }

    fn resolve_shape_capture(
        &self,
        capture: Option<&KernelValue>,
    ) -> Result<SmolStr, QueryExecError> {
        self.ops.resolve_shape_capture(capture)
    }

    fn resolve_region_capture(
        &self,
        capture: Option<&KernelValue>,
    ) -> Result<SmolStr, QueryExecError> {
        self.ops.resolve_region_capture(capture)
    }

    fn validate_world_domain(
        &self,
        capture: &SmolStr,
        domain: &KernelStructValue,
        query_name: &'static str,
    ) -> Result<i32, QueryExecError> {
        self.ops.validate_world_domain(capture, domain, query_name)
    }

    fn resolve_world_shapes(
        &self,
        capture: &SmolStr,
        detail: i32,
        root_shape_id: Option<u32>,
    ) -> Result<Vec<SmolStr>, QueryExecError> {
        self.ops
            .resolve_world_shapes(capture, detail, root_shape_id)
    }

    fn world_domain_flag_enabled(
        &self,
        domain: &KernelStructValue,
        kind: WorldQueryKind,
    ) -> Result<bool, QueryExecError> {
        self.ops.world_domain_flag_enabled(domain, kind)
    }

    fn capture_distance(
        &self,
        capture: &SmolStr,
        point: [f32; 3],
        capture_kind: crate::query_plan::CaptureKind,
    ) -> Result<f32, QueryExecError> {
        match capture_kind {
            crate::query_plan::CaptureKind::Field => self.ops.eval_field_distance(capture, point),
            crate::query_plan::CaptureKind::Shape => self.eval_shape_distance(capture, point),
            crate::query_plan::CaptureKind::Region => Err(QueryExecError::Unsupported {
                message: "region captures are only valid for world queries".to_string(),
            }),
        }
    }

    fn capture_normal(
        &self,
        capture: &SmolStr,
        point: [f32; 3],
        capture_kind: crate::query_plan::CaptureKind,
    ) -> Result<[f32; 3], QueryExecError> {
        match capture_kind {
            crate::query_plan::CaptureKind::Field => self.ops.eval_field_normal(capture, point),
            crate::query_plan::CaptureKind::Shape => {
                let eps = 0.001f32;
                let dx = self.eval_shape_distance(capture, [point[0] + eps, point[1], point[2]])?
                    - self.eval_shape_distance(capture, [point[0] - eps, point[1], point[2]])?;
                let dy = self.eval_shape_distance(capture, [point[0], point[1] + eps, point[2]])?
                    - self.eval_shape_distance(capture, [point[0], point[1] - eps, point[2]])?;
                let dz = self.eval_shape_distance(capture, [point[0], point[1], point[2] + eps])?
                    - self.eval_shape_distance(capture, [point[0], point[1], point[2] - eps])?;
                Ok(crate::query_exec::cpu::normalize3([dx, dy, dz]))
            }
            crate::query_plan::CaptureKind::Region => Err(QueryExecError::Unsupported {
                message: "region captures are only valid for world queries".to_string(),
            }),
        }
    }

    fn support_summary(
        &self,
        capture: &SmolStr,
        capture_kind: crate::query_plan::CaptureKind,
    ) -> Result<KernelValue, QueryExecError> {
        self.ops.support_summary_for_capture(capture, capture_kind)
    }

    fn support_summary_for_region(
        &self,
        capture: &SmolStr,
        detail: i32,
    ) -> Result<KernelValue, QueryExecError> {
        self.ops.support_summary_for_region(capture, detail)
    }

    fn eval_shape_distance(&self, shape: &SmolStr, point: [f32; 3]) -> Result<f32, QueryExecError> {
        let scene = self.ops.shape_scene(shape)?;
        self.eval_shape_distance_node(&scene.root, point)
    }

    fn eval_shape_support_lower_bound(
        &self,
        shape: &SmolStr,
        point: [f32; 3],
    ) -> Result<Option<f32>, QueryExecError> {
        self.ops.eval_shape_support_lower_bound(shape, point)
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
        let mut travel = 0.0f32;
        let mut steps = 0i32;
        while steps < max_steps && travel <= max_distance {
            self.ops.note_trace_step();
            let point = [
                origin[0] + direction[0] * travel,
                origin[1] + direction[1] * travel,
                origin[2] + direction[2] * travel,
            ];
            let distance = self.eval_shape_distance(shape, point)?;
            if distance <= hit_epsilon {
                let normal =
                    self.capture_normal(shape, point, crate::query_plan::CaptureKind::Shape)?;
                let winner = self.eval_shape_winner(shape, point)?;
                let feature_id = winner.feature_id;
                let (payload, local_position, local_normal, instance_id, repeat_id) = winner
                    .leaf
                    .as_ref()
                    .and_then(|leaf_ref| {
                        self.ops
                            .context()
                            .shape_leaf(&leaf_ref.scene, leaf_ref.leaf)
                            .map(|leaf| (leaf_ref, leaf))
                    })
                    .map(|(_, leaf)| {
                        let local_frame = self.ops.eval_field_local_frame(&leaf.field, point)?;
                        let local_normal = self.ops.eval_field_local_normal(&leaf.field, point)?;
                        let payload = self
                            .ops
                            .eval_payload_body(&leaf.payload)
                            .unwrap_or_else(|_| crate::query_exec::cpu::default_payload());
                        Ok::<_, QueryExecError>((
                            payload,
                            local_frame.point,
                            local_normal,
                            local_frame.instance_id,
                            local_frame.repeat_id,
                        ))
                    })
                    .transpose()?
                    .unwrap_or_else(|| {
                        (
                            crate::query_exec::cpu::default_payload(),
                            point,
                            normal,
                            0,
                            0,
                        )
                    });
                return Ok(crate::query_exec::cpu::hit_value(
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
                    crate::query_exec::stable_shape_capture_id(shape),
                    payload,
                ));
            }
            travel += distance.max(min_step);
            steps += 1;
        }
        Ok(default_hit(origin))
    }

    fn surface_at(
        &self,
        shape: &SmolStr,
        hit: &KernelStructValue,
    ) -> Result<KernelValue, QueryExecError> {
        let feature_id = expect_struct_u32(hit, "feature_id")?;
        let Some(leaf) = self
            .ops
            .context()
            .shape_leaf_ref(shape, feature_id)
            .and_then(|leaf_ref| {
                self.ops
                    .context()
                    .shape_leaf(&leaf_ref.scene, leaf_ref.leaf)
            })
        else {
            return Ok(default_surface());
        };
        self.ops
            .execute_portable_function(&leaf.material, vec![KernelValue::Struct(hit.clone())])
    }

    fn radiance_at(
        &self,
        shape: &SmolStr,
        point: [f32; 3],
        direction: [f32; 3],
    ) -> Result<KernelValue, QueryExecError> {
        let scene = self.ops.shape_scene(shape)?;
        Ok(KernelValue::Vec3(self.eval_shape_radiance_node(
            &scene.root,
            point,
            direction,
        )?))
    }

    fn medium_at(&self, shape: &SmolStr, point: [f32; 3]) -> Result<KernelValue, QueryExecError> {
        let scene = self.ops.shape_scene(shape)?;
        self.eval_shape_medium_node(&scene.root, point)
    }

    fn snapshot_observability(&self) -> QueryExecutionObservability {
        self.ops.snapshot_observability()
    }

    fn note_candidate_count(&self, count: u32) {
        self.ops.note_candidate_count(count);
    }

    fn note_support_pruned_candidates(&self, count: u32) {
        self.ops.note_support_pruned_candidates(count);
    }

    fn note_dispatch(&self) {
        self.ops.note_dispatch();
    }

    fn note_batch_dispatch_shape(&self, items: u32, world_batch: bool) {
        self.ops.note_batch_dispatch_shape(items, world_batch);
    }

    fn note_batch_execution_mode(&self, semantic_pruned: bool) {
        self.ops.note_batch_execution_mode(semantic_pruned);
    }

    fn note_solver_plan(&self, plan: &RaySolverPlan) {
        self.ops.note_solver_plan(plan);
    }

    fn note_solver_dense_fallback_reasons(&self, reasons: &[RaySolverFallbackReason]) {
        self.ops.note_solver_dense_fallback_reasons(reasons);
    }

    fn note_solver_support_rejection(&self) {
        self.ops.note_solver_support_rejection();
    }

    fn note_solver_lipschitz_step(&self) {
        self.ops.note_solver_lipschitz_step();
    }

    fn note_hit_result(&self, hit: bool, steps: u32) {
        self.ops.note_hit_result(hit, steps);
    }

    fn note_contract_validation_failure(&self) {
        self.ops.note_contract_validation_failure();
    }
}

fn validation_error(label: &str, errors: Vec<KernelValidationError>) -> QueryExecError {
    let messages = errors
        .into_iter()
        .map(|error| error.message)
        .collect::<Vec<_>>()
        .join("; ");
    QueryExecError::Unsupported {
        message: format!("virtual GPU contract validation failed for {label}: {messages}"),
    }
}

pub(crate) fn execute_capture_query(
    ctx: &QueryExecContext,
    plan: &KernelCaptureQueryPlan,
    args: &[KernelValue],
) -> Result<KernelValue, QueryExecError> {
    execute_capture_query_with_observability(ctx, plan, args).map(|(value, _)| value)
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
    snapshot: Option<&crate::world_identity::WorldSnapshotHandle>,
    plan: &KernelCaptureQueryPlan,
    args: &[KernelValue],
) -> Result<(KernelValue, QueryExecutionObservability), QueryExecError> {
    let runtime = VirtualGpuRuntime::new_with_snapshot(ctx, snapshot);
    runtime.note_dispatch();
    if let Err(errors) = validate_capture_query_plan(plan) {
        runtime.note_contract_validation_failure();
        return Err(validation_error("capture query", errors));
    }
    let value = capture::execute_capture_query(
        &VirtualGpuCaptureBackend { runtime: &runtime },
        plan,
        args,
    )?;
    Ok((value, runtime.snapshot_observability()))
}

pub(crate) fn execute_world_query(
    ctx: &QueryExecContext,
    plan: &KernelWorldQueryPlan,
    args: &[KernelValue],
) -> Result<KernelValue, QueryExecError> {
    let policy = QueryExecutionPolicy::conservative(plan.backend, None);
    execute_world_query_with_policy_with_observability(ctx, &policy, plan, args)
        .map(|(value, _)| value)
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
    snapshot: Option<&crate::world_identity::WorldSnapshotHandle>,
    plan: &KernelWorldQueryPlan,
    args: &[KernelValue],
) -> Result<(KernelValue, QueryExecutionObservability), QueryExecError> {
    let policy = QueryExecutionPolicy::conservative(plan.backend, None);
    execute_world_query_with_policy_with_snapshot_observability(ctx, snapshot, &policy, plan, args)
}

pub(crate) fn execute_world_query_with_policy_with_snapshot_observability(
    ctx: &QueryExecContext,
    snapshot: Option<&crate::world_identity::WorldSnapshotHandle>,
    _policy: &QueryExecutionPolicy,
    plan: &KernelWorldQueryPlan,
    args: &[KernelValue],
) -> Result<(KernelValue, QueryExecutionObservability), QueryExecError> {
    let evaluator = VirtualGpuDirectQueryEvaluator::new_with_snapshot(ctx, snapshot);
    evaluator.runtime.note_dispatch();
    if let Err(errors) = validate_world_query_plan(plan) {
        evaluator.runtime.note_contract_validation_failure();
        return Err(validation_error("world query", errors));
    }
    let value = evaluator.execute_world_query(plan, args)?;
    Ok((value, evaluator.runtime.snapshot_observability()))
}

struct VirtualGpuDirectQueryEvaluator<'a> {
    runtime: Box<dyn QueryContractRuntime + 'a>,
}

struct VirtualGpuCaptureBackend<'a> {
    runtime: &'a dyn QueryContractRuntime,
}

impl<'a> VirtualGpuDirectQueryEvaluator<'a> {
    fn new(ctx: &'a QueryExecContext) -> Self {
        Self::new_with_snapshot(ctx, None)
    }

    fn new_with_snapshot(
        ctx: &'a QueryExecContext,
        snapshot: Option<&crate::world_identity::WorldSnapshotHandle>,
    ) -> Self {
        Self {
            runtime: Box::new(VirtualGpuRuntime::new_with_snapshot(ctx, snapshot)),
        }
    }

    fn execute_world_query(
        &self,
        plan: &KernelWorldQueryPlan,
        args: &[KernelValue],
    ) -> Result<KernelValue, QueryExecError> {
        let kind = world_query_kind_for_contract_id(plan.contract_id).ok_or_else(|| {
            QueryExecError::Unsupported {
                message: format!(
                    "missing world query contract '{}'",
                    plan.contract_id.as_str()
                ),
            }
        })?;
        let capture = self.runtime.resolve_region_capture(args.first())?;
        let domain = expect_struct_ref_arg(args.get(1), "SceneDomain")?;
        let detail = self.runtime.validate_world_domain(
            &capture,
            domain,
            crate::query_exec::world::world_query_semantics_for_contract(plan.contract_id)
                .query_name,
        )?;
        match kind {
            WorldQueryKind::Distance => {
                let point = expect_vec3_arg(args.get(2), "point")?;
                let mut backend = VirtualGpuWorldDistanceBackend {
                    runtime: self.runtime.as_ref(),
                    capture: &capture,
                    detail,
                    point,
                    result: 1_000_000.0,
                };
                execute_world_distance(&mut backend)?;
                Ok(KernelValue::F32(backend.result))
            }
            WorldQueryKind::Normal => {
                let point = expect_vec3_arg(args.get(2), "point")?;
                let mut backend = VirtualGpuWorldNormalBackend {
                    runtime: self.runtime.as_ref(),
                    capture: &capture,
                    detail,
                    point,
                };
                Ok(KernelValue::Vec3(execute_world_normal(&mut backend)?))
            }
            WorldQueryKind::SupportSummary => {
                self.runtime.support_summary_for_region(&capture, detail)
            }
            WorldQueryKind::Nearest | WorldQueryKind::Trace => {
                let ray = expect_struct_ref_arg(args.get(2), "RayQuery")?;
                self.execute_world_ray_hit(plan, &capture, detail, ray, WorldQueryKind::Nearest)
            }
            WorldQueryKind::Occluded => {
                let ray = expect_struct_ref_arg(args.get(2), "RayQuery")?;
                let hit = self.execute_world_ray_hit(
                    plan,
                    &capture,
                    detail,
                    ray,
                    WorldQueryKind::Occluded,
                )?;
                let hit = expect_struct(&hit, "Hit3")?;
                Ok(occlusion_result(
                    expect_struct_bool(hit, "hit")?,
                    expect_struct_f32(hit, "distance")?,
                    expect_struct_i32(hit, "steps")?,
                ))
            }
            WorldQueryKind::Surface => {
                let hit = expect_struct_ref_arg(args.get(2), "Hit3")?;
                let mut backend = VirtualGpuWorldSurfaceBackend {
                    runtime: self.runtime.as_ref(),
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
                let sample = expect_struct_ref_arg(args.get(2), "PointDirectionQuery")?;
                let point = expect_struct_vec3_from_struct(sample, "point")?;
                let direction = expect_struct_vec3_from_struct(sample, "direction")?;
                let mut backend = VirtualGpuWorldRadianceBackend {
                    runtime: self.runtime.as_ref(),
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
                let point = expect_vec3_arg(args.get(2), "point")?;
                let mut backend = VirtualGpuWorldMediumBackend {
                    runtime: self.runtime.as_ref(),
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
        self.runtime.note_solver_plan(solver_plan);
        let origin = expect_struct_vec3_from_struct(ray, "origin")?;
        let direction = expect_struct_vec3_from_struct(ray, "direction")?;
        let max_distance = expect_struct_f32(ray, "max_distance")?;
        let min_step = expect_struct_f32(ray, "min_step")?;
        let hit_epsilon = expect_struct_f32(ray, "hit_epsilon")?;
        let max_steps = expect_struct_i32(ray, "max_steps")?;
        let mut backend = VirtualGpuWorldTraceBackend {
            runtime: self.runtime.as_ref(),
            capture,
            detail,
            origin,
            direction,
            max_distance,
            min_step,
            hit_epsilon,
            max_steps,
            solver_plan,
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
        if let Ok(hit) = expect_struct(&backend.result, "Hit3") {
            self.runtime.note_hit_result(
                expect_struct_bool(hit, "hit").unwrap_or(false),
                expect_struct_i32(hit, "steps").unwrap_or_default().max(0) as u32,
            );
        }
        Ok(backend.result)
    }
}

impl CaptureQueryBackend for VirtualGpuCaptureBackend<'_> {
    fn resolve_field_or_shape_capture(
        &self,
        capture: Option<&KernelValue>,
    ) -> Result<SmolStr, QueryExecError> {
        self.runtime.resolve_field_or_shape_capture(capture)
    }

    fn resolve_shape_capture(
        &self,
        capture: Option<&KernelValue>,
    ) -> Result<SmolStr, QueryExecError> {
        self.runtime.resolve_shape_capture(capture)
    }

    fn capture_distance(
        &self,
        capture: &SmolStr,
        point: [f32; 3],
        capture_kind: crate::query_plan::CaptureKind,
    ) -> Result<f32, QueryExecError> {
        self.runtime.capture_distance(capture, point, capture_kind)
    }

    fn capture_normal(
        &self,
        capture: &SmolStr,
        point: [f32; 3],
        capture_kind: crate::query_plan::CaptureKind,
    ) -> Result<[f32; 3], QueryExecError> {
        self.runtime.capture_normal(capture, point, capture_kind)
    }

    fn support_summary(
        &self,
        capture: &SmolStr,
        capture_kind: crate::query_plan::CaptureKind,
    ) -> Result<KernelValue, QueryExecError> {
        self.runtime.support_summary(capture, capture_kind)
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
        self.runtime.trace_shape(
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
        self.runtime.surface_at(shape, hit)
    }

    fn radiance_at(
        &self,
        shape: &SmolStr,
        point: [f32; 3],
        direction: [f32; 3],
    ) -> Result<KernelValue, QueryExecError> {
        self.runtime.radiance_at(shape, point, direction)
    }

    fn medium_at(&self, shape: &SmolStr, point: [f32; 3]) -> Result<KernelValue, QueryExecError> {
        self.runtime.medium_at(shape, point)
    }
}

fn vgpu_backend_with_world_shapes<B, F>(
    runtime: &dyn QueryContractRuntime,
    capture: &SmolStr,
    detail: i32,
    root_shape_id: Option<u32>,
    backend: &mut B,
    mut emit_shapes: F,
) -> Result<(), QueryExecError>
where
    F: FnMut(&mut B, &[SmolStr]) -> Result<(), QueryExecError>,
{
    let shapes = runtime.resolve_world_shapes(capture, detail, root_shape_id)?;
    emit_shapes(backend, &shapes)
}

fn vgpu_backend_with_domain_flag<B, F>(
    runtime: &dyn QueryContractRuntime,
    domain: &KernelStructValue,
    kind: WorldQueryKind,
    backend: &mut B,
    enabled: F,
) -> Result<(), QueryExecError>
where
    F: FnOnce(&mut B) -> Result<(), QueryExecError>,
{
    if runtime.world_domain_flag_enabled(domain, kind)? {
        enabled(backend)?;
    }
    Ok(())
}

fn vgpu_world_distance(
    runtime: &dyn QueryContractRuntime,
    capture: &SmolStr,
    detail: i32,
    point: [f32; 3],
) -> Result<f32, QueryExecError> {
    let mut backend = VirtualGpuWorldDistanceBackend {
        runtime,
        capture,
        detail,
        point,
        result: 1_000_000.0,
    };
    execute_world_distance(&mut backend)?;
    Ok(backend.result)
}

struct VirtualGpuWorldDistanceBackend<'a> {
    runtime: &'a dyn QueryContractRuntime,
    capture: &'a SmolStr,
    detail: i32,
    point: [f32; 3],
    result: f32,
}

impl WorldQueryBackend for VirtualGpuWorldDistanceBackend<'_> {
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
        vgpu_backend_with_world_shapes(
            self.runtime,
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

impl WorldDistanceBackend for VirtualGpuWorldDistanceBackend<'_> {
    type Error = QueryExecError;

    fn init_world_distance(&mut self) -> Result<(), Self::Error> {
        self.result = 1_000_000.0;
        Ok(())
    }

    fn accumulate_world_distance_shape(&mut self, shape: &SmolStr) -> Result<(), Self::Error> {
        if let Some(lower_bound) = self
            .runtime
            .eval_shape_support_lower_bound(shape, self.point)?
            && lower_bound > self.result
        {
            self.runtime.note_support_pruned_candidates(1);
            return Ok(());
        }
        self.runtime.note_candidate_count(1);
        self.result = self
            .result
            .min(self.runtime.eval_shape_distance(shape, self.point)?);
        Ok(())
    }
}

struct VirtualGpuWorldNormalBackend<'a> {
    runtime: &'a dyn QueryContractRuntime,
    capture: &'a SmolStr,
    detail: i32,
    point: [f32; 3],
}

impl WorldNormalBackend for VirtualGpuWorldNormalBackend<'_> {
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
        vgpu_world_distance(self.runtime, self.capture, self.detail, point)
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
        let length = (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
        if length == 0.0 {
            Ok([0.0, 0.0, 1.0])
        } else {
            Ok([normal[0] / length, normal[1] / length, normal[2] / length])
        }
    }
}

struct VirtualGpuWorldTraceBackend<'a> {
    runtime: &'a dyn QueryContractRuntime,
    capture: &'a SmolStr,
    detail: i32,
    origin: [f32; 3],
    direction: [f32; 3],
    max_distance: f32,
    min_step: f32,
    hit_epsilon: f32,
    max_steps: i32,
    solver_plan: &'a RaySolverPlan,
    result: KernelValue,
    best_distance: f32,
}

impl WorldQueryBackend for VirtualGpuWorldTraceBackend<'_> {
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
        vgpu_backend_with_world_shapes(
            self.runtime,
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

impl WorldTraceBackend for VirtualGpuWorldTraceBackend<'_> {
    type Error = QueryExecError;

    fn init_world_trace(&mut self) -> Result<(), Self::Error> {
        self.result = default_hit(self.origin);
        self.best_distance = f32::INFINITY;
        Ok(())
    }

    fn consider_world_trace_shape(&mut self, shape: &SmolStr) -> Result<(), Self::Error> {
        let prune_distance = self.best_distance.min(self.max_distance);
        if let Some(lower_bound) = self
            .runtime
            .eval_shape_support_lower_bound(shape, self.origin)?
            && lower_bound > prune_distance
        {
            self.runtime.note_support_pruned_candidates(1);
            self.runtime.note_solver_support_rejection();
            return Ok(());
        }
        self.runtime.note_candidate_count(1);
        self.runtime
            .note_solver_dense_fallback_reasons(self.solver_plan.dense_fallback_reasons());
        if self
            .solver_plan
            .method_enabled(RaySolverMethod::LipschitzSafeStepping)
        {
            self.runtime.note_solver_lipschitz_step();
        }
        let hit = self.runtime.trace_shape(
            shape,
            self.origin,
            self.direction,
            self.max_distance,
            self.min_step,
            self.hit_epsilon,
            self.max_steps,
        )?;
        let hit_ref = expect_struct(&hit, "Hit3")?;
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

struct VirtualGpuWorldSurfaceBackend<'a> {
    runtime: &'a dyn QueryContractRuntime,
    capture: &'a SmolStr,
    detail: i32,
    domain: &'a KernelStructValue,
    hit: KernelStructValue,
    root_shape_id: u32,
    result: KernelValue,
}

impl WorldQueryBackend for VirtualGpuWorldSurfaceBackend<'_> {
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
        vgpu_backend_with_world_shapes(
            self.runtime,
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
        vgpu_backend_with_domain_flag(self.runtime, self.domain, kind, self, enabled)
    }
}

impl WorldSurfaceBackend for VirtualGpuWorldSurfaceBackend<'_> {
    type Error = QueryExecError;

    fn init_world_surface(&mut self) -> Result<(), Self::Error> {
        self.result = default_surface();
        Ok(())
    }

    fn consider_world_surface_shape(&mut self, shape: &SmolStr) -> Result<(), Self::Error> {
        self.runtime.note_candidate_count(1);
        if crate::query_exec::stable_shape_capture_id(shape) == self.root_shape_id {
            self.result = self.runtime.surface_at(shape, &self.hit)?;
        }
        Ok(())
    }
}

struct VirtualGpuWorldRadianceBackend<'a> {
    runtime: &'a dyn QueryContractRuntime,
    capture: &'a SmolStr,
    detail: i32,
    domain: &'a KernelStructValue,
    point: [f32; 3],
    direction: [f32; 3],
    result: [f32; 3],
}

impl WorldQueryBackend for VirtualGpuWorldRadianceBackend<'_> {
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
        vgpu_backend_with_world_shapes(
            self.runtime,
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
        vgpu_backend_with_domain_flag(self.runtime, self.domain, kind, self, enabled)
    }
}

impl WorldRadianceBackend for VirtualGpuWorldRadianceBackend<'_> {
    type Error = QueryExecError;

    fn init_world_radiance(&mut self) -> Result<(), Self::Error> {
        self.result = [0.0, 0.0, 0.0];
        Ok(())
    }

    fn accumulate_world_radiance_shape(&mut self, shape: &SmolStr) -> Result<(), Self::Error> {
        self.runtime.note_candidate_count(1);
        let KernelValue::Vec3(next) =
            self.runtime
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

struct VirtualGpuWorldMediumBackend<'a> {
    runtime: &'a dyn QueryContractRuntime,
    capture: &'a SmolStr,
    detail: i32,
    domain: &'a KernelStructValue,
    point: [f32; 3],
    density: f32,
    emission: [f32; 3],
    anisotropy: f32,
}

impl WorldQueryBackend for VirtualGpuWorldMediumBackend<'_> {
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
        vgpu_backend_with_world_shapes(
            self.runtime,
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
        vgpu_backend_with_domain_flag(self.runtime, self.domain, kind, self, enabled)
    }
}

impl WorldMediumBackend for VirtualGpuWorldMediumBackend<'_> {
    type Error = QueryExecError;

    fn init_world_medium(&mut self) -> Result<(), Self::Error> {
        self.density = 0.0;
        self.emission = [0.0, 0.0, 0.0];
        self.anisotropy = 0.0;
        Ok(())
    }

    fn accumulate_world_medium_shape(&mut self, shape: &SmolStr) -> Result<(), Self::Error> {
        self.runtime.note_candidate_count(1);
        let KernelValue::Struct(next) = self.runtime.medium_at(shape, self.point)? else {
            return Ok(());
        };
        let next_density = expect_struct_f32(&next, "density")?;
        let next_emission = expect_struct_vec3_from_struct(&next, "emission")?;
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

pub(crate) fn execute_batch_query(
    ctx: &QueryExecContext,
    plan: &KernelBatchQueryPlan,
    args: &[KernelValue],
    trace: &KernelBatchQueryTrace,
) -> Result<KernelValue, QueryExecError> {
    execute_batch_query_with_observability(ctx, plan, args, trace).map(|(value, _)| value)
}

pub(crate) fn execute_batch_query_with_observability(
    ctx: &QueryExecContext,
    plan: &KernelBatchQueryPlan,
    args: &[KernelValue],
    trace: &KernelBatchQueryTrace,
) -> Result<(KernelValue, QueryExecutionObservability), QueryExecError> {
    execute_batch_query_with_snapshot_observability(ctx, None, plan, args, trace)
}

pub(crate) fn execute_batch_query_with_snapshot_observability(
    ctx: &QueryExecContext,
    snapshot: Option<&crate::world_identity::WorldSnapshotHandle>,
    plan: &KernelBatchQueryPlan,
    args: &[KernelValue],
    trace: &KernelBatchQueryTrace,
) -> Result<(KernelValue, QueryExecutionObservability), QueryExecError> {
    let runtime = VirtualGpuRuntime::new_with_snapshot(ctx, snapshot);
    runtime.note_dispatch();
    if let Err(errors) = validate_batch_query_plan(plan) {
        runtime.note_contract_validation_failure();
        return Err(validation_error("batch query", errors));
    }
    let evaluator = VirtualGpuDirectQueryEvaluator {
        runtime: Box::new(runtime),
    };
    let item_arg_index = if matches!(plan.capture_kind, CaptureKind::Region) {
        2
    } else {
        1
    };
    let items = match args.get(item_arg_index) {
        Some(KernelValue::Array(items)) => items,
        Some(other) => {
            return Err(QueryExecError::TypeMismatch {
                expected: "Array".to_string(),
                found: format!("{other:?}"),
            });
        }
        None => {
            return Err(QueryExecError::MissingCaptureTarget {
                kind: "batch query items",
            });
        }
    };
    evaluator.runtime.note_candidate_count(items.len() as u32);
    evaluator.runtime.note_batch_dispatch_shape(
        items.len() as u32,
        matches!(plan.capture_kind, CaptureKind::Region),
    );
    evaluator.runtime.note_batch_execution_mode(!matches!(
        plan.pruning_strategy,
        PruningStrategy::None | PruningStrategy::ConservativeTraversal
    ));

    if matches!(plan.capture_kind, CaptureKind::Region) {
        let out = execute_world_batch_query(&evaluator, plan, args, items, trace)?;
        return Ok((
            KernelValue::Array(out),
            evaluator.runtime.snapshot_observability(),
        ));
    }

    let mut out = vec![KernelValue::Nothing; items.len()];
    for iteration in &trace.iterations {
        let item_index = iteration.item_index as usize;
        let Some(item) = items.get(item_index) else {
            return Err(QueryExecError::Unsupported {
                message: format!(
                    "virtual GPU scheduled missing item index {}",
                    iteration.item_index
                ),
            });
        };
        out[item_index] = execute_batch_item_contract(
            &VirtualGpuCaptureBackend {
                runtime: evaluator.runtime.as_ref(),
            },
            &plan.item_contract,
            args.first(),
            item,
        )?;
    }
    Ok((
        KernelValue::Array(out),
        evaluator.runtime.snapshot_observability(),
    ))
}

fn execute_world_batch_query(
    evaluator: &VirtualGpuDirectQueryEvaluator<'_>,
    plan: &KernelBatchQueryPlan,
    args: &[KernelValue],
    items: &[KernelValue],
    trace: &KernelBatchQueryTrace,
) -> Result<Vec<KernelValue>, QueryExecError> {
    let KernelBatchItemContract::WorldQuery { plan: world_plan } = &plan.item_contract else {
        return Err(QueryExecError::Unsupported {
            message: "world-batch plans require a world-query item contract".to_string(),
        });
    };
    let capture = args
        .first()
        .cloned()
        .ok_or(QueryExecError::MissingCaptureTarget {
            kind: "world batch capture",
        })?;
    let domain = args
        .get(1)
        .cloned()
        .ok_or(QueryExecError::MissingCaptureTarget {
            kind: "world batch domain",
        })?;
    let mut out = vec![KernelValue::Nothing; items.len()];
    for iteration in &trace.iterations {
        let item_index = iteration.item_index as usize;
        let Some(item) = items.get(item_index) else {
            return Err(QueryExecError::Unsupported {
                message: format!(
                    "virtual GPU scheduled missing world item index {}",
                    iteration.item_index
                ),
            });
        };
        let world_args = build_world_batch_args(world_plan, &capture, &domain, item)?;
        let value = evaluator.execute_world_query(world_plan, &world_args)?;
        out[item_index] = wrap_world_batch_result(world_plan, value)?;
    }
    Ok(out)
}

fn build_world_batch_args(
    plan: &KernelWorldQueryPlan,
    capture: &KernelValue,
    domain: &KernelValue,
    item: &KernelValue,
) -> Result<Vec<KernelValue>, QueryExecError> {
    let mut args = vec![capture.clone(), domain.clone()];
    match world_query_kind_for_contract_id(plan.contract_id).ok_or_else(|| {
        QueryExecError::Unsupported {
            message: format!(
                "missing world query contract '{}'",
                plan.contract_id.as_str()
            ),
        }
    })? {
        WorldQueryKind::Distance | WorldQueryKind::Normal | WorldQueryKind::Medium => {
            let point = expect_struct(item, "PointQuery")?;
            args.push(KernelValue::Vec3(expect_struct_vec3_from_struct(
                point, "point",
            )?));
        }
        WorldQueryKind::Nearest | WorldQueryKind::Trace | WorldQueryKind::Occluded => {
            expect_struct(item, "RayQuery")?;
            args.push(item.clone());
        }
        WorldQueryKind::Surface => {
            expect_struct(item, "Hit3")?;
            args.push(item.clone());
        }
        WorldQueryKind::Radiance => {
            expect_struct(item, "PointDirectionQuery")?;
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
    match world_query_kind_for_contract_id(plan.contract_id).ok_or_else(|| {
        QueryExecError::Unsupported {
            message: format!(
                "missing world query contract '{}'",
                plan.contract_id.as_str()
            ),
        }
    })? {
        WorldQueryKind::Distance => Ok(distance_result(expect_f32_value(&value)?)),
        WorldQueryKind::Normal => Ok(normal_result(expect_vec3(&value, "normal")?)),
        _ => Ok(value),
    }
}

fn expect_struct_ref_arg<'a>(
    value: Option<&'a KernelValue>,
    name: &'static str,
) -> Result<&'a KernelStructValue, QueryExecError> {
    let value = value.ok_or(QueryExecError::MissingCaptureTarget { kind: name })?;
    expect_struct(value, name)
}

fn expect_vec3_arg(
    value: Option<&KernelValue>,
    expected: &'static str,
) -> Result<[f32; 3], QueryExecError> {
    let value = value.ok_or(QueryExecError::MissingCaptureTarget { kind: expected })?;
    expect_vec3(value, expected)
}

fn expect_struct<'a>(
    value: &'a KernelValue,
    name: &str,
) -> Result<&'a crate::kernel::KernelStructValue, QueryExecError> {
    match value {
        KernelValue::Struct(value) if value.name.as_str() == name => Ok(value),
        other => Err(QueryExecError::TypeMismatch {
            expected: name.to_string(),
            found: format!("{other:?}"),
        }),
    }
}

fn field_value<'a>(
    value: &'a crate::kernel::KernelStructValue,
    name: &str,
) -> Result<&'a KernelValue, QueryExecError> {
    value
        .fields
        .iter()
        .find(|(field, _)| field.as_str() == name)
        .map(|(_, value)| value)
        .ok_or_else(|| QueryExecError::Unsupported {
            message: format!("missing field '{name}' on {}", value.name),
        })
}

fn expect_struct_bool(
    value: &crate::kernel::KernelStructValue,
    field: &str,
) -> Result<bool, QueryExecError> {
    match field_value(value, field)? {
        KernelValue::Bool(value) => Ok(*value),
        other => Err(QueryExecError::TypeMismatch {
            expected: "Bool".to_string(),
            found: format!("{other:?}"),
        }),
    }
}

fn expect_struct_f32(
    value: &crate::kernel::KernelStructValue,
    field: &str,
) -> Result<f32, QueryExecError> {
    match field_value(value, field)? {
        KernelValue::F32(value) => Ok(*value),
        other => Err(QueryExecError::TypeMismatch {
            expected: "F32".to_string(),
            found: format!("{other:?}"),
        }),
    }
}

fn expect_struct_i32(
    value: &crate::kernel::KernelStructValue,
    field: &str,
) -> Result<i32, QueryExecError> {
    match field_value(value, field)? {
        KernelValue::I32(value) => Ok(*value),
        other => Err(QueryExecError::TypeMismatch {
            expected: "I32".to_string(),
            found: format!("{other:?}"),
        }),
    }
}

fn expect_struct_u32(
    value: &crate::kernel::KernelStructValue,
    field: &str,
) -> Result<u32, QueryExecError> {
    match field_value(value, field)? {
        KernelValue::U32(value) => Ok(*value),
        other => Err(QueryExecError::TypeMismatch {
            expected: "U32".to_string(),
            found: format!("{other:?}"),
        }),
    }
}

fn expect_struct_vec3_from_struct(
    value: &crate::kernel::KernelStructValue,
    field: &str,
) -> Result<[f32; 3], QueryExecError> {
    match field_value(value, field)? {
        KernelValue::Vec3(value) => Ok(*value),
        other => Err(QueryExecError::TypeMismatch {
            expected: "Vec3".to_string(),
            found: format!("{other:?}"),
        }),
    }
}

fn expect_vec3(value: &KernelValue, expected: &str) -> Result<[f32; 3], QueryExecError> {
    match value {
        KernelValue::Vec3(value) => Ok(*value),
        other => Err(QueryExecError::TypeMismatch {
            expected: expected.to_string(),
            found: format!("{other:?}"),
        }),
    }
}

fn expect_f32_value(value: &KernelValue) -> Result<f32, QueryExecError> {
    match value {
        KernelValue::F32(value) => Ok(*value),
        other => Err(QueryExecError::TypeMismatch {
            expected: "F32".to_string(),
            found: format!("{other:?}"),
        }),
    }
}

fn distance_result(distance: f32) -> KernelValue {
    KernelValue::Struct(crate::kernel::KernelStructValue {
        name: SmolStr::new("DistanceResult"),
        fields: vec![(SmolStr::new("distance"), KernelValue::F32(distance))],
    })
}

fn normal_result(normal: [f32; 3]) -> KernelValue {
    KernelValue::Struct(crate::kernel::KernelStructValue {
        name: SmolStr::new("NormalResult"),
        fields: vec![(SmolStr::new("normal"), KernelValue::Vec3(normal))],
    })
}

fn occlusion_result(occluded: bool, distance: f32, steps: i32) -> KernelValue {
    KernelValue::Struct(crate::kernel::KernelStructValue {
        name: SmolStr::new("OcclusionResult"),
        fields: vec![
            (SmolStr::new("occluded"), KernelValue::Bool(occluded)),
            (SmolStr::new("distance"), KernelValue::F32(distance)),
            (SmolStr::new("steps"), KernelValue::I32(steps)),
        ],
    })
}
