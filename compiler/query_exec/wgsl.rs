pub(crate) mod codegen;

use self::codegen::{ShaderPlan, generate_shader};
use crate::acceleration::cache::SupportBrickCache;
use crate::acceleration::{AccelerationForest, BoundDescriptorKind};
use crate::execution_policy::QueryExecutionPolicy;
use crate::gpu_runtime::{
    ComputePipelineKey, GPU_RUNTIME_BIND_GROUP_COUNT, GPU_RUNTIME_FRAME_BIND_GROUP_INDEX,
    GPU_RUNTIME_PASS_BIND_GROUP_INDEX, GPU_RUNTIME_SCENE_BIND_GROUP_INDEX,
    GPU_RUNTIME_SCRATCH_BIND_GROUP_INDEX, GpuBindGroupRole, GpuLayoutIdentity, GpuLimitRequest,
    GpuPassProfiler, GpuResidentScene, GpuResidentSceneKey, GpuRuntimeContext, GpuRuntimeMetrics,
    PipelineLayoutKey, bind_group_layout_signature_for_role, lock_shared_upload_arena,
    readback_storage_buffer_on as shared_readback_storage_buffer_on,
    shared_resident_scene_cache_for_request, shared_wgpu_context,
};
use crate::kernel::KernelBatchQueryTrace;
use crate::kernel::ir::{KernelBatchQueryPlan, KernelCaptureQueryPlan, KernelWorldQueryPlan};
use crate::kernel::{
    KernelStructValue, KernelValidationError, KernelValue, validate_batch_query_plan,
    validate_capture_query_plan, validate_world_query_plan,
};
use crate::portable::{
    PortableAbiType, portable_abi_array_stride, portable_abi_decode_slice,
    portable_abi_encode_slice, portable_abi_encode_value, portable_abi_layout,
};
use crate::query_contract::{
    QueryCardinality, QueryContractDescriptor, QueryItemKind, QueryResultKind, QueryTargetKind,
    SceneDomainFlag, query_contract, scene_domain_flag_name,
};
use crate::query_exec::cpu::{DirectQueryOps, QueryExecError};
use crate::query_exec::ids::stable_semantic_id;
use crate::query_exec::world::{NormalRole, world_query_semantics_for_contract};
use crate::query_exec::{QueryExecutionObservability, select_query_wgsl_workgroup_size};
use crate::query_plan::{
    BatchQueryKind, CaptureKind, WorldQueryKind, batch_query_kind_for_contract_id,
    world_query_kind_for_contract_id,
};
use crate::world_identity::SnapshotIdentityReport;
use naga::valid::{Capabilities, ValidationFlags, Validator};
use smol_str::SmolStr;
use std::borrow::Cow;
use std::cell::Cell;
use std::collections::HashMap;
use std::env;
use std::sync::{Arc, Mutex, OnceLock};
use wgpu::util::DeviceExt;

const QUERY_WGSL_STORAGE_BUFFERS_PER_SHADER_STAGE: u32 = 10;
const QUERY_WGSL_ACCEL_FLAG_LEAF: u32 = 1;
const QUERY_WGSL_ACCEL_FLAG_HAS_BOUNDS: u32 = 2;
const QUERY_WGSL_OBSERVABILITY_U32S: usize = 19;
pub const QUERY_GPU_TIMESTAMPS_ENV: &str = "WRELA_QUERY_GPU_TIMESTAMPS";

pub(crate) type NativeWgpuContext = GpuRuntimeContext;

thread_local! {
    static SHADER_F16_OVERRIDE: Cell<Option<bool>> = const { Cell::new(None) };
    static TIMESTAMP_QUERY_OVERRIDE: Cell<Option<bool>> = const { Cell::new(None) };
}

pub struct ShaderF16OverrideGuard {
    previous: Option<bool>,
}

impl Drop for ShaderF16OverrideGuard {
    fn drop(&mut self) {
        SHADER_F16_OVERRIDE.with(|cell| cell.set(self.previous));
    }
}

pub fn override_shader_f16_for_current_thread(enabled: Option<bool>) -> ShaderF16OverrideGuard {
    let previous = SHADER_F16_OVERRIDE.with(|cell| {
        let previous = cell.get();
        cell.set(enabled);
        previous
    });
    ShaderF16OverrideGuard { previous }
}

pub struct TimestampQueryOverrideGuard {
    previous: Option<bool>,
}

impl Drop for TimestampQueryOverrideGuard {
    fn drop(&mut self) {
        TIMESTAMP_QUERY_OVERRIDE.with(|cell| cell.set(self.previous));
    }
}

pub fn override_gpu_timestamps_for_current_thread(
    enabled: Option<bool>,
) -> TimestampQueryOverrideGuard {
    let previous = TIMESTAMP_QUERY_OVERRIDE.with(|cell| {
        let previous = cell.get();
        cell.set(enabled);
        previous
    });
    TimestampQueryOverrideGuard { previous }
}

pub(crate) fn gpu_timestamps_requested_for_current_thread() -> bool {
    requested_timestamp_query_feature()
}

pub fn clear_native_wgsl_test_caches() {
    use crate::gpu_runtime::clear_shared_resident_scene_caches_for_type;

    clear_shared_resident_scene_caches_for_type::<WgslResidentScenePayload>();
    generated_shader_modules_cache()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .clear();
    scene_bind_group_cache()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .clear();
    pooled_storage_buffer_cache()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .clear();
    dynamic_resources_cache()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .clear();
    capture_pipeline_cache()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .clear();
    query_pipeline_cache()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .clear();
}

#[cfg(test)]
pub(crate) fn native_wgsl_test_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
}

#[derive(Debug, Clone)]
pub(crate) struct GpuDispatchRequest {
    pub(crate) dispatch: KernelValue,
    pub(crate) items: Vec<KernelValue>,
    pub(crate) world_shape_indices: Vec<u32>,
    pub(crate) accel_nodes: Vec<KernelValue>,
    pub(crate) accel_children: Vec<u32>,
    pub(crate) cache_bricks: Vec<KernelValue>,
    pub(crate) continuation_seeds: Vec<u32>,
    pub(crate) candidate_spans: Vec<u32>,
    pub(crate) resident_scene_snapshot: Option<SnapshotIdentityReport>,
    pub(crate) resident_scene_detail: i32,
    pub(crate) resident_scene_selection_signature: u64,
}

#[derive(Debug, Clone)]
pub(crate) struct GeneratedShaderModule {
    pub(crate) source: String,
    pub(crate) workgroup_size: u32,
    pub(crate) dispatch_abi: PortableAbiType,
    pub(crate) accel_node_abi: PortableAbiType,
    pub(crate) cache_brick_abi: PortableAbiType,
    pub(crate) shape_meta_abi: PortableAbiType,
    pub(crate) item_abi: PortableAbiType,
    pub(crate) result_abi: PortableAbiType,
    pub(crate) shape_meta_values: Vec<KernelValue>,
    pub(crate) cache_observability_seed: codegen::CacheObservabilitySeed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct GeneratedShaderCacheKey {
    context_id: u64,
    plan_signature: u64,
    f16_enabled: bool,
}

static GENERATED_SHADER_MODULES: OnceLock<
    Mutex<HashMap<GeneratedShaderCacheKey, GeneratedShaderModule>>,
> = OnceLock::new();

fn generated_shader_modules_cache()
-> &'static Mutex<HashMap<GeneratedShaderCacheKey, GeneratedShaderModule>> {
    GENERATED_SHADER_MODULES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn scene_bind_group_cache() -> &'static Mutex<HashMap<WgslSceneBindGroupKey, wgpu::BindGroup>> {
    static SCENE_BIND_GROUPS: OnceLock<Mutex<HashMap<WgslSceneBindGroupKey, wgpu::BindGroup>>> =
        OnceLock::new();
    SCENE_BIND_GROUPS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn pooled_storage_buffer_cache() -> &'static Mutex<HashMap<WgslBufferPoolKey, wgpu::Buffer>> {
    static BUFFERS: OnceLock<Mutex<HashMap<WgslBufferPoolKey, wgpu::Buffer>>> = OnceLock::new();
    BUFFERS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn dynamic_resources_cache()
-> &'static Mutex<HashMap<WgslDynamicResourcesKey, &'static Mutex<WgslDynamicResources>>> {
    static RESOURCES: OnceLock<
        Mutex<HashMap<WgslDynamicResourcesKey, &'static Mutex<WgslDynamicResources>>>,
    > = OnceLock::new();
    RESOURCES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn capture_pipeline_cache() -> &'static Mutex<HashMap<WgslPipelineCacheKey, CachedPipeline>> {
    static PIPELINES: OnceLock<Mutex<HashMap<WgslPipelineCacheKey, CachedPipeline>>> =
        OnceLock::new();
    PIPELINES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn query_pipeline_cache() -> &'static Mutex<HashMap<WgslPipelineCacheKey, QueryCachedPipeline>> {
    static PIPELINES: OnceLock<Mutex<HashMap<WgslPipelineCacheKey, QueryCachedPipeline>>> =
        OnceLock::new();
    PIPELINES.get_or_init(|| Mutex::new(HashMap::new()))
}

#[derive(Debug, Clone)]
pub(crate) struct NativeWgslBridgeConfig {
    pub(crate) source: SmolStr,
    pub(crate) workgroup_size: i64,
}

#[derive(Clone)]
pub(crate) struct ResidentBatchQuerySession {
    pub(crate) native: Arc<NativeWgpuContext>,
    dynamic_resources: &'static Mutex<WgslDynamicResources>,
    pub(crate) pipeline: wgpu::ComputePipeline,
    pub(crate) scene_bind_group: wgpu::BindGroup,
    pub(crate) frame_bind_group: wgpu::BindGroup,
    pub(crate) pass_bind_group: wgpu::BindGroup,
    pub(crate) scratch_bind_group: wgpu::BindGroup,
    pub(crate) dispatch_buffer: wgpu::Buffer,
    pub(crate) input_buffer: wgpu::Buffer,
    pub(crate) input_buffer_size: u64,
    pub(crate) output_buffer: wgpu::Buffer,
    pub(crate) observability_buffer: wgpu::Buffer,
    pub(crate) continuation_seed_buffer: wgpu::Buffer,
    pub(crate) continuation_seed_buffer_size: u64,
    pub(crate) result_abi: PortableAbiType,
    pub(crate) item_count: u32,
    pub(crate) output_buffer_size: u64,
    pub(crate) observability_buffer_size: u64,
    pub(crate) layout_signature: u64,
    diagnostics: WgslDispatchDiagnostics,
    initial_gpu_runtime: GpuRuntimeMetrics,
}

impl ResidentBatchQuerySession {
    pub(crate) fn selected_workgroup_size(&self) -> u32 {
        self.diagnostics.selected_workgroup_size
    }

    pub(crate) fn initial_gpu_runtime(&self) -> GpuRuntimeMetrics {
        self.initial_gpu_runtime.clone()
    }

    pub(crate) fn summary_observability_without_readback(
        &self,
        gpu_runtime: GpuRuntimeMetrics,
    ) -> QueryExecutionObservability {
        summary_wgsl_observability(
            &self.diagnostics,
            self.item_count,
            self.layout_signature,
            gpu_runtime,
        )
    }

    pub(crate) fn initialize_dispatch_state(
        &self,
        dispatch: &KernelValue,
    ) -> Result<u64, QueryExecError> {
        self.initialize_dispatch_state_with_inputs(dispatch, None, None)
    }

    pub(crate) fn initialize_dispatch_state_with_inputs(
        &self,
        dispatch: &KernelValue,
        input_bytes: Option<&[u8]>,
        side_channel_bytes: Option<&[u8]>,
    ) -> Result<u64, QueryExecError> {
        let dispatch_bytes = encode_value(&codegen::wgsl_dispatch_config_abi(), dispatch)?;
        let observability_bytes = [0u8; QUERY_WGSL_OBSERVABILITY_U32S * std::mem::size_of::<u32>()];
        let side_channel_bytes = side_channel_bytes.unwrap_or(&[0u8; std::mem::size_of::<u32>()]);
        let mut upload_bytes = 0u64;
        let mut dynamic_resources = self
            .dynamic_resources
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let dispatch_fingerprint = stable_semantic_id(&[&dispatch_bytes]);
        if dynamic_resources.last_dispatch_fingerprint != Some(dispatch_fingerprint) {
            self.native
                .queue
                .write_buffer(&self.dispatch_buffer, 0, &dispatch_bytes);
            dynamic_resources.last_dispatch_fingerprint = Some(dispatch_fingerprint);
            upload_bytes = upload_bytes.saturating_add(storage_buffer_size(&dispatch_bytes));
        }
        if let Some(input_bytes) = input_bytes
            && !input_bytes.is_empty()
        {
            let input_fingerprint = stable_semantic_id(&[input_bytes]);
            if dynamic_resources.last_input_fingerprint != Some(input_fingerprint) {
                self.native
                    .queue
                    .write_buffer(&self.input_buffer, 0, input_bytes);
                dynamic_resources.last_input_fingerprint = Some(input_fingerprint);
                upload_bytes = upload_bytes.saturating_add(storage_buffer_size(input_bytes));
            }
        }
        self.native
            .queue
            .write_buffer(&self.observability_buffer, 0, &observability_bytes);
        upload_bytes = upload_bytes.saturating_add(observability_bytes.len() as u64);
        let continuation_fingerprint = stable_semantic_id(&[side_channel_bytes]);
        if dynamic_resources.last_continuation_seed_fingerprint != Some(continuation_fingerprint) {
            self.native
                .queue
                .write_buffer(&self.continuation_seed_buffer, 0, side_channel_bytes);
            dynamic_resources.last_continuation_seed_fingerprint = Some(continuation_fingerprint);
            upload_bytes = upload_bytes.saturating_add(storage_buffer_size(side_channel_bytes));
        }
        Ok(upload_bytes)
    }

    pub(crate) fn encode_compute_pass(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        profiler: &mut GpuPassProfiler,
    ) {
        let timestamp_writes = profiler.compute_pass_timestamp_writes();
        self.encode_compute_pass_with_timestamp_writes(encoder, timestamp_writes);
    }

    pub(crate) fn encode_compute_pass_without_timestamps(
        &self,
        encoder: &mut wgpu::CommandEncoder,
    ) {
        self.encode_compute_pass_with_timestamp_writes(encoder, None);
    }

    fn encode_compute_pass_with_timestamp_writes(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        timestamp_writes: Option<wgpu::ComputePassTimestampWrites<'_>>,
    ) {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("wrela.wgsl.compute_pass"),
            timestamp_writes,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(
            GPU_RUNTIME_SCENE_BIND_GROUP_INDEX,
            &self.scene_bind_group,
            &[],
        );
        pass.set_bind_group(
            GPU_RUNTIME_FRAME_BIND_GROUP_INDEX,
            &self.frame_bind_group,
            &[],
        );
        pass.set_bind_group(
            GPU_RUNTIME_PASS_BIND_GROUP_INDEX,
            &self.pass_bind_group,
            &[],
        );
        pass.set_bind_group(
            GPU_RUNTIME_SCRATCH_BIND_GROUP_INDEX,
            &self.scratch_bind_group,
            &[],
        );
        pass.dispatch_workgroups(
            dispatch_workgroups_x_for_items(
                self.item_count,
                self.diagnostics.selected_workgroup_size,
            ),
            1,
            1,
        );
    }

    pub(crate) fn decode_observability(
        &self,
        bytes: &[u8],
        gpu_runtime: GpuRuntimeMetrics,
    ) -> QueryExecutionObservability {
        decode_wgsl_observability(
            &self.diagnostics,
            bytes,
            self.item_count,
            self.layout_signature,
            gpu_runtime,
        )
    }
}

#[derive(Clone)]
pub(crate) struct CachedPipeline {
    pub(crate) bind_group_layout: wgpu::BindGroupLayout,
    pub(crate) pipeline: wgpu::ComputePipeline,
}

#[derive(Clone)]
pub(crate) struct QueryCachedPipeline {
    pub(crate) bind_group_layouts: [wgpu::BindGroupLayout; GPU_RUNTIME_BIND_GROUP_COUNT as usize],
    pub(crate) layout_identity: GpuLayoutIdentity,
    pub(crate) pipeline: wgpu::ComputePipeline,
}

type WgslLimitRequest = GpuLimitRequest;

#[derive(Debug, Clone, Copy)]
struct WgslDispatchDiagnostics {
    selected_workgroup_size: u32,
    used_max_storage_buffer_bytes: u64,
    requested_max_storage_buffer_bytes: u64,
    cache_observability_seed: codegen::CacheObservabilitySeed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct WgslDispatchChunkPlan {
    items_per_chunk: usize,
    chunk_count: usize,
}

#[cfg(test)]
thread_local! {
    static TEST_WGSL_CHUNK_STORAGE_BUFFER_LIMIT_OVERRIDE: Cell<Option<u64>> = const { Cell::new(None) };
}

#[derive(Debug, Clone, Copy)]
struct WgslAccelNodeRecord {
    min: [f32; 3],
    max: [f32; 3],
    child_start: u32,
    child_len: u32,
    leaf_shape_index: u32,
    flags: u32,
}

#[derive(Debug, Clone)]
struct WgslAccelerationForestData {
    root_index: u32,
    nodes: Vec<WgslAccelNodeRecord>,
    children: Vec<u32>,
}

pub(crate) fn execute_capture_query_with_observability(
    ctx: &crate::query_exec::context::QueryExecContext,
    plan: &KernelCaptureQueryPlan,
    args: &[KernelValue],
) -> Result<(KernelValue, QueryExecutionObservability), QueryExecError> {
    execute_capture_query_with_snapshot_observability(ctx, None, plan, args)
}

pub(crate) fn execute_capture_query_with_snapshot_observability(
    ctx: &crate::query_exec::context::QueryExecContext,
    snapshot: Option<&crate::world_identity::WorldSnapshotHandle>,
    plan: &KernelCaptureQueryPlan,
    args: &[KernelValue],
) -> Result<(KernelValue, QueryExecutionObservability), QueryExecError> {
    let ops = DirectQueryOps::new_with_snapshot(ctx, snapshot);
    if let Err(errors) = validate_capture_query_plan(plan) {
        ops.note_contract_validation_failure();
        return Err(validation_error("capture query", errors));
    }
    let request = build_capture_request(&ops, plan, args)?;
    let generated = generate_compiled_shader(ctx, ShaderPlan::Capture(plan))?;
    let (mut values, wgsl_observability) =
        dispatch_compiled_shader_with_observability(&generated, request)?;
    let descriptor = descriptor_for_plan(plan.contract_id)?;
    note_contract_observability(&ops);
    note_result_observability(&ops, descriptor, &values);
    let value = values.pop().ok_or_else(|| QueryExecError::Unsupported {
        message: "native WGSL backend produced no capture result".to_string(),
    })?;
    let mut observability = ops.snapshot_observability();
    observability.merge_from(&wgsl_observability);
    Ok((value, observability))
}

pub(crate) fn execute_world_query_with_observability(
    ctx: &crate::query_exec::context::QueryExecContext,
    plan: &KernelWorldQueryPlan,
    args: &[KernelValue],
) -> Result<(KernelValue, QueryExecutionObservability), QueryExecError> {
    let policy = QueryExecutionPolicy::conservative(plan.backend, None);
    execute_world_query_with_policy_with_observability(ctx, &policy, plan, args)
}

pub(crate) fn execute_world_query_with_policy_with_observability(
    ctx: &crate::query_exec::context::QueryExecContext,
    policy: &QueryExecutionPolicy,
    plan: &KernelWorldQueryPlan,
    args: &[KernelValue],
) -> Result<(KernelValue, QueryExecutionObservability), QueryExecError> {
    execute_world_query_with_policy_with_snapshot_observability(ctx, None, policy, plan, args)
}

pub(crate) fn execute_world_query_with_snapshot_observability(
    ctx: &crate::query_exec::context::QueryExecContext,
    snapshot: Option<&crate::world_identity::WorldSnapshotHandle>,
    plan: &KernelWorldQueryPlan,
    args: &[KernelValue],
) -> Result<(KernelValue, QueryExecutionObservability), QueryExecError> {
    let policy = QueryExecutionPolicy::conservative(plan.backend, None);
    execute_world_query_with_policy_with_snapshot_observability(ctx, snapshot, &policy, plan, args)
}

pub(crate) fn execute_world_query_with_policy_with_snapshot_observability(
    ctx: &crate::query_exec::context::QueryExecContext,
    snapshot: Option<&crate::world_identity::WorldSnapshotHandle>,
    _policy: &QueryExecutionPolicy,
    plan: &KernelWorldQueryPlan,
    args: &[KernelValue],
) -> Result<(KernelValue, QueryExecutionObservability), QueryExecError> {
    let ops = DirectQueryOps::new_with_snapshot(ctx, snapshot);
    if let Some(ray_solver) = &plan.ray_solver {
        ops.note_solver_plan(ray_solver);
    }
    if let Err(errors) = validate_world_query_plan(plan) {
        ops.note_contract_validation_failure();
        return Err(validation_error("world query", errors));
    }
    let request = build_world_request(&ops, plan, args)?;
    let helper_request = request.clone();
    let generated = generate_compiled_shader(ctx, ShaderPlan::World(plan))?;
    let (mut values, wgsl_observability) =
        dispatch_compiled_shader_with_observability(&generated, request)?;
    let descriptor = descriptor_for_plan(plan.contract_id)?;
    note_contract_observability(&ops);
    note_result_observability(&ops, descriptor, &values);
    let value = values.pop().ok_or_else(|| QueryExecError::Unsupported {
        message: "native WGSL backend produced no world result".to_string(),
    })?;
    let mut observability = ops.snapshot_observability();
    observability.merge_from(&wgsl_observability);
    annotate_wgsl_world_helper_path_for_world(
        &mut observability,
        plan.contract_id,
        &helper_request,
    );
    Ok((value, observability))
}

pub(crate) fn execute_batch_query_with_observability(
    ctx: &crate::query_exec::context::QueryExecContext,
    plan: &KernelBatchQueryPlan,
    args: &[KernelValue],
    _trace: &KernelBatchQueryTrace,
) -> Result<(KernelValue, QueryExecutionObservability), QueryExecError> {
    execute_batch_query_with_snapshot_observability(ctx, None, plan, args, _trace)
}

pub(crate) fn execute_batch_query_with_snapshot_observability(
    ctx: &crate::query_exec::context::QueryExecContext,
    snapshot: Option<&crate::world_identity::WorldSnapshotHandle>,
    plan: &KernelBatchQueryPlan,
    args: &[KernelValue],
    _trace: &KernelBatchQueryTrace,
) -> Result<(KernelValue, QueryExecutionObservability), QueryExecError> {
    let ops = DirectQueryOps::new_with_snapshot(ctx, snapshot);
    if let Some(ray_solver) = &plan.ray_solver {
        ops.note_solver_plan(ray_solver);
    }
    if let Err(errors) = validate_batch_query_plan(plan) {
        ops.note_contract_validation_failure();
        return Err(validation_error("batch query", errors));
    }
    let generated = generate_compiled_shader(ctx, ShaderPlan::Batch(plan))?;
    let request = build_batch_request(&ops, plan, args)?;
    let helper_request = request.clone();
    if descriptor_for_plan(plan.contract_id)?.target == QueryTargetKind::World {
        ops.note_world_batch_items(request.items.len() as u32);
    }
    let (values, wgsl_observability) =
        dispatch_compiled_shader_with_observability(&generated, request)?;
    let descriptor = descriptor_for_plan(plan.contract_id)?;
    note_contract_observability(&ops);
    note_result_observability(&ops, descriptor, &values);
    let mut observability = ops.snapshot_observability();
    observability.merge_from(&wgsl_observability);
    annotate_wgsl_world_helper_path_for_batch(
        &mut observability,
        plan.contract_id,
        &helper_request,
    );
    Ok((KernelValue::Array(values), observability))
}

fn annotate_wgsl_world_helper_path_for_world(
    observability: &mut QueryExecutionObservability,
    contract_id: crate::query_contract::QueryContractId,
    request: &GpuDispatchRequest,
) {
    let path = world_query_kind_for_contract_id(contract_id)
        .and_then(|kind| helper_path_label_for_world_kind(kind, request, observability));
    if observability.wgsl_world_helper_path.is_none() {
        observability.wgsl_world_helper_path = path;
    }
}

fn annotate_wgsl_world_helper_path_for_batch(
    observability: &mut QueryExecutionObservability,
    contract_id: crate::query_contract::QueryContractId,
    request: &GpuDispatchRequest,
) {
    let path = batch_query_kind_for_contract_id(contract_id)
        .and_then(|kind| helper_path_label_for_batch_kind(kind, request, observability));
    if observability.wgsl_world_helper_path.is_none() {
        observability.wgsl_world_helper_path = path;
    }
}

fn helper_path_label_for_world_kind(
    kind: WorldQueryKind,
    request: &GpuDispatchRequest,
    observability: &QueryExecutionObservability,
) -> Option<SmolStr> {
    let label = match kind {
        WorldQueryKind::Distance => classify_wgsl_world_helper_path(request, observability)?,
        WorldQueryKind::Normal => {
            if request.world_shape_indices.len() == 1 {
                "single_shape"
            } else {
                classify_wgsl_world_helper_path(request, observability)?
            }
        }
        WorldQueryKind::Radiance => classify_wgsl_world_helper_path(request, observability)?,
        WorldQueryKind::Medium => classify_wgsl_world_helper_path(request, observability)?,
        _ => return None,
    };
    Some(SmolStr::new(label))
}

fn helper_path_label_for_batch_kind(
    kind: BatchQueryKind,
    request: &GpuDispatchRequest,
    observability: &QueryExecutionObservability,
) -> Option<SmolStr> {
    let label = match kind {
        BatchQueryKind::Distance => classify_wgsl_world_helper_path(request, observability)?,
        BatchQueryKind::Normal => {
            if request.world_shape_indices.len() == 1 {
                "single_shape"
            } else {
                classify_wgsl_world_helper_path(request, observability)?
            }
        }
        BatchQueryKind::Radiance => classify_wgsl_world_helper_path(request, observability)?,
        BatchQueryKind::Medium => classify_wgsl_world_helper_path(request, observability)?,
        _ => return None,
    };
    Some(SmolStr::new(label))
}

fn classify_wgsl_world_helper_path<'a>(
    request: &GpuDispatchRequest,
    observability: &'a QueryExecutionObservability,
) -> Option<&'a str> {
    if request.world_shape_indices.is_empty() {
        return None;
    }
    if request.accel_nodes.is_empty() || observability.cache_budget_rejections > 0 {
        Some("dense_fallback")
    } else {
        Some("accelerated")
    }
}

