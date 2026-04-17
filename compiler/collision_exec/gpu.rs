use crate::collision_contract::CollisionRayInput;
use crate::collision_plan::{CollisionExecError, CollisionExecutionTrace, CollisionPlan};
use crate::gpu_runtime::{GpuPassProfiler, GpuRuntimeMetrics};
use crate::kernel::{KernelValue, lower_batch_query_plan};
use crate::portable::portable_abi_decode_slice;
use crate::query_exec::QueryExecContext;
use crate::query_exec::QueryExecutionObservability;
use crate::query_exec::gpu_dispatch::GpuQueryDispatcher;
use crate::query_exec::wgsl::readback_storage_buffer_on;
use crate::query_plan::{
    BatchQueryKind, BatchQueryPlan, CaptureKind, DispatchBackend, batch_query_contract_id,
};
use smol_str::SmolStr;

pub fn execute(
    plan: &CollisionPlan,
    ctx: &QueryExecContext,
    args: &[KernelValue],
) -> Result<
    (
        crate::collision_contract::CollisionResult,
        CollisionExecutionTrace,
    ),
    CollisionExecError,
> {
    crate::collision_exec::cpu::execute(plan, ctx, args)
}

pub(crate) fn prepare_batched_point_distance_dispatch(
    ctx: &QueryExecContext,
    snapshot: Option<&crate::world_identity::WorldSnapshotHandle>,
    capture: KernelValue,
    domain: KernelValue,
    points: &[KernelValue],
) -> Result<GpuQueryDispatcher, CollisionExecError> {
    prepare_world_batch_dispatch(
        ctx,
        snapshot,
        BatchQueryKind::Distance,
        capture,
        domain,
        points,
        &[],
    )
}

pub(crate) fn prepare_batched_point_normal_dispatch(
    ctx: &QueryExecContext,
    snapshot: Option<&crate::world_identity::WorldSnapshotHandle>,
    capture: KernelValue,
    domain: KernelValue,
    points: &[KernelValue],
) -> Result<GpuQueryDispatcher, CollisionExecError> {
    prepare_world_batch_dispatch(
        ctx,
        snapshot,
        BatchQueryKind::Normal,
        capture,
        domain,
        points,
        &[],
    )
}

pub(crate) fn prepare_batched_ray_trace_dispatch(
    ctx: &QueryExecContext,
    snapshot: Option<&crate::world_identity::WorldSnapshotHandle>,
    capture: KernelValue,
    domain: KernelValue,
    rays: &[KernelValue],
) -> Result<GpuQueryDispatcher, CollisionExecError> {
    prepare_world_batch_dispatch(
        ctx,
        snapshot,
        BatchQueryKind::Trace,
        capture,
        domain,
        rays,
        &[],
    )
}

fn prepare_world_batch_dispatch(
    ctx: &QueryExecContext,
    snapshot: Option<&crate::world_identity::WorldSnapshotHandle>,
    kind: BatchQueryKind,
    capture: KernelValue,
    domain: KernelValue,
    items: &[KernelValue],
    candidates: &[SmolStr],
) -> Result<GpuQueryDispatcher, CollisionExecError> {
    let contract_id = batch_query_contract_id(kind, CaptureKind::Region).ok_or_else(|| {
        CollisionExecError::ExecutionUnavailable {
            message: format!("missing batched region query contract for {kind:?}"),
        }
    })?;
    let batch_plan = BatchQueryPlan::for_contract(contract_id, DispatchBackend::Wgsl, None)
        .map_err(|message| CollisionExecError::ExecutionUnavailable {
            message: message.to_string(),
        })?;
    let lowered = lower_batch_query_plan(&batch_plan);
    let candidate_spans = pack_candidate_spans(ctx, candidates, items.len())?;
    GpuQueryDispatcher::from_batch_plan_with_candidate_spans_and_snapshot(
        ctx,
        snapshot,
        &lowered,
        &[capture, domain, KernelValue::Array(items.to_vec())],
        candidate_spans,
    )
    .map_err(|err| CollisionExecError::ExecutionUnavailable {
        message: err.to_string(),
    })
}

