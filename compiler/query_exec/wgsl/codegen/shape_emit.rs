//! Owns WGSL emission for authored field/shape primitives and their helper
//! calls inside direct-query shaders.
//! Does not own scene traversal or portable helper lowering.
//!
//! Key invariants:
//! - emitted primitive calls preserve authored primitive semantics and argument
//!   naming conventions.
//! - shape helper emission must stay compatible with the normalized query
//!   behavior chosen before codegen.
//! - unsupported primitive/value combinations fail loudly instead of being
//!   silently approximated.
//!
//! Primary entrypoints:
//! - `emit_field_primitive_call`
//! - `emit_shape_value_expr`
//! - `emit_shape_query_helpers`
//!
//! Failure modes / common pitfalls:
//! - local argument-name fallbacks that drift from scene IR conventions produce
//!   very hard-to-debug shader mismatches.
//! - mixing scene traversal responsibilities into this file weakens the module
//!   split from Phase 53.

use super::*;

pub(super) fn emit_field_primitive_call(
    ops: &DirectQueryOps<'_>,
    primitive: hir::FieldPrimitive,
    args: &[crate::scene_ir::SceneArgExpr],
    point_expr: &str,
) -> Result<String, QueryExecError> {
    Ok(match primitive {
        hir::FieldPrimitive::Sphere => format!(
            "wr_sphere({}, {})",
            point_expr,
            scene_named_arg_literal(ops, args, "radius")?
        ),
        hir::FieldPrimitive::Box => format!(
            "wr_box({}, {})",
            point_expr,
            scene_named_arg_literal(ops, args, "half").or_else(|_| scene_named_arg_literal(
                ops,
                args,
                "half_size"
            ))?
        ),
        hir::FieldPrimitive::Capsule => format!(
            "wr_capsule({}, {}, {}, {})",
            point_expr,
            scene_named_arg_literal(ops, args, "a")?,
            scene_named_arg_literal(ops, args, "b")?,
            scene_named_arg_literal(ops, args, "radius")?
        ),
        hir::FieldPrimitive::Cylinder => format!(
            "wr_cylinder({}, {}, {})",
            point_expr,
            scene_named_arg_literal(ops, args, "radius")?,
            scene_named_arg_literal(ops, args, "half_height")?
        ),
        hir::FieldPrimitive::Plane => format!(
            "wr_plane({}, {}, {})",
            point_expr,
            scene_named_arg_literal(ops, args, "normal")?,
            scene_named_arg_literal(ops, args, "offset")?
        ),
        hir::FieldPrimitive::Torus => format!(
            "wr_torus({}, {}, {})",
            point_expr,
            scene_named_arg_literal(ops, args, "major_radius")?,
            scene_named_arg_literal(ops, args, "minor_radius")?
        ),
        hir::FieldPrimitive::RoundedBox => format!(
            "wr_rounded_box({}, {}, {})",
            point_expr,
            scene_named_arg_literal(ops, args, "half")?,
            scene_named_arg_literal(ops, args, "radius")?
        ),
        hir::FieldPrimitive::Ellipsoid => format!(
            "wr_ellipsoid({}, {})",
            point_expr,
            scene_named_arg_literal(ops, args, "radii")?
        ),
        hir::FieldPrimitive::Cone => format!(
            "wr_cone({}, {}, {})",
            point_expr,
            scene_named_arg_literal(ops, args, "radius")?,
            scene_named_arg_literal(ops, args, "half_height")?
        ),
        hir::FieldPrimitive::CappedCone => format!(
            "wr_capped_cone({}, {}, {}, {})",
            point_expr,
            scene_named_arg_literal(ops, args, "radius_bottom")?,
            scene_named_arg_literal(ops, args, "radius_top")?,
            scene_named_arg_literal(ops, args, "half_height")?
        ),
        hir::FieldPrimitive::BoxFrame => format!(
            "wr_box_frame({}, {}, {})",
            point_expr,
            scene_named_arg_literal(ops, args, "half")?,
            scene_named_arg_literal(ops, args, "thickness")?
        ),
        hir::FieldPrimitive::Slab => format!(
            "wr_slab({}, {})",
            point_expr,
            scene_named_arg_literal(ops, args, "thickness")?
        ),
        hir::FieldPrimitive::TrianglePrism => format!(
            "wr_triangle_prism({}, {}, {})",
            point_expr,
            scene_named_arg_literal(ops, args, "half")?,
            scene_named_arg_literal(ops, args, "half_height")?
        ),
        hir::FieldPrimitive::HexPrism => format!(
            "wr_hex_prism({}, {}, {})",
            point_expr,
            scene_named_arg_literal(ops, args, "half")?,
            scene_named_arg_literal(ops, args, "half_height")?
        ),
    })
}

