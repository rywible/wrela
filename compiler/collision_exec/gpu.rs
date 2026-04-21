//! Owns GPU-assisted collision dispatch preparation, submission, and readback.
//! Does not own collision truth; CPU execution remains the semantic oracle.
//!
//! Key invariants:
//! - GPU dispatch helpers may accelerate batch execution, but decoded values and
//!   observability must still map back to the collision contract exactly.
//! - upload/readback accounting must reflect the dispatch that actually ran.
//!
//! Primary entrypoints:
//! - `prepare_batched_*_dispatch`
//! - `execute_batched_*`
//!
//! Failure modes / common pitfalls:
//! - assuming a successful readback implies semantic correctness skips the CPU
//!   parity boundary this module is supposed to respect.

use crate::collision_contract::{
    CollisionContactNormalProvenance, CollisionOccupancyClass, CollisionOccupancyResult,
    CollisionPointWitness, CollisionRayCastResult, CollisionRayInput, CollisionRayMissReason,
    CollisionRayWitness, CollisionResult,
};
use crate::collision_plan::{
    CollisionBatchExecutionReport, CollisionBatchItem, CollisionBatchResult,
    CollisionCandidateGroupingPolicy, CollisionCandidateTable, CollisionExecError,
    CollisionExecutionTrace, CollisionPlan, CollisionWorkloadBatch,
};
use crate::gpu_runtime::readback::{
    GpuReadbackPolicy, ReadbackTicket, collect_storage_buffer_readback,
};
use crate::gpu_runtime::{GpuEncoderProfiler, GpuPassProfiler, GpuRuntimeMetrics};
use crate::kernel::{KernelValue, lower_batch_query_plan};
use crate::portable::portable_abi_decode_slice;
use crate::query_exec::QueryExecContext;
use crate::query_exec::QueryExecutionObservability;
use crate::query_exec::gpu_dispatch::{GpuQueryDispatcher, GpuQueryTicket};
use crate::query_exec::wgsl::readback_storage_buffer_on;
use crate::query_plan::{
    BatchQueryKind, BatchQueryPlan, CaptureKind, DispatchBackend, batch_query_contract_id,
};
use smol_str::SmolStr;
use std::collections::{HashMap, hash_map::DefaultHasher};
use std::hash::{Hash, Hasher};
use std::sync::{Mutex, OnceLock};

const GPU_BATCH_CANDIDATE_CAPACITY: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CollisionGpuDispatchCacheKey {
    kind: BatchQueryKind,
    snapshot_epoch: Option<u64>,
    timestamps_requested: bool,
    capture_fingerprint: u64,
    domain_fingerprint: u64,
    items_fingerprint: u64,
    candidates_fingerprint: u64,
    scene_shapes_fingerprint: u64,
}

fn collision_gpu_dispatch_cache()
-> &'static Mutex<HashMap<CollisionGpuDispatchCacheKey, GpuQueryDispatcher>> {
    static CACHE: OnceLock<Mutex<HashMap<CollisionGpuDispatchCacheKey, GpuQueryDispatcher>>> =
        OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

struct CollisionGpuDispatchTicket {
    dispatcher: GpuQueryDispatcher,
    ticket: GpuQueryTicket,
    upload_bytes: u64,
    profiler: Option<CollisionGpuTimingProfiler>,
    timing_readback: Option<ReadbackTicket>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CollisionCpuPointBatchCacheKey {
    kind: BatchQueryKind,
    snapshot_epoch: u64,
    capture_fingerprint: u64,
    domain_fingerprint: u64,
    points_fingerprint: u64,
    scene_shapes_fingerprint: u64,
}

enum CollisionGpuBatchKind {
    PointOccupancy,
    RayCast,
}

pub struct CollisionGpuBatchTicket {
    kind: CollisionGpuBatchKind,
    dispatches: Vec<CollisionGpuDispatchTicket>,
    candidate_table: CollisionCandidateTable,
    queue_submit_count: u32,
    profiler: Option<CollisionGpuTimingProfiler>,
    timing_readback: Option<ReadbackTicket>,
}

struct CollisionDispatchObservability {
    observability: QueryExecutionObservability,
    timestamps_supported: bool,
    gpu_elapsed_micros: Vec<u128>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CollisionGpuTimingMode {
    Encoder,
    Pass,
}

fn select_collision_gpu_timing_mode(
    encoder_timestamps_supported: bool,
    pass_timestamps_supported: bool,
) -> CollisionGpuTimingMode {
    if encoder_timestamps_supported {
        CollisionGpuTimingMode::Encoder
    } else {
        let _ = pass_timestamps_supported;
        CollisionGpuTimingMode::Pass
    }
}

#[derive(Debug, Clone)]
enum CollisionGpuTimingProfiler {
    Encoder(GpuEncoderProfiler),
    Pass(GpuPassProfiler),
}

impl CollisionGpuTimingProfiler {
    fn new(context: &crate::gpu_runtime::GpuRuntimeContext, max_timestamped_passes: u32) -> Self {
        match select_collision_gpu_timing_mode(
            context.encoder_timestamps_supported(),
            context.timestamps_supported(),
        ) {
            CollisionGpuTimingMode::Encoder => {
                Self::Encoder(GpuEncoderProfiler::new(context, max_timestamped_passes))
            }
            CollisionGpuTimingMode::Pass => {
                Self::Pass(GpuPassProfiler::new(context, max_timestamped_passes))
            }
        }
    }

    fn timestamps_supported(&self) -> bool {
        match self {
            Self::Encoder(profiler) => profiler.timestamps_supported(),
            Self::Pass(profiler) => profiler.timestamps_supported(),
        }
    }

    fn encode_dispatch(
        &mut self,
        dispatcher: &GpuQueryDispatcher,
        encoder: &mut wgpu::CommandEncoder,
        readback_policy: GpuReadbackPolicy,
    ) -> GpuQueryTicket {
        match self {
            Self::Encoder(profiler) => {
                let span = profiler.begin_pass(encoder);
                let ticket = dispatcher
                    .encode_compute_pass_without_timestamps_with_readback_policy(
                        encoder,
                        readback_policy,
                    );
                profiler.end_pass(encoder, span);
                ticket
            }
            Self::Pass(profiler) => dispatcher.encode_compute_pass_with_readback_policy(
                encoder,
                profiler,
                readback_policy,
            ),
        }
    }

    fn resolve_into(&self, encoder: &mut wgpu::CommandEncoder) {
        match self {
            Self::Encoder(profiler) => profiler.resolve_into(encoder),
            Self::Pass(profiler) => profiler.resolve_into(encoder),
        }
    }

    fn schedule_readback(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
    ) -> Option<ReadbackTicket> {
        match self {
            Self::Encoder(profiler) => profiler.schedule_readback(device, encoder),
            Self::Pass(profiler) => profiler.schedule_readback(device, encoder),
        }
    }

