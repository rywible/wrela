use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldShardContractV1 {
    pub schema_version: u32,
    pub shard_id: String,
    pub owner_id: String,
    pub style_pack_id: String,
    pub capacity: u32,
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortalContractV1 {
    pub source_shard_id: String,
    pub target_shard_id: String,
    pub required_item_policy: String,
    pub currency_policy: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct WorldShardRegistryV1 {
    pub shards: BTreeMap<String, WorldShardContractV1>,
    pub portals: Vec<PortalContractV1>,
}

impl WorldShardRegistryV1 {
    pub fn create_shard(
        &mut self,
        shard_id: &str,
        owner_id: &str,
        style_pack_id: &str,
        capacity: u32,
    ) -> Result<(), String> {
        if self.shards.contains_key(shard_id) {
            return Err(format!("shard '{}' already exists", shard_id));
        }
        self.shards.insert(
            shard_id.to_string(),
            WorldShardContractV1 {
                schema_version: 1,
                shard_id: shard_id.to_string(),
                owner_id: owner_id.to_string(),
                style_pack_id: style_pack_id.to_string(),
                capacity,
                revision: 1,
            },
        );
        Ok(())
    }

    pub fn fork_shard(&mut self, source_shard_id: &str, fork_shard_id: &str) -> Result<(), String> {
        if self.shards.contains_key(fork_shard_id) {
            return Err(format!("fork target '{}' already exists", fork_shard_id));
        }
        let Some(source) = self.shards.get(source_shard_id).cloned() else {
            return Err(format!("source shard '{}' is missing", source_shard_id));
        };
        self.shards.insert(
            fork_shard_id.to_string(),
            WorldShardContractV1 {
                schema_version: 1,
                shard_id: fork_shard_id.to_string(),
                revision: source.revision,
                ..source
            },
        );
        Ok(())
    }

    pub fn apply_revision(&mut self, shard_id: &str) -> Result<u64, String> {
        let Some(shard) = self.shards.get_mut(shard_id) else {
            return Err(format!("shard '{}' is missing", shard_id));
        };
        shard.revision = shard.revision.saturating_add(1);
        Ok(shard.revision)
    }

    pub fn rollback_to_revision(&mut self, shard_id: &str, revision: u64) -> Result<(), String> {
        let Some(shard) = self.shards.get_mut(shard_id) else {
            return Err(format!("shard '{}' is missing", shard_id));
        };
        if revision == 0 || revision > shard.revision {
            return Err(format!(
                "invalid rollback revision {} for shard '{}' with current revision {}",
                revision, shard_id, shard.revision
            ));
        }
        shard.revision = revision;
        Ok(())
    }

    pub fn add_portal(&mut self, portal: PortalContractV1) -> Result<(), String> {
        if !self.shards.contains_key(portal.source_shard_id.as_str()) {
            return Err(format!(
                "portal source shard '{}' missing",
                portal.source_shard_id
            ));
        }
        if !self.shards.contains_key(portal.target_shard_id.as_str()) {
            return Err(format!(
                "portal target shard '{}' missing",
                portal.target_shard_id
            ));
        }
        self.portals.push(portal);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{PortalContractV1, WorldShardRegistryV1};

    #[test]
    fn shard_create_fork_and_rollback_flow() {
        let mut registry = WorldShardRegistryV1::default();
        registry
            .create_shard("home", "owner-1", "style-a", 1500)
            .expect("create shard");
        registry
            .fork_shard("home", "home-fork")
            .expect("fork shard");
        let rev = registry.apply_revision("home").expect("bump revision");
        assert_eq!(rev, 2);
        registry
            .rollback_to_revision("home", 1)
            .expect("rollback to revision");
        assert_eq!(registry.shards["home"].revision, 1);
    }

    #[test]
    fn portal_requires_both_shards() {
        let mut registry = WorldShardRegistryV1::default();
        registry
            .create_shard("a", "owner", "style", 1000)
            .expect("create shard a");
        registry
            .create_shard("b", "owner", "style", 1000)
            .expect("create shard b");
        registry
            .add_portal(PortalContractV1 {
                source_shard_id: "a".to_string(),
                target_shard_id: "b".to_string(),
                required_item_policy: "none".to_string(),
                currency_policy: "allow".to_string(),
            })
            .expect("add portal");
        assert_eq!(registry.portals.len(), 1);
    }
}
