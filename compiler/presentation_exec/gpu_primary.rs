use crate::gpu_runtime::GpuPassProfiler;
use crate::kernel::KernelValue;
use crate::kernel::lower_batch_query_plan;
use crate::portable::{
    PortableAbiType, PortableStructField, portable_abi_decode_slice,
    portable_abi_emit_wgsl_structs, portable_abi_layout, portable_builtin_record_abi,
};
use crate::presentation_contract::{
    CanonicalCameraInput, CanonicalRayBudget, CanonicalViewportInput,
    LegacyCompatibilityProjectionInput,
};
use crate::presentation_exec::PresentationExecError;
use crate::presentation_exec::gpu_resources::GpuAttachmentArena;
use crate::presentation_plan::PrimaryVisibilityPassContract;
use crate::query_contract::QueryContractId;
use crate::query_exec::QueryExecContext;
use crate::query_exec::gpu_dispatch::{GpuDispatchResult, GpuQueryBufferHandle};
use crate::query_exec::wgsl::{
    GpuDispatchRequest, ResidentBatchQuerySession, build_batch_request_without_items_for_shader,
    compile_batch_shader, compiled_pipeline, encode_slice, prepare_resident_batch_query,
};
use crate::query_plan::{BatchQueryPlan, DispatchBackend};
use smol_str::SmolStr;

const PRIMARY_HELPER_WORKGROUP_SIZE: u32 = 64;

#[derive(Clone)]
pub(crate) struct PrimaryVisibilityGpuDispatch {
    pub viewport: CanonicalViewportInput,
    pub primary_viewport: CanonicalViewportInput,
    request: GpuDispatchRequest,
    session: ResidentBatchQuerySession,
    input_bytes: Option<Vec<u8>>,
    side_channel_bytes: Option<Vec<u8>>,
    camera: CanonicalCameraInput,
    jitter_pixels: [f32; 2],
    ray_budget: CanonicalRayBudget,
    legacy_projection: bool,
    compatibility_projection: Option<LegacyCompatibilityProjectionInput>,
}

impl PrimaryVisibilityGpuDispatch {
    pub(crate) fn initial_gpu_runtime(&self) -> crate::gpu_runtime::GpuRuntimeMetrics {
        self.session.initial_gpu_runtime()
    }

    pub(crate) fn selected_workgroup_size(&self) -> u32 {
        self.session.selected_workgroup_size()
    }

    pub(crate) fn dispatch_result(&self) -> GpuDispatchResult {
        GpuDispatchResult {
            values: GpuQueryBufferHandle {
                buffer: self.session.output_buffer.clone(),
                size_bytes: self.session.output_buffer_size,
                abi: Some(self.session.result_abi.clone()),
            },
            metrics: Some(GpuQueryBufferHandle {
                buffer: self.session.observability_buffer.clone(),
                size_bytes: self.session.observability_buffer_size,
                abi: None,
            }),
            item_count: self.session.item_count,
        }
    }

    pub(crate) fn decode_observability(
        &self,
        bytes: &[u8],
        gpu_runtime: crate::gpu_runtime::GpuRuntimeMetrics,
    ) -> crate::query_exec::QueryExecutionObservability {
        self.session.decode_observability(bytes, gpu_runtime)
    }

    pub(crate) fn primary_hit_attachment_buffer<'a>(
        &self,
        arena: &'a GpuAttachmentArena,
        contract: &PrimaryVisibilityPassContract,
    ) -> Result<&'a wgpu::Buffer, PresentationExecError> {
        arena
            .attachment_buffer(contract.primary_hit_attachment.as_str())
            .ok_or_else(|| PresentationExecError::UnsupportedPlan {
                message: format!(
                    "missing GPU primary-hit attachment '{}'",
                    contract.primary_hit_attachment
                ),
            })
    }

    pub(crate) fn encode_passes(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        profiler: &mut GpuPassProfiler,
        arena: &GpuAttachmentArena,
        contract: &PrimaryVisibilityPassContract,
        gpu_runtime: &mut crate::gpu_runtime::GpuRuntimeMetrics,
    ) -> Result<u32, PresentationExecError> {
        gpu_runtime.upload_bytes = gpu_runtime.upload_bytes.saturating_add(
            self.session.initialize_dispatch_state_with_inputs(
                &self.request.dispatch,
                self.input_bytes.as_deref(),
                self.side_channel_bytes.as_deref(),
            )?,
        );
        let input_buffer = GpuQueryBufferHandle {
            buffer: self.session.input_buffer.clone(),
            size_bytes: self.session.input_buffer_size,
            abi: None,
        };

        let raygen = create_primary_raygen_resources(
            self.session.native.as_ref(),
            self.camera,
            self.primary_viewport,
            self.jitter_pixels,
            self.ray_budget,
            self.legacy_projection,
            self.compatibility_projection,
            &input_buffer.buffer,
            gpu_runtime,
        )?;
        encode_primary_raygen_pass(
            self.session.native.as_ref(),
            &raygen,
            encoder,
            profiler,
            gpu_runtime,
        )?;

        self.session.encode_compute_pass(encoder, profiler);
        let result = self.dispatch_result();

        let materialize = create_primary_materialize_resources(
            self.session.native.as_ref(),
            self.viewport,
            self.primary_viewport,
            &result.values.buffer,
            arena,
            contract,
            gpu_runtime,
        )?;
        encode_primary_materialize_passes(
            self.session.native.as_ref(),
            &materialize,
            encoder,
            profiler,
            gpu_runtime,
        )?;

        Ok(5)
    }
}

