//! Owns WGSL presentation pipeline objects and dispatch bundles.
//! Does not own shader-source generation or pass scheduling.
//!
//! Key invariants:
//! - pipeline/buffer bundles here must remain consistent with the shader ABI and
//!   pass contract that produced them.
//! - dispatch counts and readback labels are part of observability truth, not
//!   cosmetic metadata.
//!
//! Primary entrypoints:
//! - pipeline/dispatch structs and builders in this module
//!
//! Failure modes / common pitfalls:
//! - mutating dispatch bundle shape without updating consumers creates runtime
//!   mismatches that are hard to localize.

use super::*;

pub(super) struct MotionResolveGpuDispatch {
    pub(super) dispatch_count: u32,
    pub(super) counts_buffer: wgpu::Buffer,
    pub(super) counts_readback_label: String,
}

pub(super) struct TemporalResolveGpuDispatch {
    pub(super) dispatch_count: u32,
    pub(super) counts_buffer: wgpu::Buffer,
    pub(super) counts_readback_label: String,
}

#[derive(Clone)]
pub(super) struct PresentationCustomPipeline {
    pub(super) bind_group_layout: wgpu::BindGroupLayout,
    pub(super) pipeline: wgpu::ComputePipeline,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct PresentationCustomPipelineCacheKey {
    limits: crate::gpu_runtime::GpuLimitRequest,
    source: String,
    label: String,
    workgroup_size: u32,
}

pub(super) fn storage_buffer_with_usage_and_bytes(
    native: &crate::query_exec::wgsl::NativeWgpuContext,
    label: &str,
    bytes: &[u8],
    usage: wgpu::BufferUsages,
    gpu_runtime: &mut GpuRuntimeMetrics,
) -> wgpu::Buffer {
    let buffer = native.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: bytes.len().max(4) as u64,
        usage: usage | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    if !bytes.is_empty() {
        native.queue.write_buffer(&buffer, 0, bytes);
        gpu_runtime.upload_bytes = gpu_runtime.upload_bytes.saturating_add(bytes.len() as u64);
    }
    gpu_runtime.transient_buffer_creations =
        gpu_runtime.transient_buffer_creations.saturating_add(1);
    buffer
}

pub(super) fn zeroed_storage_buffer(
    native: &crate::query_exec::wgsl::NativeWgpuContext,
    label: &str,
    size: u64,
    usage: wgpu::BufferUsages,
    gpu_runtime: &mut GpuRuntimeMetrics,
) -> wgpu::Buffer {
    let buffer = native.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: size.max(4),
        usage: usage | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let zeroes = vec![0u8; size.max(4) as usize];
    native.queue.write_buffer(&buffer, 0, &zeroes);
    gpu_runtime.upload_bytes = gpu_runtime.upload_bytes.saturating_add(zeroes.len() as u64);
    gpu_runtime.transient_buffer_creations =
        gpu_runtime.transient_buffer_creations.saturating_add(1);
    buffer
}

pub(super) fn create_custom_pass_pipeline(
    native: &crate::query_exec::wgsl::NativeWgpuContext,
    source: &str,
    workgroup_size: u32,
    entries: &[wgpu::BindGroupLayoutEntry],
    label: &str,
    gpu_runtime: &mut GpuRuntimeMetrics,
) -> Result<PresentationCustomPipeline, PresentationExecError> {
    let key = PresentationCustomPipelineCacheKey {
        limits: native.limit_request,
        source: source.to_string(),
        label: label.to_string(),
        workgroup_size,
    };
    static PIPELINES: OnceLock<
        Mutex<HashMap<PresentationCustomPipelineCacheKey, PresentationCustomPipeline>>,
    > = OnceLock::new();
    let cache = PIPELINES.get_or_init(|| Mutex::new(HashMap::new()));

    {
        let guard = cache.lock().unwrap_or_else(|poison| poison.into_inner());
        if let Some(cached) = guard.get(&key) {
            gpu_runtime.pipeline_cache_hits = gpu_runtime.pipeline_cache_hits.saturating_add(1);
            return Ok(cached.clone());
        }
    }

    let bind_group_layout =
        native
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some(label),
                entries,
            });
    let pipeline_layout = native
        .device
        .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some(label),
            bind_group_layouts: &{
                let mut layouts = [None; GPU_RUNTIME_BIND_GROUP_COUNT as usize];
                layouts[GPU_RUNTIME_PASS_BIND_GROUP_INDEX as usize] = Some(&bind_group_layout);
                layouts
            },
            immediate_size: 0,
        });
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
            layout: Some(&pipeline_layout),
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
        .map_err(|err| PresentationExecError::UnsupportedPlan {
            message: format!("native WGSL device poll failed: {err}"),
        })?;
    if let Some(err) = pollster::block_on(error_scope.pop()) {
        return Err(PresentationExecError::Query(
            crate::query_exec::cpu::QueryExecError::Unsupported {
                message: format!("native WGSL validation failed: {err}"),
            },
        ));
    }
    gpu_runtime.pipeline_cache_misses = gpu_runtime.pipeline_cache_misses.saturating_add(1);
    let cached = PresentationCustomPipeline {
        bind_group_layout,
        pipeline,
    };
    let mut guard = cache.lock().unwrap_or_else(|poison| poison.into_inner());
    Ok(guard.entry(key).or_insert_with(|| cached.clone()).clone())
}

