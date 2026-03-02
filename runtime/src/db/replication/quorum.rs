use crate::db::raft::message::AppendEntriesResponse;
use std::cmp::Ordering;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FollowerDurabilityAck {
    pub node_id: u64,
    pub durable: bool,
    pub replication_latency_ns: u64,
    pub fsync_latency_ns: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DurabilityAckDecision {
    pub ack_emitted: bool,
    pub quorum_size: usize,
    pub durable_acks: usize,
    pub quorum_replication_latency_ns: u64,
    pub quorum_fsync_latency_ns: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FollowerAppendResponse {
    pub node_id: u64,
    pub response: AppendEntriesResponse,
    pub replication_latency_ns: u64,
    pub fsync_latency_ns: u64,
}

pub fn quorum_size(voters: usize) -> usize {
    (voters / 2) + 1
}

pub fn has_quorum(voters: usize, acks: usize) -> bool {
    acks >= quorum_size(voters)
}

pub fn durable_quorum_ack_decision(
    voters: usize,
    leader_durable: bool,
    follower_acks: &[FollowerDurabilityAck],
) -> DurabilityAckDecision {
    let quorum = quorum_size(voters);
    let mut durable_acks = usize::from(leader_durable);
    let mut max_replication = 0u64;
    let mut max_fsync = 0u64;

    for ack in follower_acks {
        if !ack.durable {
            continue;
        }
        durable_acks = durable_acks.saturating_add(1);
        max_replication = max_replication.max(ack.replication_latency_ns);
        max_fsync = max_fsync.max(ack.fsync_latency_ns);
    }

    DurabilityAckDecision {
        ack_emitted: has_quorum(voters, durable_acks),
        quorum_size: quorum,
        durable_acks,
        quorum_replication_latency_ns: max_replication,
        quorum_fsync_latency_ns: max_fsync,
    }
}

pub fn response_is_durable_ack(
    response: &AppendEntriesResponse,
    required_term: u64,
    required_index: u64,
) -> bool {
    response.success && response.term >= required_term && response.match_index >= required_index
}

pub fn follower_acks_from_append_responses(
    required_term: u64,
    required_index: u64,
    responses: &[FollowerAppendResponse],
) -> Vec<FollowerDurabilityAck> {
    let mut by_node: HashMap<u64, usize> = HashMap::with_capacity(responses.len());
    let mut deduped = Vec::with_capacity(responses.len());

    for item in responses {
        match by_node.get(&item.node_id).copied() {
            None => {
                by_node.insert(item.node_id, deduped.len());
                deduped.push(item.clone());
            }
            Some(existing_idx) => {
                if response_precedence(&item.response, &deduped[existing_idx].response)
                    == Ordering::Greater
                {
                    deduped[existing_idx] = item.clone();
                }
            }
        }
    }

    deduped
        .into_iter()
        .map(|item| FollowerDurabilityAck {
            node_id: item.node_id,
            durable: response_is_durable_ack(&item.response, required_term, required_index),
            replication_latency_ns: item.replication_latency_ns,
            fsync_latency_ns: item.fsync_latency_ns,
        })
        .collect()
}

fn response_precedence(left: &AppendEntriesResponse, right: &AppendEntriesResponse) -> Ordering {
    left.term
        .cmp(&right.term)
        .then_with(|| left.match_index.cmp(&right.match_index))
        .then_with(|| left.success.cmp(&right.success))
        .then_with(|| {
            left.conflict_index
                .unwrap_or(0)
                .cmp(&right.conflict_index.unwrap_or(0))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emits_ack_only_after_durable_quorum() {
        let decision = durable_quorum_ack_decision(
            3,
            true,
            &[FollowerDurabilityAck {
                node_id: 2,
                durable: true,
                replication_latency_ns: 40,
                fsync_latency_ns: 30,
            }],
        );
        assert!(decision.ack_emitted);
        assert_eq!(decision.quorum_size, 2);
        assert_eq!(decision.durable_acks, 2);
    }

    #[test]
    fn non_durable_followers_do_not_count_toward_quorum() {
        let decision = durable_quorum_ack_decision(
            5,
            true,
            &[
                FollowerDurabilityAck {
                    node_id: 2,
                    durable: false,
                    replication_latency_ns: 10,
                    fsync_latency_ns: 10,
                },
                FollowerDurabilityAck {
                    node_id: 3,
                    durable: true,
                    replication_latency_ns: 20,
                    fsync_latency_ns: 30,
                },
            ],
        );
        assert!(!decision.ack_emitted);
        assert_eq!(decision.quorum_size, 3);
        assert_eq!(decision.durable_acks, 2);
    }

    #[test]
    fn reports_replication_and_fsync_latency_from_durable_acks() {
        let decision = durable_quorum_ack_decision(
            3,
            true,
            &[
                FollowerDurabilityAck {
                    node_id: 2,
                    durable: true,
                    replication_latency_ns: 12,
                    fsync_latency_ns: 5,
                },
                FollowerDurabilityAck {
                    node_id: 3,
                    durable: true,
                    replication_latency_ns: 18,
                    fsync_latency_ns: 9,
                },
            ],
        );
        assert_eq!(decision.quorum_replication_latency_ns, 18);
        assert_eq!(decision.quorum_fsync_latency_ns, 9);
    }

    #[test]
    fn response_mapping_respects_required_term_and_index() {
        let responses = [
            FollowerAppendResponse {
                node_id: 2,
                response: AppendEntriesResponse {
                    term: 7,
                    success: true,
                    match_index: 120,
                    conflict_index: None,
                },
                replication_latency_ns: 20,
                fsync_latency_ns: 10,
            },
            FollowerAppendResponse {
                node_id: 3,
                response: AppendEntriesResponse {
                    term: 6,
                    success: true,
                    match_index: 120,
                    conflict_index: None,
                },
                replication_latency_ns: 25,
                fsync_latency_ns: 12,
            },
            FollowerAppendResponse {
                node_id: 4,
                response: AppendEntriesResponse {
                    term: 7,
                    success: false,
                    match_index: 80,
                    conflict_index: Some(70),
                },
                replication_latency_ns: 30,
                fsync_latency_ns: 14,
            },
        ];

        let acks = follower_acks_from_append_responses(7, 100, &responses);
        assert_eq!(acks.len(), 3);
        assert!(acks[0].durable);
        assert!(!acks[1].durable);
        assert!(!acks[2].durable);
    }

    #[test]
    fn quorum_decision_can_be_driven_from_append_responses() {
        let responses = [
            FollowerAppendResponse {
                node_id: 2,
                response: AppendEntriesResponse {
                    term: 9,
                    success: true,
                    match_index: 200,
                    conflict_index: None,
                },
                replication_latency_ns: 31,
                fsync_latency_ns: 19,
            },
            FollowerAppendResponse {
                node_id: 3,
                response: AppendEntriesResponse {
                    term: 9,
                    success: true,
                    match_index: 199,
                    conflict_index: None,
                },
                replication_latency_ns: 33,
                fsync_latency_ns: 21,
            },
        ];

        let follower_acks = follower_acks_from_append_responses(9, 199, &responses);
        let decision = durable_quorum_ack_decision(3, true, &follower_acks);
        assert!(decision.ack_emitted);
        assert_eq!(decision.durable_acks, 3);
    }

    #[test]
    fn duplicate_follower_responses_count_once_for_quorum() {
        let responses = [
            FollowerAppendResponse {
                node_id: 2,
                response: AppendEntriesResponse {
                    term: 12,
                    success: true,
                    match_index: 500,
                    conflict_index: None,
                },
                replication_latency_ns: 10,
                fsync_latency_ns: 8,
            },
            FollowerAppendResponse {
                node_id: 2,
                response: AppendEntriesResponse {
                    term: 12,
                    success: true,
                    match_index: 500,
                    conflict_index: None,
                },
                replication_latency_ns: 11,
                fsync_latency_ns: 9,
            },
        ];
        let follower_acks = follower_acks_from_append_responses(12, 500, &responses);
        let decision = durable_quorum_ack_decision(5, true, &follower_acks);
        assert!(!decision.ack_emitted);
        assert_eq!(decision.durable_acks, 2);
    }

    #[test]
    fn newer_term_failure_replaces_older_success_for_same_follower() {
        let responses = [
            FollowerAppendResponse {
                node_id: 9,
                response: AppendEntriesResponse {
                    term: 7,
                    success: true,
                    match_index: 90,
                    conflict_index: None,
                },
                replication_latency_ns: 10,
                fsync_latency_ns: 5,
            },
            FollowerAppendResponse {
                node_id: 9,
                response: AppendEntriesResponse {
                    term: 8,
                    success: false,
                    match_index: 89,
                    conflict_index: Some(60),
                },
                replication_latency_ns: 14,
                fsync_latency_ns: 6,
            },
        ];
        let follower_acks = follower_acks_from_append_responses(7, 90, &responses);
        assert_eq!(follower_acks.len(), 1);
        assert!(!follower_acks[0].durable);
        assert_eq!(follower_acks[0].node_id, 9);
    }

    #[test]
    fn dedupe_prefers_higher_conflict_index_for_equal_term_and_match() {
        let responses = [
            FollowerAppendResponse {
                node_id: 4,
                response: AppendEntriesResponse {
                    term: 10,
                    success: false,
                    match_index: 90,
                    conflict_index: Some(30),
                },
                replication_latency_ns: 10,
                fsync_latency_ns: 5,
            },
            FollowerAppendResponse {
                node_id: 4,
                response: AppendEntriesResponse {
                    term: 10,
                    success: false,
                    match_index: 90,
                    conflict_index: Some(40),
                },
                replication_latency_ns: 11,
                fsync_latency_ns: 6,
            },
        ];
        let follower_acks = follower_acks_from_append_responses(10, 91, &responses);
        assert_eq!(follower_acks.len(), 1);
        assert_eq!(follower_acks[0].node_id, 4);
        assert!(!follower_acks[0].durable);
        assert_eq!(follower_acks[0].replication_latency_ns, 11);
        assert_eq!(follower_acks[0].fsync_latency_ns, 6);
    }
}
