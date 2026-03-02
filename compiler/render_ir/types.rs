use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RenderPipelineTopologyV5 {
    Triangles,
    TriangleStrip,
    Lines,
    LineStrip,
    Points,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RenderPipelineCullModeV5 {
    None,
    BackFace,
    FrontFace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RenderPipelineTargetV5 {
    SurfaceColor,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderResourceContractV5 {
    pub id: String,
    pub group: u32,
    pub binding: u32,
    pub name: String,
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderCapabilityContractV5 {
    pub id: String,
    pub name: String,
    pub target: String,
    pub preset: String,
    pub profile: String,
    pub shader_mode: String,
    pub shader_ref: Option<String>,
    pub override_tiers: Vec<u8>,
    pub shader_module: String,
    pub pipeline_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderPipelineContractV5 {
    pub id: String,
    pub label: String,
    pub shader_module: String,
    pub vertex_entry: String,
    pub fragment_entry: String,
    pub topology: RenderPipelineTopologyV5,
    pub cull_mode: RenderPipelineCullModeV5,
    pub targets: Vec<RenderPipelineTargetV5>,
    pub capability_id: String,
    pub resources: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderPassContractV5 {
    pub name: String,
    pub pipeline_id: String,
    pub draw_phase: String,
    pub depends_on: Vec<String>,
    pub reads: Vec<String>,
    pub writes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderGraphContractV5 {
    pub resources: Vec<RenderResourceContractV5>,
    pub capabilities: Vec<RenderCapabilityContractV5>,
    pub pipelines: Vec<RenderPipelineContractV5>,
    pub passes: Vec<RenderPassContractV5>,
}

pub type RenderResourceBinding = RenderResourceContractV5;
pub type RenderPipelineIRV1 = RenderPipelineContractV5;
pub type RenderPassIRV1 = RenderPassContractV5;
pub type RenderGraphIRV1 = RenderGraphContractV5;

#[cfg(test)]
mod tests {
    use super::{
        RenderPipelineContractV5, RenderPipelineCullModeV5, RenderPipelineTargetV5,
        RenderPipelineTopologyV5,
    };
    use serde_json::json;

    #[test]
    fn pipeline_contract_rejects_adapter_tokens() {
        let payload = json!({
            "id": "geometry_pipeline",
            "label": "geometry",
            "shader_module": "geometry_shader",
            "vertex_entry": "vs_main",
            "fragment_entry": "fs_main",
            "topology": "triangle-list",
            "cull_mode": "back",
            "targets": ["bgra8unorm"],
            "capability_id": "geometry_capability",
            "resources": ["scene_globals"]
        });

        let err =
            serde_json::from_value::<RenderPipelineContractV5>(payload).expect_err("must fail");
        let message = err.to_string();
        assert!(
            message.contains("unknown variant"),
            "unexpected parse error: {message}"
        );
    }

    #[test]
    fn pipeline_contract_accepts_backend_neutral_tokens() {
        let payload = json!({
            "id": "geometry_pipeline",
            "label": "geometry",
            "shader_module": "geometry_shader",
            "vertex_entry": "vs_main",
            "fragment_entry": "fs_main",
            "topology": "triangles",
            "cull_mode": "back-face",
            "targets": ["surface-color"],
            "capability_id": "geometry_capability",
            "resources": ["scene_globals"]
        });

        let contract: RenderPipelineContractV5 =
            serde_json::from_value(payload).expect("must parse neutral tokens");
        assert_eq!(contract.topology, RenderPipelineTopologyV5::Triangles);
        assert_eq!(contract.cull_mode, RenderPipelineCullModeV5::BackFace);
        assert_eq!(contract.targets, vec![RenderPipelineTargetV5::SurfaceColor]);
    }
}
