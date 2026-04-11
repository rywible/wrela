mod cpu;
pub mod debug;
pub mod resources;
mod temporal;
mod wgsl;

use crate::kernel::{KernelStructValue, KernelValue, lower_batch_query_plan};
use crate::presentation_contract::{
    CanonicalCameraInput, CanonicalLightInput, CanonicalRayBudget, CanonicalViewportInput,
    FrameContract, HistoryCompatibilityKey, LegacyCompatibilityProjectionInput,
    PresentationLightingInputs, canonical_screen_sample_query,
    legacy_preview_screen_sample_query,
};
use crate::presentation_plan::{PresentationPlan, PrimaryVisibilityPassContract};
use crate::query_exec::{
    BatchQueryExecutionTrace, QueryExecContext, QueryExecError, execute_batch_query_with_trace_on,
};
use crate::query_plan::{BatchQueryPlan, DispatchBackend};
use crate::query_solver::{RaySolverDiagnosticSummary, RaySolverMethod};
use resources::{
    AttachmentResourceSet, PresentationResourceError, allocate_attachment_resources_without_history,
};
use smol_str::SmolStr;
use thiserror::Error;

pub use resources::{
    AttachmentResource, FrameAttachmentLayout, PresentationResourceError as ResourceError,
    allocate_attachment_resources as allocate_frame_attachment_resources,
    allocate_attachment_resources_with_history as allocate_frame_attachment_resources_with_history,
    allocate_attachment_resources_without_history as allocate_frame_attachment_resources_without_history,
    attachment_element_abi, frame_attachment_layout,
};

