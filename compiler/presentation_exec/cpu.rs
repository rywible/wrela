use crate::kernel::{KernelValue, lower_batch_query_plan};
use crate::presentation_exec::resources::AttachmentResourceSet;
use crate::presentation_exec::temporal::{
    motion_resolve, temporal_resolve_cpu, update_query_trace_continuation,
};
use crate::presentation_exec::{
    PresentationExecError, PresentationExecutionInput, PresentationExecutionResult,
    allocate_execution_attachments, build_temporal_history, execute_batch_contract, expect_array,
    expect_f32, expect_struct, expect_vec3, field, frame_state_components,
    generate_screen_samples, hit_world_normal,
    materialize_primary_visibility_attachments, point_direction_query_value, point_query_value,
    presentation_metrics, screen_sample_ray,
};
use crate::presentation_plan::{
    CompositeColorPassContract, ParticipantsResolvePassContract, PresentationPassKind,
    PresentationPlan, ShadePrimaryPassContract, SurfaceResolvePassContract,
};
use crate::query_exec::cpu::{default_medium, default_surface};
use crate::query_exec::{QueryExecContext, execute_batch_query_with_trace_on};
use crate::query_plan::{BatchQueryPlan, DispatchBackend};

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
                        DispatchBackend::Cpu,
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
                    DispatchBackend::Cpu,
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
                    DispatchBackend::Cpu,
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
                    DispatchBackend::Cpu,
                )?;
            }
            PresentationPassKind::ShadePrimary { contract } => {
                shade_primary_cpu(
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
                    temporal_resolve_cpu(&mut attachments, viewport.width, viewport.height, contract)?;
            }
            PresentationPassKind::CompositeColor { contract } => {
                composite_color_cpu(&mut attachments, contract)?;
            }
            PresentationPassKind::ExportAttachment { .. } => {}
            other => {
                return Err(PresentationExecError::UnsupportedPlan {
                    message: format!("cpu executor does not support pass kind {other:?}"),
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
        backend: DispatchBackend::Cpu,
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

fn shade_primary_cpu(
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
    let Some(output_attachment) = attachments.attachment_mut(contract.output_attachment.as_str())
    else {
        return Ok(());
    };
    for index in 0..primary_hits.len() {
        let sample =
            screen_samples
                .get(index)
                .ok_or_else(|| PresentationExecError::UnsupportedPlan {
                    message: "screen sample count drifted from primary-hit attachment".to_string(),
                })?;
        let ray = expect_struct(
            field(expect_struct(sample, "ScreenSampleQuery")?, "ray")?,
            "RayQuery",
        )?;
        let ray_direction = expect_vec3(field(ray, "direction")?)?;
        let color = shade_compatibility_color(
            &primary_hits[index],
            &surfaces[index],
            radiance
                .as_ref()
                .and_then(|values| values.get(index))
                .unwrap_or(&KernelValue::Vec3([0.0, 0.0, 0.0])),
            medium
                .as_ref()
                .and_then(|values| values.get(index))
                .unwrap_or(&default_medium),
            ray_direction,
            camera_position,
            lighting,
        )?;
        output_attachment.encode(index, &KernelValue::Vec3(color))?;
    }
    Ok(())
}

fn composite_color_cpu(
    attachments: &mut AttachmentResourceSet,
    contract: &CompositeColorPassContract,
) -> Result<(), PresentationExecError> {
    let input_values = attachments.decode_attachment(contract.input_attachment.as_str())?;
    encode_attachment_values(
        attachments,
        contract.output_attachment.as_str(),
        &input_values,
    )
}

fn shade_compatibility_color(
    hit: &KernelValue,
    surface: &KernelValue,
    radiance: &KernelValue,
    medium: &KernelValue,
    ray_direction: [f32; 3],
    camera_position: [f32; 3],
    lighting: &crate::presentation_contract::PresentationLightingInputs,
) -> Result<[f32; 3], PresentationExecError> {
    if hit_flag(hit)? {
        let hit_position = hit_position(hit)?;
        let hit_normal = hit_world_normal(hit)?;
        let key_dir = normalize3(sub3(lighting.key_light.position, hit_position));
        let view_dir = normalize3(sub3(camera_position, hit_position));
        let half_dir = normalize3(add3(key_dir, view_dir));
        let distance_to_light = length3(sub3(lighting.key_light.position, hit_position));
        let attenuation =
            clamp01(1.0 - (distance_to_light / lighting.key_light.range.max(f32::EPSILON)));
        let ndotl = clamp01(dot3(hit_normal, key_dir));
        let ndoth = clamp01(dot3(hit_normal, half_dir));
        let diffuse = ndotl * attenuation;
        let fill =
            clamp01(dot3(hit_normal, normalize3(lighting.fill_direction))) * lighting.fill_strength;
        let surface = expect_struct(surface, "Surface")?;
        let albedo = expect_vec3(field(surface, "albedo")?)?;
        let roughness = clamp01(expect_f32(field(surface, "roughness")?)?);
        let metalness = clamp01(expect_f32(field(surface, "metalness")?)?);
        let clearcoat = clamp01(expect_f32(field(surface, "clearcoat")?)?);
        let emissive = expect_vec3(field(surface, "emissive")?)?;
        let spec_power = mix(48.0, 8.0, roughness);
        let spec_raw = ndoth.powf(spec_power);
        let specular_strength = 0.10 + (metalness * 0.25) + (clearcoat * 0.20);
        let highlight = spec_raw * specular_strength;
        let lighting_rgb = add3(
            lighting.ambient_color,
            [diffuse + fill, diffuse + fill, diffuse + fill],
        );
        let direct = clamp_vec3(
            add3(
                mul3(mul3_componentwise(albedo, lighting_rgb), 1.0)
                    .zip_map(lighting.key_light.intensity, |lane, intensity| {
                        lane * intensity
                    }),
                [highlight * 220.0, highlight * 208.0, highlight * 196.0],
            ),
            0.0,
            255.0,
        );
        let medium = expect_struct(medium, "Medium")?;
        let medium_density = expect_f32(field(medium, "density")?)?;
        let medium_emission = expect_vec3(field(medium, "emission")?)?;
        let fog_strength = clamp(medium_density * distance_to_light * 0.18, 0.0, 0.55);
        let radiance = expect_vec3(radiance)?;
        let fog_color = add3(medium_emission, mul3(radiance, 0.22));
        let radiance_lit = mul3(radiance, 0.25 + (highlight * 0.15));
        let lit = add3(add3(direct, emissive), radiance_lit);
        Ok(mix3(lit, fog_color, fog_strength))
    } else {
        let radiance = expect_vec3(radiance)?;
        let medium = expect_struct(medium, "Medium")?;
        let density = expect_f32(field(medium, "density")?)?;
        let emission = expect_vec3(field(medium, "emission")?)?;
        let miss_fog = clamp(density * 3.0, 0.0, 0.45);
        let miss_mix_color = add3(emission, mul3(radiance, 0.28));
        let _ = ray_direction;
        Ok(mix3(radiance, miss_mix_color, miss_fog))
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

trait ZipMap {
    fn zip_map(self, rhs: [f32; 3], f: impl Fn(f32, f32) -> f32) -> [f32; 3];
}

impl ZipMap for [f32; 3] {
    fn zip_map(self, rhs: [f32; 3], f: impl Fn(f32, f32) -> f32) -> [f32; 3] {
        [f(self[0], rhs[0]), f(self[1], rhs[1]), f(self[2], rhs[2])]
    }
}

fn add3(lhs: [f32; 3], rhs: [f32; 3]) -> [f32; 3] {
    [lhs[0] + rhs[0], lhs[1] + rhs[1], lhs[2] + rhs[2]]
}

fn sub3(lhs: [f32; 3], rhs: [f32; 3]) -> [f32; 3] {
    [lhs[0] - rhs[0], lhs[1] - rhs[1], lhs[2] - rhs[2]]
}

fn mul3(value: [f32; 3], scalar: f32) -> [f32; 3] {
    [value[0] * scalar, value[1] * scalar, value[2] * scalar]
}

fn mul3_componentwise(lhs: [f32; 3], rhs: [f32; 3]) -> [f32; 3] {
    [lhs[0] * rhs[0], lhs[1] * rhs[1], lhs[2] * rhs[2]]
}

fn dot3(lhs: [f32; 3], rhs: [f32; 3]) -> f32 {
    (lhs[0] * rhs[0]) + (lhs[1] * rhs[1]) + (lhs[2] * rhs[2])
}

fn length3(value: [f32; 3]) -> f32 {
    dot3(value, value).sqrt()
}

fn normalize3(value: [f32; 3]) -> [f32; 3] {
    let length = length3(value);
    if length <= f32::EPSILON {
        [0.0, 0.0, 0.0]
    } else {
        [value[0] / length, value[1] / length, value[2] / length]
    }
}

fn mix(lhs: f32, rhs: f32, t: f32) -> f32 {
    lhs * (1.0 - t) + rhs * t
}

fn mix3(lhs: [f32; 3], rhs: [f32; 3], t: f32) -> [f32; 3] {
    [
        mix(lhs[0], rhs[0], t),
        mix(lhs[1], rhs[1], t),
        mix(lhs[2], rhs[2], t),
    ]
}

fn clamp(value: f32, min: f32, max: f32) -> f32 {
    value.max(min).min(max)
}

fn clamp01(value: f32) -> f32 {
    clamp(value, 0.0, 1.0)
}

fn clamp_vec3(value: [f32; 3], min: f32, max: f32) -> [f32; 3] {
    [
        clamp(value[0], min, max),
        clamp(value[1], min, max),
        clamp(value[2], min, max),
    ]
}
