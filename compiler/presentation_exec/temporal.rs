use crate::kernel::{KernelStructValue, KernelValue};
use crate::presentation_contract::{CanonicalCameraInput, CanonicalViewportInput};
use crate::presentation_exec::{
    PresentationExecError, PresentationExecutionInput, expect_bool, expect_struct, expect_u32,
    expect_vec2, expect_vec3, field, frame_state_temporal_components,
};
use crate::presentation_plan::{
    MotionResolvePassContract, PresentationPlan, TemporalResolvePassContract,
};
use crate::query_exec::BatchQueryExecutionTrace;
use smol_str::SmolStr;

use super::resources::AttachmentResourceSet;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct ContinuationCounts {
    pub available: u32,
    pub consumed: u32,
    pub rejected: u32,
    pub unavailable: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct TemporalResolveInputSample {
    pub current_color: [f32; 3],
    pub history_color: [f32; 3],
    pub clamp_min: [f32; 3],
    pub clamp_max: [f32; 3],
    pub use_history: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HistoryVerdict {
    Available,
    Rejected,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct MotionSample {
    delta_pixels: [f32; 2],
    previous_sample: [f32; 2],
    valid: bool,
    disoccluded: bool,
}

pub(super) fn motion_resolve(
    plan: &PresentationPlan,
    input: &PresentationExecutionInput,
    attachments: &mut AttachmentResourceSet,
    screen_samples: &[KernelValue],
    primary_hits: &[KernelValue],
    contract: &MotionResolvePassContract,
) -> Result<ContinuationCounts, PresentationExecError> {
    let components = frame_state_temporal_components(&input.frame_state)?;
    let history_verdict = history_verdict(
        plan,
        input,
        components.frame_index,
        components.previous_frame_index,
    )?;
    let previous_hits = contract
        .history_primary_hit_attachment
        .as_ref()
        .map(|name| attachments.decode_attachment(name))
        .transpose()?;
    let Some(motion_attachment) = attachments.attachment_mut(contract.output_attachment.as_str())
    else {
        return Ok(ContinuationCounts::default());
    };
    let mut counts = ContinuationCounts::default();
    for (index, (sample, hit)) in screen_samples.iter().zip(primary_hits).enumerate() {
        let sample = expect_struct(sample, "ScreenSampleQuery")?;
        let current_pixel = expect_vec2(field(sample, "pixel")?)?;
        let motion = if hit_flag(hit)? {
            let previous_sample = project_to_previous_sample(
                components.previous_camera,
                components.previous_viewport,
                components.previous_jitter,
                hit_position(hit)?,
            );
            if matches!(history_verdict, HistoryVerdict::Available) {
                match previous_sample {
                    Some(previous_sample)
                        if sample_in_view(previous_sample, components.previous_viewport) =>
                    {
                        let previous_index =
                            previous_history_index(previous_sample, components.previous_viewport);
                        if previous_hits
                            .as_ref()
                            .and_then(|hits| hits.get(previous_index))
                            .is_some_and(|previous_hit| {
                                identities_match(hit, previous_hit).unwrap_or(false)
                            })
                        {
                            counts.available += 1;
                            MotionSample {
                                delta_pixels: [
                                    previous_sample[0] - current_pixel[0],
                                    previous_sample[1] - current_pixel[1],
                                ],
                                previous_sample,
                                valid: true,
                                disoccluded: false,
                            }
                        } else {
                            counts.rejected += 1;
                            MotionSample {
                                delta_pixels: [
                                    previous_sample[0] - current_pixel[0],
                                    previous_sample[1] - current_pixel[1],
                                ],
                                previous_sample,
                                valid: false,
                                disoccluded: true,
                            }
                        }
                    }
                    Some(previous_sample) => {
                        counts.rejected += 1;
                        MotionSample {
                            delta_pixels: [
                                previous_sample[0] - current_pixel[0],
                                previous_sample[1] - current_pixel[1],
                            ],
                            previous_sample,
                            valid: false,
                            disoccluded: true,
                        }
                    }
                    None => {
                        counts.rejected += 1;
                        MotionSample {
                            delta_pixels: [0.0, 0.0],
                            previous_sample: [0.0, 0.0],
                            valid: false,
                            disoccluded: true,
                        }
                    }
                }
            } else {
                match history_verdict {
                    HistoryVerdict::Rejected => counts.rejected += 1,
                    HistoryVerdict::Unavailable => counts.unavailable += 1,
                    HistoryVerdict::Available => {}
                }
                MotionSample {
                    delta_pixels: [0.0, 0.0],
                    previous_sample: previous_sample.unwrap_or([0.0, 0.0]),
                    valid: false,
                    disoccluded: false,
                }
            }
        } else {
            counts.unavailable += 1;
            MotionSample {
                delta_pixels: [0.0, 0.0],
                previous_sample: [0.0, 0.0],
                valid: false,
                disoccluded: false,
            }
        };
        motion_attachment.encode(index, &motion_value(motion))?;
    }
    Ok(counts)
}

pub(super) fn temporal_resolve_inputs(
    attachments: &AttachmentResourceSet,
    width: u32,
    height: u32,
    contract: &TemporalResolvePassContract,
) -> Result<Vec<TemporalResolveInputSample>, PresentationExecError> {
    let current_color = attachments.decode_attachment(contract.input_attachment.as_str())?;
    let history_color =
        attachments.decode_attachment(contract.history_color_attachment.as_str())?;
    let motion = attachments.decode_attachment(contract.motion_attachment.as_str())?;
    let mut inputs = Vec::with_capacity(current_color.len());
    for index in 0..current_color.len() {
        let current = color_value(&current_color[index])?;
        let motion = motion_sample(&motion[index])?;
        let (clamp_min, clamp_max) =
            neighborhood_bounds(&current_color, width as usize, height as usize, index)?;
        let history = if motion.valid {
            let sample_index = previous_history_index(
                motion.previous_sample,
                CanonicalViewportInput { width, height },
            );
            color_value(
                history_color
                    .get(sample_index)
                    .unwrap_or(&KernelValue::Vec3([0.0, 0.0, 0.0])),
            )?
        } else {
            [0.0, 0.0, 0.0]
        };
        inputs.push(TemporalResolveInputSample {
            current_color: current,
            history_color: history,
            clamp_min,
            clamp_max,
            use_history: motion.valid && !motion.disoccluded,
        });
    }
    Ok(inputs)
}

pub(super) fn temporal_resolve_cpu(
    attachments: &mut AttachmentResourceSet,
    width: u32,
    height: u32,
    contract: &TemporalResolvePassContract,
) -> Result<u32, PresentationExecError> {
    let inputs = temporal_resolve_inputs(attachments, width, height, contract)?;
    let consumed_count = temporal_consumed_count(&inputs);
    let primary_hits = attachments.decode_attachment(contract.primary_hit_attachment.as_str())?;
    let Some(output) = attachments.attachment_mut(contract.output_attachment.as_str()) else {
        return Ok(consumed_count);
    };
    for (index, input) in inputs.iter().enumerate() {
        output.encode(
            index,
            &KernelValue::Vec3(resolve_temporal_color(input, contract)),
        )?;
    }
    if let Some(history_color) =
        attachments.attachment_mut(contract.history_color_attachment.as_str())
    {
        for (index, input) in inputs.iter().enumerate() {
            history_color.encode(
                index,
                &KernelValue::Vec3(resolve_temporal_color(input, contract)),
            )?;
        }
    }
    if let Some(history_primary_hit_attachment) = &contract.history_primary_hit_attachment
        && let Some(history_primary_hit) =
            attachments.attachment_mut(history_primary_hit_attachment.as_str())
    {
        for (index, hit) in primary_hits.iter().enumerate() {
            history_primary_hit.encode(index, hit)?;
        }
    }
    Ok(consumed_count)
}

pub(super) fn temporal_resolve_kernel_values(
    attachments: &AttachmentResourceSet,
    width: u32,
    height: u32,
    contract: &TemporalResolvePassContract,
) -> Result<(Vec<KernelValue>, u32), PresentationExecError> {
    let inputs = temporal_resolve_inputs(attachments, width, height, contract)?;
    let consumed_count = temporal_consumed_count(&inputs);
    let values = inputs
        .into_iter()
        .map(|input| {
            KernelValue::Struct(KernelStructValue {
                name: SmolStr::new("TemporalResolveInput"),
                fields: vec![
                    (
                        SmolStr::new("current_color"),
                        KernelValue::Vec3(input.current_color),
                    ),
                    (
                        SmolStr::new("history_color"),
                        KernelValue::Vec3(input.history_color),
                    ),
                    (
                        SmolStr::new("clamp_min"),
                        KernelValue::Vec3(input.clamp_min),
                    ),
                    (
                        SmolStr::new("clamp_max"),
                        KernelValue::Vec3(input.clamp_max),
                    ),
                    (
                        SmolStr::new("use_history"),
                        KernelValue::Bool(input.use_history),
                    ),
                ],
            })
        })
        .collect::<Vec<_>>();
    Ok((values, consumed_count))
}

pub(super) fn update_query_trace_continuation(
    trace: &mut BatchQueryExecutionTrace,
    counts: ContinuationCounts,
) {
    trace.observability.solver_continuation_available += counts.available;
    trace.observability.solver_continuation_consumed += counts.consumed;
    trace.observability.solver_continuation_rejected += counts.rejected;
    trace.observability.solver_continuation_unavailable += counts.unavailable;
}

pub(super) fn resolve_temporal_color(
    input: &TemporalResolveInputSample,
    contract: &TemporalResolvePassContract,
) -> [f32; 3] {
    if !input.use_history {
        return input.current_color;
    }
    let clamped_history = [
        input.history_color[0].clamp(input.clamp_min[0], input.clamp_max[0]),
        input.history_color[1].clamp(input.clamp_min[1], input.clamp_max[1]),
        input.history_color[2].clamp(input.clamp_min[2], input.clamp_max[2]),
    ];
    let history_weight = contract.history_weight_numerator as f32
        / contract.history_weight_denominator.max(1) as f32;
    [
        (input.current_color[0] * (1.0 - history_weight)) + (clamped_history[0] * history_weight),
        (input.current_color[1] * (1.0 - history_weight)) + (clamped_history[1] * history_weight),
        (input.current_color[2] * (1.0 - history_weight)) + (clamped_history[2] * history_weight),
    ]
}

fn temporal_consumed_count(inputs: &[TemporalResolveInputSample]) -> u32 {
    inputs.iter().filter(|input| input.use_history).count() as u32
}

fn history_verdict(
    plan: &PresentationPlan,
    input: &PresentationExecutionInput,
    frame_index: u32,
    previous_frame_index: u32,
) -> Result<HistoryVerdict, PresentationExecError> {
    let Some(temporal) = &plan.frame.temporal else {
        return Ok(HistoryVerdict::Unavailable);
    };
    let Some(history) = &input.history else {
        return Ok(HistoryVerdict::Unavailable);
    };
    let frame = expect_struct(&input.frame_state, "FrameState")?;
    if expect_bool(field(frame, "history_reset")?)?
        && matches!(
            temporal.invalidation,
            crate::presentation_contract::TemporalInvalidationPolicy::CameraCut
                | crate::presentation_contract::TemporalInvalidationPolicy::CameraCutOrHistoryCompatibilityMismatch
                | crate::presentation_contract::TemporalInvalidationPolicy::CameraCutHistoryMismatchOrDisocclusion
        )
    {
        return Ok(HistoryVerdict::Rejected);
    }
    if matches!(
        temporal.validation,
        crate::presentation_contract::TemporalValidationStrictness::Strict
    ) && previous_frame_index != history.frame_index
    {
        return Ok(HistoryVerdict::Rejected);
    }
    let age = frame_index.saturating_sub(history.frame_index);
    for slot in &temporal.history_slots {
        if age > slot.max_age_frames {
            return Ok(HistoryVerdict::Rejected);
        }
        let Some(previous_slot) = history
            .slots
            .iter()
            .find(|previous| previous.slot == slot.slot)
        else {
            return Ok(HistoryVerdict::Rejected);
        };
        if previous_slot.compatibility != slot.compatibility
            || previous_slot.attachment != slot.attachment
        {
            return Ok(HistoryVerdict::Rejected);
        }
    }
    Ok(HistoryVerdict::Available)
}

fn identities_match(
    current: &KernelValue,
    previous: &KernelValue,
) -> Result<bool, PresentationExecError> {
    if !hit_flag(current)? || !hit_flag(previous)? {
        return Ok(false);
    }
    let current = expect_struct(current, "Hit3")?;
    let previous = expect_struct(previous, "Hit3")?;
    Ok(expect_u32(field(current, "root_shape_id")?)?
        == expect_u32(field(previous, "root_shape_id")?)?
        && expect_u32(field(current, "feature_id")?)?
            == expect_u32(field(previous, "feature_id")?)?
        && expect_u32(field(current, "instance_id")?)?
            == expect_u32(field(previous, "instance_id")?)?
        && expect_u32(field(current, "repeat_id")?)? == expect_u32(field(previous, "repeat_id")?)?)
}

fn hit_flag(hit: &KernelValue) -> Result<bool, PresentationExecError> {
    match field(expect_struct(hit, "Hit3")?, "hit")? {
        KernelValue::Bool(value) => Ok(*value),
        other => Err(PresentationExecError::TypeMismatch {
            expected: "Boolean".to_string(),
            found: format!("{other:?}"),
        }),
    }
}

fn hit_position(hit: &KernelValue) -> Result<[f32; 3], PresentationExecError> {
    expect_vec3(field(expect_struct(hit, "Hit3")?, "position")?)
}

fn project_to_previous_sample(
    camera: CanonicalCameraInput,
    viewport: CanonicalViewportInput,
    jitter: [f32; 2],
    point: [f32; 3],
) -> Option<[f32; 2]> {
    let forward = normalize_or(camera.forward, [0.0, 0.0, -1.0]);
    let right = normalize_or(cross3(forward, camera.up), [1.0, 0.0, 0.0]);
    let up = normalize_or(cross3(right, forward), [0.0, 1.0, 0.0]);
    let rel = sub3(point, camera.position);
    let depth = dot3(rel, forward);
    if depth <= 1.0e-4 {
        return None;
    }
    let width = viewport.width.max(1) as f32;
    let height = viewport.height.max(1) as f32;
    let aspect = width / height;
    let vertical_scale = (camera.vertical_fov_degrees.to_radians() * 0.5)
        .tan()
        .max(1.0e-4);
    let screen_x = dot3(rel, right) / (depth * aspect * vertical_scale);
    let screen_y = dot3(rel, up) / (depth * vertical_scale);
    let uv = [(screen_x + 1.0) * 0.5, (1.0 - screen_y) * 0.5];
    Some([
        (uv[0] * width) - 0.5 - jitter[0],
        (uv[1] * height) - 0.5 - jitter[1],
    ])
}

fn sample_in_view(sample: [f32; 2], viewport: CanonicalViewportInput) -> bool {
    sample[0] >= 0.0
        && sample[1] >= 0.0
        && sample[0] < viewport.width as f32
        && sample[1] < viewport.height as f32
}

fn previous_history_index(sample: [f32; 2], viewport: CanonicalViewportInput) -> usize {
    let x = sample[0]
        .round()
        .clamp(0.0, viewport.width.saturating_sub(1) as f32) as usize;
    let y = sample[1]
        .round()
        .clamp(0.0, viewport.height.saturating_sub(1) as f32) as usize;
    y * viewport.width as usize + x
}

fn motion_value(sample: MotionSample) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("MotionVector"),
        fields: vec![
            (
                SmolStr::new("delta_pixels"),
                KernelValue::Vec2(sample.delta_pixels),
            ),
            (
                SmolStr::new("previous_sample"),
                KernelValue::Vec2(sample.previous_sample),
            ),
            (SmolStr::new("valid"), KernelValue::Bool(sample.valid)),
            (
                SmolStr::new("disoccluded"),
                KernelValue::Bool(sample.disoccluded),
            ),
        ],
    })
}

