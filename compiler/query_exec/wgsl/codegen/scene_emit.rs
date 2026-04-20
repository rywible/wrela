//! Owns WGSL emission for scene/world analytic intersection helpers and scene
//! traversal code used by direct-query shaders.
//! Does not own ABI declarations or portable PIR helper emission.
//!
//! Key invariants:
//! - emitted intersection helpers preserve the CPU oracle's hit-selection
//!   semantics and range checks.
//! - scene traversal code keeps shape/material dispatch aligned with the
//!   normalized shader behavior selected earlier in codegen.
//! - generated helper names stay stable because snapshot tests and runtime
//!   integration treat them as codegen surface.
//!
//! Primary entrypoints:
//! - `emit_analytic_intersection_helpers`
//! - `emit_scene_query_helpers`
//! - `emit_world_query_helpers`
//!
//! Failure modes / common pitfalls:
//! - changing intersection tie-break behavior here without matching CPU logic
//!   creates silent backend drift.
//! - embedding binding/ABI details locally makes scene emit harder to keep in
//!   sync with the rest of the WGSL backend.

use super::*;

pub(super) fn emit_analytic_intersection_helpers(out: &mut String) -> Result<(), QueryExecError> {
    writeln!(
        out,
        "fn wr_select_first_valid_t(t0: f32, t1: f32, start_t: f32, max_t: f32) -> f32 {{"
    )
    .ok();
    writeln!(out, "  if (t0 >= start_t && t0 <= max_t) {{ return t0; }}").ok();
    writeln!(out, "  if (t1 >= start_t && t1 <= max_t) {{ return t1; }}").ok();
    writeln!(out, "  return -1.0;").ok();
    writeln!(out, "}}\n").ok();
    writeln!(
        out,
        "fn wr_solve_ray_sphere(origin: vec3<f32>, direction: vec3<f32>, center: vec3<f32>, radius: f32, start_t: f32, max_t: f32) -> f32 {{"
    )
    .ok();
    writeln!(out, "  let oc = origin - center;").ok();
    writeln!(out, "  let a = dot(direction, direction);").ok();
    writeln!(out, "  if (a <= 1.0e-6) {{ return -1.0; }}").ok();
    writeln!(out, "  let b = 2.0 * dot(oc, direction);").ok();
    writeln!(out, "  let c = dot(oc, oc) - radius * radius;").ok();
    writeln!(out, "  let discriminant = b * b - 4.0 * a * c;").ok();
    writeln!(out, "  if (discriminant < 0.0) {{ return -1.0; }}").ok();
    writeln!(out, "  let root = sqrt(discriminant);").ok();
    writeln!(out, "  let inv = 1.0 / (2.0 * a);").ok();
    writeln!(
        out,
        "  return wr_select_first_valid_t((-b - root) * inv, (-b + root) * inv, start_t, max_t);"
    )
    .ok();
    writeln!(out, "}}\n").ok();
    writeln!(
        out,
        "fn wr_solve_ray_plane(origin: vec3<f32>, direction: vec3<f32>, normal: vec3<f32>, offset: f32, start_t: f32, max_t: f32) -> f32 {{"
    )
    .ok();
    writeln!(out, "  let denom = dot(direction, normal);").ok();
    writeln!(out, "  if (abs(denom) <= 1.0e-6) {{ return -1.0; }}").ok();
    writeln!(out, "  let t = -(dot(origin, normal) + offset) / denom;").ok();
    writeln!(
        out,
        "  if (t >= start_t && t <= max_t) {{ return t; }} return -1.0;"
    )
    .ok();
    writeln!(out, "}}\n").ok();
    writeln!(
        out,
        "fn wr_solve_ray_aabb(origin: vec3<f32>, direction: vec3<f32>, min_bounds: vec3<f32>, max_bounds: vec3<f32>, start_t: f32, max_t: f32) -> f32 {{"
    )
    .ok();
    writeln!(
        out,
        "  let interval = wr_ray_aabb_interval(origin, direction, min_bounds, max_bounds);"
    )
    .ok();
    writeln!(
        out,
        "  if (interval.accepted == 0u || interval.end_t < start_t || interval.start_t > max_t) {{ return -1.0; }}"
    )
    .ok();
    writeln!(out, "  return max(max(interval.start_t, start_t), 0.0);").ok();
    writeln!(out, "}}\n").ok();
    Ok(())
}

pub(super) fn analytic_shape_case(
    ctx: &QueryExecContext,
    ops: &DirectQueryOps<'_>,
    shape_name: &SmolStr,
) -> Result<Option<String>, QueryExecError> {
    let scene = ctx
        .scene
        .shapes
        .get(shape_name)
        .ok_or_else(|| QueryExecError::MissingShape {
            name: shape_name.clone(),
        })?;
    let leaf = match &scene.root {
        crate::scene_ir::ShapeNode::Leaf(leaf) => leaf,
        crate::scene_ir::ShapeNode::Use { target } => {
            let target_scene =
                ctx.scene
                    .shapes
                    .get(target)
                    .ok_or_else(|| QueryExecError::MissingShape {
                        name: target.clone(),
                    })?;
            let crate::scene_ir::ShapeNode::Leaf(leaf) = &target_scene.root else {
                return Ok(None);
            };
            leaf
        }
        _ => return Ok(None),
    };
    let field = ctx
        .scene
        .fields
        .get(&leaf.field)
        .ok_or_else(|| QueryExecError::MissingField {
            name: leaf.field.clone(),
        })?;
    analytic_field_case(ops, ctx, &field.root, "ray.origin", "ray.direction")
}

pub(super) fn analytic_shape_kind(
    ctx: &QueryExecContext,
    shape_name: &SmolStr,
) -> Result<u32, QueryExecError> {
    let scene = ctx
        .scene
        .shapes
        .get(shape_name)
        .ok_or_else(|| QueryExecError::MissingShape {
            name: shape_name.clone(),
        })?;
    let root = match &scene.root {
        crate::scene_ir::ShapeNode::Leaf(leaf) => ctx
            .scene
            .fields
            .get(&leaf.field)
            .ok_or_else(|| QueryExecError::MissingField {
                name: leaf.field.clone(),
            })?
            .root
            .clone(),
        crate::scene_ir::ShapeNode::Use { target } => {
            let target_scene =
                ctx.scene
                    .shapes
                    .get(target)
                    .ok_or_else(|| QueryExecError::MissingShape {
                        name: target.clone(),
                    })?;
            let crate::scene_ir::ShapeNode::Leaf(leaf) = &target_scene.root else {
                return Ok(0);
            };
            ctx.scene
                .fields
                .get(&leaf.field)
                .ok_or_else(|| QueryExecError::MissingField {
                    name: leaf.field.clone(),
                })?
                .root
                .clone()
        }
        _ => return Ok(0),
    };
    analytic_field_kind(ctx, &root)
}

