use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssetAdapterKind {
    Image,
    Mesh3d,
    Audio,
    FigmaUi,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetGenerationRequest {
    pub asset_id: String,
    pub prompt: String,
    pub style_profile: String,
    pub seed: u64,
    pub negative_constraints: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanonicalJobEnvelope {
    pub job_id: String,
    pub adapter_kind: AssetAdapterKind,
    pub provider: String,
    pub seed: u64,
    pub canonical_prompt: String,
    pub replay_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetGenerationArtifact {
    pub artifact_id: String,
    pub logical_path: String,
    pub bytes_len: u64,
    pub fingerprint: String,
    pub metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetGenerationResult {
    pub envelope: CanonicalJobEnvelope,
    pub provider_response_hash: String,
    pub artifacts: Vec<AssetGenerationArtifact>,
}

pub trait DeterministicAssetAdapter {
    fn kind(&self) -> AssetAdapterKind;
    fn provider(&self) -> &str;
    fn generate(&self, request: &AssetGenerationRequest) -> Result<AssetGenerationResult, String>;
}

#[derive(Debug, Clone)]
pub struct ImageGenerationAdapter {
    provider: String,
}

#[derive(Debug, Clone)]
pub struct MeshGenerationAdapter {
    provider: String,
}

#[derive(Debug, Clone)]
pub struct AudioGenerationAdapter {
    provider: String,
}

#[derive(Debug, Clone)]
pub struct FigmaUiAdapter {
    provider: String,
}

impl ImageGenerationAdapter {
    pub fn new(provider: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
        }
    }
}

impl MeshGenerationAdapter {
    pub fn new(provider: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
        }
    }
}

impl AudioGenerationAdapter {
    pub fn new(provider: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
        }
    }
}

impl FigmaUiAdapter {
    pub fn new(provider: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
        }
    }
}

impl DeterministicAssetAdapter for ImageGenerationAdapter {
    fn kind(&self) -> AssetAdapterKind {
        AssetAdapterKind::Image
    }

    fn provider(&self) -> &str {
        self.provider.as_str()
    }

    fn generate(&self, request: &AssetGenerationRequest) -> Result<AssetGenerationResult, String> {
        generate_deterministic_result(self.kind(), self.provider(), request, "texture")
    }
}

impl DeterministicAssetAdapter for MeshGenerationAdapter {
    fn kind(&self) -> AssetAdapterKind {
        AssetAdapterKind::Mesh3d
    }

    fn provider(&self) -> &str {
        self.provider.as_str()
    }

    fn generate(&self, request: &AssetGenerationRequest) -> Result<AssetGenerationResult, String> {
        generate_deterministic_result(self.kind(), self.provider(), request, "mesh")
    }
}

impl DeterministicAssetAdapter for AudioGenerationAdapter {
    fn kind(&self) -> AssetAdapterKind {
        AssetAdapterKind::Audio
    }

    fn provider(&self) -> &str {
        self.provider.as_str()
    }

    fn generate(&self, request: &AssetGenerationRequest) -> Result<AssetGenerationResult, String> {
        generate_deterministic_result(self.kind(), self.provider(), request, "audio")
    }
}

impl DeterministicAssetAdapter for FigmaUiAdapter {
    fn kind(&self) -> AssetAdapterKind {
        AssetAdapterKind::FigmaUi
    }

    fn provider(&self) -> &str {
        self.provider.as_str()
    }

