use crate::db::quorum::{QuorumSelection, QuorumSelectionError, select_nearest_healthy_quorum};
use crate::db::routing::health::{HealthState, NodeHealth};
use std::collections::BTreeMap;

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
) -> Result<FailoverDecision, FailoverError> {
    let failed_nodes: Vec<String> = nodes
        .iter()
        .filter(|n| n.state == HealthState::Unavailable)
        .map(|n| n.node_id.clone())
        .collect();

    let quorum = select_nearest_healthy_quorum(nodes, desired_voters, latency_hint_ms)
        .map_err(FailoverError::Quorum)?;

    let promoted_leader = if failed_nodes.iter().any(|n| n == current_leader) {
        quorum.selected_nodes.first().cloned()
    } else {
        Some(current_leader.to_string())
    };

    if promoted_leader.is_none() {
        return Err(FailoverError::NoHealthyLeaderCandidate);
    }

    let mut timeline = Vec::new();
    timeline.push(format!("failures_detected={}", failed_nodes.len()));
    timeline.push(format!(
        "quorum_selected={}",
        quorum.selected_nodes.join(",")
    ));
    timeline.push(format!(
        "leader={}",
        promoted_leader.clone().expect("leader")
    ));

    Ok(FailoverDecision {
        failed_nodes,
        promoted_leader,
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
                state: HealthState::Healthy,
                observed_at_ms: 1,
            },
        ];
        let latency = BTreeMap::from([("n2".to_string(), 5), ("n3".to_string(), 8)]);
        let decision = orchestrate_failover(&nodes, "n1", 3, &latency).expect("decision");
        assert_eq!(decision.promoted_leader, Some("n2".to_string()));
        assert_eq!(decision.quorum.quorum_size, 2);
        assert_eq!(decision.failed_nodes, vec!["n1".to_string()]);
    }
}
