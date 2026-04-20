use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex, OnceLock, mpsc};

use crate::gpu_runtime::layout::{
    GPU_RUNTIME_BIND_GROUP_COUNT, GPU_RUNTIME_FEATURE_SHADER_F16,
    GPU_RUNTIME_FEATURE_TIMESTAMP_QUERY, GPU_RUNTIME_FEATURE_TIMESTAMP_QUERY_INSIDE_ENCODERS,
    GPU_RUNTIME_FEATURE_TIMESTAMP_QUERY_INSIDE_PASSES,
};
use wgpu::util::initialize_adapter_from_env_or_default;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GpuLimitRequest {
    pub max_storage_buffers_per_shader_stage: u32,
    pub max_storage_buffer_binding_size: u64,
    pub timestamps_enabled: bool,
    pub f16_enabled: bool,
}

impl Default for GpuLimitRequest {
    fn default() -> Self {
        Self {
            max_storage_buffers_per_shader_stage: wgpu::Limits::downlevel_defaults()
                .max_storage_buffers_per_shader_stage,
            max_storage_buffer_binding_size: wgpu::Limits::downlevel_defaults()
                .max_storage_buffer_binding_size,
            timestamps_enabled: false,
            f16_enabled: false,
        }
    }
}

#[derive(Clone)]
pub struct GpuRuntimeContext {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub adapter_info: wgpu::AdapterInfo,
    pub adapter_limits: wgpu::Limits,
    pub requested_limits: wgpu::Limits,
    pub requested_features: wgpu::Features,
    pub limit_request: GpuLimitRequest,
    pub timestamp_support: bool,
}

impl GpuRuntimeContext {
    pub fn timestamps_supported(&self) -> bool {
        self.timestamp_support
    }

    pub fn encoder_timestamps_supported(&self) -> bool {
        self.requested_features.contains(
            wgpu::Features::TIMESTAMP_QUERY | wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS,
        )
    }

    pub fn requested_limits_profile_name(&self) -> String {
        format!(
            "storage_buffers_per_stage={} storage_binding_bytes={} bind_groups={} workgroup_x={}",
            self.requested_limits.max_storage_buffers_per_shader_stage,
            self.requested_limits.max_storage_buffer_binding_size,
            self.requested_limits.max_bind_groups,
            self.requested_limits.max_compute_workgroup_size_x,
        )
    }

    pub fn feature_mask(&self) -> u64 {
        let mut mask = 0u64;
        if self
            .requested_features
            .contains(wgpu::Features::TIMESTAMP_QUERY)
        {
            mask |= GPU_RUNTIME_FEATURE_TIMESTAMP_QUERY;
        }
        if self
            .requested_features
            .contains(wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS)
        {
            mask |= GPU_RUNTIME_FEATURE_TIMESTAMP_QUERY_INSIDE_ENCODERS;
        }
        if self
            .requested_features
            .contains(wgpu::Features::TIMESTAMP_QUERY_INSIDE_PASSES)
        {
            mask |= GPU_RUNTIME_FEATURE_TIMESTAMP_QUERY_INSIDE_PASSES;
        }
        if self.requested_features.contains(wgpu::Features::SHADER_F16) {
            mask |= GPU_RUNTIME_FEATURE_SHADER_F16;
        }
        mask
    }

    pub fn enabled_optional_feature_names(&self) -> Vec<String> {
        let mut features = BTreeSet::new();
        if self
            .requested_features
            .contains(wgpu::Features::TIMESTAMP_QUERY)
        {
            features.insert("timestamp_query".to_string());
        }
        if self
            .requested_features
            .contains(wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS)
        {
            features.insert("timestamp_query_inside_encoders".to_string());
        }
        if self
            .requested_features
            .contains(wgpu::Features::TIMESTAMP_QUERY_INSIDE_PASSES)
        {
            features.insert("timestamp_query_inside_passes".to_string());
        }
        if self.requested_features.contains(wgpu::Features::SHADER_F16) {
            features.insert("shader_f16".to_string());
        }
        features.into_iter().collect()
    }

