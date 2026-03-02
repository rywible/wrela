use std::collections::{BTreeMap, BTreeSet};

use wrela_runtime::db::coord::{CoordinatorRecord, CoordinatorState, Decision};
use wrela_runtime::db::txn::lifecycle::{HeartbeatLease, TxnLifecycleManager};

#[test]
fn abandoned_txn_detection_and_recovery_plan_is_deterministic() {
    let mut lifecycle = TxnLifecycleManager::default();
    lifecycle.register(11, 100, CoordinatorState::Committing);
    lifecycle.observe_intents(11, 3, 100);

    let coordinator = CoordinatorRecord {
        txn_id: 11,
        epoch: 1,
        created_ms: 0,
        participants: BTreeSet::from([1, 2, 3]),
        prepared: BTreeSet::from([1, 2, 3]),
        committed: BTreeSet::from([1, 2]),
        aborted: BTreeSet::new(),
        decision: Some(Decision::Commit),
        state: CoordinatorState::Committing,
    };

    let plans = lifecycle.abandoned_recovery_plans(
        200,
        HeartbeatLease { timeout_ms: 25 },
        &BTreeMap::from([(11, coordinator)]),
    );

    assert_eq!(plans.len(), 1);
    assert_eq!(plans[0].txn_id, 11);
    assert_eq!(plans[0].recovery_actions.len(), 1);
}

#[test]
fn intent_gc_candidates_remain_bounded_by_visibility_window() {
    let mut lifecycle = TxnLifecycleManager::default();
    lifecycle.register(21, 1_000, CoordinatorState::Preparing);
    lifecycle.observe_intents(21, 9, 1_000);

    let lease = HeartbeatLease { timeout_ms: 50 };
    assert!(
        lifecycle
            .intent_gc_candidates(1_030, lease, 1_000)
            .is_empty()
    );
    assert!(lifecycle.intent_gc_candidates(1_070, lease, 900).is_empty());

    let candidates = lifecycle.intent_gc_candidates(1_070, lease, 1_000);
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].txn_id, 21);
    assert_eq!(candidates[0].reclaimable_intents, 9);
}
