//! Owns WGSL bind-group declarations for direct-query runtime buffers and ABI
//! records.
//! Does not own ABI definition or the helper functions that consume these
//! bindings.
//!
//! Key invariants:
//! - emitted bindings match the ABI names and bind-group indices expected by the
//!   runtime.
//! - storage/read-write polarity must reflect the actual shader usage for each
//!   buffer.
//! - binding order stays stable because the host runtime mirrors it exactly.
//!
//! Primary entrypoints:
//! - `emit_bindings`
//!
//! Failure modes / common pitfalls:
//! - changing one binding index here without updating the host runtime breaks
//!   every generated shader.
//! - rebuilding ABI names locally instead of using shared helpers can desync the
//!   binding surface from the rest of codegen.

use super::*;

pub(super) fn emit_bindings(
    dispatch_abi: &PortableAbiType,
    accel_node_abi: &PortableAbiType,
    cache_brick_abi: &PortableAbiType,
    shape_meta_abi: &PortableAbiType,
    item_abi: &PortableAbiType,
    result_abi: &PortableAbiType,
) -> Result<String, QueryExecError> {
    let mut out = String::new();
    writeln!(
        out,
        "@group({GPU_RUNTIME_FRAME_BIND_GROUP_INDEX}) @binding(0)"
    )
    .ok();
    writeln!(
        out,
        "var<storage, read> dispatch_config: {};",
        abi_type_name(dispatch_abi, "Abi_")?
    )
    .ok();
    writeln!(out, "struct InputBuffer {{").ok();
    writeln!(
        out,
        "  values: array<{}>,",
        abi_type_name(item_abi, "Abi_")?
    )
    .ok();
    writeln!(out, "}}\n").ok();
    writeln!(out, "struct ResultBuffer {{").ok();
    writeln!(
        out,
        "  values: array<{}>,",
        abi_type_name(result_abi, "Abi_")?
    )
    .ok();
    writeln!(out, "}}\n").ok();
    writeln!(out, "struct AccelNodeBuffer {{").ok();
    writeln!(
        out,
        "  values: array<{}>,",
        abi_type_name(accel_node_abi, "Abi_")?
    )
    .ok();
    writeln!(out, "}}\n").ok();
    writeln!(out, "struct ShapeIndexBuffer {{").ok();
    writeln!(out, "  values: array<u32>,").ok();
    writeln!(out, "}}\n").ok();
    writeln!(out, "struct ShapeMetaBuffer {{").ok();
    writeln!(
        out,
        "  values: array<{}>,",
        abi_type_name(shape_meta_abi, "Abi_")?
    )
    .ok();
    writeln!(out, "}}\n").ok();
    writeln!(out, "struct CacheBrickBuffer {{").ok();
    writeln!(
        out,
        "  values: array<{}>,",
        abi_type_name(cache_brick_abi, "Abi_")?
    )
    .ok();
    writeln!(out, "}}\n").ok();
    writeln!(out, "struct ContinuationSeedBuffer {{").ok();
    writeln!(out, "  values: array<u32>,").ok();
    writeln!(out, "}}\n").ok();
    writeln!(out, "struct WgslObservabilityBuffer {{").ok();
    writeln!(out, "  acceleration_node_visits: atomic<u32>,").ok();
    writeln!(out, "  shape_leaf_visits: atomic<u32>,").ok();
    writeln!(out, "  acceleration_pruned_nodes: atomic<u32>,").ok();
    writeln!(out, "  ray_support_interval_rejections: atomic<u32>,").ok();
    writeln!(out, "  ray_support_entry_jumps: atomic<u32>,").ok();
    writeln!(out, "  cache_brick_visits: atomic<u32>,").ok();
    writeln!(out, "  cache_brick_hits: atomic<u32>,").ok();
    writeln!(out, "  cache_brick_misses: atomic<u32>,").ok();
    writeln!(out, "  cache_interval_advances: atomic<u32>,").ok();
    writeln!(
        out,
        "  cache_resident_shared_snapshot_artifacts: atomic<u32>,"
    )
    .ok();
    writeln!(
        out,
        "  cache_resident_observer_local_artifacts: atomic<u32>,"
    )
    .ok();
    writeln!(out, "  cache_upload_attempts: atomic<u32>,").ok();
    writeln!(out, "  cache_upload_rejections: atomic<u32>,").ok();
    writeln!(out, "  cache_budget_rejections: atomic<u32>,").ok();
    writeln!(out, "  cache_dense_fallback_rays: atomic<u32>,").ok();
    writeln!(out, "  solver_analytic_hits: atomic<u32>,").ok();
    writeln!(out, "  solver_generated_dense_fallback_rays: atomic<u32>,").ok();
    writeln!(out, "  solver_support_rejections: atomic<u32>,").ok();
    writeln!(out, "}}\n").ok();
    writeln!(
        out,
        "@group({GPU_RUNTIME_SCENE_BIND_GROUP_INDEX}) @binding(0)"
    )
    .ok();
    writeln!(out, "var<storage, read> accel_nodes: AccelNodeBuffer;").ok();
    writeln!(
        out,
        "@group({GPU_RUNTIME_SCENE_BIND_GROUP_INDEX}) @binding(1)"
    )
    .ok();
    writeln!(out, "var<storage, read> accel_children: ShapeIndexBuffer;").ok();
    writeln!(
        out,
        "@group({GPU_RUNTIME_SCENE_BIND_GROUP_INDEX}) @binding(2)"
    )
    .ok();
    writeln!(out, "var<storage, read> shape_meta: ShapeMetaBuffer;").ok();
    writeln!(
        out,
        "@group({GPU_RUNTIME_SCENE_BIND_GROUP_INDEX}) @binding(3)"
    )
    .ok();
    writeln!(out, "var<storage, read> cache_bricks: CacheBrickBuffer;").ok();
    writeln!(
        out,
        "@group({GPU_RUNTIME_PASS_BIND_GROUP_INDEX}) @binding(0)"
    )
    .ok();
    writeln!(out, "var<storage, read> input_items: InputBuffer;").ok();
    writeln!(
        out,
        "@group({GPU_RUNTIME_PASS_BIND_GROUP_INDEX}) @binding(1)"
    )
    .ok();
    writeln!(out, "var<storage, read_write> output_items: ResultBuffer;").ok();
    writeln!(
        out,
        "@group({GPU_RUNTIME_PASS_BIND_GROUP_INDEX}) @binding(2)"
    )
    .ok();
    writeln!(out, "var<storage, read> world_shapes: ShapeIndexBuffer;").ok();
    writeln!(
        out,
        "@group({GPU_RUNTIME_PASS_BIND_GROUP_INDEX}) @binding(3)"
    )
    .ok();
    writeln!(
        out,
        "var<storage, read_write> observability_metrics: WgslObservabilityBuffer;"
    )
    .ok();
    writeln!(
        out,
        "@group({GPU_RUNTIME_SCRATCH_BIND_GROUP_INDEX}) @binding(0)"
    )
    .ok();
    writeln!(
        out,
        "var<storage, read> continuation_seeds: ContinuationSeedBuffer;"
    )
    .ok();
    Ok(out)
}

