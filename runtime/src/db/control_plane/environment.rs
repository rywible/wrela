use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeLifecycleState {
    Healthy,
    Degraded,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeInfo {
    pub node_id: String,
    pub machine_id: String,
    pub slot: Option<String>,
    pub state: NodeLifecycleState,
}

pub trait EnvironmentProvider {
    type Error;

    fn list_nodes(&self) -> Result<Vec<NodeInfo>, Self::Error>;
    fn create_replacement_node(&self, replace_node_id: &str) -> Result<NodeInfo, Self::Error>;
    fn drain_node(&self, node_id: &str) -> Result<(), Self::Error>;
    fn delete_node(&self, node_id: &str) -> Result<(), Self::Error>;
}
