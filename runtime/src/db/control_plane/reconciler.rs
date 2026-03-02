use crate::db::cluster::{ClusterState, replace_node};
use crate::db::control_plane::environment::{EnvironmentProvider, NodeInfo, NodeLifecycleState};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReconcileAction {
    Noop,
    ReplaceFailedVoter { failed_node: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReconcileResult {
    pub action: ReconcileAction,
    pub trace: Vec<String>,
    pub updated_state: ClusterState,
}

pub trait Reconciler<P: EnvironmentProvider> {
    fn reconcile(
        &self,
        provider: &P,
        state: &ClusterState,
        leader_only_enabled: bool,
    ) -> Result<ReconcileResult, String>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct DefaultReconciler;

impl<P: EnvironmentProvider> Reconciler<P> for DefaultReconciler {
    fn reconcile(
        &self,
        provider: &P,
        state: &ClusterState,
        leader_only_enabled: bool,
    ) -> Result<ReconcileResult, String> {
        reconcile_once(provider, state, leader_only_enabled)
    }
}

pub fn choose_reconcile_action(state: &ClusterState, nodes: &[NodeInfo]) -> ReconcileAction {
    let failed_voter = state.voters.iter().find(|voter| {
        nodes
            .iter()
            .find(|node| &node.node_id == *voter)
            .map(|node| node.state == NodeLifecycleState::Failed)
            .unwrap_or(true)
    });

    match failed_voter {
        Some(node) => ReconcileAction::ReplaceFailedVoter {
            failed_node: node.clone(),
        },
        None => ReconcileAction::Noop,
    }
}

pub fn reconcile_once<P: EnvironmentProvider>(
    provider: &P,
    state: &ClusterState,
    leader_only_enabled: bool,
) -> Result<ReconcileResult, String> {
    if !leader_only_enabled {
        return Ok(ReconcileResult {
            action: ReconcileAction::Noop,
            trace: vec!["leader_not_authoritative".to_string()],
            updated_state: state.clone(),
        });
    }

    let nodes = provider
        .list_nodes()
        .map_err(|_| "environment_list_nodes_failed".to_string())?;
    let action = choose_reconcile_action(state, &nodes);
    let mut updated_state = state.clone();
    let mut trace = vec![format!("observed_nodes={}", nodes.len())];

    match &action {
        ReconcileAction::Noop => trace.push("action=noop".to_string()),
        ReconcileAction::ReplaceFailedVoter { failed_node } => {
            trace.push(format!("action=replace_failed_voter:{failed_node}"));
            let replacement = provider
                .create_replacement_node(failed_node)
                .map_err(|_| "environment_create_replacement_failed".to_string())?;
            let region = state
                .node_regions
                .get(failed_node)
                .cloned()
                .unwrap_or_else(|| "unknown".to_string());
            let orchestration = replace_node(
                &mut updated_state,
                failed_node,
                &replacement.node_id,
                &region,
            )
            .map_err(|err| format!("replace_node_failed:{err:?}"))?;
            trace.extend(orchestration.steps);
            provider
                .drain_node(failed_node)
                .map_err(|_| "environment_drain_failed".to_string())?;
            provider
                .delete_node(failed_node)
                .map_err(|_| "environment_delete_failed".to_string())?;
        }
    }

    Ok(ReconcileResult {
        action,
        trace,
        updated_state,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::control_plane::environment::{
        EnvironmentProvider, NodeInfo, NodeLifecycleState,
    };
    use std::cell::RefCell;
    use std::collections::{BTreeMap, VecDeque};

    #[derive(Default)]
    struct FakeProvider {
        nodes: RefCell<Vec<NodeInfo>>,
        replacements: RefCell<VecDeque<NodeInfo>>,
        drained: RefCell<Vec<String>>,
        deleted: RefCell<Vec<String>>,
    }

    impl EnvironmentProvider for FakeProvider {
        type Error = ();

        fn list_nodes(&self) -> Result<Vec<NodeInfo>, Self::Error> {
            Ok(self.nodes.borrow().clone())
        }

        fn create_replacement_node(&self, _replace_node_id: &str) -> Result<NodeInfo, Self::Error> {
            self.replacements.borrow_mut().pop_front().ok_or(())
        }

        fn drain_node(&self, node_id: &str) -> Result<(), Self::Error> {
            self.drained.borrow_mut().push(node_id.to_string());
            Ok(())
        }

        fn delete_node(&self, node_id: &str) -> Result<(), Self::Error> {
            self.deleted.borrow_mut().push(node_id.to_string());
            Ok(())
        }
    }

    #[test]
    fn reconcile_replaces_failed_voter() {
        let state = ClusterState::new(BTreeMap::from([
            ("node-a".to_string(), "ord".to_string()),
            ("node-b".to_string(), "ord".to_string()),
            ("node-c".to_string(), "ord".to_string()),
        ]));

        let provider = FakeProvider::default();
        *provider.nodes.borrow_mut() = vec![
            NodeInfo {
                node_id: "node-a".to_string(),
                machine_id: "m-a".to_string(),
                slot: Some("a".to_string()),
                state: NodeLifecycleState::Healthy,
            },
            NodeInfo {
                node_id: "node-b".to_string(),
                machine_id: "m-b".to_string(),
                slot: Some("b".to_string()),
                state: NodeLifecycleState::Failed,
            },
            NodeInfo {
                node_id: "node-c".to_string(),
                machine_id: "m-c".to_string(),
                slot: Some("c".to_string()),
                state: NodeLifecycleState::Healthy,
            },
        ];
        provider.replacements.borrow_mut().push_back(NodeInfo {
            node_id: "node-d".to_string(),
            machine_id: "m-d".to_string(),
            slot: Some("b".to_string()),
            state: NodeLifecycleState::Healthy,
        });

        let outcome = reconcile_once(&provider, &state, true).expect("reconcile");
        assert!(outcome.updated_state.voters.contains("node-d"));
        assert!(!outcome.updated_state.voters.contains("node-b"));
    }
}