pub(super) fn emit_scene_functions(
    ctx: &QueryExecContext,
    scene_index: &ShaderSceneIndex,
    behavior: &NormalizedShaderBehavior,
) -> Result<String, QueryExecError> {
    let ops = DirectQueryOps::new(ctx);
    let mut out = String::new();
    emit_profile_helper_functions(ctx, &mut out)?;
    emit_field_scene_functions(ctx, scene_index, &ops, &mut out)?;
    emit_shape_scene_functions(ctx, scene_index, &ops, behavior, &mut out)?;
    emit_scene_dispatch_functions(ctx, scene_index, behavior, &mut out)?;
    Ok(out)
}

pub(super) fn emit_profile_helper_functions(
    ctx: &QueryExecContext,
    out: &mut String,
) -> Result<(), QueryExecError> {
    let mut polygon_arities = BTreeSet::new();
    let mut polyline_arities = BTreeSet::new();
    for scene in ctx.scene.fields.values() {
        collect_profile_helper_arities(&scene.root, &mut polygon_arities, &mut polyline_arities)?;
    }
    for arity in polygon_arities {
        emit_polygon_helper(out, arity)?;
        out.push('\n');
    }
    for arity in polyline_arities {
        emit_polyline_helper(out, arity)?;
        out.push('\n');
    }
    Ok(())
}

