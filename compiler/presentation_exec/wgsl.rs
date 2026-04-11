use crate::kernel::{KernelStructValue, KernelValue, lower_batch_query_plan};
use crate::portable::{
    PortableAbiType, PortableStructField, portable_abi_emit_wgsl_structs,
    portable_abi_layout, portable_builtin_record_abi,
};
use crate::presentation_exec::resources::AttachmentResourceSet;
use crate::presentation_exec::temporal::{
    motion_resolve, temporal_resolve_kernel_values, update_query_trace_continuation,
};
use crate::presentation_exec::{
    PresentationExecError, PresentationExecutionInput, PresentationExecutionResult,
    allocate_execution_attachments, build_temporal_history, execute_batch_contract, expect_array,
    expect_struct, expect_vec3, field, frame_state_components, generate_screen_samples,
    lighting_inputs_value, materialize_primary_visibility_attachments,
    point_direction_query_value, point_query_value, presentation_metrics, screen_sample_ray,
};
use crate::presentation_plan::{
    CompositeColorPassContract, ParticipantsResolvePassContract, PresentationPassKind,
    PresentationPlan, ShadePrimaryPassContract, SurfaceResolvePassContract,
    TemporalResolvePassContract,
};
use crate::query_exec::cpu::{default_medium, default_surface};
use crate::query_exec::wgsl::{
    compiled_pipeline, encode_slice, encode_value, native_wgpu_context, readback_storage_buffer,
};
use crate::query_exec::{QueryExecContext, execute_batch_query_with_trace_on};
use crate::query_plan::{BatchQueryPlan, DispatchBackend};
use smol_str::SmolStr;
use wgpu::util::DeviceExt;

const PRESENTATION_WGSL_WORKGROUP_SIZE: u32 = 64;

pub(super) fn execute_plan(
    ctx: &QueryExecContext,
    plan: &PresentationPlan,
    input: &PresentationExecutionInput,
) -> Result<PresentationExecutionResult, PresentationExecError> {
    let (camera, viewport, jitter_pixels) = frame_state_components(&input.frame_state)?;
    let screen_samples = generate_screen_samples(
        plan,
        input,
        camera,
        viewport,
        jitter_pixels,
        input.ray_budget,
    );
    let mut attachments =
        allocate_execution_attachments(&plan.frame, viewport.width, viewport.height, input.history.as_ref())?;
    let mut primary_hits = None;
    let mut primary_trace = None;
    let mut primary_solver_summary = None;
    let mut continuation_counts = crate::presentation_exec::temporal::ContinuationCounts::default();

    for pass in &plan.passes {
        match &pass.kind {
            PresentationPassKind::GenerateScreenSamples { .. } => {}
            PresentationPassKind::PrimaryVisibility { contract } => {
                let rays = screen_samples
                    .iter()
                    .map(screen_sample_ray)
                    .collect::<Result<Vec<_>, _>>()?;
                let batch_plan = lower_batch_query_plan(
                    &BatchQueryPlan::for_contract(
                        contract.query_contract,
                        DispatchBackend::Wgsl,
                        None,
                    )
                    .map_err(|message| {
                        PresentationExecError::UnsupportedPlan {
                            message: message.to_string(),
                        }
                    })?,
                );
                let (hits, query_trace) = execute_batch_query_with_trace_on(
                    ctx,
                    DispatchBackend::Wgsl,
                    &batch_plan,
                    &[
                        KernelValue::Capture(input.region_capture.clone()),
                        input.frame_domain.clone(),
                        KernelValue::Array(rays),
                    ],
                )?;
                let hits = expect_array(&hits)?.to_vec();
                materialize_primary_visibility_attachments(&mut attachments, &hits, contract)?;
                primary_solver_summary = batch_plan
                    .ray_solver
                    .as_ref()
                    .map(|solver| solver.diagnostic_summary());
                primary_trace = Some(query_trace);
                primary_hits = Some(hits);
            }
            PresentationPassKind::SurfaceResolve { contract } => {
                let hits = primary_hits.as_ref().ok_or_else(|| {
                    PresentationExecError::MissingPrimaryVisibilityPass {
                        plan: plan.name.clone(),
                    }
                })?;
                execute_surface_resolve(
                    ctx,
                    input,
                    &mut attachments,
                    hits,
                    contract,
                    DispatchBackend::Wgsl,
                )?;
            }
            PresentationPassKind::ParticipantsResolve { contract } => {
                let hits = primary_hits.as_ref().ok_or_else(|| {
                    PresentationExecError::MissingPrimaryVisibilityPass {
                        plan: plan.name.clone(),
                    }
                })?;
                execute_participants_resolve(
                    ctx,
                    input,
                    &screen_samples,
                    &mut attachments,
                    hits,
                    contract,
                    DispatchBackend::Wgsl,
                )?;
            }
            PresentationPassKind::ShadePrimary { contract } => {
                shade_primary_wgsl(
                    &screen_samples,
                    &mut attachments,
                    &input.lighting,
                    camera.position,
                    contract,
                )?;
            }
            PresentationPassKind::MotionResolve { contract } => {
                let hits = primary_hits.as_ref().ok_or_else(|| {
                    PresentationExecError::MissingPrimaryVisibilityPass {
                        plan: plan.name.clone(),
                    }
                })?;
                continuation_counts = motion_resolve(
                    plan,
                    input,
                    &mut attachments,
                    &screen_samples,
                    hits,
                    contract,
                )?;
            }
            PresentationPassKind::TemporalResolve { contract } => {
                continuation_counts.consumed +=
                    temporal_resolve_wgsl(&mut attachments, viewport.width, viewport.height, contract)?;
            }
            PresentationPassKind::CompositeColor { contract } => {
                composite_color_wgsl(&mut attachments, contract)?;
            }
            PresentationPassKind::ExportAttachment { .. } => {}
            other => {
                return Err(PresentationExecError::UnsupportedPlan {
                    message: format!("wgsl executor does not support pass kind {other:?}"),
                });
            }
        }
    }

    let primary_hits =
        primary_hits.ok_or_else(|| PresentationExecError::MissingPrimaryVisibilityPass {
            plan: plan.name.clone(),
        })?;
    let mut primary_trace =
        primary_trace.ok_or_else(|| PresentationExecError::MissingPrimaryVisibilityPass {
            plan: plan.name.clone(),
        })?;
    update_query_trace_continuation(&mut primary_trace, continuation_counts);
    let metrics = presentation_metrics(&primary_hits, &primary_trace, primary_solver_summary);
    let history = build_temporal_history(plan, &input.frame_state, &attachments)?;
    Ok(PresentationExecutionResult {
        plan_name: plan.name.clone(),
        backend: DispatchBackend::Wgsl,
        width: viewport.width,
        height: viewport.height,
        screen_samples,
        attachments,
        history,
        metrics,
        query_trace: primary_trace,
    })
}

