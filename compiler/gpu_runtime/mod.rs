pub mod bindings;
pub mod device;
pub mod layout;
pub mod metrics;
pub mod pipeline_cache;
pub mod profiler;
pub mod readback;
pub mod resident_scene;
pub mod upload;

pub use bindings::{
    GpuBindGroupRole, bind_group_layout_signature, bind_group_layout_signature_for_role,
    pipeline_layout_identity, pipeline_layout_signature, storage_buffer_binding_entry,
    texture_view_binding_entry,
};
pub use device::{
    GpuLimitRequest, GpuRuntimeContext, readback_storage_buffer_on, shared_wgpu_context,
};
pub use layout::{
    GPU_RUNTIME_BIND_GROUP_COUNT, GPU_RUNTIME_FEATURE_SHADER_F16,
    GPU_RUNTIME_FEATURE_TIMESTAMP_QUERY, GPU_RUNTIME_FEATURE_TIMESTAMP_QUERY_INSIDE_ENCODERS,
    GPU_RUNTIME_FEATURE_TIMESTAMP_QUERY_INSIDE_PASSES, GPU_RUNTIME_FRAME_BIND_GROUP_INDEX,
    GPU_RUNTIME_PASS_BIND_GROUP_INDEX, GPU_RUNTIME_SCENE_BIND_GROUP_INDEX,
    GPU_RUNTIME_SCHEMA_VERSION, GPU_RUNTIME_SCRATCH_BIND_GROUP_INDEX, GpuLayoutIdentity,
};
pub use metrics::{GpuRuntimeMetrics, classify_execution_bound};
pub use pipeline_cache::{
    BindGroupLayoutCache, ComputePipelineCache, ComputePipelineKey, GpuResourceCache,
    PipelineLayoutCache, PipelineLayoutKey, shader_signature,
};
pub use profiler::{GpuEncoderProfiler, GpuPassProfiler};
pub use readback::{
    ReadbackReason, ReadbackRequest, ReadbackResult, ReadbackTicket,
    collect_storage_buffer_readback, collect_storage_buffer_readback_bytes,
    schedule_storage_buffer_readback,
};
pub use resident_scene::{
    GpuResidentScene, GpuResidentSceneCache, GpuResidentSceneKey, GpuResidentScenePayload,
    clear_shared_resident_scene_caches_for_type, shared_resident_scene_cache_for_request,
};
pub use upload::{
    BufferPoolKey, FrameUploadArena, GpuBufferPool, UploadError, align_copy_buffer_size, align_up,
    lock_shared_upload_arena, normalize_buffer_size, shared_buffer_pool,
};