pub(super) fn emit_shape_distance_function(
    ctx: &QueryExecContext,
    scene_index: &ShaderSceneIndex,
    shape_name: &SmolStr,
    shape_index: u32,
    record: &crate::scene_ir::ShapeNodeRecord,
    out: &mut String,
) -> Result<(), QueryExecError> {
    let fn_name = shape_distance_function_name(shape_index, record.id.0);
    writeln!(out, "fn {fn_name}(point: vec3<f32>) -> f32 {{").ok();
    match record.kind {
        ShapeNodeKindSummary::Use => {
            let target = record.target.as_ref().expect("shape use target");
            let target_index = scene_index.shape(target)?;
            let target_scene =
                ctx.scene
                    .shapes
                    .get(target)
                    .ok_or_else(|| QueryExecError::Unsupported {
                        message: format!(
                            "shape '{}' is missing target '{}' during WGSL emission",
                            shape_name, target
                        ),
                    })?;
            writeln!(
                out,
                "  return {}(point);",
                shape_distance_function_name(target_index, target_scene.root_node_id.0)
            )
            .ok();
        }
        ShapeNodeKindSummary::Leaf => {
            let leaf_id = record.leaf.expect("shape leaf id");
            let leaf =
                ctx.shape_leaf(shape_name, leaf_id)
                    .ok_or_else(|| QueryExecError::Unsupported {
                        message: format!(
                            "shape '{}' is missing leaf {} during WGSL emission",
                            shape_name, leaf_id.0
                        ),
                    })?;
            writeln!(
                out,
                "  return field_distance_dispatch({}, point);",
                scene_index.field(&leaf.field)?
            )
            .ok();
        }
        ShapeNodeKindSummary::Union => {
            writeln!(out, "  var current: f32 = 1000000.0;").ok();
            for child in &record.children {
                writeln!(
                    out,
                    "  current = wr_field_union(current, {}(point));",
                    shape_distance_function_name(shape_index, child.0)
                )
                .ok();
            }
            writeln!(out, "  return current;").ok();
        }
        ShapeNodeKindSummary::Intersection => {
            if let Some(first) = record.children.first() {
                writeln!(
                    out,
                    "  var current: f32 = {}(point);",
                    shape_distance_function_name(shape_index, first.0)
                )
                .ok();
                for child in record.children.iter().skip(1) {
                    writeln!(
                        out,
                        "  current = wr_field_intersection(current, {}(point));",
                        shape_distance_function_name(shape_index, child.0)
                    )
                    .ok();
                }
                writeln!(out, "  return current;").ok();
            } else {
                writeln!(out, "  return 1000000.0;").ok();
            }
        }
        ShapeNodeKindSummary::Subtract => {
            let left = record.children.first().copied();
            let right = record.children.get(1).copied();
            if let (Some(left), Some(right)) = (left, right) {
                writeln!(
                    out,
                    "  return wr_field_subtract({}(point), {}(point));",
                    shape_distance_function_name(shape_index, left.0),
                    shape_distance_function_name(shape_index, right.0)
                )
                .ok();
            } else {
                writeln!(out, "  return 1000000.0;").ok();
            }
        }
    }
    writeln!(out, "}}\n").ok();
    Ok(())
}