fn execute_surface_resolve(
    ctx: &QueryExecContext,
    input: &PresentationExecutionInput,
    attachments: &mut AttachmentResourceSet,
    hits: &[KernelValue],
    contract: &SurfaceResolvePassContract,
    backend: DispatchBackend,
) -> Result<(), PresentationExecError> {
    let (surfaces, _) = execute_batch_contract(
        ctx,
        backend,
        contract.query_contract,
        &[
            KernelValue::Capture(input.region_capture.clone()),
            input.frame_domain.clone(),
            KernelValue::Array(hits.to_vec()),
        ],
    )?;
    let Some(surface_attachment) = attachments.attachment_mut(contract.surface_attachment.as_str())
    else {
        return Ok(());
    };
    let default_surface = default_surface();
    for (index, (hit, surface)) in hits.iter().zip(surfaces.iter()).enumerate() {
        if hit_flag(hit)? {
            surface_attachment.encode(index, surface)?;
        } else {
            surface_attachment.encode(index, &default_surface)?;
        }
    }
    Ok(())
}

fn execute_participants_resolve(
    ctx: &QueryExecContext,
    input: &PresentationExecutionInput,
    screen_samples: &[KernelValue],
    attachments: &mut AttachmentResourceSet,
    hits: &[KernelValue],
    contract: &ParticipantsResolvePassContract,
    backend: DispatchBackend,
) -> Result<(), PresentationExecError> {
    let (point_queries, point_direction_queries) =
        build_participant_query_items(input, screen_samples, hits, contract)?;
    if let (Some(query_contract), Some(attachment_name)) = (
        contract.radiance_query_contract,
        contract.radiance_attachment.as_deref(),
    ) {
        let (radiance, _) = execute_batch_contract(
            ctx,
            backend,
            query_contract,
            &[
                KernelValue::Capture(input.region_capture.clone()),
                input.frame_domain.clone(),
                KernelValue::Array(point_direction_queries.clone()),
            ],
        )?;
        encode_attachment_values(attachments, attachment_name, &radiance)?;
    }
    if let (Some(query_contract), Some(attachment_name)) = (
        contract.medium_query_contract,
        contract.medium_attachment.as_deref(),
    ) {
        let (medium, _) = execute_batch_contract(
            ctx,
            backend,
            query_contract,
            &[
                KernelValue::Capture(input.region_capture.clone()),
                input.frame_domain.clone(),
                KernelValue::Array(point_queries),
            ],
        )?;
        encode_attachment_values(attachments, attachment_name, &medium)?;
    }
    Ok(())
}

