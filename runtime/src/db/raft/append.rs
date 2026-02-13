use crate::db::raft::message::{AppendEntries, AppendEntriesResponse};
use crate::db::raft::state::{NodeState, Role};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppendDisposition {
    Applied,
    Duplicate,
    Stale,
    Conflict,
}

#[derive(Debug, Default)]
pub struct AppendProgressTracker {
    by_follower: HashMap<u64, (u64, u64)>,
}

impl AppendProgressTracker {
    pub fn record_follower_append(
        &mut self,
        follower_id: u64,
        term: u64,
        log_index: u64,
    ) -> AppendDisposition {
        match self.by_follower.get(&follower_id).copied() {
            None => {
                self.by_follower.insert(follower_id, (term, log_index));
                AppendDisposition::Applied
            }
            Some((seen_term, _seen_index)) if term > seen_term => {
                self.by_follower.insert(follower_id, (term, log_index));
                AppendDisposition::Conflict
            }
            Some((seen_term, seen_index)) if term < seen_term || log_index < seen_index => {
                AppendDisposition::Stale
            }
            Some((seen_term, seen_index)) if term == seen_term && log_index == seen_index => {
                AppendDisposition::Duplicate
            }
            Some((seen_term, seen_index)) if term == seen_term && log_index > seen_index => {
                self.by_follower.insert(follower_id, (term, log_index));
                AppendDisposition::Applied
            }
            _ => AppendDisposition::Stale,
        }
    }

    pub fn record_follower_response(
        &mut self,
        follower_id: u64,
        response: &AppendEntriesResponse,
    ) -> AppendDisposition {
        if response.success {
            return self.record_follower_append(follower_id, response.term, response.match_index);
        }

        let conflict_index = response.conflict_index.unwrap_or(response.match_index);
        match self.by_follower.get(&follower_id).copied() {
            None => {
                self.by_follower
                    .insert(follower_id, (response.term, conflict_index));
                AppendDisposition::Conflict
            }
            Some((seen_term, _seen_index)) if response.term < seen_term => AppendDisposition::Stale,
            Some((seen_term, seen_index))
                if response.term == seen_term && conflict_index == seen_index =>
            {
                AppendDisposition::Duplicate
            }
            _ => {
                self.by_follower
                    .insert(follower_id, (response.term, conflict_index));
                AppendDisposition::Conflict
            }
        }
    }
}

pub fn handle_append_entries(
    state: &mut NodeState,
    req: &AppendEntries,
    now_tick: u64,
    election_timeout_ticks: u64,
) -> bool {
    if req.term < state.current_term {
        return false;
    }
    if req.term > state.current_term {
        state.current_term = req.term;
        state.voted_for = None;
    }
    state.role = Role::Follower;
    state.last_heartbeat_tick = now_tick;
    state.reset_election_deadline(now_tick, election_timeout_ticks);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_entries_with_newer_term_steps_down_to_follower() {
        let mut state = NodeState::with_timing(1, 0, 10);
        state.current_term = 2;
        state.role = Role::Leader;
        state.voted_for = Some(1);
        let req = AppendEntries {
            term: 3,
            leader_id: 9,
            prev_log_index: 0,
            prev_log_term: 0,
            leader_commit: 0,
        };
        assert!(handle_append_entries(&mut state, &req, 15, 12));
        assert_eq!(state.current_term, 3);
        assert_eq!(state.role, Role::Follower);
        assert_eq!(state.voted_for, None);
        assert_eq!(state.last_heartbeat_tick, 15);
        assert!(state.election_deadline_tick > 15);
    }

    #[test]
    fn append_entries_rejects_stale_term() {
        let mut state = NodeState::with_timing(1, 0, 10);
        state.current_term = 5;
        let req = AppendEntries {
            term: 4,
            leader_id: 9,
            prev_log_index: 0,
            prev_log_term: 0,
            leader_commit: 0,
        };
        assert!(!handle_append_entries(&mut state, &req, 20, 10));
        assert_eq!(state.current_term, 5);
    }

    #[test]
    fn append_tracker_treats_retries_idempotently() {
        let mut tracker = AppendProgressTracker::default();
        assert_eq!(
            tracker.record_follower_append(2, 7, 100),
            AppendDisposition::Applied
        );
        assert_eq!(
            tracker.record_follower_append(2, 7, 100),
            AppendDisposition::Duplicate
        );
        assert_eq!(
            tracker.record_follower_append(2, 7, 99),
            AppendDisposition::Stale
        );
        assert_eq!(
            tracker.record_follower_append(2, 7, 101),
            AppendDisposition::Applied
        );
    }

    #[test]
    fn append_tracker_detects_conflicting_term_advance() {
        let mut tracker = AppendProgressTracker::default();
        assert_eq!(
            tracker.record_follower_append(3, 5, 88),
            AppendDisposition::Applied
        );
        assert_eq!(
            tracker.record_follower_append(3, 6, 10),
            AppendDisposition::Conflict
        );
    }

    #[test]
    fn append_tracker_records_success_and_conflict_responses() {
        let mut tracker = AppendProgressTracker::default();
        let success = AppendEntriesResponse {
            term: 5,
            success: true,
            match_index: 42,
            conflict_index: None,
        };
        assert_eq!(
            tracker.record_follower_response(7, &success),
            AppendDisposition::Applied
        );

        let conflict = AppendEntriesResponse {
            term: 5,
            success: false,
            match_index: 41,
            conflict_index: Some(12),
        };
        assert_eq!(
            tracker.record_follower_response(7, &conflict),
            AppendDisposition::Conflict
        );
    }

    #[test]
    fn append_tracker_rejects_stale_failure_responses() {
        let mut tracker = AppendProgressTracker::default();
        assert_eq!(
            tracker.record_follower_append(4, 9, 120),
            AppendDisposition::Applied
        );
        let stale_failure = AppendEntriesResponse {
            term: 8,
            success: false,
            match_index: 100,
            conflict_index: Some(50),
        };
        assert_eq!(
            tracker.record_follower_response(4, &stale_failure),
            AppendDisposition::Stale
        );
    }
}
