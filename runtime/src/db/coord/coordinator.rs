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
    pub created_ms: u64,
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
        created_ms: u64,
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
                created_ms,
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
        if matches!(
            rec.state,
            CoordinatorState::Committing | CoordinatorState::Committed
        ) {
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
        match rec.state {
            CoordinatorState::Committing => {}
            CoordinatorState::Committed => {
                return Ok(true);
            }
            _ => {
                return Err(CoordinatorError::DecisionAlreadyFinalized(txn_id));
            }
        }

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

        if matches!(rec.decision, Some(Decision::Commit)) {
            return Err(CoordinatorError::DecisionAlreadyFinalized(txn_id));
        }

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

    pub fn finalize(&mut self, txn_id: u64) -> Option<CoordinatorRecord> {
        match self.txns.get(&txn_id) {
            Some(rec)
                if matches!(
                    rec.state,
                    CoordinatorState::Committed | CoordinatorState::Aborted
                ) =>
            {
                self.txns.remove(&txn_id)
            }
            _ => None,
        }
    }

    pub fn stale_txns(&self, now_ms: u64, timeout_ms: u64) -> Vec<u64> {
        self.txns
            .values()
            .filter(|rec| {
                rec.state == CoordinatorState::Preparing
                    && now_ms.saturating_sub(rec.created_ms) > timeout_ms
            })
            .map(|rec| rec.txn_id)
            .collect()
    }

    pub fn records(&self) -> &BTreeMap<u64, CoordinatorRecord> {
        &self.txns
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
        c.begin(7, BTreeSet::from([1, 2, 3]), 0).expect("begin");

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
    fn abort_ack_after_commit_decision_is_rejected() {
        let mut c = TwoPhaseCoordinator::default();
        c.begin(10, BTreeSet::from([1, 2]), 0).expect("begin");
        assert_eq!(c.on_prepare_ok(10, 1).expect("p1"), None);
        assert_eq!(c.on_prepare_ok(10, 2).expect("p2"), Some(Decision::Commit));
        // Decision is now Commit — an abort ack must be rejected.
        let err = c.on_abort_ack(10, 1).expect_err("must reject");
        assert_eq!(err, CoordinatorError::DecisionAlreadyFinalized(10));
        // State must remain Committing.
        assert_eq!(
            c.record(10).expect("record").state,
            CoordinatorState::Committing
        );
    }

    #[test]
    fn prepare_failed_during_committing_is_rejected() {
        let mut c = TwoPhaseCoordinator::default();
        c.begin(11, BTreeSet::from([1, 2, 3]), 0).expect("begin");
        c.on_prepare_ok(11, 1).expect("p1");
        c.on_prepare_ok(11, 2).expect("p2");
        assert_eq!(c.on_prepare_ok(11, 3).expect("p3"), Some(Decision::Commit));
        // Stale prepare_failed arriving after commit decision.
        let err = c.on_prepare_failed(11, 1).expect_err("must reject");
        assert_eq!(err, CoordinatorError::DecisionAlreadyFinalized(11));
        assert_eq!(
            c.record(11).expect("record").decision,
            Some(Decision::Commit)
        );
    }

    #[test]
    fn commit_ack_before_prepare_phase_complete_is_rejected() {
        let mut c = TwoPhaseCoordinator::default();
        c.begin(13, BTreeSet::from([1, 2]), 0).expect("begin");
        let err = c.on_commit_ack(13, 1).expect_err("must reject");
        assert_eq!(err, CoordinatorError::DecisionAlreadyFinalized(13));
        assert_eq!(
            c.record(13).expect("record").state,
            CoordinatorState::Preparing
        );
        assert_eq!(c.record(13).expect("record").decision, None);
    }

    #[test]
    fn abort_ack_after_committed_is_rejected() {
        let mut c = TwoPhaseCoordinator::default();
        c.begin(12, BTreeSet::from([1, 2]), 0).expect("begin");
        c.on_prepare_ok(12, 1).expect("p1");
        c.on_prepare_ok(12, 2).expect("p2");
        c.on_commit_ack(12, 1).expect("ack1");
        assert!(c.on_commit_ack(12, 2).expect("ack2"));
        assert_eq!(
            c.record(12).expect("record").state,
            CoordinatorState::Committed
        );
        // An abort ack after full commit must be rejected.
        let err = c.on_abort_ack(12, 1).expect_err("must reject");
        assert_eq!(err, CoordinatorError::DecisionAlreadyFinalized(12));
    }

    #[test]
    fn any_prepare_failure_switches_to_abort() {
        let mut c = TwoPhaseCoordinator::default();
        c.begin(9, BTreeSet::from([2, 4]), 0).expect("begin");
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

    #[test]
    fn finalize_removes_committed() {
        let mut c = TwoPhaseCoordinator::default();
        c.begin(1, BTreeSet::from([10, 20]), 100).expect("begin");
        c.on_prepare_ok(1, 10).expect("p10");
        c.on_prepare_ok(1, 20).expect("p20");
        c.on_commit_ack(1, 10).expect("ack10");
        c.on_commit_ack(1, 20).expect("ack20");
        assert_eq!(
            c.record(1).expect("record").state,
            CoordinatorState::Committed
        );

        let removed = c.finalize(1);
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().txn_id, 1);
        assert!(c.record(1).is_none());
    }

    #[test]
    fn finalize_refuses_in_flight() {
        let mut c = TwoPhaseCoordinator::default();
        c.begin(2, BTreeSet::from([10]), 100).expect("begin");
        // Still in Preparing state — finalize must refuse.
        assert!(c.finalize(2).is_none());
        assert!(c.record(2).is_some());
    }

    #[test]
    fn stale_txns_identifies_old_preparing() {
        let mut c = TwoPhaseCoordinator::default();
        c.begin(1, BTreeSet::from([10]), 100).expect("begin t1");
        c.begin(2, BTreeSet::from([20]), 200).expect("begin t2");
        // t1 was created at 100, t2 at 200.

        // At now=250 with timeout=100, t1 (age 150) is stale, t2 (age 50) is not.
        let stale = c.stale_txns(250, 100);
        assert_eq!(stale, vec![1]);

        // At now=350 with timeout=100, both are stale.
        let stale = c.stale_txns(350, 100);
        assert_eq!(stale, vec![1, 2]);

        // Move t1 to Committing — it should no longer appear in stale list.
        c.on_prepare_ok(1, 10).expect("p10");
        let stale = c.stale_txns(350, 100);
        assert_eq!(stale, vec![2]);
    }
}