pub(crate) fn prepare_primary_visibility_dispatch(
    ctx: &QueryExecContext,
    contract_id: QueryContractId,
    capture: KernelValue,
    frame_domain: KernelValue,
    candidate_shape_names: Option<Vec<SmolStr>>,
    candidate_spans: Vec<u32>,
    camera: CanonicalCameraInput,
    primary_viewport: CanonicalViewportInput,
    viewport: CanonicalViewportInput,
    jitter_pixels: [f32; 2],
    ray_budget: CanonicalRayBudget,
    legacy_projection: bool,
    compatibility_projection: Option<LegacyCompatibilityProjectionInput>,
) -> Result<PrimaryVisibilityGpuDispatch, PresentationExecError> {
    let batch_plan = BatchQueryPlan::for_contract(contract_id, DispatchBackend::Wgsl, None)
        .map_err(|message| PresentationExecError::UnsupportedPlan {
            message: message.to_string(),
        })?;
    let lowered = lower_batch_query_plan(&batch_plan);
    let generated = compile_batch_shader(ctx, &lowered).map_err(PresentationExecError::Query)?;
    let mut request = build_batch_request_without_items_for_shader(
        ctx,
        &lowered,
        &[capture, frame_domain],
        primary_viewport
            .width
            .saturating_mul(primary_viewport.height),
    )
    .map_err(PresentationExecError::Query)?;
    request.candidate_spans = if let Some(candidate_shape_names) = candidate_shape_names.as_ref() {
        let candidate_shape_indices = candidate_shape_names
            .iter()
            .map(|shape| {
                ctx.scene
                    .shapes
                    .keys()
                    .enumerate()
                    .find_map(|(index, candidate)| (candidate == shape).then_some(index as u32))
                    .ok_or_else(|| PresentationExecError::UnsupportedPlan {
                        message: format!("missing scene shape index for tile candidate '{shape}'"),
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        pack_candidate_span_scene_indices(candidate_spans, candidate_shape_indices)
    } else {
        candidate_spans
    };
    let side_channel_bytes = finalize_candidate_span_side_channel(&mut request)?;
    let session =
        prepare_resident_batch_query(&generated, &request).map_err(PresentationExecError::Query)?;
    Ok(PrimaryVisibilityGpuDispatch {
        viewport,
        primary_viewport,
        request,
        session,
        input_bytes: None,
        side_channel_bytes,
        camera,
        jitter_pixels,
        ray_budget,
        legacy_projection,
        compatibility_projection,
    })
}

fn finalize_candidate_span_side_channel(
    request: &mut GpuDispatchRequest,
) -> Result<Option<Vec<u8>>, PresentationExecError> {
    if let KernelValue::Struct(dispatch) = &mut request.dispatch
        && let Some((_, value)) = dispatch
            .fields
            .iter_mut()
            .find(|(name, _)| name == "candidate_spans_enabled")
    {
        *value = KernelValue::Bool(!request.candidate_spans.is_empty());
    }
    if request.candidate_spans.is_empty() {
        return Ok(None);
    }
    let values = request
        .candidate_spans
        .iter()
        .copied()
        .map(KernelValue::U32)
        .collect::<Vec<_>>();
    Ok(Some(
        encode_slice(&PortableAbiType::U32, &values).map_err(PresentationExecError::Query)?,
    ))
}

fn pack_candidate_span_scene_indices(
    candidate_spans: Vec<u32>,
    candidate_shape_indices: Vec<u32>,
) -> Vec<u32> {
    if candidate_spans.is_empty() {
        return Vec::new();
    }
    let mut packed = candidate_spans;
    packed.extend(candidate_shape_indices);
    packed
}

pub(crate) fn decode_primary_hit_attachment_bytes(
    arena: &GpuAttachmentArena,
    contract: &PrimaryVisibilityPassContract,
    bytes: &[u8],
) -> Result<Vec<KernelValue>, PresentationExecError> {
    let layout = arena
        .attachment(contract.primary_hit_attachment.as_str())
        .ok_or_else(|| PresentationExecError::UnsupportedPlan {
            message: format!(
                "missing primary-hit attachment '{}'",
                contract.primary_hit_attachment
            ),
        })?
        .layout
        .clone();
    portable_abi_decode_slice(
        &layout.element_abi,
        bytes,
        (layout.width.saturating_mul(layout.height)) as usize,
    )
    .map_err(|err| PresentationExecError::UnsupportedPlan {
        message: format!("failed to decode GPU primary-hit attachment bytes: {err}"),
    })
}

#[derive(Clone)]
struct PrimaryRaygenResources {
    item_count: u32,
    dispatch_buffer: wgpu::Buffer,
    camera_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    pipeline: wgpu::ComputePipeline,
}

#[derive(Clone)]
struct PrimaryMaterializeResources {
    item_count: u32,
    dispatch_buffer: wgpu::Buffer,
    primary_hit_bind_group: wgpu::BindGroup,
    depth_bind_group: Option<wgpu::BindGroup>,
    normal_bind_group: Option<wgpu::BindGroup>,
    primary_hit_pipeline: wgpu::ComputePipeline,
    depth_pipeline: Option<wgpu::ComputePipeline>,
    normal_pipeline: Option<wgpu::ComputePipeline>,
}

fn create_primary_raygen_resources(
    native: &crate::query_exec::wgsl::NativeWgpuContext,
    camera: CanonicalCameraInput,
    viewport: CanonicalViewportInput,
    jitter_pixels: [f32; 2],
    ray_budget: CanonicalRayBudget,
    legacy_projection: bool,
    compatibility_projection: Option<LegacyCompatibilityProjectionInput>,
    output_buffer: &wgpu::Buffer,
    gpu_runtime: &mut crate::gpu_runtime::GpuRuntimeMetrics,
) -> Result<PrimaryRaygenResources, PresentationExecError> {
    let dispatch_bytes = crate::query_exec::wgsl::encode_value(
        &raygen_dispatch_abi(),
        &raygen_dispatch_value(viewport),
    )
    .map_err(PresentationExecError::Query)?;
    let camera_bytes = crate::query_exec::wgsl::encode_value(
        &raygen_camera_abi(),
        &raygen_camera_value(
            camera,
            viewport,
            jitter_pixels,
            ray_budget,
            legacy_projection,
            compatibility_projection,
        ),
    )
    .map_err(PresentationExecError::Query)?;
    let dispatch_buffer = storage_buffer_with_bytes(
        native,
        "wrela.presentation.primary.dispatch",
        &dispatch_bytes,
        gpu_runtime,
    );
    let camera_buffer = storage_buffer_with_bytes(
        native,
        "wrela.presentation.primary.camera",
        &camera_bytes,
        gpu_runtime,
    );
    let cached = compiled_pipeline(
        native,
        &primary_raygen_shader_source()?,
        PRIMARY_HELPER_WORKGROUP_SIZE,
        crate::gpu_runtime::GPU_RUNTIME_PASS_BIND_GROUP_INDEX,
        wgpu::BufferSize::new(portable_abi_layout(&raygen_dispatch_abi()).size as u64),
        gpu_runtime,
    )
    .map_err(PresentationExecError::Query)?;
    gpu_runtime.transient_bind_group_creations =
        gpu_runtime.transient_bind_group_creations.saturating_add(1);
    let bind_group = native.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("wrela.presentation.primary.raygen.bind_group"),
        layout: &cached.bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: dispatch_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: camera_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: output_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: dispatch_buffer.as_entire_binding(),
            },
        ],
    });
    Ok(PrimaryRaygenResources {
        item_count: viewport.width.saturating_mul(viewport.height),
        dispatch_buffer,
        camera_buffer,
        bind_group,
        pipeline: cached.pipeline,
    })
}

