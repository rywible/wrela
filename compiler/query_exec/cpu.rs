use crate::hir;
use crate::hir::body::{BinaryOp, Expr, Literal, UnaryOp};
use crate::kernel::ir::{KernelBatchQueryPlan, KernelCaptureQueryPlan, KernelWorldQueryPlan};
use crate::kernel::{KernelStructValue, KernelValue};
use crate::portable;
use crate::query_exec::capture::{self, CaptureQueryBackend};
use crate::query_exec::context::QueryExecContext;
use crate::query_exec::ids::{
    stable_field_scene_capture_id, stable_region_scene_capture_id, stable_shape_capture_id,
    stable_shape_scene_capture_id,
};
use crate::query_exec::region::{select_region_exec_case, world_domain_mismatch_message};
use crate::query_exec::world::{
    WorldDistanceBackend, WorldMediumBackend, WorldNormalBackend, WorldQueryBackend,
    WorldRadianceBackend, WorldSurfaceBackend, WorldTraceBackend, execute_world_distance,
    execute_world_medium, execute_world_normal, execute_world_radiance, execute_world_surface,
    execute_world_trace, world_query_semantics,
};
use crate::query_plan::{BatchQueryKind, WorldQueryKind};
use crate::scene_ir::{
    FieldNode, RepeatKind, SceneArgExpr, SceneValueExpr, ShapeNode, SmoothKind, TransformKind,
};
use smol_str::SmolStr;
use std::collections::HashMap;
use thiserror::Error;
use wrela_runtime::{
    TypeId, Value as RuntimeValue, wr_affine_transform, wr_bend, wr_box, wr_box_frame,
    wr_capped_cone, wr_capsule, wr_cone, wr_cylinder, wr_displace, wr_ellipsoid,
    wr_field_intersection, wr_field_subtract, wr_field_union, wr_hex_prism, wr_instance_array,
    wr_mat3_component, wr_mat3_from_columns, wr_mat4_component, wr_mat4_from_columns,
    wr_mirror_array, wr_plane, wr_quat_new, wr_radial_repeat, wr_repeat_grid, wr_repeat_linear,
    wr_rotate, wr_rounded_box, wr_slab, wr_smooth_intersection, wr_smooth_subtract,
    wr_smooth_union, wr_sphere, wr_taper, wr_torus, wr_translate, wr_triangle_prism, wr_twist,
    wr_type_id, wr_uniform_scale, wr_vec_add, wr_vec_component, wr_vec_div, wr_vec_mul, wr_vec_sub,
    wr_vec2_new, wr_vec3_new, wr_vec4_new, wr_warp,
};

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
    capture::execute_capture_query(&DirectQueryOps::new(ctx), plan, args)
}

pub fn execute_world_query(
    ctx: &QueryExecContext,
    plan: &KernelWorldQueryPlan,
    args: &[KernelValue],
) -> Result<KernelValue, QueryExecError> {
    DirectQueryEvaluator::new(ctx).execute_world_query(plan, args)
}

pub(crate) fn execute_batch_query(
    ctx: &QueryExecContext,
    plan: &KernelBatchQueryPlan,
    args: &[KernelValue],
) -> Result<KernelValue, QueryExecError> {
    DirectQueryEvaluator::new(ctx).execute_batch_query(plan, args)
}

pub(crate) fn resolve_batch_capture(
    ctx: &QueryExecContext,
    plan: &KernelBatchQueryPlan,
    capture: Option<&KernelValue>,
) -> Result<SmolStr, QueryExecError> {
    let evaluator = DirectQueryEvaluator::new(ctx);
    match plan.kind {
        BatchQueryKind::Distance | BatchQueryKind::Normal => {
            evaluator.resolve_field_or_shape_capture(capture)
        }
        BatchQueryKind::Trace | BatchQueryKind::Surface | BatchQueryKind::Occluded => {
            evaluator.resolve_shape_capture(capture)
        }
    }
}

pub(crate) struct DirectQueryOps<'a> {
    ctx: &'a QueryExecContext,
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

impl<'a> std::ops::Deref for DirectQueryEvaluator<'a> {
    type Target = DirectQueryOps<'a>;

    fn deref(&self) -> &Self::Target {
        &self.ops
    }
}

impl<'a> DirectQueryEvaluator<'a> {
    pub(crate) fn new(ctx: &'a QueryExecContext) -> Self {
        Self {
            ops: DirectQueryOps::new(ctx),
        }
    }
}