pub(super) fn encode_shade_primary_gpu(
    native: &crate::query_exec::wgsl::NativeWgpuContext,
    encoder: &mut wgpu::CommandEncoder,
    profiler: &mut GpuPassProfiler,
    arena: &crate::presentation_exec::gpu_resources::GpuAttachmentArena,
    camera: crate::presentation_contract::CanonicalCameraInput,
    viewport: crate::presentation_contract::CanonicalViewportInput,
    jitter_pixels: [f32; 2],
    legacy_projection: bool,
    lighting: &crate::presentation_contract::PresentationLightingInputs,
    contract: &ShadePrimaryPassContract,
    workgroup_size: u32,
    gpu_runtime: &mut GpuRuntimeMetrics,
) -> Result<u32, PresentationExecError> {
    let shader_f16_enabled = native
        .requested_features
        .contains(wgpu::Features::SHADER_F16);
    let Some(primary_hit_buffer) =
        arena.attachment_buffer(contract.primary_hit_attachment.as_str())
    else {
        return Ok(0);
    };
    let Some(surface_buffer) = arena.attachment_buffer(contract.surface_attachment.as_str()) else {
        return Ok(0);
    };
    let Some(output_slot) = arena.attachment(contract.output_attachment.as_str()) else {
        return Ok(0);
    };
    let output_buffer =
        output_slot
            .gpu_buffer()
            .ok_or_else(|| PresentationExecError::UnsupportedPlan {
                message: format!(
                    "attachment '{}' is not GPU-backed",
                    contract.output_attachment
                ),
            })?;
    let radiance_bytes = encode_slice(
        &PortableAbiType::Vec3,
        &[KernelValue::Vec3([0.0, 0.0, 0.0])],
    )
    .map_err(PresentationExecError::Query)?;
    let medium_abi = portable_builtin_record_abi("Medium").expect("Medium abi");
    let medium_bytes =
        encode_slice(&medium_abi, &[default_medium()]).map_err(PresentationExecError::Query)?;
    let radiance_buffer = if let Some(name) = contract.radiance_attachment.as_deref() {
        arena
            .attachment_buffer(name)
            .map(|buffer| buffer.clone())
            .unwrap_or_else(|| {
                storage_buffer_with_usage_and_bytes(
                    native,
                    "wrela.presentation.shade.radiance_default",
                    &radiance_bytes,
                    wgpu::BufferUsages::STORAGE,
                    gpu_runtime,
                )
            })
    } else {
        storage_buffer_with_usage_and_bytes(
            native,
            "wrela.presentation.shade.radiance_default",
            &radiance_bytes,
            wgpu::BufferUsages::STORAGE,
            gpu_runtime,
        )
    };
    let medium_buffer = if let Some(name) = contract.medium_attachment.as_deref() {
        arena
            .attachment_buffer(name)
            .map(|buffer| buffer.clone())
            .unwrap_or_else(|| {
                storage_buffer_with_usage_and_bytes(
                    native,
                    "wrela.presentation.shade.medium_default",
                    &medium_bytes,
                    wgpu::BufferUsages::STORAGE,
                    gpu_runtime,
                )
            })
    } else {
        storage_buffer_with_usage_and_bytes(
            native,
            "wrela.presentation.shade.medium_default",
            &medium_bytes,
            wgpu::BufferUsages::STORAGE,
            gpu_runtime,
        )
    };
    let config_bytes = encode_value(
        &shade_primary_gpu_config_abi(),
        &shade_primary_gpu_config_value(
            camera,
            viewport,
            jitter_pixels,
            legacy_projection,
            lighting,
            arena,
            contract,
        ),
    )
    .map_err(PresentationExecError::Query)?;
    let config_buffer = storage_buffer_with_usage_and_bytes(
        native,
        "wrela.presentation.shade.config",
        &config_bytes,
        wgpu::BufferUsages::STORAGE,
        gpu_runtime,
    );
    let cached = create_custom_pass_pipeline(
        native,
        &shade_primary_gpu_shader_source(workgroup_size, shader_f16_enabled)?,
        workgroup_size,
        &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(config_bytes.len().max(4) as u64),
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
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
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
            wgpu::BindGroupLayoutEntry {
                binding: 4,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 5,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
        "wrela.presentation.shade.pipeline",
        gpu_runtime,
    )?;
    gpu_runtime.transient_bind_group_creations =
        gpu_runtime.transient_bind_group_creations.saturating_add(1);
    let bind_group = native.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("wrela.presentation.shade.bind_group"),
        layout: &cached.bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: config_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: primary_hit_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: surface_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: radiance_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: medium_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: output_buffer.as_entire_binding(),
            },
        ],
    });
    let timestamp_writes = profiler.compute_pass_timestamp_writes();
    let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
        label: Some("wrela.presentation.shade.compute"),
        timestamp_writes,
    });
    pass.set_pipeline(&cached.pipeline);
    pass.set_bind_group(GPU_RUNTIME_PASS_BIND_GROUP_INDEX, &bind_group, &[]);
    pass.dispatch_workgroups(
        viewport
            .width
            .saturating_mul(viewport.height)
            .div_ceil(workgroup_size.max(1))
            .max(1),
        1,
        1,
    );
    Ok(1)
}