pub(super) fn collect_profile_helper_arities(
    node: &crate::scene_ir::FieldNode,
    polygon_arities: &mut BTreeSet<usize>,
    polyline_arities: &mut BTreeSet<usize>,
) -> Result<(), QueryExecError> {
    match node {
        crate::scene_ir::FieldNode::Union { items }
        | crate::scene_ir::FieldNode::Intersection { items }
        | crate::scene_ir::FieldNode::Smooth { items, .. } => {
            for item in items {
                collect_profile_helper_arities(item, polygon_arities, polyline_arities)?;
            }
        }
        crate::scene_ir::FieldNode::Subtract { left, right } => {
            collect_profile_helper_arities(left, polygon_arities, polyline_arities)?;
            collect_profile_helper_arities(right, polygon_arities, polyline_arities)?;
        }
        crate::scene_ir::FieldNode::Transform { inner, .. }
        | crate::scene_ir::FieldNode::Repeat { inner, .. } => {
            collect_profile_helper_arities(inner, polygon_arities, polyline_arities)?;
        }
        crate::scene_ir::FieldNode::Extrude { profile, .. }
        | crate::scene_ir::FieldNode::Revolve { profile }
        | crate::scene_ir::FieldNode::Sweep { profile, .. } => {
            if let Some(profile) = profile {
                collect_profile_arity(profile, polygon_arities, polyline_arities)?;
            }
        }
        crate::scene_ir::FieldNode::Loft { from, to, .. } => {
            if let Some(profile) = from {
                collect_profile_arity(profile, polygon_arities, polyline_arities)?;
            }
            if let Some(profile) = to {
                collect_profile_arity(profile, polygon_arities, polyline_arities)?;
            }
        }
        crate::scene_ir::FieldNode::Use { .. }
        | crate::scene_ir::FieldNode::Primitive { .. }
        | crate::scene_ir::FieldNode::OpaqueLeaf => {}
    }
    Ok(())
}

pub(super) fn collect_profile_arity(
    profile: &SceneProfileExpr,
    polygon_arities: &mut BTreeSet<usize>,
    polyline_arities: &mut BTreeSet<usize>,
) -> Result<(), QueryExecError> {
    let SceneProfileExpr::Primitive { primitive, args } = profile;
    match primitive {
        hir::ProfilePrimitive::Polygon2 => {
            let arity = scene_value_list_len(scene_named_arg_value(args, "vertices")?)?;
            if arity < 3 {
                return Err(QueryExecError::Unsupported {
                    message: format!("polygon2 requires at least 3 vertices, got {arity}"),
                });
            }
            polygon_arities.insert(arity);
        }
        hir::ProfilePrimitive::Polyline2 => {
            let arity = scene_value_list_len(scene_named_arg_value(args, "vertices")?)?;
            if arity < 2 {
                return Err(QueryExecError::Unsupported {
                    message: format!("polyline2 requires at least 2 vertices, got {arity}"),
                });
            }
            polyline_arities.insert(arity);
        }
        _ => {}
    }
    Ok(())
}

pub(super) fn emit_polygon_helper(out: &mut String, arity: usize) -> Result<(), QueryExecError> {
    writeln!(
        out,
        "fn wr_polygon2_n{arity}(point: vec2<f32>, vertices: array<vec2<f32>, {arity}>) -> f32 {{"
    )
    .ok();
    writeln!(out, "  var inside = false;").ok();
    writeln!(out, "  var best = 3.4028235e38;").ok();
    writeln!(
        out,
        "  for (var index: u32 = 0u; index < {arity}u; index = index + 1u) {{"
    )
    .ok();
    writeln!(out, "    let a = vertices[index];").ok();
    writeln!(out, "    let b = vertices[(index + 1u) % {arity}u];").ok();
    writeln!(
        out,
        "    best = min(best, wr_polygon2_edge_distance(point, a, b));"
    )
    .ok();
    writeln!(out, "    if (wr_polygon2_edge_crosses(point, a, b)) {{").ok();
    writeln!(out, "      inside = !inside;").ok();
    writeln!(out, "    }}").ok();
    writeln!(out, "  }}").ok();
    writeln!(out, "  return wr_polygon2_finalize(best, inside);").ok();
    writeln!(out, "}}").ok();
    Ok(())
}

pub(super) fn emit_polyline_helper(out: &mut String, arity: usize) -> Result<(), QueryExecError> {
    writeln!(
        out,
        "fn wr_polyline2_n{arity}(point: vec2<f32>, vertices: array<vec2<f32>, {arity}>) -> f32 {{"
    )
    .ok();
    writeln!(out, "  var best = 3.4028235e38;").ok();
    writeln!(
        out,
        "  for (var index: u32 = 0u; index + 1u < {arity}u; index = index + 1u) {{"
    )
    .ok();
    writeln!(out, "    let a = vertices[index];").ok();
    writeln!(out, "    let b = vertices[index + 1u];").ok();
    writeln!(
        out,
        "    best = min(best, wr_polyline2_edge_distance(point, a, b));"
    )
    .ok();
    writeln!(out, "  }}").ok();
    writeln!(out, "  return best;").ok();
    writeln!(out, "}}").ok();
    Ok(())
}