fn build_participant_query_items(
    input: &PresentationExecutionInput,
    screen_samples: &[KernelValue],
    hits: &[KernelValue],
    contract: &ParticipantsResolvePassContract,
) -> Result<(Vec<KernelValue>, Vec<KernelValue>), PresentationExecError> {
    let frame = expect_struct(&input.frame_state, "FrameState")?;
    let view = expect_struct(field(frame, "view")?, "ViewState")?;
    let camera = expect_struct(field(view, "camera")?, "Camera")?;
    let camera_position = expect_vec3(field(camera, "position")?)?;
    let mut point_queries = Vec::with_capacity(hits.len());
    let mut point_direction_queries = Vec::with_capacity(hits.len());
    for (sample, hit) in screen_samples.iter().zip(hits) {
        let ray = expect_struct(
            field(expect_struct(sample, "ScreenSampleQuery")?, "ray")?,
            "RayQuery",
        )?;
        let ray_direction = expect_vec3(field(ray, "direction")?)?;
        let point = if hit_flag(hit)? {
            hit_position(hit)?
        } else {
            add3(
                camera_position,
                mul3(ray_direction, contract.miss_sample_distance),
            )
        };
        point_queries.push(point_query_value(point));
        point_direction_queries.push(point_direction_query_value(point, ray_direction));
    }
    Ok((point_queries, point_direction_queries))
}

fn encode_attachment_values(
    attachments: &mut AttachmentResourceSet,
    name: &str,
    values: &[KernelValue],
) -> Result<(), PresentationExecError> {
    let Some(attachment) = attachments.attachment_mut(name) else {
        return Ok(());
    };
    for (index, value) in values.iter().enumerate() {
        attachment.encode(index, value)?;
    }
    Ok(())
}