pub(crate) fn compile_world_shader(
    ctx: &crate::query_exec::context::QueryExecContext,
    plan: &KernelWorldQueryPlan,
) -> Result<GeneratedShaderModule, QueryExecError> {
    generate_compiled_shader(ctx, ShaderPlan::World(plan))
}

pub(crate) fn compile_batch_shader(
    ctx: &crate::query_exec::context::QueryExecContext,
    plan: &KernelBatchQueryPlan,
) -> Result<GeneratedShaderModule, QueryExecError> {
    generate_compiled_shader(ctx, ShaderPlan::Batch(plan))
}

pub(crate) fn build_batch_request_for_shader(
    ctx: &crate::query_exec::context::QueryExecContext,
    plan: &KernelBatchQueryPlan,
    args: &[KernelValue],
) -> Result<GpuDispatchRequest, QueryExecError> {
    build_batch_request_for_shader_with_snapshot(ctx, None, plan, args)
}

pub(crate) fn build_batch_request_for_shader_with_snapshot(
    ctx: &crate::query_exec::context::QueryExecContext,
    snapshot: Option<&crate::world_identity::WorldSnapshotHandle>,
    plan: &KernelBatchQueryPlan,
    args: &[KernelValue],
) -> Result<GpuDispatchRequest, QueryExecError> {
    if let Err(errors) = validate_batch_query_plan(plan) {
        return Err(validation_error("batch query", errors));
    }
    let ops = DirectQueryOps::new_with_snapshot(ctx, snapshot);
    build_batch_request(&ops, plan, args)
}

pub(crate) fn build_batch_request_without_items_for_shader(
    ctx: &crate::query_exec::context::QueryExecContext,
    plan: &KernelBatchQueryPlan,
    args: &[KernelValue],
    item_count: u32,
) -> Result<GpuDispatchRequest, QueryExecError> {
    build_batch_request_without_items_for_shader_with_snapshot(ctx, None, plan, args, item_count)
}

pub(crate) fn build_batch_request_without_items_for_shader_with_snapshot(
    ctx: &crate::query_exec::context::QueryExecContext,
    snapshot: Option<&crate::world_identity::WorldSnapshotHandle>,
    plan: &KernelBatchQueryPlan,
    args: &[KernelValue],
    item_count: u32,
) -> Result<GpuDispatchRequest, QueryExecError> {
    if let Err(errors) = validate_batch_query_plan(plan) {
        return Err(validation_error("batch query", errors));
    }
    let ops = DirectQueryOps::new_with_snapshot(ctx, snapshot);
    build_batch_request_without_items(&ops, plan, args, item_count)
}

pub(crate) fn prepare_resident_batch_query(
    generated: &GeneratedShaderModule,
    request: &GpuDispatchRequest,
) -> Result<ResidentBatchQuerySession, QueryExecError> {
    let prepared_payload = prepare_resident_batch_query_payload(generated, request)?;
    prepare_resident_batch_query_with_prepared_payload(generated, request, &prepared_payload)
}

pub(crate) fn prepare_resident_batch_query_payload(
    generated: &GeneratedShaderModule,
    request: &GpuDispatchRequest,
) -> Result<PreparedResidentBatchQueryPayload, QueryExecError> {
    let item_count = dispatch_item_count(request)?;
    if item_count == 0 {
        return Err(QueryExecError::Unsupported {
            message: "resident WGSL dispatch requires a positive item_count".to_string(),
        });
    }
    let dispatch = normalized_dispatch_config(request)?;

    let payloads = WgslDispatchPayloadBytes {
        dispatch_bytes: encode_value(&generated.dispatch_abi, &dispatch)?,
        input_bytes: Vec::new(),
        accel_node_bytes: encode_accel_node_values(
            &generated.accel_node_abi,
            &request.accel_nodes,
        )?,
        accel_child_bytes: encode_u32_values(&request.accel_children)?,
        cache_brick_bytes: encode_cache_brick_values(
            &generated.cache_brick_abi,
            &request.cache_bricks,
        )?,
        shape_meta_bytes: encode_shape_meta_values(
            &generated.shape_meta_abi,
            &generated.shape_meta_values,
        )?,
        world_shape_bytes: encode_shape_indices(&request.world_shape_indices)?,
        continuation_seed_bytes: dispatch_side_channel_bytes(request)?,
    };
    let item_stride = portable_abi_array_stride(&generated.item_abi) as usize;
    let result_stride = portable_abi_array_stride(&generated.result_abi) as usize;
    let input_buffer_size = (item_stride * item_count as usize).max(item_stride.max(4)) as u64;
    let output_buffer_size = (result_stride * item_count as usize).max(result_stride.max(4)) as u64;
    let used_max_storage_buffer_bytes = [
        storage_buffer_size(&payloads.dispatch_bytes),
        input_buffer_size,
        output_buffer_size,
        storage_buffer_size(&payloads.accel_node_bytes),
        storage_buffer_size(&payloads.accel_child_bytes),
        storage_buffer_size(&payloads.cache_brick_bytes),
        storage_buffer_size(&payloads.shape_meta_bytes),
        storage_buffer_size(&payloads.world_shape_bytes),
        storage_buffer_size(&payloads.continuation_seed_bytes),
        (QUERY_WGSL_OBSERVABILITY_U32S * std::mem::size_of::<u32>()) as u64,
    ]
    .into_iter()
    .max()
    .unwrap_or(4);
    Ok(PreparedResidentBatchQueryPayload {
        item_count,
        payloads,
        input_buffer_size,
        output_buffer_size,
        used_max_storage_buffer_bytes,
    })
}

pub(crate) fn prepare_resident_batch_query_with_prepared_payload(
    generated: &GeneratedShaderModule,
    request: &GpuDispatchRequest,
    prepared_payload: &PreparedResidentBatchQueryPayload,
) -> Result<ResidentBatchQuerySession, QueryExecError> {
    let item_count = prepared_payload.item_count;
    let payloads = &prepared_payload.payloads;
    let input_buffer_size = prepared_payload.input_buffer_size;
    let output_buffer_size = prepared_payload.output_buffer_size;
    let used_max_storage_buffer_bytes = prepared_payload.used_max_storage_buffer_bytes;
    let required_limit_request = WgslLimitRequest {
        max_storage_buffers_per_shader_stage: QUERY_WGSL_STORAGE_BUFFERS_PER_SHADER_STAGE,
        max_storage_buffer_binding_size: used_max_storage_buffer_bytes,
        ..WgslLimitRequest::default()
    };
    let native = native_wgpu_context_for_limits(required_limit_request)?;
    let selected_workgroup_size = select_query_wgsl_workgroup_size(&native.adapter_limits)?;
    let diagnostics = WgslDispatchDiagnostics {
        selected_workgroup_size,
        used_max_storage_buffer_bytes,
        requested_max_storage_buffer_bytes: native.requested_limits.max_storage_buffer_binding_size,
        cache_observability_seed: generated.cache_observability_seed,
    };
    let mut gpu_runtime = GpuRuntimeMetrics::default();
    gpu_runtime.note_context_metadata(&native);
    let cached = compiled_query_pipeline(
        &native,
        &generated.source,
        selected_workgroup_size,
        generated,
        &mut gpu_runtime,
    )?;
    let resident_scene_fingerprint = scene_fingerprint(
        cached.layout_identity.layout_signature,
        &payloads.accel_node_bytes,
        &payloads.accel_child_bytes,
        &payloads.cache_brick_bytes,
        &payloads.shape_meta_bytes,
    );
    let resident_world_shape_fingerprint = world_shape_fingerprint(
        cached.layout_identity.layout_signature,
        &payloads.world_shape_bytes,
    );
    let (world_shapes_buffer, scene_bind_group, scene_bind_group_created) =
        if let Some((scene, created)) = shared_resident_scene_for_request(
            cached.layout_identity,
            request,
            &payloads,
            native.limit_request,
            &native,
            &cached.bind_group_layouts[GPU_RUNTIME_SCENE_BIND_GROUP_INDEX as usize],
        )? {
            if created {
                let scene_bytes = scene_upload_bytes(&payloads);
                gpu_runtime.scene_reupload_bytes =
                    gpu_runtime.scene_reupload_bytes.saturating_add(scene_bytes);
                gpu_runtime.upload_bytes = gpu_runtime.upload_bytes.saturating_add(scene_bytes);
            }
            let (world_shapes_buffer, created_world_shapes) = pooled_storage_buffer(
                &native,
                native.limit_request,
                WgslBufferKind::SceneWorldShapes,
                storage_buffer_size(&payloads.world_shape_bytes),
                resident_world_shape_fingerprint,
                &mut gpu_runtime,
            )?;
            if created_world_shapes {
                let world_shape_bytes = world_shape_upload_bytes(&payloads);
                gpu_runtime.scene_reupload_bytes = gpu_runtime
                    .scene_reupload_bytes
                    .saturating_add(world_shape_bytes);
                gpu_runtime.upload_bytes =
                    gpu_runtime.upload_bytes.saturating_add(world_shape_bytes);
            }
            (
                world_shapes_buffer,
                scene.payload.bind_group_scene.clone(),
                created,
            )
        } else {
            let (accel_nodes_buffer, created_accel_nodes) = pooled_storage_buffer(
                &native,
                native.limit_request,
                WgslBufferKind::SceneAccelNodes,
                storage_buffer_size(&payloads.accel_node_bytes),
                resident_scene_fingerprint,
                &mut gpu_runtime,
            )?;
            let (accel_children_buffer, created_accel_children) = pooled_storage_buffer(
                &native,
                native.limit_request,
                WgslBufferKind::SceneAccelChildren,
                storage_buffer_size(&payloads.accel_child_bytes),
                resident_scene_fingerprint,
                &mut gpu_runtime,
            )?;
            let (cache_bricks_buffer, created_cache_bricks) = pooled_storage_buffer(
                &native,
                native.limit_request,
                WgslBufferKind::SceneCacheBricks,
                storage_buffer_size(&payloads.cache_brick_bytes),
                resident_scene_fingerprint,
                &mut gpu_runtime,
            )?;
            let (shape_meta_buffer, created_shape_meta) = pooled_storage_buffer(
                &native,
                native.limit_request,
                WgslBufferKind::SceneShapeMeta,
                storage_buffer_size(&payloads.shape_meta_bytes),
                resident_scene_fingerprint,
                &mut gpu_runtime,
            )?;
            let (world_shapes_buffer, created_world_shapes) = pooled_storage_buffer(
                &native,
                native.limit_request,
                WgslBufferKind::SceneWorldShapes,
                storage_buffer_size(&payloads.world_shape_bytes),
                resident_world_shape_fingerprint,
                &mut gpu_runtime,
            )?;
            if created_accel_nodes {
                gpu_runtime.scene_reupload_bytes =
                    gpu_runtime
                        .scene_reupload_bytes
                        .saturating_add(write_pooled_buffer(
                            &native,
                            &accel_nodes_buffer,
                            &payloads.accel_node_bytes,
                        ));
                gpu_runtime.upload_bytes = gpu_runtime
                    .upload_bytes
                    .saturating_add(storage_buffer_size(&payloads.accel_node_bytes));
            }
            if created_accel_children {
                gpu_runtime.scene_reupload_bytes =
                    gpu_runtime
                        .scene_reupload_bytes
                        .saturating_add(write_pooled_buffer(
                            &native,
                            &accel_children_buffer,
                            &payloads.accel_child_bytes,
                        ));
                gpu_runtime.upload_bytes = gpu_runtime
                    .upload_bytes
                    .saturating_add(storage_buffer_size(&payloads.accel_child_bytes));
            }
            if created_cache_bricks {
                gpu_runtime.scene_reupload_bytes =
                    gpu_runtime
                        .scene_reupload_bytes
                        .saturating_add(write_pooled_buffer(
                            &native,
                            &cache_bricks_buffer,
                            &payloads.cache_brick_bytes,
                        ));
                gpu_runtime.upload_bytes = gpu_runtime
                    .upload_bytes
                    .saturating_add(storage_buffer_size(&payloads.cache_brick_bytes));
            }
            if created_shape_meta {
                gpu_runtime.scene_reupload_bytes =
                    gpu_runtime
                        .scene_reupload_bytes
                        .saturating_add(write_pooled_buffer(
                            &native,
                            &shape_meta_buffer,
                            &payloads.shape_meta_bytes,
                        ));
                gpu_runtime.upload_bytes = gpu_runtime
                    .upload_bytes
                    .saturating_add(storage_buffer_size(&payloads.shape_meta_bytes));
            }
            if created_world_shapes {
                gpu_runtime.scene_reupload_bytes =
                    gpu_runtime
                        .scene_reupload_bytes
                        .saturating_add(write_pooled_buffer(
                            &native,
                            &world_shapes_buffer,
                            &payloads.world_shape_bytes,
                        ));
                gpu_runtime.upload_bytes = gpu_runtime
                    .upload_bytes
                    .saturating_add(storage_buffer_size(&payloads.world_shape_bytes));
            }
            let scene_bind_group_key = WgslSceneBindGroupKey {
                limits: native.limit_request,
                pipeline_signature: pipeline_signature(
                    &generated.source,
                    selected_workgroup_size,
                    cached.layout_identity.layout_signature,
                    native.limit_request,
                ),
                scene_fingerprint: resident_scene_fingerprint,
            };
            let (scene_bind_group, scene_bind_group_created) = {
                let cache = scene_bind_group_cache();
                let mut guard = cache.lock().unwrap_or_else(|poison| poison.into_inner());
                if let Some(bind_group) = guard.get(&scene_bind_group_key) {
                    (bind_group.clone(), false)
                } else {
                    let bind_group = native.device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("wrela.wgsl.query.group0"),
                        layout: &cached.bind_group_layouts
                            [GPU_RUNTIME_SCENE_BIND_GROUP_INDEX as usize],
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: accel_nodes_buffer.as_entire_binding(),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: accel_children_buffer.as_entire_binding(),
                            },
                            wgpu::BindGroupEntry {
                                binding: 2,
                                resource: shape_meta_buffer.as_entire_binding(),
                            },
                            wgpu::BindGroupEntry {
                                binding: 3,
                                resource: cache_bricks_buffer.as_entire_binding(),
                            },
                        ],
                    });
                    let entry = guard
                        .entry(scene_bind_group_key)
                        .or_insert_with(|| bind_group.clone());
                    (entry.clone(), true)
                }
            };
            (
                world_shapes_buffer,
                scene_bind_group,
                scene_bind_group_created,
            )
        };
    let scene_token = resident_world_shape_fingerprint;
    let dynamic_resources_key = WgslDynamicResourcesKey {
        limits: native.limit_request,
        pipeline_signature: pipeline_signature(
            &generated.source,
            selected_workgroup_size,
            cached.layout_identity.layout_signature,
            native.limit_request,
        ),
        scene_token,
        dispatch_buffer_size: storage_buffer_size(&payloads.dispatch_bytes),
        input_buffer_size,
        output_buffer_size,
        observability_buffer_size: (QUERY_WGSL_OBSERVABILITY_U32S * std::mem::size_of::<u32>())
            as u64,
        continuation_buffer_size: storage_buffer_size(&payloads.continuation_seed_bytes),
    };
    let (dynamic_resources_mutex, dynamic_resources, dynamic_resources_created) =
        lock_query_dynamic_resources(
            &native,
            dynamic_resources_key,
            &cached,
            &world_shapes_buffer,
        );
    if dynamic_resources_created {
        gpu_runtime.transient_buffer_creations =
            gpu_runtime.transient_buffer_creations.saturating_add(5);
    }
    gpu_runtime.transient_bind_group_creations =
        u32::from(scene_bind_group_created) + if dynamic_resources_created { 3 } else { 0 };
    Ok(ResidentBatchQuerySession {
        native,
        dynamic_resources: dynamic_resources_mutex,
        pipeline: cached.pipeline.clone(),
        scene_bind_group,
        frame_bind_group: dynamic_resources.frame_bind_group.clone(),
        pass_bind_group: dynamic_resources.pass_bind_group.clone(),
        scratch_bind_group: dynamic_resources.scratch_bind_group.clone(),
        dispatch_buffer: dynamic_resources.dispatch_buffer.clone(),
        input_buffer: dynamic_resources.input_buffer.clone(),
        input_buffer_size,
        output_buffer: dynamic_resources.output_buffer.clone(),
        observability_buffer: dynamic_resources.observability_buffer.clone(),
        continuation_seed_buffer: dynamic_resources.continuation_seed_buffer.clone(),
        continuation_seed_buffer_size: dynamic_resources_key.continuation_buffer_size,
        result_abi: generated.result_abi.clone(),
        item_count,
        output_buffer_size,
        observability_buffer_size: dynamic_resources_key.observability_buffer_size,
        layout_signature: cached.layout_identity.layout_signature,
        diagnostics,
        initial_gpu_runtime: gpu_runtime,
    })
}

pub(crate) fn bridge_config(shader: &GeneratedShaderModule) -> NativeWgslBridgeConfig {
    NativeWgslBridgeConfig {
        source: SmolStr::new(shader.source.as_str()),
        workgroup_size: i64::from(shader.workgroup_size),
    }
}