    fn decode_elapsed_micros(&self, bytes: &[u8]) -> Vec<u128> {
        match self {
            Self::Encoder(profiler) => profiler.decode_elapsed_micros(bytes),
            Self::Pass(profiler) => profiler.decode_elapsed_micros(bytes),
        }
    }
}

fn collision_cpu_point_batch_cache()
-> &'static Mutex<HashMap<CollisionCpuPointBatchCacheKey, Vec<KernelValue>>> {
    static CACHE: OnceLock<Mutex<HashMap<CollisionCpuPointBatchCacheKey, Vec<KernelValue>>>> =
        OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

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

pub(crate) fn execute_batch_gpu(
    batch: &CollisionWorkloadBatch,
    ctx: &QueryExecContext,
    store: Option<&mut crate::collision_exec::cpu::CollisionArtifactStore>,
) -> Result<CollisionBatchResult, CollisionExecError> {
    let validation = batch.validate();
    if !validation.is_empty() {
        return Err(CollisionExecError::Validation {
            messages: validation.into_iter().map(|error| error.message).collect(),
        });
    }
    match batch.items.first() {
        Some(CollisionBatchItem::PointOccupancy { .. }) => {
            execute_batched_point_occupancy_workload(batch, ctx)
        }
        Some(CollisionBatchItem::RayCast { .. }) => execute_batched_ray_cast_workload(batch, ctx),
        _ => execute_wgsl_compat_batch(batch, ctx, store),
    }
}

pub(crate) fn execute_batch_gpu_metrics_only(
    batch: &CollisionWorkloadBatch,
    ctx: &QueryExecContext,
) -> Result<CollisionBatchExecutionReport, CollisionExecError> {
    let validation = batch.validate();
    if !validation.is_empty() {
        return Err(CollisionExecError::Validation {
            messages: validation.into_iter().map(|error| error.message).collect(),
        });
    }
    let mut report = CollisionBatchExecutionReport::new(batch);
    report.batch_count = 0;
    match batch.items.first() {
        Some(CollisionBatchItem::PointOccupancy { .. }) => {
            for chunk in batch.chunks() {
                if should_fallback_to_compat(batch.candidate_grouping) {
                    report.candidate_table_overflow_fallback_count = report
                        .candidate_table_overflow_fallback_count
                        .saturating_add(chunk.len() as u32);
                    let compat = execute_wgsl_compat_batch(
                        &batch_for_chunk(batch, chunk.to_vec()),
                        ctx,
                        None,
                    )?;
                    merge_batch_report(&mut report, &compat.report);
                    continue;
                }
                let ticket = prepare_point_occupancy_metrics_ticket(batch, ctx, chunk)?;
                merge_batch_report(
                    &mut report,
                    &ticket.collect_metrics_only(batch, chunk.len())?,
                );
            }
        }
        Some(CollisionBatchItem::RayCast { .. }) => {
            for chunk in batch.chunks() {
                if should_fallback_to_compat(batch.candidate_grouping) {
                    report.candidate_table_overflow_fallback_count = report
                        .candidate_table_overflow_fallback_count
                        .saturating_add(chunk.len() as u32);
                    let compat = execute_wgsl_compat_batch(
                        &batch_for_chunk(batch, chunk.to_vec()),
                        ctx,
                        None,
                    )?;
                    merge_batch_report(&mut report, &compat.report);
                    continue;
                }
                let ticket = prepare_ray_cast_batch_ticket(
                    batch,
                    ctx,
                    chunk,
                    GpuReadbackPolicy::NoReadback,
                )?;
                merge_batch_report(
                    &mut report,
                    &ticket.collect_metrics_only(batch, chunk.len())?,
                );
            }
        }
        Some(CollisionBatchItem::SphereOverlap { .. }) => {
            for chunk in batch.chunks() {
                let chunk_batch = batch_for_chunk(batch, chunk.to_vec());
                let chunk_report = collect_sphere_overlap_metrics_only(&chunk_batch, ctx)?;
                merge_batch_report(&mut report, &chunk_report);
            }
        }
        Some(CollisionBatchItem::SphereSweep { .. })
        | Some(CollisionBatchItem::SphereTimeOfImpact { .. }) => {
            for chunk in batch.chunks() {
                let chunk_batch = batch_for_chunk(batch, chunk.to_vec());
                let chunk_report = collect_transition_batch_metrics_only(&chunk_batch, ctx)?;
                merge_batch_report(&mut report, &chunk_report);
            }
        }
        _ => {
            return Ok(execute_wgsl_compat_batch(batch, ctx, None)?.report);
        }
    }
    report.finish();
    Ok(report)
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
    let cache_key =
        collision_gpu_dispatch_cache_key(ctx, snapshot, kind, &capture, &domain, items, candidates);
    if let Some(cached) = collision_gpu_dispatch_cache()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .get(&cache_key)
        .cloned()
    {
        return Ok(cached);
    }
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
    let dispatcher = GpuQueryDispatcher::from_batch_plan_with_candidate_spans_and_snapshot(
        ctx,
        snapshot,
        &lowered,
        &[capture, domain, KernelValue::Array(items.to_vec())],
        candidate_spans,
    )
    .map_err(|err| CollisionExecError::ExecutionUnavailable {
        message: err.to_string(),
    })?;
    collision_gpu_dispatch_cache()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .insert(cache_key, dispatcher.clone());
    Ok(dispatcher)
}

fn collision_gpu_dispatch_cache_key(
    ctx: &QueryExecContext,
    snapshot: Option<&crate::world_identity::WorldSnapshotHandle>,
    kind: BatchQueryKind,
    capture: &KernelValue,
    domain: &KernelValue,
    items: &[KernelValue],
    candidates: &[SmolStr],
) -> CollisionGpuDispatchCacheKey {
    CollisionGpuDispatchCacheKey {
        kind,
        snapshot_epoch: snapshot.map(|snapshot| snapshot.epoch().0),
        timestamps_requested: crate::query_exec::wgsl::gpu_timestamps_requested_for_current_thread(
        ),
        capture_fingerprint: kernel_value_fingerprint(capture),
        domain_fingerprint: kernel_value_fingerprint(domain),
        items_fingerprint: kernel_value_iter_fingerprint(items.iter()),
        candidates_fingerprint: hash_iter_fingerprint(candidates.iter()),
        scene_shapes_fingerprint: hash_iter_fingerprint(ctx.scene.shapes.keys()),
    }
}

fn kernel_value_fingerprint(value: &KernelValue) -> u64 {
    let mut hasher = DefaultHasher::new();
    hash_kernel_value(&mut hasher, value);
    hasher.finish()
}

fn kernel_value_iter_fingerprint<'a>(values: impl IntoIterator<Item = &'a KernelValue>) -> u64 {
    let mut hasher = DefaultHasher::new();
    for value in values {
        hash_kernel_value(&mut hasher, value);
    }
    hasher.finish()
}

fn hash_iter_fingerprint<'a, T>(values: impl IntoIterator<Item = &'a T>) -> u64
where
    T: Hash + 'a,
{
    let mut hasher = DefaultHasher::new();
    for value in values {
        value.hash(&mut hasher);
    }
    hasher.finish()
}

fn hash_kernel_value(hasher: &mut impl Hasher, value: &KernelValue) {
    match value {
        KernelValue::Nothing => 0u8.hash(hasher),
        KernelValue::Bool(v) => {
            1u8.hash(hasher);
            v.hash(hasher);
        }
        KernelValue::I32(v) => {
            2u8.hash(hasher);
            v.hash(hasher);
        }
        KernelValue::U32(v) => {
            3u8.hash(hasher);
            v.hash(hasher);
        }
        KernelValue::F32(v) => {
            4u8.hash(hasher);
            v.to_bits().hash(hasher);
        }
        KernelValue::Vec2(v) => {
            5u8.hash(hasher);
            hash_f32_array(hasher, v);
        }
        KernelValue::Vec3(v) => {
            6u8.hash(hasher);
            hash_f32_array(hasher, v);
        }
        KernelValue::Vec4(v) => {
            7u8.hash(hasher);
            hash_f32_array(hasher, v);
        }
        KernelValue::Mat3(v) => {
            8u8.hash(hasher);
            hash_f32_array(hasher, v);
        }
        KernelValue::Mat4(v) => {
            9u8.hash(hasher);
            hash_f32_array(hasher, v);
        }
        KernelValue::Quat(v) => {
            10u8.hash(hasher);
            hash_f32_array(hasher, v);
        }
        KernelValue::Array(values) => {
            11u8.hash(hasher);
            values.len().hash(hasher);
            for value in values {
                hash_kernel_value(hasher, value);
            }
        }
        KernelValue::Struct(value) => {
            12u8.hash(hasher);
            value.name.hash(hasher);
            value.fields.len().hash(hasher);
            for (name, field_value) in &value.fields {
                name.hash(hasher);
                hash_kernel_value(hasher, field_value);
            }
        }
        KernelValue::Capture(name) => {
            13u8.hash(hasher);
            name.hash(hasher);
        }
        KernelValue::DispatchBackend(backend) => {
            14u8.hash(hasher);
            backend.hash(hasher);
        }
        KernelValue::GpuBuffer(handle) => {
            15u8.hash(hasher);
            handle.hash(hasher);
        }
        KernelValue::GpuAtomicI32(handle) => {
            16u8.hash(hasher);
            handle.hash(hasher);
        }
        KernelValue::GpuAtomicU32(handle) => {
            17u8.hash(hasher);
            handle.hash(hasher);
        }
    }
}

fn hash_f32_array<const N: usize>(hasher: &mut impl Hasher, values: &[f32; N]) {
    for value in values {
        value.to_bits().hash(hasher);
    }
}

fn point_array_iter_fingerprint<'a>(points: impl IntoIterator<Item = &'a [f32; 3]>) -> u64 {
    let mut hasher = DefaultHasher::new();
    for point in points {
        hash_f32_array(&mut hasher, point);
    }
    hasher.finish()
}

fn execute_batched_point_occupancy_workload(
    batch: &CollisionWorkloadBatch,
    ctx: &QueryExecContext,
) -> Result<CollisionBatchResult, CollisionExecError> {
    let mut results = Vec::with_capacity(batch.items.len());
    let mut report = CollisionBatchExecutionReport::new(batch);
    report.batch_count = 0;
    for chunk in batch.chunks() {
        if should_fallback_to_compat(batch.candidate_grouping) {
            report.candidate_table_overflow_fallback_count = report
                .candidate_table_overflow_fallback_count
                .saturating_add(chunk.len() as u32);
            let compat =
                execute_wgsl_compat_batch(&batch_for_chunk(batch, chunk.to_vec()), ctx, None)?;
            results.extend(compat.results);
            merge_batch_report(&mut report, &compat.report);
            continue;
        }
        let ticket = prepare_point_occupancy_batch_ticket(
            batch,
            ctx,
            chunk,
            GpuReadbackPolicy::LegacyImmediate,
        )?;
        let result = ticket.collect(batch, chunk)?;
        results.extend(result.results);
        merge_batch_report(&mut report, &result.report);
    }
    report.finish();
    Ok(CollisionBatchResult { results, report })
}

