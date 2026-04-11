use crate::kernel::KernelValue;
use crate::presentation_contract::FrameAttachmentKind;
use crate::presentation_exec::{PresentationExecError, PresentationExecutionResult};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresentationDebugArtifacts {
    pub color_ppm: PathBuf,
    pub depth_ppm: PathBuf,
    pub world_normal_ppm: PathBuf,
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
    write_color_ppm(result, &color_ppm)?;
    write_depth_ppm(result, &depth_ppm)?;
    write_world_normal_ppm(result, &world_normal_ppm)?;
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
    format!(
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
    )
}

pub fn write_depth_ppm(
    result: &PresentationExecutionResult,
    path: &Path,
) -> Result<(), PresentationExecError> {
    let depth_name = attachment_name_for_kind(result, FrameAttachmentKind::Depth)?;
    let depths = result
        .attachments
        .decode_attachment(depth_name)?
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
    write_ppm(path, result.width, result.height, &pixels)
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
    let pixels = result
        .attachments
        .decode_attachment(color_name)?
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
    let pixels = result
        .attachments
        .decode_attachment(world_normal_name)?
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
    write_ppm(path, result.width, result.height, &pixels)
}

fn encode_normal_lane(value: f32) -> u8 {
    (((value.clamp(-1.0, 1.0) * 0.5) + 0.5) * 255.0).round() as u8
}

fn encode_color_lane(value: f32) -> u8 {
    value.clamp(0.0, 255.0).round() as u8
}

fn write_ppm(
    path: &Path,
    width: u32,
    height: u32,
    pixels: &[[u8; 3]],
) -> Result<(), PresentationExecError> {
    let data = render_ppm_string(width, height, pixels);
    fs::write(path, data).map_err(|err| PresentationExecError::UnsupportedPlan {
        message: err.to_string(),
    })
}

fn render_ppm_string(width: u32, height: u32, pixels: &[[u8; 3]]) -> String {
    let mut data = format!("P3\n{} {}\n255\n", width, height);
    for pixel in pixels {
        data.push_str(&format!("{} {} {}\n", pixel[0], pixel[1], pixel[2]));
    }
    data
}

fn attachment_name_for_kind(
    result: &PresentationExecutionResult,
    kind: FrameAttachmentKind,
) -> Result<&str, PresentationExecError> {
    result
        .attachments
        .attachments
        .iter()
        .find_map(|(name, attachment)| {
            (attachment.layout.attachment.kind == kind).then_some(name.as_str())
        })
        .ok_or_else(|| PresentationExecError::UnsupportedPlan {
            message: format!("missing {:?} attachment for debug export", kind),
        })
}
