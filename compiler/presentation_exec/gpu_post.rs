use crate::gpu_runtime::{GpuPassProfiler, GpuRuntimeMetrics};
use crate::kernel::{KernelStructValue, KernelValue, lower_batch_query_plan};
use crate::portable::{PortableAbiType, PortableStructField, portable_builtin_record_abi};
use crate::presentation_contract::{
    CanonicalCameraInput, CanonicalViewportInput, LegacyCompatibilityProjectionInput,
    RealtimeRadianceMode,
};
use crate::presentation_exec::gpu_resources::GpuAttachmentArena;
use crate::presentation_exec::{
    PresentationExecError, PresentationExecutionInput, expect_struct, expect_vec3, field,
};
use crate::presentation_plan::{ParticipantsResolvePassContract, SurfaceResolvePassContract};
use crate::query_exec::QueryExecContext;
use crate::query_exec::gpu_dispatch::GpuQueryDispatcher;
use crate::query_plan::SceneSummary;
use crate::query_plan::{BatchQueryPlan, DispatchBackend};
use smol_str::SmolStr;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

#[derive(Debug, Clone)]
struct SurfaceResolveGpuConfig {
    source_width: u32,
    source_height: u32,
    target_width: u32,
    target_height: u32,
    divisor_x: u32,
    divisor_y: u32,
}

#[derive(Debug, Clone)]
struct ParticipantResolveGpuConfig {
    source_width: u32,
    source_height: u32,
    target_width: u32,
    target_height: u32,
    divisor_x: u32,
    divisor_y: u32,
    viewport_width: u32,
    viewport_height: u32,
    camera_position: [f32; 3],
    forward: [f32; 3],
    up: [f32; 3],
    vertical_fov_degrees: f32,
    jitter: [f32; 2],
    legacy_world_up: [f32; 3],
    legacy_view_scale: f32,
    legacy_active: bool,
    include_misses: bool,
    miss_sample_distance: f32,
}

pub(super) fn encode_surface_resolve_gpu(
    native: &crate::query_exec::wgsl::NativeWgpuContext,
    encoder: &mut wgpu::CommandEncoder,
    profiler: &mut GpuPassProfiler,
    arena: &GpuAttachmentArena,
    ctx: &QueryExecContext,
    input: &PresentationExecutionInput,
    contract: &SurfaceResolvePassContract,
    compact_hits: bool,
    workgroup_size: u32,
    gpu_runtime: &mut GpuRuntimeMetrics,
) -> Result<(u32, u32, Vec<String>), PresentationExecError> {
    let Some(primary_hit_slot) = arena.attachment(contract.primary_hit_attachment.as_str()) else {
        return Err(PresentationExecError::UnsupportedPlan {
            message: format!(
                "missing GPU primary-hit attachment '{}'",
                contract.primary_hit_attachment
            ),
        });
    };
    let Some(surface_slot) = arena.attachment(contract.surface_attachment.as_str()) else {
        return Ok((0, 0, Vec::new()));
    };
    let Some(primary_hit_buffer) = primary_hit_slot.gpu_buffer() else {
        return Err(PresentationExecError::UnsupportedPlan {
            message: format!(
                "attachment '{}' is not GPU-backed",
                contract.primary_hit_attachment
            ),
        });
    };
    let Some(surface_buffer) = surface_slot.gpu_buffer() else {
        return Err(PresentationExecError::UnsupportedPlan {
            message: format!(
                "attachment '{}' is not GPU-backed",
                contract.surface_attachment
            ),
        });
    };
    let source_work_items = primary_hit_slot
        .layout
        .width
        .saturating_mul(primary_hit_slot.layout.height);
    let work_item_count = if compact_hits
        || surface_slot.layout.width != primary_hit_slot.layout.width
        || surface_slot.layout.height != primary_hit_slot.layout.height
    {
        surface_slot
            .layout
            .width
            .saturating_mul(surface_slot.layout.height)
    } else {
        source_work_items
    };
    let mut notes = Vec::new();
    let source_width = primary_hit_slot.layout.width.max(1);
    let source_height = primary_hit_slot.layout.height.max(1);
    let target_width = surface_slot.layout.width.max(1);
    let target_height = surface_slot.layout.height.max(1);
    let divisor_x = surface_slot.layout.attachment.scale.divisor_x.max(1);
    let divisor_y = surface_slot.layout.attachment.scale.divisor_y.max(1);
    if compact_hits && work_item_count < source_work_items {
        notes.push(format!(
            "hit_compaction resident owner map {} of {} samples",
            work_item_count, source_work_items
        ));
    }
    if target_width != source_width || target_height != source_height {
        notes.push(format!("scaled_attachment={}", contract.surface_attachment));
    }
    let config = SurfaceResolveGpuConfig {
        source_width,
        source_height,
        target_width,
        target_height,
        divisor_x,
        divisor_y,
    };
    let owner_buffer = owner_buffer(
        native,
        encoder,
        ctx.wgsl_shader_cache_context_id,
        "wrela.presentation.surface.owner",
        target_width.saturating_mul(target_height),
        gpu_runtime,
    );
    let config_bytes = super::encode_value(
        &surface_resolve_gpu_config_abi(),
        &surface_resolve_gpu_config_value(&config),
    )
    .map_err(PresentationExecError::Query)?;
    let config_buffer = cached_storage_buffer_with_usage_and_bytes(
        native,
        ctx.wgsl_shader_cache_context_id,
        "wrela.presentation.surface.config",
        &config_bytes,
        wgpu::BufferUsages::STORAGE,
        gpu_runtime,
    );
    let batch_plan = lower_batch_query_plan(
        &BatchQueryPlan::for_contract(contract.query_contract, DispatchBackend::Wgsl, None)
            .map_err(|message| PresentationExecError::UnsupportedPlan {
                message: message.to_string(),
            })?,
    );
    let dispatcher = GpuQueryDispatcher::from_batch_plan_without_items(
        ctx,
        &batch_plan,
        &[input.region_capture_value(), input.frame_domain.clone()],
        target_width.saturating_mul(target_height).max(1),
    )
    .map_err(PresentationExecError::Query)?;
    gpu_runtime.merge_from(&dispatcher.initial_gpu_runtime());
    gpu_runtime.upload_bytes = gpu_runtime.upload_bytes.saturating_add(
        dispatcher
            .initialize_dispatch_state()
            .map_err(PresentationExecError::Query)?,
    );
    let input_buffer = dispatcher.input_buffer();
    let output_buffer = dispatcher.dispatch_result().values;

    encode_surface_owner_pass(
        native,
        encoder,
        profiler,
        &config_buffer,
        primary_hit_buffer,
        &owner_buffer,
        source_width.saturating_mul(source_height),
        workgroup_size,
        gpu_runtime,
    )?;
    encode_surface_build_pass(
        native,
        encoder,
        profiler,
        &config_buffer,
        primary_hit_buffer,
        &owner_buffer,
        &input_buffer.buffer,
        target_width.saturating_mul(target_height),
        workgroup_size,
        gpu_runtime,
    )?;
    dispatcher.encode_compute_pass(encoder, profiler);
    encode_surface_scatter_pass(
        native,
        encoder,
        profiler,
        &config_buffer,
        &owner_buffer,
        &output_buffer.buffer,
        surface_buffer,
        target_width.saturating_mul(target_height),
        workgroup_size,
        gpu_runtime,
    )?;
    Ok((work_item_count, 4, notes))
}