pub(super) fn encode_motion_resolve_gpu(
    native: &crate::query_exec::wgsl::NativeWgpuContext,
    encoder: &mut wgpu::CommandEncoder,
    profiler: &mut GpuPassProfiler,
    arena: &crate::presentation_exec::gpu_resources::GpuAttachmentArena,
    input: &PresentationExecutionInput,
    viewport: crate::presentation_contract::CanonicalViewportInput,
    contract: &MotionResolvePassContract,
    workgroup_size: u32,
    gpu_runtime: &mut GpuRuntimeMetrics,
    summary: &crate::presentation_exec::temporal::MotionResolveAssessmentSummary,
) -> Result<MotionResolveGpuDispatch, PresentationExecError> {
    let Some(primary_hit_buffer) =
        arena.attachment_buffer(contract.primary_hit_attachment.as_str())
    else {
        return Ok(MotionResolveGpuDispatch {
            dispatch_count: 0,
            counts_buffer: zeroed_storage_buffer(
                native,
                "wrela.presentation.motion.counts.empty",
                12,
                wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
                gpu_runtime,
            ),
            counts_readback_label: "wrela.presentation.motion.counts".to_string(),
        });
    };
    let Some(output_buffer) = arena.attachment_buffer(contract.output_attachment.as_str()) else {
        return Ok(MotionResolveGpuDispatch {
            dispatch_count: 0,
            counts_buffer: zeroed_storage_buffer(
                native,
                "wrela.presentation.motion.counts.empty",
                12,
                wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
                gpu_runtime,
            ),
            counts_readback_label: "wrela.presentation.motion.counts".to_string(),
        });
    };
    let components = crate::presentation_exec::frame_state_temporal_components(&input.frame_state)?;
    let history_hit_buffer = contract
        .history_primary_hit_attachment
        .as_ref()
        .and_then(|name| arena.attachment_buffer(name.as_str()))
        .map(|buffer| buffer.clone())
        .unwrap_or_else(|| primary_hit_buffer.clone());
    let config_bytes = encode_value(
        &motion_resolve_gpu_config_abi(),
        &motion_resolve_gpu_config_value(
            viewport,
            components.previous_camera,
            components.previous_viewport,
            components.previous_jitter,
            summary.history_available,
            summary.history_rejected,
            contract
                .history_primary_hit_attachment
                .as_ref()
                .is_some_and(|name| arena.attachment_buffer(name.as_str()).is_some()),
        ),
    )
    .map_err(PresentationExecError::Query)?;
    let config_buffer = storage_buffer_with_usage_and_bytes(
        native,
        "wrela.presentation.motion.config",
        &config_bytes,
        wgpu::BufferUsages::STORAGE,
        gpu_runtime,
    );
    let counts_buffer = zeroed_storage_buffer(
        native,
        "wrela.presentation.motion.counts",
        12,
        wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        gpu_runtime,
    );
    let cached = create_custom_pass_pipeline(
        native,
        &motion_resolve_gpu_shader_source(workgroup_size)?,
        workgroup_size,
        &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(config_bytes.len().max(4) as u64),
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
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 4,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
        "wrela.presentation.motion.pipeline",
        gpu_runtime,
    )?;
    gpu_runtime.transient_bind_group_creations =
        gpu_runtime.transient_bind_group_creations.saturating_add(1);
    let bind_group = native.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("wrela.presentation.motion.bind_group"),
        layout: &cached.bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: config_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: primary_hit_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: history_hit_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: output_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: counts_buffer.as_entire_binding(),
            },
        ],
    });
    let timestamp_writes = profiler.compute_pass_timestamp_writes();
    let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
        label: Some("wrela.presentation.motion.compute"),
        timestamp_writes,
    });
    pass.set_pipeline(&cached.pipeline);
    pass.set_bind_group(GPU_RUNTIME_PASS_BIND_GROUP_INDEX, &bind_group, &[]);
    pass.dispatch_workgroups(
        viewport
            .width
            .saturating_mul(viewport.height)
            .div_ceil(workgroup_size.max(1))
            .max(1),
        1,
        1,
    );
    Ok(MotionResolveGpuDispatch {
        dispatch_count: 1,
        counts_buffer,
        counts_readback_label: "wrela.presentation.motion.counts".to_string(),
    })
}

