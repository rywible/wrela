use crate::db::routing::health::HealthState;
use crate::db::routing::health_snapshot::NodeHealthSnapshot;
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DynamicQuorumPolicy {
    pub desired_voters: usize,
    pub min_distinct_regions: usize,
    pub max_degraded_selected: usize,
    pub required_additional_failures: usize,
    pub hysteresis_min_rounds: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionHistory {
    pub round: u64,
    pub selected_nodes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DynamicQuorumDecision {
    pub quorum_size: usize,
    pub selected_nodes: Vec<String>,
    pub max_selected_latency_ms: u64,
    pub distinct_regions: usize,
    pub degraded_selected: usize,
    pub survivable_additional_failures: usize,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DynamicQuorumError {
    EmptyCandidates,
    NoSafeSelection,
}

pub fn select_dynamic_quorum(
    snapshots: &[NodeHealthSnapshot],
    policy: &DynamicQuorumPolicy,
    current_round: u64,
    previous: Option<&SelectionHistory>,
) -> Result<DynamicQuorumDecision, DynamicQuorumError> {
    if snapshots.is_empty() || policy.desired_voters == 0 {
        return Err(DynamicQuorumError::EmptyCandidates);
    }

    let quorum_size = (policy.desired_voters / 2) + 1;
    let available: Vec<&NodeHealthSnapshot> = snapshots
        .iter()
        .filter(|row| row.state != HealthState::Unavailable)
        .collect();
    if available.len() < quorum_size {
        return Err(DynamicQuorumError::NoSafeSelection);
    }

    if let Some(prev) = previous
        && current_round.saturating_sub(prev.round) < policy.hysteresis_min_rounds
        && let Some(decision) = evaluate_previous_selection(
            &available,
            quorum_size,
            policy,
            &prev.selected_nodes,
            "hysteresis hold: previous quorum still safe",
        )
    {
        return Ok(decision);
    }

    let mut best: Option<(Vec<&NodeHealthSnapshot>, u64, usize, usize, Vec<String>)> = None;
    for combo in choose_k(&available, quorum_size) {
        let degraded_selected = combo
            .iter()
            .filter(|row| row.state == HealthState::Degraded)
            .count();
        if degraded_selected > policy.max_degraded_selected {
            continue;
        }

        let regions: BTreeSet<&str> = combo.iter().map(|row| row.region.as_str()).collect();
        if regions.len() < policy.min_distinct_regions {
            continue;
        }

        let survivable_additional_failures = available.len().saturating_sub(quorum_size);
        if survivable_additional_failures < policy.required_additional_failures {
            continue;
        }

        let max_latency = combo.iter().map(|row| row.latency_ms).max().unwrap_or(0);
        let sum_latency: u64 = combo.iter().map(|row| row.latency_ms).sum();

        let mut selected_nodes: Vec<String> = combo.iter().map(|row| row.node_id.clone()).collect();
        selected_nodes.sort();
        let reasons = vec![
            format!("max_latency_ms={max_latency}"),
            format!("sum_latency_ms={sum_latency}"),
            format!("distinct_regions={}", regions.len()),
            format!("degraded_selected={degraded_selected}"),
        ];

        let rank = (
            degraded_selected as u64,
            max_latency,
            sum_latency,
            stable_hash(&selected_nodes),
        );
        let score = rank.0 << 48 | rank.1.min(u16::MAX as u64) << 32 | rank.2.min(u32::MAX as u64);

        match &best {
            None => {
                best = Some((
                    combo,
                    score,
                    regions.len(),
                    survivable_additional_failures,
                    reasons,
                ))
            }
            Some((existing_combo, existing_score, _, _, _)) => {
                if score < *existing_score
                    || (score == *existing_score
                        && stable_hash(
                            &combo
                                .iter()
                                .map(|row| row.node_id.clone())
                                .collect::<Vec<_>>(),
                        ) < stable_hash(
                            &existing_combo
                                .iter()
                                .map(|row| row.node_id.clone())
                                .collect::<Vec<_>>(),
                        ))
                {
                    best = Some((
                        combo,
                        score,
                        regions.len(),
                        survivable_additional_failures,
                        reasons,
                    ));
                }
            }
        }
    }

    let Some((winner, _, distinct_regions, survivable_additional_failures, mut reasons)) = best
    else {
        return Err(DynamicQuorumError::NoSafeSelection);
    };

    let mut selected_nodes: Vec<String> = winner.iter().map(|row| row.node_id.clone()).collect();
    selected_nodes.sort();
    let max_selected_latency_ms = winner.iter().map(|row| row.latency_ms).max().unwrap_or(0);
    let degraded_selected = winner
        .iter()
        .filter(|row| row.state == HealthState::Degraded)
        .count();
    reasons.insert(0, format!("selected_nodes={}", selected_nodes.join(",")));

    Ok(DynamicQuorumDecision {
        quorum_size,
        selected_nodes,
        max_selected_latency_ms,
        distinct_regions,
        degraded_selected,
        survivable_additional_failures,
        reasons,
    })
}

fn evaluate_previous_selection(
    available: &[&NodeHealthSnapshot],
    quorum_size: usize,
    policy: &DynamicQuorumPolicy,
    previous_nodes: &[String],
    reason: &str,
) -> Option<DynamicQuorumDecision> {
    if previous_nodes.len() != quorum_size {
        return None;
    }
    let mut selected = Vec::new();
    for node_id in previous_nodes {
        let row = available.iter().find(|row| &row.node_id == node_id)?;
        selected.push((*row).clone());
    }
    let degraded_selected = selected
        .iter()
        .filter(|row| row.state == HealthState::Degraded)
        .count();
    if degraded_selected > policy.max_degraded_selected {
        return None;
    }
    let distinct_regions = selected
        .iter()
        .map(|row| row.region.as_str())
        .collect::<BTreeSet<_>>()
        .len();
    if distinct_regions < policy.min_distinct_regions {
        return None;
    }

    let survivable_additional_failures = available.len().saturating_sub(quorum_size);
    if survivable_additional_failures < policy.required_additional_failures {
        return None;
    }

    let max_selected_latency_ms = selected.iter().map(|row| row.latency_ms).max().unwrap_or(0);
    let mut selected_nodes: Vec<String> = selected.iter().map(|row| row.node_id.clone()).collect();
    selected_nodes.sort();

    Some(DynamicQuorumDecision {
        quorum_size,
        selected_nodes,
        max_selected_latency_ms,
        distinct_regions,
        degraded_selected,
        survivable_additional_failures,
        reasons: vec![reason.to_string()],
    })
}

fn choose_k<'a>(items: &'a [&'a NodeHealthSnapshot], k: usize) -> Vec<Vec<&'a NodeHealthSnapshot>> {
    let mut out = Vec::new();
    let mut current = Vec::new();
    choose_k_rec(items, k, 0, &mut current, &mut out);
    out
}

fn choose_k_rec<'a>(
    items: &'a [&'a NodeHealthSnapshot],
    k: usize,
    idx: usize,
    current: &mut Vec<&'a NodeHealthSnapshot>,
    out: &mut Vec<Vec<&'a NodeHealthSnapshot>>,
) {
    if current.len() == k {
        out.push(current.clone());
        return;
    }
    if idx >= items.len() {
        return;
    }

    current.push(items[idx]);
    choose_k_rec(items, k, idx + 1, current, out);
    current.pop();
    choose_k_rec(items, k, idx + 1, current, out);
}

fn stable_hash(nodes: &[String]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for node in nodes {
        for byte in node.as_bytes() {
            hash ^= *byte as u64;
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::{DynamicQuorumPolicy, SelectionHistory, select_dynamic_quorum};
    use crate::db::routing::health::HealthState;
    use crate::db::routing::health_snapshot::NodeHealthSnapshot;

    fn node(id: &str, region: &str, state: HealthState, latency_ms: u64) -> NodeHealthSnapshot {
        NodeHealthSnapshot {
            node_id: id.to_string(),
            region: region.to_string(),
            state,
            latency_ms,
            observed_at_ms: 1,
        }
    }

    #[test]
    fn selector_prefers_safety_before_latency() {
        let snapshots = vec![
            node("a", "us", HealthState::Healthy, 20),
            node("b", "us", HealthState::Degraded, 2),
            node("c", "eu", HealthState::Healthy, 25),
        ];
        let policy = DynamicQuorumPolicy {
            desired_voters: 3,
            min_distinct_regions: 2,
            max_degraded_selected: 0,
            required_additional_failures: 0,
            hysteresis_min_rounds: 0,
        };

        let decision = select_dynamic_quorum(&snapshots, &policy, 10, None).expect("decision");
        assert_eq!(
            decision.selected_nodes,
            vec!["a".to_string(), "c".to_string()]
        );
    }

    #[test]
    fn selector_resists_oscillation_inside_hysteresis_window() {
        let round_1 = vec![
            node("a", "us", HealthState::Healthy, 6),
            node("b", "eu", HealthState::Healthy, 7),
            node("c", "ap", HealthState::Healthy, 8),
        ];
        let policy = DynamicQuorumPolicy {
            desired_voters: 3,
            min_distinct_regions: 2,
            max_degraded_selected: 0,
            required_additional_failures: 0,
            hysteresis_min_rounds: 3,
        };
        let first = select_dynamic_quorum(&round_1, &policy, 10, None).expect("decision");

        let round_2 = vec![
            node("a", "us", HealthState::Healthy, 7),
            node("b", "eu", HealthState::Healthy, 6),
            node("c", "ap", HealthState::Healthy, 8),
        ];
        let second = select_dynamic_quorum(
            &round_2,
            &policy,
            11,
            Some(&SelectionHistory {
                round: 10,
                selected_nodes: first.selected_nodes.clone(),
            }),
        )
        .expect("decision");

        assert_eq!(first.selected_nodes, second.selected_nodes);
        assert!(
            second
                .reasons
                .iter()
                .any(|row| row.contains("hysteresis hold"))
        );
    }
}