impl<'a> DirectQueryOps<'a> {
    pub(crate) fn new(ctx: &'a QueryExecContext) -> Self {
        Self { ctx }
    }

    pub(crate) fn execute_world_query(
        &self,
        plan: &KernelWorldQueryPlan,
        args: &[KernelValue],
    ) -> Result<KernelValue, QueryExecError> {
        let semantics = world_query_semantics(plan.kind);
        let capture = self.resolve_region_capture(args.first())?;
        let domain = expect_struct(args.get(1), "SceneDomain")?;
        let detail = self.validate_world_domain(&capture, domain, semantics.query_name)?;
        match plan.kind {
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
            WorldQueryKind::Trace => {
                let origin = expect_vec3(args.get(2), "origin")?;
                let direction = expect_vec3(args.get(3), "direction")?;
                let max_distance = expect_f32(args.get(4), "max_distance")?;
                let min_step = expect_f32(args.get(5), "min_step")?;
                let hit_epsilon = expect_f32(args.get(6), "hit_epsilon")?;
                let max_steps = expect_i32(args.get(7), "max_steps")?;
                let mut backend = CpuWorldTraceBackend {
                    evaluator: self,
                    capture: &capture,
                    detail,
                    origin,
                    direction,
                    max_distance,
                    min_step,
                    hit_epsilon,
                    max_steps,
                    result: default_hit(origin),
                    best_distance: f32::INFINITY,
                };
                execute_world_trace(&mut backend)?;
                Ok(backend.result)
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
                let point = expect_vec3(args.get(2), "point")?;
                let direction = expect_vec3(args.get(3), "direction")?;
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
        let capture_scene_id = stable_region_scene_capture_id(capture);
        let domain_scene_id = expect_struct_u32(domain, "scene_id")?;
        if capture_scene_id != domain_scene_id {
            return Err(QueryExecError::Unsupported {
                message: world_domain_mismatch_message(query_name),
            });
        }
        expect_struct_i32(domain, "geometry_detail")
    }

    pub(crate) fn world_domain_flag_enabled(
        &self,
        domain: &KernelStructValue,
        kind: WorldQueryKind,
    ) -> Result<bool, QueryExecError> {
        let Some(flag) = world_query_semantics(kind).domain_flag else {
            return Ok(true);
        };
        expect_struct_bool(domain, flag)
    }

    pub(crate) fn execute_batch_query(
        &self,
        plan: &KernelBatchQueryPlan,
        args: &[KernelValue],
    ) -> Result<KernelValue, QueryExecError> {
        let capture = resolve_batch_capture(self.ctx, plan, args.first())?;
        let items = expect_array(
            args.get(1),
            if matches!(plan.kind, BatchQueryKind::Distance | BatchQueryKind::Normal) {
                "points"
            } else if matches!(plan.kind, BatchQueryKind::Surface) {
                "hits"
            } else {
                "rays"
            },
        )?;
        let mut out = Vec::with_capacity(items.len());
        for item in items {
            out.push(self.execute_batch_item(plan, &capture, item)?);
        }
        Ok(KernelValue::Array(out))
    }

    fn execute_batch_item(
        &self,
        plan: &KernelBatchQueryPlan,
        capture: &SmolStr,
        item: &KernelValue,
    ) -> Result<KernelValue, QueryExecError> {
        match plan.kind {
            BatchQueryKind::Distance => {
                let point_query = expect_struct_ref(item, "PointQuery")?;
                let point = expect_struct_vec3(point_query, "point")?;
                Ok(distance_result(self.eval_capture_distance(
                    capture,
                    point,
                    plan.capture_kind,
                )?))
            }
            BatchQueryKind::Normal => {
                let point_query = expect_struct_ref(item, "PointQuery")?;
                let point = expect_struct_vec3(point_query, "point")?;
                Ok(normal_result(self.eval_capture_normal(
                    capture,
                    point,
                    plan.capture_kind,
                )?))
            }
            BatchQueryKind::Trace => {
                let ray = expect_struct_ref(item, "RayQuery")?;
                self.trace_shape(
                    capture,
                    expect_struct_vec3(ray, "origin")?,
                    expect_struct_vec3(ray, "direction")?,
                    expect_struct_f32(ray, "max_distance")?,
                    expect_struct_f32(ray, "min_step")?,
                    expect_struct_f32(ray, "hit_epsilon")?,
                    expect_struct_i32(ray, "max_steps")?,
                )
            }
            BatchQueryKind::Surface => {
                let hit = expect_struct_ref(item, "Hit3")?;
                self.surface_at(capture, hit)
            }
            BatchQueryKind::Occluded => {
                let ray = expect_struct_ref(item, "RayQuery")?;
                let hit = self.trace_shape(
                    capture,
                    expect_struct_vec3(ray, "origin")?,
                    expect_struct_vec3(ray, "direction")?,
                    expect_struct_f32(ray, "max_distance")?,
                    expect_struct_f32(ray, "min_step")?,
                    expect_struct_f32(ray, "hit_epsilon")?,
                    expect_struct_i32(ray, "max_steps")?,
                )?;
                let hit = expect_struct_ref(&hit, "Hit3")?;
                Ok(occlusion_result(
                    expect_struct_bool(hit, "hit")?,
                    expect_struct_f32(hit, "distance")?,
                    expect_struct_i32(hit, "steps")?,
                ))
            }
        }
    }

    pub(crate) fn resolve_field_or_shape_capture(
        &self,
        capture: Option<&KernelValue>,
    ) -> Result<SmolStr, QueryExecError> {
        match capture {
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
                self.ctx
                    .field_names
                    .iter()
                    .find(|name| stable_field_scene_capture_id(name) == scene_id)
                    .cloned()
                    .ok_or_else(|| QueryExecError::UnknownFieldCapture {
                        name: SmolStr::new(format!("{scene_id}")),
                    })
            }
            Some(KernelValue::Struct(value)) if value.name.as_str() == "ShapeCapture" => {
                let scene_id = expect_struct_u32(value, "scene_id")?;
                self.ctx
                    .shape_names
                    .iter()
                    .find(|name| stable_shape_scene_capture_id(name) == scene_id)
                    .cloned()
                    .ok_or_else(|| QueryExecError::UnknownShapeCapture {
                        name: SmolStr::new(format!("{scene_id}")),
                    })
            }
            _ => Err(QueryExecError::MissingCaptureTarget {
                kind: "field-or-shape capture",
            }),
        }
    }

    pub(crate) fn resolve_shape_capture(
        &self,
        capture: Option<&KernelValue>,
    ) -> Result<SmolStr, QueryExecError> {
        match capture {
            Some(KernelValue::Capture(name)) if self.ctx.shape_names.contains(name) => {
                Ok(name.clone())
            }
            Some(KernelValue::Struct(value)) if value.name.as_str() == "ShapeCapture" => {
                let root_feature_id = expect_struct_u32(value, "root_feature_id")?;
                self.ctx
                    .shape_names
                    .iter()
                    .find(|name| stable_shape_capture_id(name) == root_feature_id)
                    .cloned()
                    .ok_or_else(|| QueryExecError::UnknownShapeCapture {
                        name: SmolStr::new(format!("{root_feature_id}")),
                    })
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
        match capture {
            Some(KernelValue::Capture(name)) if self.ctx.regions_by_name.contains_key(name) => {
                Ok(name.clone())
            }
            Some(KernelValue::Struct(value)) if value.name.as_str() == "RegionCapture" => {
                let scene_id = expect_struct_u32(value, "scene_id")?;
                self.ctx
                    .regions_by_name
                    .keys()
                    .find(|name| stable_region_scene_capture_id(name) == scene_id)
                    .cloned()
                    .ok_or_else(|| QueryExecError::UnknownRegionCapture {
                        name: SmolStr::new(format!("{scene_id}")),
                    })
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
        match capture_kind {
            crate::query_plan::CaptureKind::Field => self.eval_field_normal(capture, point),
            crate::query_plan::CaptureKind::Shape => self.eval_shape_normal(capture, point),
            crate::query_plan::CaptureKind::Region => Err(QueryExecError::Unsupported {
                message: "region captures are only valid for world queries".to_string(),
            }),
        }
    }

    pub(crate) fn eval_field_distance(
        &self,
        field: &SmolStr,
        point: [f32; 3],
    ) -> Result<f32, QueryExecError> {
        let scene =
            self.ctx
                .scene
                .fields
                .get(field)
                .ok_or_else(|| QueryExecError::MissingField {
                    name: field.clone(),
                })?;
        self.eval_field_node(&scene.root, point)
    }

    pub(crate) fn eval_field_normal(
        &self,
        field: &SmolStr,
        point: [f32; 3],
    ) -> Result<[f32; 3], QueryExecError> {
        let eps = 0.001f32;
        let dx = self.eval_field_distance(field, [point[0] + eps, point[1], point[2]])?
            - self.eval_field_distance(field, [point[0] - eps, point[1], point[2]])?;
        let dy = self.eval_field_distance(field, [point[0], point[1] + eps, point[2]])?
            - self.eval_field_distance(field, [point[0], point[1] - eps, point[2]])?;
        let dz = self.eval_field_distance(field, [point[0], point[1], point[2] + eps])?
            - self.eval_field_distance(field, [point[0], point[1], point[2] - eps])?;
        Ok(normalize3([dx, dy, dz]))
    }

    pub(crate) fn eval_shape_distance(
        &self,
        shape: &SmolStr,
        point: [f32; 3],
    ) -> Result<f32, QueryExecError> {
        let scene =
            self.ctx
                .scene
                .shapes
                .get(shape)
                .ok_or_else(|| QueryExecError::MissingShape {
                    name: shape.clone(),
                })?;
        self.eval_shape_node(&scene.root, point)
    }

    pub(crate) fn eval_shape_normal(
        &self,
        shape: &SmolStr,
        point: [f32; 3],
    ) -> Result<[f32; 3], QueryExecError> {
        let eps = 0.001f32;
        let dx = self.eval_shape_distance(shape, [point[0] + eps, point[1], point[2]])?
            - self.eval_shape_distance(shape, [point[0] - eps, point[1], point[2]])?;
        let dy = self.eval_shape_distance(shape, [point[0], point[1] + eps, point[2]])?
            - self.eval_shape_distance(shape, [point[0], point[1] - eps, point[2]])?;
        let dz = self.eval_shape_distance(shape, [point[0], point[1], point[2] + eps])?
            - self.eval_shape_distance(shape, [point[0], point[1], point[2] - eps])?;
        Ok(normalize3([dx, dy, dz]))
    }

    fn eval_field_node(&self, node: &FieldNode, point: [f32; 3]) -> Result<f32, QueryExecError> {
        match node {
            FieldNode::Use { target } => self.eval_field_distance(target, point),
            FieldNode::Primitive { primitive, args } => {
                self.eval_field_primitive(*primitive, args.as_deref().unwrap_or(&[]), point)
            }
            FieldNode::Union { items } => {
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
            other => Err(QueryExecError::Unsupported {
                message: format!(
                    "field operation '{other:?}' is not implemented in query_exec::cpu"
                ),
            }),
        }
    }

    fn eval_shape_node(&self, node: &ShapeNode, point: [f32; 3]) -> Result<f32, QueryExecError> {
        match node {
            ShapeNode::Use { target } => self.eval_shape_distance(target, point),
            ShapeNode::Leaf(leaf) => self.eval_field_distance(&leaf.field, point),
            ShapeNode::Union { items } => {
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

    fn eval_wrapped_point(
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

    fn eval_repeat_point(
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

    pub(crate) fn trace_shape(
        &self,
        shape: &SmolStr,
        origin: [f32; 3],
        direction: [f32; 3],
        max_distance: f32,
        min_step: f32,
        hit_epsilon: f32,
        max_steps: i32,
    ) -> Result<KernelValue, QueryExecError> {
        if !self.ctx.shape_names.contains(shape) {
            return Ok(default_hit(origin));
        }
        let mut travel = 0.0f32;
        let mut steps = 0i32;
        while steps < max_steps && travel <= max_distance {
            let point = [
                origin[0] + direction[0] * travel,
                origin[1] + direction[1] * travel,
                origin[2] + direction[2] * travel,
            ];
            let distance = self.eval_shape_distance(shape, point)?;
            if distance <= hit_epsilon {
                let normal = self.eval_shape_normal(shape, point)?;
                let feature_id = self.first_shape_feature_id(shape).unwrap_or(0);
                let payload = self
                    .lookup_shape_leaf(shape, feature_id)
                    .and_then(|leaf| self.eval_payload_body(&leaf.payload).ok())
                    .unwrap_or_else(default_payload);
                return Ok(hit_value(
                    true,
                    travel,
                    point,
                    normal,
                    point,
                    normal,
                    steps,
                    feature_id,
                    0,
                    0,
                    stable_shape_capture_id(shape),
                    payload,
                ));
            }
            travel += distance.max(min_step);
            steps += 1;
        }
        Ok(default_hit(origin))
    }

    pub(crate) fn surface_at(
        &self,
        shape: &SmolStr,
        hit: &KernelStructValue,
    ) -> Result<KernelValue, QueryExecError> {
        let feature_id = expect_struct_u32(hit, "feature_id")?;
        let Some(leaf) = self.lookup_shape_leaf(shape, feature_id) else {
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
        let Some(leaf) = self.lookup_first_leaf(shape) else {
            return Ok(KernelValue::Vec3([0.0, 0.0, 0.0]));
        };
        let Some(radiance) = &leaf.radiance else {
            return Ok(KernelValue::Vec3([0.0, 0.0, 0.0]));
        };
        self.execute_portable_function(
            radiance,
            vec![
                KernelValue::Vec3(point),
                KernelValue::Vec3(direction),
                KernelValue::U32(leaf.feature_id),
            ],
        )
    }

    pub(crate) fn medium_at(
        &self,
        shape: &SmolStr,
        point: [f32; 3],
    ) -> Result<KernelValue, QueryExecError> {
        let Some(leaf) = self.lookup_first_leaf(shape) else {
            return Ok(default_medium());
        };
        let Some(volume) = &leaf.volume else {
            return Ok(default_medium());
        };
        let surface_distance = self.eval_field_distance(&leaf.field, point)?;
        self.execute_portable_function(
            volume,
            vec![KernelValue::Vec3(point), KernelValue::F32(surface_distance)],
        )
    }

    pub(crate) fn resolve_world_shapes(
        &self,
        capture: &SmolStr,
        detail: i32,
    ) -> Result<Vec<SmolStr>, QueryExecError> {
        let scene_id = stable_region_scene_capture_id(capture);
        let Some(region_case) = select_region_exec_case(&self.ctx.region_cases, scene_id) else {
            return Err(QueryExecError::MissingRegion {
                name: capture.clone(),
            });
        };
        region_case
            .shapes_for_detail(detail)
            .map(|shapes| shapes.to_vec())
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

    pub(crate) fn lookup_shape_leaf(
        &self,
        shape: &SmolStr,
        feature_id: u32,
    ) -> Option<&hir::ShapeLeaf> {
        fn visit<'a>(expr: &'a hir::ShapeExpr, feature_id: u32) -> Option<&'a hir::ShapeLeaf> {
            match expr {
                hir::ShapeExpr::Use { .. } => None,
                hir::ShapeExpr::Leaf(leaf) if leaf.feature_id == feature_id => Some(leaf),
                hir::ShapeExpr::Leaf(_) => None,
                hir::ShapeExpr::Union { items } | hir::ShapeExpr::Intersection { items } => {
                    items.iter().find_map(|item| visit(item, feature_id))
                }
                hir::ShapeExpr::Subtract { left, right } => {
                    visit(left, feature_id).or_else(|| visit(right, feature_id))
                }
            }
        }
        let graph = self.ctx.shape_graphs.get(shape)?;
        visit(&graph.root, feature_id)
    }

    pub(crate) fn lookup_first_leaf(&self, shape: &SmolStr) -> Option<&hir::ShapeLeaf> {
        fn visit(expr: &hir::ShapeExpr) -> Option<&hir::ShapeLeaf> {
            match expr {
                hir::ShapeExpr::Use { .. } => None,
                hir::ShapeExpr::Leaf(leaf) => Some(leaf),
                hir::ShapeExpr::Union { items } | hir::ShapeExpr::Intersection { items } => {
                    items.iter().find_map(visit)
                }
                hir::ShapeExpr::Subtract { left, right } => visit(left).or_else(|| visit(right)),
            }
        }
        let graph = self.ctx.shape_graphs.get(shape)?;
        visit(&graph.root)
    }

    pub(crate) fn first_shape_feature_id(&self, shape: &SmolStr) -> Option<u32> {
        self.lookup_first_leaf(shape).map(|leaf| leaf.feature_id)
    }

    pub(crate) fn eval_payload_body(&self, body: &hir::Body) -> Result<KernelValue, QueryExecError> {
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

    fn eval_scene_value_expr(
        &self,
        expr: &SceneValueExpr,
        env: &HashMap<SmolStr, KernelValue>,
    ) -> Result<KernelValue, QueryExecError> {
        match expr {
            SceneValueExpr::Literal(literal) => Ok(literal_to_kernel(literal)),
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
            let fields = record
                .fields
                .iter()
                .enumerate()
                .map(|(index, field)| {
                    let value =
                        args.get(index)
                            .cloned()
                            .ok_or_else(|| QueryExecError::Unsupported {
                                message: format!(
                                    "missing constructor arg {} for builtin '{}'",
                                    index, name
                                ),
                            })?;
                    Ok((SmolStr::new(field.name), value))
                })
                .collect::<Result<Vec<_>, QueryExecError>>()?;
            return Ok(Some(KernelValue::Struct(KernelStructValue {
                name: name.clone(),
                fields,
            })));
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
        DirectQueryOps::trace_shape(
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

    fn medium_at(
        &self,
        shape: &SmolStr,
        point: [f32; 3],
    ) -> Result<KernelValue, QueryExecError> {
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
    backend: &mut B,
    mut emit_shapes: F,
) -> Result<(), QueryExecError>
where
    F: FnMut(&mut B, &[SmolStr]) -> Result<(), QueryExecError>,
{
    let shapes = evaluator.resolve_world_shapes(capture, detail)?;
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
        cpu_backend_with_world_shapes(self.evaluator, self.capture, self.detail, self, emit_shapes)
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
    result: KernelValue,
    best_distance: f32,
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
        cpu_backend_with_world_shapes(self.evaluator, self.capture, self.detail, self, emit_shapes)
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
        let hit = self.evaluator.trace_shape(
            shape,
            self.origin,
            self.direction,
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
        cpu_backend_with_world_shapes(self.evaluator, self.capture, self.detail, self, emit_shapes)
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
        if stable_shape_capture_id(shape) == self.root_shape_id {
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
        cpu_backend_with_world_shapes(self.evaluator, self.capture, self.detail, self, emit_shapes)
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
        let KernelValue::Vec3(next) = self
            .evaluator
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
        cpu_backend_with_world_shapes(self.evaluator, self.capture, self.detail, self, emit_shapes)
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
        let KernelValue::Struct(next) = self.evaluator.medium_at(shape, self.point)? else {
            return Ok(());
        };
        self.density += expect_struct_f32(&next, "density")?;
        let next_emission = expect_struct_vec3(&next, "emission")?;
        self.anisotropy += expect_struct_f32(&next, "anisotropy")?;
        self.emission = [
            self.emission[0] + next_emission[0],
            self.emission[1] + next_emission[1],
            self.emission[2] + next_emission[2],
        ];
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
    let fields = record
        .fields
        .iter()
        .enumerate()
        .map(|(index, field)| {
            let value = args
                .get(index)
                .cloned()
                .ok_or_else(|| QueryExecError::Unsupported {
                    message: format!("missing constructor arg {} for '{}'", index, record.name),
                })?;
            Ok((SmolStr::new(field.name), value))
        })
        .collect::<Result<Vec<_>, QueryExecError>>()?;
    Ok(KernelValue::Struct(KernelStructValue {
        name: SmolStr::new(record.name),
        fields,
    }))
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
            (SmolStr::new("shading_frame"), transform3_identity_value()),
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

fn normalize3(value: [f32; 3]) -> [f32; 3] {
    let len = (value[0] * value[0] + value[1] * value[1] + value[2] * value[2]).sqrt();
    if len <= f32::EPSILON {
        [0.0, 0.0, 1.0]
    } else {
        [value[0] / len, value[1] / len, value[2] / len]
    }
}

fn kernel_to_runtime(value: &KernelValue) -> Result<RuntimeValue, QueryExecError> {
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
        KernelValue::Array(_)
        | KernelValue::Struct(_)
        | KernelValue::Capture(_)
        | KernelValue::DispatchBackend(_)
        | KernelValue::GpuBuffer(_)
        | KernelValue::GpuAtomicI32(_)
        | KernelValue::GpuAtomicU32(_) => Err(QueryExecError::Unsupported {
            message: format!("cannot convert runtime math value from {value:?}"),
        }),
    }
}

fn runtime_to_kernel_value(value: RuntimeValue) -> Result<KernelValue, QueryExecError> {
    match wr_type_id(value) as u32 {
        id if id == TypeId::Nil as u32 => Ok(KernelValue::Nothing),
        id if id == TypeId::Boolean as u32 => Ok(KernelValue::Bool(value.as_bool())),
        id if id == TypeId::Integer as u32 => Ok(KernelValue::I32(value.as_int() as i32)),
        id if id == TypeId::Float as u32 => Ok(KernelValue::F32(value.as_float() as f32)),
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
