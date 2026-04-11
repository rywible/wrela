use crate::kernel::KernelValue;
use crate::presentation_contract::{
    AttachmentElementSchema, FrameAttachmentContract, FrameAttachmentKind,
};
use crate::presentation_exec::{
    PresentationExecError, PresentationExecutionResult, render_frame_cost_report,
};
use serde_json::{Map, Value, json};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresentationDebugArtifacts {
    pub color_ppm: Option<PathBuf>,
    pub depth_ppm: Option<PathBuf>,
    pub world_normal_ppm: Option<PathBuf>,
    pub stats_path: PathBuf,
}

pub fn export_frame_debug(
    result: &PresentationExecutionResult,
    out_dir: &Path,
) -> Result<PresentationDebugArtifacts, PresentationExecError> {
    fs::create_dir_all(out_dir).map_err(|err| PresentationExecError::UnsupportedPlan {
        message: err.to_string(),
    })?;
    let color_ppm = out_dir.join("color.ppm");
    let depth_ppm = out_dir.join("depth.ppm");
    let world_normal_ppm = out_dir.join("world_normal.ppm");
    let stats_path = out_dir.join("stats.txt");
    let color_ppm =
        write_attachment_ppm_if_present(result, FrameAttachmentKind::Color, &color_ppm)?;
    let depth_ppm =
        write_attachment_ppm_if_present(result, FrameAttachmentKind::Depth, &depth_ppm)?;
    let world_normal_ppm = write_attachment_ppm_if_present(
        result,
        FrameAttachmentKind::WorldNormal,
        &world_normal_ppm,
    )?;
    fs::write(&stats_path, render_primary_visibility_stats(result)).map_err(|err| {
        PresentationExecError::UnsupportedPlan {
            message: err.to_string(),
        }
    })?;
    Ok(PresentationDebugArtifacts {
        color_ppm,
        depth_ppm,
        world_normal_ppm,
        stats_path,
    })
}

pub fn export_primary_visibility_debug(
    result: &PresentationExecutionResult,
    out_dir: &Path,
) -> Result<PresentationDebugArtifacts, PresentationExecError> {
    export_frame_debug(result, out_dir)
}

fn write_attachment_ppm_if_present(
    result: &PresentationExecutionResult,
    kind: FrameAttachmentKind,
    path: &Path,
) -> Result<Option<PathBuf>, PresentationExecError> {
    let Some(attachment_name) = attachment_name_for_kind_optional(result, kind) else {
        remove_stale_debug_artifact(path)?;
        return Ok(None);
    };
    let data = render_attachment_ppm_string(result, attachment_name)?;
    fs::write(path, data).map_err(|err| PresentationExecError::UnsupportedPlan {
        message: err.to_string(),
    })?;
    Ok(Some(path.to_path_buf()))
}

fn remove_stale_debug_artifact(path: &Path) -> Result<(), PresentationExecError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(PresentationExecError::UnsupportedPlan {
            message: err.to_string(),
        }),
    }
}

