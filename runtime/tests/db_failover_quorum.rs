use std::collections::BTreeMap;
use wrela_runtime::db::failover::orchestrate_failover;
use wrela_runtime::db::routing::health::{HealthState, NodeHealth};

#[test]
fn region_loss_triggers_deterministic_failover_and_quorum_selection() {
    let nodes = vec![
        NodeHealth {
            node_id: "us-a".to_string(),
            region: "us".to_string(),
            state: HealthState::Unavailable,
            observed_at_ms: 10,
        },
        NodeHealth {
            node_id: "eu-a".to_string(),
            region: "eu".to_string(),
            state: HealthState::Healthy,
            observed_at_ms: 10,
        },
        NodeHealth {
            node_id: "ap-a".to_string(),
            region: "ap".to_string(),
            state: HealthState::Degraded,
            observed_at_ms: 10,
        },
    ];
    let latency = BTreeMap::from([("eu-a".to_string(), 7), ("ap-a".to_string(), 11)]);

    let decision = orchestrate_failover(&nodes, "us-a", 3, &latency).expect("decision");
    assert_eq!(decision.promoted_leader, Some("eu-a".to_string()));
    assert_eq!(decision.quorum.quorum_size, 2);
    assert_eq!(
        decision.quorum.selected_nodes,
        vec!["eu-a".to_string(), "ap-a".to_string()]
    );
    assert_eq!(decision.timeline[0], "failures_detected=1");
}
