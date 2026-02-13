use crate::db::routing::health::{HealthState, NodeHealth};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuorumSelection {
    pub selected_nodes: Vec<String>,
    pub quorum_size: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuorumSafetySimulation {
    pub passes: bool,
    pub quorum: QuorumSelection,
    pub available_nodes: usize,
    pub healthy_selected: usize,
    pub degraded_selected: usize,
    pub max_selected_latency_ms: u64,
    pub survivable_additional_failures: usize,
    pub required_additional_failures: usize,
    pub max_degraded_selected: usize,
    pub timeline: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuorumSelectionError {
    EmptyCandidates,
    InsufficientHealthyNodes { required: usize, available: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuorumSafetyError {
    Selection(QuorumSelectionError),
}

pub fn select_nearest_healthy_quorum(
    candidates: &[NodeHealth],
    desired_voters: usize,
    latency_hint_ms: &BTreeMap<String, u64>,
) -> Result<QuorumSelection, QuorumSelectionError> {
    if candidates.is_empty() || desired_voters == 0 {
        return Err(QuorumSelectionError::EmptyCandidates);
    }

    let quorum = (desired_voters / 2) + 1;
    let mut healthy: Vec<&NodeHealth> = candidates
        .iter()
        .filter(|node| node.state != HealthState::Unavailable)
        .collect();

    if healthy.len() < quorum {
        return Err(QuorumSelectionError::InsufficientHealthyNodes {
            required: quorum,
            available: healthy.len(),
        });
    }

    healthy.sort_by(|a, b| {
        let a_lat = latency_hint_ms
            .get(&a.node_id)
            .copied()
            .unwrap_or(u64::MAX / 2);
        let b_lat = latency_hint_ms
            .get(&b.node_id)
            .copied()
            .unwrap_or(u64::MAX / 2);
        a_lat.cmp(&b_lat).then_with(|| a.node_id.cmp(&b.node_id))
    });

    Ok(QuorumSelection {
        selected_nodes: healthy
            .into_iter()
            .take(quorum)
            .map(|node| node.node_id.clone())
            .collect(),
        quorum_size: quorum,
    })
}

pub fn simulate_quorum_safety(
    candidates: &[NodeHealth],
    desired_voters: usize,
    latency_hint_ms: &BTreeMap<String, u64>,
    required_additional_failures: usize,
    max_degraded_selected: usize,
) -> Result<QuorumSafetySimulation, QuorumSafetyError> {
    let quorum = select_nearest_healthy_quorum(candidates, desired_voters, latency_hint_ms)
        .map_err(QuorumSafetyError::Selection)?;

    let availability: BTreeMap<&str, HealthState> = candidates
        .iter()
        .map(|node| (node.node_id.as_str(), node.state))
        .collect();

    let available_nodes = candidates
        .iter()
        .filter(|node| node.state != HealthState::Unavailable)
        .count();
    let healthy_selected = quorum
        .selected_nodes
        .iter()
        .filter(|node_id| availability.get(node_id.as_str()) == Some(&HealthState::Healthy))
        .count();
    let degraded_selected = quorum
        .selected_nodes
        .iter()
        .filter(|node_id| availability.get(node_id.as_str()) == Some(&HealthState::Degraded))
        .count();

    let survivable_additional_failures = available_nodes.saturating_sub(quorum.quorum_size);
    let max_selected_latency_ms = quorum
        .selected_nodes
        .iter()
        .map(|node_id| {
            latency_hint_ms
                .get(node_id)
                .copied()
                .unwrap_or(u64::MAX / 2)
        })
        .max()
        .unwrap_or(0);
    let passes = survivable_additional_failures >= required_additional_failures
        && degraded_selected <= max_degraded_selected;

    Ok(QuorumSafetySimulation {
        passes,
        quorum: quorum.clone(),
        available_nodes,
        healthy_selected,
        degraded_selected,
        max_selected_latency_ms,
        survivable_additional_failures,
        required_additional_failures,
        max_degraded_selected,
        timeline: vec![
            format!("available_nodes={available_nodes}"),
            format!("quorum_size={}", quorum.quorum_size),
            format!("selected_nodes={}", quorum.selected_nodes.join(",")),
            format!("survivable_additional_failures={survivable_additional_failures}"),
            format!("degraded_selected={degraded_selected}"),
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_lowest_latency_non_unavailable_nodes() {
        let candidates = vec![
            NodeHealth {
                node_id: "a".to_string(),
                region: "us".to_string(),
                state: HealthState::Healthy,
                observed_at_ms: 1,
            },
            NodeHealth {
                node_id: "b".to_string(),
                region: "us".to_string(),
                state: HealthState::Degraded,
                observed_at_ms: 1,
            },
            NodeHealth {
                node_id: "c".to_string(),
                region: "eu".to_string(),
                state: HealthState::Unavailable,
                observed_at_ms: 1,
            },
        ];
        let latency = BTreeMap::from([
            ("a".to_string(), 8_u64),
            ("b".to_string(), 5_u64),
            ("c".to_string(), 1_u64),
        ]);

        let selected = select_nearest_healthy_quorum(&candidates, 3, &latency).expect("quorum");
        assert_eq!(selected.quorum_size, 2);
        assert_eq!(
            selected.selected_nodes,
            vec!["b".to_string(), "a".to_string()]
        );
    }

    #[test]
    fn safety_simulation_is_deterministic_for_same_inputs() {
        let candidates = vec![
            NodeHealth {
                node_id: "a".to_string(),
                region: "us".to_string(),
                state: HealthState::Healthy,
                observed_at_ms: 1,
            },
            NodeHealth {
                node_id: "b".to_string(),
                region: "us".to_string(),
                state: HealthState::Degraded,
                observed_at_ms: 1,
            },
            NodeHealth {
                node_id: "c".to_string(),
                region: "eu".to_string(),
                state: HealthState::Healthy,
                observed_at_ms: 1,
            },
        ];
        let latency = BTreeMap::from([
            ("a".to_string(), 6_u64),
            ("b".to_string(), 8_u64),
            ("c".to_string(), 5_u64),
        ]);

        let sim_a = simulate_quorum_safety(&candidates, 3, &latency, 1, 1).expect("sim");
        let sim_b = simulate_quorum_safety(&candidates, 3, &latency, 1, 1).expect("sim");
        assert_eq!(sim_a, sim_b);
        assert!(sim_a.passes);
    }

    #[test]
    fn safety_simulation_fails_when_failure_budget_not_met() {
        let candidates = vec![
            NodeHealth {
                node_id: "a".to_string(),
                region: "us".to_string(),
                state: HealthState::Healthy,
                observed_at_ms: 1,
            },
            NodeHealth {
                node_id: "b".to_string(),
                region: "us".to_string(),
                state: HealthState::Degraded,
                observed_at_ms: 1,
            },
            NodeHealth {
                node_id: "c".to_string(),
                region: "eu".to_string(),
                state: HealthState::Unavailable,
                observed_at_ms: 1,
            },
        ];
        let latency = BTreeMap::from([("a".to_string(), 6_u64), ("b".to_string(), 8_u64)]);
        let sim = simulate_quorum_safety(&candidates, 3, &latency, 1, 0).expect("sim");
        assert!(!sim.passes);
        assert_eq!(sim.survivable_additional_failures, 0);
        assert_eq!(sim.degraded_selected, 1);
    }
}