fn build_capture_request(
    ops: &DirectQueryOps<'_>,
    plan: &KernelCaptureQueryPlan,
    args: &[KernelValue],
) -> Result<GpuDispatchRequest, QueryExecError> {
    let descriptor = descriptor_for_plan(plan.contract_id)?;
    if descriptor.result_kind == QueryResultKind::SupportSummaryResult {
        return Err(QueryExecError::Unsupported {
            message: "support.summary is not supported by the native WGSL backend".to_string(),
        });
    }

    let (capture_kind, capture_index, cache_bricks, resident_scene_snapshot) =
        match descriptor.capture_kind {
            CaptureKind::Field => {
                let capture = ops.resolve_field_or_shape_capture(args.first())?;
                note_wgsl_normal_role_for_capture(ops, descriptor, &capture);
                (
                    0u32,
                    field_index(ops.context(), &capture)?,
                    Vec::new(),
                    ops.context().snapshot_report_for_capture_name(&capture),
                )
            }
            CaptureKind::Shape => {
                let capture = ops.resolve_shape_capture(args.first())?;
                note_wgsl_normal_role_for_capture(ops, descriptor, &capture);
                (
                    1u32,
                    shape_index(ops.context(), &capture)?,
                    shape_cache_brick_kernel_values(ops.context(), &capture),
                    ops.context().snapshot_report_for_capture_name(&capture),
                )
            }
            CaptureKind::Region => {
                return Err(QueryExecError::Unsupported {
                    message: "region captures are only valid for world queries".to_string(),
                });
            }
        };

    let item = scalar_item_arg(descriptor, args.get(1))?;

    Ok(GpuDispatchRequest {
        dispatch: dispatch_config(
            capture_kind,
            capture_index,
            1,
            0,
            0,
            0,
            cache_bricks.len() as u32,
            true,
            true,
            true,
            false,
        ),
        items: vec![item],
        world_shape_indices: Vec::new(),
        accel_nodes: Vec::new(),
        accel_children: Vec::new(),
        cache_bricks,
        continuation_seeds: Vec::new(),
        candidate_spans: Vec::new(),
        resident_scene_snapshot,
        resident_scene_detail: 0,
        resident_scene_selection_signature: 0,
    })
}

fn scalar_item_arg(
    descriptor: &QueryContractDescriptor,
    value: Option<&KernelValue>,
) -> Result<KernelValue, QueryExecError> {
    match descriptor.item_kind {
        QueryItemKind::Unit => Err(QueryExecError::Unsupported {
            message: "unit query items are not supported by the native WGSL backend".to_string(),
        }),
        QueryItemKind::PointQuery => Ok(point_query(expect_vec3_arg(value, "point")?)),
        QueryItemKind::RayQuery => {
            expect_struct_arg(value, "RayQuery")?;
            value
                .cloned()
                .ok_or(QueryExecError::MissingCaptureTarget { kind: "ray" })
        }
        QueryItemKind::Hit3 => value
            .cloned()
            .ok_or(QueryExecError::MissingCaptureTarget { kind: "hit" }),
        QueryItemKind::PointDirectionQuery => {
            expect_struct_arg(value, "PointDirectionQuery")?;
            value
                .cloned()
                .ok_or(QueryExecError::MissingCaptureTarget { kind: "sample" })
        }
    }
}

fn descriptor_for_plan(
    contract_id: crate::query_contract::QueryContractId,
) -> Result<&'static QueryContractDescriptor, QueryExecError> {
    query_contract(contract_id).ok_or_else(|| QueryExecError::Unsupported {
        message: format!("missing query contract '{}'", contract_id.as_str()),
    })
}

fn note_contract_observability(ops: &DirectQueryOps<'_>) {
    ops.note_artifact_load();
}

fn note_wgsl_normal_role_for_capture(
    ops: &DirectQueryOps<'_>,
    descriptor: &QueryContractDescriptor,
    capture: &SmolStr,
) {
    if descriptor.result_kind != QueryResultKind::NormalResult {
        return;
    }
    let role = match descriptor.capture_kind {
        CaptureKind::Field => field_normal_role_for_capture(ops.context(), capture),
        CaptureKind::Shape => shape_normal_role_for_capture(ops.context(), capture),
        CaptureKind::Region => None,
    }
    .unwrap_or(NormalRole::HeuristicShadingNormal);
    ops.note_normal_role(role);
}

fn note_wgsl_normal_role_for_world(
    ops: &DirectQueryOps<'_>,
    descriptor: &QueryContractDescriptor,
    world_shapes: &[SmolStr],
) {
    if descriptor.result_kind != QueryResultKind::NormalResult {
        return;
    }
    let role = if world_shapes.len() == 1 {
        shape_normal_role_for_capture(ops.context(), &world_shapes[0])
    } else {
        None
    }
    .unwrap_or(NormalRole::HeuristicShadingNormal);
    ops.note_normal_role(role);
}

fn field_normal_role_for_capture(
    ctx: &crate::query_exec::context::QueryExecContext,
    capture: &SmolStr,
) -> Option<NormalRole> {
    let scene = ctx.scene.fields.get(capture)?;
    if scene.opaque_boundary
        || !matches!(
            scene.analysis.differential_support,
            crate::scene_ir::SceneDifferentialSupport::CertifiedGradient
        )
    {
        return None;
    }
    field_normal_role_for_node(ctx, &scene.root)
}

fn field_normal_role_for_node(
    ctx: &crate::query_exec::context::QueryExecContext,
    node: &crate::scene_ir::FieldNode,
) -> Option<NormalRole> {
    match node {
        crate::scene_ir::FieldNode::Use { target } => field_normal_role_for_capture(ctx, target),
        crate::scene_ir::FieldNode::Primitive { primitive, .. } => match primitive {
            crate::hir::FieldPrimitive::Sphere | crate::hir::FieldPrimitive::Plane => {
                Some(NormalRole::CertifiedFieldGradient)
            }
            _ => None,
        },
        crate::scene_ir::FieldNode::Transform { kind, inner, .. } => match kind {
            crate::scene_ir::TransformKind::Translate
            | crate::scene_ir::TransformKind::Rotate
            | crate::scene_ir::TransformKind::UniformScale => {
                field_normal_role_for_node(ctx, inner)
            }
            _ => None,
        },
        crate::scene_ir::FieldNode::Smooth {
            smoothing, items, ..
        } => {
            let smoothing = smoothing
                .as_ref()
                .and_then(|expr| eval_scene_constant_f32(ctx, expr))
                .unwrap_or(0.0);
            if smoothing <= 0.0 {
                return None;
            }
            let first = items.first()?;
            field_normal_role_for_node(ctx, first)?;
            for item in items.iter().skip(1) {
                field_normal_role_for_node(ctx, item)?;
            }
            Some(NormalRole::CertifiedFieldGradient)
        }
        crate::scene_ir::FieldNode::Repeat { .. }
        | crate::scene_ir::FieldNode::Union { .. }
        | crate::scene_ir::FieldNode::Intersection { .. }
        | crate::scene_ir::FieldNode::Subtract { .. }
        | crate::scene_ir::FieldNode::Extrude { .. }
        | crate::scene_ir::FieldNode::Revolve { .. }
        | crate::scene_ir::FieldNode::Sweep { .. }
        | crate::scene_ir::FieldNode::Loft { .. }
        | crate::scene_ir::FieldNode::OpaqueLeaf => None,
    }
}

fn shape_normal_role_for_capture(
    ctx: &crate::query_exec::context::QueryExecContext,
    capture: &SmolStr,
) -> Option<NormalRole> {
    let scene = ctx.scene.shapes.get(capture)?;
    if scene.opaque_boundary
        || !matches!(
            scene.analysis.differential_support,
            crate::scene_ir::SceneDifferentialSupport::CertifiedGradient
        )
    {
        return None;
    }
    match &scene.root {
        crate::scene_ir::ShapeNode::Use { target } => shape_normal_role_for_capture(ctx, target),
        crate::scene_ir::ShapeNode::Leaf(leaf) => {
            field_normal_role_for_capture(ctx, &leaf.field)?;
            Some(NormalRole::FeatureNormal)
        }
        crate::scene_ir::ShapeNode::Union { .. }
        | crate::scene_ir::ShapeNode::Intersection { .. }
        | crate::scene_ir::ShapeNode::Subtract { .. } => None,
    }
}

fn eval_scene_constant_f32(
    ctx: &crate::query_exec::context::QueryExecContext,
    expr: &crate::scene_ir::SceneValueExpr,
) -> Option<f32> {
    let ops = DirectQueryOps::new(ctx);
    match ops.eval_scene_constant(expr).ok()? {
        KernelValue::F32(value) => Some(value),
        KernelValue::I32(value) => Some(value as f32),
        KernelValue::U32(value) => Some(value as f32),
        _ => None,
    }
}

fn note_result_observability(
    ops: &DirectQueryOps<'_>,
    descriptor: &QueryContractDescriptor,
    values: &[KernelValue],
) {
    if !descriptor.observability.trace_steps {
        return;
    }
    for value in values {
        match descriptor.result_kind {
            QueryResultKind::Hit3 => {
                if let Ok(hit) = expect_struct_arg(Some(value), "Hit3") {
                    let hit_value = expect_struct_bool(hit, "hit").unwrap_or(false);
                    let steps = expect_struct_i32(hit, "steps").unwrap_or_default().max(0) as u32;
                    // WGSL currently reports trace work from result records rather than a
                    // sideband metric buffer. Preserve the encoded value exactly; miss
                    // records that carry zero steps must stay zero instead of inventing
                    // work the backend did not report.
                    ops.note_trace_steps(steps);
                    ops.note_hit_result(hit_value, steps);
                }
            }
            QueryResultKind::OcclusionResult => {
                if let Ok(occlusion) = expect_struct_arg(Some(value), "OcclusionResult") {
                    let hit_value = expect_struct_bool(occlusion, "occluded").unwrap_or(false);
                    let steps = expect_struct_i32(occlusion, "steps")
                        .unwrap_or_default()
                        .max(0) as u32;
                    ops.note_trace_steps(steps);
                    ops.note_hit_result(hit_value, steps);
                }
            }
            _ => {
                if matches!(
                    descriptor.item_kind,
                    QueryItemKind::RayQuery | QueryItemKind::Hit3
                ) {
                    ops.note_trace_step();
                }
            }
        }
    }
}

fn scene_domain_flag_enabled(
    domain: &KernelStructValue,
    flag: SceneDomainFlag,
) -> Result<bool, QueryExecError> {
    let flag_name = scene_domain_flag_name(flag);
    let (contract_field, contract_name) = match flag {
        SceneDomainFlag::Material => ("surface", "SurfaceDomainContract"),
        SceneDomainFlag::Radiance | SceneDomainFlag::Media => {
            ("participants", "ParticipantDomainContract")
        }
    };
    let contract = expect_struct_arg(struct_field(domain, contract_field), contract_name)?;
    expect_struct_bool(contract, flag_name)
}

fn batch_array_label(descriptor: &QueryContractDescriptor) -> &'static str {
    match descriptor.item_kind {
        QueryItemKind::PointQuery => "points",
        QueryItemKind::RayQuery => "rays",
        QueryItemKind::Hit3 => "hits",
        QueryItemKind::PointDirectionQuery => "samples",
        QueryItemKind::Unit => "items",
    }
}

fn build_world_request(
    ops: &DirectQueryOps<'_>,
    plan: &KernelWorldQueryPlan,
    args: &[KernelValue],
) -> Result<GpuDispatchRequest, QueryExecError> {
    let descriptor = descriptor_for_plan(plan.contract_id)?;
    if descriptor.result_kind == QueryResultKind::SupportSummaryResult {
        return Err(QueryExecError::Unsupported {
            message: "support.summary is not supported by the native WGSL backend".to_string(),
        });
    }
    let capture = ops.resolve_region_capture(args.first())?;
    let domain = expect_struct_arg(args.get(1), "SceneDomain")?;
    let detail = ops.validate_world_domain(
        &capture,
        domain,
        world_query_semantics_for_contract(plan.contract_id).query_name,
    )?;
    let surface_root_shape_id = if descriptor.item_kind == QueryItemKind::Hit3 {
        let hit = expect_struct_arg(args.get(2), "Hit3")?;
        Some(expect_struct_u32(hit, "root_shape_id")?)
    } else {
        None
    };
    let world_shapes = ops.resolve_world_shapes(&capture, detail, surface_root_shape_id)?;
    ops.note_candidate_count(world_shapes.len() as u32);
    let world_shape_indices = world_shapes
        .iter()
        .map(|shape| shape_index(ops.context(), shape))
        .collect::<Result<Vec<_>, _>>()?;
    let accel = world_acceleration_request_data(ops.context(), &capture, detail)?;
    let cache_bricks = world_cache_brick_kernel_values(ops.context(), &capture, detail);
    note_wgsl_normal_role_for_world(ops, descriptor, &world_shapes);
    let item = scalar_item_arg(descriptor, args.get(2))?;

    Ok(GpuDispatchRequest {
        dispatch: dispatch_config(
            2,
            0,
            1,
            world_shape_indices.len() as u32,
            accel.root_index,
            accel.nodes.len() as u32,
            cache_bricks.len() as u32,
            scene_domain_flag_enabled(domain, SceneDomainFlag::Material)?,
            scene_domain_flag_enabled(domain, SceneDomainFlag::Radiance)?,
            scene_domain_flag_enabled(domain, SceneDomainFlag::Media)?,
            false,
        ),
        items: vec![item],
        world_shape_indices,
        accel_nodes: accel_nodes_kernel_values(&accel.nodes),
        accel_children: accel.children,
        cache_bricks,
        continuation_seeds: Vec::new(),
        candidate_spans: Vec::new(),
        resident_scene_snapshot: ops.context().snapshot_report_for_capture_name(&capture),
        resident_scene_detail: detail,
        resident_scene_selection_signature: u64::from(surface_root_shape_id.unwrap_or_default()),
    })
}

fn build_batch_request(
    ops: &DirectQueryOps<'_>,
    plan: &KernelBatchQueryPlan,
    args: &[KernelValue],
) -> Result<GpuDispatchRequest, QueryExecError> {
    let descriptor = descriptor_for_plan(plan.contract_id)?;
    if descriptor.cardinality != QueryCardinality::Batch {
        return Err(QueryExecError::Unsupported {
            message: format!(
                "WGSL batch execution requires a batch contract, got '{}'",
                descriptor.id.as_str()
            ),
        });
    }
    if descriptor.result_kind == QueryResultKind::SupportSummaryResult {
        return Err(QueryExecError::Unsupported {
            message: "support.summary is not supported by the native WGSL backend".to_string(),
        });
    }
    if descriptor.target == QueryTargetKind::World {
        return build_world_batch_request(ops, plan, descriptor, args);
    }
    let capture = match descriptor.capture_kind {
        CaptureKind::Field => {
            let capture = ops.resolve_field_or_shape_capture(args.first())?;
            note_wgsl_normal_role_for_capture(ops, descriptor, &capture);
            capture
        }
        CaptureKind::Shape => {
            let capture = ops.resolve_shape_capture(args.first())?;
            note_wgsl_normal_role_for_capture(ops, descriptor, &capture);
            capture
        }
        CaptureKind::Region => {
            return Err(QueryExecError::Unsupported {
                message: "region captures are only valid for world queries".to_string(),
            });
        }
    };
    let items = expect_array_arg(args.get(1), batch_array_label(descriptor))?;
    ops.note_candidate_count(items.len() as u32);
    ops.note_batch_execution_mode(!matches!(
        plan.pruning_strategy,
        crate::query_plan::PruningStrategy::None
            | crate::query_plan::PruningStrategy::ConservativeTraversal
    ));
    let cache_bricks = match descriptor.capture_kind {
        CaptureKind::Shape => shape_cache_brick_kernel_values(ops.context(), &capture),
        CaptureKind::Field | CaptureKind::Region => Vec::new(),
    };
    Ok(GpuDispatchRequest {
        dispatch: dispatch_config(
            match descriptor.capture_kind {
                CaptureKind::Field => 0,
                CaptureKind::Shape => 1,
                CaptureKind::Region => 2,
            },
            match descriptor.capture_kind {
                CaptureKind::Field => field_index(ops.context(), &capture)?,
                CaptureKind::Shape => shape_index(ops.context(), &capture)?,
                CaptureKind::Region => 0,
            },
            items.len() as u32,
            0,
            0,
            0,
            cache_bricks.len() as u32,
            true,
            true,
            true,
            false,
        ),
        items: items.to_vec(),
        world_shape_indices: Vec::new(),
        accel_nodes: Vec::new(),
        accel_children: Vec::new(),
        cache_bricks,
        continuation_seeds: Vec::new(),
        candidate_spans: Vec::new(),
        resident_scene_snapshot: ops.context().snapshot_report_for_capture_name(&capture),
        resident_scene_detail: 0,
        resident_scene_selection_signature: 0,
    })
}

fn build_batch_request_without_items(
    ops: &DirectQueryOps<'_>,
    plan: &KernelBatchQueryPlan,
    args: &[KernelValue],
    item_count: u32,
) -> Result<GpuDispatchRequest, QueryExecError> {
    let descriptor = descriptor_for_plan(plan.contract_id)?;
    let capture = ops.resolve_region_capture(args.first())?;
    let domain = expect_struct_arg(args.get(1), "SceneDomain")?;
    let detail = ops.validate_world_domain(
        &capture,
        domain,
        world_query_semantics_for_contract(plan.contract_id).query_name,
    )?;
    let world_shapes = ops.resolve_world_shapes(&capture, detail, None)?;
    ops.note_candidate_count((world_shapes.len() as u32).saturating_mul(item_count));
    ops.note_batch_execution_mode(!matches!(
        plan.pruning_strategy,
        crate::query_plan::PruningStrategy::None
            | crate::query_plan::PruningStrategy::ConservativeTraversal
    ));
    let world_shape_indices = world_shapes
        .iter()
        .map(|shape| shape_index(ops.context(), shape))
        .collect::<Result<Vec<_>, _>>()?;
    let accel = world_acceleration_request_data(ops.context(), &capture, detail)?;
    let cache_bricks = world_cache_brick_kernel_values(ops.context(), &capture, detail);
    note_wgsl_normal_role_for_world(ops, descriptor, &world_shapes);

    Ok(GpuDispatchRequest {
        dispatch: dispatch_config(
            2,
            0,
            item_count,
            world_shape_indices.len() as u32,
            accel.root_index,
            accel.nodes.len() as u32,
            cache_bricks.len() as u32,
            scene_domain_flag_enabled(domain, SceneDomainFlag::Material)?,
            scene_domain_flag_enabled(domain, SceneDomainFlag::Radiance)?,
            scene_domain_flag_enabled(domain, SceneDomainFlag::Media)?,
            false,
        ),
        items: Vec::new(),
        world_shape_indices,
        accel_nodes: accel_nodes_kernel_values(&accel.nodes),
        accel_children: accel.children,
        cache_bricks,
        continuation_seeds: Vec::new(),
        candidate_spans: Vec::new(),
        resident_scene_snapshot: ops.context().snapshot_report_for_capture_name(&capture),
        resident_scene_detail: detail,
        resident_scene_selection_signature: 0,
    })
}

fn build_world_batch_request(
    ops: &DirectQueryOps<'_>,
    plan: &KernelBatchQueryPlan,
    descriptor: &QueryContractDescriptor,
    args: &[KernelValue],
) -> Result<GpuDispatchRequest, QueryExecError> {
    let capture = ops.resolve_region_capture(args.first())?;
    let domain = expect_struct_arg(args.get(1), "SceneDomain")?;
    let detail = ops.validate_world_domain(
        &capture,
        domain,
        world_query_semantics_for_contract(plan.contract_id).query_name,
    )?;
    let items = expect_array_arg(args.get(2), batch_array_label(descriptor))?;
    let world_shapes = ops.resolve_world_shapes(&capture, detail, None)?;
    ops.note_candidate_count((world_shapes.len() * items.len()) as u32);
    ops.note_batch_execution_mode(!matches!(
        plan.pruning_strategy,
        crate::query_plan::PruningStrategy::None
            | crate::query_plan::PruningStrategy::ConservativeTraversal
    ));
    let world_shape_indices = world_shapes
        .iter()
        .map(|shape| shape_index(ops.context(), shape))
        .collect::<Result<Vec<_>, _>>()?;
    let accel = world_acceleration_request_data(ops.context(), &capture, detail)?;
    let cache_bricks = world_cache_brick_kernel_values(ops.context(), &capture, detail);
    note_wgsl_normal_role_for_world(ops, descriptor, &world_shapes);

    Ok(GpuDispatchRequest {
        dispatch: dispatch_config(
            2,
            0,
            items.len() as u32,
            world_shape_indices.len() as u32,
            accel.root_index,
            accel.nodes.len() as u32,
            cache_bricks.len() as u32,
            scene_domain_flag_enabled(domain, SceneDomainFlag::Material)?,
            scene_domain_flag_enabled(domain, SceneDomainFlag::Radiance)?,
            scene_domain_flag_enabled(domain, SceneDomainFlag::Media)?,
            false,
        ),
        items: items.to_vec(),
        world_shape_indices,
        accel_nodes: accel_nodes_kernel_values(&accel.nodes),
        accel_children: accel.children,
        cache_bricks,
        continuation_seeds: Vec::new(),
        candidate_spans: Vec::new(),
        resident_scene_snapshot: ops.context().snapshot_report_for_capture_name(&capture),
        resident_scene_detail: detail,
        resident_scene_selection_signature: 0,
    })
}

fn generate_compiled_shader(
    ctx: &crate::query_exec::context::QueryExecContext,
    plan: ShaderPlan<'_>,
) -> Result<GeneratedShaderModule, QueryExecError> {
    let key = generated_shader_cache_key(ctx, &plan);
    let cache = generated_shader_modules_cache();
    if let Some(cached) = cache
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .get(&key)
        .cloned()
    {
        return Ok(cached);
    }
    let generated = generate_shader(ctx, plan)?;
    validate_generated_shader(&generated.source)?;
    let compiled = GeneratedShaderModule {
        source: generated.source,
        workgroup_size: generated.workgroup_size,
        dispatch_abi: generated.dispatch_abi,
        accel_node_abi: generated.accel_node_abi.clone(),
        cache_brick_abi: generated.cache_brick_abi.clone(),
        shape_meta_abi: generated.shape_meta_abi.clone(),
        item_abi: generated.item_abi,
        result_abi: generated.result_abi,
        shape_meta_values: generated.shape_meta_values,
        cache_observability_seed: generated.cache_observability_seed,
    };
    cache
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .insert(key, compiled.clone());
    Ok(compiled)
}

fn generated_shader_cache_key(
    ctx: &crate::query_exec::context::QueryExecContext,
    plan: &ShaderPlan<'_>,
) -> GeneratedShaderCacheKey {
    let plan_debug = format!("{plan:?}");
    GeneratedShaderCacheKey {
        context_id: ctx.wgsl_shader_cache_context_id,
        plan_signature: stable_semantic_id(&[plan_debug.as_bytes()]),
        f16_enabled: requested_shader_f16_feature(),
    }
}