pub(super) fn encode_temporal_resolve_gpu(
    native: &crate::query_exec::wgsl::NativeWgpuContext,
    encoder: &mut wgpu::CommandEncoder,
    profiler: &mut GpuPassProfiler,
    arena: &crate::presentation_exec::gpu_resources::GpuAttachmentArena,
    width: u32,
    height: u32,
    contract: &TemporalResolvePassContract,
    workgroup_size: u32,
    gpu_runtime: &mut GpuRuntimeMetrics,
) -> Result<TemporalResolveGpuDispatch, PresentationExecError> {
    let shader_f16_enabled = native
        .requested_features
        .contains(wgpu::Features::SHADER_F16);
    let Some(current_color_buffer) = arena.attachment_buffer(contract.input_attachment.as_str())
    else {
        return Ok(TemporalResolveGpuDispatch {
            dispatch_count: 0,
            counts_buffer: zeroed_storage_buffer(
                native,
                "wrela.presentation.temporal.counts.empty",
                4,
                wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
                gpu_runtime,
            ),
            counts_readback_label: "wrela.presentation.temporal.counts".to_string(),
        });
    };
    let Some(history_color_buffer) =
        arena.attachment_buffer(contract.history_color_attachment.as_str())
    else {
        return Ok(TemporalResolveGpuDispatch {
            dispatch_count: 0,
            counts_buffer: zeroed_storage_buffer(
                native,
                "wrela.presentation.temporal.counts.empty",
                4,
                wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
                gpu_runtime,
            ),
            counts_readback_label: "wrela.presentation.temporal.counts".to_string(),
        });
    };
    let Some(motion_buffer) = arena.attachment_buffer(contract.motion_attachment.as_str()) else {
        return Ok(TemporalResolveGpuDispatch {
            dispatch_count: 0,
            counts_buffer: zeroed_storage_buffer(
                native,
                "wrela.presentation.temporal.counts.empty",
                4,
                wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
                gpu_runtime,
            ),
            counts_readback_label: "wrela.presentation.temporal.counts".to_string(),
        });
    };
    let Some(output_slot) = arena.attachment(contract.output_attachment.as_str()) else {
        return Ok(TemporalResolveGpuDispatch {
            dispatch_count: 0,
            counts_buffer: zeroed_storage_buffer(
                native,
                "wrela.presentation.temporal.counts.empty",
                4,
                wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
                gpu_runtime,
            ),
            counts_readback_label: "wrela.presentation.temporal.counts".to_string(),
        });
    };
    let output_buffer =
        output_slot
            .gpu_buffer()
            .ok_or_else(|| PresentationExecError::UnsupportedPlan {
                message: format!(
                    "attachment '{}' is not GPU-backed",
                    contract.output_attachment
                ),
            })?;
    let config_bytes = encode_value(
        &temporal_resolve_gpu_config_abi(),
        &temporal_resolve_gpu_config_value(width, height, contract),
    )
    .map_err(PresentationExecError::Query)?;
    let config_buffer = storage_buffer_with_usage_and_bytes(
        native,
        "wrela.presentation.temporal.config",
        &config_bytes,
        wgpu::BufferUsages::STORAGE,
        gpu_runtime,
    );
    let counts_buffer = zeroed_storage_buffer(
        native,
        "wrela.presentation.temporal.counts",
        4,
        wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        gpu_runtime,
    );
    let cached = create_custom_pass_pipeline(
        native,
        &temporal_resolve_gpu_shader_source(workgroup_size, shader_f16_enabled)?,
        workgroup_size,
        &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(config_bytes.len().max(4) as u64),
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
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
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
            wgpu::BindGroupLayoutEntry {
                binding: 4,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 5,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
        "wrela.presentation.temporal.pipeline",
        gpu_runtime,
    )?;
    gpu_runtime.transient_bind_group_creations =
        gpu_runtime.transient_bind_group_creations.saturating_add(1);
    let bind_group = native.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("wrela.presentation.temporal.bind_group"),
        layout: &cached.bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: config_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: current_color_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: history_color_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: motion_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: output_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: counts_buffer.as_entire_binding(),
            },
        ],
    });
    let timestamp_writes = profiler.compute_pass_timestamp_writes();
    let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
        label: Some("wrela.presentation.temporal.compute"),
        timestamp_writes,
    });
    pass.set_pipeline(&cached.pipeline);
    pass.set_bind_group(GPU_RUNTIME_PASS_BIND_GROUP_INDEX, &bind_group, &[]);
    pass.dispatch_workgroups(
        width
            .saturating_mul(height)
            .div_ceil(workgroup_size.max(1))
            .max(1),
        1,
        1,
    );
    drop(pass);
    if contract.output_attachment != contract.history_color_attachment {
        encoder.copy_buffer_to_buffer(
            output_buffer,
            0,
            history_color_buffer,
            0,
            output_slot.layout.total_size as u64,
        );
    }
    if let Some(history_primary_hit_attachment) = &contract.history_primary_hit_attachment
        && let (Some(primary_hit_buffer), Some(history_primary_hit_buffer)) = (
            arena.attachment_buffer(contract.primary_hit_attachment.as_str()),
            arena.attachment_buffer(history_primary_hit_attachment.as_str()),
        )
        && let Some(primary_hit_slot) = arena.attachment(contract.primary_hit_attachment.as_str())
    {
        encoder.copy_buffer_to_buffer(
            primary_hit_buffer,
            0,
            history_primary_hit_buffer,
            0,
            primary_hit_slot.layout.total_size as u64,
        );
    }
    Ok(TemporalResolveGpuDispatch {
        dispatch_count: 1,
        counts_buffer,
        counts_readback_label: "wrela.presentation.temporal.counts".to_string(),
    })
}

