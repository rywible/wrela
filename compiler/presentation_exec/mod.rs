pub mod controller;
pub mod cost;
mod cpu;
pub mod debug;
pub mod resources;
mod temporal;
mod wgsl;

use crate::kernel::{KernelStructValue, KernelValue, lower_batch_query_plan};
use crate::presentation_contract::{
    AttachmentLifetime, AttachmentResolutionClass, AttachmentResolutionScale, CanonicalCameraInput,
    CanonicalLightInput, CanonicalRayBudget, CanonicalViewportInput, FrameAttachmentContract,
    FrameAttachmentKind, FrameContract, HistoryCompatibilityKey,
    LegacyCompatibilityProjectionInput, PresentationLightingInputs, RealtimeQualityContract,
    RealtimeQualityState, canonical_screen_sample_query, legacy_preview_screen_sample_query,
};
use crate::presentation_plan::{PresentationPlan, PrimaryVisibilityPassContract};
use crate::query_exec::cpu::DirectQueryEvaluator;
use crate::query_exec::{
    BatchQueryExecutionTrace, QueryExecContext, QueryExecError, execute_batch_query_with_trace_on,
};
use crate::query_plan::{BatchQueryPlan, DispatchBackend};
use crate::query_solver::{RaySolverDiagnosticSummary, RaySolverMethod, ray_solver_method_name};
use resources::{
    AttachmentResourceSet, PresentationResourceError, allocate_attachment_resources_without_history,
};
use smol_str::SmolStr;
use std::collections::BTreeMap;
use thiserror::Error;

pub use self::controller::AdaptivePresentationController;
pub use cost::{
    PresentationAttachmentBytes, PresentationFrameCostReport, PresentationPassCost,
    PresentationQualityReport, quality_report, radiance_mode_name, render_frame_cost_report,
};
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
    pub quality_override: Option<RealtimeQualityState>,
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
    pub frame_cost: PresentationFrameCostReport,
    pub query_trace: BatchQueryExecutionTrace,
}