#[derive(Debug, Error)]
pub enum PresentationExecError {
    #[error("presentation plan '{plan}' does not contain a screen-sample generation pass")]
    MissingScreenSamplePass { plan: SmolStr },
    #[error("presentation plan '{plan}' does not contain a primary-visibility pass")]
    MissingPrimaryVisibilityPass { plan: SmolStr },
    #[error("presentation execution expected {expected}, found {found}")]
    TypeMismatch { expected: String, found: String },
    #[error("missing field '{field}' on '{record}'")]
    MissingField { record: String, field: SmolStr },
    #[error("unsupported presentation plan: {message}")]
    UnsupportedPlan { message: String },
    #[error(transparent)]
    Query(#[from] QueryExecError),
    #[error(transparent)]
    Resource(#[from] PresentationResourceError),
}

#[derive(Debug, Clone, PartialEq)]
pub struct PresentationExecutionInput {
    pub region_capture: SmolStr,
    pub frame_domain: KernelValue,
    pub frame_state: KernelValue,
    pub history: Option<PresentationTemporalHistory>,
    pub lighting: PresentationLightingInputs,
    pub compatibility_projection: Option<LegacyCompatibilityProjectionInput>,
    pub ray_budget: CanonicalRayBudget,
    pub backend: DispatchBackend,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresentationTemporalHistorySlot {
    pub slot: u8,
    pub attachment: SmolStr,
    pub compatibility: HistoryCompatibilityKey,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PresentationTemporalHistory {
    pub frame_index: u32,
    pub attachments: AttachmentResourceSet,
    pub slots: Vec<PresentationTemporalHistorySlot>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresentationRayStepDistribution {
    pub zero: u32,
    pub short: u32,
    pub medium: u32,
    pub long: u32,
    pub extreme: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresentationMetrics {
    pub sample_count: u32,
    pub hit_count: u32,
    pub miss_count: u32,
    pub candidate_count: u32,
    pub candidates_before_pruning: u32,
    pub candidates_after_pruning: u32,
    pub candidate_reduction: u32,
    pub trace_steps_total: u32,
    pub trace_steps_max: u32,
    pub ray_step_distribution: PresentationRayStepDistribution,
    pub dispatch_items: u32,
    pub dispatch_workgroups: [u32; 3],
    pub solver_summary: Option<RaySolverDiagnosticSummary>,
    pub solver_methods: Vec<RaySolverMethod>,
    pub dense_fallback_count: u32,
    pub continuation_available_count: u32,
    pub continuation_consumed_count: u32,
    pub continuation_rejected_count: u32,
    pub continuation_unavailable_count: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PresentationExecutionResult {
    pub plan_name: SmolStr,
    pub backend: DispatchBackend,
    pub width: u32,
    pub height: u32,
    pub screen_samples: Vec<KernelValue>,
    pub attachments: AttachmentResourceSet,
    pub history: Option<PresentationTemporalHistory>,
    pub metrics: PresentationMetrics,
    pub query_trace: BatchQueryExecutionTrace,
}

pub fn execute_plan(
    ctx: &QueryExecContext,
    plan: &PresentationPlan,
    input: &PresentationExecutionInput,
) -> Result<PresentationExecutionResult, PresentationExecError> {
    match input.backend {
        DispatchBackend::Wgsl => wgsl::execute_plan(ctx, plan, input),
        DispatchBackend::Cpu | DispatchBackend::Auto | DispatchBackend::VirtualGpu => {
            cpu::execute_plan(ctx, plan, input)
        }
    }
}

pub fn frame_state_value(
    camera: CanonicalCameraInput,
    previous_camera: CanonicalCameraInput,
    viewport: CanonicalViewportInput,
    jitter_pixels: [f32; 2],
    frame_index: u32,
    delta_seconds: f32,
) -> KernelValue {
    frame_state_value_with_history(
        camera,
        previous_camera,
        viewport,
        viewport,
        jitter_pixels,
        jitter_pixels,
        frame_index,
        frame_index.saturating_sub(1),
        delta_seconds,
        frame_index == 0,
    )
}

pub fn frame_state_value_with_history(
    camera: CanonicalCameraInput,
    previous_camera: CanonicalCameraInput,
    viewport: CanonicalViewportInput,
    previous_viewport: CanonicalViewportInput,
    jitter_pixels: [f32; 2],
    previous_jitter_pixels: [f32; 2],
    frame_index: u32,
    previous_frame_index: u32,
    delta_seconds: f32,
    history_reset: bool,
) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("FrameState"),
        fields: vec![
            (
                SmolStr::new("view"),
                KernelValue::Struct(KernelStructValue {
                    name: SmolStr::new("ViewState"),
                    fields: vec![
                        (SmolStr::new("camera"), camera_value(camera)),
                        (
                            SmolStr::new("previous_camera"),
                            camera_value(previous_camera),
                        ),
                        (SmolStr::new("viewport"), viewport_value(viewport)),
                        (
                            SmolStr::new("previous_viewport"),
                            viewport_value(previous_viewport),
                        ),
                        (SmolStr::new("jitter"), KernelValue::Vec2(jitter_pixels)),
                        (
                            SmolStr::new("previous_jitter"),
                            KernelValue::Vec2(previous_jitter_pixels),
                        ),
                    ],
                }),
            ),
            (SmolStr::new("frame_index"), KernelValue::U32(frame_index)),
            (
                SmolStr::new("previous_frame_index"),
                KernelValue::U32(previous_frame_index),
            ),
            (
                SmolStr::new("delta_seconds"),
                KernelValue::F32(delta_seconds),
            ),
            (
                SmolStr::new("history_reset"),
                KernelValue::Bool(history_reset),
            ),
        ],
    })
}

pub fn scene_domain_value(
    scene_id: u32,
    geometry_detail: i32,
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
                    fields: vec![
                        (
                            SmolStr::new("geometry_detail"),
                            KernelValue::I32(geometry_detail),
                        ),
                        (SmolStr::new("guarantee"), KernelValue::U32(0)),
                    ],
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

pub fn light_value(light: CanonicalLightInput) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("Light"),
        fields: vec![
            (SmolStr::new("position"), KernelValue::Vec3(light.position)),
            (
                SmolStr::new("direction"),
                KernelValue::Vec3(light.direction),
            ),
            (
                SmolStr::new("intensity"),
                KernelValue::Vec3(light.intensity),
            ),
            (SmolStr::new("range"), KernelValue::F32(light.range)),
        ],
    })
}

pub fn lighting_inputs_value(lighting: PresentationLightingInputs) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("PresentationLightingInputs"),
        fields: vec![
            (SmolStr::new("key_light"), light_value(lighting.key_light)),
            (
                SmolStr::new("fill_direction"),
                KernelValue::Vec3(lighting.fill_direction),
            ),
            (
                SmolStr::new("fill_strength"),
                KernelValue::F32(lighting.fill_strength),
            ),
            (
                SmolStr::new("ambient_color"),
                KernelValue::Vec3(lighting.ambient_color),
            ),
        ],
    })
}

