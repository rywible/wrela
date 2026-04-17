use crate::acceleration::clipmap::{
    ViewDistanceClipmapArtifact, ViewDistanceClipmapBuildMode, ViewDistanceClipmapFallbackReason,
    render_view_distance_clipmap_report, view_distance_clipmap_layout_signature,
    view_distance_clipmap_runtime_signature,
};
use crate::execution_policy::PresentationExecutionPolicy;
use crate::presentation_contract::RealtimeQualityState;
use crate::presentation_exec::PRESENTATION_TILE_SIZE;
use crate::presentation_exec::TileCullingMask;
use crate::world_identity::WorldSnapshotHandle;
use smol_str::SmolStr;

use super::{
    FrameStateTemporalComponents, PassRuntimeStats, PresentationClipmapPassMetadata,
    internal_resolution_viewport,
};

pub(crate) fn build_view_distance_clipmap_artifact(
    semantic_root: &str,
    current_snapshot: &WorldSnapshotHandle,
    components: &FrameStateTemporalComponents,
    quality: &RealtimeQualityState,
    execution_policy: PresentationExecutionPolicy,
    tile_cull: Option<&TileCullingMask>,
    previous: Option<&ViewDistanceClipmapArtifact>,
) -> ViewDistanceClipmapArtifact {
    let internal_viewport = internal_resolution_viewport(components.viewport, quality);
    let resolution_width = components.viewport.width.max(1);
    let resolution_height = components.viewport.height.max(1);
    let internal_width = internal_viewport.width.max(1);
    let internal_height = internal_viewport.height.max(1);
    let band_width = PRESENTATION_TILE_SIZE.max(1);
    let brick_dimensions = [
        internal_width.div_ceil(band_width).max(1),
        internal_height.div_ceil(band_width).max(1),
        1,
    ];
    let narrow_band_width = execution_policy.primary_rays.max_steps.max(1) as u32;
    let layout_signature = view_distance_clipmap_layout_signature(
        semantic_root,
        resolution_width,
        resolution_height,
        internal_width,
        internal_height,
        brick_dimensions,
        band_width,
        narrow_band_width,
    );
    let usage_count = tile_cull
        .map(|mask| mask.active_samples.len() as u32)
        .unwrap_or_default();
    let camera_motion = camera_motion_score(components);
    let snapshot_report = current_snapshot.report();

    let mut fallback_reasons = Vec::new();
    let previous_present = previous.is_some();
    let mut build_mode = ViewDistanceClipmapBuildMode::Rebuilt;

    if let Some(previous) = previous {
        if previous.snapshot.snapshot_id != snapshot_report.snapshot_id
            || previous.snapshot.epoch != snapshot_report.epoch
        {
            build_mode = ViewDistanceClipmapBuildMode::Fallback;
            fallback_reasons.push(ViewDistanceClipmapFallbackReason::SnapshotMismatch);
        } else if previous.layout_signature != layout_signature {
            build_mode = ViewDistanceClipmapBuildMode::Fallback;
            fallback_reasons.push(ViewDistanceClipmapFallbackReason::LayoutMismatch);
        } else if camera_motion <= 0.001 {
            build_mode = ViewDistanceClipmapBuildMode::Reused;
        } else if camera_motion <= 0.25 {
            build_mode = ViewDistanceClipmapBuildMode::Updated;
        } else {
            build_mode = ViewDistanceClipmapBuildMode::Fallback;
            fallback_reasons
                .push(ViewDistanceClipmapFallbackReason::CameraMotionExceededReuseThreshold);
        }
    }

    let build_bytes = u64::from(brick_dimensions[0])
        * u64::from(brick_dimensions[1])
        * u64::from(brick_dimensions[2])
        * u64::from(band_width)
        * 16;
    let mut upload_bytes = match build_mode {
        ViewDistanceClipmapBuildMode::Reused => 0,
        ViewDistanceClipmapBuildMode::Updated => {
            (build_bytes / 4).saturating_add(u64::from(usage_count) * u64::from(band_width) * 4)
        }
        ViewDistanceClipmapBuildMode::Rebuilt | ViewDistanceClipmapBuildMode::Fallback => {
            (build_bytes / 2).saturating_add(u64::from(usage_count) * u64::from(band_width) * 8)
        }
    };
    let upload_budget = u64::from(execution_policy.primary_rays.max_steps.max(1) as u32)
        * u64::from(band_width)
        * 64;
    if upload_bytes > upload_budget {
        fallback_reasons.push(ViewDistanceClipmapFallbackReason::UploadBudgetExceeded);
        build_mode = ViewDistanceClipmapBuildMode::Fallback;
        upload_bytes = upload_budget;
    }
    if tile_cull.is_none() {
        fallback_reasons.push(ViewDistanceClipmapFallbackReason::TileCullingUnavailable);
        build_mode = ViewDistanceClipmapBuildMode::Fallback;
    }

    let build_count = u32::from(matches!(
        build_mode,
        ViewDistanceClipmapBuildMode::Rebuilt | ViewDistanceClipmapBuildMode::Fallback
    ));
    let update_count = u32::from(matches!(build_mode, ViewDistanceClipmapBuildMode::Updated));
    let reuse_count = u32::from(matches!(build_mode, ViewDistanceClipmapBuildMode::Reused));
    let upload_count = u32::from(build_count + update_count > 0);
    let eviction_count =
        u32::from(previous_present && !matches!(build_mode, ViewDistanceClipmapBuildMode::Reused));
    let build_bytes = match build_mode {
        ViewDistanceClipmapBuildMode::Reused => 0,
        _ => build_bytes,
    };
    let runtime_signature = view_distance_clipmap_runtime_signature(
        layout_signature,
        snapshot_report.snapshot_id.0,
        snapshot_report.epoch.0,
        components.camera.position,
        components.camera.forward,
        build_mode,
        usage_count,
        upload_bytes,
    );

    ViewDistanceClipmapArtifact {
        schema_version: crate::acceleration::clipmap::VIEW_DISTANCE_CLIPMAP_SCHEMA_VERSION,
        semantic_root: SmolStr::new(semantic_root),
        snapshot: snapshot_report,
        resolution_width,
        resolution_height,
        internal_width,
        internal_height,
        brick_dimensions,
        voxel_size: band_width,
        narrow_band_width,
        build_mode,
        build_count,
        update_count,
        reuse_count,
        upload_count,
        build_bytes,
        upload_bytes,
        eviction_count,
        usage_count,
        fallback_reasons,
        layout_signature,
        runtime_signature,
    }
}

