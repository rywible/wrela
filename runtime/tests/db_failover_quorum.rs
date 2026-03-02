use std::collections::BTreeMap;
use wrela_runtime::db::failover::orchestrate_failover;
use wrela_runtime::db::failover::orchestrator::{AsyncFailoverGate, PromotionPolicy};
use wrela_runtime::db::routing::health::{HealthState, MemberRole, NodeHealth};

#[test]
fn region_loss_triggers_deterministic_failover_and_quorum_selection() {
    let nodes = vec![
        NodeHealth {
            node_id: "us-a".to_string(),
            region: "us".to_string(),
            state: HealthState::Unavailable,
            observed_at_ms: 10,
            role: MemberRole::Unknown,
        },
        NodeHealth {
            node_id: "eu-a".to_string(),
            region: "eu".to_string(),
            state: HealthState::Healthy,
            observed_at_ms: 10,
            role: MemberRole::Voter,
        },
        NodeHealth {
            node_id: "ap-a".to_string(),
            region: "ap".to_string(),
            state: HealthState::Degraded,
            observed_at_ms: 10,
            role: MemberRole::Voter,
        },
    ];
    let latency = BTreeMap::from([("eu-a".to_string(), 7), ("ap-a".to_string(), 11)]);

    let decision = orchestrate_failover(&nodes, "us-a", 3, &latency, &PromotionPolicy::default())
        .expect("decision");
    assert_eq!(decision.promoted_leader, Some("eu-a".to_string()));
    assert_eq!(decision.quorum.quorum_size, 2);
    assert_eq!(
        decision.quorum.selected_nodes,
        vec!["eu-a".to_string(), "ap-a".to_string()]
    );
    assert_eq!(decision.timeline[0], "failures_detected=1");
}

#[test]
fn failover_skips_learner_for_leader_promotion() {
    let nodes = vec![
        NodeHealth {
            node_id: "us-a".to_string(),
            region: "us".to_string(),
            state: HealthState::Unavailable,
            observed_at_ms: 10,
            role: MemberRole::Voter,
        },
        NodeHealth {
            node_id: "eu-a".to_string(),
            region: "eu".to_string(),
            state: HealthState::Healthy,
            observed_at_ms: 10,
            role: MemberRole::Learner,
        },
        NodeHealth {
            node_id: "ap-a".to_string(),
            region: "ap".to_string(),
            state: HealthState::Healthy,
            observed_at_ms: 10,
            role: MemberRole::Voter,
        },
    ];
    let latency = BTreeMap::from([("eu-a".to_string(), 1), ("ap-a".to_string(), 5)]);
    let decision = orchestrate_failover(&nodes, "us-a", 3, &latency, &PromotionPolicy::default())
        .expect("decision");
    assert_eq!(
        decision.promoted_leader,
        Some("ap-a".to_string()),
        "learner eu-a must be skipped for leader promotion"
    );
}

#[test]
fn failover_fails_when_only_learners_healthy() {
    // Two learners healthy, one voter unavailable. Quorum selection succeeds
    // (enough healthy nodes) but no voter is available for leader promotion.
    let nodes = vec![
        NodeHealth {
            node_id: "us-a".to_string(),
            region: "us".to_string(),
            state: HealthState::Unavailable,
            observed_at_ms: 10,
            role: MemberRole::Voter,
        },
        NodeHealth {
            node_id: "eu-a".to_string(),
            region: "eu".to_string(),
            state: HealthState::Healthy,
            observed_at_ms: 10,
            role: MemberRole::Learner,
        },
        NodeHealth {
            node_id: "ap-a".to_string(),
            region: "ap".to_string(),
            state: HealthState::Healthy,
            observed_at_ms: 10,
            role: MemberRole::Learner,
        },
    ];
    let latency = BTreeMap::from([("eu-a".to_string(), 1), ("ap-a".to_string(), 3)]);
    let result = orchestrate_failover(&nodes, "us-a", 3, &latency, &PromotionPolicy::default());
    assert!(
        matches!(
            result,
            Err(wrela_runtime::db::failover::FailoverError::NoHealthyLeaderCandidate)
        ),
        "must fail when only learners are healthy, got: {result:?}"
    );
}

#[test]
fn async_failover_candidate_requires_policy_gate_to_promote() {
    let nodes = vec![
        NodeHealth {
            node_id: "us-a".to_string(),
            region: "us".to_string(),
            state: HealthState::Unavailable,
            observed_at_ms: 10,
            role: MemberRole::Voter,
        },
        NodeHealth {
            node_id: "eu-a".to_string(),
            region: "eu".to_string(),
            state: HealthState::Healthy,
            observed_at_ms: 10,
            role: MemberRole::AsyncFailover,
        },
        NodeHealth {
            node_id: "ap-a".to_string(),
            region: "ap".to_string(),
            state: HealthState::Healthy,
            observed_at_ms: 10,
            role: MemberRole::Voter,
        },
    ];
    let latency = BTreeMap::from([("eu-a".to_string(), 1), ("ap-a".to_string(), 3)]);

    let strict_policy = PromotionPolicy {
        allow_async_failover: true,
        max_async_rpo_lag_ms: 10,
        max_async_safe_time_lag_ms: 20,
        async_failover_gate_by_node: BTreeMap::from([(
            "eu-a".to_string(),
            AsyncFailoverGate {
                rpo_lag_ms: 11,
                safe_time_lag_ms: 10,
            },
        )]),
    };
    let strict_decision =
        orchestrate_failover(&nodes, "us-a", 3, &latency, &strict_policy).expect("decision");
    assert_eq!(
        strict_decision.promoted_leader,
        Some("ap-a".to_string()),
        "async-failover must be skipped when policy gate fails"
    );

    let relaxed_policy = PromotionPolicy {
        allow_async_failover: true,
        max_async_rpo_lag_ms: 12,
        max_async_safe_time_lag_ms: 20,
        async_failover_gate_by_node: BTreeMap::from([(
            "eu-a".to_string(),
            AsyncFailoverGate {
                rpo_lag_ms: 11,
                safe_time_lag_ms: 10,
            },
        )]),
    };
    let relaxed_decision =
        orchestrate_failover(&nodes, "us-a", 3, &latency, &relaxed_policy).expect("decision");
    assert_eq!(
        relaxed_decision.promoted_leader,
        Some("eu-a".to_string()),
        "async-failover should promote when gate constraints pass"
    );
}
