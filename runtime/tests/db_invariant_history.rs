use tempfile::tempdir;
use wrela_runtime::db::DbConfig;
use wrela_runtime::db::config::ReplicationConfig;
use wrela_runtime::db::invariant_history::{
    EventKind, HistoryEvent, InvariantCheckOutcome, InvariantFailure, ObservedValue, Op,
    check_history, check_history_outcome,
};
use wrela_runtime::db::{
    ReadConsistency, close_db, open_db_with_config, read_point_consistent, submit_put, txn_begin,
    txn_commit,
};

fn open_invariant_db(path: &std::path::Path) -> i64 {
    let config = DbConfig::for_testing().with_replication(ReplicationConfig {
        factor: 3,
        write_quorum: 2,
        ..DbConfig::for_testing().replication
    });
    open_db_with_config(path, &config).expect("open db")
}

#[test]
fn invariant_checker_accepts_consistent_history() {
    let history = vec![
        HistoryEvent {
            sequence: 1,
            process: 1,
            kind: EventKind::Invoke,
            op: Op::Write {
                key: "k".to_string(),
                value: "v1".to_string(),
                ack_id: "a1".to_string(),
            },
        },
        HistoryEvent {
            sequence: 2,
            process: 1,
            kind: EventKind::Ok,
            op: Op::Write {
                key: "k".to_string(),
                value: "v1".to_string(),
                ack_id: "a1".to_string(),
            },
        },
        HistoryEvent {
            sequence: 3,
            process: 2,
            kind: EventKind::Ok,
            op: Op::Read {
                key: "k".to_string(),
                observed: ObservedValue::Present("v1".to_string()),
            },
        },
        HistoryEvent {
            sequence: 4,
            process: 3,
            kind: EventKind::Ok,
            op: Op::TxnCommit {
                txn_id: "t1".to_string(),
            },
        },
    ];

    assert!(check_history(&history).is_ok());
    assert_eq!(check_history_outcome(&history), InvariantCheckOutcome::Pass);
}

#[test]
fn invariant_checker_detects_lost_write_and_duplicate_commit() {
    let history = vec![
        HistoryEvent {
            sequence: 1,
            process: 1,
            kind: EventKind::Ok,
            op: Op::Write {
                key: "k".to_string(),
                value: "v1".to_string(),
                ack_id: "a1".to_string(),
            },
        },
        HistoryEvent {
            sequence: 2,
            process: 2,
            kind: EventKind::Ok,
            op: Op::Read {
                key: "k".to_string(),
                observed: ObservedValue::Present("v0".to_string()),
            },
        },
        HistoryEvent {
            sequence: 3,
            process: 3,
            kind: EventKind::Ok,
            op: Op::TxnCommit {
                txn_id: "t1".to_string(),
            },
        },
        HistoryEvent {
            sequence: 4,
            process: 4,
            kind: EventKind::Ok,
            op: Op::TxnCommit {
                txn_id: "t1".to_string(),
            },
        },
    ];

    let failures = check_history(&history).expect_err("must fail");
    assert!(
        failures
            .iter()
            .any(|f| matches!(f, InvariantFailure::LostAcknowledgedWrite { .. }))
    );
    assert!(
        failures
            .iter()
            .any(|f| matches!(f, InvariantFailure::DuplicateCommit { .. }))
    );
}

#[test]
fn invariant_checker_flags_insufficient_observation() {
    let history = vec![HistoryEvent {
        sequence: 1,
        process: 1,
        kind: EventKind::Ok,
        op: Op::Write {
            key: "k".to_string(),
            value: "v1".to_string(),
            ack_id: "a1".to_string(),
        },
    }];

    match check_history_outcome(&history) {
        InvariantCheckOutcome::InsufficientObservation(gaps) => {
            assert_eq!(gaps.len(), 1);
            assert_eq!(gaps[0].ack_id, "a1");
            assert_eq!(gaps[0].key, "k");
        }
        other => panic!("expected insufficient observation, got {other:?}"),
    }
}

#[test]
fn invariant_checker_rejects_substring_and_lexicographic_cheats() {
    let history = vec![
        HistoryEvent {
            sequence: 1,
            process: 1,
            kind: EventKind::Ok,
            op: Op::Write {
                key: "k".to_string(),
                value: "v10".to_string(),
                ack_id: "a1".to_string(),
            },
        },
        HistoryEvent {
            sequence: 2,
            process: 2,
            kind: EventKind::Ok,
            op: Op::Read {
                key: "k".to_string(),
                observed: ObservedValue::Present("v1".to_string()),
            },
        },
    ];

    let failures = check_history(&history).expect_err("must fail");
    assert!(
        failures
            .iter()
            .any(|f| matches!(f, InvariantFailure::DirtyRead { .. })),
        "checker must reject prefix/lexicographic tricks"
    );
}

#[test]
fn invariant_checker_accepts_live_db_trace() {
    let dir = tempdir().expect("tempdir");
    let handle = open_invariant_db(dir.path());
    submit_put(
        handle,
        b"core".to_vec(),
        b"k".to_vec(),
        b"v1".to_vec(),
        None,
    )
    .expect("write acked");
    let read_value = read_point_consistent(
        handle,
        b"core".to_vec(),
        b"k".to_vec(),
        ReadConsistency::Eventual,
        None,
    )
    .expect("read point")
    .map(|bytes| String::from_utf8(bytes).expect("utf8 read value"));

    let txn_id = txn_begin(handle).expect("begin txn");
    txn_commit(handle, txn_id).expect("commit txn");

    let history = vec![
        HistoryEvent {
            sequence: 1,
            process: 1,
            kind: EventKind::Ok,
            op: Op::Write {
                key: "core/k".to_string(),
                value: "v1".to_string(),
                ack_id: "ack-live-1".to_string(),
            },
        },
        HistoryEvent {
            sequence: 2,
            process: 2,
            kind: EventKind::Ok,
            op: Op::Read {
                key: "core/k".to_string(),
                observed: read_value
                    .map(ObservedValue::Present)
                    .unwrap_or(ObservedValue::Missing),
            },
        },
        HistoryEvent {
            sequence: 3,
            process: 3,
            kind: EventKind::Ok,
            op: Op::TxnCommit {
                txn_id: format!("txn-{txn_id}"),
            },
        },
    ];

    assert!(check_history(&history).is_ok());
    assert!(close_db(handle));
}
