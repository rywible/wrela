use crate::world_identity::{SnapshotEpoch, WorldSnapshotHandle, WorldSnapshotId};
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ArtifactPolicyDigestMode {
    None,
    Exact,
    CompatibleRange,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ArtifactReuseKey {
    pub snapshot_id: WorldSnapshotId,
    pub epoch: SnapshotEpoch,
    pub contract_id: Option<String>,
    pub logical_schema: String,
    pub compatibility_hash: u64,
    pub policy_digest: Option<u64>,
    pub policy_mode: ArtifactPolicyDigestMode,
}

impl ArtifactReuseKey {
    pub fn new(
        snapshot: &WorldSnapshotHandle,
        contract_id: Option<SmolStr>,
        logical_schema: SmolStr,
        compatibility_hash: u64,
        policy_digest: Option<u64>,
        policy_mode: ArtifactPolicyDigestMode,
    ) -> Self {
        let policy_digest = match policy_mode {
            ArtifactPolicyDigestMode::None => None,
            ArtifactPolicyDigestMode::Exact | ArtifactPolicyDigestMode::CompatibleRange => {
                policy_digest
            }
        };
        Self {
            snapshot_id: snapshot.snapshot_id(),
            epoch: snapshot.epoch(),
            contract_id: contract_id.map(|value| value.to_string()),
            logical_schema: logical_schema.to_string(),
            compatibility_hash,
            policy_digest,
            policy_mode,
        }
    }

    pub fn compatible_with(&self, candidate: &Self) -> bool {
        self.snapshot_id == candidate.snapshot_id
            && self.epoch == candidate.epoch
            && self.contract_id == candidate.contract_id
            && self.logical_schema == candidate.logical_schema
            && self.compatibility_hash == candidate.compatibility_hash
            && self.policy_is_compatible(candidate)
    }

    fn policy_is_compatible(&self, candidate: &Self) -> bool {
        self.policy_mode == candidate.policy_mode && self.policy_digest == candidate.policy_digest
    }
}
