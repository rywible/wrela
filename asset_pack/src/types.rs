use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConditioningEvidence {
    pub pipeline: String,
    pub source_hash: String,
    pub deterministic_hash: String,
    pub steps: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompressionMetadata {
    pub codec: String,
    pub uncompressed_bytes: u64,
    pub compressed_bytes: u64,
    pub ratio_milli: u32,
    pub block_bytes: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TileMetadata {
    pub tile_width: u32,
    pub tile_height: u32,
    pub tile_layers: u32,
    pub tile_rows: u32,
    pub tile_columns: u32,
    pub total_tiles: u32,
    pub tile_format: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResidencyClass {
    Core,
    Hot,
    Warm,
    Cold,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConvergenceStage {
    Bootstrap,
    Stream,
    Refine,
    Converged,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LodBounds {
    pub min: [i32; 3],
    pub max: [i32; 3],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LodLineage {
    pub source_asset_id: String,
    pub source_hash: String,
    pub max_lod: u32,
    pub bounds: LodBounds,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetChunk {
    pub id: String,
    pub path: String,
    pub bytes: u64,
    pub checksum: u32,
    pub dependencies: Vec<String>,
    pub residency_priority: String,
    pub residency_class: ResidencyClass,
    pub convergence_stage: ConvergenceStage,
    pub deterministic_hash: String,
    pub conditioning_evidence: ConditioningEvidence,
    pub compression: CompressionMetadata,
    pub tile: TileMetadata,
    pub lod: LodLineage,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetPartition {
    pub id: u32,
    pub chunk_ids: Vec<String>,
    pub residency_budget_bytes: u64,
    pub prefetch_budget: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetPackManifestV3 {
    pub schema_version: u32,
    pub kind: String,
    pub pack_id: String,
    pub streaming_budget_bytes: u64,
    pub partitions: Vec<AssetPartition>,
    pub chunks: Vec<AssetChunk>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldChunk {
    pub id: String,
    pub asset_chunk_ids: Vec<String>,
    pub hlod_asset_chunk_ids: Vec<String>,
    pub prefetch_neighbors: Vec<String>,
    pub refinement_sequence: Vec<WorldChunkRefinementStep>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldChunkRefinementStep {
    pub stage: ConvergenceStage,
    pub asset_chunk_ids: Vec<String>,
    pub hlod_asset_chunk_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldChunkPartition {
    pub world_chunk_id: String,
    pub partition_id: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldChunkManifestV2 {
    pub schema_version: u32,
    pub kind: String,
    pub world_id: String,
    pub partitions: Vec<WorldChunkPartition>,
    pub chunks: Vec<WorldChunk>,
}