fn encode_primary_raygen_pass(
    native: &crate::query_exec::wgsl::NativeWgpuContext,
    resources: &PrimaryRaygenResources,
    encoder: &mut wgpu::CommandEncoder,
    profiler: &mut GpuPassProfiler,
    _gpu_runtime: &mut crate::gpu_runtime::GpuRuntimeMetrics,
) -> Result<(), PresentationExecError> {
    let _keep_alive = (&resources.dispatch_buffer, &resources.camera_buffer);
    let timestamp_writes = profiler.compute_pass_timestamp_writes();
    let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
        label: Some("wrela.presentation.primary.raygen"),
        timestamp_writes,
    });
    pass.set_pipeline(&resources.pipeline);
    pass.set_bind_group(
        crate::gpu_runtime::GPU_RUNTIME_PASS_BIND_GROUP_INDEX,
        &resources.bind_group,
        &[],
    );
    let _ = native;
    pass.dispatch_workgroups(
        resources
            .item_count
            .div_ceil(PRIMARY_HELPER_WORKGROUP_SIZE.max(1))
            .max(1),
        1,
        1,
    );
    Ok(())
}

fn create_primary_materialize_resources(
    native: &crate::query_exec::wgsl::NativeWgpuContext,
    viewport: CanonicalViewportInput,
    primary_viewport: CanonicalViewportInput,
    input_hits: &wgpu::Buffer,
    arena: &GpuAttachmentArena,
    contract: &PrimaryVisibilityPassContract,
    gpu_runtime: &mut crate::gpu_runtime::GpuRuntimeMetrics,
) -> Result<PrimaryMaterializeResources, PresentationExecError> {
    let dispatch_bytes = crate::query_exec::wgsl::encode_value(
        &materialize_dispatch_abi(),
        &materialize_dispatch_value(viewport, primary_viewport),
    )
    .map_err(PresentationExecError::Query)?;
    let dispatch_buffer = storage_buffer_with_bytes(
        native,
        "wrela.presentation.primary.materialize_dispatch",
        &dispatch_bytes,
        gpu_runtime,
    );
    let primary_hit_buffer = arena
        .attachment_buffer(contract.primary_hit_attachment.as_str())
        .ok_or_else(|| PresentationExecError::UnsupportedPlan {
            message: format!(
                "missing GPU primary-hit attachment '{}'",
                contract.primary_hit_attachment
            ),
        })?;
    let primary_hit_pipeline = compiled_pipeline(
        native,
        &primary_hit_materialize_shader_source()?,
        PRIMARY_HELPER_WORKGROUP_SIZE,
        crate::gpu_runtime::GPU_RUNTIME_PASS_BIND_GROUP_INDEX,
        wgpu::BufferSize::new(portable_abi_layout(&materialize_dispatch_abi()).size as u64),
        gpu_runtime,
    )
    .map_err(PresentationExecError::Query)?;
    let primary_hit_bind_group = create_materialize_bind_group(
        native,
        &primary_hit_pipeline.bind_group_layout,
        &dispatch_buffer,
        input_hits,
        primary_hit_buffer,
        gpu_runtime,
        "primary_hit",
    );

    let (depth_pipeline, depth_bind_group) =
        if let Some(depth_attachment) = &contract.depth_attachment {
            let depth_buffer = arena
                .attachment_buffer(depth_attachment.as_str())
                .ok_or_else(|| PresentationExecError::UnsupportedPlan {
                    message: format!("missing GPU depth attachment '{depth_attachment}'"),
                })?;
            let cached = compiled_pipeline(
                native,
                &depth_materialize_shader_source()?,
                PRIMARY_HELPER_WORKGROUP_SIZE,
                crate::gpu_runtime::GPU_RUNTIME_PASS_BIND_GROUP_INDEX,
                wgpu::BufferSize::new(portable_abi_layout(&materialize_dispatch_abi()).size as u64),
                gpu_runtime,
            )
            .map_err(PresentationExecError::Query)?;
            let bind_group = create_materialize_bind_group(
                native,
                &cached.bind_group_layout,
                &dispatch_buffer,
                input_hits,
                depth_buffer,
                gpu_runtime,
                "depth",
            );
            (Some(cached.pipeline), Some(bind_group))
        } else {
            (None, None)
        };

    let (normal_pipeline, normal_bind_group) =
        if let Some(normal_attachment) = &contract.world_normal_attachment {
            let normal_buffer = arena
                .attachment_buffer(normal_attachment.as_str())
                .ok_or_else(|| PresentationExecError::UnsupportedPlan {
                    message: format!("missing GPU world-normal attachment '{normal_attachment}'"),
                })?;
            let cached = compiled_pipeline(
                native,
                &normal_materialize_shader_source()?,
                PRIMARY_HELPER_WORKGROUP_SIZE,
                crate::gpu_runtime::GPU_RUNTIME_PASS_BIND_GROUP_INDEX,
                wgpu::BufferSize::new(portable_abi_layout(&materialize_dispatch_abi()).size as u64),
                gpu_runtime,
            )
            .map_err(PresentationExecError::Query)?;
            let bind_group = create_materialize_bind_group(
                native,
                &cached.bind_group_layout,
                &dispatch_buffer,
                input_hits,
                normal_buffer,
                gpu_runtime,
                "world_normal",
            );
            (Some(cached.pipeline), Some(bind_group))
        } else {
            (None, None)
        };

    Ok(PrimaryMaterializeResources {
        item_count: viewport.width.saturating_mul(viewport.height),
        dispatch_buffer,
        primary_hit_bind_group,
        depth_bind_group,
        normal_bind_group,
        primary_hit_pipeline: primary_hit_pipeline.pipeline,
        depth_pipeline,
        normal_pipeline,
    })
}