pub(super) fn analytic_field_case(
    ops: &DirectQueryOps<'_>,
    ctx: &QueryExecContext,
    node: &crate::scene_ir::FieldNode,
    origin_expr: &str,
    direction_expr: &str,
) -> Result<Option<String>, QueryExecError> {
    match node {
        crate::scene_ir::FieldNode::Use { target } => {
            let field =
                ctx.scene
                    .fields
                    .get(target)
                    .ok_or_else(|| QueryExecError::MissingField {
                        name: target.clone(),
                    })?;
            analytic_field_case(ops, ctx, &field.root, origin_expr, direction_expr)
        }
        crate::scene_ir::FieldNode::Transform { kind, param, inner } => {
            let Some(param) = param.as_ref() else {
                return analytic_field_case(ops, ctx, inner, origin_expr, direction_expr);
            };
            if !matches!(
                kind,
                TransformKind::Translate | TransformKind::Rotate | TransformKind::UniformScale
            ) {
                return Ok(None);
            }
            let value = ops.eval_scene_constant(param)?;
            let rendered = kernel_value_literal(&value)?;
            let local_origin =
                analytic_transform_point_expr(*kind, &value, &rendered, origin_expr)?;
            let local_direction =
                analytic_transform_vector_expr(*kind, &value, &rendered, direction_expr)?;
            analytic_field_case(ops, ctx, inner, &local_origin, &local_direction)
        }
        crate::scene_ir::FieldNode::Primitive { primitive, args } => {
            let args = args.as_deref().unwrap_or(&[]);
            let mut out = String::new();
            match primitive {
                crate::hir::FieldPrimitive::Sphere => {
                    let radius = scene_named_arg_constant_opt(ops, args, "radius")?
                        .map(|value| kernel_value_f32(&value, "sphere radius"))
                        .transpose()?
                        .unwrap_or(1.0)
                        .abs();
                    writeln!(
                        out,
                        "      let analytic_t = wr_solve_ray_sphere({origin_expr}, {direction_expr}, vec3<f32>(0.0, 0.0, 0.0), {}, start_travel, ray.max_distance);",
                        format_f32(radius)
                    )
                    .ok();
                }
                crate::hir::FieldPrimitive::Plane => {
                    let normal = scene_named_arg_constant_opt(ops, args, "normal")?
                        .map(|value| kernel_value_vec3(&value, "plane normal"))
                        .transpose()?
                        .unwrap_or([0.0, 1.0, 0.0]);
                    let offset = scene_named_arg_constant_opt(ops, args, "offset")?
                        .map(|value| kernel_value_f32(&value, "plane offset"))
                        .transpose()?
                        .unwrap_or(0.0);
                    writeln!(
                        out,
                        "      let analytic_t = wr_solve_ray_plane({origin_expr}, {direction_expr}, vec3<f32>({}, {}, {}), {}, start_travel, ray.max_distance);",
                        format_f32(normal[0]),
                        format_f32(normal[1]),
                        format_f32(normal[2]),
                        format_f32(offset)
                    )
                    .ok();
                }
                crate::hir::FieldPrimitive::Box => {
                    let half = scene_named_arg_constant_opt(ops, args, "half")?
                        .map(|value| kernel_value_vec3(&value, "box half"))
                        .transpose()?
                        .unwrap_or([0.5, 0.5, 0.5]);
                    writeln!(
                        out,
                        "      let analytic_t = wr_solve_ray_aabb({origin_expr}, {direction_expr}, vec3<f32>({}, {}, {}), vec3<f32>({}, {}, {}), start_travel, ray.max_distance);",
                        format_f32(-half[0].abs()),
                        format_f32(-half[1].abs()),
                        format_f32(-half[2].abs()),
                        format_f32(half[0].abs()),
                        format_f32(half[1].abs()),
                        format_f32(half[2].abs())
                    )
                    .ok();
                }
                _ => return Ok(None),
            }
            out.push_str("      if (analytic_t < 0.0) { return wr_default_hit(ray.origin); }\n");
            out.push_str("      let point = ray.origin + ray.direction * analytic_t;\n");
            out.push_str("      let normal = shape_normal_dispatch(shape_index, point);\n");
            out.push_str("      let winner = shape_winner_dispatch(shape_index, point);\n");
            out.push_str("      if (winner.has_leaf != 0u) {\n");
            out.push_str(
                "        let frame = field_local_frame_dispatch(winner.field_index, point);\n",
            );
            out.push_str(
                "        let local_normal = field_local_normal_dispatch(winner.field_index, frame);\n",
            );
            out.push_str(
                "        let payload = payload_for_shape_leaf(winner.leaf_scene_index, winner.leaf_id);\n",
            );
            out.push_str(
                "        return wr_hit_value(true, analytic_t, point, normal, frame.point, local_normal, 1, winner.feature_id, frame.instance_id, frame.repeat_id, root_shape_id_for_shape(shape_index), payload);\n",
            );
            out.push_str("      }\n");
            out.push_str(
                "      return wr_hit_value(true, analytic_t, point, normal, point, normal, 1, 0u, 0u, 0u, root_shape_id_for_shape(shape_index), wr_default_payload());\n",
            );
            Ok(Some(out))
        }
        _ => Ok(None),
    }
}

pub(super) fn analytic_field_kind(
    ctx: &QueryExecContext,
    node: &crate::scene_ir::FieldNode,
) -> Result<u32, QueryExecError> {
    Ok(match node {
        crate::scene_ir::FieldNode::Use { target } => {
            let field =
                ctx.scene
                    .fields
                    .get(target)
                    .ok_or_else(|| QueryExecError::MissingField {
                        name: target.clone(),
                    })?;
            analytic_field_kind(ctx, &field.root)?
        }
        crate::scene_ir::FieldNode::Transform { kind, param, inner } => {
            if param.is_none() {
                return analytic_field_kind(ctx, inner);
            }
            if matches!(
                kind,
                TransformKind::Translate | TransformKind::Rotate | TransformKind::UniformScale
            ) {
                analytic_field_kind(ctx, inner)?
            } else {
                0
            }
        }
        crate::scene_ir::FieldNode::Primitive { primitive, .. } => match primitive {
            crate::hir::FieldPrimitive::Sphere => 1,
            crate::hir::FieldPrimitive::Plane => 2,
            crate::hir::FieldPrimitive::Box => 3,
            _ => 0,
        },
        _ => 0,
    })
}

pub(super) fn scene_named_arg_constant_opt(
    ops: &DirectQueryOps<'_>,
    args: &[crate::scene_ir::SceneArgExpr],
    name: &str,
) -> Result<Option<KernelValue>, QueryExecError> {
    scene_named_arg_value(args, name)
        .ok()
        .map(|expr| ops.eval_scene_constant(expr))
        .transpose()
}

pub(super) fn kernel_value_f32(value: &KernelValue, label: &str) -> Result<f32, QueryExecError> {
    match value {
        KernelValue::F32(value) => Ok(*value),
        KernelValue::I32(value) => Ok(*value as f32),
        KernelValue::U32(value) => Ok(*value as f32),
        other => Err(QueryExecError::TypeMismatch {
            expected: format!("f32 for {label}"),
            found: format!("{other:?}"),
        }),
    }
}

pub(super) fn kernel_value_vec3(
    value: &KernelValue,
    label: &str,
) -> Result<[f32; 3], QueryExecError> {
    match value {
        KernelValue::Vec3(value) => Ok(*value),
        other => Err(QueryExecError::TypeMismatch {
            expected: format!("vec3 for {label}"),
            found: format!("{other:?}"),
        }),
    }
}

pub(super) fn analytic_transform_point_expr(
    kind: TransformKind,
    value: &KernelValue,
    rendered_value: &str,
    point_expr: &str,
) -> Result<String, QueryExecError> {
    Ok(match kind {
        TransformKind::Translate => format!("wr_translate({rendered_value}, {point_expr})"),
        TransformKind::Rotate => format!(
            "{}({}, {})",
            rotate_helper_name(value)?,
            rendered_value,
            point_expr
        ),
        TransformKind::UniformScale => format!("wr_uniform_scale({rendered_value}, {point_expr})"),
        _ => {
            return Err(QueryExecError::Unsupported {
                message: format!("WGSL analytic point transform does not support {:?}", kind),
            });
        }
    })
}

pub(super) fn analytic_transform_vector_expr(
    kind: TransformKind,
    value: &KernelValue,
    rendered_value: &str,
    vector_expr: &str,
) -> Result<String, QueryExecError> {
    Ok(match kind {
        TransformKind::Translate => vector_expr.to_string(),
        TransformKind::Rotate => format!(
            "{}({}, {})",
            rotate_helper_name(value)?,
            rendered_value,
            vector_expr
        ),
        TransformKind::UniformScale => format!("wr_uniform_scale({rendered_value}, {vector_expr})"),
        _ => {
            return Err(QueryExecError::Unsupported {
                message: format!("WGSL analytic vector transform does not support {:?}", kind),
            });
        }
    })
}

