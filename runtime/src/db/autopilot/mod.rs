use std::collections::BTreeMap;

use crate::db::quorum::{QuorumSafetyError, QuorumSafetySimulation, simulate_quorum_safety};
use crate::db::routing::health::NodeHealth;

pub mod compiler;
pub mod compliance;
pub mod orchestrator;
pub mod read_slo_controller;

pub const DEFAULT_MAX_SKEW_RATIO: f64 = 1.5;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkewPolicyError {
    EmptyShardSet,
    InvalidThreshold,
    ZeroTotalLoad,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SkewPolicyDecision {
    pub passes: bool,
    pub threshold: f64,
    pub max_to_mean_ratio: f64,
    pub hottest_shard: String,
    pub hottest_load: u64,
    pub total_load: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SafetySimulationDecision {
    pub passes: bool,
    pub skew: SkewPolicyDecision,
    pub quorum: QuorumSafetySimulation,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SafetySimulationError {
    Skew(SkewPolicyError),
    Quorum(QuorumSafetyError),
}

#[derive(Debug, Clone)]
pub struct SafetySimulationInput<'a> {
    pub shard_loads: &'a BTreeMap<String, u64>,
    pub skew_threshold: f64,
    pub quorum_candidates: &'a [NodeHealth],
    pub desired_voters: usize,
    pub latency_hint_ms: &'a BTreeMap<String, u64>,
    pub required_additional_failures: usize,
    pub max_degraded_selected: usize,
}

pub fn evaluate_shard_skew(
    shard_loads: &BTreeMap<String, u64>,
    threshold: f64,
) -> Result<SkewPolicyDecision, SkewPolicyError> {
    if shard_loads.is_empty() {
        return Err(SkewPolicyError::EmptyShardSet);
    }
    if !threshold.is_finite() || threshold <= 0.0 {
        return Err(SkewPolicyError::InvalidThreshold);
    }

    let mut total_load = 0u64;
    let mut hottest_shard = String::new();
    let mut hottest_load = 0u64;
    for (shard, load) in shard_loads {
        total_load = total_load.saturating_add(*load);
        if *load > hottest_load || hottest_shard.is_empty() {
            hottest_load = *load;
            hottest_shard = shard.clone();
        }
    }
    if total_load == 0 {
        return Err(SkewPolicyError::ZeroTotalLoad);
    }

    let mean = total_load as f64 / shard_loads.len() as f64;
    let max_to_mean_ratio = hottest_load as f64 / mean;
    Ok(SkewPolicyDecision {
        passes: max_to_mean_ratio <= threshold,
        threshold,
        max_to_mean_ratio,
        hottest_shard,
        hottest_load,
        total_load,
    })
}

pub fn evaluate_safety_simulation(
    input: SafetySimulationInput<'_>,
) -> Result<SafetySimulationDecision, SafetySimulationError> {
    let skew = evaluate_shard_skew(input.shard_loads, input.skew_threshold)
        .map_err(SafetySimulationError::Skew)?;
    let quorum = simulate_quorum_safety(
        input.quorum_candidates,
        input.desired_voters,
        input.latency_hint_ms,
        input.required_additional_failures,
        input.max_degraded_selected,
    )
    .map_err(SafetySimulationError::Quorum)?;

    let mut reasons = Vec::new();
    if !skew.passes {
        reasons.push(format!(
            "skew ratio {:.3} exceeds threshold {:.3}",
            skew.max_to_mean_ratio, skew.threshold
        ));
    }
    if !quorum.passes {
        if quorum.survivable_additional_failures < quorum.required_additional_failures {
            reasons.push(format!(
                "survivable additional failures {} below required {}",
                quorum.survivable_additional_failures, quorum.required_additional_failures
            ));
        }
        if quorum.degraded_selected > quorum.max_degraded_selected {
            reasons.push(format!(
                "degraded selected {} exceeds max {}",
                quorum.degraded_selected, quorum.max_degraded_selected
            ));
        }
    }
    let passes = reasons.is_empty();
    if passes {
        reasons.push("safety simulation passed".to_string());
    }

    Ok(SafetySimulationDecision {
        passes,
        skew,
        quorum,
        reasons,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        DEFAULT_MAX_SKEW_RATIO, SafetySimulationInput, evaluate_safety_simulation,
        evaluate_shard_skew,
    };
    use crate::db::routing::health::{HealthState, MemberRole, NodeHealth};
    use std::collections::BTreeMap;

    #[test]
    fn balanced_load_passes_default_threshold() {
        let loads = BTreeMap::from([
            ("shard-a".to_string(), 1200),
            ("shard-b".to_string(), 1100),
            ("shard-c".to_string(), 1000),
        ]);
        let decision = evaluate_shard_skew(&loads, DEFAULT_MAX_SKEW_RATIO).expect("decision");
        assert!(decision.passes);
    }

    #[test]
    fn hotspot_load_fails_default_threshold() {
        let loads = BTreeMap::from([
            ("shard-a".to_string(), 4500),
            ("shard-b".to_string(), 900),
            ("shard-c".to_string(), 800),
        ]);
        let decision = evaluate_shard_skew(&loads, DEFAULT_MAX_SKEW_RATIO).expect("decision");
        assert!(!decision.passes);
        assert_eq!(decision.hottest_shard, "shard-a");
    }

    #[test]
    fn simulation_passes_when_skew_and_quorum_guards_pass() {
        let loads = BTreeMap::from([
            ("shard-a".to_string(), 1400),
            ("shard-b".to_string(), 1300),
            ("shard-c".to_string(), 1200),
        ]);
        let nodes = vec![
            NodeHealth {
                node_id: "n1".to_string(),
                region: "us".to_string(),
                state: HealthState::Healthy,
                observed_at_ms: 1,
                role: MemberRole::Unknown,
            },
            NodeHealth {
                node_id: "n2".to_string(),
                region: "eu".to_string(),
                state: HealthState::Healthy,
                observed_at_ms: 1,
                role: MemberRole::Unknown,
            },
            NodeHealth {
                node_id: "n3".to_string(),
                region: "ap".to_string(),
                state: HealthState::Degraded,
                observed_at_ms: 1,
                role: MemberRole::Unknown,
            },
        ];
        let latency = BTreeMap::from([
            ("n1".to_string(), 4_u64),
            ("n2".to_string(), 6_u64),
            ("n3".to_string(), 9_u64),
        ]);

        let decision = evaluate_safety_simulation(SafetySimulationInput {
            shard_loads: &loads,
            skew_threshold: DEFAULT_MAX_SKEW_RATIO,
            quorum_candidates: &nodes,
            desired_voters: 3,
            latency_hint_ms: &latency,
            required_additional_failures: 1,
            max_degraded_selected: 1,
        })
        .expect("simulation");
        assert!(decision.passes);
        assert_eq!(
            decision.reasons,
            vec!["safety simulation passed".to_string()]
        );
    }
}