fn encode_primary_materialize_passes(
    _native: &crate::query_exec::wgsl::NativeWgpuContext,
    resources: &PrimaryMaterializeResources,
    encoder: &mut wgpu::CommandEncoder,
    profiler: &mut GpuPassProfiler,
    _gpu_runtime: &mut crate::gpu_runtime::GpuRuntimeMetrics,
) -> Result<(), PresentationExecError> {
    let _keep_alive = &resources.dispatch_buffer;
    encode_materialize_pass(
        encoder,
        profiler,
        "wrela.presentation.primary.writeout.primary_hit",
        &resources.primary_hit_pipeline,
        &resources.primary_hit_bind_group,
        resources.item_count,
    );
    if let (Some(pipeline), Some(bind_group)) = (
        resources.depth_pipeline.as_ref(),
        resources.depth_bind_group.as_ref(),
    ) {
        encode_materialize_pass(
            encoder,
            profiler,
            "wrela.presentation.primary.writeout.depth",
            pipeline,
            bind_group,
            resources.item_count,
        );
    }
    if let (Some(pipeline), Some(bind_group)) = (
        resources.normal_pipeline.as_ref(),
        resources.normal_bind_group.as_ref(),
    ) {
        encode_materialize_pass(
            encoder,
            profiler,
            "wrela.presentation.primary.writeout.world_normal",
            pipeline,
            bind_group,
            resources.item_count,
        );
    }
    Ok(())
}

