use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MeshVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MeshSurface {
    pub id: String,
    pub material_id: String,
    pub vertices: Vec<MeshVertex>,
    pub indices: Vec<u32>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MeshAsset {
    pub schema_version: u32,
    pub kind: String,
    pub mesh_id: String,
    pub surfaces: Vec<MeshSurface>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LodBounds {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LodLineage {
    pub source_mesh_id: String,
    pub source_hash: String,
    pub generator: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LodLevel {
    pub level: u32,
    pub triangle_count: usize,
    pub ratio_milli: u32,
    pub source_level: Option<u32>,
    pub bounds: LodBounds,
    pub deterministic_hash: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LodChain {
    pub schema_version: u32,
    pub kind: String,
    pub mesh_id: String,
    pub lineage: LodLineage,
    pub levels: Vec<LodLevel>,
}
