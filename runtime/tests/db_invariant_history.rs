use tempfile::tempdir;
use wrela_runtime::db::invariant_history::{
    EventKind, HistoryEvent, InvariantFailure, Op, check_history,
};
use wrela_runtime::db::{close_db, open_db, read_point, submit_put, txn_begin, txn_commit};

#[test]
fn invariant_checker_accepts_consistent_history() {
    let history = vec![
        HistoryEvent {
            process: 1,
            kind: EventKind::Invoke,
            op: Op::Write {
                key: "k".to_string(),
                value: "v1".to_string(),
                ack_id: "a1".to_string(),
            },
        },
        HistoryEvent {
            process: 1,
            kind: EventKind::Ok,
            op: Op::Write {
                key: "k".to_string(),
                value: "v1".to_string(),
                ack_id: "a1".to_string(),
            },
        },
        HistoryEvent {
            process: 2,
            kind: EventKind::Ok,
            op: Op::Read {
                key: "k".to_string(),
                value: Some("v1".to_string()),
            },
        },
        HistoryEvent {
            process: 3,
            kind: EventKind::Ok,
            op: Op::TxnCommit {
                txn_id: "t1".to_string(),
            },
        },
    ];

    assert!(check_history(&history).is_ok());
}

#[test]
fn invariant_checker_detects_lost_write_and_duplicate_commit() {
    let history = vec![
        HistoryEvent {
            process: 1,
            kind: EventKind::Ok,
            op: Op::Write {
                key: "k".to_string(),
                value: "v1".to_string(),
                ack_id: "a1".to_string(),
            },
        },
        HistoryEvent {
            process: 2,
            kind: EventKind::Ok,
            op: Op::Read {
                key: "k".to_string(),
                value: Some("v0".to_string()),
            },
        },
        HistoryEvent {
            process: 3,
            kind: EventKind::Ok,
            op: Op::TxnCommit {
                txn_id: "t1".to_string(),
            },
        },
        HistoryEvent {
            process: 4,
            kind: EventKind::Ok,
            op: Op::TxnCommit {
                txn_id: "t1".to_string(),
            },
        },
        HistoryEvent {
            process: 5,
            kind: EventKind::Fail,
            op: Op::Read {
                key: "k".to_string(),
                value: Some("stale".to_string()),
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
fn invariant_checker_accepts_live_db_trace() {
    let dir = tempdir().expect("tempdir");
    let handle = open_db(dir.path()).expect("open db");
    submit_put(
        handle,
        b"core".to_vec(),
        b"k".to_vec(),
        b"v1".to_vec(),
        None,
    )
    .expect("write acked");
    let read_value = read_point(handle, b"core".to_vec(), b"k".to_vec())
        .expect("read point")
        .map(|bytes| String::from_utf8(bytes).expect("utf8 read value"));

    let txn_id = txn_begin(handle).expect("begin txn");
    txn_commit(handle, txn_id).expect("commit txn");

    let history = vec![
        HistoryEvent {
            process: 1,
            kind: EventKind::Ok,
            op: Op::Write {
                key: "core/k".to_string(),
                value: "v1".to_string(),
                ack_id: "ack-live-1".to_string(),
            },
        },
        HistoryEvent {
            process: 2,
            kind: EventKind::Ok,
            op: Op::Read {
                key: "core/k".to_string(),
                value: read_value,
            },
        },
        HistoryEvent {
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