fn world_acceleration_request_data(
    ctx: &crate::query_exec::context::QueryExecContext,
    capture: &SmolStr,
    detail: i32,
) -> Result<WgslAccelerationForestData, QueryExecError> {
    let Some(forest) = ctx.world_acceleration_forest(capture, detail) else {
        return Ok(WgslAccelerationForestData {
            root_index: 0,
            nodes: Vec::new(),
            children: Vec::new(),
        });
    };
    build_wgsl_acceleration_forest_data(ctx, forest)
}

fn build_wgsl_acceleration_forest_data(
    ctx: &crate::query_exec::context::QueryExecContext,
    forest: &AccelerationForest,
) -> Result<WgslAccelerationForestData, QueryExecError> {
    let Some(root_id) = forest.root_nodes().first() else {
        return Ok(WgslAccelerationForestData {
            root_index: 0,
            nodes: Vec::new(),
            children: Vec::new(),
        });
    };
    let node_lookup = forest
        .nodes
        .iter()
        .map(|node| (node.id.clone(), node))
        .collect::<HashMap<_, _>>();
    let mut built = HashMap::<SmolStr, usize>::new();
    let mut nodes = Vec::new();
    let mut children = Vec::new();
    let root_index = build_wgsl_acceleration_subtree(
        ctx,
        root_id,
        &node_lookup,
        &mut built,
        &mut nodes,
        &mut children,
    )?;
    Ok(WgslAccelerationForestData {
        root_index: root_index as u32,
        nodes,
        children,
    })
}

fn build_wgsl_acceleration_subtree(
    ctx: &crate::query_exec::context::QueryExecContext,
    id: &SmolStr,
    node_lookup: &HashMap<SmolStr, &crate::acceleration::AccelerationNode>,
    built: &mut HashMap<SmolStr, usize>,
    nodes: &mut Vec<WgslAccelNodeRecord>,
    children: &mut Vec<u32>,
) -> Result<usize, QueryExecError> {
    if let Some(existing) = built.get(id).copied() {
        return Ok(existing);
    }
    let source = node_lookup
        .get(id)
        .copied()
        .ok_or_else(|| QueryExecError::Unsupported {
            message: format!("shared acceleration forest is missing node '{id}'"),
        })?;
    let mut child_indices = Vec::new();
    for child_id in &source.child_ids {
        child_indices.push(build_wgsl_acceleration_subtree(
            ctx,
            child_id,
            node_lookup,
            built,
            nodes,
            children,
        )? as u32);
    }
    let child_start = children.len() as u32;
    children.extend(child_indices.iter().copied());
    let bounds = acceleration_node_bounds(source);
    let leaf_shape_index = source
        .leaf_payload
        .as_ref()
        .map(|payload| shape_index(ctx, &payload.semantic_id))
        .transpose()?
        .unwrap_or(u32::MAX);
    let mut flags = 0;
    if source.leaf_payload.is_some() {
        flags |= QUERY_WGSL_ACCEL_FLAG_LEAF;
    }
    if bounds.is_some() {
        flags |= QUERY_WGSL_ACCEL_FLAG_HAS_BOUNDS;
    }
    let (min, max) = bounds.unwrap_or(([0.0, 0.0, 0.0], [0.0, 0.0, 0.0]));
    let index = nodes.len();
    nodes.push(WgslAccelNodeRecord {
        min,
        max,
        child_start,
        child_len: child_indices.len() as u32,
        leaf_shape_index,
        flags,
    });
    built.insert(id.clone(), index);
    Ok(index)
}

fn acceleration_node_bounds(
    node: &crate::acceleration::AccelerationNode,
) -> Option<([f32; 3], [f32; 3])> {
    node.bounds.iter().find_map(|bound| {
        if !matches!(bound.kind, BoundDescriptorKind::AxisAlignedBounds) {
            return None;
        }
        parse_support_bounds_summary(&bound.summary).map(|bounds| (bounds.min, bounds.max))
    })
}

#[derive(Debug, Clone, Copy)]
struct SupportBounds {
    min: [f32; 3],
    max: [f32; 3],
}

fn parse_support_bounds_summary(summary: &str) -> Option<SupportBounds> {
    let (min, max) = summary.split_once("|max=")?;
    Some(SupportBounds {
        min: parse_summary_vec3(min.strip_prefix("min=")?)?,
        max: parse_summary_vec3(max)?,
    })
}

fn parse_summary_vec3(summary: &str) -> Option<[f32; 3]> {
    let parts = summary
        .split(',')
        .map(|part| part.trim().parse::<f32>().ok())
        .collect::<Option<Vec<_>>>()?;
    let [x, y, z] = parts.try_into().ok()?;
    Some([x, y, z])
}

fn shape_cache_brick_kernel_values(
    ctx: &crate::query_exec::context::QueryExecContext,
    shape: &SmolStr,
) -> Vec<KernelValue> {
    cache_brick_kernel_values(ctx.shape_cache_support(shape))
}

fn world_cache_brick_kernel_values(
    ctx: &crate::query_exec::context::QueryExecContext,
    capture: &SmolStr,
    detail: i32,
) -> Vec<KernelValue> {
    cache_brick_kernel_values(ctx.world_cache_support(capture, detail))
}

fn cache_brick_kernel_values(cache: Option<&SupportBrickCache>) -> Vec<KernelValue> {
    let Some(cache) = cache.filter(|cache| cache.is_ready()) else {
        return Vec::new();
    };
    if cache.bricks.is_empty() {
        return Vec::new();
    }
    cache
        .bricks
        .iter()
        .map(|brick| {
            KernelValue::Struct(KernelStructValue {
                name: SmolStr::new("WgslCacheBrick"),
                fields: vec![
                    (SmolStr::new("min"), KernelValue::Vec3(brick.bounds.min)),
                    (SmolStr::new("max"), KernelValue::Vec3(brick.bounds.max)),
                ],
            })
        })
        .collect()
}

fn accel_nodes_kernel_values(nodes: &[WgslAccelNodeRecord]) -> Vec<KernelValue> {
    if nodes.is_empty() {
        return vec![empty_accel_node_kernel_value()];
    }
    nodes
        .iter()
        .map(|node| {
            KernelValue::Struct(KernelStructValue {
                name: SmolStr::new("WgslAccelNode"),
                fields: vec![
                    (SmolStr::new("min"), KernelValue::Vec3(node.min)),
                    (SmolStr::new("max"), KernelValue::Vec3(node.max)),
                    (
                        SmolStr::new("child_start"),
                        KernelValue::U32(node.child_start),
                    ),
                    (SmolStr::new("child_len"), KernelValue::U32(node.child_len)),
                    (
                        SmolStr::new("leaf_shape_index"),
                        KernelValue::U32(node.leaf_shape_index),
                    ),
                    (SmolStr::new("flags"), KernelValue::U32(node.flags)),
                ],
            })
        })
        .collect()
}

fn empty_accel_node_kernel_value() -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("WgslAccelNode"),
        fields: vec![
            (SmolStr::new("min"), KernelValue::Vec3([0.0, 0.0, 0.0])),
            (SmolStr::new("max"), KernelValue::Vec3([0.0, 0.0, 0.0])),
            (SmolStr::new("child_start"), KernelValue::U32(0)),
            (SmolStr::new("child_len"), KernelValue::U32(0)),
            (SmolStr::new("leaf_shape_index"), KernelValue::U32(u32::MAX)),
            (SmolStr::new("flags"), KernelValue::U32(0)),
        ],
    })
}

fn empty_cache_brick_kernel_value() -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("WgslCacheBrick"),
        fields: vec![
            (SmolStr::new("min"), KernelValue::Vec3([0.0, 0.0, 0.0])),
            (SmolStr::new("max"), KernelValue::Vec3([0.0, 0.0, 0.0])),
        ],
    })
}

fn empty_shape_meta_kernel_value() -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("WgslShapeMeta"),
        fields: vec![
            (SmolStr::new("root_shape_id"), KernelValue::U32(0)),
            (SmolStr::new("analytic_kind"), KernelValue::U32(0)),
        ],
    })
}

fn encode_u32_values(values: &[u32]) -> Result<Vec<u8>, QueryExecError> {
    let values = if values.is_empty() {
        vec![KernelValue::U32(0)]
    } else {
        values.iter().copied().map(KernelValue::U32).collect()
    };
    encode_slice(&PortableAbiType::U32, &values)
}

fn encode_accel_node_values(
    abi: &PortableAbiType,
    values: &[KernelValue],
) -> Result<Vec<u8>, QueryExecError> {
    if values.is_empty() {
        encode_slice(abi, &[empty_accel_node_kernel_value()])
    } else {
        encode_slice(abi, values)
    }
}

fn encode_cache_brick_values(
    abi: &PortableAbiType,
    values: &[KernelValue],
) -> Result<Vec<u8>, QueryExecError> {
    if values.is_empty() {
        encode_slice(abi, &[empty_cache_brick_kernel_value()])
    } else {
        encode_slice(abi, values)
    }
}

fn encode_shape_meta_values(
    abi: &PortableAbiType,
    values: &[KernelValue],
) -> Result<Vec<u8>, QueryExecError> {
    if values.is_empty() {
        encode_slice(abi, &[empty_shape_meta_kernel_value()])
    } else {
        encode_slice(abi, values)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum WgslBufferKind {
    SceneAccelNodes,
    SceneAccelChildren,
    SceneCacheBricks,
    SceneShapeMeta,
    SceneWorldShapes,
}

impl WgslBufferKind {
    fn label(self) -> &'static str {
        match self {
            Self::SceneAccelNodes => "wrela.wgsl.accel_nodes",
            Self::SceneAccelChildren => "wrela.wgsl.accel_children",
            Self::SceneCacheBricks => "wrela.wgsl.cache_bricks",
            Self::SceneShapeMeta => "wrela.wgsl.shape_meta",
            Self::SceneWorldShapes => "wrela.wgsl.world_shapes",
        }
    }

    fn usage(self) -> wgpu::BufferUsages {
        match self {
            Self::SceneAccelNodes
            | Self::SceneAccelChildren
            | Self::SceneCacheBricks
            | Self::SceneShapeMeta
            | Self::SceneWorldShapes => wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct WgslBufferPoolKey {
    limits: WgslLimitRequest,
    kind: WgslBufferKind,
    size: u64,
    token: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct WgslSceneBindGroupKey {
    limits: WgslLimitRequest,
    pipeline_signature: u64,
    scene_fingerprint: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct WgslPipelineCacheKey {
    limits: WgslLimitRequest,
    pipeline: ComputePipelineKey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct WgslDynamicResourcesKey {
    limits: WgslLimitRequest,
    pipeline_signature: u64,
    scene_token: u64,
    dispatch_buffer_size: u64,
    input_buffer_size: u64,
    output_buffer_size: u64,
    observability_buffer_size: u64,
    continuation_buffer_size: u64,
}

#[derive(Clone)]
struct WgslDynamicResources {
    dispatch_buffer: wgpu::Buffer,
    input_buffer: wgpu::Buffer,
    output_buffer: wgpu::Buffer,
    observability_buffer: wgpu::Buffer,
    continuation_seed_buffer: wgpu::Buffer,
    frame_bind_group: wgpu::BindGroup,
    pass_bind_group: wgpu::BindGroup,
    scratch_bind_group: wgpu::BindGroup,
    last_dispatch_fingerprint: Option<u64>,
    last_input_fingerprint: Option<u64>,
    last_continuation_seed_fingerprint: Option<u64>,
}

#[derive(Debug, Clone)]
struct WgslDispatchPayloadBytes {
    dispatch_bytes: Vec<u8>,
    input_bytes: Vec<u8>,
    accel_node_bytes: Vec<u8>,
    accel_child_bytes: Vec<u8>,
    cache_brick_bytes: Vec<u8>,
    shape_meta_bytes: Vec<u8>,
    world_shape_bytes: Vec<u8>,
    continuation_seed_bytes: Vec<u8>,
}

#[derive(Debug, Clone)]
pub(crate) struct PreparedResidentBatchQueryPayload {
    item_count: u32,
    payloads: WgslDispatchPayloadBytes,
    input_buffer_size: u64,
    output_buffer_size: u64,
    used_max_storage_buffer_bytes: u64,
}

#[derive(Debug, Clone)]
struct WgslDispatchOutcome {
    result_bytes: Vec<u8>,
    observability_bytes: Vec<u8>,
    gpu_runtime: GpuRuntimeMetrics,
    layout_signature: u64,
}

#[derive(Debug)]
struct WgslResidentScenePayload {
    accel_nodes: wgpu::Buffer,
    accel_children: wgpu::Buffer,
    shape_meta: wgpu::Buffer,
    cache_bricks: wgpu::Buffer,
    bind_group_scene: wgpu::BindGroup,
}

fn storage_buffer_size(bytes: &[u8]) -> u64 {
    bytes.len().max(4) as u64
}

fn dispatch_item_count(request: &GpuDispatchRequest) -> Result<u32, QueryExecError> {
    let dispatch = expect_struct_arg(Some(&request.dispatch), "WgslDispatchConfig")?;
    dispatch
        .fields
        .iter()
        .find(|(name, _)| name == "item_count")
        .and_then(|(_, value)| match value {
            KernelValue::U32(count) => Some(*count),
            _ => None,
        })
        .ok_or_else(|| QueryExecError::Unsupported {
            message: "WGSL dispatch config is missing item_count".to_string(),
        })
}

fn dispatch_side_channel_bytes(request: &GpuDispatchRequest) -> Result<Vec<u8>, QueryExecError> {
    if !request.candidate_spans.is_empty() {
        return encode_u32_values(&request.candidate_spans);
    }
    encode_u32_values(&request.continuation_seeds)
}

pub(crate) fn normalized_dispatch_config(
    request: &GpuDispatchRequest,
) -> Result<KernelValue, QueryExecError> {
    let mut dispatch = expect_struct_arg(Some(&request.dispatch), "WgslDispatchConfig")?.clone();
    if let Some((_, value)) = dispatch
        .fields
        .iter_mut()
        .find(|(name, _)| name == "candidate_spans_enabled")
    {
        *value = KernelValue::Bool(!request.candidate_spans.is_empty());
    }
    Ok(KernelValue::Struct(dispatch))
}

fn padded_storage_bytes(bytes: &[u8]) -> Cow<'_, [u8]> {
    if bytes.len() >= 4 {
        Cow::Borrowed(bytes)
    } else {
        let mut padded = Vec::with_capacity(4);
        padded.extend_from_slice(bytes);
        padded.resize(4, 0);
        Cow::Owned(padded)
    }
}

fn create_storage_buffer_with_bytes(
    device: &wgpu::Device,
    label: &'static str,
    bytes: &[u8],
) -> wgpu::Buffer {
    let padded = padded_storage_bytes(bytes);
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents: padded.as_ref(),
        usage: wgpu::BufferUsages::STORAGE,
    })
}

fn scene_upload_bytes(payloads: &WgslDispatchPayloadBytes) -> u64 {
    storage_buffer_size(&payloads.accel_node_bytes)
        + storage_buffer_size(&payloads.accel_child_bytes)
        + storage_buffer_size(&payloads.cache_brick_bytes)
        + storage_buffer_size(&payloads.shape_meta_bytes)
}

fn world_shape_upload_bytes(payloads: &WgslDispatchPayloadBytes) -> u64 {
    storage_buffer_size(&payloads.world_shape_bytes)
}

fn shared_resident_scene_for_request(
    layout_identity: GpuLayoutIdentity,
    request: &GpuDispatchRequest,
    payloads: &WgslDispatchPayloadBytes,
    runtime_request: WgslLimitRequest,
    native: &NativeWgpuContext,
    scene_layout: &wgpu::BindGroupLayout,
) -> Result<Option<(Arc<GpuResidentScene<WgslResidentScenePayload>>, bool)>, QueryExecError> {
    let Some(snapshot) = request.resident_scene_snapshot.clone() else {
        return Ok(None);
    };
    let cache =
        shared_resident_scene_cache_for_request::<WgslResidentScenePayload>(runtime_request);
    let mut guard = cache.lock().unwrap_or_else(|poison| poison.into_inner());
    let key = GpuResidentSceneKey::new(snapshot, request.resident_scene_detail, layout_identity)
        .with_selection_signature(stable_semantic_id(&[
            b"query_exec::wgsl::resident_scene_scope::v1",
            &request.resident_scene_selection_signature.to_le_bytes(),
            &scene_fingerprint(
                layout_identity.layout_signature,
                &payloads.accel_node_bytes,
                &payloads.accel_child_bytes,
                &payloads.cache_brick_bytes,
                &payloads.shape_meta_bytes,
            )
            .to_le_bytes(),
            &world_shape_fingerprint(
                layout_identity.layout_signature,
                &payloads.world_shape_bytes,
            )
            .to_le_bytes(),
        ]));
    if let Some(scene) = guard.get(&key) {
        return Ok(Some((scene, false)));
    }
    let built = guard.get_or_insert_with(key, |key| {
        let accel_nodes = create_storage_buffer_with_bytes(
            &native.device,
            "wrela.wgsl.scene.accel_nodes",
            &payloads.accel_node_bytes,
        );
        let accel_children = create_storage_buffer_with_bytes(
            &native.device,
            "wrela.wgsl.scene.accel_children",
            &payloads.accel_child_bytes,
        );
        let shape_meta = create_storage_buffer_with_bytes(
            &native.device,
            "wrela.wgsl.scene.shape_meta",
            &payloads.shape_meta_bytes,
        );
        let cache_bricks = create_storage_buffer_with_bytes(
            &native.device,
            "wrela.wgsl.scene.cache_bricks",
            &payloads.cache_brick_bytes,
        );
        let bind_group_scene = native.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("wrela.wgsl.query.group0"),
            layout: scene_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: accel_nodes.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: accel_children.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: shape_meta.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: cache_bricks.as_entire_binding(),
                },
            ],
        });
        Ok(GpuResidentScene::new(
            key.clone(),
            WgslResidentScenePayload {
                accel_nodes,
                accel_children,
                shape_meta,
                cache_bricks,
                bind_group_scene,
            },
        ))
    })?;
    Ok(Some((built, true)))
}

fn pooled_storage_buffer(
    native: &NativeWgpuContext,
    limit_request: WgslLimitRequest,
    kind: WgslBufferKind,
    size: u64,
    token: u64,
    gpu_runtime: &mut GpuRuntimeMetrics,
) -> Result<(wgpu::Buffer, bool), QueryExecError> {
    let cache = pooled_storage_buffer_cache();
    let key = WgslBufferPoolKey {
        limits: limit_request,
        kind,
        size,
        token,
    };

    {
        let guard = cache.lock().unwrap_or_else(|poison| poison.into_inner());
        if let Some(buffer) = guard.get(&key) {
            return Ok((buffer.clone(), false));
        }
    }

    let buffer = native.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(kind.label()),
        size,
        usage: kind.usage(),
        mapped_at_creation: false,
    });
    let mut guard = cache.lock().unwrap_or_else(|poison| poison.into_inner());
    let created = guard.insert(key, buffer.clone()).is_none();
    if created {
        gpu_runtime.transient_buffer_creations =
            gpu_runtime.transient_buffer_creations.saturating_add(1);
    }
    Ok((buffer, created))
}

fn lock_query_dynamic_resources(
    native: &NativeWgpuContext,
    key: WgslDynamicResourcesKey,
    cached: &QueryCachedPipeline,
    world_shapes_buffer: &wgpu::Buffer,
) -> (
    &'static Mutex<WgslDynamicResources>,
    std::sync::MutexGuard<'static, WgslDynamicResources>,
    bool,
) {
    let registry = dynamic_resources_cache();
    let (resources_mutex, created) = {
        let mut guard = registry.lock().unwrap_or_else(|poison| poison.into_inner());
        let mut created = false;
        let resources_mutex = *guard.entry(key).or_insert_with(|| {
            created = true;
            let dispatch_buffer = native.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("wrela.wgsl.dispatch"),
                size: key.dispatch_buffer_size,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let input_buffer = native.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("wrela.wgsl.input"),
                size: key.input_buffer_size,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let output_buffer = native.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("wrela.wgsl.output"),
                size: key.output_buffer_size,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            });
            let observability_buffer = native.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("wrela.wgsl.observability"),
                size: key.observability_buffer_size,
                usage: wgpu::BufferUsages::STORAGE
                    | wgpu::BufferUsages::COPY_SRC
                    | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let continuation_seed_buffer = native.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("wrela.wgsl.continuation"),
                size: key.continuation_buffer_size,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let frame_bind_group = native.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("wrela.wgsl.bind_group1"),
                layout: &cached.bind_group_layouts[GPU_RUNTIME_FRAME_BIND_GROUP_INDEX as usize],
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: dispatch_buffer.as_entire_binding(),
                }],
            });
            let pass_bind_group = native.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("wrela.wgsl.bind_group2"),
                layout: &cached.bind_group_layouts[GPU_RUNTIME_PASS_BIND_GROUP_INDEX as usize],
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: input_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: output_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: world_shapes_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: observability_buffer.as_entire_binding(),
                    },
                ],
            });
            let scratch_bind_group = native.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("wrela.wgsl.bind_group3"),
                layout: &cached.bind_group_layouts[GPU_RUNTIME_SCRATCH_BIND_GROUP_INDEX as usize],
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: continuation_seed_buffer.as_entire_binding(),
                }],
            });
            Box::leak(Box::new(Mutex::new(WgslDynamicResources {
                dispatch_buffer,
                input_buffer,
                output_buffer,
                observability_buffer,
                continuation_seed_buffer,
                frame_bind_group,
                pass_bind_group,
                scratch_bind_group,
                last_dispatch_fingerprint: None,
                last_input_fingerprint: None,
                last_continuation_seed_fingerprint: None,
            })))
        });
        (resources_mutex, created)
    };
    let resources = resources_mutex
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    (resources_mutex, resources, created)
}

fn write_pooled_buffer(native: &NativeWgpuContext, buffer: &wgpu::Buffer, bytes: &[u8]) -> u64 {
    let padded = padded_storage_bytes(bytes);
    native.queue.write_buffer(buffer, 0, padded.as_ref());
    storage_buffer_size(padded.as_ref())
}

fn scene_fingerprint(
    layout_signature: u64,
    accel_node_bytes: &[u8],
    accel_child_bytes: &[u8],
    cache_brick_bytes: &[u8],
    shape_meta_bytes: &[u8],
) -> u64 {
    stable_semantic_id(&[
        b"query_exec::wgsl::resident_scene_static::v1",
        &layout_signature.to_le_bytes(),
        accel_node_bytes,
        accel_child_bytes,
        cache_brick_bytes,
        shape_meta_bytes,
    ])
}

