use std::collections::BTreeMap;

use wrela_runtime::db::autopilot::{
    DEFAULT_MAX_SKEW_RATIO, SafetySimulationInput, evaluate_safety_simulation,
};
use wrela_runtime::db::quorum::simulate_quorum_safety;
use wrela_runtime::db::routing::health::{HealthState, NodeHealth};
use wrela_runtime::db::routing::{RoutingPolicySpec, compile_policy};

#[test]
fn routing_policy_compile_is_deterministic_and_routes_stably() {
    let spec = RoutingPolicySpec {
        policy_id: "orders-v1".to_string(),
        shard_fields: vec!["tenant_id".to_string(), "order_id".to_string()],
        shard_count: 64,
        single_field_waiver_reason: None,
    };

    let compiled_a = compile_policy(&spec).expect("compile");
    let compiled_b = compile_policy(&spec).expect("compile");
    assert_eq!(compiled_a.policy_hash, compiled_b.policy_hash);

    let row = BTreeMap::from([
        ("tenant_id".to_string(), b"tenant-7".to_vec()),
        ("order_id".to_string(), b"order-120".to_vec()),
    ]);

    let route_a = compiled_a.route_row(&row).expect("route");
    let route_b = compiled_b.route_row(&row).expect("route");
    assert_eq!(route_a, route_b);
    assert!(route_a.shard_id < compiled_a.shard_key_policy().shard_count());
}

#[test]
fn quorum_simulation_is_deterministic_and_applies_guardrails() {
    let nodes = vec![
        NodeHealth {
            node_id: "us-a".to_string(),
            region: "us".to_string(),
            state: HealthState::Healthy,
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
    let latency = BTreeMap::from([
        ("us-a".to_string(), 9_u64),
        ("eu-a".to_string(), 6_u64),
        ("ap-a".to_string(), 8_u64),
    ]);

    let sim_a = simulate_quorum_safety(&nodes, 3, &latency, 1, 1).expect("sim");
    let sim_b = simulate_quorum_safety(&nodes, 3, &latency, 1, 1).expect("sim");
    assert_eq!(sim_a, sim_b);
    assert!(sim_a.passes);

    let strict = simulate_quorum_safety(&nodes, 3, &latency, 2, 0).expect("sim");
    assert!(!strict.passes);
}

#[test]
fn autopilot_safety_simulation_combines_skew_and_quorum_results() {
    let loads = BTreeMap::from([
        ("shard-a".to_string(), 4800_u64),
        ("shard-b".to_string(), 700_u64),
        ("shard-c".to_string(), 600_u64),
    ]);
    let nodes = vec![
        NodeHealth {
            node_id: "us-a".to_string(),
            region: "us".to_string(),
            state: HealthState::Healthy,
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
    let latency = BTreeMap::from([
        ("us-a".to_string(), 9_u64),
        ("eu-a".to_string(), 6_u64),
        ("ap-a".to_string(), 8_u64),
    ]);

    let decision = evaluate_safety_simulation(SafetySimulationInput {
        shard_loads: &loads,
        skew_threshold: DEFAULT_MAX_SKEW_RATIO,
        quorum_candidates: &nodes,
        desired_voters: 3,
        latency_hint_ms: &latency,
        required_additional_failures: 2,
        max_degraded_selected: 0,
    })
    .expect("decision");

    assert!(!decision.passes);
    assert_eq!(decision.reasons.len(), 3);
    assert!(
        decision
            .reasons
            .iter()
            .any(|reason| reason.contains("skew ratio"))
    );
    assert!(
        decision
            .reasons
            .iter()
            .any(|reason| reason.contains("survivable additional failures"))
    );
    assert!(
        decision
            .reasons
            .iter()
            .any(|reason| reason.contains("degraded selected"))
    );
}
