use crate::db::shard::map::{ShardMap, unique_nodes};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RebalanceMove {
    pub shard_id: u32,
    pub from_node: String,
    pub to_node: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RebalancePlan {
    pub source_version: u64,
    pub target_version: u64,
    pub moves: Vec<RebalanceMove>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RebalanceError {
    EmptyMap,
    EmptyLoadView,
}

pub fn plan_rebalance(
    map: &ShardMap,
    load_by_node: &BTreeMap<String, u64>,
    max_moves: usize,
) -> Result<RebalancePlan, RebalanceError> {
    if map.assignments.is_empty() {
        return Err(RebalanceError::EmptyMap);
    }
    if load_by_node.is_empty() || max_moves == 0 {
        return Err(RebalanceError::EmptyLoadView);
    }

    let nodes = unique_nodes(map);
    let mut ranked: Vec<(String, u64)> = nodes
        .into_iter()
        .map(|node| {
            let load = load_by_node.get(&node).copied().unwrap_or(0);
            (node, load)
        })
        .collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    let hottest = &ranked[0].0;
    let coldest = &ranked[ranked.len() - 1].0;
    if hottest == coldest {
        return Ok(RebalancePlan {
            source_version: map.version,
            target_version: map.version,
            moves: Vec::new(),
        });
    }

    let mut candidates: Vec<u32> = map
        .assignments
        .iter()
        .filter(|(_, assignment)| assignment.leader == *hottest)
        .map(|(shard_id, _)| *shard_id)
        .collect();
    candidates.sort();

    let mut moves = Vec::new();
    for shard_id in candidates.into_iter().take(max_moves) {
        moves.push(RebalanceMove {
            shard_id,
            from_node: hottest.clone(),
            to_node: coldest.clone(),
        });
    }

    let target_version = if moves.is_empty() {
        map.version
    } else {
        map.version.saturating_add(1)
    };

    Ok(RebalancePlan {
        source_version: map.version,
        target_version,
        moves,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::shard::map::build_initial_shard_map;

    #[test]
    fn produces_deterministic_moves() {
        let nodes = vec![
            "node-a".to_string(),
            "node-b".to_string(),
            "node-c".to_string(),
        ];
        let map = build_initial_shard_map(&nodes, 6, 3).expect("map");
        let load = BTreeMap::from([
            ("node-a".to_string(), 1000),
            ("node-b".to_string(), 200),
            ("node-c".to_string(), 300),
        ]);

        let plan = plan_rebalance(&map, &load, 2).expect("plan");
        assert_eq!(plan.source_version, 1);
        assert_eq!(plan.target_version, 2);
        assert!(plan.moves.len() <= 2);
        assert!(plan.moves.iter().all(|mv| mv.from_node == "node-a"));
        assert!(plan.moves.iter().all(|mv| mv.to_node == "node-b"));
    }
}