pub(super) fn emit_shape_normal_function(
    ctx: &QueryExecContext,
    scene_index: &ShaderSceneIndex,
    shape_name: &SmolStr,
    shape_index: u32,
    record: &crate::scene_ir::ShapeNodeRecord,
    out: &mut String,
) -> Result<(), QueryExecError> {
    let fn_name = shape_normal_function_name(shape_index, record.id.0);
    writeln!(
        out,
        "fn {fn_name}(point: vec3<f32>) -> CertifiedNormalSample {{"
    )
    .ok();
    match record.kind {
        ShapeNodeKindSummary::Use => {
            let target = record.target.as_ref().expect("shape use target");
            let target_index = scene_index.shape(target)?;
            let target_scene =
                ctx.scene
                    .shapes
                    .get(target)
                    .ok_or_else(|| QueryExecError::Unsupported {
                        message: format!(
                            "shape '{}' is missing target '{}' during WGSL emission",
                            shape_name, target
                        ),
                    })?;
            writeln!(
                out,
                "  return {}(point);",
                shape_normal_function_name(target_index, target_scene.root_node_id.0)
            )
            .ok();
        }
        ShapeNodeKindSummary::Leaf => {
            let leaf_id = record.leaf.expect("shape leaf id");
            let leaf =
                ctx.shape_leaf(shape_name, leaf_id)
                    .ok_or_else(|| QueryExecError::Unsupported {
                        message: format!(
                            "shape '{}' is missing leaf {} during WGSL emission",
                            shape_name, leaf_id.0
                        ),
                    })?;
            writeln!(
                out,
                "  let field_sample = field_normal_dispatch_sample({}, point);",
                scene_index.field(&leaf.field)?
            )
            .ok();
            writeln!(
                out,
                "  if (field_sample.available == 0u) {{ return field_sample; }}"
            )
            .ok();
            writeln!(
                out,
                "  return wr_feature_normal_sample(field_sample.normal);"
            )
            .ok();
        }
        ShapeNodeKindSummary::Union
        | ShapeNodeKindSummary::Intersection
        | ShapeNodeKindSummary::Subtract => {
            writeln!(out, "  return wr_unavailable_normal_sample();").ok();
        }
    }
    writeln!(out, "}}\n").ok();
    Ok(())
}

