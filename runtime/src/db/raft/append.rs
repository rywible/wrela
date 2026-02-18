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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppendApplyResult {
    pub response: AppendEntriesResponse,
    pub appended_entries: usize,
    pub commit_index: u64,
}

pub fn handle_append_entries(
    state: &mut NodeState,
    req: &AppendEntries,
    now_tick: u64,
    election_timeout_ticks: u64,
) -> AppendApplyResult {
    if req.term < state.current_term {
        return reject(
            state,
            state.last_log_index().saturating_add(1),
            0,
            state.commit_index,
        );
    }
    if req.term > state.current_term {
        state.current_term = req.term;
        state.voted_for = None;
    }

    state.role = Role::Follower;
    state.last_heartbeat_tick = now_tick;
    state.reset_election_deadline(now_tick, election_timeout_ticks);

    if req.prev_log_index > state.last_log_index() {
        return reject(
            state,
            state.last_log_index().saturating_add(1),
            0,
            state.commit_index,
        );
    }

    if req.prev_log_index > 0 {
        let local_prev_term = state.log_term_at(req.prev_log_index).unwrap_or(0);
        if local_prev_term != req.prev_log_term {
            let conflict_index = first_index_for_term(state, local_prev_term, req.prev_log_index);
            return reject(state, conflict_index, 0, state.commit_index);
        }
    }

    let mut appended_entries = 0usize;
    for (idx, entry) in req.entries.iter().enumerate() {
        let expected_index = if idx == 0 {
            req.prev_log_index.saturating_add(1)
        } else {
            req.entries[idx - 1].index.saturating_add(1)
        };

        if entry.index != expected_index {
            return reject(state, expected_index, appended_entries, state.commit_index);
        }

        match state.log_term_at(entry.index) {
            Some(existing_term) if existing_term == entry.term => {
                // Duplicate entry at this index/term; nothing to do.
            }
            Some(_) => {
                state.truncate_log_from(entry.index);
                for new_entry in &req.entries[idx..] {
                    state.append_log_entry(new_entry.clone());
                    appended_entries += 1;
                }
                break;
            }
            None => {
                for new_entry in &req.entries[idx..] {
                    state.append_log_entry(new_entry.clone());
                    appended_entries += 1;
                }
                break;
            }
        }
    }

    let leader_commit = req.leader_commit.min(state.last_log_index());
    state.commit_index = state.commit_index.max(leader_commit);

    AppendApplyResult {
        response: AppendEntriesResponse {
            term: state.current_term,
            success: true,
            match_index: state.last_log_index(),
            conflict_index: None,
        },
        appended_entries,
        commit_index: state.commit_index,
    }
}

fn reject(
    state: &NodeState,
    conflict_index: u64,
    appended_entries: usize,
    commit_index: u64,
) -> AppendApplyResult {
    AppendApplyResult {
        response: AppendEntriesResponse {
            term: state.current_term,
            success: false,
            match_index: state.last_log_index(),
            conflict_index: Some(conflict_index),
        },
        appended_entries,
        commit_index,
    }
}

fn first_index_for_term(state: &NodeState, term: u64, at_index: u64) -> u64 {
    let mut idx = at_index;
    while idx > 1 {
        let previous = idx - 1;
        if state.log_term_at(previous) != Some(term) {
            break;
        }
        idx = previous;
    }
    idx
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::raft::message::{AppendEntries, LogEntry};

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
            entries: vec![],
        };
        let result = handle_append_entries(&mut state, &req, 15, 12);
        assert!(result.response.success);
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
            entries: vec![],
        };
        let result = handle_append_entries(&mut state, &req, 20, 10);
        assert!(!result.response.success);
        assert_eq!(state.current_term, 5);
        assert_eq!(result.response.conflict_index, Some(1));
    }

    #[test]
    fn rejects_when_prev_log_index_is_past_local_tail() {
        let mut state = NodeState::with_timing(1, 0, 10);
        state.append_log_entry(LogEntry {
            index: 1,
            term: 1,
            payload: b"a".to_vec(),
        });

        let req = AppendEntries {
            term: 1,
            leader_id: 9,
            prev_log_index: 2,
            prev_log_term: 1,
            leader_commit: 0,
            entries: vec![],
        };
        let result = handle_append_entries(&mut state, &req, 5, 10);
        assert!(!result.response.success);
        assert_eq!(result.response.conflict_index, Some(2));
    }

    #[test]
    fn rejects_when_prev_log_term_mismatches_and_reports_conflict_index() {
        let mut state = NodeState::with_timing(1, 0, 10);
        state.append_log_entry(LogEntry {
            index: 1,
            term: 1,
            payload: b"a".to_vec(),
        });
        state.append_log_entry(LogEntry {
            index: 2,
            term: 2,
            payload: b"b".to_vec(),
        });
        state.append_log_entry(LogEntry {
            index: 3,
            term: 2,
            payload: b"c".to_vec(),
        });

        let req = AppendEntries {
            term: 2,
            leader_id: 9,
            prev_log_index: 3,
            prev_log_term: 7,
            leader_commit: 0,
            entries: vec![],
        };

        let result = handle_append_entries(&mut state, &req, 5, 10);
        assert!(!result.response.success);
        assert_eq!(result.response.conflict_index, Some(2));
    }

    #[test]
    fn conflicting_suffix_is_truncated_and_replaced() {
        let mut state = NodeState::with_timing(1, 0, 10);
        state.append_log_entry(LogEntry {
            index: 1,
            term: 1,
            payload: b"a".to_vec(),
        });
        state.append_log_entry(LogEntry {
            index: 2,
            term: 1,
            payload: b"b".to_vec(),
        });
        state.append_log_entry(LogEntry {
            index: 3,
            term: 2,
            payload: b"old".to_vec(),
        });

        let req = AppendEntries {
            term: 3,
            leader_id: 9,
            prev_log_index: 2,
            prev_log_term: 1,
            leader_commit: 3,
            entries: vec![
                LogEntry {
                    index: 3,
                    term: 3,
                    payload: b"new".to_vec(),
                },
                LogEntry {
                    index: 4,
                    term: 3,
                    payload: b"next".to_vec(),
                },
            ],
        };

        let result = handle_append_entries(&mut state, &req, 5, 10);
        assert!(result.response.success);
        assert_eq!(result.appended_entries, 2);
        assert_eq!(state.last_log_index(), 4);
        assert_eq!(state.log_term_at(3), Some(3));
        assert_eq!(state.log_term_at(4), Some(3));
        assert_eq!(result.commit_index, 3);
    }

    #[test]
    fn commit_index_does_not_advance_past_last_log_index() {
        let mut state = NodeState::with_timing(1, 0, 10);
        state.append_log_entry(LogEntry {
            index: 1,
            term: 1,
            payload: b"a".to_vec(),
        });

        let req = AppendEntries {
            term: 1,
            leader_id: 9,
            prev_log_index: 1,
            prev_log_term: 1,
            leader_commit: 99,
            entries: vec![],
        };
        let result = handle_append_entries(&mut state, &req, 5, 10);
        assert!(result.response.success);
        assert_eq!(result.commit_index, 1);
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
