use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, mpsc};

use wgpu::util::initialize_adapter_from_env_or_default;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GpuLimitRequest {
    pub max_storage_buffers_per_shader_stage: u32,
    pub max_storage_buffer_binding_size: u64,
}

impl Default for GpuLimitRequest {
    fn default() -> Self {
        Self {
            max_storage_buffers_per_shader_stage: wgpu::Limits::downlevel_defaults()
                .max_storage_buffers_per_shader_stage,
            max_storage_buffer_binding_size: wgpu::Limits::downlevel_defaults()
                .max_storage_buffer_binding_size,
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
    pub timestamp_support: bool,
}

impl GpuRuntimeContext {
    pub fn timestamps_supported(&self) -> bool {
        self.timestamp_support
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
    if crate::query_exec::QUERY_WGSL_BIND_GROUP_COUNT > adapter_limits.max_bind_groups {
        return Err(format!(
            "query WGSL layout needs {} bind groups but adapter profile only supports {}",
            crate::query_exec::QUERY_WGSL_BIND_GROUP_COUNT,
            adapter_limits.max_bind_groups
        ));
    }

    let timestamp_features =
        wgpu::Features::TIMESTAMP_QUERY | wgpu::Features::TIMESTAMP_QUERY_INSIDE_PASSES;
    let timestamp_support = adapter.features().contains(timestamp_features);
    let selected_workgroup_size =
        crate::query_exec::select_query_wgsl_workgroup_size(&adapter_limits)
            .map_err(|err| err.to_string())?;
    let mut required_limits = wgpu::Limits::downlevel_defaults()
        .using_resolution(adapter_limits.clone())
        .using_alignment(adapter_limits.clone());
    required_limits.max_storage_buffers_per_shader_stage =
        request.max_storage_buffers_per_shader_stage;
    required_limits.max_storage_buffer_binding_size = request.max_storage_buffer_binding_size;
    required_limits.max_buffer_size = adapter_limits.max_buffer_size;
    required_limits.max_bind_groups = crate::query_exec::QUERY_WGSL_BIND_GROUP_COUNT;
    required_limits.max_compute_invocations_per_workgroup = selected_workgroup_size;
    required_limits.max_compute_workgroup_size_x = selected_workgroup_size;
    required_limits.max_compute_workgroup_size_y = 1;
    required_limits.max_compute_workgroup_size_z = 1;
    let descriptor = wgpu::DeviceDescriptor {
        label: Some("wrela.gpu_runtime.device"),
        required_features: if timestamp_support {
            timestamp_features
        } else {
            wgpu::Features::empty()
        },
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
        timestamp_support,
    })
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
