//! Owns portable helper discovery/lowering for WGSL direct-query codegen.
//! Does not own final WGSL text emission or query-plan selection.
//!
//! Key invariants:
//! - helper roots collected here must match the material/radiance/volume needs
//!   implied by the normalized shader behavior.
//! - lowered portable helper graphs preserve dependency order so emitters can
//!   serialize them deterministically.
//! - helper selection remains descriptor/behavior driven rather than hard-coded
//!   to one query family.
//!
//! Primary entrypoints:
//! - `emit_portable_functions`
//! - `lower_portable_functions`
//! - `collect_portable_helper_roots`
//!
//! Failure modes / common pitfalls:
//! - missing one behavior-driven root here can compile a shader that typechecks
//!   but returns incomplete results.
//! - letting dependency traversal become nondeterministic makes generated WGSL
//!   snapshots noisy and harder to review.

use super::*;

pub(super) fn emit_portable_functions(
    ctx: &QueryExecContext,
    behavior: &NormalizedShaderBehavior,
) -> Result<String, QueryExecError> {
    let mut lowered = BTreeMap::<SmolStr, pir::ir::PirFunction>::new();
    let mut roots = BTreeSet::new();
    for scene in ctx.scene.shapes.values() {
        for leaf in scene.leaves.values() {
            if behavior.requires_material {
                roots.insert(leaf.material.clone());
            }
            if behavior.requires_radiance
                && let Some(radiance) = &leaf.radiance
            {
                roots.insert(radiance.clone());
            }
            if behavior.requires_volume
                && let Some(volume) = &leaf.volume
            {
                roots.insert(volume.clone());
            }
        }
    }

    for root in roots {
        let module = pir::lower_portable_entry_by_name(&ctx.module, &ctx.type_info, root.as_str())
            .map_err(|errors| QueryExecError::Unsupported {
                message: format!(
                    "failed to lower portable WGSL function '{}': {errors:?}",
                    root
                ),
            })?;
        for function in module.functions {
            lowered.entry(function.name.clone()).or_insert(function);
        }
    }

    let mut out = String::new();
    let mut scratch = 0usize;
    for function in lowered.values() {
        emit_pir_function(function, &mut scratch, &mut out)?;
        out.push('\n');
    }
    Ok(out)
}