fn encode_materialize_pass(
    encoder: &mut wgpu::CommandEncoder,
    profiler: &mut GpuPassProfiler,
    label: &str,
    pipeline: &wgpu::ComputePipeline,
    bind_group: &wgpu::BindGroup,
    item_count: u32,
) {
    let timestamp_writes = profiler.compute_pass_timestamp_writes();
    let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
        label: Some(label),
        timestamp_writes,
    });
    pass.set_pipeline(pipeline);
    pass.set_bind_group(
        crate::gpu_runtime::GPU_RUNTIME_PASS_BIND_GROUP_INDEX,
        bind_group,
        &[],
    );
    pass.dispatch_workgroups(
        item_count
            .div_ceil(PRIMARY_HELPER_WORKGROUP_SIZE.max(1))
            .max(1),
        1,
        1,
    );
}

fn create_materialize_bind_group(
    native: &crate::query_exec::wgsl::NativeWgpuContext,
    layout: &wgpu::BindGroupLayout,
    dispatch_buffer: &wgpu::Buffer,
    input_hits: &wgpu::Buffer,
    output_buffer: &wgpu::Buffer,
    gpu_runtime: &mut crate::gpu_runtime::GpuRuntimeMetrics,
    label: &str,
) -> wgpu::BindGroup {
    gpu_runtime.transient_bind_group_creations =
        gpu_runtime.transient_bind_group_creations.saturating_add(1);
    native.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(&format!("wrela.presentation.primary.materialize.{label}")),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: dispatch_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: input_hits.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: output_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: dispatch_buffer.as_entire_binding(),
            },
        ],
    })
}