pub(super) fn encode_participants_resolve_gpu(
    native: &crate::query_exec::wgsl::NativeWgpuContext,
    encoder: &mut wgpu::CommandEncoder,
    profiler: &mut GpuPassProfiler,
    arena: &GpuAttachmentArena,
    ctx: &QueryExecContext,
    input: &PresentationExecutionInput,
    camera: CanonicalCameraInput,
    viewport: CanonicalViewportInput,
    jitter_pixels: [f32; 2],
    legacy_projection: bool,
    contract: &ParticipantsResolvePassContract,
    radiance_mode: RealtimeRadianceMode,
    workgroup_size: u32,
    gpu_runtime: &mut GpuRuntimeMetrics,
) -> Result<(u32, u32, u32, Vec<String>), PresentationExecError> {
    let frame = expect_struct(&input.frame_state, "FrameState")?;
    let view = expect_struct(field(frame, "view")?, "ViewState")?;
    let view_camera = expect_struct(field(view, "camera")?, "Camera")?;
    let camera_position = expect_vec3(field(view_camera, "position")?)?;
    let compatibility =
        input
            .compatibility_projection
            .unwrap_or(LegacyCompatibilityProjectionInput {
                world_up: camera.up,
                view_scale: 0.72,
            });
    let mut radiance_count = 0;
    let mut medium_count = 0;
    let mut dispatch_count = 0;
    let mut notes = Vec::new();

    if let (Some(query_contract), Some(attachment_name)) = (
        contract.radiance_query_contract,
        contract.radiance_attachment.as_deref(),
    ) {
        let include_misses = radiance_mode == RealtimeRadianceMode::Full;
        let target_work_items = arena
            .attachment(attachment_name)
            .map(|slot| slot.layout.width.saturating_mul(slot.layout.height))
            .unwrap_or_default();
        let (count, local_dispatches, local_notes) = encode_participant_lane_gpu(
            native,
            encoder,
            profiler,
            arena,
            ctx,
            input,
            contract.primary_hit_attachment.as_str(),
            attachment_name,
            query_contract,
            ParticipantLaneKind::Radiance,
            ParticipantResolveGpuConfig {
                source_width: viewport.width,
                source_height: viewport.height,
                target_width: arena
                    .attachment(attachment_name)
                    .map(|slot| slot.layout.width.max(1))
                    .unwrap_or(1),
                target_height: arena
                    .attachment(attachment_name)
                    .map(|slot| slot.layout.height.max(1))
                    .unwrap_or(1),
                divisor_x: arena
                    .attachment(attachment_name)
                    .map(|slot| slot.layout.attachment.scale.divisor_x.max(1))
                    .unwrap_or(1),
                divisor_y: arena
                    .attachment(attachment_name)
                    .map(|slot| slot.layout.attachment.scale.divisor_y.max(1))
                    .unwrap_or(1),
                viewport_width: viewport.width,
                viewport_height: viewport.height,
                camera_position,
                forward: camera.forward,
                up: camera.up,
                vertical_fov_degrees: camera.vertical_fov_degrees,
                jitter: jitter_pixels,
                legacy_world_up: compatibility.world_up,
                legacy_view_scale: compatibility.view_scale,
                legacy_active: legacy_projection,
                include_misses,
                miss_sample_distance: contract.miss_sample_distance,
            },
            if include_misses {
                target_work_items
            } else {
                target_work_items
            },
            workgroup_size,
            gpu_runtime,
        )?;
        radiance_count = count;
        dispatch_count += local_dispatches;
        notes.extend(local_notes);
        if radiance_mode == RealtimeRadianceMode::Reduced {
            notes.push(format!("radiance_mode=reduced items={radiance_count}"));
        }
    }

    if let (Some(query_contract), Some(attachment_name)) = (
        contract.medium_query_contract,
        contract.medium_attachment.as_deref(),
    ) {
        let (count, local_dispatches, local_notes) = encode_participant_lane_gpu(
            native,
            encoder,
            profiler,
            arena,
            ctx,
            input,
            contract.primary_hit_attachment.as_str(),
            attachment_name,
            query_contract,
            ParticipantLaneKind::Medium,
            ParticipantResolveGpuConfig {
                source_width: viewport.width,
                source_height: viewport.height,
                target_width: arena
                    .attachment(attachment_name)
                    .map(|slot| slot.layout.width.max(1))
                    .unwrap_or(1),
                target_height: arena
                    .attachment(attachment_name)
                    .map(|slot| slot.layout.height.max(1))
                    .unwrap_or(1),
                divisor_x: arena
                    .attachment(attachment_name)
                    .map(|slot| slot.layout.attachment.scale.divisor_x.max(1))
                    .unwrap_or(1),
                divisor_y: arena
                    .attachment(attachment_name)
                    .map(|slot| slot.layout.attachment.scale.divisor_y.max(1))
                    .unwrap_or(1),
                viewport_width: viewport.width,
                viewport_height: viewport.height,
                camera_position,
                forward: camera.forward,
                up: camera.up,
                vertical_fov_degrees: camera.vertical_fov_degrees,
                jitter: jitter_pixels,
                legacy_world_up: compatibility.world_up,
                legacy_view_scale: compatibility.view_scale,
                legacy_active: legacy_projection,
                include_misses: true,
                miss_sample_distance: contract.miss_sample_distance,
            },
            arena
                .attachment(attachment_name)
                .map(|slot| slot.layout.width.saturating_mul(slot.layout.height))
                .unwrap_or_default(),
            workgroup_size,
            gpu_runtime,
        )?;
        medium_count = count;
        dispatch_count += local_dispatches;
        notes.extend(local_notes);
    }

    if radiance_count == 0
        && contract.radiance_attachment.is_some()
        && radiance_mode == RealtimeRadianceMode::Reduced
    {
        radiance_count = arena
            .attachment(contract.radiance_attachment.as_deref().unwrap_or_default())
            .map(|slot| slot.layout.width.saturating_mul(slot.layout.height))
            .unwrap_or_default();
    }
    if medium_count == 0 && contract.medium_attachment.is_some() {
        medium_count = arena
            .attachment(contract.medium_attachment.as_deref().unwrap_or_default())
            .map(|slot| slot.layout.width.saturating_mul(slot.layout.height))
            .unwrap_or_default();
    }
    Ok((radiance_count, medium_count, dispatch_count, notes))
}