pub(super) fn emit_main(
    behavior: &NormalizedShaderBehavior,
    item_abi: &PortableAbiType,
    result_abi: &PortableAbiType,
    cache_seed: CacheObservabilitySeed,
) -> Result<String, QueryExecError> {
    let item_expr = from_abi_expr(item_abi, "input_items.values[index]")?;
    let eval_expr = match behavior.cardinality {
        QueryCardinality::Scalar => behavior.scalar_eval_expr(&item_expr),
        QueryCardinality::Batch => match behavior.value_path {
            NormalizedQueryValuePath::WorldDistance => {
                format!("DistanceResult(wr_batch_world_distance(index, {item_expr}.point))")
            }
            NormalizedQueryValuePath::WorldNormal => {
                format!("NormalResult(wr_batch_world_normal(index, {item_expr}.point))")
            }
            NormalizedQueryValuePath::WorldTrace => {
                format!("wr_batch_world_trace(index, {item_expr})")
            }
            NormalizedQueryValuePath::WorldOcclusion => {
                format!("wr_occlusion_result_from_hit(wr_batch_world_trace(index, {item_expr}))")
            }
            _ => behavior.batch_eval_expr(&item_expr),
        },
    };
    let store_expr = to_abi_expr(result_abi, "result")?;
    let mut out = String::new();
    writeln!(
        out,
        "@compute @workgroup_size(WG_SIZE)\nfn main(@builtin(global_invocation_id) global_id: vec3<u32>) {{"
    )
    .ok();
    writeln!(out, "  let index = global_id.x;").ok();
    writeln!(
        out,
        "  if (index >= dispatch_config.item_count) {{ return; }}"
    )
    .ok();
    writeln!(out, "  if (index == 0u) {{").ok();
    writeln!(
        out,
        "    atomicAdd(&observability_metrics.cache_resident_shared_snapshot_artifacts, {}u);",
        cache_seed.resident_shared_snapshot_artifacts
    )
    .ok();
    writeln!(
        out,
        "    atomicAdd(&observability_metrics.cache_resident_observer_local_artifacts, {}u);",
        cache_seed.resident_observer_local_artifacts
    )
    .ok();
    writeln!(
        out,
        "    atomicAdd(&observability_metrics.cache_upload_attempts, {}u);",
        cache_seed.upload_attempts
    )
    .ok();
    writeln!(
        out,
        "    atomicAdd(&observability_metrics.cache_upload_rejections, {}u);",
        cache_seed.upload_rejections
    )
    .ok();
    writeln!(out, "  }}").ok();
    writeln!(out, "  let result = {eval_expr};").ok();
    writeln!(out, "  output_items.values[index] = {store_expr};").ok();
    writeln!(out, "}}").ok();
    Ok(out)
}

pub(super) fn emit_payload_lookup_function(
    ctx: &QueryExecContext,
    scene_index: &ShaderSceneIndex,
    ops: &DirectQueryOps<'_>,
    out: &mut String,
) -> Result<(), QueryExecError> {
    writeln!(
        out,
        "fn payload_for_shape_leaf(leaf_scene_index: u32, leaf_id: u32) -> Payload {{"
    )
    .ok();
    writeln!(out, "  switch leaf_scene_index {{").ok();
    for (shape_name, scene) in &ctx.scene.shapes {
        let shape_index = scene_index.shape(shape_name)?;
        writeln!(out, "    case {shape_index}u: {{").ok();
        writeln!(out, "      switch leaf_id {{").ok();
        for (leaf_id, leaf) in &scene.leaves {
            let payload = ops.eval_payload_body(&leaf.payload)?;
            let rendered_payload = match &payload {
                KernelValue::Nothing => "wr_default_payload()".to_string(),
                _ => kernel_value_literal(&payload)?,
            };
            writeln!(
                out,
                "        case {}u: {{ return {}; }}",
                leaf_id.0, rendered_payload
            )
            .ok();
        }
        writeln!(out, "        default: {{ return wr_default_payload(); }}").ok();
        writeln!(out, "      }}").ok();
        writeln!(out, "    }}").ok();
    }
    writeln!(out, "    default: {{ return wr_default_payload(); }}").ok();
    writeln!(out, "  }}").ok();
    writeln!(out, "}}\n").ok();
    Ok(())
}

pub(super) const WGSL_PRELUDE: &str = include_str!("../prelude.wgsl");

pub(super) fn emit_normal_sample_support() -> Result<String, QueryExecError> {
    let mut out = String::new();
    out.push_str("const WR_NORMAL_ROLE_UNKNOWN: u32 = 0u;\n");
    out.push_str("const WR_NORMAL_ROLE_CERTIFIED_FIELD_GRADIENT: u32 = 1u;\n");
    out.push_str("const WR_NORMAL_ROLE_FEATURE_NORMAL: u32 = 2u;\n");
    out.push_str("const WR_NORMAL_ROLE_HEURISTIC_SHADING_NORMAL: u32 = 3u;\n\n");
    out.push_str("struct CertifiedNormalSample {\n");
    out.push_str("  normal: vec3<f32>,\n");
    out.push_str("  available: u32,\n");
    out.push_str("  role: u32,\n");
    out.push_str("}\n\n");
    out.push_str(
        "fn wr_unavailable_normal_sample() -> CertifiedNormalSample { return CertifiedNormalSample(vec3<f32>(0.0, 0.0, 0.0), 0u, WR_NORMAL_ROLE_UNKNOWN); }\n",
    );
    out.push_str(
        "fn wr_certified_field_gradient_sample(normal: vec3<f32>) -> CertifiedNormalSample { return CertifiedNormalSample(wr_safe_normalize3(normal), 1u, WR_NORMAL_ROLE_CERTIFIED_FIELD_GRADIENT); }\n",
    );
    out.push_str(
        "fn wr_feature_normal_sample(normal: vec3<f32>) -> CertifiedNormalSample { return CertifiedNormalSample(wr_safe_normalize3(normal), 1u, WR_NORMAL_ROLE_FEATURE_NORMAL); }\n",
    );
    out.push_str(
        "fn wr_smooth_blend_weight(left_distance: f32, right_distance: f32, smoothing: f32) -> f32 {\n",
    );
    out.push_str("  if (smoothing <= 0.0) { return 1.0; }\n");
    out.push_str(
        "  return clamp(0.5 + 0.5 * (right_distance - left_distance) / smoothing, 0.0, 1.0);\n",
    );
    out.push_str("}\n\n");
    Ok(out)
}

pub(super) fn emit_field_scene_functions(
    ctx: &QueryExecContext,
    scene_index: &ShaderSceneIndex,
    ops: &DirectQueryOps<'_>,
    out: &mut String,
) -> Result<(), QueryExecError> {
    for (field_name, scene) in &ctx.scene.fields {
        let field_index = scene_index.field(field_name)?;
        for record in &scene.node_records {
            emit_field_node_function(ctx, scene_index, ops, field_name, field_index, record, out)?;
            emit_field_normal_function(
                ctx,
                scene_index,
                ops,
                field_name,
                field_index,
                record,
                out,
            )?;
        }
        emit_field_local_frame_functions(ctx, scene_index, ops, field_name, field_index, out)?;
    }
    Ok(())
}