pub fn render_primary_visibility_stats(result: &PresentationExecutionResult) -> String {
    let metrics = &result.metrics;
    let hit_rate = if metrics.sample_count == 0 {
        0.0
    } else {
        metrics.hit_count as f32 / metrics.sample_count as f32
    };
    let miss_rate = if metrics.sample_count == 0 {
        0.0
    } else {
        metrics.miss_count as f32 / metrics.sample_count as f32
    };
    let avg_steps = if metrics.sample_count == 0 {
        0.0
    } else {
        metrics.trace_steps_total as f32 / metrics.sample_count as f32
    };
    let solver = metrics
        .solver_summary
        .as_ref()
        .map(|summary| {
            format!(
                "{} methods={} fallback={:?}",
                summary.plan_id,
                summary
                    .methods
                    .iter()
                    .map(|method| format!("{method:?}"))
                    .collect::<Vec<_>>()
                    .join(","),
                summary.fallback
            )
        })
        .unwrap_or_else(|| "none".to_string());
    let mut out = format!(
        "backend={:?}\nresolution={}x{}\nsamples={}\nhits={}\nmisses={}\nhit_rate={:.3}\nmiss_rate={:.3}\ncandidates_before_pruning={}\ncandidates_after_pruning={}\ncandidate_reduction={}\ntrace_steps_avg={:.3}\ntrace_steps_max={}\nray_steps_zero={}\nray_steps_short={}\nray_steps_medium={}\nray_steps_long={}\nray_steps_extreme={}\ndispatch_items={}\ndispatch_workgroups={},{},{}\ndense_fallback_count={}\nsolver={}\n",
        result.backend,
        result.width,
        result.height,
        metrics.sample_count,
        metrics.hit_count,
        metrics.miss_count,
        hit_rate,
        miss_rate,
        metrics.candidates_before_pruning,
        metrics.candidates_after_pruning,
        metrics.candidate_reduction,
        avg_steps,
        metrics.trace_steps_max,
        metrics.ray_step_distribution.zero,
        metrics.ray_step_distribution.short,
        metrics.ray_step_distribution.medium,
        metrics.ray_step_distribution.long,
        metrics.ray_step_distribution.extreme,
        metrics.dispatch_items,
        metrics.dispatch_workgroups[0],
        metrics.dispatch_workgroups[1],
        metrics.dispatch_workgroups[2],
        metrics.dense_fallback_count,
        solver,
    );
    out.push('\n');
    out.push_str(&render_frame_cost_report(&result.frame_cost));
    out
}

pub fn write_depth_ppm(
    result: &PresentationExecutionResult,
    path: &Path,
) -> Result<(), PresentationExecError> {
    let depth_name = attachment_name_for_kind(result, FrameAttachmentKind::Depth)?;
    write_named_depth_ppm(result, depth_name, path)
}

pub fn write_named_depth_ppm(
    result: &PresentationExecutionResult,
    attachment_name: &str,
    path: &Path,
) -> Result<(), PresentationExecError> {
    let data = render_depth_ppm_string(result, attachment_name)?;
    fs::write(path, data).map_err(|err| PresentationExecError::UnsupportedPlan {
        message: err.to_string(),
    })
}

pub fn render_depth_ppm_string(
    result: &PresentationExecutionResult,
    attachment_name: &str,
) -> Result<String, PresentationExecError> {
    let depths = result
        .attachments
        .decode_attachment(attachment_name)?
        .into_iter()
        .map(|value| match value {
            KernelValue::F32(depth) => depth,
            _ => f32::INFINITY,
        })
        .collect::<Vec<_>>();
    let finite = depths
        .iter()
        .copied()
        .filter(|value| value.is_finite())
        .collect::<Vec<_>>();
    let (min_depth, max_depth) = finite
        .iter()
        .copied()
        .fold((f32::INFINITY, f32::NEG_INFINITY), |(min, max), value| {
            (min.min(value), max.max(value))
        });
    let pixels = depths
        .into_iter()
        .map(|depth| {
            if !depth.is_finite() || !min_depth.is_finite() {
                [0u8, 0u8, 0u8]
            } else {
                let normalized = if (max_depth - min_depth).abs() <= f32::EPSILON {
                    1.0
                } else {
                    1.0 - ((depth - min_depth) / (max_depth - min_depth)).clamp(0.0, 1.0)
                };
                let value = (normalized * 255.0).round() as u8;
                [value, value, value]
            }
        })
        .collect::<Vec<_>>();
    Ok(render_ppm_string(result.width, result.height, &pixels))
}

pub fn write_color_ppm(
    result: &PresentationExecutionResult,
    path: &Path,
) -> Result<(), PresentationExecError> {
    let data = render_color_ppm_string(result)?;
    fs::write(path, data).map_err(|err| PresentationExecError::UnsupportedPlan {
        message: err.to_string(),
    })
}

pub fn render_color_ppm_string(
    result: &PresentationExecutionResult,
) -> Result<String, PresentationExecError> {
    let color_name = attachment_name_for_kind(result, FrameAttachmentKind::Color)?;
    render_named_color_ppm_string(result, color_name)
}

