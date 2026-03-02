use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextureFormat {
    Rgba8,
}

impl TextureFormat {
    pub const fn bytes_per_pixel(self) -> u32 {
        match self {
            Self::Rgba8 => 4,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextureSource {
    pub width: u32,
    pub height: u32,
    pub format: TextureFormat,
    pub pixels: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TextureNoiseKindV1 {
    Simplex,
    Perlin,
    Worley,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TextureSdfShapeV1 {
    Circle,
    Box,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TextureBlendModeV1 {
    Add,
    Multiply,
    Screen,
    Overlay,
    Lerp,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "kebab-case")]
pub enum TextureNodeOpV1 {
    Noise { kind: TextureNoiseKindV1 },
    Sdf { shape: TextureSdfShapeV1 },
    Warp,
    Tile,
    Mask,
    Blend { mode: TextureBlendModeV1 },
    Grade,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TextureNodeV1 {
    pub id: String,
    pub op: TextureNodeOpV1,
    pub inputs: Vec<String>,
    pub params: BTreeMap<String, f32>,
    pub seed_offset: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TextureProgramV1 {
    pub schema_version: u32,
    pub kind: String,
    pub width: u32,
    pub height: u32,
    pub seed: u64,
    pub nodes: Vec<TextureNodeV1>,
    pub output: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PackedChannelSourceV1 {
    TextureChannel,
    Constant,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackedChannelBindingV1 {
    pub target_channel: u8,
    pub source_kind: PackedChannelSourceV1,
    pub source_texture: Option<String>,
    pub source_channel: Option<u8>,
    pub constant: Option<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelPackLayoutV1 {
    pub schema_version: u32,
    pub kind: String,
    pub id: String,
    pub bindings: Vec<PackedChannelBindingV1>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextureCompressionMetadata {
    pub codec: String,
    pub uncompressed_bytes: usize,
    pub compressed_bytes: usize,
    pub ratio_milli: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MipPreservationHooksV1 {
    pub roughness_channel: Option<u8>,
    pub normal_channels: Option<[u8; 3]>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MipPreservationStatsV1 {
    pub roughness_mean_milli: u32,
    pub roughness_variance_milli: u32,
    pub normal_length_mean_milli: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MipLevel {
    pub level: u32,
    pub width: u32,
    pub height: u32,
    pub byte_len: usize,
    pub pixel_hash: String,
    pub pixels: Vec<u8>,
    pub compression: TextureCompressionMetadata,
    pub preservation: Option<MipPreservationStatsV1>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextureMipChain {
    pub schema_version: u32,
    pub kind: String,
    pub source_hash: String,
    pub levels: Vec<MipLevel>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextureStreamingMetadataV1 {
    pub tile_width: u32,
    pub tile_height: u32,
    pub min_resident_mip: u32,
    pub prefetch_mips: u32,
    pub residency_priority: u8,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextureArtifactStatsV1 {
    pub roughness_base_mean_milli: u32,
    pub roughness_tail_mean_milli: u32,
    pub normal_base_mean_milli: u32,
    pub normal_tail_mean_milli: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextureArtifactV1 {
    pub schema_version: u32,
    pub kind: String,
    pub program_hash: String,
    pub content_hash: String,
    pub packed_layout: ChannelPackLayoutV1,
    pub packed_texture: TextureSource,
    pub mip_chain: TextureMipChain,
    pub streaming: TextureStreamingMetadataV1,
    pub stats: TextureArtifactStatsV1,
}
