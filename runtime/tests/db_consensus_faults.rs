use wrela_runtime::db::raft::election::{handle_vote_request, start_election};
use wrela_runtime::db::raft::message::AppendEntriesResponse;
use wrela_runtime::db::raft::persistence::PersistedElectionState;
use wrela_runtime::db::raft::state::NodeState;
use wrela_runtime::db::replication::ack::{LeaderAckInput, evaluate_leader_ack};
use wrela_runtime::db::replication::quorum::FollowerAppendResponse;

#[test]
fn partition_without_quorum_blocks_ack() {
    let input = LeaderAckInput {
        voters: 5,
        leader_durable: true,
        required_term: 7,
        required_index: 120,
        follower_responses: vec![
            FollowerAppendResponse {
                node_id: 2,
                response: AppendEntriesResponse {
                    term: 7,
                    success: true,
                    match_index: 120,
                    conflict_index: None,
                },
                replication_latency_ns: 15,
                fsync_latency_ns: 10,
            },
            FollowerAppendResponse {
                node_id: 3,
                response: AppendEntriesResponse {
                    term: 7,
                    success: false,
                    match_index: 110,
                    conflict_index: Some(90),
                },
                replication_latency_ns: 21,
                fsync_latency_ns: 12,
            },
        ],
    };

    let decision = evaluate_leader_ack(&input);
    assert!(!decision.ack_emitted);
    assert_eq!(decision.quorum_size, 3);
    assert_eq!(decision.durable_acks, 2);
}

#[test]
fn healed_partition_restores_ack_path() {
    let input = LeaderAckInput {
        voters: 5,
        leader_durable: true,
        required_term: 7,
        required_index: 120,
        follower_responses: vec![
            FollowerAppendResponse {
                node_id: 2,
                response: AppendEntriesResponse {
                    term: 7,
                    success: true,
                    match_index: 120,
                    conflict_index: None,
                },
                replication_latency_ns: 15,
                fsync_latency_ns: 10,
            },
            FollowerAppendResponse {
                node_id: 3,
                response: AppendEntriesResponse {
                    term: 7,
                    success: true,
                    match_index: 120,
                    conflict_index: None,
                },
                replication_latency_ns: 20,
                fsync_latency_ns: 12,
            },
        ],
    };

    let decision = evaluate_leader_ack(&input);
    assert!(decision.ack_emitted);
    assert_eq!(decision.quorum_size, 3);
    assert_eq!(decision.durable_acks, 3);
}

#[test]
fn stale_term_follower_response_is_excluded_from_quorum() {
    let input = LeaderAckInput {
        voters: 3,
        leader_durable: true,
        required_term: 11,
        required_index: 500,
        follower_responses: vec![FollowerAppendResponse {
            node_id: 2,
            response: AppendEntriesResponse {
                term: 10,
                success: true,
                match_index: 999,
                conflict_index: None,
            },
            replication_latency_ns: 18,
            fsync_latency_ns: 9,
        }],
    };
    let decision = evaluate_leader_ack(&input);
    assert!(!decision.ack_emitted);
    assert_eq!(decision.durable_acks, 1);
}

#[test]
fn duplicate_follower_acks_do_not_falsely_form_quorum() {
    let input = LeaderAckInput {
        voters: 5,
        leader_durable: true,
        required_term: 22,
        required_index: 900,
        follower_responses: vec![
            FollowerAppendResponse {
                node_id: 2,
                response: AppendEntriesResponse {
                    term: 22,
                    success: true,
                    match_index: 900,
                    conflict_index: None,
                },
                replication_latency_ns: 12,
                fsync_latency_ns: 8,
            },
            FollowerAppendResponse {
                node_id: 2,
                response: AppendEntriesResponse {
                    term: 22,
                    success: true,
                    match_index: 900,
                    conflict_index: None,
                },
                replication_latency_ns: 13,
                fsync_latency_ns: 9,
            },
        ],
    };

    let decision = evaluate_leader_ack(&input);
    assert!(!decision.ack_emitted);
    assert_eq!(decision.quorum_size, 3);
    assert_eq!(decision.durable_acks, 2);
}