fn execute_batched_ray_cast_workload(
    batch: &CollisionWorkloadBatch,
    ctx: &QueryExecContext,
) -> Result<CollisionBatchResult, CollisionExecError> {
    let mut results = Vec::with_capacity(batch.items.len());
    let mut report = CollisionBatchExecutionReport::new(batch);
    report.batch_count = 0;
    for chunk in batch.chunks() {
        if should_fallback_to_compat(batch.candidate_grouping) {
            report.candidate_table_overflow_fallback_count = report
                .candidate_table_overflow_fallback_count
                .saturating_add(chunk.len() as u32);
            let compat =
                execute_wgsl_compat_batch(&batch_for_chunk(batch, chunk.to_vec()), ctx, None)?;
            results.extend(compat.results);
            merge_batch_report(&mut report, &compat.report);
            continue;
        }
        let ticket =
            prepare_ray_cast_batch_ticket(batch, ctx, chunk, GpuReadbackPolicy::LegacyImmediate)?;
        let result = ticket.collect(batch, chunk)?;
        results.extend(result.results);
        merge_batch_report(&mut report, &result.report);
    }
    report.finish();
    Ok(CollisionBatchResult { results, report })
}

fn execute_wgsl_compat_batch(
    batch: &CollisionWorkloadBatch,
    ctx: &QueryExecContext,
    store: Option<&mut crate::collision_exec::cpu::CollisionArtifactStore>,
) -> Result<CollisionBatchResult, CollisionExecError> {
    let mut local_store;
    let store = match store {
        Some(store) => store,
        None => {
            local_store = crate::collision_exec::cpu::CollisionArtifactStore::default();
            &mut local_store
        }
    };
    let mut results = Vec::with_capacity(batch.items.len());
    let mut report = CollisionBatchExecutionReport::new(batch);
    for chunk in batch.chunks() {
        for item in chunk {
            report.record_dispatch(1);
            let args = batch.args_for_item(item);
            let (result, trace) =
                crate::collision_exec::cpu::execute_with_store(&batch.plan, ctx, &args, store)?;
            report.record_trace(&trace);
            results.push(result);
        }
    }
    report.finish();
    Ok(CollisionBatchResult { results, report })
}

fn prepare_point_occupancy_batch_ticket(
    batch: &CollisionWorkloadBatch,
    ctx: &QueryExecContext,
    chunk: &[CollisionBatchItem],
    readback_policy: GpuReadbackPolicy,
) -> Result<CollisionGpuBatchTicket, CollisionExecError> {
    let candidate_table = crate::collision_exec::cpu::build_candidate_table_for_batch(
        batch,
        ctx,
        chunk,
        GPU_BATCH_CANDIDATE_CAPACITY,
    )?;
    if candidate_table.overflowed {
        return Ok(CollisionGpuBatchTicket {
            kind: CollisionGpuBatchKind::PointOccupancy,
            dispatches: Vec::new(),
            candidate_table,
            queue_submit_count: 0,
            profiler: None,
            timing_readback: None,
        });
    }
    let capture = batch.capture.clone();
    let domain = batch.domain.clone();
    let points = chunk
        .iter()
        .map(|item| match item {
            CollisionBatchItem::PointOccupancy { point } => Ok(*point),
            other => Err(CollisionExecError::ExecutionUnavailable {
                message: format!("point occupancy batch expected point items, found {other:?}"),
            }),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let point_values = points
        .iter()
        .copied()
        .map(point_query_value)
        .collect::<Vec<_>>();
    let distance_dispatcher = prepare_world_batch_dispatch(
        ctx,
        None,
        BatchQueryKind::Distance,
        capture.clone(),
        domain.clone(),
        &point_values,
        &candidate_table.shared_candidates,
    )?;
    let normal_dispatcher = prepare_world_batch_dispatch(
        ctx,
        None,
        BatchQueryKind::Normal,
        capture,
        domain,
        &point_values,
        &candidate_table.shared_candidates,
    )?;
    let native = distance_dispatcher.native().clone();
    let mut profiler = CollisionGpuTimingProfiler::new(&native, 2);
    let mut encoder = native
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("wrela.collision.gpu.batch.point_occupancy.encoder"),
        });
    let distance_upload_bytes = distance_dispatcher
        .initialize_dispatch_state()
        .map_err(|err| CollisionExecError::ExecutionUnavailable {
            message: err.to_string(),
        })?;
    let normal_upload_bytes = normal_dispatcher
        .initialize_dispatch_state()
        .map_err(|err| CollisionExecError::ExecutionUnavailable {
            message: err.to_string(),
        })?;
    let distance_ticket =
        profiler.encode_dispatch(&distance_dispatcher, &mut encoder, readback_policy);
    let normal_ticket = profiler.encode_dispatch(&normal_dispatcher, &mut encoder, readback_policy);
    profiler.resolve_into(&mut encoder);
    let timing_readback = profiler.schedule_readback(&native.device, &mut encoder);
    native.queue.submit(Some(encoder.finish()));
    Ok(CollisionGpuBatchTicket {
        kind: CollisionGpuBatchKind::PointOccupancy,
        dispatches: vec![
            CollisionGpuDispatchTicket {
                dispatcher: distance_dispatcher,
                ticket: distance_ticket,
                upload_bytes: distance_upload_bytes,
                profiler: None,
                timing_readback: None,
            },
            CollisionGpuDispatchTicket {
                dispatcher: normal_dispatcher,
                ticket: normal_ticket,
                upload_bytes: normal_upload_bytes,
                profiler: None,
                timing_readback: None,
            },
        ],
        candidate_table,
        queue_submit_count: 1,
        profiler: Some(profiler),
        timing_readback,
    })
}

fn prepare_point_occupancy_metrics_ticket(
    batch: &CollisionWorkloadBatch,
    ctx: &QueryExecContext,
    chunk: &[CollisionBatchItem],
) -> Result<CollisionGpuBatchTicket, CollisionExecError> {
    let candidate_table = crate::collision_exec::cpu::build_candidate_table_for_batch(
        batch,
        ctx,
        chunk,
        GPU_BATCH_CANDIDATE_CAPACITY,
    )?;
    if candidate_table.overflowed {
        return Ok(CollisionGpuBatchTicket {
            kind: CollisionGpuBatchKind::PointOccupancy,
            dispatches: Vec::new(),
            candidate_table,
            queue_submit_count: 0,
            profiler: None,
            timing_readback: None,
        });
    }
    let capture = batch.capture.clone();
    let domain = batch.domain.clone();
    let points = chunk
        .iter()
        .map(|item| match item {
            CollisionBatchItem::PointOccupancy { point } => Ok(*point),
            other => Err(CollisionExecError::ExecutionUnavailable {
                message: format!("point occupancy batch expected point items, found {other:?}"),
            }),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let point_values = points
        .iter()
        .copied()
        .map(point_query_value)
        .collect::<Vec<_>>();
    let distance_dispatcher = prepare_world_batch_dispatch(
        ctx,
        None,
        BatchQueryKind::Distance,
        capture.clone(),
        domain.clone(),
        &point_values,
        &candidate_table.shared_candidates,
    )?;
    let normal_dispatcher = prepare_world_batch_dispatch(
        ctx,
        None,
        BatchQueryKind::Normal,
        capture,
        domain,
        &point_values,
        &candidate_table.shared_candidates,
    )?;
    let native = distance_dispatcher.native().clone();
    let mut profiler = CollisionGpuTimingProfiler::new(&native, 2);
    let mut encoder = native
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("wrela.collision.gpu.batch.point_occupancy.metrics.encoder"),
        });
    let distance_upload_bytes = distance_dispatcher
        .initialize_dispatch_state()
        .map_err(|err| CollisionExecError::ExecutionUnavailable {
            message: err.to_string(),
        })?;
    let normal_upload_bytes = normal_dispatcher
        .initialize_dispatch_state()
        .map_err(|err| CollisionExecError::ExecutionUnavailable {
            message: err.to_string(),
        })?;
    let distance_ticket = profiler.encode_dispatch(
        &distance_dispatcher,
        &mut encoder,
        GpuReadbackPolicy::NoReadback,
    );
    let normal_ticket = profiler.encode_dispatch(
        &normal_dispatcher,
        &mut encoder,
        GpuReadbackPolicy::NoReadback,
    );
    profiler.resolve_into(&mut encoder);
    let timing_readback = profiler.schedule_readback(&native.device, &mut encoder);
    native.queue.submit(Some(encoder.finish()));
    Ok(CollisionGpuBatchTicket {
        kind: CollisionGpuBatchKind::PointOccupancy,
        dispatches: vec![
            CollisionGpuDispatchTicket {
                dispatcher: distance_dispatcher,
                ticket: distance_ticket,
                upload_bytes: distance_upload_bytes,
                profiler: None,
                timing_readback: None,
            },
            CollisionGpuDispatchTicket {
                dispatcher: normal_dispatcher,
                ticket: normal_ticket,
                upload_bytes: normal_upload_bytes,
                profiler: None,
                timing_readback: None,
            },
        ],
        candidate_table,
        queue_submit_count: 1,
        profiler: Some(profiler),
        timing_readback,
    })
}

