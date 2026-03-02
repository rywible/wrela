use crate::db::coord::coordinator::{CoordinatorRecord, CoordinatorState, Decision};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryAction {
    ReprepareParticipant {
        txn_id: u64,
        epoch: u64,
        participant_id: u64,
    },
    RecommitParticipant {
        txn_id: u64,
        epoch: u64,
        participant_id: u64,
    },
    ReabortParticipant {
        txn_id: u64,
        epoch: u64,
        participant_id: u64,
    },
    FinalizeCommit,
    FinalizeAbort,
}

pub fn recovery_actions(record: &CoordinatorRecord) -> Vec<RecoveryAction> {
    let mut actions = Vec::new();
    let txn_id = record.txn_id;
    let epoch = record.epoch;

    match record.decision {
        None => {
            for participant_id in &record.participants {
                if !record.aborted.contains(participant_id) {
                    actions.push(RecoveryAction::ReabortParticipant {
                        txn_id,
                        epoch,
                        participant_id: *participant_id,
                    });
                }
            }
            if record.state == CoordinatorState::Preparing
                && record.aborted.len() == record.participants.len()
            {
                actions.push(RecoveryAction::FinalizeAbort);
            }
        }
        Some(Decision::Commit) => {
            for participant_id in &record.participants {
                if !record.committed.contains(participant_id) {
                    actions.push(RecoveryAction::RecommitParticipant {
                        txn_id,
                        epoch,
                        participant_id: *participant_id,
                    });
                }
            }
            if record.state == CoordinatorState::Committing
                && record.committed.len() == record.participants.len()
            {
                actions.push(RecoveryAction::FinalizeCommit);
            }
        }
        Some(Decision::Abort) => {
            for participant_id in &record.participants {
                if !record.aborted.contains(participant_id) {
                    actions.push(RecoveryAction::ReabortParticipant {
                        txn_id,
                        epoch,
                        participant_id: *participant_id,
                    });
                }
            }
            if record.state == CoordinatorState::Aborting
                && record.aborted.len() == record.participants.len()
            {
                actions.push(RecoveryAction::FinalizeAbort);
            }
        }
    }

    actions
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::coord::coordinator::CoordinatorRecord;
    use std::collections::BTreeSet;

    #[test]
    fn commit_recovery_rebroadcasts_missing_acks() {
        let record = CoordinatorRecord {
            txn_id: 10,
            epoch: 3,
            created_ms: 0,
            participants: BTreeSet::from([1, 2, 3]),
            prepared: BTreeSet::from([1, 2, 3]),
            committed: BTreeSet::from([1]),
            aborted: BTreeSet::new(),
            decision: Some(Decision::Commit),
            state: CoordinatorState::Committing,
        };

        let actions = recovery_actions(&record);
        assert_eq!(
            actions,
            vec![
                RecoveryAction::RecommitParticipant {
                    txn_id: 10,
                    epoch: 3,
                    participant_id: 2
                },
                RecoveryAction::RecommitParticipant {
                    txn_id: 10,
                    epoch: 3,
                    participant_id: 3
                },
            ]
        );
    }

    #[test]
    fn prepare_recovery_aborts_all_participants() {
        let record = CoordinatorRecord {
            txn_id: 22,
            epoch: 1,
            created_ms: 0,
            participants: BTreeSet::from([2, 5]),
            prepared: BTreeSet::from([2]),
            committed: BTreeSet::new(),
            aborted: BTreeSet::new(),
            decision: None,
            state: CoordinatorState::Preparing,
        };

        assert_eq!(
            recovery_actions(&record),
            vec![
                RecoveryAction::ReabortParticipant {
                    txn_id: 22,
                    epoch: 1,
                    participant_id: 2
                },
                RecoveryAction::ReabortParticipant {
                    txn_id: 22,
                    epoch: 1,
                    participant_id: 5
                }
            ]
        );
    }
}