pub(super) fn emit_field_node_function(
    ctx: &QueryExecContext,
    scene_index: &ShaderSceneIndex,
    ops: &DirectQueryOps<'_>,
    field_name: &SmolStr,
    field_index: u32,
    record: &FieldNodeRecord,
    out: &mut String,
) -> Result<(), QueryExecError> {
    let fn_name = field_node_function_name(field_index, record.id.0);
    writeln!(out, "fn {fn_name}(point: vec3<f32>) -> f32 {{").ok();
    match record.kind {
        FieldNodeKindSummary::Use => {
            let target = record.target.as_ref().expect("use target");
            let target_scene = ctx.scene.fields.get(target).expect("field use scene");
            writeln!(
                out,
                "  return {}(point);",
                field_node_function_name(scene_index.field(target)?, target_scene.root_node_id.0)
            )
            .ok();
        }
        FieldNodeKindSummary::Primitive(kind) => {
            let payload = match record.payload.as_ref() {
                Some(SceneOperatorPayload::Primitive { args }) => args.as_deref().unwrap_or(&[]),
                _ => &[],
            };
            writeln!(
                out,
                "  return {};",
                emit_field_primitive_call(ops, kind, payload, "point")?
            )
            .ok();
        }
        FieldNodeKindSummary::Union => {
            writeln!(out, "  var current: f32 = 1000000.0;").ok();
            for child in &record.children {
                writeln!(
                    out,
                    "  current = wr_field_union(current, {}(point));",
                    field_node_function_name(field_index, child.0)
                )
                .ok();
            }
            writeln!(out, "  return current;").ok();
        }
        FieldNodeKindSummary::Intersection => {
            if let Some(first) = record.children.first() {
                writeln!(
                    out,
                    "  var current: f32 = {}(point);",
                    field_node_function_name(field_index, first.0)
                )
                .ok();
                for child in record.children.iter().skip(1) {
                    writeln!(
                        out,
                        "  current = wr_field_intersection(current, {}(point));",
                        field_node_function_name(field_index, child.0)
                    )
                    .ok();
                }
                writeln!(out, "  return current;").ok();
            } else {
                writeln!(out, "  return 1000000.0;").ok();
            }
        }
        FieldNodeKindSummary::Subtract => {
            let left = record.children.first().copied();
            let right = record.children.get(1).copied();
            if let (Some(left), Some(right)) = (left, right) {
                writeln!(
                    out,
                    "  return wr_field_subtract({}(point), {}(point));",
                    field_node_function_name(field_index, left.0),
                    field_node_function_name(field_index, right.0)
                )
                .ok();
            } else {
                writeln!(out, "  return 1000000.0;").ok();
            }
        }
        FieldNodeKindSummary::Transform(kind) => {
            let inner = record.children.first().copied();
            let param = match record.payload.as_ref() {
                Some(SceneOperatorPayload::Transform { param }) => param.as_ref(),
                _ => None,
            };
            if let Some(inner) = inner {
                if let Some(param) = param {
                    let value = ops.eval_scene_constant(param)?;
                    let rendered = kernel_value_literal(&value)?;
                    writeln!(
                        out,
                        "  let local_point = {}({}, point);",
                        transform_helper_name_for_value(kind, &value)?,
                        rendered
                    )
                    .ok();
                    writeln!(
                        out,
                        "  let inner_distance = {}(local_point);",
                        field_node_function_name(field_index, inner.0)
                    )
                    .ok();
                    if matches!(kind, TransformKind::UniformScale) {
                        writeln!(out, "  return inner_distance * wr_abs_scalar({rendered});").ok();
                    } else {
                        writeln!(out, "  return inner_distance;").ok();
                    }
                } else {
                    writeln!(
                        out,
                        "  return {}(point);",
                        field_node_function_name(field_index, inner.0)
                    )
                    .ok();
                }
            } else {
                writeln!(out, "  return 1000000.0;").ok();
            }
        }
        FieldNodeKindSummary::Repeat(kind) => {
            let inner = record.children.first().copied();
            let param = match record.payload.as_ref() {
                Some(SceneOperatorPayload::Repeat { param }) => param.as_ref(),
                _ => None,
            };
            if let Some(inner) = inner {
                if let Some(param) = param {
                    let value = ops.eval_scene_constant(param)?;
                    let rendered = kernel_value_literal(&value)?;
                    writeln!(
                        out,
                        "  let local_point = {}({}, point);",
                        repeat_helper_name_for_value(kind, &value)?,
                        rendered
                    )
                    .ok();
                    writeln!(
                        out,
                        "  return {}(local_point);",
                        field_node_function_name(field_index, inner.0)
                    )
                    .ok();
                } else {
                    writeln!(
                        out,
                        "  return {}(point);",
                        field_node_function_name(field_index, inner.0)
                    )
                    .ok();
                }
            } else {
                writeln!(out, "  return 1000000.0;").ok();
            }
        }
        FieldNodeKindSummary::Smooth(kind) => {
            let smoothing = match record.payload.as_ref() {
                Some(SceneOperatorPayload::Smooth { smoothing }) => smoothing.as_ref(),
                _ => None,
            };
            if let Some(first) = record.children.first() {
                writeln!(
                    out,
                    "  var current: f32 = {}(point);",
                    field_node_function_name(field_index, first.0)
                )
                .ok();
                let smoothing = smoothing
                    .map(|value| scene_constant_literal(ops, value))
                    .transpose()?
                    .unwrap_or_else(|| "0.0".to_string());
                match kind {
                    SmoothKind::Union => {
                        for child in record.children.iter().skip(1) {
                            writeln!(
                                out,
                                "  current = wr_smooth_union(current, {}(point), {});",
                                field_node_function_name(field_index, child.0),
                                smoothing
                            )
                            .ok();
                        }
                    }
                    SmoothKind::Intersection => {
                        for child in record.children.iter().skip(1) {
                            writeln!(
                                out,
                                "  current = wr_smooth_intersection(current, {}(point), {});",
                                field_node_function_name(field_index, child.0),
                                smoothing
                            )
                            .ok();
                        }
                    }
                    SmoothKind::Subtract => {
                        if let Some(second) = record.children.get(1) {
                            writeln!(
                                out,
                                "  current = wr_smooth_subtract(current, {}(point), {});",
                                field_node_function_name(field_index, second.0),
                                smoothing
                            )
                            .ok();
                        }
                    }
                }
                writeln!(out, "  return current;").ok();
            } else {
                writeln!(out, "  return 1000000.0;").ok();
            }
        }
        FieldNodeKindSummary::Extrude => {
            let (height, profile) = match record.payload.as_ref() {
                Some(SceneOperatorPayload::Extrude { height, profile }) => {
                    (height.as_ref(), profile.as_ref())
                }
                _ => (None, None),
            };
            if let (Some(height), Some(profile)) = (height, profile) {
                let height_value = ops.eval_scene_constant(height)?;
                let abs_height = abs_scalar_kernel_value(&height_value)?;
                let half_height = abs_height * 0.5;
                let profile_distance =
                    emit_profile_expr(ops, profile, "vec2<f32>(point.x, point.z)")?;
                writeln!(out, "  let profile_distance: f32 = {profile_distance};").ok();
                writeln!(
                    out,
                    "  let axial: f32 = abs(point.y) - {};",
                    format_f32(half_height)
                )
                .ok();
                writeln!(
                    out,
                    "  return wr_profile_cap_distance(profile_distance, axial);"
                )
                .ok();
            } else {
                writeln!(out, "  return 1000000.0;").ok();
            }
        }
        FieldNodeKindSummary::Revolve => {
            let profile = match record.payload.as_ref() {
                Some(SceneOperatorPayload::Revolve { profile }) => profile.as_ref(),
                _ => None,
            };
            if let Some(profile) = profile {
                let radial = "vec2<f32>(length(vec2<f32>(point.x, point.z)), point.y)";
                writeln!(
                    out,
                    "  return {};",
                    emit_profile_expr(ops, profile, radial)?
                )
                .ok();
            } else {
                writeln!(out, "  return 1000000.0;").ok();
            }
        }
        FieldNodeKindSummary::Sweep => {
            let (path, profile) = match record.payload.as_ref() {
                Some(SceneOperatorPayload::Sweep { path, profile }) => {
                    (path.as_ref(), profile.as_ref())
                }
                _ => (None, None),
            };
            if let (Some(path), Some(profile)) = (path, profile) {
                let path_value = ops.eval_scene_constant(path)?;
                let path_length = kernel_value_length(&path_value)?;
                let path_expr = kernel_value_literal(&path_value)?;
                writeln!(
                    out,
                    "  let coords = wr_field_sweep_coords({}, point);",
                    path_expr
                )
                .ok();
                writeln!(
                    out,
                    "  let profile_distance: f32 = {};",
                    emit_profile_expr(ops, profile, "vec2<f32>(coords.x, coords.y)")?
                )
                .ok();
                writeln!(
                    out,
                    "  let axial: f32 = abs(coords.z) - {};",
                    format_f32(path_length * 0.5)
                )
                .ok();
                writeln!(
                    out,
                    "  return wr_profile_cap_distance(profile_distance, axial);"
                )
                .ok();
            } else {
                writeln!(out, "  return 1000000.0;").ok();
            }
        }
        FieldNodeKindSummary::Loft => {
            let (height, from, to) = match record.payload.as_ref() {
                Some(SceneOperatorPayload::Loft { height, from, to }) => {
                    (height.as_ref(), from.as_ref(), to.as_ref())
                }
                _ => (None, None, None),
            };
            if let (Some(height), Some(from), Some(to)) = (height, from, to) {
                let height_value = ops.eval_scene_constant(height)?;
                let abs_height = abs_scalar_kernel_value(&height_value)?;
                let half_height = abs_height * 0.5;
                let safe_height = abs_height.max(0.0001);
                writeln!(out, "  let profile_point = vec2<f32>(point.x, point.z);").ok();
                writeln!(
                    out,
                    "  let from_distance: f32 = {};",
                    emit_profile_expr(ops, from, "profile_point")?
                )
                .ok();
                writeln!(
                    out,
                    "  let to_distance: f32 = {};",
                    emit_profile_expr(ops, to, "profile_point")?
                )
                .ok();
                writeln!(
                    out,
                    "  let t: f32 = clamp((point.y + {}) / {}, 0.0, 1.0);",
                    format_f32(half_height),
                    format_f32(safe_height)
                )
                .ok();
                writeln!(
                    out,
                    "  let mixed: f32 = from_distance + (to_distance - from_distance) * t;"
                )
                .ok();
                writeln!(
                    out,
                    "  let axial: f32 = abs(point.y) - {};",
                    format_f32(half_height)
                )
                .ok();
                writeln!(out, "  return wr_profile_cap_distance(mixed, axial);").ok();
            } else {
                writeln!(out, "  return 1000000.0;").ok();
            }
        }
        FieldNodeKindSummary::OpaqueLeaf => {
            let _ = (ctx, field_name);
            writeln!(out, "  return 1000000.0;").ok();
        }
    }
    writeln!(out, "}}\n").ok();
    Ok(())
}