fn execute_batch_contract(
    ctx: &QueryExecContext,
    backend: DispatchBackend,
    contract_id: crate::query_contract::QueryContractId,
    args: &[KernelValue],
) -> Result<(Vec<KernelValue>, BatchQueryExecutionTrace), PresentationExecError> {
    let batch_plan = lower_batch_query_plan(
        &BatchQueryPlan::for_contract(contract_id, backend, None).map_err(|message| {
            PresentationExecError::UnsupportedPlan {
                message: message.to_string(),
            }
        })?,
    );
    let (values, trace) = execute_batch_query_with_trace_on(ctx, backend, &batch_plan, args)?;
    Ok((expect_array(&values)?.to_vec(), trace))
}

fn point_query_value(point: [f32; 3]) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("PointQuery"),
        fields: vec![(SmolStr::new("point"), KernelValue::Vec3(point))],
    })
}

fn point_direction_query_value(point: [f32; 3], direction: [f32; 3]) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("PointDirectionQuery"),
        fields: vec![
            (SmolStr::new("point"), KernelValue::Vec3(point)),
            (SmolStr::new("direction"), KernelValue::Vec3(direction)),
        ],
    })
}

fn allocate_execution_attachments(
    frame: &FrameContract,
    width: u32,
    height: u32,
    history: Option<&PresentationTemporalHistory>,
) -> Result<AttachmentResourceSet, PresentationExecError> {
    if let Some(history) = history {
        match crate::presentation_exec::allocate_frame_attachment_resources_with_history(
            frame,
            width,
            height,
            Some(&history.attachments),
        ) {
            Ok(resources) => return Ok(resources),
            Err(
                PresentationResourceError::MissingHistoryAttachment { .. }
                | PresentationResourceError::HistoryLayoutMismatch { .. },
            ) => {}
            Err(err) => return Err(PresentationExecError::Resource(err)),
        }
    }
    allocate_attachment_resources_without_history(frame, width, height)
        .map_err(PresentationExecError::Resource)
}

fn build_temporal_history(
    plan: &PresentationPlan,
    frame_state: &KernelValue,
    attachments: &AttachmentResourceSet,
) -> Result<Option<PresentationTemporalHistory>, PresentationExecError> {
    let Some(temporal) = &plan.frame.temporal else {
        return Ok(None);
    };
    let frame = expect_struct(frame_state, "FrameState")?;
    let frame_index = expect_u32(field(frame, "frame_index")?)?;
    Ok(Some(PresentationTemporalHistory {
        frame_index,
        attachments: attachments.clone(),
        slots: temporal
            .history_slots
            .iter()
            .map(|slot| PresentationTemporalHistorySlot {
                slot: slot.slot,
                attachment: slot.attachment.clone(),
                compatibility: slot.compatibility.clone(),
            })
            .collect(),
    }))
}

fn presentation_metrics(
    hits: &[KernelValue],
    query_trace: &BatchQueryExecutionTrace,
    solver_summary: Option<RaySolverDiagnosticSummary>,
) -> PresentationMetrics {
    let mut distribution = PresentationRayStepDistribution {
        zero: 0,
        short: 0,
        medium: 0,
        long: 0,
        extreme: 0,
    };
    let mut trace_steps_total = 0;
    for hit in hits {
        let steps = hit_steps(hit).unwrap_or_default();
        trace_steps_total += steps;
        match steps {
            0 => distribution.zero += 1,
            1..=8 => distribution.short += 1,
            9..=32 => distribution.medium += 1,
            33..=64 => distribution.long += 1,
            _ => distribution.extreme += 1,
        }
    }
    let observability = &query_trace.observability;
    PresentationMetrics {
        sample_count: hits.len() as u32,
        hit_count: observability.hit_count,
        miss_count: observability.miss_count,
        candidate_count: observability.candidate_count,
        candidates_before_pruning: observability.candidates_before_pruning,
        candidates_after_pruning: observability.candidates_after_pruning,
        candidate_reduction: observability
            .candidates_before_pruning
            .saturating_sub(observability.candidates_after_pruning),
        trace_steps_total,
        trace_steps_max: observability.trace_steps_max,
        ray_step_distribution: distribution,
        dispatch_items: observability.dispatch_items,
        dispatch_workgroups: [
            observability.dispatch_workgroups_x,
            observability.dispatch_workgroups_y,
            observability.dispatch_workgroups_z,
        ],
        solver_summary,
        solver_methods: observability.solver_methods.clone(),
        dense_fallback_count: observability.solver_dense_fallback_rays
            + observability.solver_generated_dense_fallback_rays,
        continuation_available_count: observability.solver_continuation_available,
        continuation_consumed_count: observability.solver_continuation_consumed,
        continuation_rejected_count: observability.solver_continuation_rejected,
        continuation_unavailable_count: observability.solver_continuation_unavailable,
    }
}