pub(super) fn encode_composite_color_gpu(
    native: &crate::query_exec::wgsl::NativeWgpuContext,
    encoder: &mut wgpu::CommandEncoder,
    profiler: &mut GpuPassProfiler,
    arena: &crate::presentation_exec::gpu_resources::GpuAttachmentArena,
    contract: &CompositeColorPassContract,
    workgroup_size: u32,
    gpu_runtime: &mut GpuRuntimeMetrics,
) -> Result<u32, PresentationExecError> {
    let shader_f16_enabled = native
        .requested_features
        .contains(wgpu::Features::SHADER_F16);
    let Some(input_buffer) = arena.attachment_buffer(contract.input_attachment.as_str()) else {
        return Ok(0);
    };
    let Some(output_buffer) = arena.attachment_buffer(contract.output_attachment.as_str()) else {
        return Ok(0);
    };
    let item_count = arena
        .attachment(contract.output_attachment.as_str())
        .map(|slot| slot.layout.width.saturating_mul(slot.layout.height))
        .unwrap_or_default();
    let dispatch_bytes = encode_value(
        &crate::query_exec::wgsl::codegen::wgsl_dispatch_config_abi(),
        &presentation_dispatch_config(item_count),
    )
    .map_err(PresentationExecError::Query)?;
    let config_buffer = storage_buffer_with_usage_and_bytes(
        native,
        "wrela.presentation.composite.dispatch",
        &dispatch_bytes,
        wgpu::BufferUsages::STORAGE,
        gpu_runtime,
    );
    let dummy_buffer = zeroed_storage_buffer(
        native,
        "wrela.presentation.composite.dummy",
        4,
        wgpu::BufferUsages::STORAGE,
        gpu_runtime,
    );
    let cached = create_custom_pass_pipeline(
        native,
        &copy_vec3_shader_source(workgroup_size, shader_f16_enabled)?,
        workgroup_size,
        &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(dispatch_bytes.len().max(4) as u64),
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
        ],
        "wrela.presentation.composite.pipeline",
        gpu_runtime,
    )?;
    gpu_runtime.transient_bind_group_creations =
        gpu_runtime.transient_bind_group_creations.saturating_add(1);
    let bind_group = native.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("wrela.presentation.composite.bind_group"),
        layout: &cached.bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: config_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: input_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: output_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: dummy_buffer.as_entire_binding(),
            },
        ],
    });
    let timestamp_writes = profiler.compute_pass_timestamp_writes();
    let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
        label: Some("wrela.presentation.composite.compute"),
        timestamp_writes,
    });
    pass.set_pipeline(&cached.pipeline);
    pass.set_bind_group(GPU_RUNTIME_PASS_BIND_GROUP_INDEX, &bind_group, &[]);
    pass.dispatch_workgroups(item_count.div_ceil(workgroup_size.max(1)).max(1), 1, 1);
    Ok(1)
}

#[cfg(test)]
pub(super) fn legacy_test_only_shade_primary_wgsl(
    screen_samples: &[KernelValue],
    attachments: &mut AttachmentResourceSet,
    lighting: &crate::presentation_contract::PresentationLightingInputs,
    camera_position: [f32; 3],
    contract: &ShadePrimaryPassContract,
    workgroup_size: u32,
    gpu_runtime: &mut GpuRuntimeMetrics,
) -> Result<u32, PresentationExecError> {
    let primary_hits = attachments.decode_attachment(contract.primary_hit_attachment.as_str())?;
    gpu_runtime.attachment_decode_count = gpu_runtime.attachment_decode_count.saturating_add(1);
    let default_surface = default_surface();
    let default_medium = default_medium();
    let shade_inputs = primary_hits
        .iter()
        .enumerate()
        .map(|(index, hit)| {
            let sample = screen_samples.get(index).expect("screen sample");
            let ray = expect_struct(
                field(expect_struct(sample, "ScreenSampleQuery")?, "ray")?,
                "RayQuery",
            )?;
            let ray_direction = expect_vec3(field(ray, "direction")?)?;
            let surface = shade_lookup_value(
                attachments,
                contract.surface_attachment.as_str(),
                index,
                &default_surface,
            )?;
            Ok(KernelValue::Struct(KernelStructValue {
                name: SmolStr::new("ShadePrimaryInput"),
                fields: vec![
                    (SmolStr::new("hit"), hit.clone()),
                    (SmolStr::new("surface"), surface),
                    (
                        SmolStr::new("radiance"),
                        contract
                            .radiance_attachment
                            .as_ref()
                            .map(|name| {
                                shade_lookup_value(
                                    attachments,
                                    name,
                                    index,
                                    &KernelValue::Vec3([0.0, 0.0, 0.0]),
                                )
                            })
                            .transpose()?
                            .unwrap_or(KernelValue::Vec3([0.0, 0.0, 0.0])),
                    ),
                    (
                        SmolStr::new("medium"),
                        contract
                            .medium_attachment
                            .as_ref()
                            .map(|name| {
                                shade_lookup_value(attachments, name, index, &default_medium)
                            })
                            .transpose()?
                            .unwrap_or_else(|| default_medium.clone()),
                    ),
                    (
                        SmolStr::new("ray_direction"),
                        KernelValue::Vec3(ray_direction),
                    ),
                    (
                        SmolStr::new("camera_position"),
                        KernelValue::Vec3(camera_position),
                    ),
                    (SmolStr::new("lighting"), lighting_inputs_value(*lighting)),
                ],
            }))
        })
        .collect::<Result<Vec<_>, PresentationExecError>>()?;

    let output_layout = attachments
        .attachment(contract.output_attachment.as_str())
        .ok_or_else(|| PresentationExecError::UnsupportedPlan {
            message: format!(
                "missing shade output attachment '{}'",
                contract.output_attachment
            ),
        })?
        .layout
        .plan
        .clone();
    let output_attachment = attachments
        .attachment_mut(contract.output_attachment.as_str())
        .ok_or_else(|| PresentationExecError::UnsupportedPlan {
            message: format!(
                "missing shade output attachment '{}'",
                contract.output_attachment
            ),
        })?;
    let dispatch = legacy_test_only_dispatch_linear_shader(
        &shade_primary_shader_source(workgroup_size, false)?,
        &shade_primary_input_abi(),
        &shade_inputs,
        &output_layout,
        workgroup_size,
        gpu_runtime,
    )?;
    output_attachment.bytes = dispatch.bytes.into();
    gpu_runtime.attachment_encode_count = gpu_runtime.attachment_encode_count.saturating_add(1);
    Ok(dispatch.dispatch_count)
}

