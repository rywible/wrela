use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ShaderStageV1 {
    Vertex,
    Fragment,
    Compute,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShaderBindingV1 {
    pub id: String,
    pub group: u32,
    pub binding: u32,
    pub resource_kind: String,
    pub stage: ShaderStageV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShaderEntryPointV1 {
    pub id: String,
    pub function_name: String,
    pub stage: ShaderStageV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShaderProgramIRV1 {
    pub schema_version: u32,
    pub kind: String,
    pub program_id: String,
    pub bindings: Vec<ShaderBindingV1>,
    pub entry_points: Vec<ShaderEntryPointV1>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum MaterialQualityTierV1 {
    Hero,
    Gameplay,
    Low,
}

impl MaterialQualityTierV1 {
    pub const ALL: [MaterialQualityTierV1; 3] = [
        MaterialQualityTierV1::Hero,
        MaterialQualityTierV1::Gameplay,
        MaterialQualityTierV1::Low,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            MaterialQualityTierV1::Hero => "hero",
            MaterialQualityTierV1::Gameplay => "gameplay",
            MaterialQualityTierV1::Low => "low",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaterialParamV1 {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaterialIrV1 {
    pub schema_version: u32,
    pub material_name: String,
    pub surface_model: String,
    pub alpha_mode: String,
    pub feature_bits: Vec<String>,
    pub texture_slots: Vec<String>,
    pub params: Vec<MaterialParamV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaterialShaderVariantV1 {
    pub variant_key: String,
    pub quality_tier: MaterialQualityTierV1,
    pub feature_bits: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaterialQualityTierCost {
    pub quality_tier: MaterialQualityTierV1,
    pub texture_fetch_count: u32,
    pub estimated_alu_ops: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MaterialTextureColorSpaceV1 {
    Srgb,
    Linear,
}

impl MaterialTextureColorSpaceV1 {
    pub const fn as_str(self) -> &'static str {
        match self {
            MaterialTextureColorSpaceV1::Srgb => "srgb",
            MaterialTextureColorSpaceV1::Linear => "linear",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaterialTextureBindingSummaryV1 {
    pub slot: String,
    pub source_reference: String,
    pub runtime_reference: String,
    pub color_space: MaterialTextureColorSpaceV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaterialCompileReport {
    pub texture_fetch_count: u32,
    pub estimated_alu_ops: u32,
    pub variant_count: usize,
    pub features_enabled: Vec<String>,
    pub quality_tier_costs: Vec<MaterialQualityTierCost>,
    pub explain_lines: Vec<String>,
    pub runtime_texture_format: String,
    pub texture_lint_lines: Vec<String>,
    pub texture_bindings: Vec<MaterialTextureBindingSummaryV1>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaterialCompileBudgetGatesV1 {
    pub max_texture_fetch_count: u32,
    pub max_estimated_alu_ops: u32,
    pub required_quality_tier_count: usize,
    pub require_explain_lines: bool,
}

pub const MATERIAL_COMPILE_BUDGET_GATES_V1: MaterialCompileBudgetGatesV1 =
    MaterialCompileBudgetGatesV1 {
        max_texture_fetch_count: 8,
        max_estimated_alu_ops: 80,
        required_quality_tier_count: 3,
        require_explain_lines: true,
    };
