use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthState {
    Healthy,
    Degraded,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeHealth {
    pub node_id: String,
    pub region: String,
    pub state: HealthState,
    pub observed_at_ms: u64,
}

pub fn health_by_node(nodes: &[NodeHealth]) -> BTreeMap<String, NodeHealth> {
    let mut map = BTreeMap::new();
    for node in nodes {
        map.insert(node.node_id.clone(), node.clone());
    }
    map
}
