use crate::db::quorum::{QuorumSelection, QuorumSelectionError, select_nearest_healthy_quorum};
use crate::db::routing::health::{HealthState, MemberRole, NodeHealth};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AsyncFailoverGate {
    pub rpo_lag_ms: u64,
    pub safe_time_lag_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromotionPolicy {
    pub allow_async_failover: bool,
    pub max_async_rpo_lag_ms: u64,
    pub max_async_safe_time_lag_ms: u64,
    pub async_failover_gate_by_node: BTreeMap<String, AsyncFailoverGate>,
}

impl PromotionPolicy {
    pub fn voter_only() -> Self {
        Self {
            allow_async_failover: false,
            max_async_rpo_lag_ms: 0,
            max_async_safe_time_lag_ms: 0,
            async_failover_gate_by_node: BTreeMap::new(),
        }
    }

    fn is_promotable(&self, node: &NodeHealth) -> bool {
        match node.role {
            MemberRole::Voter => true,
            MemberRole::Unknown | MemberRole::Learner => false,
            MemberRole::AsyncFailover => {
                if !self.allow_async_failover {
                    return false;
                }
                let Some(gate) = self.async_failover_gate_by_node.get(&node.node_id) else {
                    return false;
                };
                gate.rpo_lag_ms <= self.max_async_rpo_lag_ms
                    && gate.safe_time_lag_ms <= self.max_async_safe_time_lag_ms
            }
        }
    }
}

impl Default for PromotionPolicy {
    fn default() -> Self {
        Self::voter_only()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FailoverDecision {
    pub failed_nodes: Vec<String>,
    pub promoted_leader: Option<String>,
    pub quorum: QuorumSelection,
    pub timeline: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FailoverError {
    NoHealthyLeaderCandidate,
    Quorum(QuorumSelectionError),
}

pub fn orchestrate_failover(
    nodes: &[NodeHealth],
    current_leader: &str,
    desired_voters: usize,
    latency_hint_ms: &BTreeMap<String, u64>,
    promotion_policy: &PromotionPolicy,
) -> Result<FailoverDecision, FailoverError> {
    let failed_nodes: Vec<String> = nodes
        .iter()
        .filter(|n| n.state == HealthState::Unavailable)
        .map(|n| n.node_id.clone())
        .collect();

    let quorum = select_nearest_healthy_quorum(nodes, desired_voters, latency_hint_ms)
        .map_err(FailoverError::Quorum)?;

    let node_by_id: BTreeMap<&str, &NodeHealth> = nodes
        .iter()
        .map(|node| (node.node_id.as_str(), node))
        .collect();

    let promoted_leader = if failed_nodes.iter().any(|n| n == current_leader) {
        quorum.selected_nodes.iter().find_map(|node_id| {
            node_by_id
                .get(node_id.as_str())
                .filter(|node| promotion_policy.is_promotable(node))
                .map(|node| node.node_id.clone())
        })
    } else {
        Some(current_leader.to_string())
    };

    let promoted_leader = promoted_leader.ok_or(FailoverError::NoHealthyLeaderCandidate)?;

    let mut timeline = Vec::new();
    timeline.push(format!("failures_detected={}", failed_nodes.len()));
    timeline.push(format!(
        "quorum_selected={}",
        quorum.selected_nodes.join(",")
    ));
    timeline.push(format!("leader={}", promoted_leader));

    Ok(FailoverDecision {
        failed_nodes,
        promoted_leader: Some(promoted_leader),
        quorum,
        timeline,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn promotes_new_leader_when_current_is_unavailable() {
        let nodes = vec![
            NodeHealth {
                node_id: "n1".to_string(),
                region: "us".to_string(),
                state: HealthState::Unavailable,
                observed_at_ms: 1,
                role: MemberRole::Voter,
            },
            NodeHealth {
                node_id: "n2".to_string(),
                region: "eu".to_string(),
                state: HealthState::Healthy,
                observed_at_ms: 1,
                role: MemberRole::Voter,
            },
            NodeHealth {
                node_id: "n3".to_string(),
                region: "ap".to_string(),
                state: HealthState::Healthy,
                observed_at_ms: 1,
                role: MemberRole::Voter,
            },
        ];
        let latency = BTreeMap::from([("n2".to_string(), 5), ("n3".to_string(), 8)]);
        let decision = orchestrate_failover(&nodes, "n1", 3, &latency, &PromotionPolicy::default())
            .expect("decision");
        assert_eq!(decision.promoted_leader, Some("n2".to_string()));
        assert_eq!(decision.quorum.quorum_size, 2);
        assert_eq!(decision.failed_nodes, vec!["n1".to_string()]);
    }

    #[test]
    fn failover_rejects_unknown_role_for_leader_promotion() {
        let nodes = vec![
            NodeHealth {
                node_id: "n1".to_string(),
                region: "us".to_string(),
                state: HealthState::Unavailable,
                observed_at_ms: 1,
                role: MemberRole::Voter,
            },
            NodeHealth {
                node_id: "n2".to_string(),
                region: "eu".to_string(),
                state: HealthState::Healthy,
                observed_at_ms: 1,
                role: MemberRole::Unknown,
            },
            NodeHealth {
                node_id: "n3".to_string(),
                region: "ap".to_string(),
                state: HealthState::Healthy,
                observed_at_ms: 1,
                role: MemberRole::Unknown,
            },
        ];
        let latency = BTreeMap::from([("n2".to_string(), 1), ("n3".to_string(), 3)]);
        let result = orchestrate_failover(&nodes, "n1", 3, &latency, &PromotionPolicy::default());
        assert!(
            matches!(result, Err(FailoverError::NoHealthyLeaderCandidate)),
            "nodes with role=Unknown must not be promoted to leader"
        );
    }

    #[test]
    fn failover_skips_learner_for_leader_promotion() {
        let nodes = vec![
            NodeHealth {
                node_id: "n1".to_string(),
                region: "us".to_string(),
                state: HealthState::Unavailable,
                observed_at_ms: 1,
                role: MemberRole::Voter,
            },
            NodeHealth {
                node_id: "n2".to_string(),
                region: "eu".to_string(),
                state: HealthState::Healthy,
                observed_at_ms: 1,
                role: MemberRole::Learner,
            },
            NodeHealth {
                node_id: "n3".to_string(),
                region: "ap".to_string(),
                state: HealthState::Healthy,
                observed_at_ms: 1,
                role: MemberRole::Voter,
            },
        ];
        let latency = BTreeMap::from([("n2".to_string(), 1), ("n3".to_string(), 5)]);
        let decision = orchestrate_failover(&nodes, "n1", 3, &latency, &PromotionPolicy::default())
            .expect("decision");
        // n2 is closest but is a learner — n3 (voter) must be promoted instead.
        assert_eq!(decision.promoted_leader, Some("n3".to_string()));
    }

    #[test]
    fn failover_fails_when_only_learners_healthy() {
        let nodes = vec![
            NodeHealth {
                node_id: "n1".to_string(),
                region: "us".to_string(),
                state: HealthState::Unavailable,
                observed_at_ms: 1,
                role: MemberRole::Voter,
            },
            NodeHealth {
                node_id: "n2".to_string(),
                region: "eu".to_string(),
                state: HealthState::Healthy,
                observed_at_ms: 1,
                role: MemberRole::Learner,
            },
            NodeHealth {
                node_id: "n3".to_string(),
                region: "ap".to_string(),
                state: HealthState::Healthy,
                observed_at_ms: 1,
                role: MemberRole::Learner,
            },
        ];
        let latency = BTreeMap::from([("n2".to_string(), 1), ("n3".to_string(), 3)]);
        let result = orchestrate_failover(&nodes, "n1", 3, &latency, &PromotionPolicy::default());
        assert!(
            matches!(result, Err(FailoverError::NoHealthyLeaderCandidate)),
            "must fail when only learners are healthy"
        );
    }

    #[test]
    fn failover_promotes_async_failover_when_gate_passes() {
        let nodes = vec![
            NodeHealth {
                node_id: "n1".to_string(),
                region: "us".to_string(),
                state: HealthState::Unavailable,
                observed_at_ms: 1,
                role: MemberRole::Voter,
            },
            NodeHealth {
                node_id: "n2".to_string(),
                region: "eu".to_string(),
                state: HealthState::Healthy,
                observed_at_ms: 1,
                role: MemberRole::AsyncFailover,
            },
            NodeHealth {
                node_id: "n3".to_string(),
                region: "ap".to_string(),
                state: HealthState::Healthy,
                observed_at_ms: 1,
                role: MemberRole::Voter,
            },
        ];
        let latency = BTreeMap::from([("n2".to_string(), 1), ("n3".to_string(), 5)]);
        let policy = PromotionPolicy {
            allow_async_failover: true,
            max_async_rpo_lag_ms: 100,
            max_async_safe_time_lag_ms: 150,
            async_failover_gate_by_node: BTreeMap::from([(
                "n2".to_string(),
                AsyncFailoverGate {
                    rpo_lag_ms: 55,
                    safe_time_lag_ms: 140,
                },
            )]),
        };
        let decision = orchestrate_failover(&nodes, "n1", 3, &latency, &policy).expect("decision");
        assert_eq!(decision.promoted_leader, Some("n2".to_string()));
    }

    #[test]
    fn failover_skips_async_failover_when_gate_fails() {
        let nodes = vec![
            NodeHealth {
                node_id: "n1".to_string(),
                region: "us".to_string(),
                state: HealthState::Unavailable,
                observed_at_ms: 1,
                role: MemberRole::Voter,
            },
            NodeHealth {
                node_id: "n2".to_string(),
                region: "eu".to_string(),
                state: HealthState::Healthy,
                observed_at_ms: 1,
                role: MemberRole::AsyncFailover,
            },
            NodeHealth {
                node_id: "n3".to_string(),
                region: "ap".to_string(),
                state: HealthState::Healthy,
                observed_at_ms: 1,
                role: MemberRole::Voter,
            },
        ];
        let latency = BTreeMap::from([("n2".to_string(), 1), ("n3".to_string(), 5)]);
        let policy = PromotionPolicy {
            allow_async_failover: true,
            max_async_rpo_lag_ms: 25,
            max_async_safe_time_lag_ms: 100,
            async_failover_gate_by_node: BTreeMap::from([(
                "n2".to_string(),
                AsyncFailoverGate {
                    rpo_lag_ms: 26,
                    safe_time_lag_ms: 80,
                },
            )]),
        };
        let decision = orchestrate_failover(&nodes, "n1", 3, &latency, &policy).expect("decision");
        assert_eq!(decision.promoted_leader, Some("n3".to_string()));
    }
}