#[derive(Debug, Clone)]
pub struct AdaptivePresentationSession {
    controller: AdaptivePresentationController,
    history: Option<PresentationTemporalHistory>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct PassRuntimeStats {
    pub pass_id: String,
    pub pass_kind: String,
    pub work_items: u32,
    pub elapsed_micros: u128,
    pub dispatch_count: u32,
    pub attachment_bytes_read: u64,
    pub attachment_bytes_written: u64,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct TileCullingStats {
    pub total_tiles: u32,
    pub active_tiles: u32,
    pub skipped_samples: u32,
}

#[derive(Debug, Clone)]
pub(crate) struct ParticipantQueryWorkItem {
    pub target_index: usize,
    pub point_query: KernelValue,
    pub point_direction_query: KernelValue,
}

#[derive(Debug, Clone)]
pub(crate) struct TileCullingMask {
    pub active_samples: Vec<usize>,
    pub stats: TileCullingStats,
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

pub fn resolved_quality_state(
    plan: &PresentationPlan,
    input: &PresentationExecutionInput,
) -> RealtimeQualityState {
    input
        .quality_override
        .clone()
        .unwrap_or_else(|| plan.frame.quality.initial_state())
}

impl AdaptivePresentationSession {
    pub fn new(contract: RealtimeQualityContract) -> Self {
        Self {
            controller: AdaptivePresentationController::new(contract),
            history: None,
        }
    }

    pub fn with_window(mut self, moving_average_window: usize) -> Self {
        self.controller = self.controller.clone().with_window(moving_average_window);
        self
    }

    pub fn controller(&self) -> &AdaptivePresentationController {
        &self.controller
    }

    pub fn history(&self) -> Option<&PresentationTemporalHistory> {
        self.history.as_ref()
    }

    pub fn execute_frame(
        &mut self,
        ctx: &QueryExecContext,
        plan: &PresentationPlan,
        input: &PresentationExecutionInput,
    ) -> Result<PresentationExecutionResult, PresentationExecError> {
        let mut frame_input = input.clone();
        frame_input.history = self.history.clone();
        frame_input.quality_override = Some(self.controller.quality().clone());
        let result = execute_plan(ctx, plan, &frame_input)?;
        self.history = result.history.clone();
        let _ = self.controller.observe_frame(&result.frame_cost);
        Ok(result)
    }
}

pub(crate) fn effective_plan_for_quality(
    plan: &PresentationPlan,
    quality: &RealtimeQualityState,
) -> PresentationPlan {
    let mut effective = plan.clone();
    effective.apply_participant_policy(quality.radiance_enabled(), quality.media_enabled);
    let internal_divisor = internal_resolution_divisor(quality.internal_resolution_scale);
    if internal_divisor > 1 {
        for attachment in &mut effective.frame.outputs {
            if matches!(
                attachment.kind,
                FrameAttachmentKind::Surface
                    | FrameAttachmentKind::Radiance
                    | FrameAttachmentKind::Medium
            ) {
                apply_attachment_divisor(attachment, internal_divisor);
            }
        }
    }
    if quality.half_res_participants {
        for attachment in &mut effective.frame.outputs {
            if matches!(
                attachment.kind,
                FrameAttachmentKind::Radiance | FrameAttachmentKind::Medium
            ) {
                apply_attachment_divisor(attachment, 2);
            }
        }
    }
    effective
}

pub(crate) fn adjusted_ray_budget(
    budget: CanonicalRayBudget,
    quality: &RealtimeQualityState,
) -> CanonicalRayBudget {
    CanonicalRayBudget {
        max_steps: budget.max_steps.min(quality.primary_max_steps),
        ..budget
    }
}

pub(crate) fn full_attachment_byte_size(attachments: &AttachmentResourceSet, name: &str) -> u64 {
    attachments
        .attachment(name)
        .map(|attachment| attachment.bytes.len() as u64)
        .unwrap_or_default()
}

pub(crate) fn attachment_byte_reports(
    attachments: &AttachmentResourceSet,
) -> Vec<PresentationAttachmentBytes> {
    attachments
        .attachments
        .iter()
        .map(|(name, attachment)| PresentationAttachmentBytes {
            attachment: name.to_string(),
            width: attachment.layout.width,
            height: attachment.layout.height,
            total_size_bytes: attachment.bytes.len() as u64,
        })
        .collect()
}

pub(crate) fn encode_values_at_indices(
    attachments: &mut AttachmentResourceSet,
    name: &str,
    indices: &[usize],
    values: &[KernelValue],
) -> Result<(), PresentationExecError> {
    let Some(attachment) = attachments.attachment_mut(name) else {
        return Ok(());
    };
    for (index, value) in indices.iter().zip(values) {
        attachment.encode(*index, value)?;
    }
    Ok(())
}

pub(crate) fn shade_lookup_value(
    attachments: &AttachmentResourceSet,
    name: &str,
    full_index: usize,
    fallback: &KernelValue,
) -> Result<KernelValue, PresentationExecError> {
    let Some(attachment) = attachments.attachment(name) else {
        return Ok(fallback.clone());
    };
    if attachment.layout.width == attachments.width
        && attachment.layout.height == attachments.height
    {
        return attachment
            .decode(full_index)
            .map_err(PresentationExecError::Resource);
    }
    let x = (full_index as u32) % attachments.width.max(1);
    let y = (full_index as u32) / attachments.width.max(1);
    let scaled_x = x / attachment.layout.attachment.scale.divisor_x.max(1);
    let scaled_y = y / attachment.layout.attachment.scale.divisor_y.max(1);
    let scaled_index = (scaled_y * attachment.layout.width + scaled_x) as usize;
    attachment
        .decode(scaled_index)
        .map_err(PresentationExecError::Resource)
}

pub(crate) fn participant_query_work_items(
    input: &PresentationExecutionInput,
    screen_samples: &[KernelValue],
    hits: &[KernelValue],
    attachments: &AttachmentResourceSet,
    attachment_name: &str,
    miss_sample_distance: f32,
    include_misses: bool,
) -> Result<Vec<ParticipantQueryWorkItem>, PresentationExecError> {
    let frame = expect_struct(&input.frame_state, "FrameState")?;
    let view = expect_struct(field(frame, "view")?, "ViewState")?;
    let camera = expect_struct(field(view, "camera")?, "Camera")?;
    let camera_position = expect_vec3(field(camera, "position")?)?;
    let Some(attachment) = attachments.attachment(attachment_name) else {
        return Ok(Vec::new());
    };
    let scaled = attachment.layout.width != attachments.width
        || attachment.layout.height != attachments.height;
    let mut items = Vec::new();
    let mut scaled_cells = BTreeMap::new();
    for (index, (sample, hit)) in screen_samples.iter().zip(hits).enumerate() {
        let is_hit = hit_flag(hit)?;
        if !include_misses && !is_hit {
            continue;
        }
        let ray = expect_struct(
            field(expect_struct(sample, "ScreenSampleQuery")?, "ray")?,
            "RayQuery",
        )?;
        let ray_direction = expect_vec3(field(ray, "direction")?)?;
        let point = if is_hit {
            hit_position(hit)?
        } else {
            [
                camera_position[0] + ray_direction[0] * miss_sample_distance,
                camera_position[1] + ray_direction[1] * miss_sample_distance,
                camera_position[2] + ray_direction[2] * miss_sample_distance,
            ]
        };
        let target_index = attachment_target_index(attachments, attachment, index);
        let item = ParticipantQueryWorkItem {
            target_index,
            point_query: point_query_value(point),
            point_direction_query: point_direction_query_value(point, ray_direction),
        };
        if scaled {
            scaled_cells.entry(target_index).or_insert(item);
        } else {
            items.push(item);
        }
    }
    if scaled {
        items.extend(scaled_cells.into_values());
    }
    Ok(items)
}

pub(crate) fn tile_culling_mask(
    ctx: &QueryExecContext,
    input: &PresentationExecutionInput,
    camera: CanonicalCameraInput,
    viewport: CanonicalViewportInput,
    legacy_projection: bool,
) -> Result<Option<TileCullingMask>, PresentationExecError> {
    if legacy_projection {
        return Ok(None);
    }
    let evaluator = DirectQueryEvaluator::new(ctx);
    let detail = frame_domain_geometry_detail(&input.frame_domain).unwrap_or(0);
    let bounds = evaluator.region_shape_support_bounds(&input.region_capture, detail)?;
    if bounds.is_empty() {
        return Ok(None);
    }
    let tile_size = 8u32;
    let tiles_x = viewport.width.div_ceil(tile_size);
    let tiles_y = viewport.height.div_ceil(tile_size);
    let mut active = vec![false; (tiles_x * tiles_y) as usize];
    for (_, min, max) in bounds {
        if !mark_projected_bounds_tiles(
            &mut active,
            tiles_x,
            tiles_y,
            tile_size,
            camera,
            viewport,
            min,
            max,
        ) {
            return Ok(None);
        }
    }
    let mut active_samples = Vec::new();
    let mut skipped_samples = Vec::new();
    for y in 0..viewport.height {
        for x in 0..viewport.width {
            let tile_x = x / tile_size;
            let tile_y = y / tile_size;
            let tile_index = (tile_y * tiles_x + tile_x) as usize;
            let sample_index = (y * viewport.width + x) as usize;
            if active.get(tile_index).copied().unwrap_or(true) {
                active_samples.push(sample_index);
            } else {
                skipped_samples.push(sample_index);
            }
        }
    }
    let active_tiles = active.iter().filter(|tile| **tile).count() as u32;
    Ok(Some(TileCullingMask {
        active_samples,
        stats: TileCullingStats {
            total_tiles: tiles_x * tiles_y,
            active_tiles,
            skipped_samples: skipped_samples.len() as u32,
        },
    }))
}

fn mark_projected_bounds_tiles(
    active: &mut [bool],
    tiles_x: u32,
    tiles_y: u32,
    tile_size: u32,
    camera: CanonicalCameraInput,
    viewport: CanonicalViewportInput,
    min: [f32; 3],
    max: [f32; 3],
) -> bool {
    let corners = [
        [min[0], min[1], min[2]],
        [min[0], min[1], max[2]],
        [min[0], max[1], min[2]],
        [min[0], max[1], max[2]],
        [max[0], min[1], min[2]],
        [max[0], min[1], max[2]],
        [max[0], max[1], min[2]],
        [max[0], max[1], max[2]],
    ];
    let forward = normalize3(camera.forward, [0.0, 0.0, -1.0]);
    let right = normalize3(cross3(forward, camera.up), [1.0, 0.0, 0.0]);
    let up = normalize3(cross3(right, forward), [0.0, 1.0, 0.0]);
    let aspect = viewport.width.max(1) as f32 / viewport.height.max(1) as f32;
    let vertical_scale = (camera.vertical_fov_degrees.to_radians() * 0.5).tan();
    let horizontal_scale = aspect * vertical_scale;
    let mut projected = Vec::new();
    for corner in corners {
        let rel = [
            corner[0] - camera.position[0],
            corner[1] - camera.position[1],
            corner[2] - camera.position[2],
        ];
        let depth = rel[0] * forward[0] + rel[1] * forward[1] + rel[2] * forward[2];
        if depth <= 0.0 {
            return false;
        }
        let x = (rel[0] * right[0] + rel[1] * right[1] + rel[2] * right[2])
            / (depth * horizontal_scale);
        let y = (rel[0] * up[0] + rel[1] * up[1] + rel[2] * up[2]) / (depth * vertical_scale);
        projected.push([x, y]);
    }
    let min_ndc_x = projected.iter().map(|p| p[0]).fold(f32::INFINITY, f32::min);
    let max_ndc_x = projected
        .iter()
        .map(|p| p[0])
        .fold(f32::NEG_INFINITY, f32::max);
    let min_ndc_y = projected.iter().map(|p| p[1]).fold(f32::INFINITY, f32::min);
    let max_ndc_y = projected
        .iter()
        .map(|p| p[1])
        .fold(f32::NEG_INFINITY, f32::max);
    if min_ndc_x > 1.0 || max_ndc_x < -1.0 || min_ndc_y > 1.0 || max_ndc_y < -1.0 {
        return true;
    }
    let min_px = (((min_ndc_x.clamp(-1.0, 1.0) + 1.0) * 0.5) * viewport.width as f32)
        .floor()
        .max(0.0) as u32;
    let max_px = (((max_ndc_x.clamp(-1.0, 1.0) + 1.0) * 0.5) * viewport.width as f32).ceil() as u32;
    let min_py = (((1.0 - max_ndc_y.clamp(-1.0, 1.0)) * 0.5) * viewport.height as f32)
        .floor()
        .max(0.0) as u32;
    let max_py =
        (((1.0 - min_ndc_y.clamp(-1.0, 1.0)) * 0.5) * viewport.height as f32).ceil() as u32;
    let min_tile_x = (min_px / tile_size).min(tiles_x.saturating_sub(1));
    let max_tile_x = (max_px.div_ceil(tile_size)).min(tiles_x).saturating_sub(1);
    let min_tile_y = (min_py / tile_size).min(tiles_y.saturating_sub(1));
    let max_tile_y = (max_py.div_ceil(tile_size)).min(tiles_y).saturating_sub(1);
    for tile_y in min_tile_y..=max_tile_y {
        for tile_x in min_tile_x..=max_tile_x {
            let index = (tile_y * tiles_x + tile_x) as usize;
            if let Some(slot) = active.get_mut(index) {
                *slot = true;
            }
        }
    }
    true
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
    let hit_count = hits
        .iter()
        .filter(|hit| hit_flag(hit).unwrap_or(false))
        .count() as u32;
    let miss_count = hits.len() as u32 - hit_count;
    PresentationMetrics {
        sample_count: hits.len() as u32,
        hit_count,
        miss_count,
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

pub(crate) fn build_frame_cost_report(
    width: u32,
    height: u32,
    quality: &RealtimeQualityState,
    metrics: &PresentationMetrics,
    tile_cull: TileCullingStats,
    surface_resolve_count: u32,
    participant_resolve_count: u32,
    attachments: &AttachmentResourceSet,
    passes: Vec<PassRuntimeStats>,
    mut active_acceleration_artifacts: Vec<String>,
) -> PresentationFrameCostReport {
    let radiance_mode = quality.radiance_mode;
    let half_res_participants = quality.half_res_participants;
    let hit_compaction_enabled = quality.hit_compaction_enabled;
    let active_degradations_empty = quality.active_degradations.is_empty();
    let quality = quality_report(quality, width, height);
    let primary_hit_rate = if metrics.sample_count == 0 {
        0.0
    } else {
        metrics.hit_count as f32 / metrics.sample_count as f32
    };
    let average_trace_steps = if metrics.sample_count == 0 {
        0.0
    } else {
        metrics.trace_steps_total as f32 / metrics.sample_count as f32
    };
    let support_prune_effectiveness = if metrics.candidates_before_pruning == 0 {
        0.0
    } else {
        metrics.candidate_reduction as f32 / metrics.candidates_before_pruning as f32
    };
    let tile_cull_efficiency = if tile_cull.total_tiles == 0 {
        0.0
    } else {
        1.0 - (tile_cull.active_tiles as f32 / tile_cull.total_tiles as f32)
    };
    let history_reuse_total = metrics.continuation_available_count
        + metrics.continuation_consumed_count
        + metrics.continuation_rejected_count
        + metrics.continuation_unavailable_count;
    let history_reuse_rate = if history_reuse_total == 0 {
        0.0
    } else {
        metrics.continuation_consumed_count as f32 / history_reuse_total as f32
    };
    if metrics.candidate_reduction > 0 {
        active_acceleration_artifacts.push("support_pruning".to_string());
    }
    if hit_compaction_enabled {
        active_acceleration_artifacts.push("hit_compaction".to_string());
    }
    if quality.internal_resolution_scale < 1.0 {
        active_acceleration_artifacts.push("dynamic_resolution".to_string());
    }
    if tile_cull.total_tiles > 0 && tile_cull.active_tiles < tile_cull.total_tiles {
        active_acceleration_artifacts.push("view_tile_culling".to_string());
    }
    if half_res_participants {
        active_acceleration_artifacts.push("half_res_participants".to_string());
    }
    if radiance_mode == crate::presentation_contract::RealtimeRadianceMode::Reduced {
        active_acceleration_artifacts.push("reduced_radiance_queries".to_string());
    }
    active_acceleration_artifacts.extend(
        metrics
            .solver_methods
            .iter()
            .map(|method| ray_solver_method_name(*method).to_string()),
    );
    let mut deduped_artifacts = BTreeMap::new();
    for artifact in active_acceleration_artifacts {
        deduped_artifacts
            .entry(artifact.clone())
            .or_insert(artifact);
    }
    let passes = passes
        .into_iter()
        .map(|pass| PresentationPassCost {
            pass_id: pass.pass_id,
            pass_kind: pass.pass_kind,
            work_items: pass.work_items,
            elapsed_micros: pass.elapsed_micros,
            dispatch_count: pass.dispatch_count,
            attachment_bytes_read: pass.attachment_bytes_read,
            attachment_bytes_written: pass.attachment_bytes_written,
            notes: pass.notes,
        })
        .collect::<Vec<_>>();
    let bottleneck_pass = passes
        .iter()
        .max_by_key(|pass| (pass.elapsed_micros, pass.work_items))
        .map(|pass| pass.pass_id.clone());
    let mut performance_gain_sources = Vec::new();
    if support_prune_effectiveness > 0.0 || tile_cull_efficiency > 0.0 {
        performance_gain_sources.push("less_semantic_work".to_string());
    }
    if !active_degradations_empty {
        performance_gain_sources.push("quality_degradation".to_string());
    }
    if performance_gain_sources.is_empty() {
        performance_gain_sources.push("backend_speed".to_string());
    }
    PresentationFrameCostReport {
        output_width: width,
        output_height: height,
        internal_width: quality.internal_width,
        internal_height: quality.internal_height,
        quality,
        primary_hit_rate,
        average_trace_steps,
        max_trace_steps: metrics.trace_steps_max,
        candidate_count_before_pruning: metrics.candidates_before_pruning,
        candidate_count_after_pruning: metrics.candidates_after_pruning,
        support_prune_effectiveness,
        tile_cull_total_tiles: tile_cull.total_tiles,
        tile_cull_active_tiles: tile_cull.active_tiles,
        tile_cull_efficiency,
        surface_resolve_count,
        participant_resolve_count,
        history_reuse_rate,
        attachment_bytes: attachment_byte_reports(attachments),
        passes,
        active_acceleration_artifacts: deduped_artifacts.into_values().collect(),
        bottleneck_pass,
        performance_gain_sources,
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

pub(crate) fn internal_resolution_divisor(scale: f32) -> u32 {
    if scale <= 0.25 + f32::EPSILON {
        4
    } else if scale <= 0.5 + f32::EPSILON {
        2
    } else {
        1
    }
}

pub(crate) fn internal_resolution_viewport(
    viewport: CanonicalViewportInput,
    quality: &RealtimeQualityState,
) -> CanonicalViewportInput {
    let divisor = internal_resolution_divisor(quality.internal_resolution_scale);
    CanonicalViewportInput {
        width: viewport.width.div_ceil(divisor),
        height: viewport.height.div_ceil(divisor),
    }
}

pub(crate) fn expand_internal_hits(
    internal_hits: &[KernelValue],
    output_viewport: CanonicalViewportInput,
    internal_viewport: CanonicalViewportInput,
) -> Vec<KernelValue> {
    if output_viewport == internal_viewport {
        return internal_hits.to_vec();
    }
    let mut hits =
        Vec::with_capacity(output_viewport.width.saturating_mul(output_viewport.height) as usize);
    for index in 0..output_viewport.width.saturating_mul(output_viewport.height) as usize {
        let x = index as u32 % output_viewport.width.max(1);
        let y = index as u32 / output_viewport.width.max(1);
        let internal_x = (x.saturating_mul(internal_viewport.width)) / output_viewport.width.max(1);
        let internal_y =
            (y.saturating_mul(internal_viewport.height)) / output_viewport.height.max(1);
        let internal_index = (internal_y * internal_viewport.width + internal_x) as usize;
        hits.push(
            internal_hits
                .get(internal_index)
                .cloned()
                .unwrap_or_else(primary_hit_miss_value),
        );
    }
    hits
}

pub(crate) fn attachment_hit_work_items(
    attachments: &AttachmentResourceSet,
    attachment_name: &str,
    hits: &[KernelValue],
    compact_hits: bool,
) -> Result<Vec<(usize, KernelValue)>, PresentationExecError> {
    let Some(attachment) = attachments.attachment(attachment_name) else {
        return Ok(Vec::new());
    };
    if !compact_hits
        && attachment.layout.width == attachments.width
        && attachment.layout.height == attachments.height
    {
        return Ok(hits.iter().cloned().enumerate().collect());
    }
    let mut deduped = BTreeMap::new();
    for (index, hit) in hits.iter().enumerate() {
        if compact_hits && !hit_flag(hit)? {
            continue;
        }
        let target_index = attachment_target_index(attachments, attachment, index);
        deduped.entry(target_index).or_insert_with(|| hit.clone());
    }
    Ok(deduped.into_iter().collect())
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

fn frame_domain_geometry_detail(frame_domain: &KernelValue) -> Result<i32, PresentationExecError> {
    let frame_domain = expect_struct(frame_domain, "SceneDomain")?;
    let spatial = expect_struct(field(frame_domain, "spatial")?, "SpatialDomainContract")?;
    match field(spatial, "geometry_detail")? {
        KernelValue::I32(value) => Ok(*value),
        other => Err(type_mismatch("I32", other)),
    }
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

fn hit_flag(value: &KernelValue) -> Result<bool, PresentationExecError> {
    let hit = expect_struct(value, "Hit3")?;
    expect_bool(field(hit, "hit")?)
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

fn hit_position(hit: &KernelValue) -> Result<[f32; 3], PresentationExecError> {
    let hit = expect_struct(hit, "Hit3")?;
    expect_vec3(field(hit, "position")?)
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

pub(crate) fn primary_hit_miss_value() -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("Hit3"),
        fields: vec![
            (SmolStr::new("hit"), KernelValue::Bool(false)),
            (SmolStr::new("distance"), KernelValue::F32(f32::INFINITY)),
            (SmolStr::new("position"), KernelValue::Vec3([0.0, 0.0, 0.0])),
            (SmolStr::new("normal"), KernelValue::Vec3([0.0, 0.0, 0.0])),
            (
                SmolStr::new("local_position"),
                KernelValue::Vec3([0.0, 0.0, 0.0]),
            ),
            (
                SmolStr::new("local_normal"),
                KernelValue::Vec3([0.0, 0.0, 0.0]),
            ),
            (
                SmolStr::new("shading_frame"),
                KernelValue::Struct(KernelStructValue {
                    name: SmolStr::new("Transform3"),
                    fields: vec![
                        (
                            SmolStr::new("matrix"),
                            KernelValue::Mat4([
                                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0,
                                0.0, 0.0, 1.0,
                            ]),
                        ),
                        (
                            SmolStr::new("inverse"),
                            KernelValue::Mat4([
                                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0,
                                0.0, 0.0, 1.0,
                            ]),
                        ),
                    ],
                }),
            ),
            (SmolStr::new("steps"), KernelValue::I32(0)),
            (SmolStr::new("feature_id"), KernelValue::U32(0)),
            (SmolStr::new("instance_id"), KernelValue::U32(0)),
            (SmolStr::new("repeat_id"), KernelValue::U32(0)),
            (SmolStr::new("root_shape_id"), KernelValue::U32(0)),
            (SmolStr::new("payload"), default_payload_value()),
        ],
    })
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

fn attachment_target_index(
    attachments: &AttachmentResourceSet,
    attachment: &crate::presentation_exec::resources::AttachmentResource,
    full_index: usize,
) -> usize {
    if attachment.layout.width == attachments.width
        && attachment.layout.height == attachments.height
    {
        return full_index;
    }
    let x = (full_index as u32) % attachments.width.max(1);
    let y = (full_index as u32) / attachments.width.max(1);
    let scaled_x = x / attachment.layout.attachment.scale.divisor_x.max(1);
    let scaled_y = y / attachment.layout.attachment.scale.divisor_y.max(1);
    (scaled_y * attachment.layout.width + scaled_x) as usize
}

fn apply_attachment_divisor(attachment: &mut FrameAttachmentContract, requested_divisor: u32) {
    if matches!(attachment.lifetime, AttachmentLifetime::HistorySlot(_)) {
        return;
    }
    let combined_divisor = attachment
        .scale
        .divisor_x
        .max(attachment.scale.divisor_y)
        .saturating_mul(requested_divisor)
        .clamp(1, 4);
    attachment.scale = match combined_divisor {
        4 => AttachmentResolutionScale::quarter(),
        2 => AttachmentResolutionScale::half(),
        _ => AttachmentResolutionScale::full(),
    };
    attachment.resolution = match combined_divisor {
        4 => AttachmentResolutionClass::QuarterViewport,
        2 => AttachmentResolutionClass::HalfViewport,
        _ => AttachmentResolutionClass::Viewport,
    };
}

fn cross3(lhs: [f32; 3], rhs: [f32; 3]) -> [f32; 3] {
    [
        lhs[1] * rhs[2] - lhs[2] * rhs[1],
        lhs[2] * rhs[0] - lhs[0] * rhs[2],
        lhs[0] * rhs[1] - lhs[1] * rhs[0],
    ]
}

fn dot3(lhs: [f32; 3], rhs: [f32; 3]) -> f32 {
    lhs[0] * rhs[0] + lhs[1] * rhs[1] + lhs[2] * rhs[2]
}

fn normalize3(value: [f32; 3], fallback: [f32; 3]) -> [f32; 3] {
    let len_sq = dot3(value, value);
    if len_sq <= f32::EPSILON {
        fallback
    } else {
        let inv_len = len_sq.sqrt().recip();
        [value[0] * inv_len, value[1] * inv_len, value[2] * inv_len]
    }
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