    pub fn create_upload_arena(
        &self,
        chunk_size: u64,
    ) -> crate::gpu_runtime::upload::FrameUploadArena {
        crate::gpu_runtime::upload::FrameUploadArena::new(&self.device, chunk_size)
    }
}

pub fn shared_wgpu_context(request: GpuLimitRequest) -> Result<Arc<GpuRuntimeContext>, String> {
    static CONTEXTS: OnceLock<
        Mutex<HashMap<GpuLimitRequest, Result<Arc<GpuRuntimeContext>, String>>>,
    > = OnceLock::new();
    let cache = CONTEXTS.get_or_init(|| Mutex::new(HashMap::new()));
    {
        let guard = cache.lock().unwrap_or_else(|poison| poison.into_inner());
        if let Some(context) = guard.get(&request) {
            return context.clone();
        }
    }
    let context = init_shared_wgpu_context(request).map(Arc::new);
    let mut guard = cache.lock().unwrap_or_else(|poison| poison.into_inner());
    let entry = guard.entry(request).or_insert_with(|| context.clone());
    entry.clone()
}

fn requested_optional_features(
    request: GpuLimitRequest,
    adapter_features: wgpu::Features,
) -> wgpu::Features {
    let mut requested = wgpu::Features::empty();
    let encoder_or_pass_features = wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS
        | wgpu::Features::TIMESTAMP_QUERY_INSIDE_PASSES;
    if request.timestamps_enabled
        && adapter_features.contains(wgpu::Features::TIMESTAMP_QUERY)
        && adapter_features.intersects(encoder_or_pass_features)
    {
        requested |= wgpu::Features::TIMESTAMP_QUERY;
        if adapter_features.contains(wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS) {
            requested |= wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS;
        }
        if adapter_features.contains(wgpu::Features::TIMESTAMP_QUERY_INSIDE_PASSES) {
            requested |= wgpu::Features::TIMESTAMP_QUERY_INSIDE_PASSES;
        }
    }
    if request.f16_enabled && adapter_features.contains(wgpu::Features::SHADER_F16) {
        requested |= wgpu::Features::SHADER_F16;
    }
    requested
}

