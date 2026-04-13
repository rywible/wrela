use crate::kernel::{KernelStructValue, KernelValue};
use crate::presentation_contract::TemporalChangeClass;
use crate::presentation_contract::{
    CanonicalCameraInput, CanonicalViewportInput, TemporalContract,
};
use crate::presentation_exec::{
    PresentationExecError, PresentationExecutionInput, expect_bool, expect_struct, expect_u32,
    expect_vec2, expect_vec3, field, frame_state_temporal_components,
};
use crate::presentation_plan::{
    MotionResolvePassContract, PresentationPlan, TemporalResolvePassContract,
};
use crate::query_exec::BatchQueryExecutionTrace;
use crate::semantic_evidence::FactAvailability;
use crate::state_advance::ChangeClass;
use smol_str::SmolStr;

use super::resources::AttachmentResourceSet;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct ContinuationCounts {
    pub available: u32,
    pub consumed: u32,
    pub rejected: u32,
    pub unavailable: u32,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ContinuationVerdict {
    Available,
    Rejected,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ContinuationRejectReason {
    NoHistory,
    HistoryReset,
    SnapshotEpochMismatch,
    SnapshotLineageMismatch,
    StrictFrameContinuityMismatch,
    AgeExceeded,
    SlotMismatch,
    ChangeCompatibilityMismatch,
    TemporalEvidenceMismatch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ContinuationAssessment {
    verdict: ContinuationVerdict,
    reason: Option<ContinuationRejectReason>,
    change_class: TemporalChangeClass,
    accepted_change_class: TemporalChangeClass,
    expected_previous_epoch: Option<u64>,
    history_epoch: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct TemporalResolveInputSample {
    pub current_color: [f32; 3],
    pub history_color: [f32; 3],
    pub clamp_min: [f32; 3],
    pub clamp_max: [f32; 3],
    pub use_history: bool,
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
    let assessment = history_assessment(plan, input, &components)?;
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
            if matches!(assessment.verdict, ContinuationVerdict::Available) {
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
                match assessment.verdict {
                    ContinuationVerdict::Rejected => counts.rejected += 1,
                    ContinuationVerdict::Unavailable => counts.unavailable += 1,
                    ContinuationVerdict::Available => {}
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
    counts
        .diagnostics
        .push(continuation_diagnostic(&assessment));
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

fn history_assessment(
    plan: &PresentationPlan,
    input: &PresentationExecutionInput,
    components: &crate::presentation_exec::FrameStateTemporalComponents,
) -> Result<ContinuationAssessment, PresentationExecError> {
    let Some(temporal) = &plan.frame.temporal else {
        return Ok(ContinuationAssessment {
            verdict: ContinuationVerdict::Unavailable,
            reason: Some(ContinuationRejectReason::NoHistory),
            change_class: TemporalChangeClass::Unknown,
            accepted_change_class: TemporalChangeClass::Unknown,
            expected_previous_epoch: None,
            history_epoch: None,
        });
    };
    let Some(history) = &input.history else {
        return Ok(ContinuationAssessment {
            verdict: ContinuationVerdict::Unavailable,
            reason: Some(ContinuationRejectReason::NoHistory),
            change_class: frame_change_class(components),
            accepted_change_class: temporal.change_class,
            expected_previous_epoch: Some(components.previous_snapshot_epoch.0),
            history_epoch: None,
        });
    };
    let frame = expect_struct(&input.frame_state, "FrameState")?;
    let change_class = frame_change_class(components);
    let expected_previous_epoch = Some(components.previous_snapshot_epoch.0);
    let history_epoch = Some(history.snapshot_handle.epoch().0);
    if expect_bool(field(frame, "history_reset")?)?
        && matches!(
            temporal.invalidation,
            crate::presentation_contract::TemporalInvalidationPolicy::CameraCut
                | crate::presentation_contract::TemporalInvalidationPolicy::CameraCutOrHistoryCompatibilityMismatch
                | crate::presentation_contract::TemporalInvalidationPolicy::CameraCutHistoryMismatchOrDisocclusion
        )
    {
        return Ok(ContinuationAssessment {
            verdict: ContinuationVerdict::Rejected,
            reason: Some(ContinuationRejectReason::HistoryReset),
            change_class,
            accepted_change_class: temporal.change_class,
            expected_previous_epoch,
            history_epoch,
        });
    }
    if history.snapshot_handle.epoch() != components.previous_snapshot_epoch {
        return Ok(ContinuationAssessment {
            verdict: ContinuationVerdict::Rejected,
            reason: Some(ContinuationRejectReason::SnapshotEpochMismatch),
            change_class,
            accepted_change_class: temporal.change_class,
            expected_previous_epoch,
            history_epoch,
        });
    }
    if temporal.requires_snapshot_lineage_match
        && history.snapshot_handle.root_entity().lineage_id()
            != input.region_snapshot.root_entity().lineage_id()
    {
        return Ok(ContinuationAssessment {
            verdict: ContinuationVerdict::Rejected,
            reason: Some(ContinuationRejectReason::SnapshotLineageMismatch),
            change_class,
            accepted_change_class: temporal.change_class,
            expected_previous_epoch,
            history_epoch,
        });
    }
    if matches!(
        temporal.validation,
        crate::presentation_contract::TemporalValidationStrictness::Strict
    ) && components.previous_presentation_frame != history.presentation_frame
    {
        return Ok(ContinuationAssessment {
            verdict: ContinuationVerdict::Rejected,
            reason: Some(ContinuationRejectReason::StrictFrameContinuityMismatch),
            change_class,
            accepted_change_class: temporal.change_class,
            expected_previous_epoch,
            history_epoch,
        });
    }
    let age = components
        .presentation_frame
        .saturating_sub(history.presentation_frame);
    for slot in &temporal.history_slots {
        if age > slot.max_age_frames {
            return Ok(ContinuationAssessment {
                verdict: ContinuationVerdict::Rejected,
                reason: Some(ContinuationRejectReason::AgeExceeded),
                change_class,
                accepted_change_class: temporal.change_class,
                expected_previous_epoch,
                history_epoch,
            });
        }
        let Some(previous_slot) = history
            .slots
            .iter()
            .find(|previous| previous.slot == slot.slot)
        else {
            return Ok(ContinuationAssessment {
                verdict: ContinuationVerdict::Rejected,
                reason: Some(ContinuationRejectReason::SlotMismatch),
                change_class,
                accepted_change_class: temporal.change_class,
                expected_previous_epoch,
                history_epoch,
            });
        };
        if previous_slot.compatibility != slot.compatibility
            || previous_slot.attachment != slot.attachment
        {
            return Ok(ContinuationAssessment {
                verdict: ContinuationVerdict::Rejected,
                reason: Some(ContinuationRejectReason::SlotMismatch),
                change_class,
                accepted_change_class: temporal.change_class,
                expected_previous_epoch,
                history_epoch,
            });
        }
        if previous_slot.reuse_key.snapshot_id != history.snapshot_handle.snapshot_id()
            || previous_slot.reuse_key.epoch != history.snapshot_handle.epoch()
        {
            return Ok(ContinuationAssessment {
                verdict: ContinuationVerdict::Rejected,
                reason: Some(ContinuationRejectReason::SlotMismatch),
                change_class,
                accepted_change_class: temporal.change_class,
                expected_previous_epoch,
                history_epoch,
            });
        }
    }
    if !temporal
        .transition_compatibility
        .allows(frame_change_budget_class(components))
    {
        return Ok(ContinuationAssessment {
            verdict: ContinuationVerdict::Rejected,
            reason: Some(ContinuationRejectReason::ChangeCompatibilityMismatch),
            change_class,
            accepted_change_class: temporal.change_class,
            expected_previous_epoch,
            history_epoch,
        });
    }
    if required_temporal_evidence_failure(temporal, components).is_some() {
        return Ok(ContinuationAssessment {
            verdict: ContinuationVerdict::Rejected,
            reason: Some(ContinuationRejectReason::TemporalEvidenceMismatch),
            change_class,
            accepted_change_class: temporal.change_class,
            expected_previous_epoch,
            history_epoch,
        });
    }
    Ok(ContinuationAssessment {
        verdict: ContinuationVerdict::Available,
        reason: None,
        change_class,
        accepted_change_class: temporal.change_class,
        expected_previous_epoch,
        history_epoch,
    })
}

pub(super) fn frame_change_class(
    components: &crate::presentation_exec::FrameStateTemporalComponents,
) -> TemporalChangeClass {
    if components.history_reset {
        TemporalChangeClass::HistoryReset
    } else if components.change_summary_present {
        if components.change_identity_changed {
            TemporalChangeClass::IdentityShift
        } else if components.change_topology_changed {
            TemporalChangeClass::TopologyShift
        } else {
            match components.change_class {
                0 => TemporalChangeClass::Stable,
                1 => TemporalChangeClass::CameraMotion,
                2 => TemporalChangeClass::ViewportShift,
                3 => TemporalChangeClass::TopologyShift,
                4 => TemporalChangeClass::IdentityShift,
                _ => TemporalChangeClass::Unknown,
            }
        }
    } else if components.previous_viewport != components.viewport {
        TemporalChangeClass::ViewportShift
    } else if components.previous_camera != components.camera
        || components.previous_jitter != components.jitter
    {
        TemporalChangeClass::CameraMotion
    } else {
        TemporalChangeClass::Stable
    }
}

pub(super) fn frame_change_budget_class(
    components: &crate::presentation_exec::FrameStateTemporalComponents,
) -> ChangeClass {
    if components.change_summary_present && !components.change_compatible {
        return ChangeClass::Incompatible;
    }
    match frame_change_class(components) {
        TemporalChangeClass::Stable => ChangeClass::None,
        TemporalChangeClass::CameraMotion => ChangeClass::Presentation,
        TemporalChangeClass::ViewportShift => ChangeClass::Structural,
        TemporalChangeClass::TopologyShift => ChangeClass::Topology,
        TemporalChangeClass::IdentityShift => ChangeClass::Identity,
        TemporalChangeClass::HistoryReset | TemporalChangeClass::Unknown => {
            ChangeClass::Incompatible
        }
    }
}

pub(super) fn required_temporal_evidence_failure(
    temporal: &TemporalContract,
    components: &crate::presentation_exec::FrameStateTemporalComponents,
) -> Option<&'static str> {
    let change_class = frame_change_class(components);
    if temporal.required_evidence.stationary == FactAvailability::Available
        && !matches!(change_class, TemporalChangeClass::Stable)
    {
        return Some("stationary");
    }
    if temporal.required_evidence.rigid_over_interval == FactAvailability::Available
        && matches!(
            change_class,
            TemporalChangeClass::TopologyShift
                | TemporalChangeClass::IdentityShift
                | TemporalChangeClass::HistoryReset
                | TemporalChangeClass::Unknown
        )
    {
        return Some("rigid-over-interval");
    }
    if temporal.required_evidence.topology_stable == FactAvailability::Available
        && (components.change_topology_changed
            || matches!(
                change_class,
                TemporalChangeClass::TopologyShift | TemporalChangeClass::IdentityShift
            ))
    {
        return Some("topology-stable");
    }
    if temporal.required_evidence.bounded_velocity == FactAvailability::Available
        && matches!(
            change_class,
            TemporalChangeClass::TopologyShift
                | TemporalChangeClass::IdentityShift
                | TemporalChangeClass::HistoryReset
                | TemporalChangeClass::Unknown
        )
    {
        return Some("bounded-velocity");
    }
    None
}

fn continuation_diagnostic(assessment: &ContinuationAssessment) -> String {
    let verdict = match assessment.verdict {
        ContinuationVerdict::Available => "available",
        ContinuationVerdict::Rejected => "rejected",
        ContinuationVerdict::Unavailable => "unavailable",
    };
    let reason = assessment
        .reason
        .map(continuation_reject_reason_name)
        .unwrap_or("none");
    format!(
        "continuation verdict={verdict} reason={reason} change_class={} accepted_change_class={} expected_previous_epoch={} history_epoch={}",
        temporal_change_class_name(assessment.change_class),
        temporal_change_class_name(assessment.accepted_change_class),
        assessment
            .expected_previous_epoch
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_string()),
        assessment
            .history_epoch
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_string()),
    )
}

fn continuation_reject_reason_name(reason: ContinuationRejectReason) -> &'static str {
    match reason {
        ContinuationRejectReason::NoHistory => "no-history",
        ContinuationRejectReason::HistoryReset => "history-reset",
        ContinuationRejectReason::SnapshotEpochMismatch => "snapshot-epoch-mismatch",
        ContinuationRejectReason::SnapshotLineageMismatch => "snapshot-lineage-mismatch",
        ContinuationRejectReason::StrictFrameContinuityMismatch => {
            "strict-frame-continuity-mismatch"
        }
        ContinuationRejectReason::AgeExceeded => "age-exceeded",
        ContinuationRejectReason::SlotMismatch => "slot-mismatch",
        ContinuationRejectReason::ChangeCompatibilityMismatch => "change-compatibility-mismatch",
        ContinuationRejectReason::TemporalEvidenceMismatch => "temporal-evidence-mismatch",
    }
}

fn temporal_change_class_name(value: TemporalChangeClass) -> &'static str {
    match value {
        TemporalChangeClass::Stable => "stable",
        TemporalChangeClass::CameraMotion => "camera-motion",
        TemporalChangeClass::ViewportShift => "viewport-shift",
        TemporalChangeClass::TopologyShift => "topology-shift",
        TemporalChangeClass::IdentityShift => "identity-shift",
        TemporalChangeClass::HistoryReset => "history-reset",
        TemporalChangeClass::Unknown => "unknown",
    }
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