fn world_shape_fingerprint(layout_signature: u64, world_shape_bytes: &[u8]) -> u64 {
    stable_semantic_id(&[
        b"query_exec::wgsl::resident_world_shapes::v1",
        &layout_signature.to_le_bytes(),
        world_shape_bytes,
    ])
}

fn pipeline_signature(
    source: &str,
    workgroup_size: u32,
    layout_signature: u64,
    limit_request: WgslLimitRequest,
) -> u64 {
    stable_semantic_id(&[
        b"query_exec::wgsl::pipeline::v1",
        source.as_bytes(),
        &workgroup_size.to_le_bytes(),
        &layout_signature.to_le_bytes(),
        &limit_request
            .max_storage_buffers_per_shader_stage
            .to_le_bytes(),
        &limit_request.max_storage_buffer_binding_size.to_le_bytes(),
    ])
}

pub(crate) fn dispatch_compiled_shader(
    generated: &GeneratedShaderModule,
    request: GpuDispatchRequest,
) -> Result<Vec<KernelValue>, QueryExecError> {
    dispatch_compiled_shader_with_observability(generated, request).map(|(values, _)| values)
}

pub(crate) fn dispatch_compiled_shader_with_observability(
    generated: &GeneratedShaderModule,
    request: GpuDispatchRequest,
) -> Result<(Vec<KernelValue>, QueryExecutionObservability), QueryExecError> {
    if request.items.is_empty() {
        return Ok((Vec::new(), QueryExecutionObservability::default()));
    }

    let chunk_plan = compute_wgsl_dispatch_chunk_plan(generated, &request)?;
    if chunk_plan.chunk_count > 1 {
        let mut values = Vec::with_capacity(request.items.len());
        let mut observability = QueryExecutionObservability::default();
        for (chunk_index, start) in (0..request.items.len())
            .step_by(chunk_plan.items_per_chunk)
            .enumerate()
        {
            let end = (start + chunk_plan.items_per_chunk).min(request.items.len());
            let chunk_request = slice_gpu_dispatch_request(&request, start..end)?;
            let (chunk_values, mut chunk_observability) =
                dispatch_compiled_shader_single_with_observability(generated, chunk_request)?;
            if chunk_index > 0 {
                suppress_repeated_chunk_seed_metrics(&mut chunk_observability);
            }
            values.extend(chunk_values);
            observability.merge_from(&chunk_observability);
        }
        observability.gpu_runtime.dispatch_fragmentation_count = observability
            .gpu_runtime
            .dispatch_fragmentation_count
            .saturating_add(chunk_plan.chunk_count.saturating_sub(1) as u32);
        return Ok((values, observability));
    }

    dispatch_compiled_shader_single_with_observability(generated, request)
}

fn dispatch_compiled_shader_single_with_observability(
    generated: &GeneratedShaderModule,
    request: GpuDispatchRequest,
) -> Result<(Vec<KernelValue>, QueryExecutionObservability), QueryExecError> {
    if request.items.is_empty() {
        return Ok((Vec::new(), QueryExecutionObservability::default()));
    }

    let payloads = WgslDispatchPayloadBytes {
        dispatch_bytes: encode_value(
            &generated.dispatch_abi,
            &normalized_dispatch_config(&request)?,
        )?,
        input_bytes: encode_slice(&generated.item_abi, &request.items)?,
        accel_node_bytes: encode_accel_node_values(
            &generated.accel_node_abi,
            &request.accel_nodes,
        )?,
        accel_child_bytes: encode_u32_values(&request.accel_children)?,
        cache_brick_bytes: encode_cache_brick_values(
            &generated.cache_brick_abi,
            &request.cache_bricks,
        )?,
        shape_meta_bytes: encode_shape_meta_values(
            &generated.shape_meta_abi,
            &generated.shape_meta_values,
        )?,
        world_shape_bytes: encode_shape_indices(&request.world_shape_indices)?,
        continuation_seed_bytes: dispatch_side_channel_bytes(&request)?,
    };
    let result_stride = portable_abi_array_stride(&generated.result_abi) as usize;
    let result_buffer_size = (result_stride * request.items.len()).max(result_stride.max(4)) as u64;
    let used_max_storage_buffer_bytes = [
        storage_buffer_size(&payloads.dispatch_bytes),
        storage_buffer_size(&payloads.input_bytes),
        result_buffer_size,
        storage_buffer_size(&payloads.accel_node_bytes),
        storage_buffer_size(&payloads.accel_child_bytes),
        storage_buffer_size(&payloads.cache_brick_bytes),
        storage_buffer_size(&payloads.shape_meta_bytes),
        storage_buffer_size(&payloads.world_shape_bytes),
        storage_buffer_size(&payloads.continuation_seed_bytes),
        (QUERY_WGSL_OBSERVABILITY_U32S * std::mem::size_of::<u32>()) as u64,
    ]
    .into_iter()
    .max()
    .unwrap_or(4);
    let required_limit_request = WgslLimitRequest {
        max_storage_buffers_per_shader_stage: QUERY_WGSL_STORAGE_BUFFERS_PER_SHADER_STAGE,
        max_storage_buffer_binding_size: used_max_storage_buffer_bytes,
        ..WgslLimitRequest::default()
    };
    let native = native_wgpu_context_for_limits(required_limit_request)?;
    let mut profiler = GpuPassProfiler::new(&native, 1);
    let selected_workgroup_size = select_query_wgsl_workgroup_size(&native.adapter_limits)?;
    let diagnostics = WgslDispatchDiagnostics {
        selected_workgroup_size,
        used_max_storage_buffer_bytes,
        requested_max_storage_buffer_bytes: native.requested_limits.max_storage_buffer_binding_size,
        cache_observability_seed: generated.cache_observability_seed,
    };
    let outcome = dispatch_compiled_shader_with_buffers(
        generated,
        &request,
        &payloads,
        diagnostics,
        native.limit_request,
        &native,
        &mut profiler,
    )?;
    let gpu_elapsed_micros = profiler
        .readback_gpu_elapsed_micros(&native)
        .map_err(|message| QueryExecError::Unsupported {
            message: format!("native WGSL GPU timing readback failed: {message}"),
        })?;
    let mut gpu_runtime = outcome.gpu_runtime;
    gpu_runtime.note_context_metadata(&native);
    gpu_runtime.note_gpu_timings(profiler.timestamps_supported(), &gpu_elapsed_micros);
    gpu_runtime.queue_submit_count = gpu_runtime
        .queue_submit_count
        .saturating_add(2 + u32::from(profiler.timestamps_supported()));
    gpu_runtime.transient_buffer_creations = gpu_runtime
        .transient_buffer_creations
        .saturating_add(2 + u32::from(profiler.timestamps_supported()));
    gpu_runtime.readback_bytes = gpu_runtime
        .readback_bytes
        .saturating_add(result_buffer_size)
        .saturating_add((QUERY_WGSL_OBSERVABILITY_U32S * std::mem::size_of::<u32>()) as u64)
        .saturating_add(gpu_runtime.timestamped_pass_count as u64 * 16);

    Ok((
        decode_slice(
            &generated.result_abi,
            &outcome.result_bytes,
            request.items.len(),
        )?,
        decode_wgsl_observability(
            &diagnostics,
            &outcome.observability_bytes,
            request.items.len() as u32,
            outcome.layout_signature,
            gpu_runtime,
        ),
    ))
}

fn compute_wgsl_dispatch_chunk_plan(
    generated: &GeneratedShaderModule,
    request: &GpuDispatchRequest,
) -> Result<WgslDispatchChunkPlan, QueryExecError> {
    let item_count = request.items.len();
    if item_count == 0 {
        return Ok(WgslDispatchChunkPlan {
            items_per_chunk: 0,
            chunk_count: 0,
        });
    }

    let native = native_wgpu_context()?;
    let per_storage_buffer_limit = effective_chunk_storage_buffer_limit(&native.requested_limits);
    let item_stride = portable_abi_array_stride(&generated.item_abi) as u64;
    let result_stride = portable_abi_array_stride(&generated.result_abi) as u64;
    let per_item_side_channel_stride = side_channel_stride_for_chunking(request)?;
    let items_per_chunk = max_chunk_item_count(
        per_storage_buffer_limit,
        item_stride,
        result_stride,
        per_item_side_channel_stride,
    )?;
    let chunk_count = item_count.div_ceil(items_per_chunk);
    if !request.candidate_spans.is_empty()
        && request.candidate_spans.len() > request.items.len().saturating_mul(2)
        && chunk_count > 1
    {
        return Err(QueryExecError::Unsupported {
            message: format!(
                "WGSL batch chunking does not yet support slicing packed candidate span tables across {chunk_count} chunks"
            ),
        });
    }
    Ok(WgslDispatchChunkPlan {
        items_per_chunk,
        chunk_count,
    })
}

fn effective_chunk_storage_buffer_limit(requested_limits: &wgpu::Limits) -> u64 {
    #[cfg(test)]
    if let Some(limit) = TEST_WGSL_CHUNK_STORAGE_BUFFER_LIMIT_OVERRIDE.with(Cell::get) {
        return limit;
    }

    requested_limits
        .max_storage_buffer_binding_size
        .min(requested_limits.max_buffer_size)
}

#[cfg(test)]
fn with_test_chunk_storage_buffer_limit_override<T>(limit: u64, f: impl FnOnce() -> T) -> T {
    struct Reset(Option<u64>);

    impl Drop for Reset {
        fn drop(&mut self) {
            TEST_WGSL_CHUNK_STORAGE_BUFFER_LIMIT_OVERRIDE.with(|cell| cell.set(self.0));
        }
    }

    TEST_WGSL_CHUNK_STORAGE_BUFFER_LIMIT_OVERRIDE.with(|cell| {
        let previous = cell.replace(Some(limit));
        let _reset = Reset(previous);
        f()
    })
}

fn side_channel_stride_for_chunking(
    request: &GpuDispatchRequest,
) -> Result<Option<u64>, QueryExecError> {
    if !request.candidate_spans.is_empty() {
        if request.candidate_spans.len() == request.items.len().saturating_mul(2) {
            return Ok(Some((std::mem::size_of::<u32>() * 2) as u64));
        }
        if request.candidate_spans.len() >= request.items.len().saturating_mul(2) {
            return Ok(None);
        }
        return Err(QueryExecError::Unsupported {
            message: format!(
                "WGSL batch chunking requires packed candidate spans to include at least one span pair per item, found {} span values for {} items",
                request.candidate_spans.len(),
                request.items.len()
            ),
        });
    }
    if request.continuation_seeds.is_empty() {
        return Ok(None);
    }
    if request.continuation_seeds.len() == request.items.len() {
        return Ok(Some(std::mem::size_of::<u32>() as u64));
    }
    Err(QueryExecError::Unsupported {
        message: format!(
            "WGSL batch chunking requires continuation seeds or candidate spans to be empty or one-per-item, found {} side-channel values for {} items",
            request
                .continuation_seeds
                .len()
                .max(request.candidate_spans.len()),
            request.items.len()
        ),
    })
}

pub(crate) fn max_chunk_item_count(
    per_storage_buffer_limit: u64,
    item_stride: u64,
    result_stride: u64,
    per_item_side_channel_stride: Option<u64>,
) -> Result<usize, QueryExecError> {
    let mut item_limits = Vec::new();
    item_limits.push(max_items_for_stride(
        per_storage_buffer_limit,
        item_stride,
        "WGSL input item ABI",
    )?);
    item_limits.push(max_items_for_stride(
        per_storage_buffer_limit,
        result_stride,
        "WGSL result ABI",
    )?);
    if let Some(seed_stride) = per_item_side_channel_stride {
        item_limits.push(max_items_for_stride(
            per_storage_buffer_limit,
            seed_stride,
            "WGSL batch side channel ABI",
        )?);
    }
    let items_per_chunk = item_limits.into_iter().min().unwrap_or(1);
    Ok(items_per_chunk.max(1))
}

fn max_items_for_stride(
    per_storage_buffer_limit: u64,
    stride: u64,
    label: &str,
) -> Result<usize, QueryExecError> {
    if stride == 0 {
        return Err(QueryExecError::Unsupported {
            message: format!("{label} reported a zero byte stride"),
        });
    }
    let max_items = per_storage_buffer_limit / stride;
    if max_items == 0 {
        return Err(QueryExecError::Unsupported {
            message: format!(
                "{label} requires {stride} bytes per item but the adapter only allows {} bytes per storage buffer",
                per_storage_buffer_limit
            ),
        });
    }
    Ok(max_items as usize)
}

fn slice_gpu_dispatch_request(
    request: &GpuDispatchRequest,
    range: std::ops::Range<usize>,
) -> Result<GpuDispatchRequest, QueryExecError> {
    let range_start = range.start;
    let range_end = range.end;
    let mut dispatch = expect_struct_arg(Some(&request.dispatch), "WgslDispatchConfig")?.clone();
    if let Some((_, value)) = dispatch
        .fields
        .iter_mut()
        .find(|(name, _)| name == "item_count")
    {
        *value = KernelValue::U32(range.len() as u32);
    } else {
        return Err(QueryExecError::Unsupported {
            message: "WGSL dispatch config is missing item_count".to_string(),
        });
    }
    Ok(GpuDispatchRequest {
        dispatch: KernelValue::Struct(dispatch),
        items: request.items[range.clone()].to_vec(),
        world_shape_indices: request.world_shape_indices.clone(),
        accel_nodes: request.accel_nodes.clone(),
        accel_children: request.accel_children.clone(),
        cache_bricks: request.cache_bricks.clone(),
        continuation_seeds: if request.continuation_seeds.len() == request.items.len() {
            request.continuation_seeds[range_start..range_end].to_vec()
        } else {
            request.continuation_seeds.clone()
        },
        candidate_spans: if request.candidate_spans.len() == request.items.len().saturating_mul(2) {
            request.candidate_spans[range_start * 2..range_end * 2].to_vec()
        } else {
            request.candidate_spans.clone()
        },
        resident_scene_snapshot: request.resident_scene_snapshot.clone(),
        resident_scene_detail: request.resident_scene_detail,
        resident_scene_selection_signature: request.resident_scene_selection_signature,
    })
}

fn suppress_repeated_chunk_seed_metrics(observability: &mut QueryExecutionObservability) {
    observability.cache_resident_shared_snapshot_artifacts = 0;
    observability.cache_resident_observer_local_artifacts = 0;
    observability.cache_upload_attempts = 0;
    observability.cache_upload_rejections = 0;
}

fn dispatch_compiled_shader_with_buffers(
    generated: &GeneratedShaderModule,
    request: &GpuDispatchRequest,
    payloads: &WgslDispatchPayloadBytes,
    diagnostics: WgslDispatchDiagnostics,
    runtime_request: WgslLimitRequest,
    native: &NativeWgpuContext,
    profiler: &mut GpuPassProfiler,
) -> Result<WgslDispatchOutcome, QueryExecError> {
    if request.items.is_empty() {
        return Ok(WgslDispatchOutcome {
            result_bytes: Vec::new(),
            observability_bytes: Vec::new(),
            gpu_runtime: GpuRuntimeMetrics::default(),
            layout_signature: 0,
        });
    }
    let result_stride = portable_abi_array_stride(&generated.result_abi) as usize;
    let result_buffer_size = (result_stride * request.items.len()).max(result_stride.max(4)) as u64;
    let mut gpu_runtime = GpuRuntimeMetrics::default();
    gpu_runtime.note_context_metadata(native);
    let dispatch_buffer_size = storage_buffer_size(&payloads.dispatch_bytes);
    let input_buffer_size = storage_buffer_size(&payloads.input_bytes);
    let continuation_seed_buffer_size = storage_buffer_size(&payloads.continuation_seed_bytes);
    let observability_buffer_size =
        (QUERY_WGSL_OBSERVABILITY_U32S * std::mem::size_of::<u32>()) as u64;

    let cached = compiled_query_pipeline(
        &native,
        &generated.source,
        diagnostics.selected_workgroup_size,
        generated,
        &mut gpu_runtime,
    )?;
    let resident_scene_fingerprint = scene_fingerprint(
        cached.layout_identity.layout_signature,
        &payloads.accel_node_bytes,
        &payloads.accel_child_bytes,
        &payloads.cache_brick_bytes,
        &payloads.shape_meta_bytes,
    );
    let resident_world_shape_fingerprint = world_shape_fingerprint(
        cached.layout_identity.layout_signature,
        &payloads.world_shape_bytes,
    );
    let (world_shapes_buffer, scene_bind_group, scene_bind_group_created) =
        if let Some((scene, created)) = shared_resident_scene_for_request(
            cached.layout_identity,
            request,
            payloads,
            runtime_request,
            native,
            &cached.bind_group_layouts[GPU_RUNTIME_SCENE_BIND_GROUP_INDEX as usize],
        )? {
            if created {
                let scene_bytes = scene_upload_bytes(payloads);
                gpu_runtime.scene_reupload_bytes =
                    gpu_runtime.scene_reupload_bytes.saturating_add(scene_bytes);
                gpu_runtime.upload_bytes = gpu_runtime.upload_bytes.saturating_add(scene_bytes);
            }
            let (world_shapes_buffer, created_world_shapes) = pooled_storage_buffer(
                native,
                runtime_request,
                WgslBufferKind::SceneWorldShapes,
                storage_buffer_size(&payloads.world_shape_bytes),
                resident_world_shape_fingerprint,
                &mut gpu_runtime,
            )?;
            if created_world_shapes {
                let world_shape_bytes = world_shape_upload_bytes(payloads);
                gpu_runtime.scene_reupload_bytes = gpu_runtime
                    .scene_reupload_bytes
                    .saturating_add(world_shape_bytes);
                gpu_runtime.upload_bytes =
                    gpu_runtime.upload_bytes.saturating_add(world_shape_bytes);
            }
            (
                world_shapes_buffer,
                scene.payload.bind_group_scene.clone(),
                created,
            )
        } else {
            let (accel_nodes_buffer, created_accel_nodes) = pooled_storage_buffer(
                native,
                runtime_request,
                WgslBufferKind::SceneAccelNodes,
                storage_buffer_size(&payloads.accel_node_bytes),
                resident_scene_fingerprint,
                &mut gpu_runtime,
            )?;
            let (accel_children_buffer, created_accel_children) = pooled_storage_buffer(
                native,
                runtime_request,
                WgslBufferKind::SceneAccelChildren,
                storage_buffer_size(&payloads.accel_child_bytes),
                resident_scene_fingerprint,
                &mut gpu_runtime,
            )?;
            let (cache_bricks_buffer, created_cache_bricks) = pooled_storage_buffer(
                native,
                runtime_request,
                WgslBufferKind::SceneCacheBricks,
                storage_buffer_size(&payloads.cache_brick_bytes),
                resident_scene_fingerprint,
                &mut gpu_runtime,
            )?;
            let (shape_meta_buffer, created_shape_meta) = pooled_storage_buffer(
                native,
                runtime_request,
                WgslBufferKind::SceneShapeMeta,
                storage_buffer_size(&payloads.shape_meta_bytes),
                resident_scene_fingerprint,
                &mut gpu_runtime,
            )?;
            let (world_shapes_buffer, created_world_shapes) = pooled_storage_buffer(
                native,
                runtime_request,
                WgslBufferKind::SceneWorldShapes,
                storage_buffer_size(&payloads.world_shape_bytes),
                resident_world_shape_fingerprint,
                &mut gpu_runtime,
            )?;
            if created_accel_nodes {
                gpu_runtime.scene_reupload_bytes =
                    gpu_runtime
                        .scene_reupload_bytes
                        .saturating_add(write_pooled_buffer(
                            native,
                            &accel_nodes_buffer,
                            &payloads.accel_node_bytes,
                        ));
                gpu_runtime.upload_bytes = gpu_runtime
                    .upload_bytes
                    .saturating_add(storage_buffer_size(&payloads.accel_node_bytes));
            }
            if created_accel_children {
                gpu_runtime.scene_reupload_bytes =
                    gpu_runtime
                        .scene_reupload_bytes
                        .saturating_add(write_pooled_buffer(
                            native,
                            &accel_children_buffer,
                            &payloads.accel_child_bytes,
                        ));
                gpu_runtime.upload_bytes = gpu_runtime
                    .upload_bytes
                    .saturating_add(storage_buffer_size(&payloads.accel_child_bytes));
            }
            if created_cache_bricks {
                gpu_runtime.scene_reupload_bytes =
                    gpu_runtime
                        .scene_reupload_bytes
                        .saturating_add(write_pooled_buffer(
                            native,
                            &cache_bricks_buffer,
                            &payloads.cache_brick_bytes,
                        ));
                gpu_runtime.upload_bytes = gpu_runtime
                    .upload_bytes
                    .saturating_add(storage_buffer_size(&payloads.cache_brick_bytes));
            }
            if created_shape_meta {
                gpu_runtime.scene_reupload_bytes =
                    gpu_runtime
                        .scene_reupload_bytes
                        .saturating_add(write_pooled_buffer(
                            native,
                            &shape_meta_buffer,
                            &payloads.shape_meta_bytes,
                        ));
                gpu_runtime.upload_bytes = gpu_runtime
                    .upload_bytes
                    .saturating_add(storage_buffer_size(&payloads.shape_meta_bytes));
            }
            if created_world_shapes {
                gpu_runtime.scene_reupload_bytes =
                    gpu_runtime
                        .scene_reupload_bytes
                        .saturating_add(write_pooled_buffer(
                            native,
                            &world_shapes_buffer,
                            &payloads.world_shape_bytes,
                        ));
                gpu_runtime.upload_bytes = gpu_runtime
                    .upload_bytes
                    .saturating_add(storage_buffer_size(&payloads.world_shape_bytes));
            }
            let scene_bind_group_key = WgslSceneBindGroupKey {
                limits: runtime_request,
                pipeline_signature: pipeline_signature(
                    &generated.source,
                    diagnostics.selected_workgroup_size,
                    cached.layout_identity.layout_signature,
                    runtime_request,
                ),
                scene_fingerprint: resident_scene_fingerprint,
            };
            let (scene_bind_group, scene_bind_group_created) = {
                let cache = scene_bind_group_cache();
                let mut guard = cache.lock().unwrap_or_else(|poison| poison.into_inner());
                if let Some(bind_group) = guard.get(&scene_bind_group_key) {
                    (bind_group.clone(), false)
                } else {
                    let bind_group = native.device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("wrela.wgsl.query.group0"),
                        layout: &cached.bind_group_layouts
                            [GPU_RUNTIME_SCENE_BIND_GROUP_INDEX as usize],
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: accel_nodes_buffer.as_entire_binding(),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: accel_children_buffer.as_entire_binding(),
                            },
                            wgpu::BindGroupEntry {
                                binding: 2,
                                resource: shape_meta_buffer.as_entire_binding(),
                            },
                            wgpu::BindGroupEntry {
                                binding: 3,
                                resource: cache_bricks_buffer.as_entire_binding(),
                            },
                        ],
                    });
                    let entry = guard
                        .entry(scene_bind_group_key)
                        .or_insert_with(|| bind_group.clone());
                    (entry.clone(), true)
                }
            };
            (
                world_shapes_buffer,
                scene_bind_group,
                scene_bind_group_created,
            )
        };
    let scene_token = resident_world_shape_fingerprint;
    let dynamic_resources_key = WgslDynamicResourcesKey {
        limits: runtime_request,
        pipeline_signature: pipeline_signature(
            &generated.source,
            diagnostics.selected_workgroup_size,
            cached.layout_identity.layout_signature,
            runtime_request,
        ),
        scene_token,
        dispatch_buffer_size,
        input_buffer_size,
        output_buffer_size: result_buffer_size,
        observability_buffer_size,
        continuation_buffer_size: continuation_seed_buffer_size,
    };
    let (_dynamic_resources_mutex, dynamic_resources, dynamic_resources_created) =
        lock_query_dynamic_resources(native, dynamic_resources_key, &cached, &world_shapes_buffer);
    if dynamic_resources_created {
        gpu_runtime.transient_buffer_creations =
            gpu_runtime.transient_buffer_creations.saturating_add(5);
    }
    gpu_runtime.transient_bind_group_creations =
        u32::from(scene_bind_group_created) + if dynamic_resources_created { 3 } else { 0 };

    let mut upload_arena = lock_shared_upload_arena(
        runtime_request,
        &native.device,
        [
            dispatch_buffer_size,
            input_buffer_size,
            observability_buffer_size,
            continuation_seed_buffer_size,
        ]
        .into_iter()
        .max()
        .unwrap_or(4),
    );
    upload_arena.set_scratch_encoder(native.device.create_command_encoder(
        &wgpu::CommandEncoderDescriptor {
            label: Some("wrela.wgsl.upload_encoder"),
        },
    ));
    gpu_runtime.upload_bytes = gpu_runtime
        .upload_bytes
        .saturating_add(
            upload_arena
                .write_storage_bytes(
                    &dynamic_resources.dispatch_buffer,
                    0,
                    &payloads.dispatch_bytes,
                )
                .map_err(|err| QueryExecError::Unsupported {
                    message: format!("WGSL dispatch upload failed: {err:?}"),
                })?,
        )
        .saturating_add(
            upload_arena
                .write_storage_bytes(&dynamic_resources.input_buffer, 0, &payloads.input_bytes)
                .map_err(|err| QueryExecError::Unsupported {
                    message: format!("WGSL input upload failed: {err:?}"),
                })?,
        )
        .saturating_add(
            upload_arena
                .write_storage_bytes(
                    &dynamic_resources.observability_buffer,
                    0,
                    &[0u8; QUERY_WGSL_OBSERVABILITY_U32S * std::mem::size_of::<u32>()],
                )
                .map_err(|err| QueryExecError::Unsupported {
                    message: format!("WGSL observability upload failed: {err:?}"),
                })?,
        )
        .saturating_add(
            upload_arena
                .write_storage_bytes(
                    &dynamic_resources.continuation_seed_buffer,
                    0,
                    &payloads.continuation_seed_bytes,
                )
                .map_err(|err| QueryExecError::Unsupported {
                    message: format!("WGSL continuation upload failed: {err:?}"),
                })?,
        );
    if let Some(upload_commands) = upload_arena.finish() {
        native.queue.submit(Some(upload_commands));
        gpu_runtime.queue_submit_count = gpu_runtime.queue_submit_count.saturating_add(1);
    }

    let mut encoder = native
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("wrela.wgsl.encoder"),
        });
    {
        let timestamp_writes = profiler.compute_pass_timestamp_writes();
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("wrela.wgsl.compute_pass"),
            timestamp_writes,
        });
        pass.set_pipeline(&cached.pipeline);
        pass.set_bind_group(GPU_RUNTIME_SCENE_BIND_GROUP_INDEX, &scene_bind_group, &[]);
        pass.set_bind_group(
            GPU_RUNTIME_FRAME_BIND_GROUP_INDEX,
            &dynamic_resources.frame_bind_group,
            &[],
        );
        pass.set_bind_group(
            GPU_RUNTIME_PASS_BIND_GROUP_INDEX,
            &dynamic_resources.pass_bind_group,
            &[],
        );
        pass.set_bind_group(
            GPU_RUNTIME_SCRATCH_BIND_GROUP_INDEX,
            &dynamic_resources.scratch_bind_group,
            &[],
        );
        pass.dispatch_workgroups(
            dispatch_workgroups_x_for_items(
                request.items.len() as u32,
                diagnostics.selected_workgroup_size,
            ),
            1,
            1,
        );
    }
    profiler.resolve_into(&mut encoder);
    native.queue.submit(Some(encoder.finish()));
    gpu_runtime.queue_submit_count = gpu_runtime.queue_submit_count.saturating_add(1);
    let result_bytes = readback_storage_buffer_on(
        &native,
        &dynamic_resources.output_buffer,
        result_buffer_size,
    )?;
    let observability_bytes = readback_storage_buffer_on(
        &native,
        &dynamic_resources.observability_buffer,
        observability_buffer_size,
    )?;
    upload_arena.recall();
    Ok(WgslDispatchOutcome {
        result_bytes,
        observability_bytes,
        gpu_runtime,
        layout_signature: cached.layout_identity.layout_signature,
    })
}