pub fn render_named_color_ppm_string(
    result: &PresentationExecutionResult,
    attachment_name: &str,
) -> Result<String, PresentationExecError> {
    let pixels = result
        .attachments
        .decode_attachment(attachment_name)?
        .into_iter()
        .map(|value| match value {
            KernelValue::Vec3(color) => [
                encode_color_lane(color[0]),
                encode_color_lane(color[1]),
                encode_color_lane(color[2]),
            ],
            _ => [0u8, 0u8, 0u8],
        })
        .collect::<Vec<_>>();
    Ok(render_ppm_string(result.width, result.height, &pixels))
}

pub fn write_world_normal_ppm(
    result: &PresentationExecutionResult,
    path: &Path,
) -> Result<(), PresentationExecError> {
    let world_normal_name = attachment_name_for_kind(result, FrameAttachmentKind::WorldNormal)?;
    write_named_world_normal_ppm(result, world_normal_name, path)
}

pub fn write_named_world_normal_ppm(
    result: &PresentationExecutionResult,
    attachment_name: &str,
    path: &Path,
) -> Result<(), PresentationExecError> {
    let data = render_world_normal_ppm_string(result, attachment_name)?;
    fs::write(path, data).map_err(|err| PresentationExecError::UnsupportedPlan {
        message: err.to_string(),
    })
}

pub fn render_world_normal_ppm_string(
    result: &PresentationExecutionResult,
    attachment_name: &str,
) -> Result<String, PresentationExecError> {
    let pixels = result
        .attachments
        .decode_attachment(attachment_name)?
        .into_iter()
        .map(|value| match value {
            KernelValue::Vec3(normal) => normal,
            _ => [0.0, 0.0, 0.0],
        })
        .map(|normal| {
            [
                encode_normal_lane(normal[0]),
                encode_normal_lane(normal[1]),
                encode_normal_lane(normal[2]),
            ]
        })
        .collect::<Vec<_>>();
    Ok(render_ppm_string(result.width, result.height, &pixels))
}

pub fn render_attachment_ppm_string(
    result: &PresentationExecutionResult,
    attachment_name: &str,
) -> Result<String, PresentationExecError> {
    let attachment = result
        .attachments
        .attachment(attachment_name)
        .ok_or_else(|| PresentationExecError::UnsupportedPlan {
            message: format!("missing attachment '{attachment_name}'"),
        })?;
    match attachment.layout.attachment.kind {
        FrameAttachmentKind::Color => render_named_color_ppm_string(result, attachment_name),
        FrameAttachmentKind::Depth => render_depth_ppm_string(result, attachment_name),
        FrameAttachmentKind::WorldNormal => render_world_normal_ppm_string(result, attachment_name),
        kind => Err(PresentationExecError::UnsupportedPlan {
            message: format!(
                "attachment '{attachment_name}' of kind {:?} does not have a PPM export",
                kind
            ),
        }),
    }
}

pub fn attachment_json(
    result: &PresentationExecutionResult,
    attachment_name: &str,
) -> Result<Value, PresentationExecError> {
    let attachment = result
        .attachments
        .attachment(attachment_name)
        .ok_or_else(|| PresentationExecError::UnsupportedPlan {
            message: format!("missing attachment '{attachment_name}'"),
        })?;
    let values = result
        .attachments
        .decode_attachment(attachment_name)?
        .into_iter()
        .map(|value| kernel_value_json(&value))
        .collect::<Vec<_>>();
    Ok(json!({
        "name": attachment_name,
        "kind": format!("{:?}", attachment.layout.attachment.kind),
        "element_schema": attachment_element_schema_json(&attachment.layout.attachment),
        "lifetime": format!("{:?}", attachment.layout.attachment.lifetime),
        "clear_policy": format!("{:?}", attachment.layout.attachment.clear_policy),
        "resolution": format!("{:?}", attachment.layout.attachment.resolution),
        "scale": {
            "divisor_x": attachment.layout.attachment.scale.divisor_x,
            "divisor_y": attachment.layout.attachment.scale.divisor_y,
        },
        "width": attachment.layout.width,
        "height": attachment.layout.height,
        "values": values,
    }))
}