pub(super) fn emit_field_normal_function(
    ctx: &QueryExecContext,
    scene_index: &ShaderSceneIndex,
    ops: &DirectQueryOps<'_>,
    field_name: &SmolStr,
    field_index: u32,
    record: &FieldNodeRecord,
    out: &mut String,
) -> Result<(), QueryExecError> {
    let fn_name = field_normal_function_name(field_index, record.id.0);
    writeln!(
        out,
        "fn {fn_name}(point: vec3<f32>) -> CertifiedNormalSample {{"
    )
    .ok();
    match record.kind {
        FieldNodeKindSummary::Use => {
            let target = record.target.as_ref().expect("use target");
            let target_scene = ctx.scene.fields.get(target).expect("field use scene");
            writeln!(
                out,
                "  return {}(point);",
                field_normal_function_name(scene_index.field(target)?, target_scene.root_node_id.0)
            )
            .ok();
        }
        FieldNodeKindSummary::Primitive(kind) => {
            let payload = match record.payload.as_ref() {
                Some(SceneOperatorPayload::Primitive { args }) => args.as_deref().unwrap_or(&[]),
                _ => &[],
            };
            match kind {
                hir::FieldPrimitive::Sphere => {
                    writeln!(out, "  return wr_certified_field_gradient_sample(point);").ok();
                }
                hir::FieldPrimitive::Plane => {
                    writeln!(
                        out,
                        "  return wr_certified_field_gradient_sample({});",
                        scene_named_arg_literal(ops, payload, "normal")?
                    )
                    .ok();
                }
                _ => {
                    writeln!(out, "  return wr_unavailable_normal_sample();").ok();
                }
            }
        }
        FieldNodeKindSummary::Transform(kind) => {
            let inner = record.children.first().copied();
            let param = match record.payload.as_ref() {
                Some(SceneOperatorPayload::Transform { param }) => param.as_ref(),
                _ => None,
            };
            if let (Some(inner), Some(param)) = (inner, param) {
                let value = ops.eval_scene_constant(param)?;
                let rendered = kernel_value_literal(&value)?;
                if matches!(
                    kind,
                    TransformKind::Translate | TransformKind::Rotate | TransformKind::UniformScale
                ) {
                    writeln!(
                        out,
                        "  let local_point = {}({}, point);",
                        transform_helper_name_for_value(kind, &value)?,
                        rendered
                    )
                    .ok();
                    writeln!(
                        out,
                        "  let inner = {}(local_point);",
                        field_normal_function_name(field_index, inner.0)
                    )
                    .ok();
                    writeln!(out, "  if (inner.available == 0u) {{ return inner; }}").ok();
                    writeln!(
                        out,
                        "  return CertifiedNormalSample(wr_safe_normalize3({}), 1u, inner.role);",
                        transform_normal_expr_for_value(kind, &value, &rendered, "inner.normal")?
                    )
                    .ok();
                } else {
                    writeln!(out, "  return wr_unavailable_normal_sample();").ok();
                }
            } else if let Some(inner) = inner {
                writeln!(
                    out,
                    "  return {}(point);",
                    field_normal_function_name(field_index, inner.0)
                )
                .ok();
            } else {
                writeln!(out, "  return wr_unavailable_normal_sample();").ok();
            }
        }
        FieldNodeKindSummary::Smooth(kind) => {
            let smoothing = match record.payload.as_ref() {
                Some(SceneOperatorPayload::Smooth { smoothing }) => smoothing.as_ref(),
                _ => None,
            };
            if let Some(first) = record.children.first() {
                let smoothing = smoothing
                    .map(|value| scene_constant_literal(ops, value))
                    .transpose()?
                    .unwrap_or_else(|| "0.0".to_string());
                writeln!(
                    out,
                    "  if ({smoothing} <= 0.0) {{ return wr_unavailable_normal_sample(); }}"
                )
                .ok();
                writeln!(
                    out,
                    "  let first_sample = {}(point);",
                    field_normal_function_name(field_index, first.0)
                )
                .ok();
                writeln!(
                    out,
                    "  if (first_sample.available == 0u) {{ return first_sample; }}"
                )
                .ok();
                writeln!(
                    out,
                    "  var current_distance: f32 = {}(point);",
                    field_node_function_name(field_index, first.0)
                )
                .ok();
                writeln!(out, "  var current_normal = first_sample.normal;").ok();
                match kind {
                    SmoothKind::Union | SmoothKind::Intersection => {
                        for child in record.children.iter().skip(1) {
                            writeln!(
                                out,
                                "  let rhs_sample = {}(point);",
                                field_normal_function_name(field_index, child.0)
                            )
                            .ok();
                            writeln!(
                                out,
                                "  if (rhs_sample.available == 0u) {{ return rhs_sample; }}"
                            )
                            .ok();
                            writeln!(
                                out,
                                "  let rhs_distance: f32 = {}(point);",
                                field_node_function_name(field_index, child.0)
                            )
                            .ok();
                            writeln!(
                                out,
                                "  let h = wr_smooth_blend_weight(current_distance, rhs_distance, {smoothing});"
                            )
                            .ok();
                            writeln!(
                                out,
                                "  current_normal = wr_safe_normalize3(current_normal * h + rhs_sample.normal * (1.0 - h));"
                            )
                            .ok();
                            writeln!(
                                out,
                                "  current_distance = {}(current_distance, rhs_distance, {smoothing});",
                                match kind {
                                    SmoothKind::Union => "wr_smooth_union",
                                    SmoothKind::Intersection => "wr_smooth_intersection",
                                    SmoothKind::Subtract => unreachable!(),
                                }
                            )
                            .ok();
                        }
                    }
                    SmoothKind::Subtract => {
                        if let Some(second) = record.children.get(1) {
                            writeln!(
                                out,
                                "  let rhs_sample = {}(point);",
                                field_normal_function_name(field_index, second.0)
                            )
                            .ok();
                            writeln!(
                                out,
                                "  if (rhs_sample.available == 0u) {{ return rhs_sample; }}"
                            )
                            .ok();
                            writeln!(
                                out,
                                "  let rhs_distance: f32 = {}(point);",
                                field_node_function_name(field_index, second.0)
                            )
                            .ok();
                            writeln!(
                                out,
                                "  let h = wr_smooth_blend_weight(current_distance, rhs_distance, {smoothing});"
                            )
                            .ok();
                            writeln!(
                                out,
                                "  current_normal = wr_safe_normalize3(current_normal * h + (-rhs_sample.normal) * (1.0 - h));"
                            )
                            .ok();
                        } else {
                            writeln!(out, "  return wr_unavailable_normal_sample();").ok();
                        }
                    }
                }
                writeln!(
                    out,
                    "  return wr_certified_field_gradient_sample(current_normal);"
                )
                .ok();
            } else {
                writeln!(out, "  return wr_unavailable_normal_sample();").ok();
            }
        }
        FieldNodeKindSummary::Repeat(_)
        | FieldNodeKindSummary::Union
        | FieldNodeKindSummary::Intersection
        | FieldNodeKindSummary::Subtract
        | FieldNodeKindSummary::Extrude
        | FieldNodeKindSummary::Revolve
        | FieldNodeKindSummary::Sweep
        | FieldNodeKindSummary::Loft
        | FieldNodeKindSummary::OpaqueLeaf => {
            let _ = field_name;
            writeln!(out, "  return wr_unavailable_normal_sample();").ok();
        }
    }
    writeln!(out, "}}\n").ok();
    Ok(())
}

