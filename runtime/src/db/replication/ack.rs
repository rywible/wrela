use crate::db::replication::quorum::{
    DurabilityAckDecision, FollowerAppendResponse, durable_quorum_ack_decision,
    follower_acks_from_append_responses,
};

#[derive(Debug, Clone)]
pub struct LeaderAckInput {
    pub voters: usize,
    pub leader_durable: bool,
    pub required_term: u64,
    pub required_index: u64,
    pub follower_responses: Vec<FollowerAppendResponse>,
}

pub fn evaluate_leader_ack(input: &LeaderAckInput) -> DurabilityAckDecision {
    let follower_acks = follower_acks_from_append_responses(
        input.required_term,
        input.required_index,
        &input.follower_responses,
    );
    durable_quorum_ack_decision(input.voters, input.leader_durable, &follower_acks)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::raft::message::AppendEntriesResponse;

    #[test]
    fn leader_ack_requires_quorum_and_required_position() {
        let input = LeaderAckInput {
            voters: 3,
            leader_durable: true,
            required_term: 9,
            required_index: 100,
            follower_responses: vec![
                FollowerAppendResponse {
                    node_id: 2,
                    response: AppendEntriesResponse {
                        term: 9,
                        success: true,
                        match_index: 100,
                        conflict_index: None,
                    },
                    replication_latency_ns: 15,
                    fsync_latency_ns: 9,
                },
                FollowerAppendResponse {
                    node_id: 3,
                    response: AppendEntriesResponse {
                        term: 9,
                        success: true,
                        match_index: 99,
                        conflict_index: None,
                    },
                    replication_latency_ns: 18,
                    fsync_latency_ns: 11,
                },
            ],
        };
        let decision = evaluate_leader_ack(&input);
        assert!(decision.ack_emitted);
        assert_eq!(decision.durable_acks, 2);
    }

    #[test]
    fn leader_ack_blocks_when_followers_only_send_conflicts() {
        let input = LeaderAckInput {
            voters: 3,
            leader_durable: false,
            required_term: 4,
            required_index: 50,
            follower_responses: vec![
                FollowerAppendResponse {
                    node_id: 2,
                    response: AppendEntriesResponse {
                        term: 4,
                        success: false,
                        match_index: 49,
                        conflict_index: Some(20),
                    },
                    replication_latency_ns: 20,
                    fsync_latency_ns: 12,
                },
                FollowerAppendResponse {
                    node_id: 3,
                    response: AppendEntriesResponse {
                        term: 4,
                        success: false,
                        match_index: 48,
                        conflict_index: Some(19),
                    },
                    replication_latency_ns: 23,
                    fsync_latency_ns: 13,
                },
            ],
        };
        let decision = evaluate_leader_ack(&input);
        assert!(!decision.ack_emitted);
        assert_eq!(decision.durable_acks, 0);
    }
}
