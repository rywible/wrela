use std::collections::BTreeMap;

use wrela_runtime::db::quorum::dynamic_select::{
    DynamicQuorumPolicy, SelectionHistory, select_dynamic_quorum,
};
use wrela_runtime::db::routing::health::{HealthState, NodeHealth};
use wrela_runtime::db::routing::health_snapshot::build_health_snapshot;

fn node(id: &str, region: &str, state: HealthState) -> NodeHealth {
    NodeHealth {
        node_id: id.to_string(),
        region: region.to_string(),
        state,
        observed_at_ms: 1,
    }
}

#[test]
fn dynamic_selector_enforces_region_spread_and_degraded_cap() {
    let nodes = vec![
        node("n1", "us", HealthState::Healthy),
        node("n2", "us", HealthState::Degraded),
        node("n3", "eu", HealthState::Healthy),
    ];
    let latency = BTreeMap::from([
        ("n1".to_string(), 5_u64),
        ("n2".to_string(), 3_u64),
        ("n3".to_string(), 9_u64),
    ]);
    let snapshots = build_health_snapshot(&nodes, &latency);

    let policy = DynamicQuorumPolicy {
        desired_voters: 3,
        min_distinct_regions: 2,
        max_degraded_selected: 0,
        required_additional_failures: 0,
        hysteresis_min_rounds: 0,
    };

    let decision = select_dynamic_quorum(&snapshots, &policy, 10, None).expect("decision");
    assert_eq!(
        decision.selected_nodes,
        vec!["n1".to_string(), "n3".to_string()]
    );
}

#[test]
fn dynamic_selector_holds_previous_quorum_with_hysteresis() {
    let nodes = vec![
        node("n1", "us", HealthState::Healthy),
        node("n2", "eu", HealthState::Healthy),
        node("n3", "ap", HealthState::Healthy),
    ];
    let first_latency = BTreeMap::from([
        ("n1".to_string(), 4_u64),
        ("n2".to_string(), 5_u64),
        ("n3".to_string(), 7_u64),
    ]);
    let second_latency = BTreeMap::from([
        ("n1".to_string(), 6_u64),
        ("n2".to_string(), 4_u64),
        ("n3".to_string(), 7_u64),
    ]);

    let policy = DynamicQuorumPolicy {
        desired_voters: 3,
        min_distinct_regions: 2,
        max_degraded_selected: 0,
        required_additional_failures: 0,
        hysteresis_min_rounds: 3,
    };

    let first = select_dynamic_quorum(
        &build_health_snapshot(&nodes, &first_latency),
        &policy,
        50,
        None,
    )
    .expect("first");

    let second = select_dynamic_quorum(
        &build_health_snapshot(&nodes, &second_latency),
        &policy,
        51,
        Some(&SelectionHistory {
            round: 50,
            selected_nodes: first.selected_nodes.clone(),
        }),
    )
    .expect("second");

    assert_eq!(first.selected_nodes, second.selected_nodes);
    assert!(
        second
            .reasons
            .iter()
            .any(|row| row.contains("hysteresis hold"))
    );
}