pub(super) fn emit_query_helpers(
    ctx: &QueryExecContext,
    scene_index: &ShaderSceneIndex,
    behavior: &NormalizedShaderBehavior,
) -> Result<String, QueryExecError> {
    let ops = DirectQueryOps::new(ctx);
    let mut out = String::new();

    if behavior.requires_trace() {
        emit_payload_lookup_function(ctx, scene_index, &ops, &mut out)?;

        writeln!(
            out,
            "fn wr_cache_first_interval(ray: RayQuery, start_travel: f32) -> WgslRayInterval {{"
        )
        .ok();
        writeln!(
            out,
            "  atomicAdd(&observability_metrics.cache_brick_visits, 1u);"
        )
        .ok();
        writeln!(
            out,
            "  if (dispatch_config.cache_brick_count == 0u) {{ atomicAdd(&observability_metrics.cache_brick_misses, 1u); return WgslRayInterval(0.0, 0.0, 0u); }}"
        )
        .ok();
        writeln!(out, "  var found = WgslRayInterval(0.0, 0.0, 0u);").ok();
        writeln!(
            out,
            "  for (var index: u32 = 0u; index < dispatch_config.cache_brick_count; index = index + 1u) {{"
        )
        .ok();
        writeln!(out, "    let brick = cache_bricks.values[index];").ok();
        writeln!(
            out,
            "    let interval = wr_ray_aabb_interval(ray.origin, ray.direction, brick.min, brick.max);"
        )
        .ok();
        writeln!(
            out,
            "    if (interval.accepted == 0u || interval.end_t < max(start_travel, 0.0) || interval.start_t > ray.max_distance) {{ continue; }}"
        )
        .ok();
        writeln!(
            out,
            "    if (found.accepted == 0u || interval.start_t < found.start_t) {{ found = interval; }}"
        )
        .ok();
        writeln!(out, "  }}").ok();
        writeln!(
            out,
            "  if (found.accepted == 0u) {{ atomicAdd(&observability_metrics.cache_brick_misses, 1u); return found; }}"
        )
        .ok();
        writeln!(
            out,
            "  atomicAdd(&observability_metrics.cache_brick_hits, 1u);"
        )
        .ok();
        writeln!(
            out,
            "  if (max(found.start_t, 0.0) > max(start_travel, 0.0)) {{ atomicAdd(&observability_metrics.cache_interval_advances, 1u); atomicAdd(&observability_metrics.ray_support_entry_jumps, 1u); }}"
        )
        .ok();
        writeln!(out, "  return found;").ok();
        writeln!(out, "}}\n").ok();

        writeln!(
            out,
            "fn trace_shape_for_index(shape_index: u32, origin: vec3<f32>, direction: vec3<f32>, start_travel: f32, max_distance: f32, min_step: f32, hit_epsilon: f32, max_steps: i32) -> Hit3 {{"
        )
        .ok();
        writeln!(out, "  var travel: f32 = max(start_travel, 0.0);").ok();
        writeln!(
            out,
            "  if (dispatch_config.capture_kind != 2u) {{ let cache_interval = wr_cache_first_interval(RayQuery(origin, direction, max_distance, min_step, hit_epsilon, max_steps), travel); if (cache_interval.accepted != 0u) {{ travel = max(travel, max(cache_interval.start_t, 0.0)); }} else {{ atomicAdd(&observability_metrics.cache_dense_fallback_rays, 1u); }} }}"
        )
        .ok();
        writeln!(out, "  var steps: i32 = 0;").ok();
        writeln!(out, "  loop {{").ok();
        writeln!(
            out,
            "    if (!(steps < max_steps && travel <= max_distance)) {{ break; }}"
        )
        .ok();
        writeln!(out, "    let point = origin + direction * travel;").ok();
        writeln!(
            out,
            "    let distance = shape_distance_dispatch(shape_index, point);"
        )
        .ok();
        writeln!(out, "    if (distance <= hit_epsilon) {{").ok();
        writeln!(
            out,
            "      let normal = shape_normal_dispatch(shape_index, point);"
        )
        .ok();
        writeln!(
            out,
            "      let winner = shape_winner_dispatch(shape_index, point);"
        )
        .ok();
        writeln!(out, "      if (winner.has_leaf != 0u) {{").ok();
        writeln!(
            out,
            "        let frame = field_local_frame_dispatch(winner.field_index, point);"
        )
        .ok();
        writeln!(
            out,
            "        let local_normal = field_local_normal_dispatch(winner.field_index, frame);"
        )
        .ok();
        writeln!(
            out,
            "        let payload = payload_for_shape_leaf(winner.leaf_scene_index, winner.leaf_id);"
        )
        .ok();
        writeln!(
            out,
            "        return wr_hit_value(true, travel, point, normal, frame.point, local_normal, steps + 1, winner.feature_id, frame.instance_id, frame.repeat_id, root_shape_id_for_shape(shape_index), payload);"
        )
        .ok();
        writeln!(
            out,
            "      }} else {{ return wr_hit_value(true, travel, point, normal, point, normal, steps + 1, 0u, 0u, 0u, root_shape_id_for_shape(shape_index), wr_default_payload()); }}"
        )
        .ok();
        writeln!(out, "    }}").ok();
        writeln!(out, "    travel = travel + max(distance, min_step);").ok();
        writeln!(out, "    steps = steps + 1;").ok();
        writeln!(out, "  }}").ok();
        writeln!(
            out,
            "  atomicAdd(&observability_metrics.cache_dense_fallback_rays, 1u);"
        )
        .ok();
        writeln!(out, "  return wr_default_hit(origin);").ok();
        writeln!(out, "}}\n").ok();

        emit_world_trace_helpers(ctx, scene_index, &ops, behavior, &mut out)?;
    }

    emit_world_acceleration_helpers(behavior, &mut out)?;

    writeln!(out, "fn world_distance_point(point: vec3<f32>) -> f32 {{").ok();
    writeln!(out, "  return world_distance_point_accel(point);").ok();
    writeln!(out, "}}\n").ok();

    writeln!(
        out,
        "fn world_normal_point(point: vec3<f32>) -> vec3<f32> {{"
    )
    .ok();
    out.push_str("  if (dispatch_config.shape_count == 1u) {\n");
    out.push_str("    let sample = shape_normal_dispatch_sample(world_shapes.values[0], point);\n");
    out.push_str("    if (sample.available != 0u) { return wr_normalize3(sample.normal); }\n");
    out.push_str("  }\n");
    writeln!(out, "  let eps: f32 = 0.001;").ok();
    writeln!(
        out,
        "  let dx = world_distance_point(point + vec3<f32>(eps, 0.0, 0.0)) - world_distance_point(point - vec3<f32>(eps, 0.0, 0.0));"
    )
    .ok();
    writeln!(
        out,
        "  let dy = world_distance_point(point + vec3<f32>(0.0, eps, 0.0)) - world_distance_point(point - vec3<f32>(0.0, eps, 0.0));"
    )
    .ok();
    writeln!(
        out,
        "  let dz = world_distance_point(point + vec3<f32>(0.0, 0.0, eps)) - world_distance_point(point - vec3<f32>(0.0, 0.0, eps));"
    )
    .ok();
    writeln!(out, "  return wr_normalize3(vec3<f32>(dx, dy, dz));").ok();
    writeln!(out, "}}\n").ok();

    if matches!(
        behavior.value_path,
        NormalizedQueryValuePath::WorldDistance
            | NormalizedQueryValuePath::WorldNormal
            | NormalizedQueryValuePath::WorldTrace
            | NormalizedQueryValuePath::WorldOcclusion
    ) {
        writeln!(
            out,
            "fn wr_candidate_span_start(item_index: u32) -> u32 {{ return continuation_seeds.values[item_index * 2u]; }}"
        )
        .ok();
        writeln!(
            out,
            "fn wr_candidate_span_len(item_index: u32) -> u32 {{ return continuation_seeds.values[item_index * 2u + 1u]; }}"
        )
        .ok();
        writeln!(
            out,
            "fn wr_candidate_shape_word_offset() -> u32 {{ return dispatch_config.item_count * 2u; }}"
        )
        .ok();
        writeln!(
            out,
            "fn wr_candidate_shape_word_count() -> u32 {{ let total_words = arrayLength(&continuation_seeds.values); let offset = wr_candidate_shape_word_offset(); if (total_words > offset) {{ return total_words - offset; }} return 0u; }}"
        )
        .ok();
        writeln!(
            out,
            "fn wr_candidate_shape(candidate_index: u32) -> u32 {{ return continuation_seeds.values[wr_candidate_shape_word_offset() + candidate_index]; }}"
        )
        .ok();
        writeln!(
            out,
            "fn world_distance_point_candidate_span(point: vec3<f32>, candidate_start: u32, candidate_len: u32) -> f32 {{"
        )
        .ok();
        writeln!(
            out,
            "  if (candidate_start == 0xffffffffu) {{ return world_distance_point(point); }}"
        )
        .ok();
        writeln!(
            out,
            "  let candidate_shape_count = wr_candidate_shape_word_count();"
        )
        .ok();
        writeln!(
            out,
            "  if (candidate_len == 0u || candidate_start >= candidate_shape_count) {{ return world_distance_point(point); }}"
        )
        .ok();
        writeln!(out, "  var best_distance: f32 = 1000000.0;").ok();
        writeln!(
            out,
            "  let candidate_end = min(candidate_start + candidate_len, candidate_shape_count);"
        )
        .ok();
        writeln!(
            out,
            "  for (var candidate_index: u32 = candidate_start; candidate_index < candidate_end; candidate_index = candidate_index + 1u) {{"
        )
        .ok();
        writeln!(
            out,
            "    best_distance = min(best_distance, shape_distance_dispatch(wr_candidate_shape(candidate_index), point));"
        )
        .ok();
        writeln!(out, "  }}").ok();
        writeln!(out, "  return best_distance;").ok();
        writeln!(out, "}}\n").ok();
        writeln!(
            out,
            "fn world_normal_point_candidate_span(point: vec3<f32>, candidate_start: u32, candidate_len: u32) -> vec3<f32> {{"
        )
        .ok();
        writeln!(
            out,
            "  if (candidate_start == 0xffffffffu) {{ return world_normal_point(point); }}"
        )
        .ok();
        writeln!(
            out,
            "  let candidate_shape_count = wr_candidate_shape_word_count();"
        )
        .ok();
        writeln!(
            out,
            "  if (candidate_len == 0u || candidate_start >= candidate_shape_count) {{ return world_normal_point(point); }}"
        )
        .ok();
        writeln!(
            out,
            "  let candidate_end = min(candidate_start + candidate_len, candidate_shape_count);"
        )
        .ok();
        writeln!(
            out,
            "  var best_shape = wr_candidate_shape(candidate_start);"
        )
        .ok();
        writeln!(
            out,
            "  var best_distance = shape_distance_dispatch(best_shape, point);"
        )
        .ok();
        writeln!(
            out,
            "  for (var candidate_index: u32 = candidate_start + 1u; candidate_index < candidate_end; candidate_index = candidate_index + 1u) {{"
        )
        .ok();
        writeln!(
            out,
            "    let candidate_shape = wr_candidate_shape(candidate_index);"
        )
        .ok();
        writeln!(
            out,
            "    let candidate_distance = shape_distance_dispatch(candidate_shape, point);"
        )
        .ok();
        writeln!(
            out,
            "    if (candidate_distance < best_distance) {{ best_distance = candidate_distance; best_shape = candidate_shape; }}"
        )
        .ok();
        writeln!(out, "  }}").ok();
        writeln!(
            out,
            "  let sample = shape_normal_dispatch_sample(best_shape, point);"
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
            "  let dx = shape_distance_dispatch(best_shape, point + vec3<f32>(eps, 0.0, 0.0)) - shape_distance_dispatch(best_shape, point - vec3<f32>(eps, 0.0, 0.0));"
        )
        .ok();
        writeln!(
            out,
            "  let dy = shape_distance_dispatch(best_shape, point + vec3<f32>(0.0, eps, 0.0)) - shape_distance_dispatch(best_shape, point - vec3<f32>(0.0, eps, 0.0));"
        )
        .ok();
        writeln!(
            out,
            "  let dz = shape_distance_dispatch(best_shape, point + vec3<f32>(0.0, 0.0, eps)) - shape_distance_dispatch(best_shape, point - vec3<f32>(0.0, 0.0, eps));"
        )
        .ok();
        writeln!(out, "  return wr_normalize3(vec3<f32>(dx, dy, dz));").ok();
        writeln!(out, "}}\n").ok();
        writeln!(
            out,
            "fn wr_batch_world_distance(item_index: u32, point: vec3<f32>) -> f32 {{"
        )
        .ok();
        writeln!(
            out,
            "  if (dispatch_config.candidate_spans_enabled != 0u) {{ return world_distance_point_candidate_span(point, wr_candidate_span_start(item_index), wr_candidate_span_len(item_index)); }}"
        )
        .ok();
        writeln!(out, "  return world_distance_point(point);").ok();
        writeln!(out, "}}\n").ok();
        writeln!(
            out,
            "fn wr_batch_world_normal(item_index: u32, point: vec3<f32>) -> vec3<f32> {{"
        )
        .ok();
        writeln!(
            out,
            "  if (dispatch_config.candidate_spans_enabled != 0u) {{ return world_normal_point_candidate_span(point, wr_candidate_span_start(item_index), wr_candidate_span_len(item_index)); }}"
        )
        .ok();
        writeln!(out, "  return world_normal_point(point);").ok();
        writeln!(out, "}}\n").ok();
    }

    if matches!(
        behavior.value_path,
        NormalizedQueryValuePath::WorldTrace | NormalizedQueryValuePath::WorldOcclusion
    ) {
        writeln!(out, "fn world_trace_ray(ray: RayQuery) -> Hit3 {{").ok();
        writeln!(out, "  var cache_start: f32 = 0.0;").ok();
        writeln!(
            out,
            "  let cache_interval = wr_cache_first_interval(ray, 0.0);"
        )
        .ok();
        writeln!(
            out,
            "  if (cache_interval.accepted != 0u) {{ cache_start = max(cache_interval.start_t, 0.0); }}"
        )
        .ok();
        writeln!(
            out,
            "  if (dispatch_config.accel_node_count == 0u) {{ atomicAdd(&observability_metrics.cache_budget_rejections, 1u); return world_trace_ray_dense(ray, cache_start); }}"
        )
        .ok();
        writeln!(out, "  var best = wr_default_hit(ray.origin);").ok();
        writeln!(out, "  var best_distance: f32 = 1e30;").ok();
        writeln!(out, "  var stack_len: u32 = 0u;").ok();
        writeln!(out, "  var stack_nodes: array<u32, 128>;").ok();
        writeln!(out, "  var stack_starts: array<f32, 128>;").ok();
        writeln!(
            out,
            "  if (!wr_push_accel_node(ray, dispatch_config.accel_root_index, cache_start, &stack_len, &stack_nodes, &stack_starts)) {{ return world_trace_ray_dense(ray, cache_start); }}"
        )
        .ok();
        writeln!(out, "  loop {{").ok();
        writeln!(out, "    if (stack_len == 0u) {{ break; }}").ok();
        writeln!(
            out,
            "    let traversal = wr_pop_best_accel_node(&stack_len, &stack_nodes, &stack_starts);"
        )
        .ok();
        writeln!(
            out,
            "    atomicAdd(&observability_metrics.acceleration_node_visits, 1u);"
        )
        .ok();
        writeln!(
            out,
            "    if (traversal.start_t > min(best_distance, ray.max_distance)) {{"
        )
        .ok();
        writeln!(
            out,
            "      atomicAdd(&observability_metrics.acceleration_pruned_nodes, 1u);"
        )
        .ok();
        writeln!(out, "      continue;").ok();
        writeln!(out, "    }}").ok();
        writeln!(
            out,
            "    let node = accel_nodes.values[traversal.node_index];"
        )
        .ok();
        writeln!(
            out,
            "    if ((node.flags & WR_ACCEL_NODE_FLAG_LEAF) != 0u) {{"
        )
        .ok();
        writeln!(
            out,
            "      atomicAdd(&observability_metrics.shape_leaf_visits, 1u);"
        )
        .ok();
        writeln!(
            out,
            "      let hit = trace_world_shape_candidate(node.leaf_shape_index, ray, traversal.start_t);"
        )
        .ok();
        writeln!(
            out,
            "      if (hit.hit && hit.distance < best_distance) {{ best_distance = hit.distance; best = hit; }}"
        )
        .ok();
        writeln!(out, "      continue;").ok();
        writeln!(out, "    }}").ok();
        writeln!(
            out,
            "    for (var child_offset: u32 = 0u; child_offset < node.child_len; child_offset = child_offset + 1u) {{"
        )
        .ok();
        writeln!(
            out,
            "      let child_index = accel_children.values[node.child_start + child_offset];"
        )
        .ok();
        writeln!(
            out,
            "      if (!wr_push_accel_node(ray, child_index, traversal.start_t, &stack_len, &stack_nodes, &stack_starts)) {{ return world_trace_ray_dense(ray, traversal.start_t); }}"
        )
        .ok();
        writeln!(out, "    }}").ok();
        writeln!(out, "  }}").ok();
        writeln!(out, "  return best;").ok();
        writeln!(out, "}}\n").ok();

        writeln!(
            out,
            "fn world_trace_ray_candidate_span(ray: RayQuery, candidate_start: u32, candidate_len: u32) -> Hit3 {{"
        )
        .ok();
        writeln!(
            out,
            "  if (candidate_start == 0xffffffffu) {{ return world_trace_ray(ray); }}"
        )
        .ok();
        writeln!(
            out,
            "  let candidate_shape_count = wr_candidate_shape_word_count();"
        )
        .ok();
        writeln!(
            out,
            "  if (candidate_len == 0u || candidate_start >= candidate_shape_count) {{ return world_trace_ray(ray); }}"
        )
        .ok();
        writeln!(out, "  var cache_start: f32 = 0.0;").ok();
        writeln!(
            out,
            "  let cache_interval = wr_cache_first_interval(ray, 0.0);"
        )
        .ok();
        writeln!(
            out,
            "  if (cache_interval.accepted != 0u) {{ cache_start = max(cache_interval.start_t, 0.0); }}"
        )
        .ok();
        writeln!(out, "  var best = wr_default_hit(ray.origin);").ok();
        writeln!(out, "  var best_distance: f32 = 1e30;").ok();
        writeln!(
            out,
            "  let candidate_end = min(candidate_start + candidate_len, candidate_shape_count);"
        )
        .ok();
        writeln!(
            out,
            "  for (var candidate_index: u32 = candidate_start; candidate_index < candidate_end; candidate_index = candidate_index + 1u) {{"
        )
        .ok();
        writeln!(
            out,
            "    let hit = trace_world_shape_candidate(wr_candidate_shape(candidate_index), ray, cache_start);"
        )
        .ok();
        writeln!(
            out,
            "    if (hit.hit && hit.distance < best_distance) {{ best_distance = hit.distance; best = hit; }}"
        )
        .ok();
        writeln!(out, "  }}").ok();
        writeln!(out, "  if (!best.hit) {{ return world_trace_ray(ray); }}").ok();
        writeln!(out, "  return best;").ok();
        writeln!(out, "}}\n").ok();
        writeln!(
            out,
            "fn wr_batch_world_trace(item_index: u32, ray: RayQuery) -> Hit3 {{"
        )
        .ok();
        writeln!(
            out,
            "  if (dispatch_config.candidate_spans_enabled != 0u) {{ return world_trace_ray_candidate_span(ray, wr_candidate_span_start(item_index), wr_candidate_span_len(item_index)); }}"
        )
        .ok();
        writeln!(out, "  return world_trace_ray(ray);").ok();
        writeln!(out, "}}\n").ok();
    }

    if matches!(behavior.value_path, NormalizedQueryValuePath::WorldSurface) {
        writeln!(out, "fn world_surface_hit(hit: Hit3) -> Surface {{").ok();
        out.push_str(
            "  if (dispatch_config.material_enabled == 0u) { return wr_default_surface(); }\n",
        );
        writeln!(
            out,
            "  let shape_index = shape_index_from_root_shape_id(hit.root_shape_id);"
        )
        .ok();
        out.push_str("  if (shape_index == 0xffffffffu) { return wr_default_surface(); }\n");
        writeln!(out, "  return surface_at_shape_dispatch(shape_index, hit);").ok();
        writeln!(out, "}}\n").ok();
    }

    if matches!(behavior.value_path, NormalizedQueryValuePath::WorldRadiance) {
        writeln!(
            out,
            "fn world_radiance_query(query: PointDirectionQuery) -> vec3<f32> {{"
        )
        .ok();
        writeln!(out, "  return world_radiance_query_accel(query);").ok();
        writeln!(out, "}}\n").ok();
    }

    if matches!(behavior.value_path, NormalizedQueryValuePath::WorldMedium) {
        writeln!(out, "fn world_medium_point(point: PointQuery) -> Medium {{").ok();
        writeln!(out, "  return world_medium_point_accel(point);").ok();
        writeln!(out, "}}\n").ok();
    }

    writeln!(
        out,
        "fn capture_distance_point(point: PointQuery) -> f32 {{"
    )
    .ok();
    out.push_str("  if (dispatch_config.capture_kind == 0u) { return field_distance_dispatch(dispatch_config.capture_index, point.point); }\n");
    writeln!(
        out,
        "  return shape_distance_dispatch(dispatch_config.capture_index, point.point);"
    )
    .ok();
    writeln!(out, "}}\n").ok();

    writeln!(
        out,
        "fn capture_normal_point(point: PointQuery) -> vec3<f32> {{"
    )
    .ok();
    out.push_str("  if (dispatch_config.capture_kind == 0u) { return field_normal_dispatch(dispatch_config.capture_index, point.point); }\n");
    writeln!(
        out,
        "  return shape_normal_dispatch(dispatch_config.capture_index, point.point);"
    )
    .ok();
    writeln!(out, "}}\n").ok();

    if matches!(
        behavior.value_path,
        NormalizedQueryValuePath::CaptureTrace | NormalizedQueryValuePath::CaptureOcclusion
    ) {
        out.push_str("fn capture_trace_ray(ray: RayQuery) -> Hit3 { return trace_shape_for_index(dispatch_config.capture_index, ray.origin, ray.direction, 0.0, ray.max_distance, ray.min_step, ray.hit_epsilon, ray.max_steps); }\n\n");
    }

    if matches!(
        behavior.value_path,
        NormalizedQueryValuePath::CaptureSurface
    ) {
        out.push_str("fn capture_surface_hit(hit: Hit3) -> Surface { return surface_at_shape_dispatch(dispatch_config.capture_index, hit); }\n\n");
    }

    if matches!(
        behavior.value_path,
        NormalizedQueryValuePath::CaptureRadiance
    ) {
        out.push_str("fn capture_radiance_query(query: PointDirectionQuery) -> vec3<f32> { return radiance_at_shape_dispatch(dispatch_config.capture_index, query.point, query.direction); }\n\n");
    }

    if matches!(behavior.value_path, NormalizedQueryValuePath::CaptureMedium) {
        out.push_str("fn capture_medium_point(point: PointQuery) -> Medium { return medium_at_shape_dispatch(dispatch_config.capture_index, point.point); }\n\n");
    }

    Ok(out)
}