pub fn attachment_name_for_selector<'a>(
    result: &'a PresentationExecutionResult,
    selector: &str,
) -> Result<&'a str, PresentationExecError> {
    let normalized = selector.trim();
    let kind = match normalized {
        "color" => Some(FrameAttachmentKind::Color),
        "depth" => Some(FrameAttachmentKind::Depth),
        "normal" | "world_normal" => Some(FrameAttachmentKind::WorldNormal),
        "motion" => Some(FrameAttachmentKind::Motion),
        _ => None,
    };
    if let Some(kind) = kind {
        return attachment_name_for_kind(result, kind);
    }
    if let Some((name, _)) = result.attachments.attachments.get_key_value(normalized) {
        return Ok(name.as_str());
    }
    Err(PresentationExecError::UnsupportedPlan {
        message: format!("unknown frame attachment selector '{selector}'"),
    })
}

fn encode_normal_lane(value: f32) -> u8 {
    (((value.clamp(-1.0, 1.0) * 0.5) + 0.5) * 255.0).round() as u8
}

fn encode_color_lane(value: f32) -> u8 {
    value.clamp(0.0, 255.0).round() as u8
}

fn render_ppm_string(width: u32, height: u32, pixels: &[[u8; 3]]) -> String {
    let mut data = format!("P3\n{} {}\n255\n", width, height);
    for pixel in pixels {
        data.push_str(&format!("{} {} {}\n", pixel[0], pixel[1], pixel[2]));
    }
    data
}

pub fn attachment_name_for_kind(
    result: &PresentationExecutionResult,
    kind: FrameAttachmentKind,
) -> Result<&str, PresentationExecError> {
    attachment_name_for_kind_optional(result, kind).ok_or_else(|| {
        PresentationExecError::UnsupportedPlan {
            message: format!("missing {:?} attachment for debug export", kind),
        }
    })
}

fn attachment_name_for_kind_optional(
    result: &PresentationExecutionResult,
    kind: FrameAttachmentKind,
) -> Option<&str> {
    result
        .attachments
        .attachments
        .iter()
        .find_map(|(name, attachment)| {
            (attachment.layout.attachment.kind == kind).then_some(name.as_str())
        })
}

fn attachment_element_schema_json(attachment: &FrameAttachmentContract) -> Value {
    match &attachment.element_schema {
        AttachmentElementSchema::NamedRecord(name) => json!({
            "kind": "record",
            "name": name.to_string(),
        }),
        AttachmentElementSchema::ScalarF32 => json!({ "kind": "scalar_f32" }),
        AttachmentElementSchema::Vec2F32 => json!({ "kind": "vec2_f32" }),
        AttachmentElementSchema::Vec3F32 => json!({ "kind": "vec3_f32" }),
        AttachmentElementSchema::Vec4F32 => json!({ "kind": "vec4_f32" }),
    }
}

fn kernel_value_json(value: &KernelValue) -> Value {
    match value {
        KernelValue::Nothing => Value::Null,
        KernelValue::Bool(value) => Value::Bool(*value),
        KernelValue::I32(value) => json!(value),
        KernelValue::U32(value) => json!(value),
        KernelValue::F32(value) => json!(value),
        KernelValue::Vec2(value) => json!(value),
        KernelValue::Vec3(value) => json!(value),
        KernelValue::Vec4(value) => json!(value),
        KernelValue::Mat3(value) => json!(value),
        KernelValue::Mat4(value) => json!(value),
        KernelValue::Quat(value) => json!(value),
        KernelValue::Array(values) => Value::Array(values.iter().map(kernel_value_json).collect()),
        KernelValue::Struct(value) => {
            let mut fields = Map::new();
            fields.insert(
                "__record".to_string(),
                Value::String(value.name.to_string()),
            );
            for (name, value) in &value.fields {
                fields.insert(name.to_string(), kernel_value_json(value));
            }
            Value::Object(fields)
        }
        KernelValue::Capture(value) => json!({ "capture": value.to_string() }),
        KernelValue::DispatchBackend(value) => json!(format!("{value:?}")),
        KernelValue::GpuBuffer(handle) => json!({ "gpu_buffer": handle }),
        KernelValue::GpuAtomicI32(handle) => json!({ "gpu_atomic_i32": handle }),
        KernelValue::GpuAtomicU32(handle) => json!({ "gpu_atomic_u32": handle }),
    }
}
