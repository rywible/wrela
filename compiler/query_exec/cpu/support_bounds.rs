//! Owns CPU support-bound and support-derived helper evaluation for query
//! execution.
//! Does not own full query traversal or final witness/report assembly.
//!
//! Key invariants:
//! - support-bound shortcuts may prune work, but they must stay conservative
//!   with respect to authored geometry/material meaning.
//! - any cached/support-derived value here must remain compatible with the query
//!   policy that requested it.
//!
//! Primary entrypoints:
//! - support and surface helper methods on `DirectQueryOps`
//!
//! Failure modes / common pitfalls:
//! - returning optimistic support information here turns a performance helper
//!   into a correctness bug.

use super::*;

impl<'a> DirectQueryOps<'a> {
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

    pub(crate) fn eval_support_lower_bound_for_field_scene(
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

    pub(crate) fn eval_support_lower_bound_for_shape_scene(
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

    pub(crate) fn eval_field_support_record(
        &self,
        scene: &crate::scene_ir::FieldScene,
        id: crate::scene_ir::SupportNodeId,
        point: [f32; 3],
    ) -> Result<Option<f32>, QueryExecError> {
        let Some(record) = scene.support_node_record(id) else {
            return Ok(None);
        };
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

    pub(crate) fn eval_shape_support_record(
        &self,
        scene: &crate::scene_ir::ShapeScene,
        id: crate::scene_ir::SupportNodeId,
        point: [f32; 3],
    ) -> Result<Option<f32>, QueryExecError> {
        let Some(record) = scene.support_node_record(id) else {
            return Ok(None);
        };
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

    pub(crate) fn eval_support_leaf_payload(
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

    pub(crate) fn field_support_bounds(
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

    pub(crate) fn shape_support_bounds(
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

    pub(crate) fn support_payload_bounds(
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

    pub(crate) fn shape_node_support_bounds(
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

    pub(crate) fn world_acceleration_tree(
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

    pub(crate) fn shape_root_union_tree(
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

    pub(crate) fn support_cache_probe(
        &self,
        cache: Option<&SupportBrickCache>,
        origin: [f32; 3],
        direction: [f32; 3],
        start_t: f32,
        max_t: f32,
    ) -> RaySupportProbe {
        self.note_cache_brick_visit();
        let Some(cache) = cache else {
            self.note_cache_brick_miss();
            return RaySupportProbe::Unavailable;
        };
        if !cache.is_ready() {
            self.note_cache_brick_miss();
            self.note_cache_disable_reasons(&cache.report.rejection_reasons);
            return RaySupportProbe::Unavailable;
        }
        self.note_artifact_load();
        match cache.first_occupied_interval(origin, direction, start_t, max_t) {
            Some(interval) => {
                self.note_cache_brick_hit();
                if interval.start_t.max(0.0) > start_t.max(0.0) + f32::EPSILON {
                    self.note_cache_interval_advance();
                }
                RaySupportProbe::Interval(RaySupportInterval {
                    start_t: interval.start_t,
                    end_t: interval.end_t,
                    starts_inside: interval.start_t <= start_t.max(0.0),
                    conservative: true,
                })
            }
            None => {
                self.note_cache_brick_miss();
                RaySupportProbe::Rejected
            }
        }
    }

    pub(crate) fn shape_cache_support_probe(
        &self,
        shape: &SmolStr,
        origin: [f32; 3],
        direction: [f32; 3],
        start_t: f32,
        max_t: f32,
    ) -> RaySupportProbe {
        self.support_cache_probe(
            self.ctx.shape_cache_support(shape),
            origin,
            direction,
            start_t,
            max_t,
        )
    }

    pub(crate) fn world_cache_support_probe(
        &self,
        capture: &SmolStr,
        detail: i32,
        origin: [f32; 3],
        direction: [f32; 3],
        start_t: f32,
        max_t: f32,
    ) -> RaySupportProbe {
        self.support_cache_probe(
            self.ctx.world_cache_support(capture, detail),
            origin,
            direction,
            start_t,
            max_t,
        )
    }

    pub(crate) fn shape_ray_support_probe_world(
        &self,
        shape: &SmolStr,
        origin: [f32; 3],
        direction: [f32; 3],
    ) -> Result<RaySupportProbe, QueryExecError> {
        let cache_probe =
            self.shape_cache_support_probe(shape, origin, direction, 0.0, f32::INFINITY);
        if matches!(cache_probe, RaySupportProbe::Interval(_)) {
            return Ok(cache_probe);
        }
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

    pub(crate) fn field_ray_support_probe(
        &self,
        field: &SmolStr,
        origin: [f32; 3],
        direction: [f32; 3],
    ) -> Result<RaySupportProbe, QueryExecError> {
        let scene = self.field_scene(field)?;
        self.field_ray_support_probe_record(scene, scene.root_support_id, origin, direction)
    }

    pub(crate) fn field_ray_support_probe_record(
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

    pub(crate) fn shape_ray_support_probe(
        &self,
        shape: &SmolStr,
        origin: [f32; 3],
        direction: [f32; 3],
    ) -> Result<RaySupportProbe, QueryExecError> {
        let scene = self.shape_scene(shape)?;
        self.shape_ray_support_probe_record(scene, scene.root_support_id, origin, direction)
    }

    pub(crate) fn shape_ray_support_probe_record(
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

    pub(crate) fn support_leaf_ray_support_probe(
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

    pub(crate) fn field_support_children_bounds(
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

    pub(crate) fn shape_support_children_bounds(
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

    pub(crate) fn transform_support_bounds(
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

    pub(crate) fn repeat_support_bounds(
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

    pub(crate) fn eval_field_support_children(
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

    pub(crate) fn eval_shape_support_children(
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

    pub(crate) fn shape_leaf_from_winner(
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
}
