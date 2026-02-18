use crate::db::shard::advisor::{
    AdvisorRecommendation, ShardKeyTelemetryProfile, conformance_gate, recommend,
};
use crate::db::shard::evolution::{EvolutionPlan, EvolutionPlanInput, plan};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateThresholds {
    pub max_advisor_risk_per_mille: u64,
    pub max_copy_window_seconds: u64,
    pub max_dual_write_overhead_per_mille: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateFailure {
    pub code: &'static str,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateReport {
    pub ok: bool,
    pub failures: Vec<GateFailure>,
    pub advisor_offenders: Vec<AdvisorRecommendation>,
}

pub fn evaluate_gates(
    profiles: &[ShardKeyTelemetryProfile],
    plans: &[EvolutionPlan],
    thresholds: &GateThresholds,
) -> GateReport {
    let mut failures = Vec::new();

    let advisor_offenders = match conformance_gate(profiles, thresholds.max_advisor_risk_per_mille)
    {
        Ok(()) => Vec::new(),
        Err(offenders) => {
            failures.push(GateFailure {
                code: "ADVISOR_RISK_EXCEEDED",
                message: format!(
                    "{} advisor recommendations exceed risk threshold {}‰",
                    offenders.len(),
                    thresholds.max_advisor_risk_per_mille
                ),
            });
            offenders
        }
    };

    for p in plans {
        if p.predicted_copy_seconds > thresholds.max_copy_window_seconds {
            failures.push(GateFailure {
                code: "COPY_WINDOW_EXCEEDED",
                message: format!(
                    "relation {} predicted copy window {}s exceeds {}s",
                    p.relation, p.predicted_copy_seconds, thresholds.max_copy_window_seconds
                ),
            });
        }
        if p.predicted_dual_write_overhead_per_mille > thresholds.max_dual_write_overhead_per_mille
        {
            failures.push(GateFailure {
                code: "DUAL_WRITE_OVERHEAD_EXCEEDED",
                message: format!(
                    "relation {} dual-write overhead {}‰ exceeds {}‰",
                    p.relation,
                    p.predicted_dual_write_overhead_per_mille,
                    thresholds.max_dual_write_overhead_per_mille
                ),
            });
        }
    }

    GateReport {
        ok: failures.is_empty(),
        failures,
        advisor_offenders,
    }
}

pub fn evaluate_plan_inputs(
    profiles: &[ShardKeyTelemetryProfile],
    inputs: &[EvolutionPlanInput],
    thresholds: &GateThresholds,
) -> GateReport {
    let plans: Vec<EvolutionPlan> = inputs.iter().filter_map(|input| plan(input).ok()).collect();
    evaluate_gates(profiles, &plans, thresholds)
}

pub fn perf_budget_summary(plans: &[EvolutionPlan]) -> (u64, u64) {
    let total_copy_seconds = plans.iter().map(|p| p.predicted_copy_seconds).sum();
    let max_dual_write_overhead = plans
        .iter()
        .map(|p| p.predicted_dual_write_overhead_per_mille)
        .max()
        .unwrap_or(0);
    (total_copy_seconds, max_dual_write_overhead)
}

pub fn suggested_thresholds(profiles: &[ShardKeyTelemetryProfile]) -> GateThresholds {
    let max_risk = profiles
        .iter()
        .map(recommend)
        .map(|r| r.risk_score_per_mille)
        .max()
        .unwrap_or(400)
        .max(400);

    GateThresholds {
        max_advisor_risk_per_mille: max_risk,
        max_copy_window_seconds: 7200,
        max_dual_write_overhead_per_mille: 500,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::shard::advisor::ShardKeyTelemetryProfile;

    #[test]
    fn gate_report_flags_budget_and_risk_violations() {
        let profiles = vec![ShardKeyTelemetryProfile {
            relation: "orders".to_string(),
            key_spec: "region".to_string(),
            shard_count: 2,
            total_reads: 1000,
            total_writes: 100,
            total_observations: 1100,
            hottest_shard: 1,
            hottest_shard_ops: 1000,
            coldest_shard: 2,
            coldest_shard_ops: 100,
            skew_per_mille: 909,
            cardinality_ratio_per_mille: 20,
        }];

        let plans = vec![EvolutionPlan {
            relation: "orders".to_string(),
            from_key: "region".to_string(),
            to_key: "region,user_id".to_string(),
            predicted_copy_seconds: 10_000,
            predicted_dual_write_overhead_per_mille: 700,
            risk: crate::db::shard::evolution::EvolutionRisk {
                score_per_mille: 700,
                reasons: vec!["stress".to_string()],
            },
        }];

        let report = evaluate_gates(
            &profiles,
            &plans,
            &GateThresholds {
                max_advisor_risk_per_mille: 500,
                max_copy_window_seconds: 3600,
                max_dual_write_overhead_per_mille: 500,
            },
        );
        assert!(!report.ok);
        assert!(report.failures.len() >= 2);
    }
}
