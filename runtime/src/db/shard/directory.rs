use crate::db::types::BatchOp;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const FNV64_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV64_PRIME: u64 = 0x0000_0100_0000_01b3;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogicalShard {
    pub shard_id: u32,
    pub start_hash_inclusive: u64,
    pub end_hash_inclusive: u64,
    pub active_group_id: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShardDirectorySnapshot {
    pub epoch: u64,
    pub next_shard_id: u32,
    pub active_group_count: u32,
    pub shards: Vec<LogicalShard>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShardOwnershipRecord {
    pub logical_shard_id: u32,
    pub active_group_id: u32,
    pub sovereignty_id: String,
    pub home_region: String,
    pub home_epoch: u64,
    pub leader_node: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShardRoute {
    pub logical_shard_id: u32,
    pub active_group_id: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShardDirectoryError {
    InvalidInitialLogicalShardCount,
    InvalidInitialActiveGroupCount,
    RouteMiss,
    UnknownShard(u32),
    UnknownActiveGroup(u32),
    UnsplittableShard(u32),
    NonAdjacentMerge(u32, u32),
    InvalidSnapshot(&'static str),
}

#[derive(Debug, Clone)]
pub struct ShardDirectory {
    epoch: u64,
    next_shard_id: u32,
    active_group_count: u32,
    shards: Vec<LogicalShard>,
}

impl ShardDirectory {
    pub fn new(
        initial_logical_shards: u32,
        initial_active_groups: u32,
    ) -> Result<Self, ShardDirectoryError> {
        if initial_logical_shards == 0 {
            return Err(ShardDirectoryError::InvalidInitialLogicalShardCount);
        }
        if initial_active_groups == 0 {
            return Err(ShardDirectoryError::InvalidInitialActiveGroupCount);
        }
        let mut shards = Vec::with_capacity(initial_logical_shards as usize);
        let space = (u64::MAX as u128) + 1;
        for idx in 0..initial_logical_shards {
            let start = ((idx as u128) * space / (initial_logical_shards as u128)) as u64;
            let end = if idx + 1 == initial_logical_shards {
                u64::MAX
            } else {
                (((idx as u128 + 1) * space / (initial_logical_shards as u128)) - 1) as u64
            };
            shards.push(LogicalShard {
                shard_id: idx,
                start_hash_inclusive: start,
                end_hash_inclusive: end,
                active_group_id: idx % initial_active_groups,
            });
        }
        Ok(Self {
            epoch: 1,
            next_shard_id: initial_logical_shards,
            active_group_count: initial_active_groups,
            shards,
        })
    }

    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    pub fn logical_shard_count(&self) -> u32 {
        self.shards.len() as u32
    }

    pub fn active_group_count(&self) -> u32 {
        self.active_group_count
    }

    pub fn snapshot(&self) -> ShardDirectorySnapshot {
        ShardDirectorySnapshot {
            epoch: self.epoch,
            next_shard_id: self.next_shard_id,
            active_group_count: self.active_group_count,
            shards: self.shards.clone(),
        }
    }

    pub fn from_snapshot(snapshot: ShardDirectorySnapshot) -> Result<Self, ShardDirectoryError> {
        if snapshot.active_group_count == 0 {
            return Err(ShardDirectoryError::InvalidSnapshot(
                "active_group_count must be > 0",
            ));
        }
        if snapshot.shards.is_empty() {
            return Err(ShardDirectoryError::InvalidSnapshot(
                "snapshot requires at least one shard",
            ));
        }

        let mut sorted = snapshot.shards.clone();
        sorted.sort_by_key(|shard| shard.start_hash_inclusive);
        if sorted[0].start_hash_inclusive != 0 {
            return Err(ShardDirectoryError::InvalidSnapshot(
                "shard hash space must start at 0",
            ));
        }
        for window in sorted.windows(2) {
            let left = &window[0];
            let right = &window[1];
            if left.end_hash_inclusive.saturating_add(1) != right.start_hash_inclusive {
                return Err(ShardDirectoryError::InvalidSnapshot(
                    "shard hash ranges must be contiguous",
                ));
            }
            if right.active_group_id >= snapshot.active_group_count {
                return Err(ShardDirectoryError::InvalidSnapshot(
                    "shard active group out of bounds",
                ));
            }
        }
        if sorted
            .last()
            .map(|shard| shard.end_hash_inclusive)
            .unwrap_or(0)
            != u64::MAX
        {
            return Err(ShardDirectoryError::InvalidSnapshot(
                "shard hash space must end at u64::MAX",
            ));
        }
        if sorted
            .iter()
            .any(|shard| shard.active_group_id >= snapshot.active_group_count)
        {
            return Err(ShardDirectoryError::InvalidSnapshot(
                "shard active group out of bounds",
            ));
        }

        Ok(Self {
            epoch: snapshot.epoch.max(1),
            next_shard_id: snapshot.next_shard_id,
            active_group_count: snapshot.active_group_count,
            shards: sorted,
        })
    }

    pub fn ownership_records(
        &self,
        sovereignty_id: &str,
        home_region: &str,
        leader_node: &str,
    ) -> Vec<ShardOwnershipRecord> {
        let sovereignty_id = sovereignty_id.trim().to_ascii_lowercase();
        let home_region = home_region.trim().to_ascii_lowercase();
        let leader_node = leader_node.trim().to_ascii_lowercase();
        self.shards
            .iter()
            .map(|entry| ShardOwnershipRecord {
                logical_shard_id: entry.shard_id,
                active_group_id: entry.active_group_id,
                sovereignty_id: sovereignty_id.clone(),
                home_region: home_region.clone(),
                home_epoch: self.epoch,
                leader_node: leader_node.clone(),
            })
            .collect()
    }

    pub fn shards(&self) -> &[LogicalShard] {
        &self.shards
    }

    pub fn add_active_group(&mut self) -> u32 {
        let new_group = self.active_group_count;
        self.active_group_count = self.active_group_count.saturating_add(1);
        self.epoch = self.epoch.saturating_add(1);
        new_group
    }

    pub fn reassign_shard_group(
        &mut self,
        shard_id: u32,
        target_group_id: u32,
    ) -> Result<(), ShardDirectoryError> {
        if target_group_id >= self.active_group_count {
            return Err(ShardDirectoryError::UnknownActiveGroup(target_group_id));
        }
        let Some(shard) = self
            .shards
            .iter_mut()
            .find(|shard| shard.shard_id == shard_id)
        else {
            return Err(ShardDirectoryError::UnknownShard(shard_id));
        };
        if shard.active_group_id != target_group_id {
            shard.active_group_id = target_group_id;
            self.epoch = self.epoch.saturating_add(1);
        }
        Ok(())
    }

    pub fn route_key(
        &self,
        namespace: &[u8],
        key: &[u8],
    ) -> Result<ShardRoute, ShardDirectoryError> {
        let hash = hash_namespace_key(namespace, key);
        self.route_hash(hash)
    }

    pub fn route_batch(&self, ops: &[BatchOp]) -> Result<ShardRoute, ShardDirectoryError> {
        let mut selected: Option<ShardRoute> = None;
        for op in ops {
            let route = match op {
                BatchOp::Put { namespace, key, .. } => self.route_key(namespace, key)?,
                BatchOp::Delete { namespace, key, .. } => self.route_key(namespace, key)?,
            };
            if let Some(existing) = &selected {
                if existing.logical_shard_id != route.logical_shard_id {
                    return Err(ShardDirectoryError::RouteMiss);
                }
            } else {
                selected = Some(route);
            }
        }
        selected.ok_or(ShardDirectoryError::RouteMiss)
    }

    pub fn split_shard(&mut self, shard_id: u32) -> Result<(u32, u32), ShardDirectoryError> {
        let Some(idx) = self
            .shards
            .iter()
            .position(|entry| entry.shard_id == shard_id)
        else {
            return Err(ShardDirectoryError::UnknownShard(shard_id));
        };
        let parent = self.shards[idx].clone();
        if parent.start_hash_inclusive == parent.end_hash_inclusive {
            return Err(ShardDirectoryError::UnsplittableShard(shard_id));
        }
        let midpoint = parent.start_hash_inclusive
            + (parent.end_hash_inclusive - parent.start_hash_inclusive) / 2;
        let right_start = midpoint.saturating_add(1);
        let new_shard_id = self.next_shard_id;
        self.next_shard_id = self.next_shard_id.saturating_add(1);
        self.shards[idx].end_hash_inclusive = midpoint;
        self.shards.insert(
            idx + 1,
            LogicalShard {
                shard_id: new_shard_id,
                start_hash_inclusive: right_start,
                end_hash_inclusive: parent.end_hash_inclusive,
                active_group_id: self.least_loaded_active_group(),
            },
        );
        self.epoch = self.epoch.saturating_add(1);
        Ok((shard_id, new_shard_id))
    }

    pub fn merge_shards(
        &mut self,
        left_shard_id: u32,
        right_shard_id: u32,
    ) -> Result<u32, ShardDirectoryError> {
        let Some(left_idx) = self
            .shards
            .iter()
            .position(|entry| entry.shard_id == left_shard_id)
        else {
            return Err(ShardDirectoryError::UnknownShard(left_shard_id));
        };
        let Some(right_idx) = self
            .shards
            .iter()
            .position(|entry| entry.shard_id == right_shard_id)
        else {
            return Err(ShardDirectoryError::UnknownShard(right_shard_id));
        };
        if left_idx == right_idx {
            return Err(ShardDirectoryError::NonAdjacentMerge(
                left_shard_id,
                right_shard_id,
            ));
        }

        let (first_idx, second_idx) = if left_idx < right_idx {
            (left_idx, right_idx)
        } else {
            (right_idx, left_idx)
        };
        let first = self.shards[first_idx].clone();
        let second = self.shards[second_idx].clone();
        let adjacent = first.end_hash_inclusive.saturating_add(1) == second.start_hash_inclusive;
        if !adjacent {
            return Err(ShardDirectoryError::NonAdjacentMerge(
                first.shard_id,
                second.shard_id,
            ));
        }

        self.shards[first_idx].end_hash_inclusive = second.end_hash_inclusive;
        self.shards.remove(second_idx);
        self.epoch = self.epoch.saturating_add(1);
        Ok(self.shards[first_idx].shard_id)
    }

    fn route_hash(&self, hash: u64) -> Result<ShardRoute, ShardDirectoryError> {
        let Some(entry) = self
            .shards
            .iter()
            .find(|entry| hash >= entry.start_hash_inclusive && hash <= entry.end_hash_inclusive)
        else {
            return Err(ShardDirectoryError::RouteMiss);
        };
        Ok(ShardRoute {
            logical_shard_id: entry.shard_id,
            active_group_id: entry.active_group_id,
        })
    }

    fn least_loaded_active_group(&self) -> u32 {
        let mut counts = BTreeMap::new();
        for group in 0..self.active_group_count {
            counts.insert(group, 0usize);
        }
        for shard in &self.shards {
            *counts.entry(shard.active_group_id).or_default() += 1;
        }
        counts
            .into_iter()
            .min_by_key(|(group, count)| (*count, *group))
            .map(|(group, _)| group)
            .unwrap_or(0)
    }
}

fn hash_namespace_key(namespace: &[u8], key: &[u8]) -> u64 {
    let mut bytes = Vec::with_capacity(namespace.len() + key.len() + 8);
    bytes.extend_from_slice(&(namespace.len() as u32).to_be_bytes());
    bytes.extend_from_slice(namespace);
    bytes.extend_from_slice(&(key.len() as u32).to_be_bytes());
    bytes.extend_from_slice(key);
    fnv64(&bytes)
}

fn fnv64(bytes: &[u8]) -> u64 {
    let mut hash = FNV64_OFFSET_BASIS;
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(FNV64_PRIME);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;

    #[test]
    fn deterministic_route_for_same_key() {
        let dir = ShardDirectory::new(16, 4).expect("dir");
        let a = dir.route_key(b"core", b"k1").expect("route");
        let b = dir.route_key(b"core", b"k1").expect("route");
        assert_eq!(a, b);
    }

    #[test]
    fn split_increases_epoch_and_shard_count() {
        let mut dir = ShardDirectory::new(8, 2).expect("dir");
        let before_epoch = dir.epoch();
        let before_count = dir.logical_shard_count();
        let target = dir
            .route_key(b"core", b"hot")
            .expect("route")
            .logical_shard_id;
        let (_left, _right) = dir.split_shard(target).expect("split");
        assert_eq!(dir.logical_shard_count(), before_count + 1);
        assert_eq!(dir.epoch(), before_epoch + 1);
    }

    #[test]
    fn merge_adjacent_shards_reduces_count() {
        let mut dir = ShardDirectory::new(4, 2).expect("dir");
        let left = dir.shards[0].shard_id;
        let right = dir.shards[1].shard_id;
        let before = dir.logical_shard_count();
        dir.merge_shards(left, right).expect("merge");
        assert_eq!(dir.logical_shard_count(), before - 1);
    }

    #[test]
    fn route_batch_rejects_multi_shard_payload() {
        let dir = ShardDirectory::new(64, 4).expect("dir");
        let ops = vec![
            BatchOp::Put {
                namespace: Bytes::from_static(b"core"),
                key: Bytes::from_static(b"a"),
                value: Bytes::from_static(b"1"),
                expected_version: None,
            },
            BatchOp::Put {
                namespace: Bytes::from_static(b"core"),
                key: Bytes::from_static(b"b"),
                value: Bytes::from_static(b"2"),
                expected_version: None,
            },
        ];
        let result = dir.route_batch(&ops);
        if let Ok(route) = result {
            let first = dir.route_key(b"core", b"a").expect("first");
            let second = dir.route_key(b"core", b"b").expect("second");
            assert!(
                first.logical_shard_id == second.logical_shard_id
                    && route.logical_shard_id == first.logical_shard_id
            );
        }
    }

    #[test]
    fn ownership_records_include_sovereignty_metadata() {
        let dir = ShardDirectory::new(8, 2).expect("dir");
        let records = dir.ownership_records(" Core-NA ", " Us ", " NODE-9 ");
        assert_eq!(records.len(), dir.logical_shard_count() as usize);
        assert!(
            records
                .iter()
                .all(|record| record.sovereignty_id == "core-na")
        );
        assert!(records.iter().all(|record| record.home_region == "us"));
        assert!(
            records
                .iter()
                .all(|record| record.home_epoch == dir.epoch())
        );
        assert!(records.iter().all(|record| record.leader_node == "node-9"));
    }
}