pub(super) fn emit_world_trace_helpers(
    ctx: &QueryExecContext,
    scene_index: &ShaderSceneIndex,
    ops: &DirectQueryOps<'_>,
    behavior: &NormalizedShaderBehavior,
    out: &mut String,
) -> Result<(), QueryExecError> {
    let _ = behavior;
    writeln!(out, "struct WgslTraversalEntry {{").ok();
    writeln!(out, "  node_index: u32,").ok();
    writeln!(out, "  start_t: f32,").ok();
    writeln!(out, "}}\n").ok();
    writeln!(out, "struct WgslRayInterval {{").ok();
    writeln!(out, "  start_t: f32,").ok();
    writeln!(out, "  end_t: f32,").ok();
    writeln!(out, "  accepted: u32,").ok();
    writeln!(out, "}}\n").ok();
    writeln!(
        out,
        "fn wr_ray_aabb_interval(origin: vec3<f32>, direction: vec3<f32>, min_bounds: vec3<f32>, max_bounds: vec3<f32>) -> WgslRayInterval {{"
    )
    .ok();
    writeln!(out, "  var t_min = -1e30;").ok();
    writeln!(out, "  var t_max = 1e30;").ok();
    writeln!(
        out,
        "  for (var axis: u32 = 0u; axis < 3u; axis = axis + 1u) {{"
    )
    .ok();
    writeln!(out, "    let dir = direction[axis];").ok();
    writeln!(out, "    if (abs(dir) <= 1.0e-6) {{").ok();
    writeln!(
        out,
        "      if (origin[axis] < min_bounds[axis] || origin[axis] > max_bounds[axis]) {{ return WgslRayInterval(0.0, 0.0, 0u); }}"
    )
    .ok();
    writeln!(out, "      continue;").ok();
    writeln!(out, "    }}").ok();
    writeln!(out, "    let inv = 1.0 / dir;").ok();
    writeln!(out, "    let t0 = (min_bounds[axis] - origin[axis]) * inv;").ok();
    writeln!(out, "    let t1 = (max_bounds[axis] - origin[axis]) * inv;").ok();
    writeln!(out, "    t_min = max(t_min, min(t0, t1));").ok();
    writeln!(out, "    t_max = min(t_max, max(t0, t1));").ok();
    writeln!(out, "  }}").ok();
    writeln!(
        out,
        "  if (t_max < max(t_min, 0.0)) {{ return WgslRayInterval(0.0, 0.0, 0u); }}"
    )
    .ok();
    writeln!(out, "  return WgslRayInterval(t_min, t_max, 1u);").ok();
    writeln!(out, "}}\n").ok();
    writeln!(
        out,
        "fn wr_push_accel_node(ray: RayQuery, node_index: u32, start_override: f32, stack_len: ptr<function, u32>, stack_nodes: ptr<function, array<u32, 128>>, stack_starts: ptr<function, array<f32, 128>>) -> bool {{"
    )
    .ok();
    writeln!(
        out,
        "  if (node_index >= dispatch_config.accel_node_count || *stack_len >= 128u) {{ atomicAdd(&observability_metrics.cache_budget_rejections, 1u); return false; }}"
    )
    .ok();
    writeln!(out, "  let node = accel_nodes.values[node_index];").ok();
    writeln!(out, "  var start_t = max(start_override, 0.0);").ok();
    writeln!(
        out,
        "  if (WR_SOLVER_ENABLE_SUPPORT != 0u && (node.flags & WR_ACCEL_NODE_FLAG_HAS_BOUNDS) != 0u) {{"
    )
    .ok();
    writeln!(
        out,
        "    let interval = wr_ray_aabb_interval(ray.origin, ray.direction, node.min, node.max);"
    )
    .ok();
    writeln!(
        out,
        "    if (interval.accepted == 0u || interval.end_t < 0.0 || interval.start_t > ray.max_distance) {{"
    )
    .ok();
    writeln!(
        out,
        "      atomicAdd(&observability_metrics.ray_support_interval_rejections, 1u);"
    )
    .ok();
    writeln!(
        out,
        "      atomicAdd(&observability_metrics.solver_support_rejections, 1u);"
    )
    .ok();
    writeln!(out, "      return false;").ok();
    writeln!(out, "    }}").ok();
    writeln!(out, "    if (interval.start_t > start_t) {{").ok();
    writeln!(
        out,
        "      atomicAdd(&observability_metrics.ray_support_entry_jumps, 1u);"
    )
    .ok();
    writeln!(out, "    }}").ok();
    writeln!(
        out,
        "    start_t = max(start_t, max(interval.start_t, 0.0));"
    )
    .ok();
    writeln!(out, "  }}").ok();
    writeln!(out, "  (*stack_nodes)[*stack_len] = node_index;").ok();
    writeln!(out, "  (*stack_starts)[*stack_len] = start_t;").ok();
    writeln!(out, "  *stack_len = *stack_len + 1u;").ok();
    writeln!(out, "  return true;").ok();
    writeln!(out, "}}\n").ok();
    writeln!(
        out,
        "fn wr_pop_best_accel_node(stack_len: ptr<function, u32>, stack_nodes: ptr<function, array<u32, 128>>, stack_starts: ptr<function, array<f32, 128>>) -> WgslTraversalEntry {{"
    )
    .ok();
    writeln!(out, "  var best_index: u32 = 0u;").ok();
    writeln!(out, "  var best_start = (*stack_starts)[0u];").ok();
    writeln!(
        out,
        "  for (var index: u32 = 1u; index < *stack_len; index = index + 1u) {{"
    )
    .ok();
    writeln!(out, "    if ((*stack_starts)[index] < best_start) {{").ok();
    writeln!(out, "      best_start = (*stack_starts)[index];").ok();
    writeln!(out, "      best_index = index;").ok();
    writeln!(out, "    }}").ok();
    writeln!(out, "  }}").ok();
    writeln!(out, "  let last = *stack_len - 1u;").ok();
    writeln!(
        out,
        "  let entry = WgslTraversalEntry((*stack_nodes)[best_index], (*stack_starts)[best_index]);"
    )
    .ok();
    writeln!(out, "  (*stack_nodes)[best_index] = (*stack_nodes)[last];").ok();
    writeln!(
        out,
        "  (*stack_starts)[best_index] = (*stack_starts)[last];"
    )
    .ok();
    writeln!(out, "  *stack_len = last;").ok();
    writeln!(out, "  return entry;").ok();
    writeln!(out, "}}\n").ok();
    writeln!(
        out,
        "fn world_trace_ray_dense(ray: RayQuery, start_travel: f32) -> Hit3 {{"
    )
    .ok();
    writeln!(
        out,
        "  atomicAdd(&observability_metrics.cache_dense_fallback_rays, 1u);"
    )
    .ok();
    writeln!(
        out,
        "  atomicAdd(&observability_metrics.solver_generated_dense_fallback_rays, 1u);"
    )
    .ok();
    writeln!(out, "  var best = wr_default_hit(ray.origin);").ok();
    writeln!(out, "  var best_distance: f32 = 1e30;").ok();
    writeln!(
        out,
        "  for (var index: u32 = 0u; index < dispatch_config.shape_count; index = index + 1u) {{"
    )
    .ok();
    writeln!(
        out,
        "    let hit = trace_shape_for_index(world_shapes.values[index], ray.origin, ray.direction, start_travel, ray.max_distance, ray.min_step, ray.hit_epsilon, ray.max_steps);"
    )
    .ok();
    writeln!(
        out,
        "    if (hit.hit && hit.distance < best_distance) {{ best_distance = hit.distance; best = hit; }}"
    )
    .ok();
    writeln!(out, "  }}").ok();
    writeln!(out, "  return best;").ok();
    writeln!(out, "}}\n").ok();
    emit_analytic_trace_helper(ctx, scene_index, ops, out)?;
    writeln!(
        out,
        "fn trace_world_shape_candidate(shape_index: u32, ray: RayQuery, start_travel: f32) -> Hit3 {{"
    )
    .ok();
    writeln!(
        out,
        "  if (WR_SOLVER_ENABLE_ANALYTIC != 0u && shape_index < arrayLength(&shape_meta.values) && shape_meta.values[shape_index].analytic_kind != WR_SHAPE_ANALYTIC_NONE) {{"
    )
    .ok();
    writeln!(
        out,
        "    let hit = trace_shape_analytic_for_index(shape_index, ray, start_travel);"
    )
    .ok();
    writeln!(
        out,
        "    if (hit.hit) {{ atomicAdd(&observability_metrics.solver_analytic_hits, 1u); }}"
    )
    .ok();
    writeln!(out, "    return hit;").ok();
    writeln!(out, "  }}").ok();
    writeln!(
        out,
        "  atomicAdd(&observability_metrics.solver_generated_dense_fallback_rays, 1u);"
    )
    .ok();
    writeln!(
        out,
        "  return trace_shape_for_index(shape_index, ray.origin, ray.direction, start_travel, ray.max_distance, ray.min_step, ray.hit_epsilon, ray.max_steps);"
    )
    .ok();
    writeln!(out, "}}\n").ok();
    Ok(())
}

