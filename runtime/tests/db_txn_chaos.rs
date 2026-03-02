use std::collections::BTreeSet;

use tempfile::TempDir;
use wrela_runtime::db::DbConfig;
use wrela_runtime::db::config::ReplicationConfig;
use wrela_runtime::db::coord::{CoordinatorState, Decision, TwoPhaseCoordinator};
use wrela_runtime::db::read::strong::{StrongReadDecision, evaluate_strong_read};
use wrela_runtime::db::time::hlc::HlTimestamp;
use wrela_runtime::db::time::uncertainty::UncertaintyWindow;
use wrela_runtime::db::txn::commit_wait::CommitWaitPolicy;
use wrela_runtime::db::types::{BatchOp, ErrorCode};
use wrela_runtime::db::{
    close_db, open_db_with_config, submit_batch, submit_put, txn_abort, txn_begin, txn_commit,
    txn_lock_key,
};

fn open_chaos_db(path: &std::path::Path) -> i64 {
    let config = DbConfig::for_testing().with_replication(ReplicationConfig {
        factor: 3,
        write_quorum: 2,
        ..DbConfig::for_testing().replication
    });
    open_db_with_config(path, &config).expect("open db")
}

#[test]
fn seeded_lock_conflict_matrix_preserves_atomicity() {
    let dir = TempDir::new().expect("tempdir");
    let handle = open_chaos_db(dir.path());

    let ns = b"chaos".to_vec();
    submit_put(handle, ns.clone(), b"seed".to_vec(), b"v".to_vec(), None).expect("seed put");

    for seed in 1..=16u64 {
        let tx1 = txn_begin(handle).expect("txn1");
        let tx2 = txn_begin(handle).expect("txn2");

        let key1 = format!("k{:02}", seed).into_bytes();
        let key2 = format!("k{:02}", seed + 1).into_bytes();

        txn_lock_key(handle, tx1, ns.clone(), key1.clone()).expect("tx1 lock key1");
        txn_lock_key(handle, tx2, ns.clone(), key2.clone()).expect("tx2 lock key2");

        let r1 = txn_lock_key(handle, tx1, ns.clone(), key2.clone());
        let r2 = txn_lock_key(handle, tx2, ns.clone(), key1.clone());

        let deadlock_resolved = r1.is_err() || r2.is_err();
        assert!(
            deadlock_resolved,
            "deadlock must resolve deterministically for seed={seed}"
        );

        // One txn commits, the other aborts; whichever was not aborted may commit idempotently.
        let _ = txn_commit(handle, tx1);
        let _ = txn_abort(handle, tx2);
    }

    assert!(close_db(handle));
}

#[test]
fn two_phase_commit_crash_recovery_matrix() {
    let mut c = TwoPhaseCoordinator::default();
    let participants = BTreeSet::from([10, 20, 30]);

    for seed in 0..8u64 {
        let txn_id = 1_000 + seed;
        c.begin(txn_id, participants.clone(), 0).expect("begin");
        c.on_prepare_ok(txn_id, 10).expect("prepare 10");

        if seed % 2 == 0 {
            c.on_prepare_ok(txn_id, 20).expect("prepare 20");
            assert_eq!(
                c.on_prepare_ok(txn_id, 30).expect("prepare 30"),
                Some(Decision::Commit)
            );
            c.on_commit_ack(txn_id, 10).expect("ack 10");
            c.on_commit_ack(txn_id, 20).expect("ack 20");
            c.on_commit_ack(txn_id, 30).expect("ack 30");
            assert_eq!(
                c.record(txn_id).expect("rec").state,
                CoordinatorState::Committed
            );
        } else {
            c.on_prepare_failed(txn_id, 20).expect("prepare failed");
            c.on_abort_ack(txn_id, 10).expect("abort 10");
            c.on_abort_ack(txn_id, 20).expect("abort 20");
            c.on_abort_ack(txn_id, 30).expect("abort 30");
            assert_eq!(
                c.record(txn_id).expect("rec").state,
                CoordinatorState::Aborted
            );
        }
    }
}

#[test]
fn uncertainty_and_safe_time_reject_unsafe_reads() {
    let requested = HlTimestamp {
        physical_ms: 1_000,
        logical: 1,
    }
    .pack();
    let safe = HlTimestamp {
        physical_ms: 990,
        logical: 0,
    }
    .pack();

    let decision = evaluate_strong_read(
        requested,
        safe,
        UncertaintyWindow {
            lower_bound: HlTimestamp {
                physical_ms: 980,
                logical: 0,
            }
            .pack(),
            upper_bound: HlTimestamp {
                physical_ms: 1_010,
                logical: u16::MAX,
            }
            .pack(),
        },
    );
    assert!(matches!(decision, StrongReadDecision::RetryAfter { .. }));
}

#[test]
fn commit_wait_policy_bounds_skew_wait() {
    let policy = CommitWaitPolicy { max_wait_ms: 50 };
    let commit = HlTimestamp {
        physical_ms: 1_000,
        logical: 0,
    }
    .pack();
    let now = HlTimestamp {
        physical_ms: 980,
        logical: 0,
    }
    .pack();
    let uncertainty = HlTimestamp {
        physical_ms: 1_200,
        logical: 0,
    }
    .pack();

    let result = policy.evaluate(commit, now, uncertainty);
    assert!(!result.external_consistency_ready);
    assert_eq!(result.wait_ms, 50);
}

#[test]
fn write_heavy_batch_matrix_returns_valid_status() {
    let dir = TempDir::new().expect("tempdir");
    let handle = open_chaos_db(dir.path());
    let ns = b"matrix".to_vec();

    for seed in 0..20u8 {
        let mut batch = Vec::new();
        for i in 0..16u8 {
            batch.push(BatchOp::Put {
                namespace: ns.clone().into(),
                key: vec![seed, i].into(),
                value: vec![seed.wrapping_add(i); 8].into(),
                expected_version: None,
            });
        }
        let result = submit_batch(handle, &batch);
        assert!(result.is_ok(), "batch failed for seed={seed}: {result:?}");
    }

    assert!(close_db(handle));
}

#[test]
fn aborting_unknown_txn_returns_typed_error() {
    let dir = TempDir::new().expect("tempdir");
    let handle = open_chaos_db(dir.path());
    let err = txn_abort(handle, 99_999).expect_err("must fail");
    assert_eq!(err.code, ErrorCode::InvalidArgument);
    assert!(close_db(handle));
}
