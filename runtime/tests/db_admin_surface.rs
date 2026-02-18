use std::collections::BTreeMap;

use wrela_runtime::db::admin_api::{
    cluster_health_snapshot, policy_explain_summary, quorum_explain_summary, residency_audit,
};
use wrela_runtime::db::routing::RoutingPolicySpec;
use wrela_runtime::db::routing::health::{HealthState, NodeHealth};
use wrela_runtime::db::security::residency::{ResidencyErrorToken, ResidencyPolicy, ResidencyRule};

fn sample_nodes() -> Vec<NodeHealth> {
    vec![
        NodeHealth {
            node_id: "n1".to_string(),
            region: "us".to_string(),
            state: HealthState::Healthy,
            observed_at_ms: 1,
        },
        NodeHealth {
            node_id: "n2".to_string(),
            region: "eu".to_string(),
            state: HealthState::Healthy,
            observed_at_ms: 1,
        },
        NodeHealth {
            node_id: "n3".to_string(),
            region: "ap".to_string(),
            state: HealthState::Degraded,
            observed_at_ms: 1,
        },
    ]
}

#[test]
fn admin_cluster_snapshot_and_quorum_explain_are_machine_readable() {
    let nodes = sample_nodes();
    let snapshot = cluster_health_snapshot(&nodes);
    assert_eq!(snapshot.total_nodes, 3);
    assert_eq!(snapshot.healthy_nodes, 2);

    let latency = BTreeMap::from([
        ("n1".to_string(), 3_u64),
        ("n2".to_string(), 8_u64),
        ("n3".to_string(), 11_u64),
    ]);
    let quorum = quorum_explain_summary(&nodes, 3, &latency).expect("quorum");
    assert_eq!(quorum.quorum_size, 2);
    assert_eq!(quorum.max_selected_latency_ms, 8);
}

#[test]
fn admin_policy_explain_and_residency_audit_fail_closed() {
    let spec = RoutingPolicySpec {
        policy_id: "orders-v1".to_string(),
        shard_fields: vec!["tenant_id".to_string(), "order_id".to_string()],
        shard_count: 64,
        single_field_waiver_reason: None,
    };
    let policy = policy_explain_summary(&spec).expect("policy explain");
    assert_eq!(policy.policy_id, "orders-v1");

    let residency = ResidencyPolicy::with_rules(vec![ResidencyRule {
        shard: b"orders".to_vec(),
        allowed_regions: vec!["us".to_string()],
    }]);

    let deny = residency_audit(b"orders", "eu", &residency);
    assert!(!deny.allowed);
    assert_eq!(deny.token, Some(ResidencyErrorToken::EgressDeny.as_str()));

    let unsat = residency_audit(b"billing", "us", &residency);
    assert!(!unsat.allowed);
    assert_eq!(
        unsat.token,
        Some(ResidencyErrorToken::EgressPolicyUnsat.as_str())
    );
}