pub(super) fn emit_field_local_frame_functions(
    ctx: &QueryExecContext,
    scene_index: &ShaderSceneIndex,
    ops: &DirectQueryOps<'_>,
    field_name: &SmolStr,
    field_index: u32,
    out: &mut String,
) -> Result<(), QueryExecError> {
    let scene = ctx.scene.fields.get(field_name).expect("field scene");
    for record in &scene.node_records {
        let fn_name = field_local_frame_function_name(field_index, record.id.0);
        writeln!(
            out,
            "fn {fn_name}(point: vec3<f32>, instance_id: u32, repeat_id: u32) -> FieldLocalFrame {{"
        )
        .ok();
        match record.kind {
            FieldNodeKindSummary::Use => {
                let target = record.target.as_ref().expect("field use target");
                let target_scene = ctx.scene.fields.get(target).expect("field use scene");
                writeln!(
                    out,
                    "  return {}(point, instance_id, repeat_id);",
                    field_local_frame_function_name(
                        scene_index.field(target)?,
                        target_scene.root_node_id.0,
                    )
                )
                .ok();
            }
            FieldNodeKindSummary::Transform(kind) => {
                let inner = record.children.first().copied();
                let param = match record.payload.as_ref() {
                    Some(SceneOperatorPayload::Transform { param }) => param.as_ref(),
                    _ => None,
                };
                if let (Some(inner), Some(param)) = (inner, param) {
                    let value = ops.eval_scene_constant(param)?;
                    let rendered = kernel_value_literal(&value)?;
                    writeln!(
                        out,
                        "  let local_point = {}({}, point);",
                        transform_helper_name_for_value(kind, &value)?,
                        rendered
                    )
                    .ok();
                    writeln!(
                        out,
                        "  return {}(local_point, instance_id, repeat_id);",
                        field_local_frame_function_name(field_index, inner.0)
                    )
                    .ok();
                } else if let Some(inner) = inner {
                    writeln!(
                        out,
                        "  return {}(point, instance_id, repeat_id);",
                        field_local_frame_function_name(field_index, inner.0)
                    )
                    .ok();
                } else {
                    writeln!(
                        out,
                        "  return FieldLocalFrame(point, instance_id, repeat_id, {}u);",
                        record.id.0
                    )
                    .ok();
                }
            }
            FieldNodeKindSummary::Repeat(kind) => {
                let inner = record.children.first().copied();
                let param = match record.payload.as_ref() {
                    Some(SceneOperatorPayload::Repeat { param }) => param.as_ref(),
                    _ => None,
                };
                if let (Some(inner), Some(param)) = (inner, param) {
                    let value = ops.eval_scene_constant(param)?;
                    let rendered = kernel_value_literal(&value)?;
                    let identity_fn = repeat_identity_helper_name_for_value(kind, &value)?;
                    match kind {
                        RepeatKind::InstanceArray => {
                            writeln!(out, "  let component = {}({});", identity_fn, rendered).ok();
                        }
                        _ => {
                            writeln!(
                                out,
                                "  let component = {}({}, point);",
                                identity_fn, rendered
                            )
                            .ok();
                        }
                    }
                    writeln!(
                        out,
                        "  let local_point = {}({}, point);",
                        repeat_helper_name_for_value(kind, &value)?,
                        rendered
                    )
                    .ok();
                    match kind {
                        RepeatKind::InstanceArray => {
                            writeln!(
                                out,
                                "  let next_instance_id = wr_chain_identity_component(instance_id, component);"
                            )
                            .ok();
                            writeln!(
                                out,
                                "  return {}(local_point, next_instance_id, repeat_id);",
                                field_local_frame_function_name(field_index, inner.0)
                            )
                            .ok();
                        }
                        _ => {
                            writeln!(
                                out,
                                "  let next_repeat_id = wr_chain_identity_component(repeat_id, component);"
                            )
                            .ok();
                            writeln!(
                                out,
                                "  return {}(local_point, instance_id, next_repeat_id);",
                                field_local_frame_function_name(field_index, inner.0)
                            )
                            .ok();
                        }
                    }
                } else if let Some(inner) = inner {
                    writeln!(
                        out,
                        "  return {}(point, instance_id, repeat_id);",
                        field_local_frame_function_name(field_index, inner.0)
                    )
                    .ok();
                } else {
                    writeln!(
                        out,
                        "  return FieldLocalFrame(point, instance_id, repeat_id, {}u);",
                        record.id.0
                    )
                    .ok();
                }
            }
            _ => {
                writeln!(
                    out,
                    "  return FieldLocalFrame(point, instance_id, repeat_id, {}u);",
                    record.id.0
                )
                .ok();
            }
        }
        writeln!(out, "}}\n").ok();
    }

    let opaque_distance = if scene.opaque_boundary {
        let bounds = scene
            .authored_bounds
            .as_ref()
            .ok_or_else(|| QueryExecError::Unsupported {
                message: format!("opaque field '{}' is missing authored bounds", field_name),
            })?;
        let bounds_value = ops.eval_scene_constant(bounds)?;
        let (center, half) = bounds_center_half(&bounds_value)?;
        format!(
            "wr_box(point - {}, {})",
            kernel_value_literal(&KernelValue::Vec3(center))?,
            kernel_value_literal(&KernelValue::Vec3(half))?
        )
    } else {
        "1000000.0".to_string()
    };
    writeln!(
        out,
        "fn {}(point: vec3<f32>) -> f32 {{ return {}; }}\n",
        field_opaque_distance_function_name(field_index),
        opaque_distance
    )
    .ok();

    writeln!(
        out,
        "fn {}(terminal_node_id: u32, point: vec3<f32>) -> f32 {{",
        field_terminal_distance_function_name(field_index)
    )
    .ok();
    writeln!(out, "  switch terminal_node_id {{").ok();
    for record in &scene.node_records {
        writeln!(out, "    case {}u: {{", record.id.0).ok();
        if matches!(record.kind, FieldNodeKindSummary::OpaqueLeaf) {
            writeln!(
                out,
                "      return {}(point);",
                field_opaque_distance_function_name(field_index)
            )
            .ok();
        } else {
            writeln!(
                out,
                "      return {}(point);",
                field_node_function_name(field_index, record.id.0)
            )
            .ok();
        }
        writeln!(out, "    }}").ok();
    }
    writeln!(out, "    default: {{ return 1000000.0; }}").ok();
    writeln!(out, "  }}").ok();
    writeln!(out, "}}\n").ok();

    writeln!(
        out,
        "fn {}(terminal_node_id: u32, point: vec3<f32>) -> CertifiedNormalSample {{",
        field_terminal_normal_function_name(field_index)
    )
    .ok();
    writeln!(out, "  switch terminal_node_id {{").ok();
    for record in &scene.node_records {
        writeln!(out, "    case {}u: {{", record.id.0).ok();
        if matches!(record.kind, FieldNodeKindSummary::OpaqueLeaf) {
            writeln!(out, "      return wr_unavailable_normal_sample();").ok();
        } else {
            writeln!(
                out,
                "      return {}(point);",
                field_normal_function_name(field_index, record.id.0)
            )
            .ok();
        }
        writeln!(out, "    }}").ok();
    }
    writeln!(
        out,
        "    default: {{ return wr_unavailable_normal_sample(); }}"
    )
    .ok();
    writeln!(out, "  }}").ok();
    writeln!(out, "}}\n").ok();

    Ok(())
}

pub(super) fn emit_shape_scene_functions(
    ctx: &QueryExecContext,
    scene_index: &ShaderSceneIndex,
    ops: &DirectQueryOps<'_>,
    behavior: &NormalizedShaderBehavior,
    out: &mut String,
) -> Result<(), QueryExecError> {
    for (shape_name, scene) in &ctx.scene.shapes {
        let shape_index = scene_index.shape(shape_name)?;
        for record in &scene.node_records {
            emit_shape_distance_function(ctx, scene_index, shape_name, shape_index, record, out)?;
            emit_shape_normal_function(ctx, scene_index, shape_name, shape_index, record, out)?;
            if behavior.requires_trace() {
                emit_shape_winner_function(ctx, scene_index, shape_name, shape_index, record, out)?;
            }
            if behavior.requires_radiance {
                emit_shape_radiance_function(
                    ctx,
                    scene_index,
                    shape_name,
                    shape_index,
                    record,
                    out,
                )?;
            }
            if behavior.requires_volume {
                emit_shape_medium_function(
                    ctx,
                    scene_index,
                    ops,
                    shape_name,
                    shape_index,
                    record,
                    out,
                )?;
            }
        }
        if behavior.requires_material {
            emit_shape_surface_function(ctx, scene_index, shape_name, shape_index, out)?;
        }
    }
    Ok(())
}

pub(super) fn emit_scene_dispatch_functions(
    ctx: &QueryExecContext,
    scene_index: &ShaderSceneIndex,
    behavior: &NormalizedShaderBehavior,
    out: &mut String,
) -> Result<(), QueryExecError> {
    emit_field_dispatch_functions(ctx, scene_index, out)?;
    emit_shape_dispatch_functions(ctx, scene_index, behavior, out)?;
    Ok(())
}