#[derive(Clone, Copy)]
enum ParticipantLaneKind {
    Radiance,
    Medium,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CachedGpuBufferKey {
    limits: crate::gpu_runtime::GpuLimitRequest,
    context_id: u64,
    label: String,
    size_bytes: u64,
    usage_bits: u32,
}

fn encode_participant_lane_gpu(
    native: &crate::query_exec::wgsl::NativeWgpuContext,
    encoder: &mut wgpu::CommandEncoder,
    profiler: &mut GpuPassProfiler,
    arena: &GpuAttachmentArena,
    ctx: &QueryExecContext,
    input: &PresentationExecutionInput,
    primary_hit_attachment: &str,
    attachment_name: &str,
    query_contract: crate::query_contract::QueryContractId,
    lane: ParticipantLaneKind,
    config: ParticipantResolveGpuConfig,
    expected_work_items: u32,
    workgroup_size: u32,
    gpu_runtime: &mut GpuRuntimeMetrics,
) -> Result<(u32, u32, Vec<String>), PresentationExecError> {
    let Some(primary_hit_buffer) = arena.attachment_buffer(primary_hit_attachment) else {
        return Err(PresentationExecError::UnsupportedPlan {
            message: format!("missing GPU primary-hit attachment '{primary_hit_attachment}'"),
        });
    };
    let Some(output_attachment) = arena.attachment(attachment_name) else {
        return Ok((0, 0, Vec::new()));
    };
    let Some(output_buffer) = output_attachment.gpu_buffer() else {
        return Err(PresentationExecError::UnsupportedPlan {
            message: format!("attachment '{attachment_name}' is not GPU-backed"),
        });
    };
    let mut notes = Vec::new();
    if output_attachment.layout.width != config.source_width
        || output_attachment.layout.height != config.source_height
    {
        notes.push(format!("scaled_attachment={attachment_name}"));
    }
    let identity_mapping = output_attachment.layout.width == config.source_width
        && output_attachment.layout.height == config.source_height;
    let config_bytes = super::encode_value(
        &participant_resolve_gpu_config_abi(),
        &participant_resolve_gpu_config_value(&config),
    )
    .map_err(PresentationExecError::Query)?;
    let config_buffer = cached_storage_buffer_with_usage_and_bytes(
        native,
        ctx.wgsl_shader_cache_context_id,
        &format!("wrela.presentation.{attachment_name}.config"),
        &config_bytes,
        wgpu::BufferUsages::STORAGE,
        gpu_runtime,
    );
    let batch_plan = lower_batch_query_plan(
        &BatchQueryPlan::for_contract(
            query_contract,
            DispatchBackend::Wgsl,
            participant_query_scene_summary(ctx, input),
        )
        .map_err(|message| PresentationExecError::UnsupportedPlan {
            message: message.to_string(),
        })?,
    );
    let item_count = config
        .target_width
        .saturating_mul(config.target_height)
        .max(1);
    let dispatcher = GpuQueryDispatcher::from_batch_plan_without_items(
        ctx,
        &batch_plan,
        &[input.region_capture_value(), input.frame_domain.clone()],
        item_count,
    )
    .map_err(PresentationExecError::Query)?;
    gpu_runtime.merge_from(&dispatcher.initial_gpu_runtime());
    gpu_runtime.upload_bytes = gpu_runtime.upload_bytes.saturating_add(
        dispatcher
            .initialize_dispatch_state()
            .map_err(PresentationExecError::Query)?,
    );
    let input_buffer = dispatcher.input_buffer();
    let query_output = dispatcher.dispatch_result().values;
    if identity_mapping && config.include_misses {
        encode_identity_participant_build_pass(
            native,
            encoder,
            profiler,
            &config_buffer,
            primary_hit_buffer,
            &input_buffer.buffer,
            item_count,
            workgroup_size,
            lane,
            gpu_runtime,
        )?;
        dispatcher.encode_compute_pass(encoder, profiler);
        encoder.copy_buffer_to_buffer(
            &query_output.buffer,
            0,
            output_buffer,
            0,
            query_output.size_bytes,
        );
        return Ok((expected_work_items, 2, notes));
    }
    let owner_buffer = owner_buffer(
        native,
        encoder,
        ctx.wgsl_shader_cache_context_id,
        &format!("wrela.presentation.{attachment_name}.owner"),
        config.target_width.saturating_mul(config.target_height),
        gpu_runtime,
    );
    encode_participant_owner_pass(
        native,
        encoder,
        profiler,
        &config_buffer,
        primary_hit_buffer,
        &owner_buffer,
        config.source_width.saturating_mul(config.source_height),
        workgroup_size,
        gpu_runtime,
    )?;
    match lane {
        ParticipantLaneKind::Radiance => {
            encode_radiance_item_build_pass(
                native,
                encoder,
                profiler,
                &config_buffer,
                primary_hit_buffer,
                &owner_buffer,
                &input_buffer.buffer,
                item_count,
                workgroup_size,
                gpu_runtime,
            )?;
        }
        ParticipantLaneKind::Medium => {
            encode_medium_item_build_pass(
                native,
                encoder,
                profiler,
                &config_buffer,
                primary_hit_buffer,
                &owner_buffer,
                &input_buffer.buffer,
                item_count,
                workgroup_size,
                gpu_runtime,
            )?;
        }
    }
    dispatcher.encode_compute_pass(encoder, profiler);
    match lane {
        ParticipantLaneKind::Radiance => encode_radiance_scatter_pass(
            native,
            encoder,
            profiler,
            &config_buffer,
            &owner_buffer,
            &query_output.buffer,
            output_buffer,
            item_count,
            workgroup_size,
            gpu_runtime,
        )?,
        ParticipantLaneKind::Medium => encode_medium_scatter_pass(
            native,
            encoder,
            profiler,
            &config_buffer,
            &owner_buffer,
            &query_output.buffer,
            output_buffer,
            item_count,
            workgroup_size,
            gpu_runtime,
        )?,
    }
    let work_items = expected_work_items;
    Ok((work_items, 4, notes))
}

fn participant_query_scene_summary(
    ctx: &QueryExecContext,
    input: &PresentationExecutionInput,
) -> Option<SceneSummary> {
    let detail = frame_domain_geometry_detail(&input.frame_domain).ok()?;
    ctx.region_scene_summary(input.region_capture_name(), detail)
}

fn frame_domain_geometry_detail(frame_domain: &KernelValue) -> Result<i32, PresentationExecError> {
    let frame_domain = expect_struct(frame_domain, "SceneDomain")?;
    let spatial = expect_struct(field(frame_domain, "spatial")?, "SpatialDomainContract")?;
    match field(spatial, "geometry_detail")? {
        KernelValue::I32(value) => Ok(*value),
        other => Err(PresentationExecError::TypeMismatch {
            expected: "I32".to_string(),
            found: match other {
                KernelValue::Nothing => "Nothing",
                KernelValue::Bool(_) => "Boolean",
                KernelValue::I32(_) => "I32",
                KernelValue::U32(_) => "U32",
                KernelValue::F32(_) => "F32",
                KernelValue::Vec2(_) => "Vec2",
                KernelValue::Vec3(_) => "Vec3",
                KernelValue::Vec4(_) => "Vec4",
                KernelValue::Mat3(_) => "Mat3",
                KernelValue::Mat4(_) => "Mat4",
                KernelValue::Quat(_) => "Quat",
                KernelValue::Array(_) => "Array",
                KernelValue::Struct(_) => "Struct",
                KernelValue::Capture(_) => "Capture",
                KernelValue::DispatchBackend(_) => "DispatchBackend",
                KernelValue::GpuBuffer(_) => "GpuBuffer",
                KernelValue::GpuAtomicI32(_) => "GpuAtomicI32",
                KernelValue::GpuAtomicU32(_) => "GpuAtomicU32",
            }
            .to_string(),
        }),
    }
}

fn owner_buffer(
    native: &crate::query_exec::wgsl::NativeWgpuContext,
    encoder: &mut wgpu::CommandEncoder,
    context_id: u64,
    label: &str,
    item_count: u32,
    gpu_runtime: &mut GpuRuntimeMetrics,
) -> wgpu::Buffer {
    let size_bytes = (item_count.max(1) as u64) * std::mem::size_of::<u32>() as u64;
    let buffer = cached_storage_buffer(
        native,
        context_id,
        label,
        size_bytes,
        wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        gpu_runtime,
    );
    encoder.clear_buffer(&buffer, 0, None);
    buffer
}

fn cached_storage_buffer_with_usage_and_bytes(
    native: &crate::query_exec::wgsl::NativeWgpuContext,
    context_id: u64,
    label: &str,
    bytes: &[u8],
    usage: wgpu::BufferUsages,
    gpu_runtime: &mut GpuRuntimeMetrics,
) -> wgpu::Buffer {
    let buffer = cached_storage_buffer(
        native,
        context_id,
        label,
        bytes.len().max(4) as u64,
        usage | wgpu::BufferUsages::COPY_DST,
        gpu_runtime,
    );
    if !bytes.is_empty() {
        native.queue.write_buffer(&buffer, 0, bytes);
        gpu_runtime.upload_bytes = gpu_runtime.upload_bytes.saturating_add(bytes.len() as u64);
    }
    buffer
}

fn cached_storage_buffer(
    native: &crate::query_exec::wgsl::NativeWgpuContext,
    context_id: u64,
    label: &str,
    size_bytes: u64,
    usage: wgpu::BufferUsages,
    gpu_runtime: &mut GpuRuntimeMetrics,
) -> wgpu::Buffer {
    static BUFFERS: OnceLock<Mutex<HashMap<CachedGpuBufferKey, wgpu::Buffer>>> = OnceLock::new();
    let buffers = BUFFERS.get_or_init(|| Mutex::new(HashMap::new()));
    let key = CachedGpuBufferKey {
        limits: native.limit_request,
        context_id,
        label: label.to_string(),
        size_bytes: size_bytes.max(4),
        usage_bits: usage.bits(),
    };
    if let Some(buffer) = buffers
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .get(&key)
        .cloned()
    {
        return buffer;
    }
    let buffer = native.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: key.size_bytes,
        usage,
        mapped_at_creation: false,
    });
    gpu_runtime.transient_buffer_creations =
        gpu_runtime.transient_buffer_creations.saturating_add(1);
    buffers
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .insert(key, buffer.clone());
    buffer
}