fn storage_buffer_with_bytes(
    native: &crate::query_exec::wgsl::NativeWgpuContext,
    label: &str,
    bytes: &[u8],
    gpu_runtime: &mut crate::gpu_runtime::GpuRuntimeMetrics,
) -> wgpu::Buffer {
    let buffer = native.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: bytes.len().max(4) as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
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

fn raygen_dispatch_abi() -> PortableAbiType {
    PortableAbiType::Struct {
        name: SmolStr::new("PrimaryRaygenDispatch"),
        class_id: 0,
        fields: vec![
            PortableStructField {
                name: SmolStr::new("item_count"),
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
        ],
    }
}

fn raygen_camera_abi() -> PortableAbiType {
    PortableAbiType::Struct {
        name: SmolStr::new("PrimaryRaygenCamera"),
        class_id: 0,
        fields: vec![
            PortableStructField {
                name: SmolStr::new("position"),
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
                name: SmolStr::new("max_distance"),
                ty: PortableAbiType::F32,
            },
            PortableStructField {
                name: SmolStr::new("min_step"),
                ty: PortableAbiType::F32,
            },
            PortableStructField {
                name: SmolStr::new("hit_epsilon"),
                ty: PortableAbiType::F32,
            },
            PortableStructField {
                name: SmolStr::new("max_steps"),
                ty: PortableAbiType::I32,
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
        ],
    }
}

fn materialize_dispatch_abi() -> PortableAbiType {
    PortableAbiType::Struct {
        name: SmolStr::new("PrimaryMaterializeDispatch"),
        class_id: 0,
        fields: vec![
            PortableStructField {
                name: SmolStr::new("item_count"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("output_width"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("output_height"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("internal_width"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("internal_height"),
                ty: PortableAbiType::U32,
            },
        ],
    }
}

fn raygen_camera_value(
    camera: CanonicalCameraInput,
    viewport: CanonicalViewportInput,
    jitter_pixels: [f32; 2],
    ray_budget: CanonicalRayBudget,
    legacy_projection: bool,
    compatibility_projection: Option<LegacyCompatibilityProjectionInput>,
) -> KernelValue {
    let _ = viewport;
    let compatibility = compatibility_projection.unwrap_or(LegacyCompatibilityProjectionInput {
        world_up: camera.up,
        view_scale: 0.72,
    });
    KernelValue::Struct(crate::kernel::KernelStructValue {
        name: SmolStr::new("PrimaryRaygenCamera"),
        fields: vec![
            (SmolStr::new("position"), KernelValue::Vec3(camera.position)),
            (SmolStr::new("forward"), KernelValue::Vec3(camera.forward)),
            (SmolStr::new("up"), KernelValue::Vec3(camera.up)),
            (
                SmolStr::new("vertical_fov_degrees"),
                KernelValue::F32(camera.vertical_fov_degrees),
            ),
            (SmolStr::new("jitter"), KernelValue::Vec2(jitter_pixels)),
            (
                SmolStr::new("max_distance"),
                KernelValue::F32(ray_budget.max_distance),
            ),
            (
                SmolStr::new("min_step"),
                KernelValue::F32(ray_budget.min_step),
            ),
            (
                SmolStr::new("hit_epsilon"),
                KernelValue::F32(ray_budget.hit_epsilon),
            ),
            (
                SmolStr::new("max_steps"),
                KernelValue::I32(ray_budget.max_steps),
            ),
            (
                SmolStr::new("legacy_world_up"),
                KernelValue::Vec3(compatibility.world_up),
            ),
            (
                SmolStr::new("legacy_view_scale"),
                KernelValue::F32(compatibility.view_scale),
            ),
            (
                SmolStr::new("legacy_active"),
                KernelValue::U32(u32::from(legacy_projection)),
            ),
        ],
    })
}

fn raygen_dispatch_value(viewport: CanonicalViewportInput) -> KernelValue {
    KernelValue::Struct(crate::kernel::KernelStructValue {
        name: SmolStr::new("PrimaryRaygenDispatch"),
        fields: vec![
            (
                SmolStr::new("item_count"),
                KernelValue::U32(viewport.width.saturating_mul(viewport.height)),
            ),
            (
                SmolStr::new("viewport_width"),
                KernelValue::U32(viewport.width),
            ),
            (
                SmolStr::new("viewport_height"),
                KernelValue::U32(viewport.height),
            ),
        ],
    })
}

fn materialize_dispatch_value(
    viewport: CanonicalViewportInput,
    primary_viewport: CanonicalViewportInput,
) -> KernelValue {
    KernelValue::Struct(crate::kernel::KernelStructValue {
        name: SmolStr::new("PrimaryMaterializeDispatch"),
        fields: vec![
            (
                SmolStr::new("item_count"),
                KernelValue::U32(viewport.width.saturating_mul(viewport.height)),
            ),
            (
                SmolStr::new("output_width"),
                KernelValue::U32(viewport.width),
            ),
            (
                SmolStr::new("output_height"),
                KernelValue::U32(viewport.height),
            ),
            (
                SmolStr::new("internal_width"),
                KernelValue::U32(primary_viewport.width),
            ),
            (
                SmolStr::new("internal_height"),
                KernelValue::U32(primary_viewport.height),
            ),
        ],
    })
}

fn primary_raygen_shader_source() -> Result<String, PresentationExecError> {
    let source = portable_abi_emit_wgsl_structs(&[
        raygen_dispatch_abi(),
        raygen_camera_abi(),
        portable_builtin_record_abi("RayQuery").expect("RayQuery abi"),
    ])
    .map_err(|err| PresentationExecError::UnsupportedPlan {
        message: format!("failed to emit primary raygen ABI structs: {err}"),
    })?;
    Ok(format!(
        "{source}

override WG_SIZE: u32 = {PRIMARY_HELPER_WORKGROUP_SIZE}u;

@group(2) @binding(0)
var<storage, read> dispatch_config: PrimaryRaygenDispatch;

@group(2) @binding(1)
var<storage, read> camera_config: PrimaryRaygenCamera;

struct OutputBuffer {{
  values: array<RayQuery>,
}}

struct DummyBuffer {{
  values: array<u32>,
}}

@group(2) @binding(2)
var<storage, read_write> output_items: OutputBuffer;
@group(2) @binding(3)
var<storage, read> dummy_items: DummyBuffer;

fn wr_normalize_or(value: vec3<f32>, fallback: vec3<f32>) -> vec3<f32> {{
  let len_sq = dot(value, value);
  if (len_sq <= 0.0000001) {{
    return fallback;
  }}
  return normalize(value);
}}

@compute @workgroup_size(WG_SIZE)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {{
  let index = global_id.x;
  _ = dummy_items.values[0];
  if (index >= dispatch_config.item_count) {{
    return;
  }}
  let width = max(dispatch_config.viewport_width, 1u);
  let height = max(dispatch_config.viewport_height, 1u);
  let x = index % width;
  let y = index / width;
  let uv = vec2<f32>(
    (f32(x) + 0.5 + camera_config.jitter.x) / f32(width),
    (f32(y) + 0.5 + camera_config.jitter.y) / f32(height),
  );
  let forward = wr_normalize_or(camera_config.forward, vec3<f32>(0.0, 0.0, -1.0));
  var direction = forward;
  if (camera_config.legacy_active != 0u) {{
    let right = wr_normalize_or(cross(forward, camera_config.legacy_world_up), vec3<f32>(1.0, 0.0, 0.0));
    let up = wr_normalize_or(cross(right, forward), vec3<f32>(0.0, 1.0, 0.0));
    let aspect = f32(width) / f32(height);
    let screen_x = (uv.x * 2.0 - 1.0) * aspect * camera_config.legacy_view_scale;
    let screen_y = (1.0 - uv.y * 2.0) * camera_config.legacy_view_scale;
    direction = wr_normalize_or(forward + (right * screen_x) + (up * screen_y), forward);
  }} else {{
    let right = wr_normalize_or(cross(forward, camera_config.up), vec3<f32>(1.0, 0.0, 0.0));
    let up = wr_normalize_or(cross(right, forward), vec3<f32>(0.0, 1.0, 0.0));
    let aspect = f32(width) / f32(height);
    let vertical_scale = tan(radians(camera_config.vertical_fov_degrees) * 0.5);
    let screen_x = (uv.x * 2.0 - 1.0) * aspect * vertical_scale;
    let screen_y = (1.0 - uv.y * 2.0) * vertical_scale;
    direction = wr_normalize_or(forward + (right * screen_x) + (up * screen_y), forward);
  }}
  output_items.values[index] = RayQuery(
    camera_config.position,
    direction,
    camera_config.max_distance,
    camera_config.min_step,
    camera_config.hit_epsilon,
    camera_config.max_steps
  );
}}
"
    ))
}

fn primary_hit_materialize_shader_source() -> Result<String, PresentationExecError> {
    primary_materialize_shader_source(PrimaryMaterializeKind::Hit)
}

fn depth_materialize_shader_source() -> Result<String, PresentationExecError> {
    primary_materialize_shader_source(PrimaryMaterializeKind::Depth)
}

fn normal_materialize_shader_source() -> Result<String, PresentationExecError> {
    primary_materialize_shader_source(PrimaryMaterializeKind::WorldNormal)
}

#[derive(Clone, Copy)]
enum PrimaryMaterializeKind {
    Hit,
    Depth,
    WorldNormal,
}

fn primary_materialize_shader_source(
    kind: PrimaryMaterializeKind,
) -> Result<String, PresentationExecError> {
    let roots = vec![
        materialize_dispatch_abi(),
        portable_builtin_record_abi("Hit3").expect("Hit3 abi"),
    ];
    let (output_struct, output_write) = match kind {
        PrimaryMaterializeKind::Hit => (
            "struct OutputBuffer { values: array<Hit3>, }\n".to_string(),
            "output_items.values[index] = hit;".to_string(),
        ),
        PrimaryMaterializeKind::Depth => (
            "struct OutputBuffer { values: array<f32>, }\n".to_string(),
            "output_items.values[index] = select(bitcast<f32>(0x7f800000u), hit.distance, hit.hit != 0u);".to_string(),
        ),
        PrimaryMaterializeKind::WorldNormal => (
            "struct OutputBuffer { values: array<vec3<f32>>, }\n".to_string(),
            "output_items.values[index] = select(vec3<f32>(0.0, 0.0, 0.0), hit.normal, hit.hit != 0u);".to_string(),
        ),
    };
    let source = portable_abi_emit_wgsl_structs(&roots).map_err(|err| {
        PresentationExecError::UnsupportedPlan {
            message: format!("failed to emit primary materialize ABI structs: {err}"),
        }
    })?;
    Ok(format!(
        "{source}

override WG_SIZE: u32 = {PRIMARY_HELPER_WORKGROUP_SIZE}u;

@group(2) @binding(0)
var<storage, read> dispatch_config: PrimaryMaterializeDispatch;

struct InputBuffer {{
  values: array<Hit3>,
}}

{output_struct}
struct DummyBuffer {{
  values: array<u32>,
}}

@group(2) @binding(1)
var<storage, read> input_items: InputBuffer;
@group(2) @binding(2)
var<storage, read_write> output_items: OutputBuffer;
@group(2) @binding(3)
var<storage, read> dummy_items: DummyBuffer;

fn internal_index_for_output(index: u32) -> u32 {{
  let output_width = max(dispatch_config.output_width, 1u);
  let output_height = max(dispatch_config.output_height, 1u);
  let internal_width = max(dispatch_config.internal_width, 1u);
  let internal_height = max(dispatch_config.internal_height, 1u);
  let x = index % output_width;
  let y = index / output_width;
  let internal_x = (x * internal_width) / output_width;
  let internal_y = (y * internal_height) / output_height;
  return min(
    internal_y * internal_width + internal_x,
    max(dispatch_config.item_count, 1u) - 1u
  );
}}

@compute @workgroup_size(WG_SIZE)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {{
  let index = global_id.x;
  _ = dummy_items.values[0];
  if (index >= dispatch_config.item_count) {{
    return;
  }}
  let hit = input_items.values[internal_index_for_output(index)];
  {output_write}
}}
"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::KernelStructValue;

    #[test]
    fn materialize_decode_uses_primary_hit_layout_count() {
        let abi = portable_builtin_record_abi("Hit3").expect("Hit3 abi");
        let bytes = vec![0u8; portable_abi_layout(&abi).size as usize];
        let mut arena = GpuAttachmentArena::new(1, 1);
        arena.attachments.insert(
            SmolStr::new("primary_hit"),
            crate::presentation_exec::gpu_resources::GpuAttachmentSlot::new_cpu(
                crate::presentation_exec::resources::FrameAttachmentLayout {
                    attachment: crate::presentation_contract::FrameAttachmentContract {
                        name: SmolStr::new("primary_hit"),
                        kind: crate::presentation_contract::FrameAttachmentKind::PrimaryHit,
                        element_schema: crate::presentation_contract::AttachmentElementSchema::NamedRecord(SmolStr::new("Hit3")),
                        resolution: crate::presentation_contract::AttachmentResolutionClass::Viewport,
                        scale: crate::presentation_contract::AttachmentResolutionScale::full(),
                        clear_policy: crate::presentation_contract::AttachmentClearPolicy::SemanticDefault,
                        lifetime: crate::presentation_contract::AttachmentLifetime::Transient,
                    },
                    width: 1,
                    height: 1,
                    element_abi: abi,
                    element_size: portable_abi_layout(&portable_builtin_record_abi("Hit3").expect("Hit3 abi")).size,
                    element_stride: crate::portable::portable_abi_array_stride(&portable_builtin_record_abi("Hit3").expect("Hit3 abi")),
                    total_size: bytes.len() as u32,
                    wgsl_storage_type: "Abi_Hit3".to_string(),
                    plan: crate::presentation_exec::resources::FrameAttachmentLayoutPlan {
                        meaning: crate::presentation_exec::resources::FrameAttachmentLayoutMeaning {
                            attachment: crate::presentation_contract::FrameAttachmentContract {
                                name: SmolStr::new("primary_hit"),
                                kind: crate::presentation_contract::FrameAttachmentKind::PrimaryHit,
                                element_schema: crate::presentation_contract::AttachmentElementSchema::NamedRecord(SmolStr::new("Hit3")),
                                resolution: crate::presentation_contract::AttachmentResolutionClass::Viewport,
                                scale: crate::presentation_contract::AttachmentResolutionScale::full(),
                                clear_policy: crate::presentation_contract::AttachmentClearPolicy::SemanticDefault,
                                lifetime: crate::presentation_contract::AttachmentLifetime::Transient,
                            },
                            element_abi: portable_builtin_record_abi("Hit3").expect("Hit3 abi"),
                            width: 1,
                            height: 1,
                        },
                        physical: crate::artifact_layout::PhysicalLayoutPlan::dense_buffer(
                            1,
                            1,
                            crate::portable::portable_abi_array_stride(
                                &portable_builtin_record_abi("Hit3").expect("Hit3 abi"),
                            ),
                        ),
                        element_size: portable_abi_layout(
                            &portable_builtin_record_abi("Hit3").expect("Hit3 abi"),
                        )
                        .size,
                        wgsl_storage_type: "Abi_Hit3".to_string(),
                    },
                },
                bytes.clone(),
            ),
        );
        let contract = PrimaryVisibilityPassContract {
            query_contract: QueryContractId::new("spatial.nearest.batch.world"),
            primary_hit_attachment: SmolStr::new("primary_hit"),
            depth_attachment: None,
            world_normal_attachment: None,
        };
        let values = decode_primary_hit_attachment_bytes(&arena, &contract, &bytes)
            .expect("decode primary hit bytes");
        assert_eq!(values.len(), 1);
    }

    #[test]
    fn candidate_shape_indices_pack_side_channel_and_preserve_world_shapes() {
        let packed = pack_candidate_span_scene_indices(vec![4, 2, 8, 1], vec![11, 13]);
        assert_eq!(packed, vec![4, 2, 8, 1, 11, 13]);
    }

    #[test]
    fn empty_candidate_spans_skip_side_channel_upload_and_keep_dispatch_disabled() {
        let mut request = GpuDispatchRequest {
            dispatch: KernelValue::Struct(KernelStructValue {
                name: SmolStr::new("WgslDispatchConfig"),
                fields: vec![(
                    SmolStr::new("candidate_spans_enabled"),
                    KernelValue::Bool(true),
                )],
            }),
            items: Vec::new(),
            world_shape_indices: vec![7, 8, 9],
            accel_nodes: Vec::new(),
            accel_children: Vec::new(),
            cache_bricks: Vec::new(),
            continuation_seeds: Vec::new(),
            candidate_spans: Vec::new(),
            resident_scene_snapshot: None,
            resident_scene_detail: 0,
            resident_scene_selection_signature: 0,
        };

        let side_channel_bytes =
            finalize_candidate_span_side_channel(&mut request).expect("finalize empty spans");

        assert!(side_channel_bytes.is_none());
        let KernelValue::Struct(dispatch) = &request.dispatch else {
            panic!("dispatch config");
        };
        assert_eq!(
            dispatch
                .fields
                .iter()
                .find(|(name, _)| name == "candidate_spans_enabled")
                .map(|(_, value)| value.clone()),
            Some(KernelValue::Bool(false))
        );
    }

    #[test]
    fn candidate_span_side_channel_encodes_words_and_marks_dispatch_enabled() {
        let mut request = GpuDispatchRequest {
            dispatch: KernelValue::Struct(KernelStructValue {
                name: SmolStr::new("WgslDispatchConfig"),
                fields: vec![(
                    SmolStr::new("candidate_spans_enabled"),
                    KernelValue::Bool(false),
                )],
            }),
            items: Vec::new(),
            world_shape_indices: vec![7, 8, 9],
            accel_nodes: Vec::new(),
            accel_children: Vec::new(),
            cache_bricks: Vec::new(),
            continuation_seeds: Vec::new(),
            candidate_spans: vec![4, 2, 8, 1],
            resident_scene_snapshot: None,
            resident_scene_detail: 0,
            resident_scene_selection_signature: 0,
        };

        let side_channel_bytes =
            finalize_candidate_span_side_channel(&mut request).expect("finalize spans");
        let decoded = crate::portable::portable_abi_decode_slice(
            &PortableAbiType::U32,
            &side_channel_bytes.expect("candidate span upload"),
            4,
        )
        .expect("decode candidate span upload");

        assert_eq!(
            decoded,
            vec![
                KernelValue::U32(4),
                KernelValue::U32(2),
                KernelValue::U32(8),
                KernelValue::U32(1)
            ]
        );
        let KernelValue::Struct(dispatch) = &request.dispatch else {
            panic!("dispatch config");
        };
        assert_eq!(
            dispatch
                .fields
                .iter()
                .find(|(name, _)| name == "candidate_spans_enabled")
                .map(|(_, value)| value.clone()),
            Some(KernelValue::Bool(true))
        );
    }
}
