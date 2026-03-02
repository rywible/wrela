use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct DeployRequest {
    pub project_root: PathBuf,
    pub deploy_context_root: PathBuf,
    pub app: String,
    pub region: String,
    pub machines: usize,
    pub region_machine_counts: BTreeMap<String, usize>,
    pub target_voters: u32,
    pub replication_factor: u32,
    pub write_quorum: u32,
    pub config_path: PathBuf,
    pub dockerfile_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployReport {
    pub provider: String,
    pub app: String,
    pub url: String,
    pub region: String,
    pub machines: usize,
    pub started_at_unix_ms: u128,
    pub finished_at_unix_ms: u128,
    pub pre_health: serde_json::Value,
    pub post_health: serde_json::Value,
    pub pre_probe: serde_json::Value,
    pub post_probe: serde_json::Value,
    pub machine_ids: Vec<String>,
    pub notes: Vec<String>,
}

pub trait DeployProvider {
    fn deploy(&self, request: &DeployRequest) -> Result<DeployReport, String>;
}