#[test]
fn split_vote_then_next_term_up_to_date_candidate_wins() {
    let mut candidate_a = NodeState::with_timing(1, 0, 10);
    candidate_a.current_term = 4;
    let mut candidate_b = NodeState::with_timing(2, 0, 10);
    candidate_b.current_term = 4;
    let mut voter = NodeState::with_timing(3, 0, 10);
    voter.current_term = 4;

    // First term: voter grants candidate A and rejects candidate B => split tendency.
    let req_a = start_election(&mut candidate_a, 10, 10, 40, 4);
    let rsp_a = handle_vote_request(&mut voter, &req_a, 40, 4, 10, 10);
    assert!(rsp_a.vote_granted);
    let req_b_same_term = start_election(&mut candidate_b, 10, 10, 40, 4);
    assert_eq!(req_b_same_term.term, req_a.term);
    let rsp_b_same_term = handle_vote_request(&mut voter, &req_b_same_term, 40, 4, 10, 10);
    assert!(!rsp_b_same_term.vote_granted);

    // Next term: B has more up-to-date log and now wins vote.
    let req_b_next_term = start_election(&mut candidate_b, 20, 10, 55, 5);
    let rsp_b_next_term = handle_vote_request(&mut voter, &req_b_next_term, 40, 4, 20, 10);
    assert!(rsp_b_next_term.vote_granted);
    assert_eq!(rsp_b_next_term.term, req_b_next_term.term);
}

#[test]
fn restart_preserves_vote_and_rejects_different_candidate_same_term() {
    let mut voter = NodeState::with_timing(3, 0, 10);
    voter.current_term = 4;

    let mut candidate_a = NodeState::with_timing(1, 5, 10);
    candidate_a.current_term = 4;
    let req_a = start_election(&mut candidate_a, 5, 10, 20, 4);
    let rsp_a = handle_vote_request(&mut voter, &req_a, 20, 4, 5, 10);
    assert!(rsp_a.vote_granted);
    assert_eq!(voter.voted_for, Some(req_a.candidate_id));

    let persisted = PersistedElectionState::capture(&voter);
    let mut restarted_voter = NodeState::with_timing(3, 30, 10);
    persisted.restore_into(&mut restarted_voter, 30, 10);

    let mut candidate_b = NodeState::with_timing(2, 31, 10);
    candidate_b.current_term = 4;
    let req_b_same_term = start_election(&mut candidate_b, 31, 10, 20, 4);
    assert_eq!(req_b_same_term.term, req_a.term);
    let rsp_b = handle_vote_request(&mut restarted_voter, &req_b_same_term, 20, 4, 31, 10);
    assert!(!rsp_b.vote_granted);
    assert_eq!(rsp_b.term, req_a.term);
}

#[test]
fn candidate_restart_forces_follower_and_requires_higher_term_to_progress() {
    let mut candidate = NodeState::with_timing(4, 100, 10);
    candidate.current_term = 9;
    let self_vote_req = start_election(&mut candidate, 100, 10, 50, 9);
    assert_eq!(candidate.voted_for, Some(4));
    assert_eq!(candidate.current_term, self_vote_req.term);

    let persisted = PersistedElectionState::capture(&candidate);
    let mut restarted = NodeState::with_timing(4, 200, 10);
    persisted.restore_into(&mut restarted, 200, 10);
    assert_eq!(
        restarted.role,
        wrela_runtime::db::raft::state::Role::Follower
    );

    let mut challenger = NodeState::with_timing(5, 205, 10);
    challenger.current_term = restarted.current_term.saturating_sub(1);
    let same_term_request = start_election(&mut challenger, 205, 10, 50, 9);
    assert_eq!(same_term_request.term, restarted.current_term);
    let same_term_rsp = handle_vote_request(&mut restarted, &same_term_request, 50, 9, 205, 10);
    assert!(!same_term_rsp.vote_granted);

    let next_term_req = start_election(&mut restarted, 210, 10, 50, 9);
    assert_eq!(next_term_req.term, same_term_rsp.term + 1);
    assert_eq!(restarted.voted_for, Some(restarted.node_id));
}
