#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadRouteCandidate {
    pub region: String,
    pub p95_latency_ms: u64,
    pub throughput_rps: u64,
    pub residency_allowed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadSloPolicy {
    pub max_p95_latency_ms: u64,
    pub min_throughput_rps: u64,
    pub max_regions: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadSloAction {
    Hold,
    ShiftToFastest,
    ReduceRegionSet,
    FallbackPrimaryOnly,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadSloDecision {
    pub action: ReadSloAction,
    pub selected_regions: Vec<String>,
    pub aggregate_throughput_rps: u64,
    pub worst_selected_p95_ms: u64,
    pub reasons: Vec<String>,
}

pub fn evaluate_read_slo_controller(
    candidates: &[ReadRouteCandidate],
    policy: &ReadSloPolicy,
) -> ReadSloDecision {
    let mut allowed: Vec<&ReadRouteCandidate> = candidates
        .iter()
        .filter(|row| row.residency_allowed)
        .collect();

    allowed.sort_by(|a, b| {
        b.throughput_rps
            .cmp(&a.throughput_rps)
            .then_with(|| a.p95_latency_ms.cmp(&b.p95_latency_ms))
            .then_with(|| a.region.cmp(&b.region))
    });

    let cap = policy.max_regions.max(1);
    let selected: Vec<&ReadRouteCandidate> = allowed.into_iter().take(cap).collect();

    let selected_regions: Vec<String> = selected.iter().map(|row| row.region.clone()).collect();
    let aggregate_throughput_rps = selected
        .iter()
        .fold(0_u64, |acc, row| acc.saturating_add(row.throughput_rps));
    let worst_selected_p95_ms = selected
        .iter()
        .map(|row| row.p95_latency_ms)
        .max()
        .unwrap_or(0);

    let mut reasons = Vec::new();
    if selected_regions.is_empty() {
        reasons.push("no residency-compliant regions available".to_string());
        return ReadSloDecision {
            action: ReadSloAction::FallbackPrimaryOnly,
            selected_regions,
            aggregate_throughput_rps,
            worst_selected_p95_ms,
            reasons,
        };
    }

    if aggregate_throughput_rps < policy.min_throughput_rps {
        reasons.push(format!(
            "throughput {} below minimum {}",
            aggregate_throughput_rps, policy.min_throughput_rps
        ));
    }

    if worst_selected_p95_ms > policy.max_p95_latency_ms {
        reasons.push(format!(
            "p95 {}ms above max {}ms",
            worst_selected_p95_ms, policy.max_p95_latency_ms
        ));
    }

    let action = if reasons.is_empty() && selected_regions.len() > 1 {
        ReadSloAction::Hold
    } else if reasons.is_empty() {
        ReadSloAction::ShiftToFastest
    } else if selected_regions.len() > 1 {
        ReadSloAction::ReduceRegionSet
    } else {
        ReadSloAction::FallbackPrimaryOnly
    };

    if reasons.is_empty() {
        reasons.push("slo within guardrails".to_string());
    }

    ReadSloDecision {
        action,
        selected_regions,
        aggregate_throughput_rps,
        worst_selected_p95_ms,
        reasons,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controller_prefers_high_throughput_residency_allowed_regions() {
        let candidates = vec![
            ReadRouteCandidate {
                region: "us".to_string(),
                p95_latency_ms: 8,
                throughput_rps: 1200,
                residency_allowed: true,
            },
            ReadRouteCandidate {
                region: "eu".to_string(),
                p95_latency_ms: 12,
                throughput_rps: 900,
                residency_allowed: true,
            },
            ReadRouteCandidate {
                region: "ap".to_string(),
                p95_latency_ms: 6,
                throughput_rps: 700,
                residency_allowed: false,
            },
        ];

        let decision = evaluate_read_slo_controller(
            &candidates,
            &ReadSloPolicy {
                max_p95_latency_ms: 15,
                min_throughput_rps: 1800,
                max_regions: 2,
            },
        );

        assert_eq!(
            decision.selected_regions,
            vec!["us".to_string(), "eu".to_string()]
        );
        assert_eq!(decision.action, ReadSloAction::Hold);
    }

    #[test]
    fn controller_fails_closed_when_no_residency_allowed_regions_exist() {
        let candidates = vec![ReadRouteCandidate {
            region: "eu".to_string(),
            p95_latency_ms: 4,
            throughput_rps: 1500,
            residency_allowed: false,
        }];

        let decision = evaluate_read_slo_controller(
            &candidates,
            &ReadSloPolicy {
                max_p95_latency_ms: 10,
                min_throughput_rps: 500,
                max_regions: 2,
            },
        );

        assert_eq!(decision.action, ReadSloAction::FallbackPrimaryOnly);
        assert!(decision.selected_regions.is_empty());
    }
}
