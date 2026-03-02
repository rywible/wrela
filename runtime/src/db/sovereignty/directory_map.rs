use super::hierarchy::Sovereignty;
use crate::db::shard::directory::ShardOwnershipRecord;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const FNV64_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV64_PRIME: u64 = 0x0000_0100_0000_01b3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DirectoryMapError {
    EmptySovereigntyId,
    UnknownSovereignty(String),
    UnknownRegion {
        sovereignty_id: String,
        region_id: String,
    },
    EmptyLeaderNode,
    NonMonotonicHomeEpoch {
        logical_shard_id: u32,
        current_home_epoch: u64,
        incoming_home_epoch: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedCacheEntry {
    pub signed_epoch: u64,
    pub signature: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlobalDirectoryRecord {
    pub ownership: ShardOwnershipRecord,
    pub cache: SignedCacheEntry,
}

#[derive(Debug, Clone)]
pub struct GlobalDirectoryMap {
    epoch: u64,
    sovereignties: BTreeMap<String, Sovereignty>,
    ownership_by_shard: BTreeMap<u32, GlobalDirectoryRecord>,
}

impl GlobalDirectoryMap {
    pub fn new() -> Self {
        Self {
            epoch: 1,
            sovereignties: BTreeMap::new(),
            ownership_by_shard: BTreeMap::new(),
        }
    }

    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    pub fn register_sovereignty(
        &mut self,
        sovereignty: Sovereignty,
    ) -> Result<(), DirectoryMapError> {
        if sovereignty.sovereignty_id.is_empty() {
            return Err(DirectoryMapError::EmptySovereigntyId);
        }
        self.sovereignties
            .insert(sovereignty.sovereignty_id.clone(), sovereignty);
        self.bump_epoch();
        Ok(())
    }

    pub fn resolve_region(
        &self,
        sovereignty_id: &str,
        region_id: &str,
    ) -> Result<String, DirectoryMapError> {
        let sovereignty = self.sovereignty(sovereignty_id)?;
        let resolved = sovereignty.resolve_region(region_id).ok_or_else(|| {
            DirectoryMapError::UnknownRegion {
                sovereignty_id: normalize_id(sovereignty_id),
                region_id: normalize_id(region_id),
            }
        })?;
        Ok(resolved.region_id.clone())
    }

    pub fn region_ids(&self, sovereignty_id: &str) -> Result<Vec<String>, DirectoryMapError> {
        let sovereignty = self.sovereignty(sovereignty_id)?;
        Ok(sovereignty.region_ids())
    }

    pub fn upsert_ownership(
        &mut self,
        mut ownership: ShardOwnershipRecord,
    ) -> Result<GlobalDirectoryRecord, DirectoryMapError> {
        ownership.sovereignty_id = normalize_id(&ownership.sovereignty_id);
        ownership.home_region = normalize_id(&ownership.home_region);
        ownership.leader_node = normalize_id(&ownership.leader_node);
        if ownership.leader_node.trim().is_empty() {
            return Err(DirectoryMapError::EmptyLeaderNode);
        }
        let sovereignty = self.sovereignty(&ownership.sovereignty_id)?;
        if !sovereignty.has_region(&ownership.home_region) {
            return Err(DirectoryMapError::UnknownRegion {
                sovereignty_id: ownership.sovereignty_id.clone(),
                region_id: ownership.home_region.clone(),
            });
        }
        if let Some(existing) = self.ownership_by_shard.get(&ownership.logical_shard_id)
            && ownership.home_epoch < existing.ownership.home_epoch
        {
            return Err(DirectoryMapError::NonMonotonicHomeEpoch {
                logical_shard_id: ownership.logical_shard_id,
                current_home_epoch: existing.ownership.home_epoch,
                incoming_home_epoch: ownership.home_epoch,
            });
        }

        self.bump_epoch();
        let cache = SignedCacheEntry {
            signed_epoch: self.epoch,
            signature: deterministic_signature(&ownership, self.epoch),
        };
        let record = GlobalDirectoryRecord { ownership, cache };
        self.ownership_by_shard
            .insert(record.ownership.logical_shard_id, record.clone());
        Ok(record)
    }

    pub fn get_ownership(&self, logical_shard_id: u32) -> Option<&GlobalDirectoryRecord> {
        self.ownership_by_shard.get(&logical_shard_id)
    }

    pub fn verify_cache_entry(&self, logical_shard_id: u32) -> bool {
        let Some(record) = self.ownership_by_shard.get(&logical_shard_id) else {
            return false;
        };
        let expected = deterministic_signature(&record.ownership, record.cache.signed_epoch);
        expected == record.cache.signature
    }

    fn sovereignty(&self, sovereignty_id: &str) -> Result<&Sovereignty, DirectoryMapError> {
        let sovereignty_id = normalize_id(sovereignty_id);
        if sovereignty_id.is_empty() {
            return Err(DirectoryMapError::EmptySovereigntyId);
        }
        self.sovereignties
            .get(&sovereignty_id)
            .ok_or(DirectoryMapError::UnknownSovereignty(sovereignty_id))
    }

    fn bump_epoch(&mut self) {
        self.epoch = self.epoch.saturating_add(1);
    }
}

impl Default for GlobalDirectoryMap {
    fn default() -> Self {
        Self::new()
    }
}

pub fn deterministic_signature(ownership: &ShardOwnershipRecord, signed_epoch: u64) -> u64 {
    let mut bytes = Vec::with_capacity(160);
    bytes.extend_from_slice(&ownership.logical_shard_id.to_be_bytes());
    bytes.extend_from_slice(&ownership.active_group_id.to_be_bytes());
    bytes.extend_from_slice(&ownership.home_epoch.to_be_bytes());
    bytes.extend_from_slice(&signed_epoch.to_be_bytes());
    append_string(&mut bytes, &ownership.sovereignty_id);
    append_string(&mut bytes, &ownership.home_region);
    append_string(&mut bytes, &ownership.leader_node);
    fnv64(&bytes)
}

fn append_string(buf: &mut Vec<u8>, value: &str) {
    buf.extend_from_slice(&(value.len() as u32).to_be_bytes());
    buf.extend_from_slice(value.as_bytes());
}

fn fnv64(bytes: &[u8]) -> u64 {
    let mut hash = FNV64_OFFSET_BASIS;
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(FNV64_PRIME);
    }
    hash
}

fn normalize_id(raw: &str) -> String {
    raw.trim().to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::{DirectoryMapError, GlobalDirectoryMap, deterministic_signature};
    use crate::db::routing::sovereignty::{AZ, Node, Region, Sovereignty};
    use crate::db::shard::directory::ShardOwnershipRecord;

    fn sovereignty_fixture() -> Sovereignty {
        Sovereignty::new(
            "core-na",
            vec![
                Region::new(
                    "us",
                    vec![AZ::new("az-1", vec![Node::new("n-us-1").expect("node")]).expect("az")],
                )
                .expect("region"),
                Region::new(
                    "eu",
                    vec![AZ::new("az-2", vec![Node::new("n-eu-1").expect("node")]).expect("az")],
                )
                .expect("region"),
            ],
        )
        .expect("sovereignty")
    }

    fn ownership(home_epoch: u64) -> ShardOwnershipRecord {
        ShardOwnershipRecord {
            logical_shard_id: 7,
            active_group_id: 2,
            sovereignty_id: "core-na".to_string(),
            home_region: "us".to_string(),
            home_epoch,
            leader_node: "n-us-1".to_string(),
        }
    }

    #[test]
    fn map_epoch_is_monotonic_across_writes() {
        let mut map = GlobalDirectoryMap::new();
        let before = map.epoch();
        map.register_sovereignty(sovereignty_fixture())
            .expect("register sovereignty");
        assert!(map.epoch() > before);

        let after_register = map.epoch();
        map.upsert_ownership(ownership(10))
            .expect("upsert ownership");
        assert!(map.epoch() > after_register);
    }

    #[test]
    fn upsert_rejects_non_monotonic_home_epoch() {
        let mut map = GlobalDirectoryMap::new();
        map.register_sovereignty(sovereignty_fixture())
            .expect("register sovereignty");
        map.upsert_ownership(ownership(10))
            .expect("initial ownership");

        let err = map
            .upsert_ownership(ownership(9))
            .expect_err("must reject older home epoch");
        assert_eq!(
            err,
            DirectoryMapError::NonMonotonicHomeEpoch {
                logical_shard_id: 7,
                current_home_epoch: 10,
                incoming_home_epoch: 9,
            }
        );
    }

    #[test]
    fn signed_cache_entries_are_deterministic_and_verifiable() {
        let mut map = GlobalDirectoryMap::new();
        map.register_sovereignty(sovereignty_fixture())
            .expect("register sovereignty");
        let inserted = map.upsert_ownership(ownership(10)).expect("upsert");
        let expected = deterministic_signature(&inserted.ownership, inserted.cache.signed_epoch);
        assert_eq!(inserted.cache.signature, expected);
        assert!(map.verify_cache_entry(7));
    }

    #[test]
    fn resolve_region_uses_primary_on_empty_request() {
        let mut map = GlobalDirectoryMap::new();
        map.register_sovereignty(sovereignty_fixture())
            .expect("register sovereignty");
        assert_eq!(map.resolve_region("core-na", "").expect("resolve"), "eu");
    }
}
