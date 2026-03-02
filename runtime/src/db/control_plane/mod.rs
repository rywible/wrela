pub mod control_loop;
pub mod environment;
pub mod providers;
pub mod reconciler;

pub use control_loop::ControlLoop;
pub use environment::{EnvironmentProvider, NodeInfo, NodeLifecycleState};
pub use providers::fly::{FlyMachinesProvider, FlyProviderError};
pub use reconciler::{
    DefaultReconciler, ReconcileAction, ReconcileResult, Reconciler, choose_reconcile_action,
    reconcile_once,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ControlLoopConfig {
    pub desired_voters: usize,
    pub leader_only_enabled: bool,
}

impl Default for ControlLoopConfig {
    fn default() -> Self {
        Self {
            desired_voters: 3,
            leader_only_enabled: true,
        }
    }
}