#[cfg(test)]
pub(super) fn legacy_test_only_composite_color_wgsl(
    attachments: &mut AttachmentResourceSet,
    contract: &CompositeColorPassContract,
    workgroup_size: u32,
    gpu_runtime: &mut GpuRuntimeMetrics,
) -> Result<u32, PresentationExecError> {
    let input_values = attachments.decode_attachment(contract.input_attachment.as_str())?;
    gpu_runtime.attachment_decode_count = gpu_runtime.attachment_decode_count.saturating_add(1);
    let output_layout = attachments
        .attachment(contract.output_attachment.as_str())
        .ok_or_else(|| PresentationExecError::UnsupportedPlan {
            message: format!(
                "missing composite output attachment '{}'",
                contract.output_attachment
            ),
        })?
        .layout
        .plan
        .clone();
    let output_attachment = attachments
        .attachment_mut(contract.output_attachment.as_str())
        .ok_or_else(|| PresentationExecError::UnsupportedPlan {
            message: format!(
                "missing composite output attachment '{}'",
                contract.output_attachment
            ),
        })?;
    let dispatch = legacy_test_only_dispatch_linear_shader(
        &copy_vec3_shader_source(workgroup_size, false)?,
        &PortableAbiType::Vec3,
        &input_values,
        &output_layout,
        workgroup_size,
        gpu_runtime,
    )?;
    output_attachment.bytes = dispatch.bytes.into();
    gpu_runtime.attachment_encode_count = gpu_runtime.attachment_encode_count.saturating_add(1);
    Ok(dispatch.dispatch_count)
}

#[cfg(test)]
pub(super) fn legacy_test_only_temporal_resolve_wgsl(
    attachments: &mut AttachmentResourceSet,
    width: u32,
    height: u32,
    contract: &TemporalResolvePassContract,
    workgroup_size: u32,
    gpu_runtime: &mut GpuRuntimeMetrics,
) -> Result<LegacyTestOnlyTemporalResolveDispatchResult, PresentationExecError> {
    let (input_values, consumed_count) =
        temporal_resolve_kernel_values(attachments, width, height, contract)?;
    let output_layout = attachments
        .attachment(contract.output_attachment.as_str())
        .ok_or_else(|| PresentationExecError::UnsupportedPlan {
            message: format!(
                "missing temporal resolve output attachment '{}'",
                contract.output_attachment
            ),
        })?
        .layout
        .plan
        .clone();
    if input_values.is_empty() {
        return Ok(LegacyTestOnlyTemporalResolveDispatchResult {
            consumed_count,
            dispatch_count: 0,
        });
    }
    let dispatch = legacy_test_only_dispatch_linear_shader(
        &temporal_resolve_shader_source(contract, workgroup_size, false)?,
        &temporal_resolve_input_abi(),
        &input_values,
        &output_layout,
        workgroup_size,
        gpu_runtime,
    )?;
    attachments
        .attachment_mut(contract.output_attachment.as_str())
        .expect("temporal output attachment")
        .bytes = dispatch.bytes.clone().into();
    gpu_runtime.attachment_encode_count = gpu_runtime.attachment_encode_count.saturating_add(2);
    if let Some(history_color) =
        attachments.attachment_mut(contract.history_color_attachment.as_str())
    {
        history_color.bytes = dispatch.bytes.into();
    }
    if let Some(history_primary_hit_attachment) = &contract.history_primary_hit_attachment {
        let primary_hits =
            attachments.decode_attachment(contract.primary_hit_attachment.as_str())?;
        gpu_runtime.attachment_decode_count = gpu_runtime.attachment_decode_count.saturating_add(1);
        if let Some(history_primary_hit) =
            attachments.attachment_mut(history_primary_hit_attachment.as_str())
        {
            for (index, hit) in primary_hits.iter().enumerate() {
                history_primary_hit.encode(index, hit)?;
                gpu_runtime.attachment_encode_count =
                    gpu_runtime.attachment_encode_count.saturating_add(1);
            }
        }
    }
    Ok(LegacyTestOnlyTemporalResolveDispatchResult {
        consumed_count,
        dispatch_count: dispatch.dispatch_count,
    })
}

#[cfg(test)]
pub(super) fn legacy_test_only_dispatch_linear_shader(
    source: &str,
    input_abi: &PortableAbiType,
    input_values: &[KernelValue],
    output_layout: &FrameAttachmentLayoutPlan,
    workgroup_size: u32,
    gpu_runtime: &mut GpuRuntimeMetrics,
) -> Result<LegacyTestOnlyLinearShaderDispatchResult, PresentationExecError> {
    legacy_test_only_dispatch_linear_shader_with_chunk_limit(
        source,
        input_abi,
        input_values,
        output_layout,
        workgroup_size,
        None,
        gpu_runtime,
    )
}

