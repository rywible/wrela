use crate::db::routing::health::{HealthState, NodeHealth};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeHealthSnapshot {
    pub node_id: String,
    pub region: String,
    pub state: HealthState,
    pub latency_ms: u64,
    pub observed_at_ms: u64,
}

pub fn build_health_snapshot(
    nodes: &[NodeHealth],
    latency_hint_ms: &BTreeMap<String, u64>,
) -> Vec<NodeHealthSnapshot> {
    let mut out: Vec<NodeHealthSnapshot> = nodes
        .iter()
        .map(|node| NodeHealthSnapshot {
            node_id: node.node_id.clone(),
            region: node.region.clone(),
            state: node.state,
            latency_ms: latency_hint_ms
                .get(&node.node_id)
                .copied()
                .unwrap_or(u64::MAX / 2),
            observed_at_ms: node.observed_at_ms,
        })
        .collect();
    out.sort_by(|a, b| {
        a.region
            .cmp(&b.region)
            .then_with(|| a.node_id.cmp(&b.node_id))
    });
    out
}

#[cfg(test)]
mod tests {
    use super::build_health_snapshot;
    use crate::db::routing::health::{HealthState, NodeHealth};
    use std::collections::BTreeMap;

    #[test]
    fn health_snapshot_is_deterministic() {
        let nodes = vec![
            NodeHealth {
                node_id: "n2".to_string(),
                region: "eu".to_string(),
                state: HealthState::Healthy,
                observed_at_ms: 2,
            },
            NodeHealth {
                node_id: "n1".to_string(),
                region: "us".to_string(),
                state: HealthState::Degraded,
                observed_at_ms: 1,
            },
        ];
        let latency = BTreeMap::from([("n1".to_string(), 7_u64), ("n2".to_string(), 4_u64)]);

        let a = build_health_snapshot(&nodes, &latency);
        let b = build_health_snapshot(&nodes, &latency);
        assert_eq!(a, b);
    }
}
