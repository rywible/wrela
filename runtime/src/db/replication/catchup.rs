use crate::db::raft::install_snapshot::{
    InstallSnapshotDisposition, InstallSnapshotRequest, handle_install_snapshot,
};
use crate::db::raft::snapshot::{LogTruncationPlan, plan_log_truncation};
use crate::db::raft::state::NodeState;
use crate::db::snapshot::builder::build_manifest;
use crate::db::snapshot::manifest::SnapshotValidationError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatchUpAction {
    UpToDate,
    SendLogFrom {
        start_index: u64,
    },
    InstallSnapshotThenLog {
        snapshot_index: u64,
        snapshot_term: u64,
        tail_start_index: Option<u64>,
    },
    FollowerAhead {
        follower_match_index: u64,
        leader_last_index: u64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatchUpExecutionError {
    MissingSnapshotPayload,
    FollowerAhead {
        follower_match_index: u64,
        leader_last_index: u64,
    },
    SnapshotValidation(SnapshotValidationError),
    InstallRejected(InstallSnapshotDisposition),
    InvalidTruncation {
        snapshot_last_included_index: u64,
        committed_index: u64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatchUpExecution {
    pub action: CatchUpAction,
    pub snapshot_applied: bool,
    pub next_log_start: Option<u64>,
    pub truncation: Option<LogTruncationPlan>,
}

pub fn plan_catch_up(
    follower_match_index: u64,
    leader_snapshot_index: u64,
    leader_snapshot_term: u64,
    leader_last_log_index: u64,
) -> CatchUpAction {
    if follower_match_index > leader_last_log_index {
        return CatchUpAction::FollowerAhead {
            follower_match_index,
            leader_last_index: leader_last_log_index,
        };
    }

    if follower_match_index == leader_last_log_index {
        return CatchUpAction::UpToDate;
    }

    if follower_match_index < leader_snapshot_index {
        let tail_start_index = leader_snapshot_index
            .checked_add(1)
            .filter(|start| *start <= leader_last_log_index);
        return CatchUpAction::InstallSnapshotThenLog {
            snapshot_index: leader_snapshot_index,
            snapshot_term: leader_snapshot_term,
            tail_start_index,
        };
    }

    CatchUpAction::SendLogFrom {
        start_index: follower_match_index.saturating_add(1),
    }
}

#[allow(clippy::too_many_arguments)]
pub fn execute_catch_up(
    follower_state: &mut NodeState,
    follower_match_index: u64,
    follower_last_snapshot_index: u64,
    follower_last_snapshot_term: u64,
    leader_term: u64,
    leader_snapshot_index: u64,
    leader_snapshot_term: u64,
    leader_last_log_index: u64,
    leader_current_first_log_index: u64,
    leader_committed_index: u64,
    retention_entries: u64,
    now_tick: u64,
    election_timeout_ticks: u64,
    snapshot_payload: Option<&[u8]>,
) -> Result<CatchUpExecution, CatchUpExecutionError> {
    let action = plan_catch_up(
        follower_match_index,
        leader_snapshot_index,
        leader_snapshot_term,
        leader_last_log_index,
    );

    match action {
        CatchUpAction::UpToDate => Ok(CatchUpExecution {
            action,
            snapshot_applied: false,
            next_log_start: None,
            truncation: None,
        }),
        CatchUpAction::FollowerAhead {
            follower_match_index,
            leader_last_index,
        } => Err(CatchUpExecutionError::FollowerAhead {
            follower_match_index,
            leader_last_index,
        }),
        CatchUpAction::SendLogFrom { start_index } => Ok(CatchUpExecution {
            action,
            snapshot_applied: false,
            next_log_start: Some(start_index),
            truncation: None,
        }),
        CatchUpAction::InstallSnapshotThenLog {
            snapshot_index,
            snapshot_term,
            tail_start_index,
        } => {
            let payload = snapshot_payload.ok_or(CatchUpExecutionError::MissingSnapshotPayload)?;
            let manifest = build_manifest(payload, snapshot_index, snapshot_term);
            manifest
                .validate_payload(payload)
                .map_err(CatchUpExecutionError::SnapshotValidation)?;

            let req = InstallSnapshotRequest {
                term: leader_term,
                leader_id: 0,
                last_included_index: snapshot_index,
                last_included_term: snapshot_term,
            };
            let install = handle_install_snapshot(
                follower_state,
                &req,
                follower_last_snapshot_index,
                follower_last_snapshot_term,
                now_tick,
                election_timeout_ticks,
            );

            let snapshot_applied = match install {
                InstallSnapshotDisposition::Applied { .. } => true,
                InstallSnapshotDisposition::Duplicate => false,
                other => return Err(CatchUpExecutionError::InstallRejected(other)),
            };

            let truncation = plan_log_truncation(
                leader_current_first_log_index,
                snapshot_index,
                leader_committed_index,
                retention_entries,
            );
            if let LogTruncationPlan::InvalidSnapshotIndex {
                snapshot_last_included_index,
                committed_index,
            } = truncation
            {
                return Err(CatchUpExecutionError::InvalidTruncation {
                    snapshot_last_included_index,
                    committed_index,
                });
            }

            Ok(CatchUpExecution {
                action,
                snapshot_applied,
                next_log_start: tail_start_index,
                truncation: Some(truncation),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::raft::state::NodeState;

    struct CatchUpHarness {
        follower_state: NodeState,
        follower_match_index: u64,
        follower_last_snapshot_index: u64,
        follower_last_snapshot_term: u64,
        leader_term: u64,
        leader_snapshot_index: u64,
        leader_snapshot_term: u64,
        leader_last_log_index: u64,
        leader_current_first_log_index: u64,
        leader_committed_index: u64,
        retention_entries: u64,
        now_tick: u64,
        election_timeout_ticks: u64,
    }

    impl CatchUpHarness {
        fn run_round(
            &mut self,
            snapshot_payload: Option<&[u8]>,
        ) -> Result<CatchUpExecution, CatchUpExecutionError> {
            let execution = execute_catch_up(
                &mut self.follower_state,
                self.follower_match_index,
                self.follower_last_snapshot_index,
                self.follower_last_snapshot_term,
                self.leader_term,
                self.leader_snapshot_index,
                self.leader_snapshot_term,
                self.leader_last_log_index,
                self.leader_current_first_log_index,
                self.leader_committed_index,
                self.retention_entries,
                self.now_tick,
                self.election_timeout_ticks,
                snapshot_payload,
            )?;

            match execution.action {
                CatchUpAction::UpToDate => {}
                CatchUpAction::FollowerAhead { .. } => {}
                CatchUpAction::SendLogFrom { .. } => {
                    self.follower_match_index = self.leader_last_log_index;
                }
                CatchUpAction::InstallSnapshotThenLog {
                    snapshot_index,
                    snapshot_term,
                    tail_start_index,
                } => {
                    if execution.snapshot_applied {
                        self.follower_last_snapshot_index = snapshot_index;
                        self.follower_last_snapshot_term = snapshot_term;
                        self.follower_match_index = self.follower_match_index.max(snapshot_index);
                    }
                    if tail_start_index.is_some() {
                        self.follower_match_index = self.leader_last_log_index;
                    }
                }
            }

            if let Some(LogTruncationPlan::TruncatePrefixTo {
                new_first_log_index,
            }) = execution.truncation
            {
                self.leader_current_first_log_index = new_first_log_index;
            }

            self.now_tick = self.now_tick.saturating_add(1);
            Ok(execution)
        }
    }

    #[test]
    fn returns_up_to_date_when_indices_match() {
        let action = plan_catch_up(40, 20, 7, 40);
        assert_eq!(action, CatchUpAction::UpToDate);
    }

    #[test]
    fn plans_log_only_when_follower_is_within_log_window() {
        let action = plan_catch_up(30, 20, 7, 40);
        assert_eq!(action, CatchUpAction::SendLogFrom { start_index: 31 });
    }

    #[test]
    fn plans_snapshot_and_tail_when_follower_is_behind_snapshot_boundary() {
        let action = plan_catch_up(10, 20, 7, 40);
        assert_eq!(
            action,
            CatchUpAction::InstallSnapshotThenLog {
                snapshot_index: 20,
                snapshot_term: 7,
                tail_start_index: Some(21),
            }
        );
    }

    #[test]
    fn omits_tail_when_snapshot_reaches_current_tip() {
        let action = plan_catch_up(10, 20, 7, 20);
        assert_eq!(
            action,
            CatchUpAction::InstallSnapshotThenLog {
                snapshot_index: 20,
                snapshot_term: 7,
                tail_start_index: None,
            }
        );
    }

    #[test]
    fn flags_follower_ahead_as_inconsistent() {
        let action = plan_catch_up(41, 20, 7, 40);
        assert_eq!(
            action,
            CatchUpAction::FollowerAhead {
                follower_match_index: 41,
                leader_last_index: 40,
            }
        );
    }

    #[test]
    fn execute_catch_up_applies_snapshot_and_plans_tail_and_truncation() {
        let mut state = NodeState::with_timing(2, 0, 10);
        state.current_term = 3;
        let payload = b"snapshot-data";

        let result = execute_catch_up(
            &mut state,
            10,
            5,
            2,
            4,
            20,
            7,
            40,
            1,
            40,
            10,
            50,
            15,
            Some(payload),
        )
        .expect("catch-up should succeed");

        assert!(result.snapshot_applied);
        assert_eq!(result.next_log_start, Some(21));
        assert_eq!(
            result.truncation,
            Some(LogTruncationPlan::TruncatePrefixTo {
                new_first_log_index: 11
            })
        );
    }

    #[test]
    fn execute_catch_up_rejects_missing_snapshot_payload() {
        let mut state = NodeState::with_timing(2, 0, 10);
        let err = execute_catch_up(&mut state, 0, 0, 0, 2, 20, 7, 40, 1, 40, 0, 10, 10, None)
            .expect_err("payload is required");
        assert_eq!(err, CatchUpExecutionError::MissingSnapshotPayload);
    }

    #[test]
    fn execute_catch_up_rejects_stale_leader_term_for_snapshot_install() {
        let mut state = NodeState::with_timing(2, 0, 10);
        state.current_term = 9;
        let err = execute_catch_up(
            &mut state,
            0,
            0,
            0,
            8,
            20,
            7,
            40,
            1,
            40,
            0,
            10,
            10,
            Some(b"x"),
        )
        .expect_err("stale term should be rejected");
        assert_eq!(
            err,
            CatchUpExecutionError::InstallRejected(InstallSnapshotDisposition::StaleTerm)
        );
    }

    #[test]
    fn execute_catch_up_rejects_follower_ahead_inconsistency() {
        let mut state = NodeState::with_timing(2, 0, 10);
        let err = execute_catch_up(
            &mut state,
            50,
            10,
            2,
            5,
            20,
            7,
            40,
            1,
            40,
            0,
            10,
            10,
            Some(b"x"),
        )
        .expect_err("follower-ahead should be rejected");
        assert_eq!(
            err,
            CatchUpExecutionError::FollowerAhead {
                follower_match_index: 50,
                leader_last_index: 40
            }
        );
    }

    #[test]
    fn integration_round_trip_snapshot_then_tail_then_steady_state() {
        let mut harness = CatchUpHarness {
            follower_state: NodeState::with_timing(2, 0, 10),
            follower_match_index: 5,
            follower_last_snapshot_index: 5,
            follower_last_snapshot_term: 2,
            leader_term: 4,
            leader_snapshot_index: 20,
            leader_snapshot_term: 7,
            leader_last_log_index: 40,
            leader_current_first_log_index: 1,
            leader_committed_index: 40,
            retention_entries: 10,
            now_tick: 50,
            election_timeout_ticks: 15,
        };

        let first = harness
            .run_round(Some(b"snapshot-data"))
            .expect("first catch-up round should succeed");
        assert_eq!(
            first.action,
            CatchUpAction::InstallSnapshotThenLog {
                snapshot_index: 20,
                snapshot_term: 7,
                tail_start_index: Some(21),
            }
        );
        assert!(first.snapshot_applied);
        assert_eq!(
            first.truncation,
            Some(LogTruncationPlan::TruncatePrefixTo {
                new_first_log_index: 11
            })
        );
        assert_eq!(harness.follower_last_snapshot_index, 20);
        assert_eq!(harness.follower_last_snapshot_term, 7);
        assert_eq!(harness.follower_match_index, 40);
        assert_eq!(harness.leader_current_first_log_index, 11);

        let second = harness
            .run_round(Some(b"snapshot-data"))
            .expect("second round should observe steady-state");
        assert_eq!(second.action, CatchUpAction::UpToDate);
        assert!(!second.snapshot_applied);
        assert_eq!(second.next_log_start, None);
        assert_eq!(second.truncation, None);
    }

    #[test]
    fn integration_duplicate_snapshot_is_idempotent_and_still_sends_tail() {
        let mut harness = CatchUpHarness {
            follower_state: NodeState::with_timing(2, 0, 10),
            follower_match_index: 10,
            follower_last_snapshot_index: 20,
            follower_last_snapshot_term: 7,
            leader_term: 8,
            leader_snapshot_index: 20,
            leader_snapshot_term: 7,
            leader_last_log_index: 30,
            leader_current_first_log_index: 1,
            leader_committed_index: 30,
            retention_entries: 0,
            now_tick: 100,
            election_timeout_ticks: 20,
        };

        let round = harness
            .run_round(Some(b"same-snapshot"))
            .expect("duplicate install should still permit tail catch-up");
        assert_eq!(
            round.action,
            CatchUpAction::InstallSnapshotThenLog {
                snapshot_index: 20,
                snapshot_term: 7,
                tail_start_index: Some(21),
            }
        );
        assert!(!round.snapshot_applied);
        assert_eq!(harness.follower_last_snapshot_index, 20);
        assert_eq!(harness.follower_last_snapshot_term, 7);
        assert_eq!(harness.follower_match_index, 30);
    }
}
