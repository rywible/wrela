use proptest::prelude::*;
use std::collections::HashMap;
use wrela_runtime::db::raft::append::handle_append_entries;
use wrela_runtime::db::raft::election::{handle_vote_request, start_election};
use wrela_runtime::db::raft::message::{AppendEntries, LogEntry};
use wrela_runtime::db::raft::state::{NodeState, Role};

/// Raft event that can be applied to a cluster of nodes.
#[derive(Debug, Clone)]
enum RaftEvent {
    /// Node triggers an election timeout and starts an election.
    ElectionTimeout { node: usize },
    /// Node sends vote request to all peers (vote responses are applied inline).
    BroadcastVotes { candidate: usize },
    /// Leader appends entries and sends to a follower.
    AppendEntries {
        leader: usize,
        follower: usize,
        entry_count: u8,
    },
}

fn raft_event_strategy(num_nodes: usize) -> impl Strategy<Value = RaftEvent> {
    prop_oneof![
        3 => (0..num_nodes).prop_map(|n| RaftEvent::ElectionTimeout { node: n }),
        3 => (0..num_nodes).prop_map(|n| RaftEvent::BroadcastVotes { candidate: n }),
        4 => (0..num_nodes, 0..num_nodes, 1u8..4u8).prop_map(|(l, f, c)| {
            RaftEvent::AppendEntries {
                leader: l,
                follower: f,
                entry_count: c,
            }
        }),
    ]
}

