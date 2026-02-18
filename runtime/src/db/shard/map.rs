use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShardAssignment {
    pub shard_id: u32,
    pub replicas: Vec<String>,
    pub leader: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShardMap {
    pub version: u64,
    pub assignments: BTreeMap<u32, ShardAssignment>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShardMapError {
    EmptyNodes,
    ZeroShardCount,
    ZeroReplicaCount,
}

pub fn build_initial_shard_map(
    nodes: &[String],
    shard_count: u32,
    replicas_per_shard: usize,
) -> Result<ShardMap, ShardMapError> {
    if nodes.is_empty() {
        return Err(ShardMapError::EmptyNodes);
    }
    if shard_count == 0 {
        return Err(ShardMapError::ZeroShardCount);
    }
    if replicas_per_shard == 0 {
        return Err(ShardMapError::ZeroReplicaCount);
    }

    let mut sorted_nodes = nodes.to_vec();
    sorted_nodes.sort();
    sorted_nodes.dedup();

    let mut assignments = BTreeMap::new();
    for shard_id in 0..shard_count {
        let mut replicas = Vec::with_capacity(replicas_per_shard);
        for offset in 0..replicas_per_shard {
            let idx = (shard_id as usize + offset) % sorted_nodes.len();
            let node = sorted_nodes[idx].clone();
            if !replicas.contains(&node) {
                replicas.push(node);
            }
        }
        let leader = replicas[0].clone();
        assignments.insert(
            shard_id,
            ShardAssignment {
                shard_id,
                replicas,
                leader,
            },
        );
    }

    Ok(ShardMap {
        version: 1,
        assignments,
    })
}

pub fn unique_nodes(map: &ShardMap) -> BTreeSet<String> {
    map.assignments
        .values()
        .flat_map(|assignment| assignment.replicas.iter().cloned())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_map_is_deterministic() {
        let nodes = vec![
            "node-c".to_string(),
            "node-a".to_string(),
            "node-b".to_string(),
        ];
        let map_a = build_initial_shard_map(&nodes, 4, 3).expect("map");
        let map_b = build_initial_shard_map(&nodes, 4, 3).expect("map");
        assert_eq!(map_a, map_b);
        assert_eq!(map_a.assignments.len(), 4);
    }
}
