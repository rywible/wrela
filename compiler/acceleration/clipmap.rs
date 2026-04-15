use crate::query_exec::ids::stable_semantic_id;
use crate::world_identity::SnapshotIdentityReport;
use smol_str::SmolStr;

pub const VIEW_DISTANCE_CLIPMAP_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewDistanceClipmapBuildMode {
    Reused,
    Updated,
    Rebuilt,
    Fallback,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewDistanceClipmapArtifact {
    pub schema_version: u32,
    pub semantic_root: SmolStr,
    pub snapshot: SnapshotIdentityReport,
    pub resolution_width: u32,
    pub resolution_height: u32,
    pub internal_width: u32,
    pub internal_height: u32,
    pub brick_dimensions: [u32; 3],
    pub voxel_size: u32,
    pub narrow_band_width: u32,
    pub build_mode: ViewDistanceClipmapBuildMode,
    pub build_count: u32,
    pub update_count: u32,
    pub reuse_count: u32,
    pub upload_count: u32,
    pub build_bytes: u64,
    pub upload_bytes: u64,
    pub eviction_count: u32,
    pub usage_count: u32,
    pub fallback_reasons: Vec<SmolStr>,
    pub layout_signature: u64,
    pub runtime_signature: u64,
}

impl ViewDistanceClipmapArtifact {
    pub fn status_name(&self) -> &'static str {
        match self.build_mode {
            ViewDistanceClipmapBuildMode::Reused => "reused",
            ViewDistanceClipmapBuildMode::Updated => "updated",
            ViewDistanceClipmapBuildMode::Rebuilt => "rebuilt",
            ViewDistanceClipmapBuildMode::Fallback => "fallback",
        }
    }

    pub fn is_reused(&self) -> bool {
        matches!(self.build_mode, ViewDistanceClipmapBuildMode::Reused)
    }
}

pub fn view_distance_clipmap_layout_signature(
    semantic_root: &str,
    resolution_width: u32,
    resolution_height: u32,
    internal_width: u32,
    internal_height: u32,
    brick_dimensions: [u32; 3],
    voxel_size: u32,
    narrow_band_width: u32,
) -> u64 {
    stable_semantic_id(&[
        semantic_root.as_bytes(),
        &resolution_width.to_le_bytes(),
        &resolution_height.to_le_bytes(),
        &internal_width.to_le_bytes(),
        &internal_height.to_le_bytes(),
        &brick_dimensions[0].to_le_bytes(),
        &brick_dimensions[1].to_le_bytes(),
        &brick_dimensions[2].to_le_bytes(),
        &voxel_size.to_le_bytes(),
        &narrow_band_width.to_le_bytes(),
    ])
}

pub fn view_distance_clipmap_runtime_signature(
    layout_signature: u64,
    snapshot_id: u64,
    snapshot_epoch: u64,
    camera_position: [f32; 3],
    camera_forward: [f32; 3],
    build_mode: ViewDistanceClipmapBuildMode,
    usage_count: u32,
    upload_bytes: u64,
) -> u64 {
    let mode = match build_mode {
        ViewDistanceClipmapBuildMode::Reused => 0u32,
        ViewDistanceClipmapBuildMode::Updated => 1u32,
        ViewDistanceClipmapBuildMode::Rebuilt => 2u32,
        ViewDistanceClipmapBuildMode::Fallback => 3u32,
    };
    stable_semantic_id(&[
        &layout_signature.to_le_bytes(),
        &snapshot_id.to_le_bytes(),
        &snapshot_epoch.to_le_bytes(),
        &camera_position[0].to_bits().to_le_bytes(),
        &camera_position[1].to_bits().to_le_bytes(),
        &camera_position[2].to_bits().to_le_bytes(),
        &camera_forward[0].to_bits().to_le_bytes(),
        &camera_forward[1].to_bits().to_le_bytes(),
        &camera_forward[2].to_bits().to_le_bytes(),
        &mode.to_le_bytes(),
        &usage_count.to_le_bytes(),
        &upload_bytes.to_le_bytes(),
    ])
}

pub fn render_view_distance_clipmap_report(artifact: &ViewDistanceClipmapArtifact) -> String {
    let fallback_reasons = if artifact.fallback_reasons.is_empty() {
        "none".to_string()
    } else {
        artifact
            .fallback_reasons
            .iter()
            .map(|reason| reason.as_str())
            .collect::<Vec<_>>()
            .join("|")
    };
    format!(
        "view_distance_clipmap schema_version={} semantic_root={} status={} resolution={}x{} internal={}x{} bricks={},{},{} voxel_size={} narrow_band_width={} build={} update={} reuse={} upload={} build_bytes={} upload_bytes={} eviction={} usage={} fallback_reasons={} layout_signature={} runtime_signature={}",
        artifact.schema_version,
        artifact.semantic_root,
        artifact.status_name(),
        artifact.resolution_width,
        artifact.resolution_height,
        artifact.internal_width,
        artifact.internal_height,
        artifact.brick_dimensions[0],
        artifact.brick_dimensions[1],
        artifact.brick_dimensions[2],
        artifact.voxel_size,
        artifact.narrow_band_width,
        artifact.build_count,
        artifact.update_count,
        artifact.reuse_count,
        artifact.upload_count,
        artifact.build_bytes,
        artifact.upload_bytes,
        artifact.eviction_count,
        artifact.usage_count,
        fallback_reasons,
        artifact.layout_signature,
        artifact.runtime_signature,
    )
}
