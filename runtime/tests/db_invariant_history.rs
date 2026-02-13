use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq)]
enum EventKind {
    Invoke,
    Ok,
    Fail,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Op {
    Write {
        key: String,
        value: String,
        ack_id: String,
    },
    Read {
        key: String,
        value: Option<String>,
    },
    TxnCommit {
        txn_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HistoryEvent {
    process: u64,
    kind: EventKind,
    op: Op,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum InvariantFailure {
    LostAcknowledgedWrite {
        ack_id: String,
        key: String,
        value: String,
    },
    DirtyRead {
        key: String,
        observed: String,
        expected_prefix: String,
    },
    DuplicateCommit {
        txn_id: String,
    },
}

fn check_history(events: &[HistoryEvent]) -> Result<(), Vec<InvariantFailure>> {
    let mut failures = Vec::new();
    let mut acked_writes: BTreeMap<String, (String, String)> = BTreeMap::new();
    let mut last_read: BTreeMap<String, String> = BTreeMap::new();
    let mut committed_txns: BTreeSet<String> = BTreeSet::new();

    for event in events {
        if event.kind != EventKind::Ok {
            continue;
        }
        match &event.op {
            Op::Write { key, value, ack_id } => {
                acked_writes.insert(ack_id.clone(), (key.clone(), value.clone()));
            }
            Op::Read { key, value } => {
                if let Some(observed) = value {
                    if let Some(previous) = last_read.get(key)
                        && observed < previous
                    {
                        failures.push(InvariantFailure::DirtyRead {
                            key: key.clone(),
                            observed: observed.clone(),
                            expected_prefix: previous.clone(),
                        });
                    }
                    last_read.insert(key.clone(), observed.clone());
                }
            }
            Op::TxnCommit { txn_id } => {
                if !committed_txns.insert(txn_id.clone()) {
                    failures.push(InvariantFailure::DuplicateCommit {
                        txn_id: txn_id.clone(),
                    });
                }
            }
        }
    }

    for (ack_id, (key, value)) in acked_writes {
        let seen = last_read.get(&key).cloned().unwrap_or_default();
        if !seen.contains(&value) {
            failures.push(InvariantFailure::LostAcknowledgedWrite { ack_id, key, value });
        }
    }

    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures)
    }
}

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