pub(crate) fn execute_batched_point_distance_query(
    ctx: &QueryExecContext,
    snapshot: Option<&crate::world_identity::WorldSnapshotHandle>,
    capture: KernelValue,
    domain: KernelValue,
    point: [f32; 3],
) -> Result<(KernelValue, QueryExecutionObservability), CollisionExecError> {
    let (value, observability) =
        execute_single_result_dispatch(prepare_batched_point_distance_dispatch(
            ctx,
            snapshot,
            capture,
            domain,
            &[point_query_value(point)],
        )?)?;
    let distance = match value {
        KernelValue::Struct(result) => result
            .fields
            .into_iter()
            .find(|(name, _)| name.as_str() == "distance")
            .map(|(_, value)| value)
            .ok_or_else(|| CollisionExecError::ExecutionUnavailable {
                message: "collision WGSL distance result is missing a distance field".to_string(),
            })?,
        other => {
            return Err(CollisionExecError::ExecutionUnavailable {
                message: format!("collision WGSL distance result has unexpected shape: {other:?}"),
            });
        }
    };
    Ok((distance, observability))
}

pub(crate) fn execute_batched_point_distance_queries(
    ctx: &QueryExecContext,
    snapshot: Option<&crate::world_identity::WorldSnapshotHandle>,
    capture: KernelValue,
    domain: KernelValue,
    points: &[[f32; 3]],
) -> Result<(Vec<KernelValue>, QueryExecutionObservability), CollisionExecError> {
    let items = points
        .iter()
        .copied()
        .map(point_query_value)
        .collect::<Vec<_>>();
    let (values, observability) = execute_dispatch(prepare_batched_point_distance_dispatch(
        ctx, snapshot, capture, domain, &items,
    )?)?;
    Ok((extract_distance_values(values)?, observability))
}

pub(crate) fn execute_batched_point_distance_queries_with_candidates(
    ctx: &QueryExecContext,
    snapshot: Option<&crate::world_identity::WorldSnapshotHandle>,
    capture: KernelValue,
    domain: KernelValue,
    points: &[[f32; 3]],
    candidates: &[SmolStr],
) -> Result<(Vec<KernelValue>, QueryExecutionObservability), CollisionExecError> {
    let items = points
        .iter()
        .copied()
        .map(point_query_value)
        .collect::<Vec<_>>();
    let (values, observability) = execute_dispatch(prepare_world_batch_dispatch(
        ctx,
        snapshot,
        BatchQueryKind::Distance,
        capture,
        domain,
        &items,
        candidates,
    )?)?;
    Ok((extract_distance_values(values)?, observability))
}

pub(crate) fn execute_batched_point_normal_query(
    ctx: &QueryExecContext,
    snapshot: Option<&crate::world_identity::WorldSnapshotHandle>,
    capture: KernelValue,
    domain: KernelValue,
    point: [f32; 3],
) -> Result<(KernelValue, QueryExecutionObservability), CollisionExecError> {
    let (value, observability) =
        execute_single_result_dispatch(prepare_batched_point_normal_dispatch(
            ctx,
            snapshot,
            capture,
            domain,
            &[point_query_value(point)],
        )?)?;
    let normal = match value {
        KernelValue::Struct(result) => result
            .fields
            .into_iter()
            .find(|(name, _)| name.as_str() == "normal")
            .map(|(_, value)| value)
            .ok_or_else(|| CollisionExecError::ExecutionUnavailable {
                message: "collision WGSL normal result is missing a normal field".to_string(),
            })?,
        KernelValue::Vec3(_) => value,
        other => {
            return Err(CollisionExecError::ExecutionUnavailable {
                message: format!("collision WGSL normal result has unexpected shape: {other:?}"),
            });
        }
    };
    Ok((normal, observability))
}

pub(crate) fn execute_batched_point_normal_queries(
    ctx: &QueryExecContext,
    snapshot: Option<&crate::world_identity::WorldSnapshotHandle>,
    capture: KernelValue,
    domain: KernelValue,
    points: &[[f32; 3]],
) -> Result<(Vec<KernelValue>, QueryExecutionObservability), CollisionExecError> {
    let items = points
        .iter()
        .copied()
        .map(point_query_value)
        .collect::<Vec<_>>();
    let (values, observability) = execute_dispatch(prepare_batched_point_normal_dispatch(
        ctx, snapshot, capture, domain, &items,
    )?)?;
    Ok((extract_normal_values(values)?, observability))
}