    fn generate(&self, request: &AssetGenerationRequest) -> Result<AssetGenerationResult, String> {
        generate_deterministic_result(self.kind(), self.provider(), request, "ui")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionTopology {
    Local,
    CloudBurst,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HybridExecutionPolicy {
    pub cloud_burst_threshold: usize,
    pub force_local: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HybridExecutionDecision {
    pub topology: ExecutionTopology,
    pub reason: String,
}

pub fn decide_execution_topology(
    policy: &HybridExecutionPolicy,
    pending_jobs: usize,
) -> HybridExecutionDecision {
    if policy.force_local || pending_jobs < policy.cloud_burst_threshold {
        return HybridExecutionDecision {
            topology: ExecutionTopology::Local,
            reason: "local-capacity".to_string(),
        };
    }
    HybridExecutionDecision {
        topology: ExecutionTopology::CloudBurst,
        reason: "cloud-threshold-exceeded".to_string(),
    }
}

#[derive(Debug, Clone)]
pub struct AssetArtifactCache {
    root: PathBuf,
}

impl AssetArtifactCache {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn cache_key(envelope: &CanonicalJobEnvelope) -> String {
        envelope.replay_hash.clone()
    }

    pub fn put(&self, result: &AssetGenerationResult) -> Result<PathBuf, String> {
        let key = Self::cache_key(&result.envelope);
        let key_dir = self.root.join(key.as_str());
        fs::create_dir_all(key_dir.as_path())
            .map_err(|error| format!("failed to create cache directory: {error}"))?;
        let manifest_path = key_dir.join("result.json");
        let bytes = serde_json::to_vec_pretty(result)
            .map_err(|error| format!("failed to serialize cache result: {error}"))?;
        fs::write(manifest_path.as_path(), bytes.as_slice())
            .map_err(|error| format!("failed to write cache result: {error}"))?;
        Ok(manifest_path)
    }

    pub fn get(
        &self,
        envelope: &CanonicalJobEnvelope,
    ) -> Result<Option<AssetGenerationResult>, String> {
        let key = Self::cache_key(envelope);
        let manifest_path = self.root.join(key).join("result.json");
        if !manifest_path.exists() {
            return Ok(None);
        }
        let bytes = fs::read(manifest_path.as_path())
            .map_err(|error| format!("failed to read cache result: {error}"))?;
        let parsed: AssetGenerationResult = serde_json::from_slice(bytes.as_slice())
            .map_err(|error| format!("failed to decode cache result: {error}"))?;
        Ok(Some(parsed))
    }
}

fn generate_deterministic_result(
    kind: AssetAdapterKind,
    provider: &str,
    request: &AssetGenerationRequest,
    extension: &str,
) -> Result<AssetGenerationResult, String> {
    if request.asset_id.trim().is_empty() {
        return Err("asset generation request requires non-empty asset_id".to_string());
    }
    let canonical_prompt = canonical_prompt(request);
    let replay_hash = replay_hash(kind, provider, canonical_prompt.as_str(), request.seed);
    let job_id = format!("job-{}", &replay_hash[..16]);
    let artifact_id = format!("asset-{}", &replay_hash[16..32]);
    let logical_path = format!(
        "generated/{}/{}.{}",
        request.asset_id, artifact_id, extension
    );
    let mut metadata = BTreeMap::new();
    metadata.insert("provider".to_string(), provider.to_string());
    metadata.insert("style_profile".to_string(), request.style_profile.clone());
    metadata.insert("adapter_kind".to_string(), format!("{kind:?}"));

    let envelope = CanonicalJobEnvelope {
        job_id,
        adapter_kind: kind,
        provider: provider.to_string(),
        seed: request.seed,
        canonical_prompt,
        replay_hash: replay_hash.clone(),
    };
    let artifact = AssetGenerationArtifact {
        artifact_id,
        logical_path,
        bytes_len: 1024,
        fingerprint: replay_hash.clone(),
        metadata,
    };
    Ok(AssetGenerationResult {
        envelope,
        provider_response_hash: replay_hash,
        artifacts: vec![artifact],
    })
}

fn canonical_prompt(request: &AssetGenerationRequest) -> String {
    let mut constraints = request.negative_constraints.clone();
    constraints.sort();
    format!(
        "asset_id={};prompt={};style={};negative={}",
        request.asset_id,
        request.prompt.trim(),
        request.style_profile.trim(),
        constraints.join("|")
    )
}

fn replay_hash(
    kind: AssetAdapterKind,
    provider: &str,
    canonical_prompt: &str,
    seed: u64,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(format!("{kind:?}").as_bytes());
    hasher.update([0]);
    hasher.update(provider.as_bytes());
    hasher.update([0]);
    hasher.update(canonical_prompt.as_bytes());
    hasher.update([0]);
    hasher.update(seed.to_be_bytes());
    let digest = hasher.finalize();
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

pub fn cache_root_for_workspace(workspace_root: &Path) -> PathBuf {
    workspace_root.join(".wrela").join("asset-factory-cache-v1")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_request() -> AssetGenerationRequest {
        AssetGenerationRequest {
            asset_id: "forest_tree_oak".to_string(),
            prompt: "stylized oak tree".to_string(),
            style_profile: "hero".to_string(),
            seed: 17,
            negative_constraints: vec!["blurry".to_string(), "lowpoly".to_string()],
        }
    }

    #[test]
    fn adapter_generation_is_deterministic() {
        let adapter = ImageGenerationAdapter::new("image-default");
        let request = sample_request();
        let a = adapter.generate(&request).expect("first generation");
        let b = adapter.generate(&request).expect("second generation");
        assert_eq!(a, b);
    }

    #[test]
    fn hybrid_topology_switches_to_cloud_burst() {
        let policy = HybridExecutionPolicy {
            cloud_burst_threshold: 4,
            force_local: false,
        };
        let decision = decide_execution_topology(&policy, 6);
        assert_eq!(decision.topology, ExecutionTopology::CloudBurst);
    }

    #[test]
    fn cache_roundtrip_returns_stored_result() {
        let temp = tempfile::tempdir().expect("tempdir");
        let cache = AssetArtifactCache::new(temp.path());
        let adapter = MeshGenerationAdapter::new("mesh-default");
        let request = sample_request();
        let result = adapter.generate(&request).expect("generate");
        cache.put(&result).expect("cache put");
        let cached = cache
            .get(&result.envelope)
            .expect("cache get")
            .expect("cached value");
        assert_eq!(cached, result);
    }
}
