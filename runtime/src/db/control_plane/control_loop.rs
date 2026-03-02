use crate::db::cluster::ClusterState;
use crate::db::control_plane::environment::EnvironmentProvider;
use crate::db::control_plane::reconciler::{ReconcileResult, Reconciler};

#[derive(Debug, Clone)]
pub struct ControlLoop<R> {
    reconciler: R,
    leader_only_enabled: bool,
}

impl<R> ControlLoop<R> {
    pub fn new(reconciler: R, leader_only_enabled: bool) -> Self {
        Self {
            reconciler,
            leader_only_enabled,
        }
    }
}

impl<R> ControlLoop<R> {
    pub fn tick<P>(&self, provider: &P, state: &ClusterState) -> Result<ReconcileResult, String>
    where
        P: EnvironmentProvider,
        R: Reconciler<P>,
    {
        self.reconciler
            .reconcile(provider, state, self.leader_only_enabled)
    }
}