fn materialize_primary_visibility_attachments(
    attachments: &mut AttachmentResourceSet,
    hits: &[KernelValue],
    contract: &PrimaryVisibilityPassContract,
) -> Result<(), PresentationExecError> {
    for (index, hit) in hits.iter().enumerate() {
        let attachment_hit = normalize_hit_for_attachment(hit)?;
        if let Some(primary_hit) =
            attachments.attachment_mut(contract.primary_hit_attachment.as_str())
        {
            primary_hit.encode(index, &attachment_hit)?;
        }
        if let Some(depth_attachment) = &contract.depth_attachment
            && let Some(depth) = attachments.attachment_mut(depth_attachment.as_str())
        {
            depth.encode(
                index,
                &KernelValue::F32(hit_depth(&attachment_hit).unwrap_or(f32::INFINITY)),
            )?;
        }
        if let Some(world_normal_attachment) = &contract.world_normal_attachment
            && let Some(world_normal) = attachments.attachment_mut(world_normal_attachment.as_str())
        {
            world_normal.encode(
                index,
                &KernelValue::Vec3(hit_world_normal(&attachment_hit).unwrap_or([0.0, 0.0, 0.0])),
            )?;
        }
    }
    Ok(())
}

fn generate_screen_samples(
    plan: &PresentationPlan,
    input: &PresentationExecutionInput,
    camera: CanonicalCameraInput,
    viewport: CanonicalViewportInput,
    jitter_pixels: [f32; 2],
    ray_budget: CanonicalRayBudget,
) -> Vec<KernelValue> {
    let mut samples = Vec::with_capacity(viewport.width.saturating_mul(viewport.height) as usize);
    for y in 0..viewport.height {
        for x in 0..viewport.width {
            let sample = if plan.view.compatibility_projection.legacy_path_active {
                legacy_preview_screen_sample_query(
                    camera,
                    viewport,
                    x,
                    y,
                    jitter_pixels,
                    ray_budget,
                    input
                        .compatibility_projection
                        .unwrap_or(LegacyCompatibilityProjectionInput {
                            world_up: camera.up,
                            view_scale: 0.72,
                        }),
                )
            } else {
                canonical_screen_sample_query(camera, viewport, x, y, jitter_pixels, ray_budget)
            };
            samples.push(KernelValue::Struct(KernelStructValue {
                name: SmolStr::new("ScreenSampleQuery"),
                fields: vec![
                    (SmolStr::new("pixel"), KernelValue::Vec2(sample.pixel)),
                    (SmolStr::new("uv"), KernelValue::Vec2(sample.uv)),
                    (
                        SmolStr::new("ray"),
                        KernelValue::Struct(KernelStructValue {
                            name: SmolStr::new("RayQuery"),
                            fields: vec![
                                (SmolStr::new("origin"), KernelValue::Vec3(sample.ray.origin)),
                                (
                                    SmolStr::new("direction"),
                                    KernelValue::Vec3(sample.ray.direction),
                                ),
                                (
                                    SmolStr::new("max_distance"),
                                    KernelValue::F32(sample.ray.max_distance),
                                ),
                                (
                                    SmolStr::new("min_step"),
                                    KernelValue::F32(sample.ray.min_step),
                                ),
                                (
                                    SmolStr::new("hit_epsilon"),
                                    KernelValue::F32(sample.ray.hit_epsilon),
                                ),
                                (
                                    SmolStr::new("max_steps"),
                                    KernelValue::I32(sample.ray.max_steps),
                                ),
                            ],
                        }),
                    ),
                ],
            }));
        }
    }
    samples
}