pub(crate) fn clipmap_pass_runtime(
    pass_id: impl Into<SmolStr>,
    artifact: &ViewDistanceClipmapArtifact,
) -> PassRuntimeStats {
    PassRuntimeStats {
        pass_id: pass_id.into().to_string(),
        pass_kind: "view_distance_clipmap".to_string(),
        work_items: artifact.usage_count,
        elapsed_micros: 0,
        gpu_elapsed_micros: None,
        dispatch_count: artifact.upload_count + artifact.eviction_count,
        attachment_bytes_read: artifact.build_bytes,
        attachment_bytes_written: artifact.upload_bytes,
        clipmap: Some(PresentationClipmapPassMetadata {
            status: artifact.build_mode,
            fallback_reasons: artifact.fallback_reasons.clone(),
        }),
        notes: vec![render_view_distance_clipmap_report(artifact)],
    }
}

pub(crate) fn clipmap_pass_note(artifact: &ViewDistanceClipmapArtifact) -> String {
    render_view_distance_clipmap_report(artifact)
}

fn camera_motion_score(components: &FrameStateTemporalComponents) -> f32 {
    let position_delta = distance3(
        components.camera.position,
        components.previous_camera.position,
    );
    let forward_delta = distance3(
        components.camera.forward,
        components.previous_camera.forward,
    );
    let viewport_delta = if components.viewport == components.previous_viewport {
        0.0
    } else {
        1.0
    };
    position_delta + forward_delta + viewport_delta
}

fn distance3(lhs: [f32; 3], rhs: [f32; 3]) -> f32 {
    let dx = lhs[0] - rhs[0];
    let dy = lhs[1] - rhs[1];
    let dz = lhs[2] - rhs[2];
    (dx * dx + dy * dy + dz * dz).sqrt()
}
