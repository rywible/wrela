use crate::db::quorum::{QuorumSelectionError, select_nearest_healthy_quorum};
use crate::db::routing::health::{HealthState, NodeHealth};
use crate::db::routing::{
    CompiledRoutingPolicy, RoutingPolicyError, RoutingPolicySpec, compile_policy,
};
use crate::db::security::residency::{ResidencyErrorToken, ResidencyPolicy};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClusterHealthSnapshot {
    pub total_nodes: usize,
    pub healthy_nodes: usize,
    pub degraded_nodes: usize,
    pub unavailable_nodes: usize,
    pub nodes_by_region: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuorumExplainSummary {
    pub quorum_size: usize,
    pub selected_nodes: Vec<String>,
    pub max_selected_latency_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyExplainSummary {
    pub policy_id: String,
    pub policy_hash: u64,
    pub shard_fields: Vec<String>,
    pub shard_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResidencyAuditResult {
    pub shard: Vec<u8>,
    pub sink_region: String,
    pub allowed: bool,
    pub token: Option<&'static str>,
    pub reason: String,
}

pub fn cluster_health_snapshot(nodes: &[NodeHealth]) -> ClusterHealthSnapshot {
    let mut healthy_nodes = 0usize;
    let mut degraded_nodes = 0usize;
    let mut unavailable_nodes = 0usize;
    let mut nodes_by_region = BTreeMap::new();

    for node in nodes {
        match node.state {
            HealthState::Healthy => healthy_nodes += 1,
            HealthState::Degraded => degraded_nodes += 1,
            HealthState::Unavailable => unavailable_nodes += 1,
        }
        *nodes_by_region.entry(node.region.clone()).or_insert(0) += 1;
    }

    ClusterHealthSnapshot {
        total_nodes: nodes.len(),
        healthy_nodes,
        degraded_nodes,
        unavailable_nodes,
        nodes_by_region,
    }
}

pub fn quorum_explain_summary(
    nodes: &[NodeHealth],
    desired_voters: usize,
    latency_hint_ms: &BTreeMap<String, u64>,
) -> Result<QuorumExplainSummary, QuorumSelectionError> {
    let selection = select_nearest_healthy_quorum(nodes, desired_voters, latency_hint_ms)?;
    let max_selected_latency_ms = selection
        .selected_nodes
        .iter()
        .map(|node| latency_hint_ms.get(node).copied().unwrap_or(u64::MAX / 2))
        .max()
        .unwrap_or(0);

    Ok(QuorumExplainSummary {
        quorum_size: selection.quorum_size,
        selected_nodes: selection.selected_nodes,
        max_selected_latency_ms,
    })
}

pub fn policy_explain_summary(
    spec: &RoutingPolicySpec,
) -> Result<PolicyExplainSummary, RoutingPolicyError> {
    let compiled: CompiledRoutingPolicy = compile_policy(spec)?;
    let policy_id = compiled.policy_id.clone();
    let policy_hash = compiled.policy_hash;
    let shard_fields = compiled.shard_key_policy().shard_fields().to_vec();
    let shard_count = compiled.shard_key_policy().shard_count();
    Ok(PolicyExplainSummary {
        policy_id,
        policy_hash,
        shard_fields,
        shard_count,
    })
}

pub fn residency_audit(
    shard: &[u8],
    sink_region: &str,
    policy: &ResidencyPolicy,
) -> ResidencyAuditResult {
    match policy.authorize_egress(shard, sink_region) {
        Ok(()) => ResidencyAuditResult {
            shard: shard.to_vec(),
            sink_region: sink_region.to_string(),
            allowed: true,
            token: None,
            reason: "allowed".to_string(),
        },
        Err(err) => ResidencyAuditResult {
            shard: shard.to_vec(),
            sink_region: sink_region.to_string(),
            allowed: false,
            token: Some(match err.token {
                ResidencyErrorToken::EgressDeny => ResidencyErrorToken::EgressDeny.as_str(),
                ResidencyErrorToken::EgressPolicyUnsat => {
                    ResidencyErrorToken::EgressPolicyUnsat.as_str()
                }
            }),
            reason: err.reason,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::routing::RoutingPolicySpec;
    use crate::db::security::residency::{ResidencyPolicy, ResidencyRule};

    #[test]
    fn cluster_health_snapshot_counts_states_and_regions() {
        let nodes = vec![
            NodeHealth {
                node_id: "n1".to_string(),
                region: "us".to_string(),
                state: HealthState::Healthy,
                observed_at_ms: 1,
            },
            NodeHealth {
                node_id: "n2".to_string(),
                region: "us".to_string(),
                state: HealthState::Degraded,
                observed_at_ms: 1,
            },
            NodeHealth {
                node_id: "n3".to_string(),
                region: "eu".to_string(),
                state: HealthState::Unavailable,
                observed_at_ms: 1,
            },
        ];

        let snapshot = cluster_health_snapshot(&nodes);
        assert_eq!(snapshot.total_nodes, 3);
        assert_eq!(snapshot.healthy_nodes, 1);
        assert_eq!(snapshot.degraded_nodes, 1);
        assert_eq!(snapshot.unavailable_nodes, 1);
        assert_eq!(snapshot.nodes_by_region.get("us"), Some(&2));
        assert_eq!(snapshot.nodes_by_region.get("eu"), Some(&1));
    }

    #[test]
    fn quorum_explain_selects_nodes_and_latency() {
        let nodes = vec![
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
        ];
        let latency = BTreeMap::from([
            ("n1".to_string(), 5_u64),
            ("n2".to_string(), 7_u64),
            ("n3".to_string(), 10_u64),
        ]);

        let summary = quorum_explain_summary(&nodes, 3, &latency).expect("summary");
        assert_eq!(summary.quorum_size, 2);
        assert_eq!(
            summary.selected_nodes,
            vec!["n1".to_string(), "n2".to_string()]
        );
        assert_eq!(summary.max_selected_latency_ms, 7);
    }

    #[test]
    fn policy_explain_is_deterministic() {
        let spec = RoutingPolicySpec {
            policy_id: "orders".to_string(),
            shard_fields: vec!["tenant".to_string(), "order".to_string()],
            shard_count: 32,
            single_field_waiver_reason: None,
        };

        let a = policy_explain_summary(&spec).expect("explain");
        let b = policy_explain_summary(&spec).expect("explain");
        assert_eq!(a, b);
    }

    #[test]
    fn residency_audit_fails_closed_on_policy_unsat() {
        let policy = ResidencyPolicy::with_rules(vec![ResidencyRule {
            shard: b"core".to_vec(),
            allowed_regions: vec!["us".to_string()],
        }]);

        let denied = residency_audit(b"core", "eu", &policy);
        assert!(!denied.allowed);
        assert_eq!(denied.token, Some(ResidencyErrorToken::EgressDeny.as_str()));

        let unsat = residency_audit(b"missing", "us", &policy);
        assert!(!unsat.allowed);
        assert_eq!(
            unsat.token,
            Some(ResidencyErrorToken::EgressPolicyUnsat.as_str())
        );
    }
}
