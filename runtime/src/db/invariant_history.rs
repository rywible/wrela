use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventKind {
    Invoke,
    Ok,
    Fail,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op {
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
pub struct HistoryEvent {
    pub process: u64,
    pub kind: EventKind,
    pub op: Op,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvariantFailure {
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

pub fn check_history(events: &[HistoryEvent]) -> Result<(), Vec<InvariantFailure>> {
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