pub(super) fn emit_field_dispatch_functions(
    ctx: &QueryExecContext,
    scene_index: &ShaderSceneIndex,
    out: &mut String,
) -> Result<(), QueryExecError> {
    writeln!(
        out,
        "fn field_distance_dispatch(field_index: u32, point: vec3<f32>) -> f32 {{"
    )
    .ok();
    writeln!(
        out,
        "  atomicAdd(&observability_metrics.field_samples, 1u);"
    )
    .ok();
    writeln!(out, "  switch field_index {{").ok();
    for (field_name, scene) in &ctx.scene.fields {
        let field_index = scene_index.field(field_name)?;
        if scene.opaque_boundary {
            writeln!(
                out,
                "    case {field_index}u: {{ return {}(point); }}",
                field_opaque_distance_function_name(field_index)
            )
            .ok();
        } else {
            writeln!(
                out,
                "    case {field_index}u: {{ return {}(point); }}",
                field_node_function_name(field_index, scene.root_node_id.0)
            )
            .ok();
        }
    }
    writeln!(out, "    default: {{ return 1000000.0; }}").ok();
    writeln!(out, "  }}").ok();
    writeln!(out, "}}\n").ok();

    writeln!(
        out,
        "fn field_local_frame_dispatch_with_ids(field_index: u32, point: vec3<f32>, instance_id: u32, repeat_id: u32) -> FieldLocalFrame {{"
    )
    .ok();
    writeln!(out, "  switch field_index {{").ok();
    for (field_name, scene) in &ctx.scene.fields {
        let field_index = scene_index.field(field_name)?;
        writeln!(
            out,
            "    case {field_index}u: {{ return {}(point, instance_id, repeat_id); }}",
            field_local_frame_function_name(field_index, scene.root_node_id.0)
        )
        .ok();
    }
    writeln!(
        out,
        "    default: {{ return FieldLocalFrame(point, instance_id, repeat_id, 0u); }}"
    )
    .ok();
    writeln!(out, "  }}").ok();
    writeln!(out, "}}\n").ok();

    writeln!(
        out,
        "fn field_local_frame_dispatch(field_index: u32, point: vec3<f32>) -> FieldLocalFrame {{"
    )
    .ok();
    writeln!(
        out,
        "  return field_local_frame_dispatch_with_ids(field_index, point, 0u, 0u);"
    )
    .ok();
    writeln!(out, "}}\n").ok();

    writeln!(
        out,
        "fn field_terminal_distance_dispatch(field_index: u32, terminal_node_id: u32, point: vec3<f32>) -> f32 {{"
    )
    .ok();
    writeln!(
        out,
        "  atomicAdd(&observability_metrics.field_samples, 1u);"
    )
    .ok();
    writeln!(out, "  switch field_index {{").ok();
    for (field_name, _scene) in &ctx.scene.fields {
        let field_index = scene_index.field(field_name)?;
        writeln!(
            out,
            "    case {field_index}u: {{ return {}(terminal_node_id, point); }}",
            field_terminal_distance_function_name(field_index)
        )
        .ok();
    }
    writeln!(out, "    default: {{ return 1000000.0; }}").ok();
    writeln!(out, "  }}").ok();
    writeln!(out, "}}\n").ok();

    writeln!(
        out,
        "fn field_terminal_normal_dispatch_sample(field_index: u32, terminal_node_id: u32, point: vec3<f32>) -> CertifiedNormalSample {{"
    )
    .ok();
    writeln!(out, "  switch field_index {{").ok();
    for (field_name, _scene) in &ctx.scene.fields {
        let field_index = scene_index.field(field_name)?;
        writeln!(
            out,
            "    case {field_index}u: {{ return {}(terminal_node_id, point); }}",
            field_terminal_normal_function_name(field_index)
        )
        .ok();
    }
    writeln!(
        out,
        "    default: {{ return wr_unavailable_normal_sample(); }}"
    )
    .ok();
    writeln!(out, "  }}").ok();
    writeln!(out, "}}\n").ok();

    writeln!(
        out,
        "fn field_normal_dispatch_sample(field_index: u32, point: vec3<f32>) -> CertifiedNormalSample {{"
    )
    .ok();
    writeln!(out, "  switch field_index {{").ok();
    for (field_name, scene) in &ctx.scene.fields {
        let field_index = scene_index.field(field_name)?;
        writeln!(
            out,
            "    case {field_index}u: {{ return {}(point); }}",
            field_normal_function_name(field_index, scene.root_node_id.0)
        )
        .ok();
    }
    writeln!(
        out,
        "    default: {{ return wr_unavailable_normal_sample(); }}"
    )
    .ok();
    writeln!(out, "  }}").ok();
    writeln!(out, "}}\n").ok();

    writeln!(
        out,
        "fn field_normal_dispatch(field_index: u32, point: vec3<f32>) -> vec3<f32> {{"
    )
    .ok();
    writeln!(
        out,
        "  let sample = field_normal_dispatch_sample(field_index, point);"
    )
    .ok();
    writeln!(
        out,
        "  if (sample.available != 0u) {{ return wr_normalize3(sample.normal); }}"
    )
    .ok();
    writeln!(out, "  let eps: f32 = 0.001;").ok();
    writeln!(
        out,
        "  let dx = field_distance_dispatch(field_index, point + vec3<f32>(eps, 0.0, 0.0)) - field_distance_dispatch(field_index, point - vec3<f32>(eps, 0.0, 0.0));"
    )
    .ok();
    writeln!(
        out,
        "  let dy = field_distance_dispatch(field_index, point + vec3<f32>(0.0, eps, 0.0)) - field_distance_dispatch(field_index, point - vec3<f32>(0.0, eps, 0.0));"
    )
    .ok();
    writeln!(
        out,
        "  let dz = field_distance_dispatch(field_index, point + vec3<f32>(0.0, 0.0, eps)) - field_distance_dispatch(field_index, point - vec3<f32>(0.0, 0.0, eps));"
    )
    .ok();
    writeln!(out, "  return wr_normalize3(vec3<f32>(dx, dy, dz));").ok();
    writeln!(out, "}}\n").ok();

    writeln!(
        out,
        "fn field_local_normal_dispatch(field_index: u32, frame: FieldLocalFrame) -> vec3<f32> {{"
    )
    .ok();
    writeln!(
        out,
        "  let sample = field_terminal_normal_dispatch_sample(field_index, frame.terminal_node_id, frame.point);"
    )
    .ok();
    writeln!(
        out,
        "  if (sample.available != 0u) {{ return wr_normalize3(sample.normal); }}"
    )
    .ok();
    writeln!(out, "  let eps: f32 = 0.001;").ok();
    writeln!(
        out,
        "  let dx = field_terminal_distance_dispatch(field_index, frame.terminal_node_id, frame.point + vec3<f32>(eps, 0.0, 0.0)) - field_terminal_distance_dispatch(field_index, frame.terminal_node_id, frame.point - vec3<f32>(eps, 0.0, 0.0));"
    )
    .ok();
    writeln!(
        out,
        "  let dy = field_terminal_distance_dispatch(field_index, frame.terminal_node_id, frame.point + vec3<f32>(0.0, eps, 0.0)) - field_terminal_distance_dispatch(field_index, frame.terminal_node_id, frame.point - vec3<f32>(0.0, eps, 0.0));"
    )
    .ok();
    writeln!(
        out,
        "  let dz = field_terminal_distance_dispatch(field_index, frame.terminal_node_id, frame.point + vec3<f32>(0.0, 0.0, eps)) - field_terminal_distance_dispatch(field_index, frame.terminal_node_id, frame.point - vec3<f32>(0.0, 0.0, eps));"
    )
    .ok();
    writeln!(out, "  return wr_normalize3(vec3<f32>(dx, dy, dz));").ok();
    writeln!(out, "}}\n").ok();

    Ok(())
}