pub(super) fn emit_shape_winner_function(
    ctx: &QueryExecContext,
    scene_index: &ShaderSceneIndex,
    shape_name: &SmolStr,
    shape_index: u32,
    record: &crate::scene_ir::ShapeNodeRecord,
    out: &mut String,
) -> Result<(), QueryExecError> {
    let fn_name = shape_winner_function_name(shape_index, record.id.0);
    writeln!(out, "fn {fn_name}(point: vec3<f32>) -> ShapeWinner {{").ok();
    let scene = ctx.scene.shapes.get(shape_name).expect("shape scene");
    match record.kind {
        ShapeNodeKindSummary::Use => {
            let target = record.target.as_ref().expect("shape use target");
            let target_index = scene_index.shape(target)?;
            let target_scene =
                ctx.scene
                    .shapes
                    .get(target)
                    .ok_or_else(|| QueryExecError::Unsupported {
                        message: format!(
                            "shape '{}' is missing target '{}' during WGSL emission",
                            shape_name, target
                        ),
                    })?;
            writeln!(
                out,
                "  return {}(point);",
                shape_winner_function_name(target_index, target_scene.root_node_id.0)
            )
            .ok();
        }
        ShapeNodeKindSummary::Leaf => {
            let leaf_id = record.leaf.expect("shape leaf id");
            let leaf =
                ctx.shape_leaf(shape_name, leaf_id)
                    .ok_or_else(|| QueryExecError::Unsupported {
                        message: format!(
                            "shape '{}' is missing leaf {} during WGSL emission",
                            shape_name, leaf_id.0
                        ),
                    })?;
            let leaf_scene_index = scene_index.shape(shape_name)?;
            let field_index = scene_index.field(&leaf.field)?;
            writeln!(
                out,
                "  return ShapeWinner(field_distance_dispatch({field_index}u, point), {}u, 1u, {}u, {}u, {field_index}u);",
                leaf.feature_id,
                leaf_scene_index,
                leaf_id.0
            )
            .ok();
        }
        ShapeNodeKindSummary::Union => {
            emit_shape_merge_winner(
                record,
                out,
                shape_index,
                scene
                    .provenance_record(record.id)
                    .and_then(|record| match record.policy {
                        ShapeNodeProvenancePolicy::Union(policy) => Some(policy),
                        _ => None,
                    })
                    .unwrap_or(ShapeMergeProvenancePolicy::Nearest),
                true,
            )?;
        }
        ShapeNodeKindSummary::Intersection => {
            emit_shape_merge_winner(
                record,
                out,
                shape_index,
                scene
                    .provenance_record(record.id)
                    .and_then(|record| match record.policy {
                        ShapeNodeProvenancePolicy::Intersection(policy) => Some(policy),
                        _ => None,
                    })
                    .unwrap_or(ShapeMergeProvenancePolicy::Nearest),
                false,
            )?;
        }
        ShapeNodeKindSummary::Subtract => {
            let left = record.children.first().copied();
            let right = record.children.get(1).copied();
            let policy = scene
                .provenance_record(record.id)
                .and_then(|record| match record.policy {
                    ShapeNodeProvenancePolicy::Subtract(policy) => Some(policy),
                    _ => None,
                })
                .unwrap_or(ShapeSubtractProvenancePolicy::Left);
            if let (Some(left), Some(right)) = (left, right) {
                writeln!(
                    out,
                    "  let left = {}(point);",
                    shape_winner_function_name(shape_index, left.0)
                )
                .ok();
                writeln!(
                    out,
                    "  let right = {}(point);",
                    shape_winner_function_name(shape_index, right.0)
                )
                .ok();
                writeln!(out, "  let neg_right = -right.distance;").ok();
                writeln!(out, "  if (left.distance >= neg_right) {{ return left; }}").ok();
                let chooser = match policy {
                    ShapeSubtractProvenancePolicy::Left => "left",
                    ShapeSubtractProvenancePolicy::Right => "right",
                };
                writeln!(
                    out,
                    "  return ShapeWinner(neg_right, {chooser}.feature_id, {chooser}.has_leaf, {chooser}.leaf_scene_index, {chooser}.leaf_id, {chooser}.field_index);"
                )
                .ok();
            } else {
                writeln!(out, "  return wr_default_shape_winner();").ok();
            }
        }
    }
    writeln!(out, "}}\n").ok();
    Ok(())
}

pub(super) fn emit_shape_merge_winner(
    record: &crate::scene_ir::ShapeNodeRecord,
    out: &mut String,
    shape_index: u32,
    policy: ShapeMergeProvenancePolicy,
    is_union: bool,
) -> Result<(), QueryExecError> {
    if let Some(first) = record.children.first() {
        writeln!(
            out,
            "  var current = {}(point);",
            shape_winner_function_name(shape_index, first.0)
        )
        .ok();
        for (index, child) in record.children.iter().skip(1).enumerate() {
            let next_name = format!("next_{index}");
            writeln!(
                out,
                "  let {next_name} = {}(point);",
                shape_winner_function_name(shape_index, child.0),
            )
            .ok();
            match policy {
                ShapeMergeProvenancePolicy::Ordered => {
                    writeln!(
                        out,
                        "  current.distance = {}(current.distance, {next_name}.distance);",
                        if is_union {
                            "wr_field_union"
                        } else {
                            "wr_field_intersection"
                        }
                    )
                    .ok();
                }
                ShapeMergeProvenancePolicy::Nearest => {
                    writeln!(
                        out,
                        "  if ({next_name}.distance {} current.distance) {{ current = {next_name}; }}",
                        if is_union { "<" } else { ">" }
                    )
                    .ok();
                }
            }
        }
        writeln!(out, "  return current;").ok();
    } else {
        writeln!(out, "  return wr_default_shape_winner();").ok();
    }
    Ok(())
}