pub(crate) fn execute_batched_point_normal_queries_with_candidates(
    ctx: &QueryExecContext,
    snapshot: Option<&crate::world_identity::WorldSnapshotHandle>,
    capture: KernelValue,
    domain: KernelValue,
    points: &[[f32; 3]],
    candidates: &[SmolStr],
) -> Result<(Vec<KernelValue>, QueryExecutionObservability), CollisionExecError> {
    let items = points
        .iter()
        .copied()
        .map(point_query_value)
        .collect::<Vec<_>>();
    let (values, observability) = execute_dispatch(prepare_world_batch_dispatch(
        ctx,
        snapshot,
        BatchQueryKind::Normal,
        capture,
        domain,
        &items,
        candidates,
    )?)?;
    Ok((extract_normal_values(values)?, observability))
}

pub(crate) fn execute_batched_ray_trace_query(
    ctx: &QueryExecContext,
    snapshot: Option<&crate::world_identity::WorldSnapshotHandle>,
    capture: KernelValue,
    domain: KernelValue,
    ray: CollisionRayInput,
) -> Result<(KernelValue, QueryExecutionObservability), CollisionExecError> {
    execute_single_result_dispatch(prepare_batched_ray_trace_dispatch(
        ctx,
        snapshot,
        capture,
        domain,
        &[ray_query_value(ray)],
    )?)
}

pub(crate) fn execute_batched_ray_trace_query_with_candidates(
    ctx: &QueryExecContext,
    snapshot: Option<&crate::world_identity::WorldSnapshotHandle>,
    capture: KernelValue,
    domain: KernelValue,
    ray: CollisionRayInput,
    candidates: &[SmolStr],
) -> Result<(KernelValue, QueryExecutionObservability), CollisionExecError> {
    execute_single_result_dispatch(prepare_world_batch_dispatch(
        ctx,
        snapshot,
        BatchQueryKind::Trace,
        capture,
        domain,
        &[ray_query_value(ray)],
        candidates,
    )?)
}

fn pack_candidate_spans(
    ctx: &QueryExecContext,
    candidates: &[SmolStr],
    item_count: usize,
) -> Result<Vec<u32>, CollisionExecError> {
    if candidates.is_empty() {
        return Ok(Vec::new());
    }
    let candidate_indices = candidates
        .iter()
        .map(|candidate| shape_index(ctx, candidate))
        .collect::<Result<Vec<_>, _>>()?;
    let mut spans = Vec::with_capacity(item_count.saturating_mul(2) + candidate_indices.len());
    for _ in 0..item_count {
        spans.push(0);
        spans.push(candidate_indices.len() as u32);
    }
    spans.extend(candidate_indices);
    Ok(spans)
}

fn shape_index(ctx: &QueryExecContext, name: &SmolStr) -> Result<u32, CollisionExecError> {
    ctx.scene
        .shapes
        .keys()
        .enumerate()
        .find_map(|(index, candidate)| (candidate == name).then_some(index as u32))
        .ok_or_else(|| CollisionExecError::ExecutionUnavailable {
            message: format!("collision WGSL candidate '{name}' is not present in the scene index"),
        })
}

fn extract_distance_values(
    values: Vec<KernelValue>,
) -> Result<Vec<KernelValue>, CollisionExecError> {
    values.into_iter().map(extract_distance_value).collect()
}

fn extract_distance_value(value: KernelValue) -> Result<KernelValue, CollisionExecError> {
    match value {
        KernelValue::Struct(result) => result
            .fields
            .into_iter()
            .find(|(name, _)| name.as_str() == "distance")
            .map(|(_, value)| value)
            .ok_or_else(|| CollisionExecError::ExecutionUnavailable {
                message: "collision WGSL distance result is missing a distance field".to_string(),
            }),
        KernelValue::F32(_) => Ok(value),
        other => Err(CollisionExecError::ExecutionUnavailable {
            message: format!("collision WGSL distance result has unexpected shape: {other:?}"),
        }),
    }
}

