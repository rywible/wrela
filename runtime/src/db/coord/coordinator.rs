use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Commit,
    Abort,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoordinatorState {
    Preparing,
    Committing,
    Aborting,
    Committed,
    Aborted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoordinatorRecord {
    pub txn_id: u64,
    pub epoch: u64,
    pub participants: BTreeSet<u64>,
    pub prepared: BTreeSet<u64>,
    pub committed: BTreeSet<u64>,
    pub aborted: BTreeSet<u64>,
    pub decision: Option<Decision>,
    pub state: CoordinatorState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoordinatorError {
    AlreadyExists(u64),
    UnknownTxn(u64),
    EmptyParticipants,
    DecisionAlreadyFinalized(u64),
    ParticipantNotInTxn { txn_id: u64, participant_id: u64 },
}

#[derive(Debug, Default)]
pub struct TwoPhaseCoordinator {
    txns: BTreeMap<u64, CoordinatorRecord>,
}

impl TwoPhaseCoordinator {
    pub fn begin(
        &mut self,
        txn_id: u64,
        participants: BTreeSet<u64>,
    ) -> Result<(), CoordinatorError> {
        if participants.is_empty() {
            return Err(CoordinatorError::EmptyParticipants);
        }
        if self.txns.contains_key(&txn_id) {
            return Err(CoordinatorError::AlreadyExists(txn_id));
        }
        self.txns.insert(
            txn_id,
            CoordinatorRecord {
                txn_id,
                epoch: 1,
                participants,
                prepared: BTreeSet::new(),
                committed: BTreeSet::new(),
                aborted: BTreeSet::new(),
                decision: None,
                state: CoordinatorState::Preparing,
            },
        );
        Ok(())
    }

    pub fn on_prepare_ok(
        &mut self,
        txn_id: u64,
        participant_id: u64,
    ) -> Result<Option<Decision>, CoordinatorError> {
        let rec = self
            .txns
            .get_mut(&txn_id)
            .ok_or(CoordinatorError::UnknownTxn(txn_id))?;
        ensure_participant(rec, participant_id)?;

        if matches!(
            rec.state,
            CoordinatorState::Aborting | CoordinatorState::Aborted
        ) {
            return Ok(Some(Decision::Abort));
        }

        rec.prepared.insert(participant_id);
        if rec.prepared.len() == rec.participants.len() {
            rec.decision = Some(Decision::Commit);
            rec.state = CoordinatorState::Committing;
            return Ok(Some(Decision::Commit));
        }
        Ok(None)
    }

    pub fn on_prepare_failed(
        &mut self,
        txn_id: u64,
        participant_id: u64,
    ) -> Result<Decision, CoordinatorError> {
        let rec = self
            .txns
            .get_mut(&txn_id)
            .ok_or(CoordinatorError::UnknownTxn(txn_id))?;
        ensure_participant(rec, participant_id)?;
        if matches!(rec.state, CoordinatorState::Committed) {
            return Err(CoordinatorError::DecisionAlreadyFinalized(txn_id));
        }
        rec.decision = Some(Decision::Abort);
        rec.state = CoordinatorState::Aborting;
        Ok(Decision::Abort)
    }

    pub fn on_commit_ack(
        &mut self,
        txn_id: u64,
        participant_id: u64,
    ) -> Result<bool, CoordinatorError> {
        let rec = self
            .txns
            .get_mut(&txn_id)
            .ok_or(CoordinatorError::UnknownTxn(txn_id))?;
        ensure_participant(rec, participant_id)?;

        if rec.decision == Some(Decision::Abort) {
            return Err(CoordinatorError::DecisionAlreadyFinalized(txn_id));
        }

        rec.decision = Some(Decision::Commit);
        rec.state = CoordinatorState::Committing;
        rec.committed.insert(participant_id);
        if rec.committed.len() == rec.participants.len() {
            rec.state = CoordinatorState::Committed;
            return Ok(true);
        }
        Ok(false)
    }

    pub fn on_abort_ack(
        &mut self,
        txn_id: u64,
        participant_id: u64,
    ) -> Result<bool, CoordinatorError> {
        let rec = self
            .txns
            .get_mut(&txn_id)
            .ok_or(CoordinatorError::UnknownTxn(txn_id))?;
        ensure_participant(rec, participant_id)?;

        rec.decision = Some(Decision::Abort);
        rec.state = CoordinatorState::Aborting;
        rec.aborted.insert(participant_id);
        if rec.aborted.len() == rec.participants.len() {
            rec.state = CoordinatorState::Aborted;
            return Ok(true);
        }
        Ok(false)
    }

    pub fn record(&self, txn_id: u64) -> Option<&CoordinatorRecord> {
        self.txns.get(&txn_id)
    }

    pub fn recover_record(&mut self, record: CoordinatorRecord) {
        self.txns.insert(record.txn_id, record);
    }
}

fn ensure_participant(
    rec: &CoordinatorRecord,
    participant_id: u64,
) -> Result<(), CoordinatorError> {
    if rec.participants.contains(&participant_id) {
        Ok(())
    } else {
        Err(CoordinatorError::ParticipantNotInTxn {
            txn_id: rec.txn_id,
            participant_id,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commit_path_is_idempotent_and_all_or_nothing() {
        let mut c = TwoPhaseCoordinator::default();
        c.begin(7, BTreeSet::from([1, 2, 3])).expect("begin");

        assert_eq!(c.on_prepare_ok(7, 1).expect("p1"), None);
        assert_eq!(c.on_prepare_ok(7, 2).expect("p2"), None);
        assert_eq!(c.on_prepare_ok(7, 3).expect("p3"), Some(Decision::Commit));

        assert!(!c.on_commit_ack(7, 1).expect("ack1"));
        assert!(!c.on_commit_ack(7, 2).expect("ack2"));
        assert!(c.on_commit_ack(7, 3).expect("ack3"));

        // idempotent repeated ack
        assert!(c.on_commit_ack(7, 3).expect("ack3 repeat"));
        assert_eq!(
            c.record(7).expect("record").state,
            CoordinatorState::Committed
        );
    }

    #[test]
    fn any_prepare_failure_switches_to_abort() {
        let mut c = TwoPhaseCoordinator::default();
        c.begin(9, BTreeSet::from([2, 4])).expect("begin");
        assert_eq!(c.on_prepare_ok(9, 2).expect("p2"), None);
        assert_eq!(c.on_prepare_failed(9, 4).expect("fail"), Decision::Abort);
        assert_eq!(
            c.record(9).expect("record").state,
            CoordinatorState::Aborting
        );

        assert!(!c.on_abort_ack(9, 2).expect("ack2"));
        assert!(c.on_abort_ack(9, 4).expect("ack4"));
        assert_eq!(
            c.record(9).expect("record").state,
            CoordinatorState::Aborted
        );
    }
}