fn dispatch_workgroups_x_for_items(item_count: u32, workgroup_size: u32) -> u32 {
    item_count.div_ceil(workgroup_size.max(1))
}

// Legacy/test-only helper for CPU-bounce WGSL validation paths. Do not use this from the
// timed resident frame path; use explicit readback scheduling instead.
pub(crate) fn legacy_test_only_readback_storage_buffer(
    buffer: &wgpu::Buffer,
    size: u64,
) -> Result<Vec<u8>, QueryExecError> {
    let native = native_wgpu_context()?;
    readback_storage_buffer_on(&native, buffer, size)
}

pub(crate) fn readback_storage_buffer_on(
    native: &NativeWgpuContext,
    buffer: &wgpu::Buffer,
    size: u64,
) -> Result<Vec<u8>, QueryExecError> {
    shared_readback_storage_buffer_on(native, buffer, size).map_err(|message| {
        QueryExecError::Unsupported {
            message: format!("native WGSL readback failed: {message}"),
        }
    })
}

pub(crate) fn compiled_pipeline(
    native: &NativeWgpuContext,
    source: &str,
    workgroup_size: u32,
    bind_group_index: u32,
    dispatch_min_size: Option<wgpu::BufferSize>,
    gpu_runtime: &mut GpuRuntimeMetrics,
) -> Result<CachedPipeline, QueryExecError> {
    if bind_group_index >= GPU_RUNTIME_BIND_GROUP_COUNT {
        return Err(QueryExecError::Unsupported {
            message: format!(
                "native WGSL bind group index {bind_group_index} exceeds shared runtime convention count {GPU_RUNTIME_BIND_GROUP_COUNT}"
            ),
        });
    }
    let role = match bind_group_index {
        GPU_RUNTIME_PASS_BIND_GROUP_INDEX => GpuBindGroupRole::Pass,
        GPU_RUNTIME_FRAME_BIND_GROUP_INDEX => GpuBindGroupRole::Frame,
        GPU_RUNTIME_SCENE_BIND_GROUP_INDEX => GpuBindGroupRole::SceneStatic,
        GPU_RUNTIME_SCRATCH_BIND_GROUP_INDEX => GpuBindGroupRole::Scratch,
        _ => unreachable!("bind group index already range checked"),
    };
    let entries = [
        wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: dispatch_min_size,
            },
            count: None,
        },
        wgpu::BindGroupLayoutEntry {
            binding: 1,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        },
        wgpu::BindGroupLayoutEntry {
            binding: 2,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: false },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        },
        wgpu::BindGroupLayoutEntry {
            binding: 3,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        },
    ];
    let descriptor = wgpu::BindGroupLayoutDescriptor {
        label: Some("wrela.wgsl.bind_group_layout"),
        entries: &entries,
    };
    let bind_group_layout_signature = bind_group_layout_signature_for_role(role, &descriptor);
    let mut bind_group_layout_signatures = [0u64; GPU_RUNTIME_BIND_GROUP_COUNT as usize];
    bind_group_layout_signatures[bind_group_index as usize] = bind_group_layout_signature;
    let layout_key = PipelineLayoutKey::from_bind_group_layout_signatures(
        &bind_group_layout_signatures,
        0,
        native.feature_mask(),
    );
    let key = WgslPipelineCacheKey {
        limits: native.limit_request,
        pipeline: ComputePipelineKey::from_shader_source(
            layout_key.clone(),
            source,
            "main",
            workgroup_size,
        ),
    };
    let cache = capture_pipeline_cache();

    {
        let guard = cache.lock().unwrap_or_else(|poison| poison.into_inner());
        if let Some(cached) = guard.get(&key) {
            gpu_runtime.pipeline_cache_hits = gpu_runtime.pipeline_cache_hits.saturating_add(1);
            return Ok(cached.clone());
        }
    }
    gpu_runtime.pipeline_cache_misses = gpu_runtime.pipeline_cache_misses.saturating_add(1);

    let bind_group_layout = native.device.create_bind_group_layout(&descriptor);
    let pipeline_layout = native
        .device
        .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("wrela.wgsl.pipeline_layout"),
            bind_group_layouts: &{
                let mut layouts = [None; GPU_RUNTIME_BIND_GROUP_COUNT as usize];
                layouts[bind_group_index as usize] = Some(&bind_group_layout);
                layouts
            },
            immediate_size: 0,
        });
    let pipeline = create_compute_pipeline(
        native,
        source,
        workgroup_size,
        &pipeline_layout,
        "wrela.wgsl.pipeline",
    )?;

    let cached = CachedPipeline {
        bind_group_layout,
        pipeline,
    };
    let mut guard = cache.lock().unwrap_or_else(|poison| poison.into_inner());
    Ok(guard.entry(key).or_insert_with(|| cached.clone()).clone())
}

fn compiled_query_pipeline(
    native: &NativeWgpuContext,
    source: &str,
    workgroup_size: u32,
    generated: &GeneratedShaderModule,
    gpu_runtime: &mut GpuRuntimeMetrics,
) -> Result<QueryCachedPipeline, QueryExecError> {
    let frame_entries = [wgpu::BindGroupLayoutEntry {
        binding: 0,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only: true },
            has_dynamic_offset: false,
            min_binding_size: wgpu::BufferSize::new(
                portable_abi_layout(&generated.dispatch_abi).size as u64,
            ),
        },
        count: None,
    }];
    let frame_descriptor = wgpu::BindGroupLayoutDescriptor {
        label: Some("wrela.wgsl.query.group1"),
        entries: &frame_entries,
    };
    let scene_entries = [
        wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: wgpu::BufferSize::new(
                    portable_abi_layout(&generated.accel_node_abi).size as u64,
                ),
            },
            count: None,
        },
        wgpu::BindGroupLayoutEntry {
            binding: 1,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: wgpu::BufferSize::new(4),
            },
            count: None,
        },
        wgpu::BindGroupLayoutEntry {
            binding: 2,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: wgpu::BufferSize::new(
                    portable_abi_layout(&generated.shape_meta_abi).size as u64,
                ),
            },
            count: None,
        },
        wgpu::BindGroupLayoutEntry {
            binding: 3,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: wgpu::BufferSize::new(
                    portable_abi_layout(&generated.cache_brick_abi).size as u64,
                ),
            },
            count: None,
        },
    ];
    let scene_descriptor = wgpu::BindGroupLayoutDescriptor {
        label: Some("wrela.wgsl.query.group0"),
        entries: &scene_entries,
    };
    let pass_entries = [
        wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        },
        wgpu::BindGroupLayoutEntry {
            binding: 1,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: false },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        },
        wgpu::BindGroupLayoutEntry {
            binding: 2,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: wgpu::BufferSize::new(4),
            },
            count: None,
        },
        wgpu::BindGroupLayoutEntry {
            binding: 3,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: false },
                has_dynamic_offset: false,
                min_binding_size: wgpu::BufferSize::new(
                    (QUERY_WGSL_OBSERVABILITY_U32S * std::mem::size_of::<u32>()) as u64,
                ),
            },
            count: None,
        },
    ];
    let pass_descriptor = wgpu::BindGroupLayoutDescriptor {
        label: Some("wrela.wgsl.query.group2"),
        entries: &pass_entries,
    };
    let scratch_entries = [wgpu::BindGroupLayoutEntry {
        binding: 0,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only: true },
            has_dynamic_offset: false,
            min_binding_size: wgpu::BufferSize::new(4),
        },
        count: None,
    }];
    let scratch_descriptor = wgpu::BindGroupLayoutDescriptor {
        label: Some("wrela.wgsl.query.group3"),
        entries: &scratch_entries,
    };
    let bind_group_layout_signatures = [
        bind_group_layout_signature_for_role(GpuBindGroupRole::SceneStatic, &scene_descriptor),
        bind_group_layout_signature_for_role(GpuBindGroupRole::Frame, &frame_descriptor),
        bind_group_layout_signature_for_role(GpuBindGroupRole::Pass, &pass_descriptor),
        bind_group_layout_signature_for_role(GpuBindGroupRole::Scratch, &scratch_descriptor),
    ];
    let layout_key = PipelineLayoutKey::from_bind_group_layout_signatures(
        &bind_group_layout_signatures,
        0,
        native.feature_mask(),
    );
    let key = WgslPipelineCacheKey {
        limits: native.limit_request,
        pipeline: ComputePipelineKey::from_shader_source(
            layout_key.clone(),
            source,
            "main",
            workgroup_size,
        ),
    };
    let cache = query_pipeline_cache();

    {
        let guard = cache.lock().unwrap_or_else(|poison| poison.into_inner());
        if let Some(cached) = guard.get(&key) {
            gpu_runtime.pipeline_cache_hits = gpu_runtime.pipeline_cache_hits.saturating_add(1);
            return Ok(cached.clone());
        }
    }
    gpu_runtime.pipeline_cache_misses = gpu_runtime.pipeline_cache_misses.saturating_add(1);

    let frame_layout = native.device.create_bind_group_layout(&frame_descriptor);
    let scene_layout = native.device.create_bind_group_layout(&scene_descriptor);
    let pass_layout = native.device.create_bind_group_layout(&pass_descriptor);
    let scratch_layout = native.device.create_bind_group_layout(&scratch_descriptor);
    let pipeline_layout = native
        .device
        .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("wrela.wgsl.query.pipeline_layout"),
            bind_group_layouts: &[
                Some(&scene_layout),
                Some(&frame_layout),
                Some(&pass_layout),
                Some(&scratch_layout),
            ],
            immediate_size: 0,
        });
    let pipeline = create_compute_pipeline(
        native,
        source,
        workgroup_size,
        &pipeline_layout,
        "wrela.wgsl.query.pipeline",
    )?;

    let cached = QueryCachedPipeline {
        bind_group_layouts: [scene_layout, frame_layout, pass_layout, scratch_layout],
        layout_identity: layout_key.layout,
        pipeline,
    };
    let mut guard = cache.lock().unwrap_or_else(|poison| poison.into_inner());
    Ok(guard.entry(key).or_insert_with(|| cached.clone()).clone())
}

fn create_compute_pipeline(
    native: &NativeWgpuContext,
    source: &str,
    workgroup_size: u32,
    pipeline_layout: &wgpu::PipelineLayout,
    label: &str,
) -> Result<wgpu::ComputePipeline, QueryExecError> {
    let shader_module = native
        .device
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(label),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(source)),
        });
    let error_scope = native
        .device
        .push_error_scope(wgpu::ErrorFilter::Validation);
    let pipeline = native
        .device
        .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some(label),
            layout: Some(pipeline_layout),
            module: &shader_module,
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions {
                constants: &[("WG_SIZE", workgroup_size as f64)],
                zero_initialize_workgroup_memory: true,
            },
            cache: None,
        });
    native
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(wgpu_poll_error)?;
    if let Some(err) = pollster::block_on(error_scope.pop()) {
        return Err(QueryExecError::Unsupported {
            message: format!("native WGSL validation failed: {err}"),
        });
    }
    Ok(pipeline)
}

fn decode_wgsl_observability(
    diagnostics: &WgslDispatchDiagnostics,
    bytes: &[u8],
    dispatch_items: u32,
    layout_signature: u64,
    gpu_runtime: GpuRuntimeMetrics,
) -> QueryExecutionObservability {
    let read_u32 = |index: usize| -> u32 {
        let start = index * std::mem::size_of::<u32>();
        let end = start + std::mem::size_of::<u32>();
        bytes
            .get(start..end)
            .and_then(|slice| slice.try_into().ok())
            .map(u32::from_le_bytes)
            .unwrap_or_default()
    };
    let mut observability =
        summary_wgsl_observability(diagnostics, dispatch_items, layout_signature, gpu_runtime);
    observability.acceleration_node_visits = read_u32(0);
    observability.shape_leaf_visits = read_u32(1);
    observability.acceleration_pruned_nodes = read_u32(2);
    observability.ray_support_interval_rejections = read_u32(3);
    observability.ray_support_entry_jumps = read_u32(4);
    observability.cache_brick_visits = read_u32(5);
    observability.cache_brick_hits = read_u32(6);
    observability.cache_brick_misses = read_u32(7);
    observability.cache_interval_advances = read_u32(8);
    observability.cache_resident_shared_snapshot_artifacts = read_u32(9);
    observability.cache_resident_observer_local_artifacts = read_u32(10);
    observability.cache_upload_attempts = read_u32(11);
    observability.cache_upload_rejections = read_u32(12);
    observability.cache_budget_rejections = read_u32(13);
    observability.cache_dense_fallback_rays = read_u32(14);
    observability.solver_analytic_hits = read_u32(15);
    observability.solver_generated_dense_fallback_rays = read_u32(16);
    observability.solver_support_rejections = read_u32(17);
    observability.field_samples = read_u32(18);
    observability
}

fn summary_wgsl_observability(
    diagnostics: &WgslDispatchDiagnostics,
    dispatch_items: u32,
    layout_signature: u64,
    gpu_runtime: GpuRuntimeMetrics,
) -> QueryExecutionObservability {
    QueryExecutionObservability {
        cache_resident_shared_snapshot_artifacts: diagnostics
            .cache_observability_seed
            .resident_shared_snapshot_artifacts,
        cache_resident_observer_local_artifacts: diagnostics
            .cache_observability_seed
            .resident_observer_local_artifacts,
        cache_upload_attempts: diagnostics.cache_observability_seed.upload_attempts,
        cache_upload_rejections: diagnostics.cache_observability_seed.upload_rejections,
        dispatch_count: 1,
        dispatch_items,
        dispatch_workgroups_x: dispatch_workgroups_x_for_items(
            dispatch_items,
            diagnostics.selected_workgroup_size,
        ),
        dispatch_workgroups_y: 1,
        dispatch_workgroups_z: 1,
        wgsl_layout_signature: Some(layout_signature),
        wgsl_bind_group_count: GPU_RUNTIME_BIND_GROUP_COUNT,
        wgsl_requested_max_storage_buffer_bytes: diagnostics.requested_max_storage_buffer_bytes,
        wgsl_used_max_storage_buffer_bytes: diagnostics.used_max_storage_buffer_bytes,
        wgsl_selected_workgroup_size: diagnostics.selected_workgroup_size,
        gpu_runtime,
        ..QueryExecutionObservability::default()
    }
}

fn validate_generated_shader(source: &str) -> Result<(), QueryExecError> {
    let module =
        naga::front::wgsl::parse_str(source).map_err(|err| QueryExecError::Unsupported {
            message: format!("native WGSL parse failed: {err}"),
        })?;
    Validator::new(ValidationFlags::all(), Capabilities::all())
        .validate(&module)
        .map_err(|err| QueryExecError::Unsupported {
            message: format!("native WGSL validation failed: {err:?}"),
        })?;
    Ok(())
}

pub(crate) fn native_wgpu_context() -> Result<Arc<NativeWgpuContext>, QueryExecError> {
    native_wgpu_context_for_limits(WgslLimitRequest {
        f16_enabled: requested_shader_f16_feature(),
        ..WgslLimitRequest::default()
    })
}

fn requested_shader_f16_feature() -> bool {
    if let Some(enabled) = SHADER_F16_OVERRIDE.with(Cell::get) {
        return enabled;
    }
    matches!(
        env::var("WRELA_PRESENTATION_SHADER_F16"),
        Ok(value) if matches!(value.trim(), "1" | "true" | "TRUE" | "yes" | "YES" | "on" | "ON")
    )
}

fn requested_timestamp_query_feature() -> bool {
    if let Some(enabled) = TIMESTAMP_QUERY_OVERRIDE.with(Cell::get) {
        return enabled;
    }
    matches!(
        env::var(QUERY_GPU_TIMESTAMPS_ENV),
        Ok(value) if matches!(value.trim(), "1" | "true" | "TRUE" | "yes" | "YES" | "on" | "ON")
    )
}

