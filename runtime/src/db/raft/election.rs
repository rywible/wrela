use crate::db::raft::message::{VoteRequest, VoteResponse};
use crate::db::raft::state::{NodeState, Role};

pub fn start_election(
    state: &mut NodeState,
    now_tick: u64,
    election_timeout_ticks: u64,
    last_log_index: u64,
    last_log_term: u64,
) -> VoteRequest {
    state.current_term = state.current_term.saturating_add(1);
    state.voted_for = Some(state.node_id);
    state.role = Role::Candidate;
    state.reset_election_deadline(now_tick, election_timeout_ticks);
    VoteRequest {
        term: state.current_term,
        candidate_id: state.node_id,
        last_log_index,
        last_log_term,
    }
}

pub fn handle_vote_request(
    state: &mut NodeState,
    req: &VoteRequest,
    local_last_log_index: u64,
    local_last_log_term: u64,
    now_tick: u64,
    election_timeout_ticks: u64,
) -> VoteResponse {
    if req.term < state.current_term {
        return VoteResponse {
            term: state.current_term,
            vote_granted: false,
        };
    }
    if req.term > state.current_term {
        state.current_term = req.term;
        state.voted_for = None;
        state.role = Role::Follower;
    }

    let candidate_log_up_to_date = req.last_log_term > local_last_log_term
        || (req.last_log_term == local_last_log_term && req.last_log_index >= local_last_log_index);

    let grant = match state.voted_for {
        Some(existing) => existing == req.candidate_id && candidate_log_up_to_date,
        None => candidate_log_up_to_date,
    };
    if grant {
        state.voted_for = Some(req.candidate_id);
        state.reset_election_deadline(now_tick, election_timeout_ticks);
    }
    VoteResponse {
        term: state.current_term,
        vote_granted: grant,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_election_increments_term_and_votes_self() {
        let mut state = NodeState::with_timing(7, 100, 20);
        let req = start_election(&mut state, 100, 20, 55, 3);
        assert_eq!(state.current_term, 1);
        assert_eq!(state.role, Role::Candidate);
        assert_eq!(state.voted_for, Some(7));
        assert_eq!(req.term, 1);
        assert_eq!(req.candidate_id, 7);
        assert_eq!(req.last_log_index, 55);
        assert_eq!(req.last_log_term, 3);
    }

    #[test]
    fn vote_request_rejects_stale_candidate_log_even_with_higher_term() {
        let mut state = NodeState::with_timing(1, 0, 10);
        state.current_term = 2;
        let req = VoteRequest {
            term: 3,
            candidate_id: 9,
            last_log_index: 1,
            last_log_term: 1,
        };
        let rsp = handle_vote_request(&mut state, &req, 10, 4, 5, 10);
        assert!(!rsp.vote_granted);
        assert_eq!(rsp.term, 3);
        assert_eq!(state.current_term, 3);
        assert_eq!(state.role, Role::Follower);
    }

    #[test]
    fn vote_request_grants_up_to_date_candidate_and_sets_vote() {
        let mut state = NodeState::with_timing(1, 0, 10);
        let req = VoteRequest {
            term: 4,
            candidate_id: 9,
            last_log_index: 20,
            last_log_term: 2,
        };
        let rsp = handle_vote_request(&mut state, &req, 19, 2, 7, 10);
        assert!(rsp.vote_granted);
        assert_eq!(state.current_term, 4);
        assert_eq!(state.voted_for, Some(9));
        assert!(state.election_deadline_tick > 7);
    }
}
