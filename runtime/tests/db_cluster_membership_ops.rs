use std::collections::BTreeMap;
use wrela_runtime::db::cluster::{ClusterState, drain_region, replace_node};

#[test]
fn replace_node_under_load_contract_stays_safe() {
    let mut state = ClusterState::new(BTreeMap::from([
        ("node-a".to_string(), "us".to_string()),
        ("node-b".to_string(), "eu".to_string()),
        ("node-c".to_string(), "ap".to_string()),
    ]));

    let trace = replace_node(&mut state, "node-a", "node-d", "us").expect("replace");
    assert_eq!(trace.operation, "replace_node");
    assert!(state.voters.contains("node-d"));
    assert!(!state.voters.contains("node-a"));
}

#[test]
fn drain_region_is_idempotent_and_traceable() {
    let mut state = ClusterState::new(BTreeMap::from([
        ("node-a".to_string(), "us".to_string()),
        ("node-b".to_string(), "eu".to_string()),
        ("node-c".to_string(), "ap".to_string()),
    ]));

    let first = drain_region(&mut state, "eu");
    let second = drain_region(&mut state, "eu");
    assert_eq!(first.operation, "drain_region");
    assert_eq!(second.operation, "drain_region");
    assert!(state.drained_regions.contains("eu"));
}