fn apply_event(nodes: &mut [NodeState], tick: &mut u64, event: &RaftEvent) {
    *tick += 1;
    match event {
        RaftEvent::ElectionTimeout { node } => {
            let n = *node;
            if n < nodes.len() {
                let last_log_index = nodes[n].last_log_index();
                let last_log_term = nodes[n].last_log_term();
                start_election(&mut nodes[n], *tick, 10, last_log_index, last_log_term);
            }
        }
        RaftEvent::BroadcastVotes { candidate } => {
            let c = *candidate;
            if c >= nodes.len() || nodes[c].role != Role::Candidate {
                return;
            }
            let req = wrela_runtime::db::raft::message::VoteRequest {
                term: nodes[c].current_term,
                candidate_id: nodes[c].node_id,
                last_log_index: nodes[c].last_log_index(),
                last_log_term: nodes[c].last_log_term(),
            };
            let mut votes = 1u64; // self vote
            for i in 0..nodes.len() {
                if i == c {
                    continue;
                }
                let local_lli = nodes[i].last_log_index();
                let local_llt = nodes[i].last_log_term();
                let rsp = handle_vote_request(&mut nodes[i], &req, local_lli, local_llt, *tick, 10);
                if rsp.vote_granted {
                    votes += 1;
                }
                // Step down candidate if it sees a higher term.
                if rsp.term > nodes[c].current_term {
                    nodes[c].current_term = rsp.term;
                    nodes[c].role = Role::Follower;
                    nodes[c].voted_for = None;
                    return;
                }
            }
            let majority = (nodes.len() as u64) / 2 + 1;
            if votes >= majority && nodes[c].role == Role::Candidate {
                nodes[c].role = Role::Leader;
            }
        }
        RaftEvent::AppendEntries {
            leader,
            follower,
            entry_count,
        } => {
            let l = *leader;
            let f = *follower;
            if l >= nodes.len() || f >= nodes.len() || l == f {
                return;
            }
            if nodes[l].role != Role::Leader {
                return;
            }
            // Leader appends entries to its own log first.
            for _ in 0..*entry_count {
                let next_index = nodes[l].last_log_index() + 1;
                let term = nodes[l].current_term;
                let _ = nodes[l].append_log_entry_checked(LogEntry {
                    index: next_index,
                    term,
                    payload: format!("e-{next_index}").into_bytes(),
                });
            }
            // Build append request.
            let prev_log_index = nodes[f].last_log_index();
            let prev_log_term = nodes[f].last_log_term();
            let entries: Vec<LogEntry> = nodes[l]
                .log
                .iter()
                .filter(|e| e.index > prev_log_index)
                .cloned()
                .collect();
            let req = AppendEntries {
                term: nodes[l].current_term,
                leader_id: nodes[l].node_id,
                prev_log_index,
                prev_log_term,
                leader_commit: nodes[l].commit_index,
                entries,
            };
            handle_append_entries(&mut nodes[f], &req, *tick, 10);
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    /// Property: at most one leader per term.
    #[test]
    fn at_most_one_leader_per_term(
        events in prop::collection::vec(raft_event_strategy(5), 10..60)
    ) {
        let mut nodes: Vec<NodeState> = (0..5u64)
            .map(|id| NodeState::with_timing(id, 0, 10))
            .collect();
        let mut tick = 0u64;

        for event in &events {
            apply_event(&mut nodes, &mut tick, event);
        }

        // Check: at most one leader per term.
        let mut leaders_by_term: HashMap<u64, Vec<u64>> = HashMap::new();
        for node in &nodes {
            if node.role == Role::Leader {
                leaders_by_term
                    .entry(node.current_term)
                    .or_default()
                    .push(node.node_id);
            }
        }
        for (term, leaders) in &leaders_by_term {
            prop_assert!(
                leaders.len() <= 1,
                "term {term} has {} leaders: {:?}",
                leaders.len(),
                leaders
            );
        }
    }

    /// Property: committed entries are never overwritten (log matching).
    /// If two nodes have committed an entry at the same index, they must
    /// agree on the term.
    #[test]
    fn committed_entries_never_overwritten(
        events in prop::collection::vec(raft_event_strategy(3), 10..40)
    ) {
        let mut nodes: Vec<NodeState> = (0..3u64)
            .map(|id| NodeState::with_timing(id, 0, 10))
            .collect();
        let mut tick = 0u64;

        for event in &events {
            apply_event(&mut nodes, &mut tick, event);
        }

        // For each pair of nodes, check that committed log entries agree.
        for i in 0..nodes.len() {
            for j in (i + 1)..nodes.len() {
                let commit_i = nodes[i].commit_index;
                let commit_j = nodes[j].commit_index;
                let min_commit = commit_i.min(commit_j);
                for idx in 1..=min_commit {
                    let term_i = nodes[i].log_term_at(idx);
                    let term_j = nodes[j].log_term_at(idx);
                    if let (Some(ti), Some(tj)) = (term_i, term_j) {
                        prop_assert_eq!(
                            ti, tj,
                            "nodes {} and {} disagree on committed entry at index {}: term {} vs {}",
                            nodes[i].node_id, nodes[j].node_id, idx, ti, tj
                        );
                    }
                }
            }
        }
    }

    /// Property: log contiguity — each node's log entries have strictly
    /// contiguous indices starting from 1.
    #[test]
    fn log_entries_are_contiguous(
        events in prop::collection::vec(raft_event_strategy(3), 10..40)
    ) {
        let mut nodes: Vec<NodeState> = (0..3u64)
            .map(|id| NodeState::with_timing(id, 0, 10))
            .collect();
        let mut tick = 0u64;

        for event in &events {
            apply_event(&mut nodes, &mut tick, event);
        }

        for node in &nodes {
            for (pos, entry) in node.log.iter().enumerate() {
                let expected_index = (pos as u64) + 1;
                prop_assert_eq!(
                    entry.index,
                    expected_index,
                    "node {} log not contiguous at position {}: expected index {} got {}",
                    node.node_id,
                    pos,
                    expected_index,
                    entry.index
                );
            }
            // commit_index never exceeds last_log_index.
            prop_assert!(
                node.commit_index <= node.last_log_index(),
                "node {} commit_index {} exceeds last_log_index {}",
                node.node_id,
                node.commit_index,
                node.last_log_index()
            );
        }
    }
}