fn extract_normal_values(values: Vec<KernelValue>) -> Result<Vec<KernelValue>, CollisionExecError> {
    values.into_iter().map(extract_normal_value).collect()
}

fn extract_normal_value(value: KernelValue) -> Result<KernelValue, CollisionExecError> {
    match value {
        KernelValue::Struct(result) => result
            .fields
            .into_iter()
            .find(|(name, _)| name.as_str() == "normal")
            .map(|(_, value)| value)
            .ok_or_else(|| CollisionExecError::ExecutionUnavailable {
                message: "collision WGSL normal result is missing a normal field".to_string(),
            }),
        KernelValue::Vec3(_) => Ok(value),
        other => Err(CollisionExecError::ExecutionUnavailable {
            message: format!("collision WGSL normal result has unexpected shape: {other:?}"),
        }),
    }
}

fn execute_single_result_dispatch(
    dispatcher: GpuQueryDispatcher,
) -> Result<(KernelValue, QueryExecutionObservability), CollisionExecError> {
    let (mut values, observability) = execute_dispatch(dispatcher)?;
    let value =
        values
            .drain(..)
            .next()
            .ok_or_else(|| CollisionExecError::ExecutionUnavailable {
                message: "collision WGSL batch dispatch returned no values".to_string(),
            })?;
    Ok((value, observability))
}

fn execute_dispatch(
    dispatcher: GpuQueryDispatcher,
) -> Result<(Vec<KernelValue>, QueryExecutionObservability), CollisionExecError> {
    let native = dispatcher.native().clone();
    let mut profiler = GpuPassProfiler::new(&native, 1);
    let mut encoder = native
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("wrela.collision.gpu.dispatch.encoder"),
        });
    let initial_gpu_runtime = dispatcher.initial_gpu_runtime();
    let upload_bytes = dispatcher.initialize_dispatch_state().map_err(|err| {
        CollisionExecError::ExecutionUnavailable {
            message: err.to_string(),
        }
    })?;
    dispatcher.encode_compute_pass(&mut encoder, &mut profiler);
    profiler.resolve_into(&mut encoder);
    native.queue.submit(Some(encoder.finish()));

    let result = dispatcher.dispatch_result();
    let result_bytes =
        readback_storage_buffer_on(&native, &result.values.buffer, result.values.size_bytes)
            .map_err(|err| CollisionExecError::ExecutionUnavailable {
                message: err.to_string(),
            })?;
    let values = portable_abi_decode_slice(
        result
            .values
            .abi
            .as_ref()
            .ok_or_else(|| CollisionExecError::ExecutionUnavailable {
                message: "collision WGSL batch result is missing an ABI".to_string(),
            })?,
        &result_bytes,
        result.item_count as usize,
    )
    .map_err(|err| CollisionExecError::ExecutionUnavailable {
        message: err.to_string(),
    })?;
    let observability_handle =
        result
            .metrics
            .ok_or_else(|| CollisionExecError::ExecutionUnavailable {
                message: "collision WGSL batch result is missing observability metrics".to_string(),
            })?;
    let observability_bytes = readback_storage_buffer_on(
        &native,
        &observability_handle.buffer,
        observability_handle.size_bytes,
    )
    .map_err(|err| CollisionExecError::ExecutionUnavailable {
        message: err.to_string(),
    })?;
    let observability = dispatcher.decode_observability(
        &observability_bytes,
        GpuRuntimeMetrics {
            upload_bytes,
            ..initial_gpu_runtime
        },
    );
    Ok((values, observability))
}

fn point_query_value(point: [f32; 3]) -> KernelValue {
    KernelValue::Struct(crate::kernel::KernelStructValue {
        name: SmolStr::new("PointQuery"),
        fields: vec![(SmolStr::new("point"), KernelValue::Vec3(point))],
    })
}