fn screen_sample_ray(sample: &KernelValue) -> Result<KernelValue, PresentationExecError> {
    let sample = expect_struct(sample, "ScreenSampleQuery")?;
    Ok(field(sample, "ray")?.clone())
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct FrameStateTemporalComponents {
    pub camera: CanonicalCameraInput,
    pub previous_camera: CanonicalCameraInput,
    pub viewport: CanonicalViewportInput,
    pub previous_viewport: CanonicalViewportInput,
    pub jitter: [f32; 2],
    pub previous_jitter: [f32; 2],
    pub frame_index: u32,
    pub previous_frame_index: u32,
    pub delta_seconds: f32,
    pub history_reset: bool,
}

fn frame_state_components(
    frame_state: &KernelValue,
) -> Result<(CanonicalCameraInput, CanonicalViewportInput, [f32; 2]), PresentationExecError> {
    let components = frame_state_temporal_components(frame_state)?;
    Ok((components.camera, components.viewport, components.jitter))
}

pub(super) fn frame_state_temporal_components(
    frame_state: &KernelValue,
) -> Result<FrameStateTemporalComponents, PresentationExecError> {
    let frame = expect_struct(frame_state, "FrameState")?;
    let view = expect_struct(field(frame, "view")?, "ViewState")?;
    let camera = expect_struct(field(view, "camera")?, "Camera")?;
    let previous_camera = expect_struct(field(view, "previous_camera")?, "Camera")?;
    let viewport = expect_struct(field(view, "viewport")?, "Viewport")?;
    let previous_viewport = expect_struct(field(view, "previous_viewport")?, "Viewport")?;
    let jitter = expect_vec2(field(view, "jitter")?)?;
    let previous_jitter = expect_vec2(field(view, "previous_jitter")?)?;
    Ok(FrameStateTemporalComponents {
        camera: CanonicalCameraInput {
            position: expect_vec3(field(camera, "position")?)?,
            forward: expect_vec3(field(camera, "forward")?)?,
            up: expect_vec3(field(camera, "up")?)?,
            vertical_fov_degrees: expect_f32(field(camera, "vertical_fov_degrees")?)?,
        },
        previous_camera: CanonicalCameraInput {
            position: expect_vec3(field(previous_camera, "position")?)?,
            forward: expect_vec3(field(previous_camera, "forward")?)?,
            up: expect_vec3(field(previous_camera, "up")?)?,
            vertical_fov_degrees: expect_f32(field(previous_camera, "vertical_fov_degrees")?)?,
        },
        viewport: CanonicalViewportInput {
            width: expect_u32(field(viewport, "width")?)?,
            height: expect_u32(field(viewport, "height")?)?,
        },
        previous_viewport: CanonicalViewportInput {
            width: expect_u32(field(previous_viewport, "width")?)?,
            height: expect_u32(field(previous_viewport, "height")?)?,
        },
        jitter,
        previous_jitter,
        frame_index: expect_u32(field(frame, "frame_index")?)?,
        previous_frame_index: expect_u32(field(frame, "previous_frame_index")?)?,
        delta_seconds: expect_f32(field(frame, "delta_seconds")?)?,
        history_reset: expect_bool(field(frame, "history_reset")?)?,
    })
}

fn camera_value(camera: CanonicalCameraInput) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("Camera"),
        fields: vec![
            (SmolStr::new("position"), KernelValue::Vec3(camera.position)),
            (SmolStr::new("forward"), KernelValue::Vec3(camera.forward)),
            (SmolStr::new("up"), KernelValue::Vec3(camera.up)),
            (
                SmolStr::new("vertical_fov_degrees"),
                KernelValue::F32(camera.vertical_fov_degrees),
            ),
        ],
    })
}

fn viewport_value(viewport: CanonicalViewportInput) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("Viewport"),
        fields: vec![
            (SmolStr::new("width"), KernelValue::U32(viewport.width)),
            (SmolStr::new("height"), KernelValue::U32(viewport.height)),
        ],
    })
}

fn expect_array(value: &KernelValue) -> Result<&[KernelValue], PresentationExecError> {
    match value {
        KernelValue::Array(values) => Ok(values),
        other => Err(type_mismatch("Array", other)),
    }
}

fn expect_struct<'a>(
    value: &'a KernelValue,
    expected: &str,
) -> Result<&'a KernelStructValue, PresentationExecError> {
    match value {
        KernelValue::Struct(struct_value) if struct_value.name == expected => Ok(struct_value),
        KernelValue::Struct(struct_value) => Err(PresentationExecError::TypeMismatch {
            expected: expected.to_string(),
            found: struct_value.name.to_string(),
        }),
        other => Err(type_mismatch(expected, other)),
    }
}