fn init_shared_wgpu_context(request: GpuLimitRequest) -> Result<GpuRuntimeContext, String> {
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let adapter = pollster::block_on(initialize_adapter_from_env_or_default(&instance, None))
        .map_err(|err| format!("request adapter failed: {err}"))?;
    let adapter_info = adapter.get_info();
    let adapter_limits = adapter.limits();
    if request.max_storage_buffers_per_shader_stage
        > adapter_limits.max_storage_buffers_per_shader_stage
    {
        return Err(format!(
            "requested {} storage buffers per shader stage but adapter profile only supports {}",
            request.max_storage_buffers_per_shader_stage,
            adapter_limits.max_storage_buffers_per_shader_stage
        ));
    }
    if request.max_storage_buffer_binding_size > adapter_limits.max_storage_buffer_binding_size {
        return Err(format!(
            "requested storage buffer binding size {} exceeds adapter profile {}",
            request.max_storage_buffer_binding_size, adapter_limits.max_storage_buffer_binding_size
        ));
    }
    if request.max_storage_buffer_binding_size > adapter_limits.max_buffer_size {
        return Err(format!(
            "requested storage buffer binding size {} exceeds adapter max buffer size {}",
            request.max_storage_buffer_binding_size, adapter_limits.max_buffer_size
        ));
    }
    if GPU_RUNTIME_BIND_GROUP_COUNT > adapter_limits.max_bind_groups {
        return Err(format!(
            "gpu runtime layout needs {} bind groups but adapter profile only supports {}",
            GPU_RUNTIME_BIND_GROUP_COUNT, adapter_limits.max_bind_groups
        ));
    }

    let requested_features = requested_optional_features(request, adapter.features());
    let timestamp_support = requested_features
        .contains(wgpu::Features::TIMESTAMP_QUERY | wgpu::Features::TIMESTAMP_QUERY_INSIDE_PASSES);
    let selected_workgroup_size = [128u32, 64, 32]
        .into_iter()
        .find(|candidate| {
            *candidate <= adapter_limits.max_compute_workgroup_size_x
                && *candidate <= adapter_limits.max_compute_invocations_per_workgroup
        })
        .ok_or_else(|| {
            format!(
                "adapter does not support any legal query WGSL workgroup size in {:?}",
                crate::query_exec::QUERY_WGSL_LEGAL_WORKGROUP_SIZES
            )
        })?;
    let mut required_limits = wgpu::Limits::downlevel_defaults()
        .using_resolution(adapter_limits.clone())
        .using_alignment(adapter_limits.clone());
    required_limits.max_storage_buffers_per_shader_stage =
        request.max_storage_buffers_per_shader_stage;
    required_limits.max_storage_buffer_binding_size = request.max_storage_buffer_binding_size;
    required_limits.max_buffer_size = adapter_limits.max_buffer_size;
    required_limits.max_bind_groups = GPU_RUNTIME_BIND_GROUP_COUNT;
    required_limits.max_compute_invocations_per_workgroup = selected_workgroup_size;
    required_limits.max_compute_workgroup_size_x = selected_workgroup_size;
    required_limits.max_compute_workgroup_size_y = 1;
    required_limits.max_compute_workgroup_size_z = 1;
    let descriptor = wgpu::DeviceDescriptor {
        label: Some("wrela.gpu_runtime.device"),
        required_features: requested_features,
        required_limits: required_limits.clone(),
        experimental_features: wgpu::ExperimentalFeatures::disabled(),
        memory_hints: wgpu::MemoryHints::Performance,
        trace: wgpu::Trace::Off,
    };
    let (device, queue) = pollster::block_on(adapter.request_device(&descriptor))
        .map_err(|err| format!("request device failed: {err}"))?;
    Ok(GpuRuntimeContext {
        device,
        queue,
        adapter_info,
        adapter_limits,
        requested_limits: required_limits,
        requested_features: descriptor.required_features,
        limit_request: request,
        timestamp_support,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_limit_request_disables_optional_features() {
        let requested = requested_optional_features(
            GpuLimitRequest::default(),
            wgpu::Features::TIMESTAMP_QUERY
                | wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS
                | wgpu::Features::TIMESTAMP_QUERY_INSIDE_PASSES,
        );

        assert_eq!(requested, wgpu::Features::empty());
    }

    #[test]
    fn timestamp_features_request_encoder_or_pass_writes_when_available() {
        let request = GpuLimitRequest {
            timestamps_enabled: true,
            ..GpuLimitRequest::default()
        };
        let full_support = wgpu::Features::TIMESTAMP_QUERY
            | wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS
            | wgpu::Features::TIMESTAMP_QUERY_INSIDE_PASSES;

        let requested = requested_optional_features(request, full_support);
        assert_eq!(requested, full_support);

        let encoder_only = requested_optional_features(
            request,
            wgpu::Features::TIMESTAMP_QUERY | wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS,
        );
        assert_eq!(
            encoder_only,
            wgpu::Features::TIMESTAMP_QUERY | wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS
        );

        let query_only = requested_optional_features(request, wgpu::Features::TIMESTAMP_QUERY);
        assert_eq!(query_only, wgpu::Features::empty());
    }
}

pub fn readback_storage_buffer_on(
    context: &GpuRuntimeContext,
    buffer: &wgpu::Buffer,
    size: u64,
) -> Result<Vec<u8>, String> {
    let readback_buffer = context.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("wrela.gpu_runtime.readback"),
        size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = context
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("wrela.gpu_runtime.readback_encoder"),
        });
    encoder.copy_buffer_to_buffer(buffer, 0, &readback_buffer, 0, size);
    context.queue.submit(Some(encoder.finish()));

    let slice = readback_buffer.slice(..size);
    let (tx, rx) = mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = tx.send(result);
    });
    context
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|err| format!("device poll failed: {err}"))?;
    match rx.recv() {
        Ok(Ok(())) => {}
        Ok(Err(err)) => {
            return Err(format!("native WGSL readback failed: {err}"));
        }
        Err(err) => {
            return Err(format!("native WGSL readback channel failed: {err}"));
        }
    }
    let bytes = slice.get_mapped_range().to_vec();
    let _ = slice;
    readback_buffer.unmap();
    Ok(bytes)
}