pub(super) fn emit_shape_radiance_function(
    ctx: &QueryExecContext,
    scene_index: &ShaderSceneIndex,
    shape_name: &SmolStr,
    shape_index: u32,
    record: &crate::scene_ir::ShapeNodeRecord,
    out: &mut String,
) -> Result<(), QueryExecError> {
    let fn_name = shape_radiance_function_name(shape_index, record.id.0);
    writeln!(
        out,
        "fn {fn_name}(point: vec3<f32>, direction: vec3<f32>) -> vec3<f32> {{"
    )
    .ok();
    match record.kind {
        ShapeNodeKindSummary::Use => {
            let target = record.target.as_ref().expect("shape use target");
            let target_index = scene_index.shape(target)?;
            let target_scene =
                ctx.scene
                    .shapes
                    .get(target)
                    .ok_or_else(|| QueryExecError::Unsupported {
                        message: format!(
                            "shape '{}' is missing target '{}' during WGSL emission",
                            shape_name, target
                        ),
                    })?;
            writeln!(
                out,
                "  return {}(point, direction);",
                shape_radiance_function_name(target_index, target_scene.root_node_id.0)
            )
            .ok();
        }
        ShapeNodeKindSummary::Leaf => {
            let leaf_id = record.leaf.expect("shape leaf id");
            let leaf =
                ctx.shape_leaf(shape_name, leaf_id)
                    .ok_or_else(|| QueryExecError::Unsupported {
                        message: format!(
                            "shape '{}' is missing leaf {} during WGSL emission",
                            shape_name, leaf_id.0
                        ),
                    })?;
            if let Some(radiance) = &leaf.radiance {
                let field_index = scene_index.field(&leaf.field)?;
                writeln!(
                    out,
                    "  let frame = field_local_frame_dispatch({field_index}u, point);"
                )
                .ok();
                writeln!(
                    out,
                    "  return {}(frame.point, direction, {}u);",
                    portable_function_name(radiance),
                    leaf.feature_id
                )
                .ok();
            } else {
                writeln!(out, "  return vec3<f32>(0.0, 0.0, 0.0);").ok();
            }
        }
        ShapeNodeKindSummary::Union | ShapeNodeKindSummary::Intersection => {
            writeln!(out, "  var total = vec3<f32>(0.0, 0.0, 0.0);").ok();
            for child in &record.children {
                writeln!(
                    out,
                    "  total = total + {}(point, direction);",
                    shape_radiance_function_name(shape_index, child.0)
                )
                .ok();
            }
            writeln!(out, "  return total;").ok();
        }
        ShapeNodeKindSummary::Subtract => {
            let left = record.children.first().copied();
            let right = record.children.get(1).copied();
            if let (Some(left), Some(right)) = (left, right) {
                writeln!(
                    out,
                    "  return {}(point, direction) + {}(point, direction);",
                    shape_radiance_function_name(shape_index, left.0),
                    shape_radiance_function_name(shape_index, right.0)
                )
                .ok();
            } else {
                writeln!(out, "  return vec3<f32>(0.0, 0.0, 0.0);").ok();
            }
        }
    }
    writeln!(out, "}}\n").ok();
    Ok(())
}