pub(super) fn emit_shape_dispatch_functions(
    ctx: &QueryExecContext,
    scene_index: &ShaderSceneIndex,
    behavior: &NormalizedShaderBehavior,
    out: &mut String,
) -> Result<(), QueryExecError> {
    writeln!(
        out,
        "fn shape_distance_dispatch(shape_index: u32, point: vec3<f32>) -> f32 {{"
    )
    .ok();
    writeln!(
        out,
        "  atomicAdd(&observability_metrics.field_samples, 1u);"
    )
    .ok();
    writeln!(out, "  switch shape_index {{").ok();
    for (shape_name, scene) in &ctx.scene.shapes {
        let shape_index = scene_index.shape(shape_name)?;
        writeln!(
            out,
            "    case {shape_index}u: {{ return {}(point); }}",
            shape_distance_function_name(shape_index, scene.root_node_id.0)
        )
        .ok();
    }
    writeln!(out, "    default: {{ return 1000000.0; }}").ok();
    writeln!(out, "  }}").ok();
    writeln!(out, "}}\n").ok();

    if behavior.requires_trace() {
        writeln!(
            out,
            "fn shape_winner_dispatch(shape_index: u32, point: vec3<f32>) -> ShapeWinner {{"
        )
        .ok();
        writeln!(out, "  switch shape_index {{").ok();
        for (shape_name, scene) in &ctx.scene.shapes {
            let shape_index = scene_index.shape(shape_name)?;
            writeln!(
                out,
                "    case {shape_index}u: {{ return {}(point); }}",
                shape_winner_function_name(shape_index, scene.root_node_id.0)
            )
            .ok();
        }
        writeln!(out, "    default: {{ return wr_default_shape_winner(); }}").ok();
        writeln!(out, "  }}").ok();
        writeln!(out, "}}\n").ok();
    }

    writeln!(
        out,
        "fn shape_normal_dispatch_sample(shape_index: u32, point: vec3<f32>) -> CertifiedNormalSample {{"
    )
    .ok();
    writeln!(out, "  switch shape_index {{").ok();
    for (shape_name, scene) in &ctx.scene.shapes {
        let shape_index = scene_index.shape(shape_name)?;
        writeln!(
            out,
            "    case {shape_index}u: {{ return {}(point); }}",
            shape_normal_function_name(shape_index, scene.root_node_id.0)
        )
        .ok();
    }
    writeln!(
        out,
        "    default: {{ return wr_unavailable_normal_sample(); }}"
    )
    .ok();
    writeln!(out, "  }}").ok();
    writeln!(out, "}}\n").ok();

    writeln!(
        out,
        "fn shape_normal_dispatch(shape_index: u32, point: vec3<f32>) -> vec3<f32> {{"
    )
    .ok();
    writeln!(
        out,
        "  let sample = shape_normal_dispatch_sample(shape_index, point);"
    )
    .ok();
    writeln!(
        out,
        "  if (sample.available != 0u) {{ return wr_normalize3(sample.normal); }}"
    )
    .ok();
    writeln!(out, "  let eps: f32 = 0.001;").ok();
    writeln!(
        out,
        "  let dx = shape_distance_dispatch(shape_index, point + vec3<f32>(eps, 0.0, 0.0)) - shape_distance_dispatch(shape_index, point - vec3<f32>(eps, 0.0, 0.0));"
    )
    .ok();
    writeln!(
        out,
        "  let dy = shape_distance_dispatch(shape_index, point + vec3<f32>(0.0, eps, 0.0)) - shape_distance_dispatch(shape_index, point - vec3<f32>(0.0, eps, 0.0));"
    )
    .ok();
    writeln!(
        out,
        "  let dz = shape_distance_dispatch(shape_index, point + vec3<f32>(0.0, 0.0, eps)) - shape_distance_dispatch(shape_index, point - vec3<f32>(0.0, 0.0, eps));"
    )
    .ok();
    writeln!(out, "  return wr_normalize3(vec3<f32>(dx, dy, dz));").ok();
    writeln!(out, "}}\n").ok();

    if behavior.requires_material {
        writeln!(
            out,
            "fn surface_at_shape_dispatch(shape_index: u32, hit: Hit3) -> Surface {{"
        )
        .ok();
        writeln!(out, "  switch shape_index {{").ok();
        for (shape_name, _scene) in &ctx.scene.shapes {
            let shape_index = scene_index.shape(shape_name)?;
            writeln!(
                out,
                "    case {shape_index}u: {{ return {}(hit); }}",
                shape_surface_function_name(shape_index)
            )
            .ok();
        }
        writeln!(out, "    default: {{ return wr_default_surface(); }}").ok();
        writeln!(out, "  }}").ok();
        writeln!(out, "}}\n").ok();
    }

    if behavior.requires_radiance {
        writeln!(out, "fn radiance_at_shape_dispatch(shape_index: u32, point: vec3<f32>, direction: vec3<f32>) -> vec3<f32> {{").ok();
        writeln!(out, "  switch shape_index {{").ok();
        for (shape_name, scene) in &ctx.scene.shapes {
            let shape_index = scene_index.shape(shape_name)?;
            writeln!(
                out,
                "    case {shape_index}u: {{ return {}(point, direction); }}",
                shape_radiance_function_name(shape_index, scene.root_node_id.0)
            )
            .ok();
        }
        writeln!(out, "    default: {{ return vec3<f32>(0.0, 0.0, 0.0); }}").ok();
        writeln!(out, "  }}").ok();
        writeln!(out, "}}\n").ok();
    }

    if behavior.requires_volume {
        writeln!(
            out,
            "fn medium_at_shape_dispatch(shape_index: u32, point: vec3<f32>) -> Medium {{"
        )
        .ok();
        writeln!(out, "  switch shape_index {{").ok();
        for (shape_name, scene) in &ctx.scene.shapes {
            let shape_index = scene_index.shape(shape_name)?;
            writeln!(
                out,
                "    case {shape_index}u: {{ return {}(point); }}",
                shape_medium_function_name(shape_index, scene.root_node_id.0)
            )
            .ok();
        }
        writeln!(out, "    default: {{ return wr_default_medium(); }}").ok();
        writeln!(out, "  }}").ok();
        writeln!(out, "}}\n").ok();
    }

    if behavior.requires_trace() {
        writeln!(
            out,
            "fn root_shape_id_for_shape(shape_index: u32) -> u32 {{"
        )
        .ok();
        writeln!(out, "  switch shape_index {{").ok();
        for (shape_name, _scene) in &ctx.scene.shapes {
            let shape_index = scene_index.shape(shape_name)?;
            writeln!(
                out,
                "    case {shape_index}u: {{ return {}u; }}",
                crate::query_exec::stable_shape_capture_id(shape_name)
            )
            .ok();
        }
        writeln!(out, "    default: {{ return 0u; }}").ok();
        writeln!(out, "  }}").ok();
        writeln!(out, "}}\n").ok();
    }

    if behavior.requires_root_shape_lookup() {
        writeln!(
            out,
            "fn shape_index_from_root_shape_id(root_shape_id: u32) -> u32 {{"
        )
        .ok();
        writeln!(out, "  switch root_shape_id {{").ok();
        for (shape_name, _scene) in &ctx.scene.shapes {
            let shape_index = scene_index.shape(shape_name)?;
            writeln!(
                out,
                "    case {}u: {{ return {shape_index}u; }}",
                crate::query_exec::stable_shape_capture_id(shape_name)
            )
            .ok();
        }
        writeln!(out, "    default: {{ return 0xffffffffu; }}").ok();
        writeln!(out, "  }}").ok();
        writeln!(out, "}}\n").ok();
    }

    Ok(())
}

pub(super) fn field_node_function_name(field_index: u32, node_id: u32) -> String {
    format!("wr_field_{field_index}_node_{node_id}")
}

pub(super) fn field_local_frame_function_name(field_index: u32, node_id: u32) -> String {
    format!("wr_field_{field_index}_local_frame_{node_id}")
}

pub(super) fn field_terminal_distance_function_name(field_index: u32) -> String {
    format!("wr_field_{field_index}_terminal_distance")
}

pub(super) fn field_terminal_normal_function_name(field_index: u32) -> String {
    format!("wr_field_{field_index}_terminal_normal")
}

pub(super) fn field_normal_function_name(field_index: u32, node_id: u32) -> String {
    format!("wr_field_{field_index}_normal_{node_id}")
}

pub(super) fn field_opaque_distance_function_name(field_index: u32) -> String {
    format!("wr_field_{field_index}_opaque_distance")
}

pub(super) fn shape_distance_function_name(shape_index: u32, node_id: u32) -> String {
    format!("wr_shape_{shape_index}_distance_{node_id}")
}

pub(super) fn shape_normal_function_name(shape_index: u32, node_id: u32) -> String {
    format!("wr_shape_{shape_index}_normal_{node_id}")
}

pub(super) fn shape_winner_function_name(shape_index: u32, node_id: u32) -> String {
    format!("wr_shape_{shape_index}_winner_{node_id}")
}

pub(super) fn shape_radiance_function_name(shape_index: u32, node_id: u32) -> String {
    format!("wr_shape_{shape_index}_radiance_{node_id}")
}

pub(super) fn shape_medium_function_name(shape_index: u32, node_id: u32) -> String {
    format!("wr_shape_{shape_index}_medium_{node_id}")
}

pub(super) fn shape_surface_function_name(shape_index: u32) -> String {
    format!("wr_shape_{shape_index}_surface")
}