fn shade_primary_wgsl(
    screen_samples: &[KernelValue],
    attachments: &mut AttachmentResourceSet,
    lighting: &crate::presentation_contract::PresentationLightingInputs,
    camera_position: [f32; 3],
    contract: &ShadePrimaryPassContract,
) -> Result<(), PresentationExecError> {
    let primary_hits = attachments.decode_attachment(contract.primary_hit_attachment.as_str())?;
    let surfaces = attachments.decode_attachment(contract.surface_attachment.as_str())?;
    let radiance = contract
        .radiance_attachment
        .as_ref()
        .map(|name| attachments.decode_attachment(name))
        .transpose()?;
    let medium = contract
        .medium_attachment
        .as_ref()
        .map(|name| attachments.decode_attachment(name))
        .transpose()?;
    let default_medium = default_medium();
    let shade_inputs = primary_hits
        .iter()
        .zip(&surfaces)
        .enumerate()
        .map(|(index, (hit, surface))| {
            let sample = screen_samples.get(index).expect("screen sample");
            let ray = expect_struct(
                field(expect_struct(sample, "ScreenSampleQuery")?, "ray")?,
                "RayQuery",
            )?;
            let ray_direction = expect_vec3(field(ray, "direction")?)?;
            Ok(KernelValue::Struct(KernelStructValue {
                name: SmolStr::new("ShadePrimaryInput"),
                fields: vec![
                    (SmolStr::new("hit"), hit.clone()),
                    (SmolStr::new("surface"), surface.clone()),
                    (
                        SmolStr::new("radiance"),
                        radiance
                            .as_ref()
                            .and_then(|values| values.get(index))
                            .cloned()
                            .unwrap_or(KernelValue::Vec3([0.0, 0.0, 0.0])),
                    ),
                    (
                        SmolStr::new("medium"),
                        medium
                            .as_ref()
                            .and_then(|values| values.get(index))
                            .cloned()
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

    let output_attachment = attachments
        .attachment_mut(contract.output_attachment.as_str())
        .ok_or_else(|| PresentationExecError::UnsupportedPlan {
            message: format!(
                "missing shade output attachment '{}'",
                contract.output_attachment
            ),
        })?;
    output_attachment.bytes = dispatch_linear_shader(
        &shade_primary_shader_source()?,
        &shade_primary_input_abi(),
        &shade_inputs,
        output_attachment.bytes.len() as u64,
    )?;
    Ok(())
}

fn composite_color_wgsl(
    attachments: &mut AttachmentResourceSet,
    contract: &CompositeColorPassContract,
) -> Result<(), PresentationExecError> {
    let input_values = attachments.decode_attachment(contract.input_attachment.as_str())?;
    let output_attachment = attachments
        .attachment_mut(contract.output_attachment.as_str())
        .ok_or_else(|| PresentationExecError::UnsupportedPlan {
            message: format!(
                "missing composite output attachment '{}'",
                contract.output_attachment
            ),
        })?;
    output_attachment.bytes = dispatch_linear_shader(
        &copy_vec3_shader_source()?,
        &PortableAbiType::Vec3,
        &input_values,
        output_attachment.bytes.len() as u64,
    )?;
    Ok(())
}

fn temporal_resolve_wgsl(
    attachments: &mut AttachmentResourceSet,
    width: u32,
    height: u32,
    contract: &TemporalResolvePassContract,
) -> Result<u32, PresentationExecError> {
    let (input_values, consumed_count) =
        temporal_resolve_kernel_values(attachments, width, height, contract)?;
    let output_size = attachments
        .attachment(contract.output_attachment.as_str())
        .ok_or_else(|| PresentationExecError::UnsupportedPlan {
            message: format!(
                "missing temporal resolve output attachment '{}'",
                contract.output_attachment
            ),
        })?
        .bytes
        .len() as u64;
    if input_values.is_empty() {
        return Ok(consumed_count);
    }
    let output_bytes = dispatch_linear_shader(
        &temporal_resolve_shader_source(contract)?,
        &temporal_resolve_input_abi(),
        &input_values,
        output_size,
    )?;
    attachments
        .attachment_mut(contract.output_attachment.as_str())
        .expect("temporal output attachment")
        .bytes = output_bytes.clone();
    if let Some(history_color) = attachments.attachment_mut(contract.history_color_attachment.as_str()) {
        history_color.bytes = output_bytes;
    }
    if let Some(history_primary_hit_attachment) = &contract.history_primary_hit_attachment {
        let primary_hits = attachments.decode_attachment(contract.primary_hit_attachment.as_str())?;
        if let Some(history_primary_hit) = attachments.attachment_mut(history_primary_hit_attachment.as_str()) {
            for (index, hit) in primary_hits.iter().enumerate() {
                history_primary_hit.encode(index, hit)?;
            }
        }
    }
    Ok(consumed_count)
}

fn dispatch_linear_shader(
    source: &str,
    input_abi: &PortableAbiType,
    input_values: &[KernelValue],
    output_size: u64,
) -> Result<Vec<u8>, PresentationExecError> {
    if input_values.is_empty() {
        return Ok(Vec::new());
    }
    let native = native_wgpu_context()?;
    let dispatch_abi = crate::query_exec::wgsl::codegen::wgsl_dispatch_config_abi();
    let dispatch_buffer = native
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("wrela.presentation.dispatch"),
            contents: &encode_value(
                &dispatch_abi,
                &presentation_dispatch_config(input_values.len() as u32),
            )
            .map_err(PresentationExecError::Query)?,
            usage: wgpu::BufferUsages::STORAGE,
        });
    let input_buffer = native
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("wrela.presentation.input"),
            contents: &encode_slice(input_abi, input_values)
                .map_err(PresentationExecError::Query)?,
            usage: wgpu::BufferUsages::STORAGE,
        });
    let output_buffer = native.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("wrela.presentation.output"),
        size: output_size.max(4),
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let aux_buffer = native
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("wrela.presentation.aux"),
            contents: &[0u8; 4],
            usage: wgpu::BufferUsages::STORAGE,
        });
    let cached = compiled_pipeline(
        native,
        source,
        PRESENTATION_WGSL_WORKGROUP_SIZE,
        wgpu::BufferSize::new(portable_abi_layout(&dispatch_abi).size as u64),
    )?;
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
    let mut encoder = native
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("wrela.presentation.encoder"),
        });
    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("wrela.presentation.compute"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&cached.pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups(
            (input_values.len() as u32)
                .div_ceil(PRESENTATION_WGSL_WORKGROUP_SIZE)
                .max(1),
            1,
            1,
        );
    }
    native.queue.submit(Some(encoder.finish()));
    readback_storage_buffer(&output_buffer, output_size).map_err(PresentationExecError::Query)
}