pub(super) fn emit_shape_medium_function(
    ctx: &QueryExecContext,
    scene_index: &ShaderSceneIndex,
    _ops: &DirectQueryOps<'_>,
    shape_name: &SmolStr,
    shape_index: u32,
    record: &crate::scene_ir::ShapeNodeRecord,
    out: &mut String,
) -> Result<(), QueryExecError> {
    let fn_name = shape_medium_function_name(shape_index, record.id.0);
    writeln!(out, "fn {fn_name}(point: vec3<f32>) -> Medium {{").ok();
    match record.kind {
        ShapeNodeKindSummary::Use => {
            let target = record.target.as_ref().expect("shape use target");
            let target_index = scene_index.shape(target)?;
            let target_scene =
                ctx.scene
                    .shapes
                    .get(target)
                    .ok_or_else(|| QueryExecError::Unsupported {
                        message: format!(
                            "shape '{}' is missing target '{}' during WGSL emission",
                            shape_name, target
                        ),
                    })?;
            writeln!(
                out,
                "  return {}(point);",
                shape_medium_function_name(target_index, target_scene.root_node_id.0)
            )
            .ok();
        }
        ShapeNodeKindSummary::Leaf => {
            let leaf_id = record.leaf.expect("shape leaf id");
            let leaf =
                ctx.shape_leaf(shape_name, leaf_id)
                    .ok_or_else(|| QueryExecError::Unsupported {
                        message: format!(
                            "shape '{}' is missing leaf {} during WGSL emission",
                            shape_name, leaf_id.0
                        ),
                    })?;
            if let Some(volume) = &leaf.volume {
                let field_index = scene_index.field(&leaf.field)?;
                writeln!(
                    out,
                    "  let frame = field_local_frame_dispatch({field_index}u, point);"
                )
                .ok();
                writeln!(
                    out,
                    "  let surface_distance = field_terminal_distance_dispatch({field_index}u, frame.terminal_node_id, frame.point);"
                )
                .ok();
                writeln!(
                    out,
                    "  return {}(frame.point, surface_distance);",
                    portable_function_name(volume)
                )
                .ok();
            } else {
                writeln!(out, "  return wr_default_medium();").ok();
            }
        }
        ShapeNodeKindSummary::Union | ShapeNodeKindSummary::Intersection => {
            writeln!(out, "  var total = wr_default_medium();").ok();
            for child in &record.children {
                writeln!(
                    out,
                    "  total = wr_combine_medium_values(total, {}(point));",
                    shape_medium_function_name(shape_index, child.0)
                )
                .ok();
            }
            writeln!(out, "  return total;").ok();
        }
        ShapeNodeKindSummary::Subtract => {
            let left = record.children.first().copied();
            let right = record.children.get(1).copied();
            if let (Some(left), Some(right)) = (left, right) {
                writeln!(
                    out,
                    "  return wr_combine_medium_values({}(point), {}(point));",
                    shape_medium_function_name(shape_index, left.0),
                    shape_medium_function_name(shape_index, right.0)
                )
                .ok();
            } else {
                writeln!(out, "  return wr_default_medium();").ok();
            }
        }
    }
    writeln!(out, "}}\n").ok();
    Ok(())
}

pub(super) fn emit_shape_surface_function(
    ctx: &QueryExecContext,
    _scene_index: &ShaderSceneIndex,
    shape_name: &SmolStr,
    shape_index: u32,
    out: &mut String,
) -> Result<(), QueryExecError> {
    let scene = ctx.scene.shapes.get(shape_name).expect("shape scene");
    writeln!(
        out,
        "fn {}(hit: Hit3) -> Surface {{",
        shape_surface_function_name(shape_index)
    )
    .ok();
    writeln!(out, "  switch hit.feature_id {{").ok();
    for (feature_id, leaf_ref) in &scene.feature_leaves {
        let leaf = ctx
            .shape_leaf(&leaf_ref.scene, leaf_ref.leaf)
            .ok_or_else(|| QueryExecError::Unsupported {
                message: format!(
                    "shape '{}' is missing leaf {} during surface WGSL emission",
                    leaf_ref.scene, leaf_ref.leaf.0
                ),
            })?;
        writeln!(
            out,
            "    case {}u: {{ return {}(hit); }}",
            feature_id,
            portable_function_name(&leaf.material)
        )
        .ok();
    }
    writeln!(out, "    default: {{ return wr_default_surface(); }}").ok();
    writeln!(out, "  }}").ok();
    writeln!(out, "}}\n").ok();
    Ok(())
}