fn field<'a>(
    struct_value: &'a KernelStructValue,
    name: &str,
) -> Result<&'a KernelValue, PresentationExecError> {
    struct_value
        .fields
        .iter()
        .find_map(|(field_name, value)| (field_name == name).then_some(value))
        .ok_or_else(|| PresentationExecError::MissingField {
            record: struct_value.name.to_string(),
            field: SmolStr::new(name),
        })
}

fn expect_vec2(value: &KernelValue) -> Result<[f32; 2], PresentationExecError> {
    match value {
        KernelValue::Vec2(value) => Ok(*value),
        other => Err(type_mismatch("Vec2", other)),
    }
}

fn expect_vec3(value: &KernelValue) -> Result<[f32; 3], PresentationExecError> {
    match value {
        KernelValue::Vec3(value) => Ok(*value),
        other => Err(type_mismatch("Vec3", other)),
    }
}

fn expect_f32(value: &KernelValue) -> Result<f32, PresentationExecError> {
    match value {
        KernelValue::F32(value) => Ok(*value),
        other => Err(type_mismatch("F32", other)),
    }
}

fn expect_u32(value: &KernelValue) -> Result<u32, PresentationExecError> {
    match value {
        KernelValue::U32(value) => Ok(*value),
        other => Err(type_mismatch("U32", other)),
    }
}

fn expect_bool(value: &KernelValue) -> Result<bool, PresentationExecError> {
    match value {
        KernelValue::Bool(value) => Ok(*value),
        other => Err(type_mismatch("Boolean", other)),
    }
}

fn hit_depth(hit: &KernelValue) -> Result<f32, PresentationExecError> {
    let hit = expect_struct(hit, "Hit3")?;
    let did_hit = match field(hit, "hit")? {
        KernelValue::Bool(value) => *value,
        other => return Err(type_mismatch("Boolean", other)),
    };
    if did_hit {
        expect_f32(field(hit, "distance")?)
    } else {
        Ok(f32::INFINITY)
    }
}

fn hit_world_normal(hit: &KernelValue) -> Result<[f32; 3], PresentationExecError> {
    let hit = expect_struct(hit, "Hit3")?;
    let did_hit = match field(hit, "hit")? {
        KernelValue::Bool(value) => *value,
        other => return Err(type_mismatch("Boolean", other)),
    };
    if did_hit {
        expect_vec3(field(hit, "normal")?)
    } else {
        Ok([0.0, 0.0, 0.0])
    }
}

fn hit_steps(hit: &KernelValue) -> Result<u32, PresentationExecError> {
    let hit = expect_struct(hit, "Hit3")?;
    match field(hit, "steps")? {
        KernelValue::I32(value) => Ok((*value).max(0) as u32),
        other => Err(type_mismatch("I32", other)),
    }
}

fn normalize_hit_for_attachment(hit: &KernelValue) -> Result<KernelValue, PresentationExecError> {
    let hit = expect_struct(hit, "Hit3")?;
    let mut fields = hit.fields.clone();
    if let Some((_, payload)) = fields.iter_mut().find(|(name, _)| name == "payload")
        && matches!(payload, KernelValue::Nothing)
    {
        *payload = default_payload_value();
    }
    Ok(KernelValue::Struct(KernelStructValue {
        name: hit.name.clone(),
        fields,
    }))
}

fn default_payload_value() -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("Payload"),
        fields: vec![
            (SmolStr::new("entity_id"), KernelValue::U32(0)),
            (SmolStr::new("material_id"), KernelValue::U32(0)),
            (
                SmolStr::new("actor"),
                KernelValue::Struct(KernelStructValue {
                    name: SmolStr::new("ActorHandle"),
                    fields: vec![
                        (SmolStr::new("id"), KernelValue::U32(0)),
                        (SmolStr::new("generation"), KernelValue::U32(0)),
                    ],
                }),
            ),
        ],
    })
}

fn type_mismatch(expected: &str, found: &KernelValue) -> PresentationExecError {
    PresentationExecError::TypeMismatch {
        expected: expected.to_string(),
        found: kernel_value_kind(found).to_string(),
    }
}

fn kernel_value_kind(value: &KernelValue) -> &'static str {
    match value {
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
}
