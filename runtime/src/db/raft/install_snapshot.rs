use crate::db::raft::state::{NodeState, Role};

#[derive(Debug, Clone)]
pub struct InstallSnapshotRequest {
    pub term: u64,
    pub leader_id: u64,
    pub last_included_index: u64,
    pub last_included_term: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallSnapshotDisposition {
    Applied {
        last_included_index: u64,
        last_included_term: u64,
    },
    Duplicate,
    StaleTerm,
    StaleSnapshot,
    InconsistentSnapshot,
}

pub fn handle_install_snapshot(
    state: &mut NodeState,
    req: &InstallSnapshotRequest,
    local_last_included_index: u64,
    local_last_included_term: u64,
    now_tick: u64,
    election_timeout_ticks: u64,
) -> InstallSnapshotDisposition {
    let _ = req.leader_id;
    if req.term < state.current_term {
        return InstallSnapshotDisposition::StaleTerm;
    }
    if req.term > state.current_term {
        state.current_term = req.term;
        state.voted_for = None;
    }

    state.role = Role::Follower;
    state.last_heartbeat_tick = now_tick;
    state.reset_election_deadline(now_tick, election_timeout_ticks);

    if req.last_included_index < local_last_included_index {
        return InstallSnapshotDisposition::StaleSnapshot;
    }

    if req.last_included_index == local_last_included_index {
        if req.last_included_term == local_last_included_term {
            return InstallSnapshotDisposition::Duplicate;
        }
        return InstallSnapshotDisposition::InconsistentSnapshot;
    }

    if local_last_included_index > 0 && req.last_included_term < local_last_included_term {
        return InstallSnapshotDisposition::InconsistentSnapshot;
    }

    InstallSnapshotDisposition::Applied {
        last_included_index: req.last_included_index,
        last_included_term: req.last_included_term,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_stale_term_without_state_change() {
        let mut state = NodeState::with_timing(1, 10, 5);
        state.current_term = 7;
        let original_deadline = state.election_deadline_tick;
        let req = InstallSnapshotRequest {
            term: 6,
            leader_id: 9,
            last_included_index: 100,
            last_included_term: 6,
        };

        let result = handle_install_snapshot(&mut state, &req, 80, 5, 12, 10);
        assert_eq!(result, InstallSnapshotDisposition::StaleTerm);
        assert_eq!(state.current_term, 7);
        assert_eq!(state.election_deadline_tick, original_deadline);
    }

    #[test]
    fn accepts_newer_term_and_applies_newer_snapshot() {
        let mut state = NodeState::with_timing(1, 0, 10);
        state.current_term = 3;
        state.role = Role::Leader;
        state.voted_for = Some(1);
        let req = InstallSnapshotRequest {
            term: 4,
            leader_id: 2,
            last_included_index: 120,
            last_included_term: 9,
        };

        let result = handle_install_snapshot(&mut state, &req, 100, 8, 50, 15);
        assert_eq!(
            result,
            InstallSnapshotDisposition::Applied {
                last_included_index: 120,
                last_included_term: 9,
            }
        );
        assert_eq!(state.current_term, 4);
        assert_eq!(state.voted_for, None);
        assert_eq!(state.role, Role::Follower);
        assert_eq!(state.last_heartbeat_tick, 50);
        assert!(state.election_deadline_tick > 50);
    }

    #[test]
    fn treats_equal_snapshot_as_duplicate() {
        let mut state = NodeState::with_timing(1, 0, 10);
        state.current_term = 5;
        let req = InstallSnapshotRequest {
            term: 5,
            leader_id: 2,
            last_included_index: 200,
            last_included_term: 10,
        };

        let result = handle_install_snapshot(&mut state, &req, 200, 10, 11, 10);
        assert_eq!(result, InstallSnapshotDisposition::Duplicate);
    }

    #[test]
    fn rejects_older_and_inconsistent_snapshots() {
        let mut state = NodeState::with_timing(1, 0, 10);
        state.current_term = 5;

        let stale = InstallSnapshotRequest {
            term: 5,
            leader_id: 2,
            last_included_index: 150,
            last_included_term: 9,
        };
        assert_eq!(
            handle_install_snapshot(&mut state, &stale, 151, 9, 20, 10),
            InstallSnapshotDisposition::StaleSnapshot
        );

        let conflicting_term = InstallSnapshotRequest {
            term: 5,
            leader_id: 2,
            last_included_index: 151,
            last_included_term: 8,
        };
        assert_eq!(
            handle_install_snapshot(&mut state, &conflicting_term, 151, 9, 21, 10),
            InstallSnapshotDisposition::InconsistentSnapshot
        );

        let non_monotonic_term = InstallSnapshotRequest {
            term: 5,
            leader_id: 2,
            last_included_index: 170,
            last_included_term: 7,
        };
        assert_eq!(
            handle_install_snapshot(&mut state, &non_monotonic_term, 151, 9, 22, 10),
            InstallSnapshotDisposition::InconsistentSnapshot
        );
    }
}