fn presentation_dispatch_config(item_count: u32) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("WgslDispatchConfig"),
        fields: vec![
            (SmolStr::new("capture_kind"), KernelValue::U32(0)),
            (SmolStr::new("capture_index"), KernelValue::U32(0)),
            (SmolStr::new("item_count"), KernelValue::U32(item_count)),
            (SmolStr::new("shape_count"), KernelValue::U32(0)),
            (SmolStr::new("material_enabled"), KernelValue::Bool(false)),
            (SmolStr::new("radiance_enabled"), KernelValue::Bool(false)),
            (SmolStr::new("media_enabled"), KernelValue::Bool(false)),
        ],
    })
}

fn shade_primary_input_abi() -> PortableAbiType {
    PortableAbiType::Struct {
        name: SmolStr::new("ShadePrimaryInput"),
        class_id: 0,
        fields: vec![
            PortableStructField {
                name: SmolStr::new("hit"),
                ty: portable_builtin_record_abi("Hit3").expect("Hit3 abi"),
            },
            PortableStructField {
                name: SmolStr::new("surface"),
                ty: portable_builtin_record_abi("Surface").expect("Surface abi"),
            },
            PortableStructField {
                name: SmolStr::new("radiance"),
                ty: PortableAbiType::Vec3,
            },
            PortableStructField {
                name: SmolStr::new("medium"),
                ty: portable_builtin_record_abi("Medium").expect("Medium abi"),
            },
            PortableStructField {
                name: SmolStr::new("ray_direction"),
                ty: PortableAbiType::Vec3,
            },
            PortableStructField {
                name: SmolStr::new("camera_position"),
                ty: PortableAbiType::Vec3,
            },
            PortableStructField {
                name: SmolStr::new("lighting"),
                ty: lighting_inputs_abi(),
            },
        ],
    }
}

fn lighting_inputs_abi() -> PortableAbiType {
    PortableAbiType::Struct {
        name: SmolStr::new("PresentationLightingInputs"),
        class_id: 0,
        fields: vec![
            PortableStructField {
                name: SmolStr::new("key_light"),
                ty: portable_builtin_record_abi("Light").expect("Light abi"),
            },
            PortableStructField {
                name: SmolStr::new("fill_direction"),
                ty: PortableAbiType::Vec3,
            },
            PortableStructField {
                name: SmolStr::new("fill_strength"),
                ty: PortableAbiType::F32,
            },
            PortableStructField {
                name: SmolStr::new("ambient_color"),
                ty: PortableAbiType::Vec3,
            },
        ],
    }
}

fn temporal_resolve_input_abi() -> PortableAbiType {
    PortableAbiType::Struct {
        name: SmolStr::new("TemporalResolveInput"),
        class_id: 0,
        fields: vec![
            PortableStructField {
                name: SmolStr::new("current_color"),
                ty: PortableAbiType::Vec3,
            },
            PortableStructField {
                name: SmolStr::new("history_color"),
                ty: PortableAbiType::Vec3,
            },
            PortableStructField {
                name: SmolStr::new("clamp_min"),
                ty: PortableAbiType::Vec3,
            },
            PortableStructField {
                name: SmolStr::new("clamp_max"),
                ty: PortableAbiType::Vec3,
            },
            PortableStructField {
                name: SmolStr::new("use_history"),
                ty: PortableAbiType::Bool,
            },
        ],
    }
}

