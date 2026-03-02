use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentPipelineV2 {
    CompilerFirstOneShot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentFidelityModeV2 {
    Fast,
    Balanced,
    Strict,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentVibeSpecV2 {
    pub style: String,
    pub audience: String,
    pub creativity: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentFidelitySpecV2 {
    pub mode: AgentFidelityModeV2,
    pub min_test_coverage: u8,
    pub require_reproducible_artifacts: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentBudgetSpecV2 {
    pub max_input_tokens: u32,
    pub max_output_tokens: u32,
    pub max_tool_calls: u32,
    pub max_wall_time_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentPolicyProfileV2 {
    pub sandbox_mode: String,
    pub network_access: bool,
    pub approval_mode: String,
    pub data_policy: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentToolContractV2 {
    pub tool_name: String,
    pub mode: String,
    pub required: bool,
    pub deterministic: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentExecutionContractSpecV2 {
    pub pipeline: AgentPipelineV2,
    pub max_retries: u8,
    pub fallback_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetFactoryIntentV1 {
    pub enabled: bool,
    pub mode: String,
    pub providers: Vec<String>,
    pub strict_provenance: bool,
    pub variant_count: u8,
    pub seed_lock: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentRunIntentV2 {
    pub schema_version: u32,
    pub kind: String,
    pub prompt: String,
    pub app_path: String,
    pub seed: u64,
    pub target_profile: String,
    pub execution_profile: String,
    pub constraints: Vec<String>,
    pub vibe_spec: AgentVibeSpecV2,
    pub fidelity_spec: AgentFidelitySpecV2,
    pub budget_spec: AgentBudgetSpecV2,
    pub policy_profile: AgentPolicyProfileV2,
    pub tool_contracts: Vec<AgentToolContractV2>,
    pub execution_contract: AgentExecutionContractSpecV2,
    pub asset_factory_intent: AssetFactoryIntentV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentDeterministicExecutionContractV2 {
    pub pipeline: AgentPipelineV2,
    pub deterministic: bool,
    pub stage_order: Vec<String>,
    pub task_graph_hash: String,
    pub max_retries: u8,
    pub fallback_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentTaskSpecV2 {
    pub id: String,
    pub step: String,
    pub depends_on: Vec<String>,
    pub command: String,
    pub args: Vec<String>,
    pub gate: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentExecutionPlanV2 {
    pub schema_version: u32,
    pub kind: String,
    pub run_id: String,
    pub intent_hash: String,
    pub created_from_intent_kind: String,
    pub execution_contract: AgentDeterministicExecutionContractV2,
    pub task_count: usize,
    pub tasks: Vec<AgentTaskSpecV2>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentVariantScoreDecisionV1 {
    pub variant_id: String,
    pub score: f64,
    pub rank: u32,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentVariantScoringReportV1 {
    pub schema_version: u32,
    pub kind: String,
    pub run_id: String,
    pub selected_variant: String,
    pub decisions: Vec<AgentVariantScoreDecisionV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentRunSummaryV2 {
    pub schema_version: u32,
    pub kind: String,
    pub run_id: String,
    pub status: String,
    pub selected_variant: String,
    pub artifacts: Vec<String>,
}
