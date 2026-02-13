use crate::db::raft::state::{NodeState, Role};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PersistedElectionState {
    pub current_term: u64,
    pub voted_for: Option<u64>,
}

impl PersistedElectionState {
    pub fn capture(state: &NodeState) -> Self {
        Self {
            current_term: state.current_term,
            voted_for: state.voted_for,
        }
    }

    pub fn restore_into(&self, state: &mut NodeState, now_tick: u64, election_timeout_ticks: u64) {
        state.current_term = self.current_term;
        state.voted_for = self.voted_for;
        state.role = Role::Follower;
        state.last_heartbeat_tick = now_tick;
        state.reset_election_deadline(now_tick, election_timeout_ticks);
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
        let persisted = PersistedElectionState::capture(&state);

        let mut restarted = NodeState::with_timing(9, 100, 3);
        persisted.restore_into(&mut restarted, 42, 10);

        assert_eq!(restarted.current_term, 7);
        assert_eq!(restarted.voted_for, Some(9));
        assert_eq!(restarted.role, Role::Follower);
        assert_eq!(restarted.last_heartbeat_tick, 42);
        assert_eq!(restarted.election_deadline_tick, 52);
    }
}