fn shade_primary_shader_source() -> Result<String, PresentationExecError> {
    let structs = emit_wgsl_structs(&[
        crate::query_exec::wgsl::codegen::wgsl_dispatch_config_abi(),
        shade_primary_input_abi(),
    ])?;
    Ok(format!(
        "{structs}

override WG_SIZE: u32 = {PRESENTATION_WGSL_WORKGROUP_SIZE}u;

@group(0) @binding(0)
var<storage, read> dispatch_config: Abi_WgslDispatchConfig;

struct InputBuffer {{
  values: array<Abi_ShadePrimaryInput>,
}}

struct OutputBuffer {{
  values: array<vec3<f32>>,
}}

struct DummyBuffer {{
  values: array<u32>,
}}

@group(0) @binding(1)
var<storage, read> input_items: InputBuffer;
@group(0) @binding(2)
var<storage, read_write> output_items: OutputBuffer;
@group(0) @binding(3)
var<storage, read> dummy_items: DummyBuffer;

fn clamp_vec3(value: vec3<f32>, min_value: f32, max_value: f32) -> vec3<f32> {{
  return vec3<f32>(
    clamp(value.x, min_value, max_value),
    clamp(value.y, min_value, max_value),
    clamp(value.z, min_value, max_value)
  );
}}

@compute @workgroup_size(WG_SIZE)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {{
  let index = global_id.x;
  _ = dummy_items.values[0];
  if (index >= dispatch_config.item_count) {{
    return;
  }}
  let input = input_items.values[index];
  if (input.hit.hit != 0u) {{
    let key_delta = input.lighting.key_light.position - input.hit.position;
    let key_dir = normalize(key_delta);
    let view_dir = normalize(input.camera_position - input.hit.position);
    let half_dir = normalize(key_dir + view_dir);
    let distance_to_light = length(key_delta);
    let attenuation = clamp(1.0 - (distance_to_light / max(input.lighting.key_light.range, 0.00001)), 0.0, 1.0);
    let ndotl = max(dot(input.hit.normal, key_dir), 0.0);
    let ndoth = max(dot(input.hit.normal, half_dir), 0.0);
    let diffuse = ndotl * attenuation;
    let fill = max(dot(input.hit.normal, normalize(input.lighting.fill_direction)), 0.0) * input.lighting.fill_strength;
    let roughness = clamp(input.surface.roughness, 0.0, 1.0);
    let spec_power = mix(48.0, 8.0, roughness);
    let metalness = clamp(input.surface.metalness, 0.0, 1.0);
    let clearcoat = clamp(input.surface.clearcoat, 0.0, 1.0);
    let highlight = pow(ndoth, spec_power) * (0.10 + (metalness * 0.25) + (clearcoat * 0.20));
    let lighting_rgb = input.lighting.ambient_color + vec3<f32>(diffuse + fill);
    let direct = clamp_vec3(
      (input.surface.albedo * lighting_rgb * input.lighting.key_light.intensity)
        + vec3<f32>(highlight * 220.0, highlight * 208.0, highlight * 196.0),
      0.0,
      255.0,
    );
    let fog_strength = clamp(input.medium.density * distance_to_light * 0.18, 0.0, 0.55);
    let fog_color = input.medium.emission + (input.radiance * 0.22);
    let radiance_lit = input.radiance * (0.25 + (highlight * 0.15));
    let lit = direct + input.surface.emissive + radiance_lit;
    output_items.values[index] = mix(lit, fog_color, vec3<f32>(fog_strength));
  }} else {{
    let miss_fog = clamp(input.medium.density * 3.0, 0.0, 0.45);
    let miss_mix_color = input.medium.emission + (input.radiance * 0.28);
    output_items.values[index] = mix(input.radiance, miss_mix_color, vec3<f32>(miss_fog));
  }}
}}
"
    ))
}

fn copy_vec3_shader_source() -> Result<String, PresentationExecError> {
    let structs =
        emit_wgsl_structs(&[crate::query_exec::wgsl::codegen::wgsl_dispatch_config_abi()])?;
    Ok(format!(
        "{structs}

override WG_SIZE: u32 = {PRESENTATION_WGSL_WORKGROUP_SIZE}u;

@group(0) @binding(0)
var<storage, read> dispatch_config: Abi_WgslDispatchConfig;

struct InputBuffer {{
  values: array<vec3<f32>>,
}}

struct OutputBuffer {{
  values: array<vec3<f32>>,
}}

struct DummyBuffer {{
  values: array<u32>,
}}

@group(0) @binding(1)
var<storage, read> input_items: InputBuffer;
@group(0) @binding(2)
var<storage, read_write> output_items: OutputBuffer;
@group(0) @binding(3)
var<storage, read> dummy_items: DummyBuffer;

@compute @workgroup_size(WG_SIZE)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {{
  let index = global_id.x;
  _ = dummy_items.values[0];
  if (index >= dispatch_config.item_count) {{
    return;
  }}
  output_items.values[index] = input_items.values[index];
}}
"
    ))
}

