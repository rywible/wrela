use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EventKind {
    Invoke,
    Ok,
    Fail,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObservedValue {
    Missing,
    Present(String),
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
        observed: ObservedValue,
    },
    TxnCommit {
        txn_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryEvent {
    pub sequence: u64,
    pub process: u64,
    pub kind: EventKind,
    pub op: Op,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservationGap {
    pub ack_id: String,
    pub key: String,
    pub value: String,
    pub ack_sequence: u64,
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
        expected: String,
        sequence: u64,
    },
    DuplicateCommit {
        txn_id: String,
    },
    InsufficientObservation {
        ack_id: String,
        key: String,
        value: String,
        ack_sequence: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvariantCheckOutcome {
    Pass,
    Fail(Vec<InvariantFailure>),
    InsufficientObservation(Vec<ObservationGap>),
}

#[derive(Debug, Clone)]
struct AckedWriteState {
    key: String,
    value: String,
    ack_sequence: u64,
    any_post_ack_read: bool,
    value_observed_after_ack: bool,
}

pub fn check_history_outcome(events: &[HistoryEvent]) -> InvariantCheckOutcome {
    let mut ordered: Vec<&HistoryEvent> = events.iter().collect();
    ordered.sort_by(|a, b| {
        a.sequence
            .cmp(&b.sequence)
            .then_with(|| a.process.cmp(&b.process))
            .then_with(|| a.kind.cmp(&b.kind))
    });

    let mut failures = Vec::new();
    let mut acked_writes: BTreeMap<String, AckedWriteState> = BTreeMap::new();
    let mut ack_ids_by_key: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut latest_acked_value_by_key: BTreeMap<String, String> = BTreeMap::new();
    let mut committed_txns: BTreeSet<String> = BTreeSet::new();

    for event in ordered {
        if event.kind != EventKind::Ok {
            continue;
        }
        match &event.op {
            Op::Write { key, value, ack_id } => {
                let state = AckedWriteState {
                    key: key.clone(),
                    value: value.clone(),
                    ack_sequence: event.sequence,
                    any_post_ack_read: false,
                    value_observed_after_ack: false,
                };
                acked_writes.insert(ack_id.clone(), state);
                ack_ids_by_key
                    .entry(key.clone())
                    .or_default()
                    .push(ack_id.clone());
                latest_acked_value_by_key.insert(key.clone(), value.clone());
            }
            Op::Read { key, observed } => {
                if let Some(expected_latest) = latest_acked_value_by_key.get(key) {
                    match observed {
                        ObservedValue::Missing => {
                            failures.push(InvariantFailure::DirtyRead {
                                key: key.clone(),
                                observed: "MISSING".to_string(),
                                expected: expected_latest.clone(),
                                sequence: event.sequence,
                            });
                        }
                        ObservedValue::Present(observed_value)
                            if observed_value != expected_latest =>
                        {
                            failures.push(InvariantFailure::DirtyRead {
                                key: key.clone(),
                                observed: observed_value.clone(),
                                expected: expected_latest.clone(),
                                sequence: event.sequence,
                            });
                        }
                        ObservedValue::Present(_) => {}
                    }
                }

                if let Some(ack_ids) = ack_ids_by_key.get(key) {
                    for ack_id in ack_ids {
                        if let Some(acked) = acked_writes.get_mut(ack_id)
                            && event.sequence >= acked.ack_sequence
                        {
                            acked.any_post_ack_read = true;
                            if let ObservedValue::Present(observed_value) = observed
                                && observed_value == &acked.value
                            {
                                acked.value_observed_after_ack = true;
                            }
                        }
                    }
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

    let mut gaps = Vec::new();
    for (ack_id, state) in acked_writes {
        if !state.any_post_ack_read {
            gaps.push(ObservationGap {
                ack_id,
                key: state.key,
                value: state.value,
                ack_sequence: state.ack_sequence,
            });
            continue;
        }
        if !state.value_observed_after_ack {
            failures.push(InvariantFailure::LostAcknowledgedWrite {
                ack_id,
                key: state.key,
                value: state.value,
            });
        }
    }

    if !failures.is_empty() {
        InvariantCheckOutcome::Fail(failures)
    } else if !gaps.is_empty() {
        InvariantCheckOutcome::InsufficientObservation(gaps)
    } else {
        InvariantCheckOutcome::Pass
    }
}

pub fn check_history(events: &[HistoryEvent]) -> Result<(), Vec<InvariantFailure>> {
    match check_history_outcome(events) {
        InvariantCheckOutcome::Pass => Ok(()),
        InvariantCheckOutcome::Fail(failures) => Err(failures),
        InvariantCheckOutcome::InsufficientObservation(gaps) => Err(gaps
            .into_iter()
            .map(|gap| InvariantFailure::InsufficientObservation {
                ack_id: gap.ack_id,
                key: gap.key,
                value: gap.value,
                ack_sequence: gap.ack_sequence,
            })
            .collect()),
    }
}
