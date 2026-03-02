#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParticipantState {
    Idle,
    Prepared { txn_id: u64, epoch: u64 },
    Committed { txn_id: u64, epoch: u64 },
    Aborted { txn_id: u64, epoch: u64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParticipantError {
    AlreadyPreparedDifferentTxn { active_txn: u64, requested_txn: u64 },
    EpochRegression { seen: u64, got: u64 },
}

#[derive(Debug, Clone)]
pub struct ParticipantFsm {
    state: ParticipantState,
}

impl Default for ParticipantFsm {
    fn default() -> Self {
        Self {
            state: ParticipantState::Idle,
        }
    }
}

impl ParticipantFsm {
    pub fn state(&self) -> ParticipantState {
        self.state
    }

    pub fn on_prepare(&mut self, txn_id: u64, epoch: u64) -> Result<bool, ParticipantError> {
        match self.state {
            ParticipantState::Idle => {
                self.state = ParticipantState::Prepared { txn_id, epoch };
                Ok(true)
            }
            ParticipantState::Prepared {
                txn_id: active,
                epoch: seen,
            } => {
                if txn_id != active {
                    return Err(ParticipantError::AlreadyPreparedDifferentTxn {
                        active_txn: active,
                        requested_txn: txn_id,
                    });
                }
                if epoch < seen {
                    return Err(ParticipantError::EpochRegression { seen, got: epoch });
                }
                self.state = ParticipantState::Prepared { txn_id, epoch };
                Ok(false)
            }
            ParticipantState::Committed {
                txn_id: done,
                epoch: seen,
            }
            | ParticipantState::Aborted {
                txn_id: done,
                epoch: seen,
            } => {
                if txn_id == done && epoch >= seen {
                    Ok(false)
                } else if epoch < seen {
                    Err(ParticipantError::EpochRegression { seen, got: epoch })
                } else {
                    Err(ParticipantError::AlreadyPreparedDifferentTxn {
                        active_txn: done,
                        requested_txn: txn_id,
                    })
                }
            }
        }
    }

    pub fn on_commit(&mut self, txn_id: u64, epoch: u64) -> Result<bool, ParticipantError> {
        self.transition_terminal(txn_id, epoch, true)
    }

    pub fn on_abort(&mut self, txn_id: u64, epoch: u64) -> Result<bool, ParticipantError> {
        self.transition_terminal(txn_id, epoch, false)
    }

    /// Transition to a terminal state (Committed or Aborted).
    /// Idle -> Committed/Aborted is allowed for recovery: the coordinator may push a decision
    /// to a participant that restarted and lost in-memory state, so it never prepared.
    fn transition_terminal(
        &mut self,
        txn_id: u64,
        epoch: u64,
        commit: bool,
    ) -> Result<bool, ParticipantError> {
        match self.state {
            ParticipantState::Idle | ParticipantState::Prepared { .. } => {
                self.state = if commit {
                    ParticipantState::Committed { txn_id, epoch }
                } else {
                    ParticipantState::Aborted { txn_id, epoch }
                };
                Ok(true)
            }
            ParticipantState::Committed {
                txn_id: done,
                epoch: seen,
            }
            | ParticipantState::Aborted {
                txn_id: done,
                epoch: seen,
            } => {
                if txn_id != done {
                    return Err(ParticipantError::AlreadyPreparedDifferentTxn {
                        active_txn: done,
                        requested_txn: txn_id,
                    });
                }
                if epoch < seen {
                    return Err(ParticipantError::EpochRegression { seen, got: epoch });
                }
                Ok(false)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn participant_prepare_commit_is_idempotent() {
        let mut p = ParticipantFsm::default();
        assert!(p.on_prepare(99, 1).expect("prepare"));
        assert!(!p.on_prepare(99, 1).expect("prepare retry"));
        assert!(p.on_commit(99, 1).expect("commit"));
        assert!(!p.on_commit(99, 1).expect("commit retry"));
        assert_eq!(
            p.state(),
            ParticipantState::Committed {
                txn_id: 99,
                epoch: 1
            }
        );
    }

    #[test]
    fn rejects_cross_txn_reuse() {
        let mut p = ParticipantFsm::default();
        p.on_prepare(1, 7).expect("prepare");
        let err = p.on_prepare(2, 7).expect_err("must fail");
        assert_eq!(
            err,
            ParticipantError::AlreadyPreparedDifferentTxn {
                active_txn: 1,
                requested_txn: 2,
            }
        );
    }
}