fn motion_sample(value: &KernelValue) -> Result<MotionSample, PresentationExecError> {
    let value = expect_struct(value, "MotionVector")?;
    Ok(MotionSample {
        delta_pixels: expect_vec2(field(value, "delta_pixels")?)?,
        previous_sample: expect_vec2(field(value, "previous_sample")?)?,
        valid: expect_bool(field(value, "valid")?)?,
        disoccluded: expect_bool(field(value, "disoccluded")?)?,
    })
}

fn color_value(value: &KernelValue) -> Result<[f32; 3], PresentationExecError> {
    match value {
        KernelValue::Vec3(value) => Ok(*value),
        other => Err(PresentationExecError::TypeMismatch {
            expected: "Vec3".to_string(),
            found: format!("{other:?}"),
        }),
    }
}

fn neighborhood_bounds(
    colors: &[KernelValue],
    width: usize,
    height: usize,
    index: usize,
) -> Result<([f32; 3], [f32; 3]), PresentationExecError> {
    let x = index % width.max(1);
    let y = index / width.max(1);
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for sample_y in y.saturating_sub(1)..=(y + 1).min(height.saturating_sub(1)) {
        for sample_x in x.saturating_sub(1)..=(x + 1).min(width.saturating_sub(1)) {
            let color = color_value(&colors[sample_y * width + sample_x])?;
            for lane in 0..3 {
                min[lane] = min[lane].min(color[lane]);
                max[lane] = max[lane].max(color[lane]);
            }
        }
    }
    Ok((min, max))
}

fn dot3(lhs: [f32; 3], rhs: [f32; 3]) -> f32 {
    (lhs[0] * rhs[0]) + (lhs[1] * rhs[1]) + (lhs[2] * rhs[2])
}

fn sub3(lhs: [f32; 3], rhs: [f32; 3]) -> [f32; 3] {
    [lhs[0] - rhs[0], lhs[1] - rhs[1], lhs[2] - rhs[2]]
}

fn cross3(lhs: [f32; 3], rhs: [f32; 3]) -> [f32; 3] {
    [
        (lhs[1] * rhs[2]) - (lhs[2] * rhs[1]),
        (lhs[2] * rhs[0]) - (lhs[0] * rhs[2]),
        (lhs[0] * rhs[1]) - (lhs[1] * rhs[0]),
    ]
}

fn normalize_or(value: [f32; 3], fallback: [f32; 3]) -> [f32; 3] {
    let len_sq = dot3(value, value);
    if len_sq <= 1.0e-6 {
        fallback
    } else {
        let inv_len = len_sq.sqrt().recip();
        [value[0] * inv_len, value[1] * inv_len, value[2] * inv_len]
    }
}
