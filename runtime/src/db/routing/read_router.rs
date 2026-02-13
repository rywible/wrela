use crate::db::routing::telemetry::{ReadTelemetryStore, RegionTelemetry};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadConsistencyMode {
    Strong,
    BoundedStale { max_staleness_ms: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadRouteCandidate {
    pub region: String,
    pub strong_consistent: bool,
    pub estimated_staleness_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadRoutingDecision {
    pub selected_region: String,
    pub mode: ReadConsistencyMode,
    pub score: u64,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadRoutingError {
    EmptyCandidateSet,
    NoEligibleRoute,
}

pub fn choose_read_region(
    shard: &[u8],
    mode: ReadConsistencyMode,
    candidates: &[ReadRouteCandidate],
    telemetry: &ReadTelemetryStore,
) -> Result<ReadRoutingDecision, ReadRoutingError> {
    if candidates.is_empty() {
        return Err(ReadRoutingError::EmptyCandidateSet);
    }

    let telemetry_map: BTreeMap<String, RegionTelemetry> = telemetry
        .snapshot_for_shard(shard)
        .into_iter()
        .map(|row| (row.region.clone(), row))
        .collect();

    let mut eligible = Vec::new();
    for candidate in candidates {
        let mode_ok = match mode {
            ReadConsistencyMode::Strong => candidate.strong_consistent,
            ReadConsistencyMode::BoundedStale { max_staleness_ms } => {
                candidate.estimated_staleness_ms <= max_staleness_ms
            }
        };
        if mode_ok {
            eligible.push(candidate);
        }
    }
    if eligible.is_empty() {
        return Err(ReadRoutingError::NoEligibleRoute);
    }

    let mut scored: Vec<(&ReadRouteCandidate, u64, Vec<String>)> = eligible
        .into_iter()
        .map(|candidate| {
            let telem = telemetry_map.get(&candidate.region);
            let latency = telem.map_or(250_u64, |row| row.ewma_latency_ms);
            let demand = telem.map_or(0_u64, |row| row.recent_reads.min(5_000));
            let staleness_penalty = candidate.estimated_staleness_ms.min(5_000);

            // Lower score is better. Demand-heavy regions get a small bonus if latency is close.
            let score = latency
                .saturating_mul(100)
                .saturating_add(staleness_penalty)
                .saturating_sub(demand / 20);

            let reasons = vec![
                format!("region={}", candidate.region),
                format!("latency_ms={latency}"),
                format!(
                    "estimated_staleness_ms={}",
                    candidate.estimated_staleness_ms
                ),
                format!("recent_reads={demand}"),
                format!("score={score}"),
            ];
            (candidate, score, reasons)
        })
        .collect();

    scored.sort_by(|(a, a_score, _), (b, b_score, _)| {
        a_score
            .cmp(b_score)
            .then_with(|| a.region.cmp(&b.region))
            .then_with(|| a.estimated_staleness_ms.cmp(&b.estimated_staleness_ms))
    });

    let (winner, score, mut reasons) = scored.remove(0);
    reasons.insert(0, format!("mode={mode:?}"));
    if matches!(mode, ReadConsistencyMode::Strong) {
        reasons.push("strong guardrail enforced".to_string());
    }

    Ok(ReadRoutingDecision {
        selected_region: winner.region.clone(),
        mode,
        score,
        reasons,
    })
}

#[cfg(test)]
mod tests {
    use super::{ReadConsistencyMode, ReadRouteCandidate, ReadRoutingError, choose_read_region};
    use crate::db::routing::telemetry::{ReadSample, ReadTelemetryStore};

    #[test]
    fn strong_mode_rejects_non_strong_candidates() {
        let telemetry = ReadTelemetryStore::new(3_000);
        let err = choose_read_region(
            b"orders",
            ReadConsistencyMode::Strong,
            &[ReadRouteCandidate {
                region: "eu".to_string(),
                strong_consistent: false,
                estimated_staleness_ms: 3,
            }],
            &telemetry,
        )
        .expect_err("must reject non-strong candidate");
        assert_eq!(err, ReadRoutingError::NoEligibleRoute);
    }

    #[test]
    fn router_adapts_to_telemetry_shift() {
        let mut telemetry = ReadTelemetryStore::new(4_000);
        telemetry.record(ReadSample {
            shard: b"orders".to_vec(),
            region: "eu".to_string(),
            latency_ms: 18,
            reads: 20,
        });
        telemetry.record(ReadSample {
            shard: b"orders".to_vec(),
            region: "us".to_string(),
            latency_ms: 8,
            reads: 12,
        });

        let candidates = vec![
            ReadRouteCandidate {
                region: "eu".to_string(),
                strong_consistent: true,
                estimated_staleness_ms: 2,
            },
            ReadRouteCandidate {
                region: "us".to_string(),
                strong_consistent: true,
                estimated_staleness_ms: 2,
            },
        ];

        let first = choose_read_region(
            b"orders",
            ReadConsistencyMode::BoundedStale {
                max_staleness_ms: 20,
            },
            &candidates,
            &telemetry,
        )
        .expect("decision");
        assert_eq!(first.selected_region, "us");

        for _ in 0..5 {
            telemetry.record(ReadSample {
                shard: b"orders".to_vec(),
                region: "eu".to_string(),
                latency_ms: 5,
                reads: 50,
            });
        }

        let second = choose_read_region(
            b"orders",
            ReadConsistencyMode::BoundedStale {
                max_staleness_ms: 20,
            },
            &candidates,
            &telemetry,
        )
        .expect("decision");
        assert_eq!(second.selected_region, "eu");
    }
}