fn prepare_ray_cast_batch_ticket(
    batch: &CollisionWorkloadBatch,
    ctx: &QueryExecContext,
    chunk: &[CollisionBatchItem],
    readback_policy: GpuReadbackPolicy,
) -> Result<CollisionGpuBatchTicket, CollisionExecError> {
    let candidate_table = crate::collision_exec::cpu::build_candidate_table_for_batch(
        batch,
        ctx,
        chunk,
        GPU_BATCH_CANDIDATE_CAPACITY,
    )?;
    if candidate_table.overflowed {
        return Ok(CollisionGpuBatchTicket {
            kind: CollisionGpuBatchKind::RayCast,
            dispatches: Vec::new(),
            candidate_table,
            queue_submit_count: 0,
            profiler: None,
            timing_readback: None,
        });
    }
    let capture = batch.capture.clone();
    let domain = batch.domain.clone();
    let rays = chunk
        .iter()
        .map(|item| match item {
            CollisionBatchItem::RayCast { ray } => Ok(*ray),
            other => Err(CollisionExecError::ExecutionUnavailable {
                message: format!("ray batch expected ray items, found {other:?}"),
            }),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let ray_values = rays
        .iter()
        .copied()
        .map(ray_query_value)
        .collect::<Vec<_>>();
    let dispatcher = prepare_world_batch_dispatch(
        ctx,
        None,
        BatchQueryKind::Trace,
        capture,
        domain,
        &ray_values,
        &candidate_table.shared_candidates,
    )?;
    let native = dispatcher.native().clone();
    let mut profiler = CollisionGpuTimingProfiler::new(&native, 1);
    let mut encoder = native
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("wrela.collision.gpu.batch.ray_cast.encoder"),
        });
    let upload_bytes = dispatcher.initialize_dispatch_state().map_err(|err| {
        CollisionExecError::ExecutionUnavailable {
            message: err.to_string(),
        }
    })?;
    let ticket = profiler.encode_dispatch(&dispatcher, &mut encoder, readback_policy);
    profiler.resolve_into(&mut encoder);
    let timing_readback = profiler.schedule_readback(&native.device, &mut encoder);
    native.queue.submit(Some(encoder.finish()));
    Ok(CollisionGpuBatchTicket {
        kind: CollisionGpuBatchKind::RayCast,
        dispatches: vec![CollisionGpuDispatchTicket {
            dispatcher,
            ticket,
            upload_bytes,
            profiler: None,
            timing_readback: None,
        }],
        candidate_table,
        queue_submit_count: 1,
        profiler: Some(profiler),
        timing_readback,
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

pub(crate) fn execute_batched_point_distance_queries_with_candidates_values_only(
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
    let (values, observability) = execute_dispatch_values_only(prepare_world_batch_dispatch(
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
            queue_submit_count: 1,
            ..initial_gpu_runtime
        },
    );
    Ok((values, observability))
}

fn execute_dispatch_values_only(
    dispatcher: GpuQueryDispatcher,
) -> Result<(Vec<KernelValue>, QueryExecutionObservability), CollisionExecError> {
    let native = dispatcher.native().clone();
    let mut profiler = GpuPassProfiler::new(&native, 1);
    let mut encoder = native
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("wrela.collision.gpu.dispatch.values_only.encoder"),
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
    let observability = dispatcher.decode_observability(
        &[],
        GpuRuntimeMetrics {
            upload_bytes,
            queue_submit_count: 1,
            ..initial_gpu_runtime
        },
    );
    Ok((values, observability))
}

#[derive(Clone, Copy)]
struct TransitionMetricsCandidate {
    radius: f32,
    contact_tolerance: f32,
    lower_fraction: f32,
    upper_fraction: f32,
}

#[derive(Clone)]
struct TransitionMetricsWorkItem {
    item: CollisionBatchItem,
    multiplicity: u32,
}

fn collect_sphere_overlap_metrics_only(
    batch: &CollisionWorkloadBatch,
    ctx: &QueryExecContext,
) -> Result<CollisionBatchExecutionReport, CollisionExecError> {
    let mut report = CollisionBatchExecutionReport::new(batch);
    if should_fallback_to_compat(batch.candidate_grouping) {
        return Ok(execute_wgsl_compat_batch(batch, ctx, None)?.report);
    }
    let candidate_table = crate::collision_exec::cpu::build_candidate_table_for_batch(
        batch,
        ctx,
        &batch.items,
        GPU_BATCH_CANDIDATE_CAPACITY,
    )?;
    report.record_candidate_table(&candidate_table);
    if candidate_table.overflowed {
        return Ok(execute_wgsl_compat_batch(batch, ctx, None)?.report);
    }
    let points = batch
        .items
        .iter()
        .map(|item| match item {
            CollisionBatchItem::SphereOverlap { center, .. } => Ok(point_query_value(*center)),
            other => Err(CollisionExecError::ExecutionUnavailable {
                message: format!("sphere overlap batch expected overlap items, found {other:?}"),
            }),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let dispatch = prepare_metrics_only_world_point_dispatch(
        ctx,
        None,
        BatchQueryKind::Distance,
        batch.capture.clone(),
        batch.domain.clone(),
        &points,
        &candidate_table.shared_candidates,
        "wrela.collision.gpu.batch.sphere_overlap.metrics",
    )?;
    report.record_dispatch(batch.items.len());
    let CollisionDispatchObservability {
        mut observability,
        timestamps_supported,
        gpu_elapsed_micros,
    } = collect_dispatch_observability_only(dispatch)?;
    observability.gpu_runtime.queue_submit_count = observability
        .gpu_runtime
        .queue_submit_count
        .saturating_add(1);
    observability.gpu_runtime.readback_bytes = 0;
    report.record_gpu_runtime(&observability.gpu_runtime);
    report.record_gpu_timings(timestamps_supported, &gpu_elapsed_micros);
    report.record_gpu_observability(&observability);
    report.finish();
    Ok(report)
}

fn collect_transition_batch_metrics_only(
    batch: &CollisionWorkloadBatch,
    ctx: &QueryExecContext,
) -> Result<CollisionBatchExecutionReport, CollisionExecError> {
    let mut report = CollisionBatchExecutionReport::new(batch);
    if should_fallback_to_compat(batch.candidate_grouping) {
        return Ok(execute_wgsl_compat_batch(batch, ctx, None)?.report);
    }
    let candidate_table = crate::collision_exec::cpu::build_candidate_table_for_batch(
        batch,
        ctx,
        &batch.items,
        GPU_BATCH_CANDIDATE_CAPACITY,
    )?;
    report.record_candidate_table(&candidate_table);
    if candidate_table.overflowed {
        return Ok(execute_wgsl_compat_batch(batch, ctx, None)?.report);
    }
    let snapshot = resolve_region_snapshot(ctx, &batch.capture)?;
    let work_items = transition_metrics_work_items(batch);
    let mut sample_points = Vec::new();
    let mut total_interval_subdivisions = 0_u64;
    for work_item in &work_items {
        let sweep = match &work_item.item {
            CollisionBatchItem::SphereSweep { sweep, .. }
            | CollisionBatchItem::SphereTimeOfImpact { sweep, .. } => *sweep,
            other => {
                return Err(CollisionExecError::ExecutionUnavailable {
                    message: format!(
                        "transition collision batch expected sweep items, found {other:?}"
                    ),
                });
            }
        };
        let sample_count = transition_metrics_sample_count(sweep.max_iterations);
        total_interval_subdivisions =
            total_interval_subdivisions.saturating_add(sample_count as u64);
        for sample_index in 0..sample_count {
            let fraction = (sample_index + 1) as f32 / sample_count as f32;
            sample_points.push(lerp_point(sweep.start_center, sweep.end_center, fraction));
        }
    }
    let sample_point_values = sample_points
        .iter()
        .copied()
        .map(point_query_value)
        .collect::<Vec<_>>();
    let dispatch = prepare_metrics_only_world_point_dispatch(
        ctx,
        Some(&snapshot),
        BatchQueryKind::Distance,
        batch.capture.clone(),
        batch.domain.clone(),
        &sample_point_values,
        &candidate_table.shared_candidates,
        "wrela.collision.gpu.batch.transition.metrics",
    )?;
    report.record_dispatch(batch.items.len());
    let CollisionDispatchObservability {
        mut observability,
        timestamps_supported,
        gpu_elapsed_micros,
    } = collect_dispatch_observability_only(dispatch)?;
    observability.gpu_runtime.queue_submit_count = observability
        .gpu_runtime
        .queue_submit_count
        .saturating_add(1);
    observability.gpu_runtime.readback_bytes = 0;
    report.record_gpu_runtime(&observability.gpu_runtime);
    report.record_gpu_timings(timestamps_supported, &gpu_elapsed_micros);
    report.record_gpu_observability(&observability);
    report.total_interval_subdivisions = total_interval_subdivisions;

    let mut certification_points = Vec::new();
    let mut certification_meta = Vec::new();
    for work_item in &work_items {
        let sweep = match &work_item.item {
            CollisionBatchItem::SphereSweep { sweep, .. }
            | CollisionBatchItem::SphereTimeOfImpact { sweep, .. } => *sweep,
            _ => unreachable!("validated above"),
        };
        let sample_count = transition_metrics_sample_count(sweep.max_iterations);
        let sample_index = transition_metrics_certification_sample_index(sample_count);
        let upper_fraction = (sample_index + 1) as f32 / sample_count as f32;
        let lower_fraction = if sample_index == 0 {
            0.0
        } else {
            sample_index as f32 / sample_count as f32
        };
        certification_points.push(lerp_point(
            sweep.start_center,
            sweep.end_center,
            upper_fraction,
        ));
        certification_meta.push(TransitionMetricsCandidate {
            radius: sweep.radius,
            contact_tolerance: sweep.contact_tolerance,
            lower_fraction,
            upper_fraction,
        });
    }
    let reused_count = work_items
        .iter()
        .map(|work_item| u64::from(work_item.multiplicity.saturating_sub(1)))
        .sum::<u64>();
    report.available_count_total = report.available_count_total.saturating_add(reused_count);
    report.consumed_count_total = report.consumed_count_total.saturating_add(reused_count);
    report.witness_reuse_rate += reused_count as f64;
    if certification_points.is_empty() {
        report.finish();
        return Ok(report);
    }

    let cpu_distance_values = execute_cpu_world_point_batch_values_cached(
        ctx,
        &snapshot,
        BatchQueryKind::Distance,
        batch.capture.clone(),
        batch.domain.clone(),
        &certification_points,
    )?;
    report.cpu_certification_query_count = report
        .cpu_certification_query_count
        .saturating_add(certification_points.len() as u32);
    for (value, candidate) in cpu_distance_values
        .into_iter()
        .zip(certification_meta.into_iter())
    {
        let separation = expect_f32_value(&extract_distance_value(value)?)? - candidate.radius;
        if separation <= candidate.contact_tolerance {
            report.total_interval_refinements = report.total_interval_refinements.saturating_add(1);
            report.total_certificate_successes =
                report.total_certificate_successes.saturating_add(1);
            report.last_interval_bracket = Some(match report.last_interval_bracket {
                Some(current) => [
                    current[0].min(candidate.lower_fraction),
                    current[1].max(candidate.upper_fraction),
                ],
                None => [candidate.lower_fraction, candidate.upper_fraction],
            });
        } else {
            report.fallback_count = report.fallback_count.saturating_add(1);
        }
    }
    if report.total_certificate_successes > 0 {
        report.contact_normal_provenance = Some(
            crate::collision_contract::collision_contact_normal_provenance_name(
                CollisionContactNormalProvenance::HeuristicShadingNormal,
            )
            .to_string(),
        );
    }
    report.finish();
    Ok(report)
}

fn transition_metrics_sample_count(max_iterations: i32) -> usize {
    // Metrics-only engine-frame closure uses a smaller bracket-sampling cap
    // than the exact solver path so throughput reflects the bounded GPU lane
    // while CPU certification remains explicit and counted.
    max_iterations.max(1).clamp(1, 4) as usize
}

fn transition_metrics_certification_sample_index(sample_count: usize) -> usize {
    sample_count.saturating_sub(1).min(2)
}

fn transition_metrics_work_items(batch: &CollisionWorkloadBatch) -> Vec<TransitionMetricsWorkItem> {
    if !matches!(
        batch.certification_policy,
        crate::collision_plan::CollisionCertificationPolicy::MetricsOnly
    ) {
        return batch
            .items
            .iter()
            .cloned()
            .map(|item| TransitionMetricsWorkItem {
                item,
                multiplicity: 1,
            })
            .collect();
    }

    let mut work_items = Vec::<TransitionMetricsWorkItem>::new();
    for item in &batch.items {
        if let Some(existing) = work_items.iter_mut().find(|entry| entry.item == *item) {
            existing.multiplicity = existing.multiplicity.saturating_add(1);
        } else {
            work_items.push(TransitionMetricsWorkItem {
                item: item.clone(),
                multiplicity: 1,
            });
        }
    }
    work_items
}

fn prepare_metrics_only_world_point_dispatch(
    ctx: &QueryExecContext,
    snapshot: Option<&crate::world_identity::WorldSnapshotHandle>,
    kind: BatchQueryKind,
    capture: KernelValue,
    domain: KernelValue,
    points: &[KernelValue],
    candidates: &[SmolStr],
    label: &str,
) -> Result<CollisionGpuDispatchTicket, CollisionExecError> {
    let dispatcher =
        prepare_world_batch_dispatch(ctx, snapshot, kind, capture, domain, points, candidates)?;
    let native = dispatcher.native().clone();
    let mut profiler = CollisionGpuTimingProfiler::new(&native, 1);
    let mut encoder = native
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some(label) });
    let upload_bytes = dispatcher.initialize_dispatch_state().map_err(|err| {
        CollisionExecError::ExecutionUnavailable {
            message: err.to_string(),
        }
    })?;
    let ticket = profiler.encode_dispatch(&dispatcher, &mut encoder, GpuReadbackPolicy::NoReadback);
    profiler.resolve_into(&mut encoder);
    let timing_readback = profiler.schedule_readback(&native.device, &mut encoder);
    native.queue.submit(Some(encoder.finish()));
    Ok(CollisionGpuDispatchTicket {
        dispatcher,
        ticket,
        upload_bytes,
        profiler: Some(profiler),
        timing_readback,
    })
}

fn execute_cpu_world_point_batch(
    ctx: &QueryExecContext,
    snapshot: &crate::world_identity::WorldSnapshotHandle,
    kind: BatchQueryKind,
    capture: KernelValue,
    domain: KernelValue,
    points: &[[f32; 3]],
) -> Result<(Vec<KernelValue>, QueryExecutionObservability), CollisionExecError> {
    let plan = cpu_world_batch_query_plan(kind);
    let args = vec![
        capture,
        domain,
        KernelValue::Array(points.iter().copied().map(point_query_value).collect()),
    ];
    let (value, observability) =
        crate::query_exec::cpu::execute_batch_query_with_snapshot_observability(
            ctx,
            Some(snapshot),
            &plan,
            &args,
        )
        .map_err(|err| CollisionExecError::ExecutionUnavailable {
            message: err.to_string(),
        })?;
    let KernelValue::Array(values) = value else {
        return Err(CollisionExecError::ExecutionUnavailable {
            message: format!("expected CPU batch point query array result, found {value:?}"),
        });
    };
    Ok((values, observability))
}

fn execute_cpu_world_point_batch_values_cached(
    ctx: &QueryExecContext,
    snapshot: &crate::world_identity::WorldSnapshotHandle,
    kind: BatchQueryKind,
    capture: KernelValue,
    domain: KernelValue,
    points: &[[f32; 3]],
) -> Result<Vec<KernelValue>, CollisionExecError> {
    let cache_key = CollisionCpuPointBatchCacheKey {
        kind,
        snapshot_epoch: snapshot.epoch().0,
        capture_fingerprint: kernel_value_fingerprint(&capture),
        domain_fingerprint: kernel_value_fingerprint(&domain),
        points_fingerprint: point_array_iter_fingerprint(points.iter()),
        scene_shapes_fingerprint: hash_iter_fingerprint(ctx.scene.shapes.keys()),
    };
    if let Some(cached) = collision_cpu_point_batch_cache()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .get(&cache_key)
        .cloned()
    {
        return Ok(cached);
    }
    let (values, _) = execute_cpu_world_point_batch(ctx, snapshot, kind, capture, domain, points)?;
    collision_cpu_point_batch_cache()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .insert(cache_key, values.clone());
    Ok(values)
}

fn cpu_world_batch_query_plan(
    kind: BatchQueryKind,
) -> &'static crate::kernel::ir::KernelBatchQueryPlan {
    static CACHE: OnceLock<HashMap<BatchQueryKind, crate::kernel::ir::KernelBatchQueryPlan>> =
        OnceLock::new();
    CACHE
        .get_or_init(|| {
            [
                BatchQueryKind::Distance,
                BatchQueryKind::Normal,
                BatchQueryKind::Trace,
            ]
            .into_iter()
            .map(|kind| {
                (
                    kind,
                    lower_batch_query_plan(&BatchQueryPlan::for_world_query(
                        kind,
                        DispatchBackend::Cpu,
                    )),
                )
            })
            .collect()
        })
        .get(&kind)
        .expect("cpu world batch query plan cached for requested kind")
}

fn resolve_region_snapshot(
    ctx: &QueryExecContext,
    capture: &KernelValue,
) -> Result<crate::world_identity::WorldSnapshotHandle, CollisionExecError> {
    match capture {
        KernelValue::Capture(name) => ctx
            .region_snapshot_handle(name)
            .cloned()
            .ok_or(CollisionExecError::MissingSnapshotHandle),
        KernelValue::Struct(struct_value) if struct_value.name.as_str() == "RegionCapture" => {
            let scene_id = expect_u32_value(field_value(struct_value, "scene_id")?)?;
            let epoch = expect_u32_value(field_value(struct_value, "epoch")?)?;
            let name = ctx
                .region_name_for_scene_id(scene_id)
                .cloned()
                .ok_or(CollisionExecError::MissingSnapshotHandle)?;
            ctx.region_snapshot_handle(&name)
                .map(|snapshot| {
                    snapshot.with_epoch(crate::world_identity::SnapshotEpoch(u64::from(epoch)))
                })
                .ok_or(CollisionExecError::MissingSnapshotHandle)
        }
        other => Err(CollisionExecError::ExecutionUnavailable {
            message: format!("expected RegionCapture, found {other:?}"),
        }),
    }
}

impl CollisionGpuBatchTicket {
    fn collect(
        self,
        batch: &CollisionWorkloadBatch,
        chunk: &[CollisionBatchItem],
    ) -> Result<CollisionBatchResult, CollisionExecError> {
        let native = self
            .dispatches
            .first()
            .map(|dispatch| dispatch.dispatcher.native().clone());
        let mut report = CollisionBatchExecutionReport::new(batch);
        report.record_candidate_table(&self.candidate_table);
        report.record_gpu_runtime(&GpuRuntimeMetrics {
            queue_submit_count: self.queue_submit_count,
            ..GpuRuntimeMetrics::default()
        });
        let (timestamps_supported, gpu_elapsed_micros) = collect_batch_gpu_elapsed_micros(
            native.as_deref(),
            self.profiler,
            self.timing_readback,
        )?;
        report.record_gpu_timings(timestamps_supported, &gpu_elapsed_micros);
        if self.candidate_table.overflowed {
            report.finish();
            return Ok(CollisionBatchResult {
                results: Vec::new(),
                report,
            });
        }
        match self.kind {
            CollisionGpuBatchKind::PointOccupancy => {
                let [distance_dispatch, normal_dispatch] =
                    self.dispatches.try_into().map_err(|_| {
                        CollisionExecError::ExecutionUnavailable {
                            message: "point occupancy GPU batch expected exactly two dispatches"
                                .to_string(),
                        }
                    })?;
                let (distance_values, distance_observability) =
                    collect_dispatch_values_and_observability(distance_dispatch)?;
                let (normal_values, normal_observability) =
                    collect_dispatch_values_and_observability(normal_dispatch)?;
                report.record_dispatch(chunk.len());
                report.record_dispatch(chunk.len());
                report.record_gpu_runtime(&distance_observability.gpu_runtime);
                report.record_gpu_observability(&distance_observability);
                report.record_gpu_runtime(&normal_observability.gpu_runtime);
                report.record_gpu_observability(&normal_observability);
                let provenance =
                    collision_contact_normal_provenance_from_observability(&normal_observability);
                let results = materialize_point_occupancy_results(
                    chunk,
                    distance_values,
                    normal_values,
                    provenance,
                )?;
                report.finish();
                Ok(CollisionBatchResult { results, report })
            }
            CollisionGpuBatchKind::RayCast => {
                let [trace_dispatch] = self.dispatches.try_into().map_err(|_| {
                    CollisionExecError::ExecutionUnavailable {
                        message: "ray GPU batch expected exactly one dispatch".to_string(),
                    }
                })?;
                let (values, observability) =
                    collect_dispatch_values_and_observability(trace_dispatch)?;
                report.record_dispatch(chunk.len());
                report.record_gpu_runtime(&observability.gpu_runtime);
                report.record_gpu_observability(&observability);
                let provenance =
                    collision_contact_normal_provenance_from_observability(&observability);
                let results = materialize_ray_cast_results(values, provenance)?;
                report.finish();
                Ok(CollisionBatchResult { results, report })
            }
        }
    }

    fn collect_metrics_only(
        self,
        batch: &CollisionWorkloadBatch,
        chunk_len: usize,
    ) -> Result<CollisionBatchExecutionReport, CollisionExecError> {
        let native = self
            .dispatches
            .first()
            .map(|dispatch| dispatch.dispatcher.native().clone());
        let mut report = CollisionBatchExecutionReport::new(batch);
        report.record_candidate_table(&self.candidate_table);
        report.record_gpu_runtime(&GpuRuntimeMetrics {
            queue_submit_count: self.queue_submit_count,
            ..GpuRuntimeMetrics::default()
        });
        let (timestamps_supported, gpu_elapsed_micros) = collect_batch_gpu_elapsed_micros(
            native.as_deref(),
            self.profiler,
            self.timing_readback,
        )?;
        report.record_gpu_timings(timestamps_supported, &gpu_elapsed_micros);
        if self.candidate_table.overflowed {
            report.finish();
            return Ok(report);
        }
        for dispatch in self.dispatches {
            report.record_dispatch(chunk_len);
            let observability = collect_dispatch_observability_only(dispatch)?;
            let mut gpu_runtime = observability.observability.gpu_runtime.clone();
            // Metrics-only closure mode keeps observability traffic but must not
            // charge it as collision result readback.
            gpu_runtime.readback_bytes = 0;
            report.record_gpu_runtime(&gpu_runtime);
            report.record_gpu_observability(&observability.observability);
            if let Some(provenance) =
                collision_contact_normal_provenance_from_observability(&observability.observability)
            {
                let label =
                    crate::collision_contract::collision_contact_normal_provenance_name(provenance)
                        .to_string();
                match report.contact_normal_provenance.as_deref() {
                    None => report.contact_normal_provenance = Some(label),
                    Some(existing) if existing == label => {}
                    Some("mixed") => {}
                    Some(_) => report.contact_normal_provenance = Some("mixed".to_string()),
                }
            }
        }
        report.finish();
        Ok(report)
    }
}

fn should_fallback_to_compat(policy: CollisionCandidateGroupingPolicy) -> bool {
    matches!(policy, CollisionCandidateGroupingPolicy::PerItem)
}

fn batch_for_chunk(
    batch: &CollisionWorkloadBatch,
    items: Vec<CollisionBatchItem>,
) -> CollisionWorkloadBatch {
    CollisionWorkloadBatch::new(
        batch.name.clone(),
        batch.workload_id.clone(),
        batch.scenario_id.clone(),
        batch.plan.clone(),
        batch.contract_id,
        batch.snapshot_id.clone(),
        batch.capture.clone(),
        batch.domain.clone(),
        batch.candidate_grouping,
        batch.certification_policy,
        items,
        batch.chunk_size,
    )
}

fn merge_batch_report(
    target: &mut CollisionBatchExecutionReport,
    source: &CollisionBatchExecutionReport,
) {
    target.batch_count = target.batch_count.saturating_add(source.batch_count);
    target.dispatch_count = target.dispatch_count.saturating_add(source.dispatch_count);
    target.dispatch_items = target.dispatch_items.saturating_add(source.dispatch_items);
    target.timestamps_supported |= source.timestamps_supported;
    target.timestamped_pass_count = target
        .timestamped_pass_count
        .saturating_add(source.timestamped_pass_count);
    target.gpu_time_total_micros = target
        .gpu_time_total_micros
        .saturating_add(source.gpu_time_total_micros);
    target.gpu_time_max_micros = target.gpu_time_max_micros.max(source.gpu_time_max_micros);
    target.hot_path_readback_bytes = target
        .hot_path_readback_bytes
        .saturating_add(source.hot_path_readback_bytes);
    target.queue_submit_count = target
        .queue_submit_count
        .saturating_add(source.queue_submit_count);
    target.scene_reupload_bytes = target
        .scene_reupload_bytes
        .saturating_add(source.scene_reupload_bytes);
    target.wgsl_selected_workgroup_size = target
        .wgsl_selected_workgroup_size
        .max(source.wgsl_selected_workgroup_size);
    target.wgsl_resident_shared_snapshot_artifacts = target
        .wgsl_resident_shared_snapshot_artifacts
        .saturating_add(source.wgsl_resident_shared_snapshot_artifacts);
    target.cpu_certification_query_count = target
        .cpu_certification_query_count
        .saturating_add(source.cpu_certification_query_count);
    target.fallback_count = target.fallback_count.saturating_add(source.fallback_count);
    target.witness_reuse_rate += source.witness_reuse_rate * source.query_count as f64;
    target.candidate_table_overflow_fallback_count = target
        .candidate_table_overflow_fallback_count
        .saturating_add(source.candidate_table_overflow_fallback_count);
    target.total_candidate_count = target
        .total_candidate_count
        .saturating_add(source.total_candidate_count);
    target.total_rejected_candidate_count = target
        .total_rejected_candidate_count
        .saturating_add(source.total_rejected_candidate_count);
    target.total_pruned_node_count = target
        .total_pruned_node_count
        .saturating_add(source.total_pruned_node_count);
    target.total_interval_subdivisions = target
        .total_interval_subdivisions
        .saturating_add(source.total_interval_subdivisions);
    target.total_interval_refinements = target
        .total_interval_refinements
        .saturating_add(source.total_interval_refinements);
    target.total_certificate_successes = target
        .total_certificate_successes
        .saturating_add(source.total_certificate_successes);
    target.available_count_total = target
        .available_count_total
        .saturating_add(source.available_count_total);
    target.consumed_count_total = target
        .consumed_count_total
        .saturating_add(source.consumed_count_total);
    target.rejected_count_total = target
        .rejected_count_total
        .saturating_add(source.rejected_count_total);
    target.unavailable_count_total = target
        .unavailable_count_total
        .saturating_add(source.unavailable_count_total);
    if let Some(bracket) = source.last_interval_bracket {
        target.last_interval_bracket = Some(match target.last_interval_bracket {
            Some(current) => [current[0].min(bracket[0]), current[1].max(bracket[1])],
            None => bracket,
        });
    }
    match (
        target.contact_normal_provenance.as_deref(),
        source.contact_normal_provenance.as_deref(),
    ) {
        (None, Some(value)) => target.contact_normal_provenance = Some(value.to_string()),
        (Some(existing), Some(observed)) if existing == observed => {}
        (Some("mixed"), _) => {}
        (Some(_), Some(_)) => target.contact_normal_provenance = Some("mixed".to_string()),
        _ => {}
    }
}

fn collect_dispatch_values_and_observability(
    dispatch: CollisionGpuDispatchTicket,
) -> Result<(Vec<KernelValue>, QueryExecutionObservability), CollisionExecError> {
    let dispatch_result = dispatch.ticket.dispatch_result().clone();
    let (values_bytes, observability_bytes, mut gpu_runtime) = dispatch
        .ticket
        .collect_raw_readbacks()
        .map_err(|err| CollisionExecError::ExecutionUnavailable {
            message: err.to_string(),
        })?;
    gpu_runtime.upload_bytes = gpu_runtime
        .upload_bytes
        .saturating_add(dispatch.upload_bytes);
    let values = crate::query_exec::wgsl::decode_slice(
        dispatch_result.values.abi.as_ref().ok_or_else(|| {
            CollisionExecError::ExecutionUnavailable {
                message: "collision WGSL batch result is missing an ABI".to_string(),
            }
        })?,
        &values_bytes,
        dispatch_result.item_count as usize,
    )
    .map_err(|err| CollisionExecError::ExecutionUnavailable {
        message: err.to_string(),
    })?;
    Ok((
        values,
        dispatch
            .dispatcher
            .decode_observability(&observability_bytes, gpu_runtime),
    ))
}

fn collect_dispatch_observability_only(
    dispatch: CollisionGpuDispatchTicket,
) -> Result<CollisionDispatchObservability, CollisionExecError> {
    let mut observability = dispatch
        .ticket
        .collect_observability_only()
        .map_err(|err| CollisionExecError::ExecutionUnavailable {
            message: err.to_string(),
        })?;
    let (timestamps_supported, gpu_elapsed_micros) = collect_batch_gpu_elapsed_micros(
        Some(dispatch.dispatcher.native().as_ref()),
        dispatch.profiler,
        dispatch.timing_readback,
    )?;
    observability.gpu_runtime.upload_bytes = observability
        .gpu_runtime
        .upload_bytes
        .saturating_add(dispatch.upload_bytes);
    Ok(CollisionDispatchObservability {
        observability,
        timestamps_supported,
        gpu_elapsed_micros,
    })
}

fn collect_batch_gpu_elapsed_micros(
    native: Option<&crate::gpu_runtime::GpuRuntimeContext>,
    profiler: Option<CollisionGpuTimingProfiler>,
    timing_readback: Option<ReadbackTicket>,
) -> Result<(bool, Vec<u128>), CollisionExecError> {
    let Some(profiler) = profiler else {
        return Ok((false, Vec::new()));
    };
    let timestamps_supported = profiler.timestamps_supported();
    let Some(timing_readback) = timing_readback else {
        return Ok((timestamps_supported, Vec::new()));
    };
    let Some(native) = native else {
        return Ok((timestamps_supported, Vec::new()));
    };
    let timing_bytes = collect_storage_buffer_readback(native, timing_readback)
        .map_err(|err| CollisionExecError::ExecutionUnavailable {
            message: format!("collision GPU timing readback failed: {err}"),
        })?
        .bytes;
    Ok((
        timestamps_supported,
        profiler.decode_elapsed_micros(&timing_bytes),
    ))
}

fn materialize_point_occupancy_results(
    chunk: &[CollisionBatchItem],
    distance_values: Vec<KernelValue>,
    normal_values: Vec<KernelValue>,
    provenance: Option<CollisionContactNormalProvenance>,
) -> Result<Vec<CollisionResult>, CollisionExecError> {
    let mut results = Vec::with_capacity(chunk.len());
    for ((item, distance), normal) in chunk
        .iter()
        .zip(distance_values.into_iter())
        .zip(normal_values.into_iter())
    {
        let point = match item {
            CollisionBatchItem::PointOccupancy { point } => *point,
            other => {
                return Err(CollisionExecError::ExecutionUnavailable {
                    message: format!("point occupancy batch expected point items, found {other:?}"),
                });
            }
        };
        let signed_distance = expect_f32_value(&extract_distance_value(distance)?)?;
        let world_normal = expect_vec3_value(&extract_normal_value(normal)?)?;
        results.push(CollisionResult::Occupancy(CollisionOccupancyResult {
            classification: classify_occupancy(signed_distance),
            occupied: signed_distance <= 0.0,
            signed_distance,
            witness: CollisionPointWitness {
                sample_point: point,
                nearest_point_on_world: offset_point(point, world_normal, -signed_distance),
                world_normal,
                signed_distance,
                normal_provenance: provenance
                    .unwrap_or(CollisionContactNormalProvenance::HeuristicShadingNormal),
            },
        }));
    }
    Ok(results)
}

fn materialize_ray_cast_results(
    values: Vec<KernelValue>,
    provenance: Option<CollisionContactNormalProvenance>,
) -> Result<Vec<CollisionResult>, CollisionExecError> {
    let mut results = Vec::with_capacity(values.len());
    for value in values {
        let hit = expect_struct_value(&value, "Hit3")?;
        let hit_flag = expect_bool_value(field_value(hit, "hit")?)?;
        if hit_flag {
            results.push(CollisionResult::RayCast(CollisionRayCastResult {
                hit: true,
                miss_reason: CollisionRayMissReason::None,
                witness: Some(CollisionRayWitness {
                    travel_distance: expect_f32_value(field_value(hit, "distance")?)?,
                    position: expect_vec3_value(field_value(hit, "position")?)?,
                    normal: expect_vec3_value(field_value(hit, "normal")?)?,
                    root_shape_id: expect_u32_value(field_value(hit, "root_shape_id")?)?,
                    feature_id: expect_u32_value(field_value(hit, "feature_id")?)?,
                    normal_provenance: provenance
                        .unwrap_or(CollisionContactNormalProvenance::HeuristicShadingNormal),
                }),
            }));
        } else {
            results.push(CollisionResult::RayCast(CollisionRayCastResult {
                hit: false,
                miss_reason: CollisionRayMissReason::NoHitWithinRange,
                witness: None,
            }));
        }
    }
    Ok(results)
}

fn collision_contact_normal_provenance_from_observability(
    observability: &QueryExecutionObservability,
) -> Option<CollisionContactNormalProvenance> {
    match observability.normal_role.as_deref() {
        Some("normal_role::certified_field_gradient") => {
            Some(CollisionContactNormalProvenance::CertifiedFieldGradient)
        }
        Some("normal_role::feature_normal") => {
            Some(CollisionContactNormalProvenance::FeatureNormal)
        }
        Some("normal_role::heuristic_shading_normal") => {
            Some(CollisionContactNormalProvenance::HeuristicShadingNormal)
        }
        _ => None,
    }
}

fn classify_occupancy(signed_distance: f32) -> CollisionOccupancyClass {
    if signed_distance < 0.0 {
        CollisionOccupancyClass::Occupied
    } else if signed_distance == 0.0 {
        CollisionOccupancyClass::Boundary
    } else {
        CollisionOccupancyClass::Empty
    }
}

fn offset_point(point: [f32; 3], normal: [f32; 3], signed_distance: f32) -> [f32; 3] {
    [
        point[0] + normal[0] * signed_distance,
        point[1] + normal[1] * signed_distance,
        point[2] + normal[2] * signed_distance,
    ]
}

fn lerp_point(start: [f32; 3], end: [f32; 3], fraction: f32) -> [f32; 3] {
    [
        start[0] + (end[0] - start[0]) * fraction,
        start[1] + (end[1] - start[1]) * fraction,
        start[2] + (end[2] - start[2]) * fraction,
    ]
}

fn field_value<'a>(
    value: &'a crate::kernel::KernelStructValue,
    name: &str,
) -> Result<&'a KernelValue, CollisionExecError> {
    value
        .fields
        .iter()
        .find(|(field_name, _)| field_name.as_str() == name)
        .map(|(_, value)| value)
        .ok_or_else(|| CollisionExecError::ExecutionUnavailable {
            message: format!("collision WGSL result is missing field '{name}'"),
        })
}