fn surface_resolve_gpu_config_abi() -> PortableAbiType {
    PortableAbiType::Struct {
        name: SmolStr::new("SurfaceResolveGpuConfig"),
        class_id: 0,
        fields: vec![
            PortableStructField {
                name: SmolStr::new("source_width"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("source_height"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("target_width"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("target_height"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("divisor_x"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("divisor_y"),
                ty: PortableAbiType::U32,
            },
        ],
    }
}

fn surface_resolve_gpu_config_value(config: &SurfaceResolveGpuConfig) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("SurfaceResolveGpuConfig"),
        fields: vec![
            (
                SmolStr::new("source_width"),
                KernelValue::U32(config.source_width),
            ),
            (
                SmolStr::new("source_height"),
                KernelValue::U32(config.source_height),
            ),
            (
                SmolStr::new("target_width"),
                KernelValue::U32(config.target_width),
            ),
            (
                SmolStr::new("target_height"),
                KernelValue::U32(config.target_height),
            ),
            (
                SmolStr::new("divisor_x"),
                KernelValue::U32(config.divisor_x),
            ),
            (
                SmolStr::new("divisor_y"),
                KernelValue::U32(config.divisor_y),
            ),
        ],
    })
}

fn participant_resolve_gpu_config_abi() -> PortableAbiType {
    PortableAbiType::Struct {
        name: SmolStr::new("ParticipantResolveGpuConfig"),
        class_id: 0,
        fields: vec![
            PortableStructField {
                name: SmolStr::new("source_width"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("source_height"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("target_width"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("target_height"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("divisor_x"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("divisor_y"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("viewport_width"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("viewport_height"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("camera_position"),
                ty: PortableAbiType::Vec3,
            },
            PortableStructField {
                name: SmolStr::new("forward"),
                ty: PortableAbiType::Vec3,
            },
            PortableStructField {
                name: SmolStr::new("up"),
                ty: PortableAbiType::Vec3,
            },
            PortableStructField {
                name: SmolStr::new("vertical_fov_degrees"),
                ty: PortableAbiType::F32,
            },
            PortableStructField {
                name: SmolStr::new("jitter"),
                ty: PortableAbiType::Vec2,
            },
            PortableStructField {
                name: SmolStr::new("legacy_world_up"),
                ty: PortableAbiType::Vec3,
            },
            PortableStructField {
                name: SmolStr::new("legacy_view_scale"),
                ty: PortableAbiType::F32,
            },
            PortableStructField {
                name: SmolStr::new("legacy_active"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("include_misses"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("miss_sample_distance"),
                ty: PortableAbiType::F32,
            },
        ],
    }
}

fn participant_resolve_gpu_config_value(config: &ParticipantResolveGpuConfig) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("ParticipantResolveGpuConfig"),
        fields: vec![
            (
                SmolStr::new("source_width"),
                KernelValue::U32(config.source_width),
            ),
            (
                SmolStr::new("source_height"),
                KernelValue::U32(config.source_height),
            ),
            (
                SmolStr::new("target_width"),
                KernelValue::U32(config.target_width),
            ),
            (
                SmolStr::new("target_height"),
                KernelValue::U32(config.target_height),
            ),
            (
                SmolStr::new("divisor_x"),
                KernelValue::U32(config.divisor_x),
            ),
            (
                SmolStr::new("divisor_y"),
                KernelValue::U32(config.divisor_y),
            ),
            (
                SmolStr::new("viewport_width"),
                KernelValue::U32(config.viewport_width),
            ),
            (
                SmolStr::new("viewport_height"),
                KernelValue::U32(config.viewport_height),
            ),
            (
                SmolStr::new("camera_position"),
                KernelValue::Vec3(config.camera_position),
            ),
            (SmolStr::new("forward"), KernelValue::Vec3(config.forward)),
            (SmolStr::new("up"), KernelValue::Vec3(config.up)),
            (
                SmolStr::new("vertical_fov_degrees"),
                KernelValue::F32(config.vertical_fov_degrees),
            ),
            (SmolStr::new("jitter"), KernelValue::Vec2(config.jitter)),
            (
                SmolStr::new("legacy_world_up"),
                KernelValue::Vec3(config.legacy_world_up),
            ),
            (
                SmolStr::new("legacy_view_scale"),
                KernelValue::F32(config.legacy_view_scale),
            ),
            (
                SmolStr::new("legacy_active"),
                KernelValue::U32(u32::from(config.legacy_active)),
            ),
            (
                SmolStr::new("include_misses"),
                KernelValue::U32(u32::from(config.include_misses)),
            ),
            (
                SmolStr::new("miss_sample_distance"),
                KernelValue::F32(config.miss_sample_distance),
            ),
        ],
    })
}

fn encode_identity_participant_build_pass(
    native: &crate::query_exec::wgsl::NativeWgpuContext,
    encoder: &mut wgpu::CommandEncoder,
    profiler: &mut GpuPassProfiler,
    config_buffer: &wgpu::Buffer,
    primary_hit_buffer: &wgpu::Buffer,
    input_buffer: &wgpu::Buffer,
    item_count: u32,
    workgroup_size: u32,
    lane: ParticipantLaneKind,
    gpu_runtime: &mut GpuRuntimeMetrics,
) -> Result<(), PresentationExecError> {
    let (source, pipeline_label, bind_group_label, dispatch_label) = match lane {
        ParticipantLaneKind::Radiance => (
            participant_identity_build_shader_source(workgroup_size, lane)?,
            "wrela.presentation.radiance.identity_build.pipeline",
            "wrela.presentation.radiance.identity_build.bind_group",
            "wrela.presentation.radiance.identity_build.compute",
        ),
        ParticipantLaneKind::Medium => (
            participant_identity_build_shader_source(workgroup_size, lane)?,
            "wrela.presentation.medium.identity_build.pipeline",
            "wrela.presentation.medium.identity_build.bind_group",
            "wrela.presentation.medium.identity_build.compute",
        ),
    };
    let cached = super::create_custom_pass_pipeline(
        native,
        &source,
        workgroup_size,
        &[
            storage_entry(0, true),
            storage_entry(1, true),
            storage_entry(2, false),
        ],
        pipeline_label,
        gpu_runtime,
    )?;
    let bind_group = create_bind_group(
        native,
        bind_group_label,
        &cached.bind_group_layout,
        &[
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
                resource: input_buffer.as_entire_binding(),
            },
        ],
        gpu_runtime,
    );
    dispatch_custom_pipeline(
        encoder,
        profiler,
        &cached.pipeline,
        &bind_group,
        item_count,
        workgroup_size,
        dispatch_label,
    );
    Ok(())
}

fn encode_surface_owner_pass(
    native: &crate::query_exec::wgsl::NativeWgpuContext,
    encoder: &mut wgpu::CommandEncoder,
    profiler: &mut GpuPassProfiler,
    config_buffer: &wgpu::Buffer,
    primary_hit_buffer: &wgpu::Buffer,
    owner_buffer: &wgpu::Buffer,
    item_count: u32,
    workgroup_size: u32,
    gpu_runtime: &mut GpuRuntimeMetrics,
) -> Result<(), PresentationExecError> {
    let cached = super::create_custom_pass_pipeline(
        native,
        &surface_owner_shader_source(workgroup_size)?,
        workgroup_size,
        &[
            storage_entry(0, true),
            storage_entry(1, true),
            storage_entry(2, false),
        ],
        "wrela.presentation.surface.owner.pipeline",
        gpu_runtime,
    )?;
    let bind_group = create_bind_group(
        native,
        "wrela.presentation.surface.owner.bind_group",
        &cached.bind_group_layout,
        &[
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
                resource: owner_buffer.as_entire_binding(),
            },
        ],
        gpu_runtime,
    );
    dispatch_custom_pipeline(
        encoder,
        profiler,
        &cached.pipeline,
        &bind_group,
        item_count,
        workgroup_size,
        "wrela.presentation.surface.owner.compute",
    );
    Ok(())
}

fn encode_surface_build_pass(
    native: &crate::query_exec::wgsl::NativeWgpuContext,
    encoder: &mut wgpu::CommandEncoder,
    profiler: &mut GpuPassProfiler,
    config_buffer: &wgpu::Buffer,
    primary_hit_buffer: &wgpu::Buffer,
    owner_buffer: &wgpu::Buffer,
    input_buffer: &wgpu::Buffer,
    item_count: u32,
    workgroup_size: u32,
    gpu_runtime: &mut GpuRuntimeMetrics,
) -> Result<(), PresentationExecError> {
    let cached = super::create_custom_pass_pipeline(
        native,
        &surface_build_shader_source(workgroup_size)?,
        workgroup_size,
        &[
            storage_entry(0, true),
            storage_entry(1, true),
            storage_entry(2, true),
            storage_entry(3, false),
        ],
        "wrela.presentation.surface.build.pipeline",
        gpu_runtime,
    )?;
    let bind_group = create_bind_group(
        native,
        "wrela.presentation.surface.build.bind_group",
        &cached.bind_group_layout,
        &[
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
                resource: owner_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: input_buffer.as_entire_binding(),
            },
        ],
        gpu_runtime,
    );
    dispatch_custom_pipeline(
        encoder,
        profiler,
        &cached.pipeline,
        &bind_group,
        item_count,
        workgroup_size,
        "wrela.presentation.surface.build.compute",
    );
    Ok(())
}

fn encode_surface_scatter_pass(
    native: &crate::query_exec::wgsl::NativeWgpuContext,
    encoder: &mut wgpu::CommandEncoder,
    profiler: &mut GpuPassProfiler,
    config_buffer: &wgpu::Buffer,
    owner_buffer: &wgpu::Buffer,
    query_output_buffer: &wgpu::Buffer,
    surface_buffer: &wgpu::Buffer,
    item_count: u32,
    workgroup_size: u32,
    gpu_runtime: &mut GpuRuntimeMetrics,
) -> Result<(), PresentationExecError> {
    let cached = super::create_custom_pass_pipeline(
        native,
        &surface_scatter_shader_source(workgroup_size)?,
        workgroup_size,
        &[
            storage_entry(0, true),
            storage_entry(1, true),
            storage_entry(2, true),
            storage_entry(3, false),
        ],
        "wrela.presentation.surface.scatter.pipeline",
        gpu_runtime,
    )?;
    let bind_group = create_bind_group(
        native,
        "wrela.presentation.surface.scatter.bind_group",
        &cached.bind_group_layout,
        &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: config_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: owner_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: query_output_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: surface_buffer.as_entire_binding(),
            },
        ],
        gpu_runtime,
    );
    dispatch_custom_pipeline(
        encoder,
        profiler,
        &cached.pipeline,
        &bind_group,
        item_count,
        workgroup_size,
        "wrela.presentation.surface.scatter.compute",
    );
    Ok(())
}

fn encode_participant_owner_pass(
    native: &crate::query_exec::wgsl::NativeWgpuContext,
    encoder: &mut wgpu::CommandEncoder,
    profiler: &mut GpuPassProfiler,
    config_buffer: &wgpu::Buffer,
    primary_hit_buffer: &wgpu::Buffer,
    owner_buffer: &wgpu::Buffer,
    item_count: u32,
    workgroup_size: u32,
    gpu_runtime: &mut GpuRuntimeMetrics,
) -> Result<(), PresentationExecError> {
    let cached = super::create_custom_pass_pipeline(
        native,
        &participant_owner_shader_source(workgroup_size)?,
        workgroup_size,
        &[
            storage_entry(0, true),
            storage_entry(1, true),
            storage_entry(2, false),
        ],
        "wrela.presentation.participant.owner.pipeline",
        gpu_runtime,
    )?;
    let bind_group = create_bind_group(
        native,
        "wrela.presentation.participant.owner.bind_group",
        &cached.bind_group_layout,
        &[
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
                resource: owner_buffer.as_entire_binding(),
            },
        ],
        gpu_runtime,
    );
    dispatch_custom_pipeline(
        encoder,
        profiler,
        &cached.pipeline,
        &bind_group,
        item_count,
        workgroup_size,
        "wrela.presentation.participant.owner.compute",
    );
    Ok(())
}

fn encode_radiance_item_build_pass(
    native: &crate::query_exec::wgsl::NativeWgpuContext,
    encoder: &mut wgpu::CommandEncoder,
    profiler: &mut GpuPassProfiler,
    config_buffer: &wgpu::Buffer,
    primary_hit_buffer: &wgpu::Buffer,
    owner_buffer: &wgpu::Buffer,
    input_buffer: &wgpu::Buffer,
    item_count: u32,
    workgroup_size: u32,
    gpu_runtime: &mut GpuRuntimeMetrics,
) -> Result<(), PresentationExecError> {
    let cached = super::create_custom_pass_pipeline(
        native,
        &participant_build_shader_source(workgroup_size, ParticipantLaneKind::Radiance)?,
        workgroup_size,
        &[
            storage_entry(0, true),
            storage_entry(1, true),
            storage_entry(2, true),
            storage_entry(3, false),
        ],
        "wrela.presentation.radiance.build.pipeline",
        gpu_runtime,
    )?;
    let bind_group = create_bind_group(
        native,
        "wrela.presentation.radiance.build.bind_group",
        &cached.bind_group_layout,
        &[
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
                resource: owner_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: input_buffer.as_entire_binding(),
            },
        ],
        gpu_runtime,
    );
    dispatch_custom_pipeline(
        encoder,
        profiler,
        &cached.pipeline,
        &bind_group,
        item_count,
        workgroup_size,
        "wrela.presentation.radiance.build.compute",
    );
    Ok(())
}

fn encode_medium_item_build_pass(
    native: &crate::query_exec::wgsl::NativeWgpuContext,
    encoder: &mut wgpu::CommandEncoder,
    profiler: &mut GpuPassProfiler,
    config_buffer: &wgpu::Buffer,
    primary_hit_buffer: &wgpu::Buffer,
    owner_buffer: &wgpu::Buffer,
    input_buffer: &wgpu::Buffer,
    item_count: u32,
    workgroup_size: u32,
    gpu_runtime: &mut GpuRuntimeMetrics,
) -> Result<(), PresentationExecError> {
    let cached = super::create_custom_pass_pipeline(
        native,
        &participant_build_shader_source(workgroup_size, ParticipantLaneKind::Medium)?,
        workgroup_size,
        &[
            storage_entry(0, true),
            storage_entry(1, true),
            storage_entry(2, true),
            storage_entry(3, false),
        ],
        "wrela.presentation.medium.build.pipeline",
        gpu_runtime,
    )?;
    let bind_group = create_bind_group(
        native,
        "wrela.presentation.medium.build.bind_group",
        &cached.bind_group_layout,
        &[
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
                resource: owner_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: input_buffer.as_entire_binding(),
            },
        ],
        gpu_runtime,
    );
    dispatch_custom_pipeline(
        encoder,
        profiler,
        &cached.pipeline,
        &bind_group,
        item_count,
        workgroup_size,
        "wrela.presentation.medium.build.compute",
    );
    Ok(())
}

fn encode_radiance_scatter_pass(
    native: &crate::query_exec::wgsl::NativeWgpuContext,
    encoder: &mut wgpu::CommandEncoder,
    profiler: &mut GpuPassProfiler,
    config_buffer: &wgpu::Buffer,
    owner_buffer: &wgpu::Buffer,
    query_output_buffer: &wgpu::Buffer,
    output_attachment: &wgpu::Buffer,
    item_count: u32,
    workgroup_size: u32,
    gpu_runtime: &mut GpuRuntimeMetrics,
) -> Result<(), PresentationExecError> {
    let cached = super::create_custom_pass_pipeline(
        native,
        &participant_scatter_shader_source(workgroup_size, ParticipantLaneKind::Radiance)?,
        workgroup_size,
        &[
            storage_entry(0, true),
            storage_entry(1, true),
            storage_entry(2, true),
            storage_entry(3, false),
        ],
        "wrela.presentation.radiance.scatter.pipeline",
        gpu_runtime,
    )?;
    let bind_group = create_bind_group(
        native,
        "wrela.presentation.radiance.scatter.bind_group",
        &cached.bind_group_layout,
        &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: config_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: owner_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: query_output_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: output_attachment.as_entire_binding(),
            },
        ],
        gpu_runtime,
    );
    dispatch_custom_pipeline(
        encoder,
        profiler,
        &cached.pipeline,
        &bind_group,
        item_count,
        workgroup_size,
        "wrela.presentation.radiance.scatter.compute",
    );
    Ok(())
}

