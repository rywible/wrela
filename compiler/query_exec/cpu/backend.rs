//! Owns CPU-query backend validation and backend-specific bridge helpers.
//! Does not own high-level query traversal or observability policy.
//!
//! Key invariants:
//! - backend validation must fail before execution begins.
//! - backend adapters may specialize execution, but they cannot weaken the
//!   query contract selected upstream.
//!
//! Primary entrypoints:
//! - backend validation/error helpers in this module
//!
//! Failure modes / common pitfalls:
//! - reporting backend-specific validation too late obscures the real contract
//!   failure site.

use super::*;

pub(super) fn validation_error(label: &str, errors: Vec<KernelValidationError>) -> QueryExecError {
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

pub(super) fn batch_kind_for_plan(
    plan: &KernelBatchQueryPlan,
) -> Result<BatchQueryKind, QueryExecError> {
    batch_query_kind_for_contract_id(plan.contract_id).ok_or_else(|| QueryExecError::Unsupported {
        message: format!(
            "missing batch query contract '{}'",
            plan.contract_id.as_str()
        ),
    })
}

pub(super) fn build_world_batch_args(
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

pub(super) fn wrap_world_batch_result(
    plan: &KernelWorldQueryPlan,
    value: KernelValue,
) -> Result<KernelValue, QueryExecError> {
    match world_kind_for_plan(plan)? {
        WorldQueryKind::Distance => Ok(distance_result(expect_f32(Some(&value), "distance")?)),
        WorldQueryKind::Normal => Ok(normal_result(expect_vec3(Some(&value), "normal")?)),
        _ => Ok(value),
    }
}

pub(super) fn world_kind_for_plan(
    plan: &KernelWorldQueryPlan,
) -> Result<WorldQueryKind, QueryExecError> {
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

pub(super) fn eval_builtin_callable(
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

pub(super) struct CpuWorldDistanceBackend<'a, 'ctx> {
    pub(super) evaluator: &'a DirectQueryOps<'ctx>,
    pub(super) capture: &'a SmolStr,
    pub(super) detail: i32,
    pub(super) point: [f32; 3],
    pub(super) result: f32,
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

pub(super) struct CpuWorldNormalBackend<'a, 'ctx> {
    pub(super) evaluator: &'a DirectQueryOps<'ctx>,
    pub(super) capture: &'a SmolStr,
    pub(super) detail: i32,
    pub(super) point: [f32; 3],
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

pub(super) struct CpuWorldTraceBackend<'a, 'ctx> {
    pub(super) evaluator: &'a DirectQueryOps<'ctx>,
    pub(super) capture: &'a SmolStr,
    pub(super) detail: i32,
    pub(super) origin: [f32; 3],
    pub(super) direction: [f32; 3],
    pub(super) max_distance: f32,
    pub(super) min_step: f32,
    pub(super) hit_epsilon: f32,
    pub(super) max_steps: i32,
    pub(super) solver_plan: &'a RaySolverPlan,
    pub(super) artifact_contracts: &'a [ArtifactContract],
    pub(super) result: KernelValue,
    pub(super) best_distance: f32,
    pub(super) cache_start_t: f32,
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
        }
        .max(self.cache_start_t);
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
        self.cache_start_t = match self.evaluator.world_cache_support_probe(
            self.capture,
            self.detail,
            self.origin,
            self.direction,
            0.0,
            self.max_distance,
        ) {
            RaySupportProbe::Interval(interval) => interval.start_t.max(0.0),
            RaySupportProbe::Rejected | RaySupportProbe::Unavailable => 0.0,
        };
        if let Some(tree) = self
            .evaluator
            .world_acceleration_tree(self.capture, self.detail)?
        {
            return self.trace_world_hierarchically(&tree);
        }
        self.evaluator.note_cache_dense_fallback();
        if self.cache_start_t <= 0.0 {
            self.evaluator.note_cache_budget_rejection();
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
        self.cache_start_t = 0.0;
        Ok(())
    }

    fn consider_world_trace_shape(&mut self, shape: &SmolStr) -> Result<(), Self::Error> {
        let prune_distance = self.best_distance.min(self.max_distance);
        let mut start_travel = self.cache_start_t;
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

pub(super) struct CpuWorldSurfaceBackend<'a, 'ctx> {
    pub(super) evaluator: &'a DirectQueryOps<'ctx>,
    pub(super) capture: &'a SmolStr,
    pub(super) detail: i32,
    pub(super) domain: &'a KernelStructValue,
    pub(super) hit: KernelStructValue,
    pub(super) root_shape_id: u32,
    pub(super) result: KernelValue,
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

pub(super) struct CpuWorldRadianceBackend<'a, 'ctx> {
    pub(super) evaluator: &'a DirectQueryOps<'ctx>,
    pub(super) capture: &'a SmolStr,
    pub(super) detail: i32,
    pub(super) domain: &'a KernelStructValue,
    pub(super) point: [f32; 3],
    pub(super) direction: [f32; 3],
    pub(super) result: [f32; 3],
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

pub(super) struct CpuWorldMediumBackend<'a, 'ctx> {
    pub(super) evaluator: &'a DirectQueryOps<'ctx>,
    pub(super) capture: &'a SmolStr,
    pub(super) detail: i32,
    pub(super) domain: &'a KernelStructValue,
    pub(super) point: [f32; 3],
    pub(super) density: f32,
    pub(super) emission: [f32; 3],
    pub(super) anisotropy: f32,
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