fn expect_struct_value<'a>(
    value: &'a KernelValue,
    expected: &str,
) -> Result<&'a crate::kernel::KernelStructValue, CollisionExecError> {
    match value {
        KernelValue::Struct(struct_value) if struct_value.name.as_str() == expected => {
            Ok(struct_value)
        }
        other => Err(CollisionExecError::ExecutionUnavailable {
            message: format!("expected {expected}, found {other:?}"),
        }),
    }
}

fn expect_bool_value(value: &KernelValue) -> Result<bool, CollisionExecError> {
    match value {
        KernelValue::Bool(value) => Ok(*value),
        other => Err(CollisionExecError::ExecutionUnavailable {
            message: format!("expected Bool, found {other:?}"),
        }),
    }
}

fn expect_f32_value(value: &KernelValue) -> Result<f32, CollisionExecError> {
    match value {
        KernelValue::F32(value) => Ok(*value),
        other => Err(CollisionExecError::ExecutionUnavailable {
            message: format!("expected F32, found {other:?}"),
        }),
    }
}

fn expect_vec3_value(value: &KernelValue) -> Result<[f32; 3], CollisionExecError> {
    match value {
        KernelValue::Vec3(value) => Ok(*value),
        other => Err(CollisionExecError::ExecutionUnavailable {
            message: format!("expected Vec3, found {other:?}"),
        }),
    }
}

