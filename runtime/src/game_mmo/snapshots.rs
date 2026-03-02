use crate::game_mmo::events::WorldEventV1;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldSnapshotV1 {
    pub schema_version: u32,
    pub shard_id: String,
    pub revision: u64,
    pub content_hash: String,
    pub event_count: usize,
}

pub fn compile_snapshot(shard_id: &str, revision: u64, events: &[WorldEventV1]) -> WorldSnapshotV1 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for event in events {
        hash = mix(hash, event.event_id);
        hash = mix(hash, event.revision);
        for byte in event.kind.as_bytes() {
            hash = mix(hash, u64::from(*byte));
        }
        for byte in event.payload.as_bytes() {
            hash = mix(hash, u64::from(*byte));
        }
    }
    WorldSnapshotV1 {
        schema_version: 1,
        shard_id: shard_id.to_string(),
        revision,
        content_hash: format!("{hash:016x}"),
        event_count: events.len(),
    }
}

fn mix(current: u64, value: u64) -> u64 {
    let mut hash = current;
    hash ^= value;
    hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    hash
}

#[cfg(test)]
mod tests {
    use super::compile_snapshot;
    use crate::game_mmo::events::WorldEventV1;

    #[test]
    fn snapshot_hash_is_deterministic() {
        let events = vec![WorldEventV1 {
            event_id: 1,
            revision: 1,
            shard_id: "s1".to_string(),
            kind: "spawn".to_string(),
            payload: "{}".to_string(),
        }];
        let a = compile_snapshot("s1", 1, &events);
        let b = compile_snapshot("s1", 1, &events);
        assert_eq!(a.content_hash, b.content_hash);
    }
}