fn query_runtime_limit_request(
    required_request: WgslLimitRequest,
) -> Result<WgslLimitRequest, QueryExecError> {
    let shader_f16_enabled = required_request.f16_enabled || requested_shader_f16_feature();
    let timestamps_enabled =
        required_request.timestamps_enabled || requested_timestamp_query_feature();
    let adapter_context = shared_wgpu_context(WgslLimitRequest::default()).map_err(|message| {
        QueryExecError::Unsupported {
            message: format!("native WGSL backend initialization failed: {message}"),
        }
    })?;
    if QUERY_WGSL_STORAGE_BUFFERS_PER_SHADER_STAGE
        > adapter_context
            .adapter_limits
            .max_storage_buffers_per_shader_stage
    {
        return Err(QueryExecError::Unsupported {
            message: format!(
                "native WGSL runtime requires {} storage buffers per shader stage but adapter only supports {}",
                QUERY_WGSL_STORAGE_BUFFERS_PER_SHADER_STAGE,
                adapter_context
                    .adapter_limits
                    .max_storage_buffers_per_shader_stage
            ),
        });
    }
    if required_request.max_storage_buffer_binding_size
        > adapter_context
            .adapter_limits
            .max_storage_buffer_binding_size
    {
        return Err(QueryExecError::Unsupported {
            message: format!(
                "native WGSL runtime requires storage buffer binding size {} but adapter only supports {}",
                required_request.max_storage_buffer_binding_size,
                adapter_context
                    .adapter_limits
                    .max_storage_buffer_binding_size
            ),
        });
    }
    Ok(WgslLimitRequest {
        max_storage_buffers_per_shader_stage: QUERY_WGSL_STORAGE_BUFFERS_PER_SHADER_STAGE,
        // Keep query WGSL on a stable adapter-max device so resident scenes and
        // cached pipelines survive dispatch-size changes.
        max_storage_buffer_binding_size: adapter_context
            .adapter_limits
            .max_storage_buffer_binding_size,
        timestamps_enabled,
        f16_enabled: shader_f16_enabled,
        ..WgslLimitRequest::default()
    })
}

fn native_wgpu_context_for_limits(
    required_request: WgslLimitRequest,
) -> Result<Arc<NativeWgpuContext>, QueryExecError> {
    let runtime_request = query_runtime_limit_request(required_request)?;
    shared_wgpu_context(runtime_request).map_err(|message| QueryExecError::Unsupported {
        message: format!("native WGSL backend initialization failed: {message}"),
    })
}

fn validation_error(label: &str, errors: Vec<KernelValidationError>) -> QueryExecError {
    let messages = errors
        .into_iter()
        .map(|error| error.message)
        .collect::<Vec<_>>()
        .join("; ");
    QueryExecError::Unsupported {
        message: format!("native WGSL contract validation failed for {label}: {messages}"),
    }
}

fn field_index(
    ctx: &crate::query_exec::context::QueryExecContext,
    name: &SmolStr,
) -> Result<u32, QueryExecError> {
    ctx.scene
        .fields
        .keys()
        .enumerate()
        .find_map(|(index, candidate)| (candidate == name).then_some(index as u32))
        .ok_or_else(|| QueryExecError::MissingField { name: name.clone() })
}

fn shape_index(
    ctx: &crate::query_exec::context::QueryExecContext,
    name: &SmolStr,
) -> Result<u32, QueryExecError> {
    ctx.scene
        .shapes
        .keys()
        .enumerate()
        .find_map(|(index, candidate)| (candidate == name).then_some(index as u32))
        .ok_or_else(|| QueryExecError::MissingShape { name: name.clone() })
}

pub(crate) fn dispatch_config(
    capture_kind: u32,
    capture_index: u32,
    item_count: u32,
    shape_count: u32,
    accel_root_index: u32,
    accel_node_count: u32,
    cache_brick_count: u32,
    material_enabled: bool,
    radiance_enabled: bool,
    media_enabled: bool,
    candidate_spans_enabled: bool,
) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("WgslDispatchConfig"),
        fields: vec![
            (SmolStr::new("capture_kind"), KernelValue::U32(capture_kind)),
            (
                SmolStr::new("capture_index"),
                KernelValue::U32(capture_index),
            ),
            (SmolStr::new("item_count"), KernelValue::U32(item_count)),
            (SmolStr::new("shape_count"), KernelValue::U32(shape_count)),
            (
                SmolStr::new("accel_root_index"),
                KernelValue::U32(accel_root_index),
            ),
            (
                SmolStr::new("accel_node_count"),
                KernelValue::U32(accel_node_count),
            ),
            (
                SmolStr::new("cache_brick_count"),
                KernelValue::U32(cache_brick_count),
            ),
            (
                SmolStr::new("material_enabled"),
                KernelValue::Bool(material_enabled),
            ),
            (
                SmolStr::new("radiance_enabled"),
                KernelValue::Bool(radiance_enabled),
            ),
            (
                SmolStr::new("media_enabled"),
                KernelValue::Bool(media_enabled),
            ),
            (
                SmolStr::new("candidate_spans_enabled"),
                KernelValue::Bool(candidate_spans_enabled),
            ),
        ],
    })
}

fn point_query(point: [f32; 3]) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("PointQuery"),
        fields: vec![(SmolStr::new("point"), KernelValue::Vec3(point))],
    })
}

pub(crate) fn encode_shape_indices(indices: &[u32]) -> Result<Vec<u8>, QueryExecError> {
    let values = if indices.is_empty() {
        vec![KernelValue::U32(0)]
    } else {
        indices.iter().copied().map(KernelValue::U32).collect()
    };
    encode_slice(&PortableAbiType::U32, &values)
}

pub(crate) fn encode_value(
    abi: &PortableAbiType,
    value: &KernelValue,
) -> Result<Vec<u8>, QueryExecError> {
    portable_abi_encode_value(abi, value).map_err(portable_abi_error)
}

pub(crate) fn encode_slice(
    abi: &PortableAbiType,
    values: &[KernelValue],
) -> Result<Vec<u8>, QueryExecError> {
    portable_abi_encode_slice(abi, values).map_err(portable_abi_error)
}

pub(crate) fn decode_slice(
    abi: &PortableAbiType,
    bytes: &[u8],
    len: usize,
) -> Result<Vec<KernelValue>, QueryExecError> {
    portable_abi_decode_slice(abi, bytes, len).map_err(portable_abi_error)
}

fn portable_abi_error(err: crate::portable::PortableAbiError) -> QueryExecError {
    QueryExecError::Unsupported {
        message: format!("native WGSL ABI conversion failed: {err}"),
    }
}

pub(crate) fn wgpu_poll_error(err: wgpu::PollError) -> QueryExecError {
    QueryExecError::Unsupported {
        message: format!("native WGSL device poll failed: {err}"),
    }
}

fn expect_array_arg<'a>(
    value: Option<&'a KernelValue>,
    name: &'static str,
) -> Result<&'a [KernelValue], QueryExecError> {
    match value {
        Some(KernelValue::Array(values)) => Ok(values),
        Some(other) => Err(QueryExecError::TypeMismatch {
            expected: format!("Array for {name}"),
            found: format!("{other:?}"),
        }),
        None => Err(QueryExecError::MissingCaptureTarget { kind: name }),
    }
}

fn expect_struct_arg<'a>(
    value: Option<&'a KernelValue>,
    name: &'static str,
) -> Result<&'a KernelStructValue, QueryExecError> {
    match value {
        Some(KernelValue::Struct(value)) if value.name.as_str() == name => Ok(value),
        Some(other) => Err(QueryExecError::TypeMismatch {
            expected: name.to_string(),
            found: format!("{other:?}"),
        }),
        None => Err(QueryExecError::MissingCaptureTarget { kind: name }),
    }
}

fn struct_field<'a>(value: &'a KernelStructValue, field: &str) -> Option<&'a KernelValue> {
    value
        .fields
        .iter()
        .find_map(|(name, value)| (name.as_str() == field).then_some(value))
}

fn expect_struct_bool(value: &KernelStructValue, field: &str) -> Result<bool, QueryExecError> {
    let Some(value) = struct_field(value, field) else {
        return Err(QueryExecError::MissingCaptureTarget {
            kind: "struct field",
        });
    };
    match value {
        KernelValue::Bool(value) => Ok(*value),
        other => Err(QueryExecError::TypeMismatch {
            expected: format!("Bool for field {field}"),
            found: format!("{other:?}"),
        }),
    }
}

fn expect_struct_u32(value: &KernelStructValue, field: &str) -> Result<u32, QueryExecError> {
    let Some(value) = struct_field(value, field) else {
        return Err(QueryExecError::MissingCaptureTarget {
            kind: "struct field",
        });
    };
    match value {
        KernelValue::U32(value) => Ok(*value),
        KernelValue::I32(value) if *value >= 0 => Ok(*value as u32),
        other => Err(QueryExecError::TypeMismatch {
            expected: format!("U32 for field {field}"),
            found: format!("{other:?}"),
        }),
    }
}

fn expect_struct_i32(value: &KernelStructValue, field: &str) -> Result<i32, QueryExecError> {
    let Some(value) = struct_field(value, field) else {
        return Err(QueryExecError::MissingCaptureTarget {
            kind: "struct field",
        });
    };
    match value {
        KernelValue::I32(value) => Ok(*value),
        KernelValue::U32(value) if *value <= i32::MAX as u32 => Ok(*value as i32),
        other => Err(QueryExecError::TypeMismatch {
            expected: format!("I32 for field {field}"),
            found: format!("{other:?}"),
        }),
    }
}