fn ray_query_value(ray: CollisionRayInput) -> KernelValue {
    KernelValue::Struct(crate::kernel::KernelStructValue {
        name: SmolStr::new("RayQuery"),
        fields: vec![
            (SmolStr::new("origin"), KernelValue::Vec3(ray.origin)),
            (SmolStr::new("direction"), KernelValue::Vec3(ray.direction)),
            (
                SmolStr::new("max_distance"),
                KernelValue::F32(ray.max_distance),
            ),
            (SmolStr::new("min_step"), KernelValue::F32(ray.min_step)),
            (
                SmolStr::new("hit_epsilon"),
                KernelValue::F32(ray.hit_epsilon),
            ),
            (SmolStr::new("max_steps"), KernelValue::I32(ray.max_steps)),
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::{
        point_query_value, prepare_batched_point_distance_dispatch,
        prepare_batched_point_normal_dispatch, prepare_batched_ray_trace_dispatch, ray_query_value,
    };
    use crate::collision_contract::CollisionRayInput;
    use crate::gpu_runtime::GpuPassProfiler;
    use crate::hir;
    use crate::hir::lower as hir_lower;
    use crate::kernel::{KernelStructValue, KernelValue};
    use crate::parser::ast;
    use crate::parser::ast::AstNode;
    use crate::parser::parse;
    use crate::portable::portable_abi_decode_slice;
    use crate::query_exec::QueryExecContext;
    use crate::query_exec::stable_region_scene_capture_id;
    use crate::query_exec::wgsl::readback_storage_buffer_on;
    use smol_str::SmolStr;

    fn lower_inline_module_from_source(source: &str) -> hir::Module {
        let node = parse(source);
        let root = ast::Root::cast(node).expect("root");
        hir_lower::lower(root)
    }

    fn typed_query_module(source: &str) -> QueryExecContext {
        let module = lower_inline_module_from_source(source);
        let semantic = hir::semantic::check_module(&module);
        assert!(
            semantic.errors.is_empty(),
            "semantic errors: {:?}",
            semantic.errors
        );
        let (type_errors, type_info) = hir::typeck::check_module_with_info(&module);
        assert!(type_errors.is_empty(), "type errors: {type_errors:?}");
        QueryExecContext::compile(&module, &type_info)
    }

    fn fixture_source() -> &'static str {
        r#"
field exact distance collision_field(p: Vec3) -> F32 {
    sphere(radius = 0.5)
}

material collision_surface(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(0.8, 0.3, 0.2),
        roughness=0.2,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

shape collision_shape {
    field = collision_field
    material = collision_surface
}

region collision_region() {
    place sample = collision_shape
}

domain collision_domain(world: RegionCapture) {
    geometry_detail = 1
    material = true
    radiance = false
    media = false
    max_distance = 6.0
    min_step = 0.05
    hit_epsilon = 0.001
    max_steps = 96
}
"#
    }

    fn scene_domain(scene_id: u32) -> KernelValue {
        KernelValue::Struct(KernelStructValue {
            name: SmolStr::new("SceneDomain"),
            fields: vec![
                (SmolStr::new("scene_id"), KernelValue::U32(scene_id)),
                (
                    SmolStr::new("spatial"),
                    KernelValue::Struct(KernelStructValue {
                        name: SmolStr::new("SpatialDomainContract"),
                        fields: vec![(SmolStr::new("geometry_detail"), KernelValue::I32(1))],
                    }),
                ),
                (
                    SmolStr::new("surface"),
                    KernelValue::Struct(KernelStructValue {
                        name: SmolStr::new("SurfaceDomainContract"),
                        fields: vec![(SmolStr::new("material"), KernelValue::Bool(true))],
                    }),
                ),
                (
                    SmolStr::new("participants"),
                    KernelValue::Struct(KernelStructValue {
                        name: SmolStr::new("ParticipantDomainContract"),
                        fields: vec![
                            (SmolStr::new("radiance"), KernelValue::Bool(false)),
                            (SmolStr::new("media"), KernelValue::Bool(false)),
                        ],
                    }),
                ),
            ],
        })
    }

    fn region_capture(scene_id: u32, epoch: u32) -> KernelValue {
        KernelValue::Struct(KernelStructValue {
            name: SmolStr::new("RegionCapture"),
            fields: vec![
                (SmolStr::new("scene_id"), KernelValue::U32(scene_id)),
                (SmolStr::new("epoch"), KernelValue::U32(epoch)),
            ],
        })
    }

    fn point_query(point: [f32; 3]) -> KernelValue {
        point_query_value(point)
    }

    fn ray_query(origin: [f32; 3], direction: [f32; 3]) -> KernelValue {
        ray_query_value(CollisionRayInput {
            origin,
            direction,
            max_distance: 6.0,
            min_step: 0.05,
            hit_epsilon: 0.001,
            max_steps: 96,
        })
    }

    #[test]
    fn collision_gpu_adapter_executes_batched_world_distance_queries() {
        let ctx = typed_query_module(fixture_source());
        let scene_id = stable_region_scene_capture_id(&SmolStr::new("collision_region"));
        let dispatcher = prepare_batched_point_distance_dispatch(
            &ctx,
            None,
            region_capture(scene_id, 1),
            scene_domain(scene_id),
            &[point_query([0.0, 0.0, 0.0]), point_query([2.0, 0.0, 0.0])],
        )
        .expect("collision GPU point-distance dispatcher");

        let native = dispatcher.native().clone();
        let mut profiler = GpuPassProfiler::new(&native, 1);
        let mut encoder = native
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("wrela.collision.gpu.test.encoder"),
            });
        assert!(
            dispatcher
                .initialize_dispatch_state()
                .expect("dispatch upload")
                > 0
        );
        dispatcher.encode_compute_pass(&mut encoder, &mut profiler);
        profiler.resolve_into(&mut encoder);
        native.queue.submit(Some(encoder.finish()));

        let result = dispatcher.dispatch_result();
        let result_bytes =
            readback_storage_buffer_on(&native, &result.values.buffer, result.values.size_bytes)
                .expect("result readback");
        let values = portable_abi_decode_slice(
            result.values.abi.as_ref().expect("distance result ABI"),
            &result_bytes,
            result.item_count as usize,
        )
        .expect("decode distance results");
        let distances = values
            .into_iter()
            .map(|value| match value {
                KernelValue::Struct(result) => match result
                    .fields
                    .iter()
                    .find(|(name, _)| name.as_str() == "distance")
                {
                    Some((_, KernelValue::F32(distance))) => *distance,
                    other => panic!("expected DistanceResult.distance, found {other:?}"),
                },
                other => panic!("expected DistanceResult values, found {other:?}"),
            })
            .collect::<Vec<_>>();
        assert_eq!(distances.len(), 2);
        assert!(
            (distances[0] + 0.5).abs() < 0.05,
            "expected inner hit distance near -0.5, found {}",
            distances[0]
        );
        assert!(
            (distances[1] - 1.5).abs() < 0.05,
            "expected outer miss distance near 1.5, found {}",
            distances[1]
        );

        let metrics = result.metrics.expect("observability handle");
        let metrics_bytes =
            readback_storage_buffer_on(&native, &metrics.buffer, metrics.size_bytes)
                .expect("metrics readback");
        let observability =
            dispatcher.decode_observability(&metrics_bytes, dispatcher.initial_gpu_runtime());
        assert_eq!(observability.dispatch_count, 1);
        assert_eq!(observability.dispatch_items, 2);
        assert_eq!(observability.gpu_runtime.attachment_decode_count, 0);
    }

    #[test]
    fn collision_gpu_adapter_executes_batched_world_normal_queries() {
        let ctx = typed_query_module(fixture_source());
        let scene_id = stable_region_scene_capture_id(&SmolStr::new("collision_region"));
        let dispatcher = prepare_batched_point_normal_dispatch(
            &ctx,
            None,
            region_capture(scene_id, 1),
            scene_domain(scene_id),
            &[point_query([0.0, 0.0, 0.0]), point_query([2.0, 0.0, 0.0])],
        )
        .expect("collision GPU point-normal dispatcher");

        let native = dispatcher.native().clone();
        let mut profiler = GpuPassProfiler::new(&native, 1);
        let mut encoder = native
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("wrela.collision.gpu.normal.test.encoder"),
            });
        assert!(
            dispatcher
                .initialize_dispatch_state()
                .expect("dispatch upload")
                > 0
        );
        dispatcher.encode_compute_pass(&mut encoder, &mut profiler);
        profiler.resolve_into(&mut encoder);
        native.queue.submit(Some(encoder.finish()));

        let result = dispatcher.dispatch_result();
        let result_bytes =
            readback_storage_buffer_on(&native, &result.values.buffer, result.values.size_bytes)
                .expect("result readback");
        let values = portable_abi_decode_slice(
            result.values.abi.as_ref().expect("normal result ABI"),
            &result_bytes,
            result.item_count as usize,
        )
        .expect("decode normal results");
        let normals = values
            .into_iter()
            .map(|value| match value {
                KernelValue::Struct(result) => match result
                    .fields
                    .iter()
                    .find(|(name, _)| name.as_str() == "normal")
                {
                    Some((_, KernelValue::Vec3(normal))) => *normal,
                    other => panic!("expected NormalResult.normal, found {other:?}"),
                },
                other => panic!("expected NormalResult values, found {other:?}"),
            })
            .collect::<Vec<_>>();
        assert_eq!(normals.len(), 2);
        assert!(normals[0][2].abs() > 0.5);

        let metrics = result.metrics.expect("observability handle");
        let metrics_bytes =
            readback_storage_buffer_on(&native, &metrics.buffer, metrics.size_bytes)
                .expect("metrics readback");
        let observability =
            dispatcher.decode_observability(&metrics_bytes, dispatcher.initial_gpu_runtime());
        assert_eq!(observability.dispatch_count, 1);
        assert_eq!(observability.dispatch_items, 2);
        assert_eq!(observability.gpu_runtime.attachment_decode_count, 0);
    }

    #[test]
    fn collision_gpu_adapter_executes_batched_world_trace_queries() {
        let ctx = typed_query_module(fixture_source());
        let scene_id = stable_region_scene_capture_id(&SmolStr::new("collision_region"));
        let dispatcher = prepare_batched_ray_trace_dispatch(
            &ctx,
            None,
            region_capture(scene_id, 1),
            scene_domain(scene_id),
            &[
                ray_query([0.0, 0.0, 2.0], [0.0, 0.0, -1.0]),
                ray_query([2.0, 0.0, 2.0], [0.0, 0.0, -1.0]),
            ],
        )
        .expect("collision GPU ray-trace dispatcher");

        let native = dispatcher.native().clone();
        let mut profiler = GpuPassProfiler::new(&native, 1);
        let mut encoder = native
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("wrela.collision.gpu.trace.test.encoder"),
            });
        assert!(
            dispatcher
                .initialize_dispatch_state()
                .expect("dispatch upload")
                > 0
        );
        dispatcher.encode_compute_pass(&mut encoder, &mut profiler);
        profiler.resolve_into(&mut encoder);
        native.queue.submit(Some(encoder.finish()));

        let result = dispatcher.dispatch_result();
        let result_bytes =
            readback_storage_buffer_on(&native, &result.values.buffer, result.values.size_bytes)
                .expect("result readback");
        let values = portable_abi_decode_slice(
            result.values.abi.as_ref().expect("trace result ABI"),
            &result_bytes,
            result.item_count as usize,
        )
        .expect("decode trace results");
        let hits = values
            .into_iter()
            .map(|value| match value {
                KernelValue::Struct(result) => result,
                other => panic!("expected Hit3 results, found {other:?}"),
            })
            .collect::<Vec<_>>();
        assert_eq!(hits.len(), 2);
        assert!(matches!(
            hits[0]
                .fields
                .iter()
                .find(|(name, _)| name.as_str() == "hit"),
            Some((_, KernelValue::Bool(true)))
        ));
        assert!(matches!(
            hits[1]
                .fields
                .iter()
                .find(|(name, _)| name.as_str() == "hit"),
            Some((_, KernelValue::Bool(false)))
        ));

        let metrics = result.metrics.expect("observability handle");
        let metrics_bytes =
            readback_storage_buffer_on(&native, &metrics.buffer, metrics.size_bytes)
                .expect("metrics readback");
        let observability =
            dispatcher.decode_observability(&metrics_bytes, dispatcher.initial_gpu_runtime());
        assert_eq!(observability.dispatch_count, 1);
        assert_eq!(observability.dispatch_items, 2);
        assert_eq!(observability.gpu_runtime.attachment_decode_count, 0);
    }
}