fn encode_medium_scatter_pass(
    native: &crate::query_exec::wgsl::NativeWgpuContext,
    encoder: &mut wgpu::CommandEncoder,
    profiler: &mut GpuPassProfiler,
    config_buffer: &wgpu::Buffer,
    owner_buffer: &wgpu::Buffer,
    query_output_buffer: &wgpu::Buffer,
    output_attachment: &wgpu::Buffer,
    item_count: u32,
    workgroup_size: u32,
    gpu_runtime: &mut GpuRuntimeMetrics,
) -> Result<(), PresentationExecError> {
    let cached = super::create_custom_pass_pipeline(
        native,
        &participant_scatter_shader_source(workgroup_size, ParticipantLaneKind::Medium)?,
        workgroup_size,
        &[
            storage_entry(0, true),
            storage_entry(1, true),
            storage_entry(2, true),
            storage_entry(3, false),
        ],
        "wrela.presentation.medium.scatter.pipeline",
        gpu_runtime,
    )?;
    let bind_group = create_bind_group(
        native,
        "wrela.presentation.medium.scatter.bind_group",
        &cached.bind_group_layout,
        &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: config_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: owner_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: query_output_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: output_attachment.as_entire_binding(),
            },
        ],
        gpu_runtime,
    );
    dispatch_custom_pipeline(
        encoder,
        profiler,
        &cached.pipeline,
        &bind_group,
        item_count,
        workgroup_size,
        "wrela.presentation.medium.scatter.compute",
    );
    Ok(())
}

