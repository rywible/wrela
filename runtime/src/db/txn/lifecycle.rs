use crate::db::coord::recovery::{RecoveryAction, recovery_actions};
use crate::db::coord::{CoordinatorRecord, CoordinatorState};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeartbeatLease {
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TxnLiveness {
    pub txn_id: u64,
    pub started_ms: u64,
    pub last_heartbeat_ms: u64,
    pub last_activity_ms: u64,
    pub pending_intents: usize,
    pub coordinator_state: CoordinatorState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntentGcDecision {
    pub txn_id: u64,
    pub reclaimable_intents: usize,
    pub reason: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbandonedTxnPlan {
    pub txn_id: u64,
    pub recovery_actions: Vec<RecoveryAction>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifecycleSnapshot {
    pub live_txns: usize,
    pub abandoned_txns: usize,
    pub total_pending_intents: usize,
}

#[derive(Debug, Default)]
pub struct TxnLifecycleManager {
    by_txn: BTreeMap<u64, TxnLiveness>,
}

impl TxnLifecycleManager {
    pub fn register(
        &mut self,
        txn_id: u64,
        now_ms: u64,
        coordinator_state: CoordinatorState,
    ) -> bool {
        self.by_txn
            .insert(
                txn_id,
                TxnLiveness {
                    txn_id,
                    started_ms: now_ms,
                    last_heartbeat_ms: now_ms,
                    last_activity_ms: now_ms,
                    pending_intents: 0,
                    coordinator_state,
                },
            )
            .is_none()
    }

    pub fn heartbeat(&mut self, txn_id: u64, now_ms: u64) -> bool {
        if let Some(txn) = self.by_txn.get_mut(&txn_id) {
            txn.last_heartbeat_ms = txn.last_heartbeat_ms.max(now_ms);
            txn.last_activity_ms = txn.last_activity_ms.max(now_ms);
            true
        } else {
            false
        }
    }

    pub fn observe_intents(&mut self, txn_id: u64, pending_intents: usize, now_ms: u64) {
        if let Some(txn) = self.by_txn.get_mut(&txn_id) {
            txn.pending_intents = pending_intents;
            txn.last_activity_ms = txn.last_activity_ms.max(now_ms);
        }
    }

    pub fn set_coordinator_state(&mut self, txn_id: u64, state: CoordinatorState, now_ms: u64) {
        if let Some(txn) = self.by_txn.get_mut(&txn_id) {
            txn.coordinator_state = state;
            txn.last_activity_ms = txn.last_activity_ms.max(now_ms);
        }
    }

    pub fn abandoned(&self, now_ms: u64, lease: HeartbeatLease) -> BTreeSet<u64> {
        self.by_txn
            .values()
            .filter(|txn| {
                matches!(
                    txn.coordinator_state,
                    CoordinatorState::Preparing
                        | CoordinatorState::Committing
                        | CoordinatorState::Aborting
                ) && now_ms.saturating_sub(txn.last_heartbeat_ms) > lease.timeout_ms
            })
            .map(|txn| txn.txn_id)
            .collect()
    }

    pub fn intent_gc_candidates(
        &self,
        now_ms: u64,
        lease: HeartbeatLease,
        max_safe_visible_ms: u64,
    ) -> Vec<IntentGcDecision> {
        let mut out = Vec::new();
        for txn in self.by_txn.values() {
            if txn.pending_intents == 0 {
                continue;
            }
            let is_quiescent = now_ms.saturating_sub(txn.last_activity_ms) > lease.timeout_ms;
            if is_quiescent && txn.last_activity_ms <= max_safe_visible_ms {
                out.push(IntentGcDecision {
                    txn_id: txn.txn_id,
                    reclaimable_intents: txn.pending_intents,
                    reason: "abandoned-and-visible-window-cleared",
                });
            }
        }
        out
    }

    pub fn abandoned_recovery_plans(
        &self,
        now_ms: u64,
        lease: HeartbeatLease,
        coordinator_records: &BTreeMap<u64, CoordinatorRecord>,
    ) -> Vec<AbandonedTxnPlan> {
        let abandoned = self.abandoned(now_ms, lease);
        abandoned
            .into_iter()
            .map(|txn_id| {
                let actions = coordinator_records
                    .get(&txn_id)
                    .map(recovery_actions)
                    .unwrap_or_default();
                AbandonedTxnPlan {
                    txn_id,
                    recovery_actions: actions,
                }
            })
            .collect()
    }

    pub fn finalize(&mut self, txn_id: u64) -> bool {
        self.by_txn.remove(&txn_id).is_some()
    }

    pub fn snapshot(&self, now_ms: u64, lease: HeartbeatLease) -> LifecycleSnapshot {
        let abandoned = self.abandoned(now_ms, lease).len();
        let total_pending_intents = self.by_txn.values().map(|txn| txn.pending_intents).sum();
        LifecycleSnapshot {
            live_txns: self.by_txn.len(),
            abandoned_txns: abandoned,
            total_pending_intents,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::coord::{CoordinatorRecord, Decision};

    #[test]
    fn heartbeat_and_abandonment_detection_are_deterministic() {
        let mut mgr = TxnLifecycleManager::default();
        let lease = HeartbeatLease { timeout_ms: 30 };
        assert!(mgr.register(10, 100, CoordinatorState::Preparing));

        assert!(mgr.abandoned(120, lease).is_empty());
        assert!(mgr.abandoned(131, lease).contains(&10));

        assert!(mgr.heartbeat(10, 135));
        assert!(mgr.abandoned(160, lease).is_empty());
        assert!(mgr.abandoned(166, lease).contains(&10));
    }

    #[test]
    fn intent_gc_requires_quiescent_and_visibility_safe() {
        let mut mgr = TxnLifecycleManager::default();
        let lease = HeartbeatLease { timeout_ms: 20 };
        mgr.register(9, 100, CoordinatorState::Preparing);
        mgr.observe_intents(9, 4, 100);

        assert!(mgr.intent_gc_candidates(110, lease, 90).is_empty());
        assert!(mgr.intent_gc_candidates(130, lease, 90).is_empty());

        let gc = mgr.intent_gc_candidates(130, lease, 100);
        assert_eq!(gc.len(), 1);
        assert_eq!(gc[0].txn_id, 9);
        assert_eq!(gc[0].reclaimable_intents, 4);
    }

    #[test]
    fn abandoned_recovery_plan_uses_coordinator_record() {
        let mut mgr = TxnLifecycleManager::default();
        let lease = HeartbeatLease { timeout_ms: 10 };
        mgr.register(77, 100, CoordinatorState::Committing);

        let record = CoordinatorRecord {
            txn_id: 77,
            epoch: 1,
            created_ms: 0,
            participants: BTreeSet::from([1, 2]),
            prepared: BTreeSet::from([1, 2]),
            committed: BTreeSet::from([1]),
            aborted: BTreeSet::new(),
            decision: Some(Decision::Commit),
            state: CoordinatorState::Committing,
        };
        let plans = mgr.abandoned_recovery_plans(200, lease, &BTreeMap::from([(77, record)]));
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].txn_id, 77);
        assert_eq!(
            plans[0].recovery_actions,
            vec![RecoveryAction::RecommitParticipant {
                txn_id: 77,
                epoch: 1,
                participant_id: 2,
            }]
        );
    }
}
