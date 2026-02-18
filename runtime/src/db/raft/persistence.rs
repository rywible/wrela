use crate::db::raft::message::LogEntry;
use crate::db::raft::state::{NodeState, Role};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistedElectionState {
    pub current_term: u64,
    pub voted_for: Option<u64>,
    pub commit_index: u64,
    pub log: Vec<LogEntry>,
}

impl PersistedElectionState {
    pub fn capture(state: &NodeState) -> Self {
        Self {
            current_term: state.current_term,
            voted_for: state.voted_for,
            commit_index: state.commit_index,
            log: state.log.clone(),
        }
    }

    pub fn restore_into(&self, state: &mut NodeState, now_tick: u64, election_timeout_ticks: u64) {
        state.current_term = self.current_term;
        state.voted_for = self.voted_for;
        state.role = Role::Follower;
        state.last_heartbeat_tick = now_tick;
        state.reset_election_deadline(now_tick, election_timeout_ticks);
        state.commit_index = self.commit_index;
        state.log = self.log.clone();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restore_rehydrates_term_vote_and_forces_follower_role() {
        let mut state = NodeState::with_timing(9, 5, 5);
        state.current_term = 7;
        state.voted_for = Some(9);
        state.role = Role::Leader;
        state.commit_index = 2;
        state.log = vec![
            LogEntry {
                index: 1,
                term: 6,
                payload: b"a".to_vec(),
            },
            LogEntry {
                index: 2,
                term: 7,
                payload: b"b".to_vec(),
            },
        ];
        let persisted = PersistedElectionState::capture(&state);

        let mut restarted = NodeState::with_timing(9, 100, 3);
        persisted.restore_into(&mut restarted, 42, 10);

        assert_eq!(restarted.current_term, 7);
        assert_eq!(restarted.voted_for, Some(9));
        assert_eq!(restarted.role, Role::Follower);
        assert_eq!(restarted.last_heartbeat_tick, 42);
        assert_eq!(restarted.election_deadline_tick, 52);
        assert_eq!(restarted.commit_index, 2);
        assert_eq!(restarted.log.len(), 2);
        assert_eq!(restarted.log[1].term, 7);
    }
}