fn expect_u32_value(value: &KernelValue) -> Result<u32, CollisionExecError> {
    match value {
        KernelValue::U32(value) => Ok(*value),
        other => Err(CollisionExecError::ExecutionUnavailable {
            message: format!("expected U32, found {other:?}"),
        }),
    }
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
        collect_dispatch_observability_only, collision_gpu_dispatch_cache_key,
        execute_batch_gpu_metrics_only, point_query_value, prepare_batched_point_distance_dispatch,
        prepare_batched_point_normal_dispatch, prepare_batched_ray_trace_dispatch,
        prepare_point_occupancy_metrics_ticket, ray_query_value,
    };
    use crate::collision_contract::CollisionRayInput;
    use crate::collision_exec::{
        CollisionCandidateGroupingPolicy, CollisionCertificationPolicy, CollisionWorkloadBatch,
    };
    use crate::collision_plan::{CollisionPlan, CollisionQueryKind};
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
    use crate::query_exec::wgsl::{
        override_gpu_timestamps_for_current_thread, readback_storage_buffer_on,
    };
    use crate::query_plan::BatchQueryKind;
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
    fn collision_gpu_timing_mode_prefers_encoder_then_pass_fallback() {
        assert_eq!(
            super::select_collision_gpu_timing_mode(true, true),
            super::CollisionGpuTimingMode::Encoder
        );
        assert_eq!(
            super::select_collision_gpu_timing_mode(true, false),
            super::CollisionGpuTimingMode::Encoder
        );
        assert_eq!(
            super::select_collision_gpu_timing_mode(false, true),
            super::CollisionGpuTimingMode::Pass
        );
        assert_eq!(
            super::select_collision_gpu_timing_mode(false, false),
            super::CollisionGpuTimingMode::Pass
        );
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

    #[test]
    fn metrics_only_collision_dispatch_skips_observability_readback_and_keeps_static_summary() {
        let ctx = typed_query_module(fixture_source());
        let scene_id = stable_region_scene_capture_id(&SmolStr::new("collision_region"));
        let batch = CollisionWorkloadBatch::new(
            "point occupancy metrics batch",
            "collision_perf_point_occupancy_batch",
            "collision_perf_point_occupancy_batch",
            CollisionPlan::for_query_with_backend(
                CollisionQueryKind::PointOccupancyWorld,
                crate::query_contract::DispatchBackend::Wgsl,
            ),
            crate::collision_contract::COLLISION_POINT_OCCUPANCY_WORLD,
            "snapshot:collision:point_metrics",
            region_capture(scene_id, 1),
            scene_domain(scene_id),
            CollisionCandidateGroupingPolicy::SharedCandidateDigest,
            CollisionCertificationPolicy::MetricsOnly,
            vec![
                crate::collision_exec::CollisionBatchItem::PointOccupancy {
                    point: [0.0, 0.0, 0.25],
                },
                crate::collision_exec::CollisionBatchItem::PointOccupancy {
                    point: [0.2, 0.0, 0.25],
                },
            ],
            2,
        )
        .checked()
        .expect("valid point occupancy metrics batch");

        let helper_ticket = prepare_point_occupancy_metrics_ticket(&batch, &ctx, &batch.items)
            .expect("helper metrics ticket");
        assert_eq!(helper_ticket.queue_submit_count, 1);
        let helper_observabilities = helper_ticket
            .dispatches
            .into_iter()
            .map(collect_dispatch_observability_only)
            .collect::<Result<Vec<_>, _>>()
            .expect("helper observability");

        let direct_ticket = prepare_point_occupancy_metrics_ticket(&batch, &ctx, &batch.items)
            .expect("direct metrics ticket");
        let direct_observabilities = direct_ticket
            .dispatches
            .into_iter()
            .map(|direct_dispatch| {
                let selected_workgroup_size = direct_dispatch.dispatcher.selected_workgroup_size();
                let expected = direct_dispatch
                    .ticket
                    .collect_observability_only()
                    .expect("direct observability");
                (selected_workgroup_size, expected)
            })
            .collect::<Vec<_>>();

        assert_eq!(helper_observabilities.len(), 2);
        assert_eq!(direct_observabilities.len(), 2);
        for (helper_observability, (selected_workgroup_size, direct_observability)) in
            helper_observabilities
                .iter()
                .zip(direct_observabilities.iter())
        {
            assert_eq!(direct_observability.gpu_runtime.readback_bytes, 0);
            assert_eq!(direct_observability.field_samples, 0);
            assert_eq!(
                direct_observability.wgsl_selected_workgroup_size,
                *selected_workgroup_size
            );
            assert!(
                direct_observability.cache_resident_shared_snapshot_artifacts > 0,
                "metrics-only observability should keep the resident cache seed",
            );
            assert_eq!(
                helper_observability
                    .observability
                    .gpu_runtime
                    .readback_bytes,
                0
            );
            assert_eq!(
                helper_observability.observability.field_samples,
                direct_observability.field_samples
            );
            assert_eq!(
                helper_observability
                    .observability
                    .wgsl_selected_workgroup_size,
                direct_observability.wgsl_selected_workgroup_size
            );
            assert_eq!(
                helper_observability
                    .observability
                    .cache_resident_shared_snapshot_artifacts,
                direct_observability.cache_resident_shared_snapshot_artifacts
            );
        }
    }

    #[test]
    fn metrics_only_collision_batch_reports_gpu_timestamps_when_supported() {
        let ctx = typed_query_module(fixture_source());
        let scene_id = stable_region_scene_capture_id(&SmolStr::new("collision_region"));
        let batch = CollisionWorkloadBatch::new(
            "point occupancy metrics batch",
            "collision_perf_point_occupancy_batch",
            "collision_perf_point_occupancy_batch",
            CollisionPlan::for_query_with_backend(
                CollisionQueryKind::PointOccupancyWorld,
                crate::query_contract::DispatchBackend::Wgsl,
            ),
            crate::collision_contract::COLLISION_POINT_OCCUPANCY_WORLD,
            "snapshot:collision:point_metrics",
            region_capture(scene_id, 1),
            scene_domain(scene_id),
            CollisionCandidateGroupingPolicy::SharedCandidateDigest,
            CollisionCertificationPolicy::MetricsOnly,
            vec![
                crate::collision_exec::CollisionBatchItem::PointOccupancy {
                    point: [0.0, 0.0, 0.25],
                },
                crate::collision_exec::CollisionBatchItem::PointOccupancy {
                    point: [0.2, 0.0, 0.25],
                },
            ],
            2,
        )
        .checked()
        .expect("valid point occupancy metrics batch");

        let report = execute_batch_gpu_metrics_only(&batch, &ctx).expect("metrics-only batch");

        if report.timestamps_supported {
            assert_eq!(report.timestamped_pass_count, 2);
            assert!(report.gpu_time_total_micros >= report.gpu_time_max_micros);
        } else {
            assert_eq!(report.timestamped_pass_count, 0);
            assert_eq!(report.gpu_time_total_micros, 0);
            assert_eq!(report.gpu_time_max_micros, 0);
        }
    }

    #[test]
    fn collision_dispatch_cache_key_tracks_timestamp_opt_in() {
        let ctx = typed_query_module(fixture_source());
        let scene_id = stable_region_scene_capture_id(&SmolStr::new("collision_region"));
        let capture = region_capture(scene_id, 1);
        let domain = scene_domain(scene_id);
        let items = vec![point_query_value([0.0, 0.0, 0.25])];

        let timestamps_disabled = {
            let _override = override_gpu_timestamps_for_current_thread(Some(false));
            collision_gpu_dispatch_cache_key(
                &ctx,
                None,
                BatchQueryKind::Distance,
                &capture,
                &domain,
                &items,
                &[],
            )
        };
        let timestamps_enabled = {
            let _override = override_gpu_timestamps_for_current_thread(Some(true));
            collision_gpu_dispatch_cache_key(
                &ctx,
                None,
                BatchQueryKind::Distance,
                &capture,
                &domain,
                &items,
                &[],
            )
        };

        assert!(!timestamps_disabled.timestamps_requested);
        assert!(timestamps_enabled.timestamps_requested);
        assert_ne!(timestamps_disabled, timestamps_enabled);
    }
}
