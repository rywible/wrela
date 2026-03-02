use std::collections::BTreeMap;
use wrela_runtime::db::shard::{build_initial_shard_map, plan_rebalance};

#[test]
fn shard_map_round_robin_is_stable() {
    let nodes = vec![
        "node-2".to_string(),
        "node-1".to_string(),
        "node-3".to_string(),
    ];
    let map_a = build_initial_shard_map(&nodes, 8, 3).expect("map");
    let map_b = build_initial_shard_map(&nodes, 8, 3).expect("map");
    assert_eq!(map_a, map_b);
}

#[test]
fn rebalancer_moves_hottest_leaders_to_coldest_node() {
    let nodes = vec![
        "node-a".to_string(),
        "node-b".to_string(),
        "node-c".to_string(),
    ];
    let map = build_initial_shard_map(&nodes, 9, 3).expect("map");
    let load = BTreeMap::from([
        ("node-a".to_string(), 500),
        ("node-b".to_string(), 50),
        ("node-c".to_string(), 200),
    ]);

    let plan = plan_rebalance(&map, &load, 3).expect("plan");
    assert_eq!(plan.target_version, map.version + 1);
    assert!(plan.moves.iter().all(|mv| mv.from_node == "node-a"));
    assert!(plan.moves.iter().all(|mv| mv.to_node == "node-b"));
}