fn temporal_resolve_shader_source(
    contract: &TemporalResolvePassContract,
) -> Result<String, PresentationExecError> {
    let structs = emit_wgsl_structs(&[
        crate::query_exec::wgsl::codegen::wgsl_dispatch_config_abi(),
        temporal_resolve_input_abi(),
    ])?;
    let history_weight = contract.history_weight_numerator as f32
        / contract.history_weight_denominator.max(1) as f32;
    Ok(format!(
        "{structs}

override WG_SIZE: u32 = {PRESENTATION_WGSL_WORKGROUP_SIZE}u;

@group(0) @binding(0)
var<storage, read> dispatch_config: Abi_WgslDispatchConfig;

struct InputBuffer {{
  values: array<Abi_TemporalResolveInput>,
}}

struct OutputBuffer {{
  values: array<vec3<f32>>,
}}

struct DummyBuffer {{
  values: array<u32>,
}}

@group(0) @binding(1)
var<storage, read> input_items: InputBuffer;
@group(0) @binding(2)
var<storage, read_write> output_items: OutputBuffer;
@group(0) @binding(3)
var<storage, read> dummy_items: DummyBuffer;

fn clamp_vec3(value: vec3<f32>, min_value: vec3<f32>, max_value: vec3<f32>) -> vec3<f32> {{
  return vec3<f32>(
    clamp(value.x, min_value.x, max_value.x),
    clamp(value.y, min_value.y, max_value.y),
    clamp(value.z, min_value.z, max_value.z)
  );
}}

@compute @workgroup_size(WG_SIZE)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {{
  let index = global_id.x;
  _ = dummy_items.values[0];
  if (index >= dispatch_config.item_count) {{
    return;
  }}
  let input = input_items.values[index];
  if (input.use_history != 0u) {{
    let clamped_history = clamp_vec3(input.history_color, input.clamp_min, input.clamp_max);
    output_items.values[index] = mix(input.current_color, clamped_history, vec3<f32>({history_weight}));
  }} else {{
    output_items.values[index] = input.current_color;
  }}
}}
"
    ))
}

fn emit_wgsl_structs(roots: &[PortableAbiType]) -> Result<String, PresentationExecError> {
    let prefixed = roots
        .iter()
        .cloned()
        .map(prefix_abi_name)
        .collect::<Vec<_>>();
    portable_abi_emit_wgsl_structs(&prefixed).map_err(|err| {
        PresentationExecError::UnsupportedPlan {
            message: err.to_string(),
        }
    })
}

fn prefix_abi_name(abi: PortableAbiType) -> PortableAbiType {
    match abi {
        PortableAbiType::Struct {
            name,
            class_id,
            fields,
        } => PortableAbiType::Struct {
            name: SmolStr::new(format!("Abi_{name}")),
            class_id,
            fields: fields
                .into_iter()
                .map(|field| PortableStructField {
                    name: field.name,
                    ty: prefix_abi_name(field.ty),
                })
                .collect(),
        },
        PortableAbiType::Array(inner, len) => {
            PortableAbiType::Array(Box::new(prefix_abi_name(*inner)), len)
        }
        other => other,
    }
}

fn hit_flag(value: &KernelValue) -> Result<bool, PresentationExecError> {
    match field(expect_struct(value, "Hit3")?, "hit")? {
        KernelValue::Bool(value) => Ok(*value),
        other => Err(PresentationExecError::TypeMismatch {
            expected: "Boolean".to_string(),
            found: format!("{other:?}"),
        }),
    }
}

fn hit_position(value: &KernelValue) -> Result<[f32; 3], PresentationExecError> {
    expect_vec3(field(expect_struct(value, "Hit3")?, "position")?)
}

fn add3(lhs: [f32; 3], rhs: [f32; 3]) -> [f32; 3] {
    [lhs[0] + rhs[0], lhs[1] + rhs[1], lhs[2] + rhs[2]]
}

fn mul3(value: [f32; 3], scalar: f32) -> [f32; 3] {
    [value[0] * scalar, value[1] * scalar, value[2] * scalar]
}