fn expect_vec3_arg(
    value: Option<&KernelValue>,
    name: &'static str,
) -> Result<[f32; 3], QueryExecError> {
    match value {
        Some(KernelValue::Vec3(value)) => Ok(*value),
        Some(other) => Err(QueryExecError::TypeMismatch {
            expected: format!("Vec3 for {name}"),
            found: format!("{other:?}"),
        }),
        None => Err(QueryExecError::MissingCaptureTarget { kind: name }),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        GeneratedShaderModule, GpuDispatchRequest, WgslDispatchChunkPlan, WgslLimitRequest,
        WgslPipelineCacheKey, annotate_wgsl_world_helper_path_for_world,
        build_batch_request_for_shader, compile_batch_shader, compile_world_shader,
        dispatch_compiled_shader_with_observability, dispatch_config,
        dispatch_workgroups_x_for_items, max_chunk_item_count, native_wgsl_test_lock,
        normalized_dispatch_config, slice_gpu_dispatch_request,
        suppress_repeated_chunk_seed_metrics, with_test_chunk_storage_buffer_limit_override,
    };
    use crate::gpu_runtime::{ComputePipelineKey, GpuLayoutIdentity, PipelineLayoutKey};
    use crate::gpu_runtime::{GpuPassProfiler, readback::GpuReadbackPolicy};
    use crate::hir;
    use crate::hir::lower as hir_lower;
    use crate::kernel::{
        KernelStructValue, KernelValue, lower_batch_query_plan, lower_world_query_plan,
    };
    use crate::parser::ast;
    use crate::parser::ast::AstNode;
    use crate::parser::parse;
    use crate::portable::{PortableAbiType, portable_abi_emit_wgsl_structs};
    use crate::query_contract;
    use crate::query_exec::QueryExecContext;
    use crate::query_exec::QueryExecutionObservability;
    use crate::query_exec::gpu_dispatch::GpuQueryDispatcher;
    use crate::query_exec::ids::{stable_region_scene_capture_id, stable_region_snapshot_handle};
    use crate::query_exec::wgsl::codegen::{
        wgsl_accel_node_abi, wgsl_cache_brick_abi, wgsl_dispatch_config_abi, wgsl_shape_meta_abi,
    };
    use crate::query_plan::{
        BatchQueryPlan, DispatchBackend, WorldQueryKind, WorldQueryPlan, world_query_contract_id,
    };
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

    fn accelerated_world_helper_fixture_source() -> &'static str {
        r#"
field exact distance near_field(p: Vec3) -> F32 {
    translate = vec3(-6.0, 0.0, 0.0) {
        sphere(radius = 0.45)
    }
}

field exact distance mid_a_field(p: Vec3) -> F32 {
    translate = vec3(-3.0, 0.0, 0.0) {
        sphere(radius = 0.45)
    }
}

field exact distance mid_b_field(p: Vec3) -> F32 {
    translate = vec3(0.0, 0.0, 0.0) {
        sphere(radius = 0.45)
    }
}

field exact distance focus_field(p: Vec3) -> F32 {
    translate = vec3(6.0, 0.0, 0.0) {
        sphere(radius = 0.45)
    }
}

material shade(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(0.25, 0.35, 0.45),
        roughness=0.5,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

radiance field glow(p: Vec3, direction: Vec3, feature_id: U32) -> Vec3 {
    return vec3(0.1, 0.2, 0.3) + direction * 0.0 + vec3(f32(feature_id) * 0.0, 0.0, 0.0)
}

volume field fog(p: Vec3, surface_distance: F32) -> Medium {
    return Medium(
        density=0.2,
        emission=vec3(0.05, 0.06, 0.07) + vec3(abs(surface_distance) * 0.0, 0.0, 0.0),
        anisotropy=0.1
    )
}

shape near_shape {
    field = near_field
    material = shade
    radiance = glow
    volume = fog
    payload = Payload(
        entity_id=u32(101),
        material_id=u32(201),
        actor=ActorHandle(id=u32(301), generation=u32(0))
    )
}

shape mid_a_shape {
    field = mid_a_field
    material = shade
    radiance = glow
    volume = fog
    payload = Payload(
        entity_id=u32(102),
        material_id=u32(202),
        actor=ActorHandle(id=u32(302), generation=u32(0))
    )
}

shape mid_b_shape {
    field = mid_b_field
    material = shade
    radiance = glow
    volume = fog
    payload = Payload(
        entity_id=u32(103),
        material_id=u32(203),
        actor=ActorHandle(id=u32(303), generation=u32(0))
    )
}

shape focus_shape {
    field = focus_field
    material = shade
    radiance = glow
    volume = fog
    payload = Payload(
        entity_id=u32(104),
        material_id=u32(204),
        actor=ActorHandle(id=u32(304), generation=u32(0))
    )
}

region accelerated_region() {
    place near = near_shape
    place mid_a = mid_a_shape
    place mid_b = mid_b_shape
    place focus = focus_shape
}

domain accelerated_domain(world: RegionCapture) {
    geometry_detail = 1
    material = true
    radiance = true
    media = true
    max_distance = 12.0
    min_step = 0.05
    hit_epsilon = 0.001
    max_steps = 96
}
"#
    }

    fn scene_domain(
        scene_id: u32,
        detail: i32,
        material: bool,
        radiance: bool,
        media: bool,
    ) -> KernelValue {
        KernelValue::Struct(KernelStructValue {
            name: SmolStr::new("SceneDomain"),
            fields: vec![
                (SmolStr::new("scene_id"), KernelValue::U32(scene_id)),
                (
                    SmolStr::new("spatial"),
                    KernelValue::Struct(KernelStructValue {
                        name: SmolStr::new("SpatialDomainContract"),
                        fields: vec![(SmolStr::new("geometry_detail"), KernelValue::I32(detail))],
                    }),
                ),
                (
                    SmolStr::new("surface"),
                    KernelValue::Struct(KernelStructValue {
                        name: SmolStr::new("SurfaceDomainContract"),
                        fields: vec![(SmolStr::new("material"), KernelValue::Bool(material))],
                    }),
                ),
                (
                    SmolStr::new("participants"),
                    KernelValue::Struct(KernelStructValue {
                        name: SmolStr::new("ParticipantDomainContract"),
                        fields: vec![
                            (SmolStr::new("radiance"), KernelValue::Bool(radiance)),
                            (SmolStr::new("media"), KernelValue::Bool(media)),
                        ],
                    }),
                ),
            ],
        })
    }

    fn ray_query_with_limits(
        origin: [f32; 3],
        direction: [f32; 3],
        max_distance: f32,
        min_step: f32,
        hit_epsilon: f32,
        max_steps: i32,
    ) -> KernelValue {
        KernelValue::Struct(KernelStructValue {
            name: SmolStr::new("RayQuery"),
            fields: vec![
                (SmolStr::new("origin"), KernelValue::Vec3(origin)),
                (SmolStr::new("direction"), KernelValue::Vec3(direction)),
                (SmolStr::new("max_distance"), KernelValue::F32(max_distance)),
                (SmolStr::new("min_step"), KernelValue::F32(min_step)),
                (SmolStr::new("hit_epsilon"), KernelValue::F32(hit_epsilon)),
                (SmolStr::new("max_steps"), KernelValue::I32(max_steps)),
            ],
        })
    }

    fn expect_struct<'a>(value: &'a KernelValue, name: &str) -> &'a KernelStructValue {
        match value {
            KernelValue::Struct(value) if value.name.as_str() == name => value,
            other => panic!("expected {name}, got {other:?}"),
        }
    }

    fn field<'a>(value: &'a KernelStructValue, name: &str) -> &'a KernelValue {
        value
            .fields
            .iter()
            .find(|(field_name, _)| field_name.as_str() == name)
            .map(|(_, value)| value)
            .unwrap_or_else(|| panic!("missing field {name} on {}", value.name))
    }

    fn expect_u32(value: &KernelValue) -> u32 {
        match value {
            KernelValue::U32(value) => *value,
            other => panic!("expected U32, got {other:?}"),
        }
    }

    fn expect_f32(value: &KernelValue) -> f32 {
        match value {
            KernelValue::F32(value) => *value,
            other => panic!("expected F32, got {other:?}"),
        }
    }

    fn expect_vec3(value: &KernelValue) -> [f32; 3] {
        match value {
            KernelValue::Vec3(value) => *value,
            other => panic!("expected Vec3, got {other:?}"),
        }
    }

    fn legacy_immediate_query_dispatcher() -> GpuQueryDispatcher {
        let ctx = typed_query_module(accelerated_world_helper_fixture_source());
        let region_name = SmolStr::new("accelerated_region");
        let region_scene_id = stable_region_scene_capture_id(&region_name);
        let domain = scene_domain(region_scene_id, 1, true, true, true);
        let plan = lower_batch_query_plan(
            &BatchQueryPlan::for_contract(
                query_contract::SPATIAL_NEAREST_BATCH_WORLD,
                DispatchBackend::Wgsl,
                None,
            )
            .expect("world nearest batch plan"),
        );
        let dispatcher = GpuQueryDispatcher::from_batch_plan(
            &ctx,
            &plan,
            &[
                KernelValue::Capture(region_name),
                domain,
                KernelValue::Array(vec![ray_query_with_limits(
                    [6.0, 0.0, 3.0],
                    [0.0, 0.0, -1.0],
                    12.0,
                    0.05,
                    0.001,
                    96,
                )]),
            ],
        )
        .expect("world nearest dispatcher");
        dispatcher
            .initialize_dispatch_state()
            .expect("dispatcher initialization");
        dispatcher
    }

    #[test]
    fn dispatch_workgroups_follow_selected_workgroup_size() {
        assert_eq!(dispatch_workgroups_x_for_items(96, 32), 3);
        assert_eq!(dispatch_workgroups_x_for_items(96, 64), 2);
        assert_eq!(dispatch_workgroups_x_for_items(96, 128), 1);
    }

    #[test]
    fn chunk_plan_caps_items_to_storage_buffer_limits() {
        let plan = WgslDispatchChunkPlan {
            items_per_chunk: max_chunk_item_count(128 << 20, 48, 256, None).unwrap(),
            chunk_count: 2_073_600usize.div_ceil(524_288),
        };
        assert_eq!(plan.items_per_chunk, 524_288);
        assert_eq!(plan.chunk_count, 4);
    }

    #[test]
    fn slice_request_updates_dispatch_item_count_and_seeds() {
        let request = GpuDispatchRequest {
            dispatch: dispatch_config(2, 0, 6, 3, 1, 4, 0, true, false, false, false),
            items: (0..6).map(KernelValue::U32).collect(),
            world_shape_indices: vec![7, 8, 9],
            accel_nodes: Vec::new(),
            accel_children: Vec::new(),
            cache_bricks: Vec::new(),
            continuation_seeds: vec![10, 11, 12, 13, 14, 15],
            candidate_spans: Vec::new(),
            resident_scene_snapshot: None,
            resident_scene_detail: 0,
            resident_scene_selection_signature: 0,
        };
        let sliced = slice_gpu_dispatch_request(&request, 2..5).unwrap();
        let dispatch = match sliced.dispatch {
            KernelValue::Struct(ref dispatch) => dispatch,
            _ => panic!("expected struct dispatch config"),
        };
        let item_count = dispatch
            .fields
            .iter()
            .find(|(name, _)| name == "item_count")
            .and_then(|(_, value)| match value {
                KernelValue::U32(count) => Some(*count),
                _ => None,
            })
            .expect("item_count field");
        assert_eq!(item_count, 3);
        assert_eq!(sliced.items.len(), 3);
        assert_eq!(sliced.continuation_seeds, vec![12, 13, 14]);
        assert_eq!(sliced.world_shape_indices, vec![7, 8, 9]);
    }

    #[test]
    fn slice_request_preserves_candidate_span_pairs() {
        let request = GpuDispatchRequest {
            dispatch: dispatch_config(2, 0, 4, 3, 1, 4, 0, true, false, false, false),
            items: (0..4).map(KernelValue::U32).collect(),
            world_shape_indices: vec![7, 8, 9],
            accel_nodes: Vec::new(),
            accel_children: Vec::new(),
            cache_bricks: Vec::new(),
            continuation_seeds: Vec::new(),
            candidate_spans: vec![0xffffffff, 0, 4, 2, 8, 1, 9, 3],
            resident_scene_snapshot: None,
            resident_scene_detail: 0,
            resident_scene_selection_signature: 0,
        };
        let normalized = normalized_dispatch_config(&request).unwrap();
        let normalized_dispatch = match normalized {
            KernelValue::Struct(ref dispatch) => dispatch,
            _ => panic!("expected struct dispatch config"),
        };
        let candidate_spans_enabled = normalized_dispatch
            .fields
            .iter()
            .find(|(name, _)| name == "candidate_spans_enabled")
            .and_then(|(_, value)| match value {
                KernelValue::Bool(enabled) => Some(*enabled),
                _ => None,
            })
            .expect("candidate_spans_enabled field");
        assert!(candidate_spans_enabled);
        let sliced = slice_gpu_dispatch_request(&request, 1..3).unwrap();

        assert_eq!(sliced.candidate_spans, vec![4, 2, 8, 1]);
    }

    #[test]
    fn repeated_chunk_seed_metrics_are_suppressed_after_first_chunk() {
        let mut merged = QueryExecutionObservability {
            cache_resident_shared_snapshot_artifacts: 3,
            cache_resident_observer_local_artifacts: 2,
            cache_upload_attempts: 5,
            cache_upload_rejections: 1,
            dispatch_count: 1,
            dispatch_items: 4,
            ..QueryExecutionObservability::default()
        };
        let mut repeated_chunk = QueryExecutionObservability {
            cache_resident_shared_snapshot_artifacts: 3,
            cache_resident_observer_local_artifacts: 2,
            cache_upload_attempts: 5,
            cache_upload_rejections: 1,
            dispatch_count: 1,
            dispatch_items: 2,
            ..QueryExecutionObservability::default()
        };

        suppress_repeated_chunk_seed_metrics(&mut repeated_chunk);
        merged.merge_from(&repeated_chunk);

        assert_eq!(merged.cache_resident_shared_snapshot_artifacts, 3);
        assert_eq!(merged.cache_resident_observer_local_artifacts, 2);
        assert_eq!(merged.cache_upload_attempts, 5);
        assert_eq!(merged.cache_upload_rejections, 1);
        assert_eq!(merged.dispatch_count, 2);
        assert_eq!(merged.dispatch_items, 6);
    }

    #[test]
    fn world_helper_annotation_reports_dense_fallback_and_accelerated_paths() {
        fn accel_node_value() -> KernelValue {
            KernelValue::Struct(KernelStructValue {
                name: SmolStr::new("WgslAccelNode"),
                fields: vec![
                    (SmolStr::new("min"), KernelValue::Vec3([0.0, 0.0, 0.0])),
                    (SmolStr::new("max"), KernelValue::Vec3([1.0, 1.0, 1.0])),
                    (SmolStr::new("child_start"), KernelValue::U32(0)),
                    (SmolStr::new("child_len"), KernelValue::U32(0)),
                    (SmolStr::new("leaf_shape_index"), KernelValue::U32(0)),
                    (SmolStr::new("flags"), KernelValue::U32(0)),
                ],
            })
        }

        let contract_id = world_query_contract_id(WorldQueryKind::Distance);
        let base_request = GpuDispatchRequest {
            dispatch: dispatch_config(2, 0, 1, 1, 0, 0, 0, false, false, false, false),
            items: vec![KernelValue::Vec3([0.0, 0.0, 0.0])],
            world_shape_indices: vec![0],
            accel_nodes: Vec::new(),
            accel_children: Vec::new(),
            cache_bricks: Vec::new(),
            continuation_seeds: Vec::new(),
            candidate_spans: Vec::new(),
            resident_scene_snapshot: None,
            resident_scene_detail: 0,
            resident_scene_selection_signature: 0,
        };

        let mut dense = QueryExecutionObservability::default();
        annotate_wgsl_world_helper_path_for_world(&mut dense, contract_id, &base_request);
        assert_eq!(
            dense.wgsl_world_helper_path.as_deref(),
            Some("dense_fallback")
        );

        let mut accelerated = QueryExecutionObservability::default();
        let mut accelerated_request = base_request.clone();
        accelerated_request.accel_nodes = vec![accel_node_value()];
        annotate_wgsl_world_helper_path_for_world(
            &mut accelerated,
            contract_id,
            &accelerated_request,
        );
        assert_eq!(
            accelerated.wgsl_world_helper_path.as_deref(),
            Some("accelerated")
        );

        let mut rejected = QueryExecutionObservability {
            cache_budget_rejections: 1,
            ..QueryExecutionObservability::default()
        };
        annotate_wgsl_world_helper_path_for_world(&mut rejected, contract_id, &accelerated_request);
        assert_eq!(
            rejected.wgsl_world_helper_path.as_deref(),
            Some("dense_fallback")
        );
    }

    #[test]
    fn pipeline_cache_key_tracks_runtime_limit_request() {
        let layout = PipelineLayoutKey::new(GpuLayoutIdentity::new(7, 11), 4, 0);
        let pipeline =
            ComputePipelineKey::from_shader_source(layout, "@compute fn main() {}", "main", 64);
        let left = WgslPipelineCacheKey {
            limits: WgslLimitRequest {
                max_storage_buffers_per_shader_stage: 10,
                max_storage_buffer_binding_size: 4096,
                ..WgslLimitRequest::default()
            },
            pipeline: pipeline.clone(),
        };
        let right = WgslPipelineCacheKey {
            limits: WgslLimitRequest {
                max_storage_buffers_per_shader_stage: 10,
                max_storage_buffer_binding_size: 8192,
                ..WgslLimitRequest::default()
            },
            pipeline,
        };

        assert_ne!(left, right);
    }

    #[test]
    fn chunked_direct_dispatch_preserves_results_and_merged_observability() {
        let _lock = native_wgsl_test_lock();
        let structs = portable_abi_emit_wgsl_structs(&[
            wgsl_dispatch_config_abi(),
            wgsl_accel_node_abi(),
            wgsl_cache_brick_abi(),
            wgsl_shape_meta_abi(),
        ])
        .expect("wgsl structs");
        let source = format!(
            "{structs}

override WG_SIZE: u32 = 64u;

struct InputBuffer {{
  values: array<u32>,
}}

struct ResultBuffer {{
  values: array<u32>,
}}

struct ShapeIndexBuffer {{
  values: array<u32>,
}}

struct AccelNodeBuffer {{
  values: array<WgslAccelNode>,
}}

struct ShapeMetaBuffer {{
  values: array<WgslShapeMeta>,
}}

struct CacheBrickBuffer {{
  values: array<WgslCacheBrick>,
}}

struct ContinuationSeedBuffer {{
  values: array<u32>,
}}

struct WgslObservabilityBuffer {{
  acceleration_node_visits: atomic<u32>,
  shape_leaf_visits: atomic<u32>,
  acceleration_pruned_nodes: atomic<u32>,
  ray_support_interval_rejections: atomic<u32>,
  ray_support_entry_jumps: atomic<u32>,
  cache_brick_visits: atomic<u32>,
  cache_brick_hits: atomic<u32>,
  cache_brick_misses: atomic<u32>,
  cache_interval_advances: atomic<u32>,
  cache_resident_shared_snapshot_artifacts: atomic<u32>,
  cache_resident_observer_local_artifacts: atomic<u32>,
  cache_upload_attempts: atomic<u32>,
  cache_upload_rejections: atomic<u32>,
  cache_budget_rejections: atomic<u32>,
  cache_dense_fallback_rays: atomic<u32>,
  solver_analytic_hits: atomic<u32>,
  solver_generated_dense_fallback_rays: atomic<u32>,
  solver_support_rejections: atomic<u32>,
  field_samples: atomic<u32>,
}}

@group(0) @binding(0)
var<storage, read> accel_nodes: AccelNodeBuffer;
@group(0) @binding(1)
var<storage, read> accel_children: ShapeIndexBuffer;
@group(0) @binding(2)
var<storage, read> shape_meta: ShapeMetaBuffer;
@group(0) @binding(3)
var<storage, read> cache_bricks: CacheBrickBuffer;
@group(1) @binding(0)
var<storage, read> dispatch_config: WgslDispatchConfig;
@group(2) @binding(0)
var<storage, read> input_items: InputBuffer;
@group(2) @binding(1)
var<storage, read_write> output_items: ResultBuffer;
@group(2) @binding(2)
var<storage, read> world_shapes: ShapeIndexBuffer;
@group(2) @binding(3)
var<storage, read_write> observability_metrics: WgslObservabilityBuffer;
@group(3) @binding(0)
var<storage, read> continuation_seeds: ContinuationSeedBuffer;

@compute @workgroup_size(WG_SIZE)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {{
  let index = global_id.x;
  _ = accel_nodes.values[0].flags;
  _ = accel_children.values[0];
  _ = shape_meta.values[0].root_shape_id;
  _ = cache_bricks.values[0].min.x;
  _ = world_shapes.values[0];
  _ = continuation_seeds.values[0];
  if (index >= dispatch_config.item_count) {{
    return;
  }}
  if (index == 0u) {{
    atomicAdd(&observability_metrics.cache_resident_shared_snapshot_artifacts, 3u);
    atomicAdd(&observability_metrics.cache_upload_attempts, 3u);
    atomicAdd(&observability_metrics.field_samples, 7u);
  }}
  output_items.values[index] = input_items.values[index];
}}
"
        );
        let generated = GeneratedShaderModule {
            source,
            workgroup_size: 64,
            dispatch_abi: wgsl_dispatch_config_abi(),
            accel_node_abi: wgsl_accel_node_abi(),
            cache_brick_abi: wgsl_cache_brick_abi(),
            shape_meta_abi: wgsl_shape_meta_abi(),
            item_abi: PortableAbiType::U32,
            result_abi: PortableAbiType::U32,
            cache_observability_seed: crate::query_exec::wgsl::codegen::CacheObservabilitySeed {
                resident_shared_snapshot_artifacts: 3,
                resident_observer_local_artifacts: 0,
                upload_attempts: 3,
                upload_rejections: 0,
            },
            shape_meta_values: vec![KernelValue::Struct(KernelStructValue {
                name: SmolStr::new("WgslShapeMeta"),
                fields: vec![
                    (SmolStr::new("root_shape_id"), KernelValue::U32(123)),
                    (SmolStr::new("analytic_kind"), KernelValue::U32(0)),
                ],
            })],
        };
        let request = GpuDispatchRequest {
            dispatch: dispatch_config(0, 0, 6, 1, 0, 1, 1, false, false, false, false),
            items: (0..6).map(KernelValue::U32).collect(),
            world_shape_indices: vec![0],
            accel_nodes: vec![KernelValue::Struct(KernelStructValue {
                name: SmolStr::new("WgslAccelNode"),
                fields: vec![
                    (SmolStr::new("min"), KernelValue::Vec3([0.0, 0.0, 0.0])),
                    (SmolStr::new("max"), KernelValue::Vec3([1.0, 1.0, 1.0])),
                    (SmolStr::new("child_start"), KernelValue::U32(0)),
                    (SmolStr::new("child_len"), KernelValue::U32(0)),
                    (SmolStr::new("leaf_shape_index"), KernelValue::U32(0)),
                    (SmolStr::new("flags"), KernelValue::U32(0)),
                ],
            })],
            accel_children: vec![0],
            cache_bricks: vec![KernelValue::Struct(KernelStructValue {
                name: SmolStr::new("WgslCacheBrick"),
                fields: vec![
                    (SmolStr::new("min"), KernelValue::Vec3([0.0, 0.0, 0.0])),
                    (SmolStr::new("max"), KernelValue::Vec3([1.0, 1.0, 1.0])),
                ],
            })],
            continuation_seeds: vec![0; 6],
            candidate_spans: Vec::new(),
            resident_scene_snapshot: Some(
                stable_region_snapshot_handle(&SmolStr::new("chunked_dispatch_region")).report(),
            ),
            resident_scene_detail: 0,
            resident_scene_selection_signature: 11,
        };

        let (values, observability) = with_test_chunk_storage_buffer_limit_override(16, || {
            dispatch_compiled_shader_with_observability(&generated, request.clone())
                .expect("chunked direct dispatch")
        });
        let (second_values, second_observability) =
            with_test_chunk_storage_buffer_limit_override(16, || {
                dispatch_compiled_shader_with_observability(&generated, request.clone())
                    .expect("chunked direct dispatch reuse")
            });
        let mut alternate_request = request.clone();
        alternate_request.resident_scene_selection_signature = 12;
        alternate_request.world_shape_indices = vec![1, 2];
        let (alternate_values, alternate_observability) =
            with_test_chunk_storage_buffer_limit_override(16, || {
                dispatch_compiled_shader_with_observability(&generated, alternate_request.clone())
                    .expect("chunked direct dispatch alternate selection")
            });

        assert_eq!(values, request.items);
        assert_eq!(second_values, request.items);
        assert_eq!(alternate_values, request.items);
        assert_eq!(observability.dispatch_count, 2);
        assert_eq!(observability.dispatch_items, 6);
        assert_eq!(observability.cache_resident_shared_snapshot_artifacts, 3);
        assert_eq!(observability.cache_upload_attempts, 3);
        assert_eq!(observability.field_samples, 14);
        assert!(observability.gpu_runtime.scene_reupload_bytes > 0);
        assert!(observability.gpu_runtime.upload_bytes > 0);
        assert_eq!(second_observability.gpu_runtime.scene_reupload_bytes, 0);
        assert!(
            second_observability.gpu_runtime.upload_bytes < observability.gpu_runtime.upload_bytes
        );
        assert!(alternate_observability.gpu_runtime.scene_reupload_bytes > 0);
        assert!(
            alternate_observability.gpu_runtime.upload_bytes
                > second_observability.gpu_runtime.upload_bytes
        );
        assert!(
            second_observability.gpu_runtime.transient_buffer_creations
                < observability.gpu_runtime.transient_buffer_creations
        );
        assert!(
            second_observability
                .gpu_runtime
                .transient_bind_group_creations
                < observability.gpu_runtime.transient_bind_group_creations
        );
    }

    #[test]
    fn gpu_query_ticket_can_be_encoded_without_immediate_value_readback() {
        let _lock = native_wgsl_test_lock();
        let dispatcher = legacy_immediate_query_dispatcher();
        let native = dispatcher.native().clone();
        let mut encoder = native
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("wrela.query_exec.ticket.no_readback.encoder"),
            });
        let mut profiler = GpuPassProfiler::new(&native, 1);

        let ticket = dispatcher.encode_compute_pass_with_readback_policy(
            &mut encoder,
            &mut profiler,
            GpuReadbackPolicy::NoReadback,
        );

        assert_eq!(ticket.readback_policy(), GpuReadbackPolicy::NoReadback);
        assert!(!ticket.has_value_readback());
        assert!(ticket.dispatch_result().values.size_bytes > 0);
    }

    #[test]
    fn gpu_query_no_readback_policy_schedules_no_value_readback() {
        let _lock = native_wgsl_test_lock();
        let dispatcher = legacy_immediate_query_dispatcher();
        let native = dispatcher.native().clone();
        let mut encoder = native
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("wrela.query_exec.ticket.no_readback_policy.encoder"),
            });
        let mut profiler = GpuPassProfiler::new(&native, 1);

        let ticket = dispatcher.encode_compute_pass_with_readback_policy(
            &mut encoder,
            &mut profiler,
            GpuReadbackPolicy::NoReadback,
        );

        assert!(!GpuReadbackPolicy::NoReadback.should_schedule_value_readback());
        assert!(!ticket.has_value_readback());
    }

    #[test]
    fn gpu_query_legacy_immediate_collection_decodes_correctly() {
        let _lock = native_wgsl_test_lock();
        let dispatcher = legacy_immediate_query_dispatcher();
        let native = dispatcher.native().clone();
        let mut encoder = native
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("wrela.query_exec.ticket.legacy_immediate.encoder"),
            });
        let mut profiler = GpuPassProfiler::new(&native, 1);

        let ticket = dispatcher.encode_compute_pass_with_readback_policy(
            &mut encoder,
            &mut profiler,
            GpuReadbackPolicy::LegacyImmediate,
        );
        assert!(ticket.has_value_readback());

        native.queue.submit(Some(encoder.finish()));

        let (values, observability) = ticket.collect().expect("legacy immediate ticket collect");
        let hit = expect_struct(values.first().expect("batch hit"), "Hit3");
        let payload = expect_struct(field(hit, "payload"), "Payload");
        assert_eq!(expect_u32(field(payload, "entity_id")), 104);
        assert!(observability.dispatch_count > 0);
        assert!(observability.field_samples > 0);
        assert!(observability.gpu_runtime.readback_bytes > 0);
    }

    #[test]
    fn accelerated_world_helpers_stop_emitting_dense_helper_functions() {
        let _lock = native_wgsl_test_lock();
        let ctx = typed_query_module(accelerated_world_helper_fixture_source());

        let distance_plan =
            lower_world_query_plan(&WorldQueryPlan::for_query(WorldQueryKind::Distance));
        let distance_shader = compile_world_shader(&ctx, &distance_plan).expect("distance shader");
        assert!(
            distance_shader
                .source
                .contains("fn world_distance_point_accel(")
        );
        assert!(
            !distance_shader
                .source
                .contains("fn world_distance_point_dense(")
        );

        let radiance_plan =
            lower_world_query_plan(&WorldQueryPlan::for_query(WorldQueryKind::Radiance));
        let radiance_shader = compile_world_shader(&ctx, &radiance_plan).expect("radiance shader");
        assert!(
            radiance_shader
                .source
                .contains("fn world_radiance_query_accel(")
        );
        assert!(
            !radiance_shader
                .source
                .contains("fn world_radiance_query_dense(")
        );

        let medium_plan =
            lower_world_query_plan(&WorldQueryPlan::for_query(WorldQueryKind::Medium));
        let medium_shader = compile_world_shader(&ctx, &medium_plan).expect("medium shader");
        assert!(
            medium_shader
                .source
                .contains("fn world_medium_point_accel(")
        );
        assert!(
            !medium_shader
                .source
                .contains("fn world_medium_point_dense(")
        );
    }

    #[test]
    fn candidate_span_miss_falls_back_to_full_world_trace() {
        let _lock = native_wgsl_test_lock();
        let ctx = typed_query_module(accelerated_world_helper_fixture_source());
        let region_name = SmolStr::new("accelerated_region");
        let region_scene_id = stable_region_scene_capture_id(&region_name);
        let domain = scene_domain(region_scene_id, 1, true, true, true);
        let plan = lower_batch_query_plan(
            &BatchQueryPlan::for_contract(
                query_contract::SPATIAL_NEAREST_BATCH_WORLD,
                DispatchBackend::Wgsl,
                None,
            )
            .expect("world nearest batch plan"),
        );
        let generated = compile_batch_shader(&ctx, &plan).expect("world nearest batch shader");
        let mut request = build_batch_request_for_shader(
            &ctx,
            &plan,
            &[
                KernelValue::Capture(region_name.clone()),
                domain,
                KernelValue::Array(vec![ray_query_with_limits(
                    [6.0, 0.0, 3.0],
                    [0.0, 0.0, -1.0],
                    12.0,
                    0.05,
                    0.001,
                    96,
                )]),
            ],
        )
        .expect("world nearest batch request");
        assert!(!request.accel_nodes.is_empty());
        let near_shape_index = ctx
            .scene
            .shapes
            .keys()
            .enumerate()
            .find_map(|(index, shape)| (shape.as_str() == "near_shape").then_some(index as u32))
            .expect("near_shape scene index");
        let focus_shape_index = ctx
            .scene
            .shapes
            .keys()
            .enumerate()
            .find_map(|(index, shape)| (shape.as_str() == "focus_shape").then_some(index as u32))
            .expect("focus_shape scene index");
        assert!(request.world_shape_indices.contains(&focus_shape_index));

        request.candidate_spans = vec![0, 1, near_shape_index];
        let (values, observability) =
            dispatch_compiled_shader_with_observability(&generated, request)
                .expect("candidate-span fallback dispatch");

        let hit = expect_struct(values.first().expect("batch hit"), "Hit3");
        let payload = expect_struct(field(hit, "payload"), "Payload");
        assert_eq!(expect_u32(field(payload, "entity_id")), 104);
        assert!(observability.acceleration_node_visits > 0);
        assert!(observability.field_samples > 0);
        assert_eq!(observability.cache_budget_rejections, 0);
    }

    #[test]
    fn candidate_span_restricts_world_distance_batches() {
        let _lock = native_wgsl_test_lock();
        let ctx = typed_query_module(accelerated_world_helper_fixture_source());
        let region_name = SmolStr::new("accelerated_region");
        let region_scene_id = stable_region_scene_capture_id(&region_name);
        let domain = scene_domain(region_scene_id, 1, true, true, true);
        let plan = lower_batch_query_plan(
            &BatchQueryPlan::for_contract(
                query_contract::SPATIAL_DISTANCE_BATCH_WORLD,
                DispatchBackend::Wgsl,
                None,
            )
            .expect("world distance batch plan"),
        );
        let generated = compile_batch_shader(&ctx, &plan).expect("world distance batch shader");
        let mut request = build_batch_request_for_shader(
            &ctx,
            &plan,
            &[
                KernelValue::Capture(region_name.clone()),
                domain,
                KernelValue::Array(vec![super::point_query([5.4, 0.0, 0.0])]),
            ],
        )
        .expect("world distance batch request");
        let near_shape_index = ctx
            .scene
            .shapes
            .keys()
            .enumerate()
            .find_map(|(index, shape)| (shape.as_str() == "near_shape").then_some(index as u32))
            .expect("near_shape scene index");
        request.candidate_spans = vec![0, 1, near_shape_index];

        let (values, observability) =
            dispatch_compiled_shader_with_observability(&generated, request)
                .expect("candidate-span distance dispatch");

        let result = expect_struct(values.first().expect("distance value"), "DistanceResult");
        assert!(expect_f32(field(result, "distance")) > 10.0);
        assert!(observability.field_samples > 0);
    }

    #[test]
    fn candidate_span_restricts_world_normal_batches() {
        let _lock = native_wgsl_test_lock();
        let ctx = typed_query_module(accelerated_world_helper_fixture_source());
        let region_name = SmolStr::new("accelerated_region");
        let region_scene_id = stable_region_scene_capture_id(&region_name);
        let domain = scene_domain(region_scene_id, 1, true, true, true);
        let plan = lower_batch_query_plan(
            &BatchQueryPlan::for_contract(
                query_contract::SPATIAL_NORMAL_BATCH_WORLD,
                DispatchBackend::Wgsl,
                None,
            )
            .expect("world normal batch plan"),
        );
        let generated = compile_batch_shader(&ctx, &plan).expect("world normal batch shader");
        let mut request = build_batch_request_for_shader(
            &ctx,
            &plan,
            &[
                KernelValue::Capture(region_name.clone()),
                domain,
                KernelValue::Array(vec![super::point_query([5.4, 0.0, 0.0])]),
            ],
        )
        .expect("world normal batch request");
        let near_shape_index = ctx
            .scene
            .shapes
            .keys()
            .enumerate()
            .find_map(|(index, shape)| (shape.as_str() == "near_shape").then_some(index as u32))
            .expect("near_shape scene index");
        request.candidate_spans = vec![0, 1, near_shape_index];

        let (values, _) = dispatch_compiled_shader_with_observability(&generated, request)
            .expect("candidate-span normal dispatch");

        let result = expect_struct(values.first().expect("normal value"), "NormalResult");
        assert_eq!(expect_vec3(field(result, "normal")), [1.0, 0.0, 0.0]);
    }
}