// Legacy/test-only helper for CPU-bounce WGSL verification paths. The timed resident framegraph
// must not route through this immediate-readback loop, so we compile it only for tests.
#[cfg(test)]
pub(super) fn legacy_test_only_dispatch_linear_shader_with_chunk_limit(
    source: &str,
    input_abi: &PortableAbiType,
    input_values: &[KernelValue],
    output_layout: &FrameAttachmentLayoutPlan,
    workgroup_size: u32,
    per_storage_buffer_limit_override: Option<u64>,
    gpu_runtime: &mut GpuRuntimeMetrics,
) -> Result<LegacyTestOnlyLinearShaderDispatchResult, PresentationExecError> {
    if input_values.is_empty() {
        return Ok(LegacyTestOnlyLinearShaderDispatchResult {
            bytes: Vec::new(),
            dispatch_count: 0,
        });
    }
    let dense_output_size = output_layout.dense_output_size() as u64;
    let native = native_wgpu_context()?;
    let dispatch_abi = crate::query_exec::wgsl::codegen::wgsl_dispatch_config_abi();
    let per_storage_buffer_limit = per_storage_buffer_limit_override.unwrap_or_else(|| {
        native
            .requested_limits
            .max_storage_buffer_binding_size
            .min(native.requested_limits.max_buffer_size)
    });
    let items_per_chunk = crate::query_exec::wgsl::max_chunk_item_count(
        per_storage_buffer_limit,
        portable_abi_array_stride(input_abi) as u64,
        output_layout.physical.element_stride as u64,
        None,
    )
    .map_err(PresentationExecError::Query)?;
    let chunk_count = input_values.len().div_ceil(items_per_chunk);
    let mut profiler = GpuPassProfiler::new(&native, chunk_count as u32);
    let mut local_gpu_runtime = GpuRuntimeMetrics {
        ..GpuRuntimeMetrics::default()
    };
    local_gpu_runtime.note_context_metadata(&native);
    let cached = compiled_pipeline(
        &native,
        source,
        workgroup_size,
        GPU_RUNTIME_PASS_BIND_GROUP_INDEX,
        wgpu::BufferSize::new(portable_abi_layout(&dispatch_abi).size as u64),
        &mut local_gpu_runtime,
    )?;
    let dispatch_bytes_size = portable_abi_layout(&dispatch_abi).size as u64;
    let input_buffer_size =
        ((items_per_chunk as u64) * portable_abi_array_stride(input_abi) as u64).max(4);
    let output_buffer_size =
        ((items_per_chunk as u64) * output_layout.physical.element_stride as u64).max(4);
    let mut leased_buffers = Vec::new();
    let (aux_buffer, aux_pool_key) = acquire_presentation_upload_buffer(
        &native,
        4,
        wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        Some("wrela.presentation.aux"),
        &mut local_gpu_runtime,
    );
    leased_buffers.push((aux_pool_key, aux_buffer.clone()));
    let (dispatch_buffer, dispatch_pool_key) = acquire_presentation_upload_buffer(
        &native,
        dispatch_bytes_size,
        wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        Some("wrela.presentation.dispatch"),
        &mut local_gpu_runtime,
    );
    leased_buffers.push((dispatch_pool_key, dispatch_buffer.clone()));
    let (input_buffer, input_pool_key) = acquire_presentation_upload_buffer(
        &native,
        input_buffer_size,
        wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        Some("wrela.presentation.input"),
        &mut local_gpu_runtime,
    );
    leased_buffers.push((input_pool_key, input_buffer.clone()));
    let (output_buffer, output_pool_key) = acquire_presentation_upload_buffer(
        &native,
        output_buffer_size,
        wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        Some("wrela.presentation.output"),
        &mut local_gpu_runtime,
    );
    leased_buffers.push((output_pool_key, output_buffer.clone()));
    let mut upload_arena = lock_shared_upload_arena(
        native.limit_request,
        &native.device,
        dispatch_bytes_size.max(input_buffer_size).max(4),
    );
    upload_arena.set_scratch_encoder(native.device.create_command_encoder(
        &wgpu::CommandEncoderDescriptor {
            label: Some("wrela.presentation.upload_init"),
        },
    ));
    local_gpu_runtime.upload_bytes = local_gpu_runtime.upload_bytes.saturating_add(
        upload_arena
            .write_storage_bytes(&aux_buffer, 0, &[0u8; 4])
            .map_err(|err| {
                PresentationExecError::Query(crate::query_exec::cpu::QueryExecError::Unsupported {
                    message: format!("presentation aux upload failed: {err:?}"),
                })
            })?,
    );
    if let Some(upload_commands) = upload_arena.finish() {
        native.queue.submit(Some(upload_commands));
        local_gpu_runtime.queue_submit_count =
            local_gpu_runtime.queue_submit_count.saturating_add(1);
    }
    local_gpu_runtime.transient_bind_group_creations = local_gpu_runtime
        .transient_bind_group_creations
        .saturating_add(1);
    let bind_group = native.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("wrela.presentation.bind_group"),
        layout: &cached.bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: dispatch_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: input_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: output_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: aux_buffer.as_entire_binding(),
            },
        ],
    });
    let mut dense_bytes = vec![0u8; dense_output_size as usize];
    let mut dispatch_count = 0u32;
    for (chunk_index, chunk) in input_values.chunks(items_per_chunk).enumerate() {
        let chunk_stride = output_layout.physical.element_stride as usize;
        let chunk_start = chunk_index * items_per_chunk;
        let chunk_dense_size = (chunk.len() * chunk_stride).max(4) as u64;
        let dispatch_bytes = encode_value(
            &dispatch_abi,
            &presentation_dispatch_config(chunk.len() as u32),
        )
        .map_err(PresentationExecError::Query)?;
        let input_bytes = encode_slice(input_abi, chunk).map_err(PresentationExecError::Query)?;
        upload_arena.set_scratch_encoder(native.device.create_command_encoder(
            &wgpu::CommandEncoderDescriptor {
                label: Some("wrela.presentation.upload_encoder"),
            },
        ));
        local_gpu_runtime.upload_bytes = local_gpu_runtime
            .upload_bytes
            .saturating_add(
                upload_arena
                    .write_storage_bytes(&dispatch_buffer, 0, &dispatch_bytes)
                    .map_err(|err| {
                        PresentationExecError::Query(
                            crate::query_exec::cpu::QueryExecError::Unsupported {
                                message: format!("presentation dispatch upload failed: {err:?}"),
                            },
                        )
                    })?,
            )
            .saturating_add(
                upload_arena
                    .write_storage_bytes(&input_buffer, 0, &input_bytes)
                    .map_err(|err| {
                        PresentationExecError::Query(
                            crate::query_exec::cpu::QueryExecError::Unsupported {
                                message: format!("presentation input upload failed: {err:?}"),
                            },
                        )
                    })?,
            );
        if let Some(upload_commands) = upload_arena.finish() {
            native.queue.submit(Some(upload_commands));
            local_gpu_runtime.queue_submit_count =
                local_gpu_runtime.queue_submit_count.saturating_add(1);
        }
        let mut encoder = native
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("wrela.presentation.encoder"),
            });
        {
            let timestamp_writes = profiler.compute_pass_timestamp_writes();
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("wrela.presentation.compute"),
                timestamp_writes,
            });
            pass.set_pipeline(&cached.pipeline);
            pass.set_bind_group(GPU_RUNTIME_PASS_BIND_GROUP_INDEX, &bind_group, &[]);
            pass.dispatch_workgroups(
                (chunk.len() as u32).div_ceil(workgroup_size.max(1)).max(1),
                1,
                1,
            );
        }
        profiler.resolve_into(&mut encoder);
        native.queue.submit(Some(encoder.finish()));
        dispatch_count = dispatch_count.saturating_add(1);
        local_gpu_runtime.queue_submit_count =
            local_gpu_runtime.queue_submit_count.saturating_add(1);
        let chunk_bytes = legacy_test_only_readback_storage_buffer(
            &output_buffer,
            chunk_dense_size,
        )
        .map_err(|message| {
            PresentationExecError::Query(crate::query_exec::cpu::QueryExecError::Unsupported {
                message: format!("native WGSL readback failed: {message}"),
            })
        })?;
        upload_arena.recall();
        local_gpu_runtime.queue_submit_count =
            local_gpu_runtime.queue_submit_count.saturating_add(1);
        local_gpu_runtime.transient_buffer_creations = local_gpu_runtime
            .transient_buffer_creations
            .saturating_add(1);
        local_gpu_runtime.readback_bytes = local_gpu_runtime
            .readback_bytes
            .saturating_add(chunk_dense_size);
        let chunk_byte_offset = chunk_start * chunk_stride;
        let chunk_byte_end = chunk_byte_offset + chunk.len() * chunk_stride;
        dense_bytes[chunk_byte_offset..chunk_byte_end]
            .copy_from_slice(&chunk_bytes[..chunk_byte_end - chunk_byte_offset]);
    }
    upload_arena.recall();
    release_presentation_upload_buffers(&native, leased_buffers);
    let gpu_elapsed_micros = profiler
        .readback_gpu_elapsed_micros(&native)
        .map_err(|message| {
            PresentationExecError::Query(crate::query_exec::cpu::QueryExecError::Unsupported {
                message: format!("native WGSL GPU timing readback failed: {message}"),
            })
        })?;
    local_gpu_runtime.note_gpu_timings(profiler.timestamps_supported(), &gpu_elapsed_micros);
    if profiler.timestamps_supported() {
        local_gpu_runtime.queue_submit_count =
            local_gpu_runtime.queue_submit_count.saturating_add(1);
        local_gpu_runtime.transient_buffer_creations = local_gpu_runtime
            .transient_buffer_creations
            .saturating_add(1);
        local_gpu_runtime.readback_bytes = local_gpu_runtime
            .readback_bytes
            .saturating_add((gpu_elapsed_micros.len() as u64) * 16);
    }
    gpu_runtime.merge_from(&local_gpu_runtime);
    Ok(LegacyTestOnlyLinearShaderDispatchResult {
        bytes: output_layout
            .pack_dense_output_bytes(&dense_bytes)
            .map_err(PresentationExecError::Resource)?,
        dispatch_count,
    })
}