fn create_bind_group(
    native: &crate::query_exec::wgsl::NativeWgpuContext,
    label: &str,
    layout: &wgpu::BindGroupLayout,
    entries: &[wgpu::BindGroupEntry<'_>],
    gpu_runtime: &mut GpuRuntimeMetrics,
) -> wgpu::BindGroup {
    gpu_runtime.transient_bind_group_creations =
        gpu_runtime.transient_bind_group_creations.saturating_add(1);
    native.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(label),
        layout,
        entries,
    })
}

fn dispatch_custom_pipeline(
    encoder: &mut wgpu::CommandEncoder,
    profiler: &mut GpuPassProfiler,
    pipeline: &wgpu::ComputePipeline,
    bind_group: &wgpu::BindGroup,
    item_count: u32,
    workgroup_size: u32,
    label: &str,
) {
    let timestamp_writes = profiler.compute_pass_timestamp_writes();
    let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
        label: Some(label),
        timestamp_writes,
    });
    pass.set_pipeline(pipeline);
    pass.set_bind_group(super::GPU_RUNTIME_PASS_BIND_GROUP_INDEX, bind_group, &[]);
    pass.dispatch_workgroups(item_count.div_ceil(workgroup_size.max(1)).max(1), 1, 1);
}

fn storage_entry(binding: u32, read_only: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn surface_owner_shader_source(workgroup_size: u32) -> Result<String, PresentationExecError> {
    let structs = super::emit_wgsl_structs(&[
        surface_resolve_gpu_config_abi(),
        portable_builtin_record_abi("Hit3").expect("Hit3 abi"),
    ])?;
    Ok(format!(
        "{structs}

override WG_SIZE: u32 = {workgroup_size}u;

struct HitBuffer {{ values: array<Abi_Hit3>, }}
struct OwnerBuffer {{ values: array<atomic<u32>>, }}

@group({}) @binding(0) var<storage, read> config: Abi_SurfaceResolveGpuConfig;
@group({}) @binding(1) var<storage, read> hits: HitBuffer;
@group({}) @binding(2) var<storage, read_write> owners: OwnerBuffer;

fn scaled_index(index: u32) -> u32 {{
  let x = index % max(config.source_width, 1u);
  let y = index / max(config.source_width, 1u);
  let tx = min(x / max(config.divisor_x, 1u), max(config.target_width, 1u) - 1u);
  let ty = min(y / max(config.divisor_y, 1u), max(config.target_height, 1u) - 1u);
  return ty * max(config.target_width, 1u) + tx;
}}

@compute @workgroup_size(WG_SIZE)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {{
  let index = global_id.x;
  if (index >= max(config.source_width, 1u) * max(config.source_height, 1u)) {{ return; }}
  if (hits.values[index].hit == 0u) {{ return; }}
  let target_index = scaled_index(index);
  _ = atomicMax(&owners.values[target_index], 0xffffffffu - index);
}}",
        super::GPU_RUNTIME_PASS_BIND_GROUP_INDEX,
        super::GPU_RUNTIME_PASS_BIND_GROUP_INDEX,
        super::GPU_RUNTIME_PASS_BIND_GROUP_INDEX
    ))
}

fn surface_build_shader_source(workgroup_size: u32) -> Result<String, PresentationExecError> {
    let structs = super::emit_wgsl_structs(&[
        surface_resolve_gpu_config_abi(),
        portable_builtin_record_abi("Hit3").expect("Hit3 abi"),
    ])?;
    Ok(format!(
        "{structs}

override WG_SIZE: u32 = {workgroup_size}u;

struct HitBuffer {{ values: array<Abi_Hit3>, }}
struct OwnerBuffer {{ values: array<atomic<u32>>, }}
struct InputBuffer {{ values: array<Abi_Hit3>, }}

@group({}) @binding(0) var<storage, read> config: Abi_SurfaceResolveGpuConfig;
@group({}) @binding(1) var<storage, read> hits: HitBuffer;
@group({}) @binding(2) var<storage, read> owners: OwnerBuffer;
@group({}) @binding(3) var<storage, read_write> inputs: InputBuffer;

@compute @workgroup_size(WG_SIZE)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {{
  let index = global_id.x;
  if (index >= max(config.target_width, 1u) * max(config.target_height, 1u)) {{ return; }}
  let owner = atomicLoad(&owners.values[index]);
  let has_owner = owner != 0u;
  let source = select(0u, 0xffffffffu - owner, has_owner);
  inputs.values[index] = hits.values[source];
}}",
        super::GPU_RUNTIME_PASS_BIND_GROUP_INDEX,
        super::GPU_RUNTIME_PASS_BIND_GROUP_INDEX,
        super::GPU_RUNTIME_PASS_BIND_GROUP_INDEX,
        super::GPU_RUNTIME_PASS_BIND_GROUP_INDEX
    ))
}

fn surface_scatter_shader_source(workgroup_size: u32) -> Result<String, PresentationExecError> {
    let structs = super::emit_wgsl_structs(&[
        surface_resolve_gpu_config_abi(),
        portable_builtin_record_abi("Surface").expect("Surface abi"),
    ])?;
    Ok(format!(
        "{structs}

override WG_SIZE: u32 = {workgroup_size}u;

struct OwnerBuffer {{ values: array<atomic<u32>>, }}
struct SurfaceBuffer {{ values: array<Abi_Surface>, }}

@group({}) @binding(0) var<storage, read> config: Abi_SurfaceResolveGpuConfig;
@group({}) @binding(1) var<storage, read> owners: OwnerBuffer;
@group({}) @binding(2) var<storage, read> query_values: SurfaceBuffer;
@group({}) @binding(3) var<storage, read_write> outputs: SurfaceBuffer;

@compute @workgroup_size(WG_SIZE)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {{
  let index = global_id.x;
  if (index >= max(config.target_width, 1u) * max(config.target_height, 1u)) {{ return; }}
  let owner = atomicLoad(&owners.values[index]);
  if (owner == 0u) {{ return; }}
  outputs.values[index] = query_values.values[index];
}}",
        super::GPU_RUNTIME_PASS_BIND_GROUP_INDEX,
        super::GPU_RUNTIME_PASS_BIND_GROUP_INDEX,
        super::GPU_RUNTIME_PASS_BIND_GROUP_INDEX,
        super::GPU_RUNTIME_PASS_BIND_GROUP_INDEX
    ))
}

fn participant_owner_shader_source(workgroup_size: u32) -> Result<String, PresentationExecError> {
    let structs = super::emit_wgsl_structs(&[
        participant_resolve_gpu_config_abi(),
        portable_builtin_record_abi("Hit3").expect("Hit3 abi"),
    ])?;
    Ok(format!(
        "{structs}

override WG_SIZE: u32 = {workgroup_size}u;

struct HitBuffer {{ values: array<Abi_Hit3>, }}
struct OwnerBuffer {{ values: array<atomic<u32>>, }}

@group({}) @binding(0) var<storage, read> config: Abi_ParticipantResolveGpuConfig;
@group({}) @binding(1) var<storage, read> hits: HitBuffer;
@group({}) @binding(2) var<storage, read_write> owners: OwnerBuffer;

fn scaled_index(index: u32) -> u32 {{
  let x = index % max(config.source_width, 1u);
  let y = index / max(config.source_width, 1u);
  let tx = min(x / max(config.divisor_x, 1u), max(config.target_width, 1u) - 1u);
  let ty = min(y / max(config.divisor_y, 1u), max(config.target_height, 1u) - 1u);
  return ty * max(config.target_width, 1u) + tx;
}}

@compute @workgroup_size(WG_SIZE)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {{
  let index = global_id.x;
  if (index >= max(config.source_width, 1u) * max(config.source_height, 1u)) {{ return; }}
  if (config.include_misses == 0u && hits.values[index].hit == 0u) {{ return; }}
  let target_index = scaled_index(index);
  _ = atomicMax(&owners.values[target_index], 0xffffffffu - index);
}}",
        super::GPU_RUNTIME_PASS_BIND_GROUP_INDEX,
        super::GPU_RUNTIME_PASS_BIND_GROUP_INDEX,
        super::GPU_RUNTIME_PASS_BIND_GROUP_INDEX
    ))
}

fn participant_build_shader_source(
    workgroup_size: u32,
    lane: ParticipantLaneKind,
) -> Result<String, PresentationExecError> {
    let item_abi = match lane {
        ParticipantLaneKind::Radiance => {
            portable_builtin_record_abi("PointDirectionQuery").expect("PointDirectionQuery abi")
        }
        ParticipantLaneKind::Medium => {
            portable_builtin_record_abi("PointQuery").expect("PointQuery abi")
        }
    };
    let structs = super::emit_wgsl_structs(&[
        participant_resolve_gpu_config_abi(),
        portable_builtin_record_abi("Hit3").expect("Hit3 abi"),
        item_abi.clone(),
    ])?;
    let output_struct = match lane {
        ParticipantLaneKind::Radiance => "Abi_PointDirectionQuery",
        ParticipantLaneKind::Medium => "Abi_PointQuery",
    };
    let output_value = match lane {
        ParticipantLaneKind::Radiance => "Abi_PointDirectionQuery(point, ray_direction)",
        ParticipantLaneKind::Medium => "Abi_PointQuery(point)",
    };
    Ok(format!(
        "{structs}

override WG_SIZE: u32 = {workgroup_size}u;

struct HitBuffer {{ values: array<Abi_Hit3>, }}
struct OwnerBuffer {{ values: array<atomic<u32>>, }}
struct OutputBuffer {{ values: array<{output_struct}>, }}

@group({}) @binding(0) var<storage, read> config: Abi_ParticipantResolveGpuConfig;
@group({}) @binding(1) var<storage, read> hits: HitBuffer;
@group({}) @binding(2) var<storage, read> owners: OwnerBuffer;
@group({}) @binding(3) var<storage, read_write> outputs: OutputBuffer;

fn wr_normalize_or(value: vec3<f32>, fallback: vec3<f32>) -> vec3<f32> {{
  let len_sq = dot(value, value);
  if (len_sq <= 0.0000001) {{ return fallback; }}
  return normalize(value);
}}

fn ray_direction(index: u32) -> vec3<f32> {{
  let width = max(config.viewport_width, 1u);
  let height = max(config.viewport_height, 1u);
  let x = index % width;
  let y = index / width;
  let uv = vec2<f32>(
    (f32(x) + 0.5 + config.jitter.x) / f32(width),
    (f32(y) + 0.5 + config.jitter.y) / f32(height)
  );
  let forward = wr_normalize_or(config.forward, vec3<f32>(0.0, 0.0, -1.0));
  if (config.legacy_active != 0u) {{
    let right = wr_normalize_or(cross(forward, config.legacy_world_up), vec3<f32>(1.0, 0.0, 0.0));
    let up = wr_normalize_or(cross(right, forward), vec3<f32>(0.0, 1.0, 0.0));
    let aspect = f32(width) / f32(height);
    let screen_x = (uv.x * 2.0 - 1.0) * aspect * config.legacy_view_scale;
    let screen_y = (1.0 - uv.y * 2.0) * config.legacy_view_scale;
    return wr_normalize_or(forward + right * screen_x + up * screen_y, forward);
  }}
  let right = wr_normalize_or(cross(forward, config.up), vec3<f32>(1.0, 0.0, 0.0));
  let up = wr_normalize_or(cross(right, forward), vec3<f32>(0.0, 1.0, 0.0));
  let aspect = f32(width) / f32(height);
  let tan_half_fov = tan(radians(config.vertical_fov_degrees) * 0.5);
  let ndc_x = uv.x * 2.0 - 1.0;
  let ndc_y = 1.0 - uv.y * 2.0;
  return wr_normalize_or(
    forward + right * (ndc_x * aspect * tan_half_fov) + up * (ndc_y * tan_half_fov),
    forward
  );
}}

@compute @workgroup_size(WG_SIZE)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {{
  let index = global_id.x;
  if (index >= max(config.target_width, 1u) * max(config.target_height, 1u)) {{ return; }}
  let owner = atomicLoad(&owners.values[index]);
  let has_owner = owner != 0u;
  let source = select(0u, 0xffffffffu - owner, has_owner);
  let ray_direction = ray_direction(source);
  let hit = hits.values[source];
  var point = hit.position;
  if (hit.hit == 0u) {{
    point = config.camera_position + ray_direction * config.miss_sample_distance;
  }}
  outputs.values[index] = {output_value};
}}",
        super::GPU_RUNTIME_PASS_BIND_GROUP_INDEX,
        super::GPU_RUNTIME_PASS_BIND_GROUP_INDEX,
        super::GPU_RUNTIME_PASS_BIND_GROUP_INDEX,
        super::GPU_RUNTIME_PASS_BIND_GROUP_INDEX
    ))
}

fn participant_identity_build_shader_source(
    workgroup_size: u32,
    lane: ParticipantLaneKind,
) -> Result<String, PresentationExecError> {
    let item_abi = match lane {
        ParticipantLaneKind::Radiance => {
            portable_builtin_record_abi("PointDirectionQuery").expect("PointDirectionQuery abi")
        }
        ParticipantLaneKind::Medium => {
            portable_builtin_record_abi("PointQuery").expect("PointQuery abi")
        }
    };
    let structs = super::emit_wgsl_structs(&[
        participant_resolve_gpu_config_abi(),
        portable_builtin_record_abi("Hit3").expect("Hit3 abi"),
        item_abi.clone(),
    ])?;
    let output_struct = match lane {
        ParticipantLaneKind::Radiance => "Abi_PointDirectionQuery",
        ParticipantLaneKind::Medium => "Abi_PointQuery",
    };
    let output_value = match lane {
        ParticipantLaneKind::Radiance => "Abi_PointDirectionQuery(point, ray_direction)",
        ParticipantLaneKind::Medium => "Abi_PointQuery(point)",
    };
    Ok(format!(
        "{structs}

override WG_SIZE: u32 = {workgroup_size}u;

struct HitBuffer {{ values: array<Abi_Hit3>, }}
struct OutputBuffer {{ values: array<{output_struct}>, }}

@group({}) @binding(0) var<storage, read> config: Abi_ParticipantResolveGpuConfig;
@group({}) @binding(1) var<storage, read> hits: HitBuffer;
@group({}) @binding(2) var<storage, read_write> outputs: OutputBuffer;

fn wr_normalize_or(value: vec3<f32>, fallback: vec3<f32>) -> vec3<f32> {{
  let len_sq = dot(value, value);
  if (len_sq <= 0.0000001) {{ return fallback; }}
  return normalize(value);
}}

fn ray_direction(index: u32) -> vec3<f32> {{
  let width = max(config.viewport_width, 1u);
  let height = max(config.viewport_height, 1u);
  let x = index % width;
  let y = index / width;
  let uv = vec2<f32>(
    (f32(x) + 0.5 + config.jitter.x) / f32(width),
    (f32(y) + 0.5 + config.jitter.y) / f32(height)
  );
  let forward = wr_normalize_or(config.forward, vec3<f32>(0.0, 0.0, -1.0));
  if (config.legacy_active != 0u) {{
    let right = wr_normalize_or(cross(forward, config.legacy_world_up), vec3<f32>(1.0, 0.0, 0.0));
    let up = wr_normalize_or(cross(right, forward), vec3<f32>(0.0, 1.0, 0.0));
    let aspect = f32(width) / f32(height);
    let screen_x = (uv.x * 2.0 - 1.0) * aspect * config.legacy_view_scale;
    let screen_y = (1.0 - uv.y * 2.0) * config.legacy_view_scale;
    return wr_normalize_or(forward + right * screen_x + up * screen_y, forward);
  }}
  let right = wr_normalize_or(cross(forward, config.up), vec3<f32>(1.0, 0.0, 0.0));
  let up = wr_normalize_or(cross(right, forward), vec3<f32>(0.0, 1.0, 0.0));
  let aspect = f32(width) / f32(height);
  let tan_half_fov = tan(radians(config.vertical_fov_degrees) * 0.5);
  let ndc_x = uv.x * 2.0 - 1.0;
  let ndc_y = 1.0 - uv.y * 2.0;
  return wr_normalize_or(
    forward + right * (ndc_x * aspect * tan_half_fov) + up * (ndc_y * tan_half_fov),
    forward
  );
}}

@compute @workgroup_size(WG_SIZE)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {{
  let index = global_id.x;
  if (index >= max(config.target_width, 1u) * max(config.target_height, 1u)) {{ return; }}
  let ray_direction = ray_direction(index);
  let hit = hits.values[index];
  var point = hit.position;
  if (hit.hit == 0u) {{
    point = config.camera_position + ray_direction * config.miss_sample_distance;
  }}
  outputs.values[index] = {output_value};
}}",
        super::GPU_RUNTIME_PASS_BIND_GROUP_INDEX,
        super::GPU_RUNTIME_PASS_BIND_GROUP_INDEX,
        super::GPU_RUNTIME_PASS_BIND_GROUP_INDEX
    ))
}

fn participant_scatter_shader_source(
    workgroup_size: u32,
    lane: ParticipantLaneKind,
) -> Result<String, PresentationExecError> {
    let value_abi = match lane {
        ParticipantLaneKind::Radiance => PortableAbiType::Vec3,
        ParticipantLaneKind::Medium => portable_builtin_record_abi("Medium").expect("Medium abi"),
    };
    let output_struct = match lane {
        ParticipantLaneKind::Radiance => "vec3<f32>",
        ParticipantLaneKind::Medium => "Abi_Medium",
    };
    let structs = super::emit_wgsl_structs(&[participant_resolve_gpu_config_abi(), value_abi])?;
    Ok(format!(
        "{structs}

override WG_SIZE: u32 = {workgroup_size}u;

struct OwnerBuffer {{ values: array<atomic<u32>>, }}
struct ValueBuffer {{ values: array<{output_struct}>, }}

@group({}) @binding(0) var<storage, read> config: Abi_ParticipantResolveGpuConfig;
@group({}) @binding(1) var<storage, read> owners: OwnerBuffer;
@group({}) @binding(2) var<storage, read> query_values: ValueBuffer;
@group({}) @binding(3) var<storage, read_write> outputs: ValueBuffer;

@compute @workgroup_size(WG_SIZE)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {{
  let index = global_id.x;
  if (index >= max(config.target_width, 1u) * max(config.target_height, 1u)) {{ return; }}
  let owner = atomicLoad(&owners.values[index]);
  if (owner == 0u) {{ return; }}
  outputs.values[index] = query_values.values[index];
}}",
        super::GPU_RUNTIME_PASS_BIND_GROUP_INDEX,
        super::GPU_RUNTIME_PASS_BIND_GROUP_INDEX,
        super::GPU_RUNTIME_PASS_BIND_GROUP_INDEX,
        super::GPU_RUNTIME_PASS_BIND_GROUP_INDEX
    ))
}