pub(super) fn emit_analytic_trace_helper(
    ctx: &QueryExecContext,
    scene_index: &ShaderSceneIndex,
    ops: &DirectQueryOps<'_>,
    out: &mut String,
) -> Result<(), QueryExecError> {
    emit_analytic_intersection_helpers(out)?;
    writeln!(
        out,
        "fn trace_shape_analytic_for_index(shape_index: u32, ray: RayQuery, start_travel: f32) -> Hit3 {{"
    )
    .ok();
    writeln!(out, "  switch shape_index {{").ok();
    for shape_name in ctx.scene.shapes.keys() {
        let Some(case_body) = analytic_shape_case(ctx, ops, shape_name)? else {
            continue;
        };
        let shape_index = scene_index.shape(shape_name)?;
        writeln!(out, "    case {shape_index}u: {{").ok();
        out.push_str(&case_body);
        writeln!(out, "    }}").ok();
    }
    writeln!(out, "    default: {{ return wr_default_hit(ray.origin); }}").ok();
    writeln!(out, "  }}").ok();
    writeln!(out, "}}\n").ok();
    Ok(())
}

pub(super) fn emit_world_acceleration_helpers(
    behavior: &NormalizedShaderBehavior,
    out: &mut String,
) -> Result<(), QueryExecError> {
    let needs_distance_helpers = true;
    let needs_radiance_helpers =
        matches!(behavior.value_path, NormalizedQueryValuePath::WorldRadiance);
    let needs_medium_helpers = matches!(behavior.value_path, NormalizedQueryValuePath::WorldMedium);
    let point_support_pruning_enabled = behavior.world_support_lower_bound_pruning
        && (needs_radiance_helpers || needs_medium_helpers);
    if !needs_distance_helpers && !needs_radiance_helpers && !needs_medium_helpers {
        return Ok(());
    }
    out.push_str(
        "// world helper selection: accelerated tree when acceleration data exists; dense fallback otherwise\n",
    );
    out.push_str(
        "struct WgslWorldTraversalEntry {\n  node_index: u32,\n  lower_bound: f32,\n}\n\n",
    );
    out.push_str(
        "fn wr_world_point_aabb_lower_bound(point: vec3<f32>, min_bounds: vec3<f32>, max_bounds: vec3<f32>) -> f32 {\n  let clamped = clamp(point, min_bounds, max_bounds);\n  return length(point - clamped);\n}\n\n",
    );
    out.push_str(
        "fn wr_world_node_lower_bound(point: vec3<f32>, node_min: vec3<f32>, node_max: vec3<f32>, node_flags: u32) -> f32 {\n  if ((node_flags & WR_ACCEL_NODE_FLAG_HAS_BOUNDS) == 0u) { return 0.0; }\n  return wr_world_point_aabb_lower_bound(point, node_min, node_max);\n}\n\n",
    );
    out.push_str(
        "fn wr_push_world_node(node_index: u32, lower_bound: f32, stack_len: ptr<function, u32>, stack_nodes: ptr<function, array<u32, 128>>, stack_bounds: ptr<function, array<f32, 128>>) -> bool {\n  if (node_index >= dispatch_config.accel_node_count || *stack_len >= 128u) { atomicAdd(&observability_metrics.cache_budget_rejections, 1u); return false; }\n  (*stack_nodes)[*stack_len] = node_index;\n  (*stack_bounds)[*stack_len] = lower_bound;\n  *stack_len = *stack_len + 1u;\n  return true;\n}\n\n",
    );
    out.push_str(
        "fn wr_pop_best_world_node(stack_len: ptr<function, u32>, stack_nodes: ptr<function, array<u32, 128>>, stack_bounds: ptr<function, array<f32, 128>>) -> WgslWorldTraversalEntry {\n  var best_index: u32 = 0u;\n  var best_bound = (*stack_bounds)[0u];\n  for (var index: u32 = 1u; index < *stack_len; index = index + 1u) {\n    if ((*stack_bounds)[index] < best_bound) {\n      best_bound = (*stack_bounds)[index];\n      best_index = index;\n    }\n  }\n  let last = *stack_len - 1u;\n  let entry = WgslWorldTraversalEntry((*stack_nodes)[best_index], (*stack_bounds)[best_index]);\n  (*stack_nodes)[best_index] = (*stack_nodes)[last];\n  (*stack_bounds)[best_index] = (*stack_bounds)[last];\n  *stack_len = last;\n  return entry;\n}\n\n",
    );
    let indent_block = |source: &str, indent: &str| -> String {
        let mut block = String::new();
        for line in source.lines() {
            block.push_str(indent);
            block.push_str(line);
            block.push('\n');
        }
        block
    };
    if needs_distance_helpers {
        let dense_distance_scan = indent_block(
            "var dense_distance: f32 = 1000000.0;\nfor (var index: u32 = 0u; index < dispatch_config.shape_count; index = index + 1u) {\n  dense_distance = min(dense_distance, shape_distance_dispatch(world_shapes.values[index], point));\n}\nreturn dense_distance;",
            "    ",
        );
        let dense_distance_scan_in_child = indent_block(
            "var dense_distance: f32 = 1000000.0;\nfor (var index: u32 = 0u; index < dispatch_config.shape_count; index = index + 1u) {\n  dense_distance = min(dense_distance, shape_distance_dispatch(world_shapes.values[index], point));\n}\nreturn dense_distance;",
            "        ",
        );
        out.push_str(&format!(
            "fn world_distance_point_accel(point: vec3<f32>) -> f32 {{\n  if (dispatch_config.accel_node_count == 0u) {{\n{dense_distance_scan}  }}\n  var stack_len: u32 = 0u;\n  var stack_nodes: array<u32, 128>;\n  var stack_bounds: array<f32, 128>;\n  let root = accel_nodes.values[dispatch_config.accel_root_index];\n  if (!wr_push_world_node(dispatch_config.accel_root_index, wr_world_node_lower_bound(point, root.min, root.max, root.flags), &stack_len, &stack_nodes, &stack_bounds)) {{\n{dense_distance_scan}  }}\n  var current: f32 = 1000000.0;\n  loop {{\n    if (stack_len == 0u) {{ break; }}\n    let traversal = wr_pop_best_world_node(&stack_len, &stack_nodes, &stack_bounds);\n    atomicAdd(&observability_metrics.acceleration_node_visits, 1u);\n    if (traversal.lower_bound > current) {{\n      atomicAdd(&observability_metrics.acceleration_pruned_nodes, 1u);\n      continue;\n    }}\n    let node = accel_nodes.values[traversal.node_index];\n    if ((node.flags & WR_ACCEL_NODE_FLAG_LEAF) != 0u) {{\n      atomicAdd(&observability_metrics.shape_leaf_visits, 1u);\n      current = min(current, shape_distance_dispatch(node.leaf_shape_index, point));\n      continue;\n    }}\n    for (var child_offset: u32 = 0u; child_offset < node.child_len; child_offset = child_offset + 1u) {{\n      let child_index = accel_children.values[node.child_start + child_offset];\n      let child = accel_nodes.values[child_index];\n      if (!wr_push_world_node(child_index, wr_world_node_lower_bound(point, child.min, child.max, child.flags), &stack_len, &stack_nodes, &stack_bounds)) {{\n{dense_distance_scan_in_child}      }}\n    }}\n  }}\n  return current;\n}}\n\n"
        ));
    }
    if needs_radiance_helpers {
        let dense_radiance_scan = indent_block(
            "var dense_total = vec3<f32>(0.0, 0.0, 0.0);\nfor (var index: u32 = 0u; index < dispatch_config.shape_count; index = index + 1u) {\n  dense_total = dense_total + radiance_at_shape_dispatch(world_shapes.values[index], query.point, query.direction);\n}\nreturn dense_total;",
            "    ",
        );
        let dense_radiance_scan_in_child = indent_block(
            "var dense_total = vec3<f32>(0.0, 0.0, 0.0);\nfor (var index: u32 = 0u; index < dispatch_config.shape_count; index = index + 1u) {\n  dense_total = dense_total + radiance_at_shape_dispatch(world_shapes.values[index], query.point, query.direction);\n}\nreturn dense_total;",
            "        ",
        );
        let root_lower_bound = if point_support_pruning_enabled {
            "  let root_lower_bound = wr_world_node_lower_bound(query.point, root.min, root.max, root.flags);\n  if (root_lower_bound > 0.0) {\n    atomicAdd(&observability_metrics.acceleration_pruned_nodes, 1u);\n    return vec3<f32>(0.0, 0.0, 0.0);\n  }\n"
        } else {
            ""
        };
        let traversal_prune = if point_support_pruning_enabled {
            "    if (traversal.lower_bound > 0.0) {\n      atomicAdd(&observability_metrics.acceleration_pruned_nodes, 1u);\n      continue;\n    }\n"
        } else {
            ""
        };
        let child_push = if point_support_pruning_enabled {
            "      let child_lower_bound = wr_world_node_lower_bound(query.point, child.min, child.max, child.flags);\n      if (child_lower_bound > 0.0) {\n        atomicAdd(&observability_metrics.acceleration_pruned_nodes, 1u);\n        continue;\n      }\n      if (!wr_push_world_node(child_index, child_lower_bound, &stack_len, &stack_nodes, &stack_bounds)) {\n"
        } else {
            "      if (!wr_push_world_node(child_index, wr_world_node_lower_bound(query.point, child.min, child.max, child.flags), &stack_len, &stack_nodes, &stack_bounds)) {\n"
        };
        out.push_str(&format!(
            "fn world_radiance_query_accel(query: PointDirectionQuery) -> vec3<f32> {{\n  if (dispatch_config.radiance_enabled == 0u) {{ return vec3<f32>(0.0, 0.0, 0.0); }}\n  if (dispatch_config.accel_node_count == 0u) {{\n{dense_radiance_scan}  }}\n  var stack_len: u32 = 0u;\n  var stack_nodes: array<u32, 128>;\n  var stack_bounds: array<f32, 128>;\n  let root = accel_nodes.values[dispatch_config.accel_root_index];\n{root_lower_bound}  if (!wr_push_world_node(dispatch_config.accel_root_index, wr_world_node_lower_bound(query.point, root.min, root.max, root.flags), &stack_len, &stack_nodes, &stack_bounds)) {{\n{dense_radiance_scan}  }}\n  var total = vec3<f32>(0.0, 0.0, 0.0);\n  loop {{\n    if (stack_len == 0u) {{ break; }}\n    let traversal = wr_pop_best_world_node(&stack_len, &stack_nodes, &stack_bounds);\n    atomicAdd(&observability_metrics.acceleration_node_visits, 1u);\n{traversal_prune}    let node = accel_nodes.values[traversal.node_index];\n    if ((node.flags & WR_ACCEL_NODE_FLAG_LEAF) != 0u) {{\n      atomicAdd(&observability_metrics.shape_leaf_visits, 1u);\n      total = total + radiance_at_shape_dispatch(node.leaf_shape_index, query.point, query.direction);\n      continue;\n    }}\n    for (var child_offset: u32 = 0u; child_offset < node.child_len; child_offset = child_offset + 1u) {{\n      let child_index = accel_children.values[node.child_start + child_offset];\n      let child = accel_nodes.values[child_index];\n{child_push}{dense_radiance_scan_in_child}      }}\n    }}\n  }}\n  return total;\n}}\n\n"
        ));
    }
    if needs_medium_helpers {
        let dense_medium_scan = indent_block(
            "var dense_total = wr_default_medium();\nfor (var index: u32 = 0u; index < dispatch_config.shape_count; index = index + 1u) {\n  dense_total = wr_combine_medium_values(dense_total, medium_at_shape_dispatch(world_shapes.values[index], point.point));\n}\nreturn dense_total;",
            "    ",
        );
        let dense_medium_scan_in_child = indent_block(
            "var dense_total = wr_default_medium();\nfor (var index: u32 = 0u; index < dispatch_config.shape_count; index = index + 1u) {\n  dense_total = wr_combine_medium_values(dense_total, medium_at_shape_dispatch(world_shapes.values[index], point.point));\n}\nreturn dense_total;",
            "        ",
        );
        let root_lower_bound = if point_support_pruning_enabled {
            "  let root_lower_bound = wr_world_node_lower_bound(point.point, root.min, root.max, root.flags);\n  if (root_lower_bound > 0.0) {\n    atomicAdd(&observability_metrics.acceleration_pruned_nodes, 1u);\n    return wr_default_medium();\n  }\n"
        } else {
            ""
        };
        let traversal_prune = if point_support_pruning_enabled {
            "    if (traversal.lower_bound > 0.0) {\n      atomicAdd(&observability_metrics.acceleration_pruned_nodes, 1u);\n      continue;\n    }\n"
        } else {
            ""
        };
        let child_push = if point_support_pruning_enabled {
            "      let child_lower_bound = wr_world_node_lower_bound(point.point, child.min, child.max, child.flags);\n      if (child_lower_bound > 0.0) {\n        atomicAdd(&observability_metrics.acceleration_pruned_nodes, 1u);\n        continue;\n      }\n      if (!wr_push_world_node(child_index, child_lower_bound, &stack_len, &stack_nodes, &stack_bounds)) {\n"
        } else {
            "      if (!wr_push_world_node(child_index, wr_world_node_lower_bound(point.point, child.min, child.max, child.flags), &stack_len, &stack_nodes, &stack_bounds)) {\n"
        };
        out.push_str(&format!(
            "fn world_medium_point_accel(point: PointQuery) -> Medium {{\n  if (dispatch_config.media_enabled == 0u) {{ return wr_default_medium(); }}\n  if (dispatch_config.accel_node_count == 0u) {{\n{dense_medium_scan}  }}\n  var stack_len: u32 = 0u;\n  var stack_nodes: array<u32, 128>;\n  var stack_bounds: array<f32, 128>;\n  let root = accel_nodes.values[dispatch_config.accel_root_index];\n{root_lower_bound}  if (!wr_push_world_node(dispatch_config.accel_root_index, wr_world_node_lower_bound(point.point, root.min, root.max, root.flags), &stack_len, &stack_nodes, &stack_bounds)) {{\n{dense_medium_scan}  }}\n  var total = wr_default_medium();\n  loop {{\n    if (stack_len == 0u) {{ break; }}\n    let traversal = wr_pop_best_world_node(&stack_len, &stack_nodes, &stack_bounds);\n    atomicAdd(&observability_metrics.acceleration_node_visits, 1u);\n{traversal_prune}    let node = accel_nodes.values[traversal.node_index];\n    if ((node.flags & WR_ACCEL_NODE_FLAG_LEAF) != 0u) {{\n      atomicAdd(&observability_metrics.shape_leaf_visits, 1u);\n      total = wr_combine_medium_values(total, medium_at_shape_dispatch(node.leaf_shape_index, point.point));\n      continue;\n    }}\n    for (var child_offset: u32 = 0u; child_offset < node.child_len; child_offset = child_offset + 1u) {{\n      let child_index = accel_children.values[node.child_start + child_offset];\n      let child = accel_nodes.values[child_index];\n{child_push}{dense_medium_scan_in_child}      }}\n    }}\n  }}\n  return total;\n}}\n\n"
        ));
    }
    Ok(())
}
