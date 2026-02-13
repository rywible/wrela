use crate::db::analytics::policy::FederatedResidencyError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalyticsCorrectnessEvidence {
    pub cdc_checkpoint_monotonic: bool,
    pub query_repeatable: bool,
    pub federated_merge_deterministic: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AnalyticsPerfEvidence {
    pub p95_latency_ms: f64,
    pub max_p95_latency_ms: f64,
    pub ingest_rows_per_sec: f64,
    pub min_ingest_rows_per_sec: f64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalyticsDurabilityEvidence {
    pub backup_restore_roundtrip: bool,
    pub checkpoint_recovery_proven: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalyticsResidencyEvidence {
    pub policy_enforced: bool,
    pub violations: Vec<FederatedResidencyError>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AnalyticsGaGateInput {
    pub correctness: AnalyticsCorrectnessEvidence,
    pub perf: AnalyticsPerfEvidence,
    pub durability: AnalyticsDurabilityEvidence,
    pub residency: AnalyticsResidencyEvidence,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnalyticsGaGateCode {
    CorrectnessFailure,
    PerfLatencyExceeded,
    PerfThroughputRegression,
    DurabilityFailure,
    ResidencyFailure,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalyticsGaGateFailure {
    pub code: AnalyticsGaGateCode,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalyticsGaGateReport {
    pub passed: bool,
    pub failures: Vec<AnalyticsGaGateFailure>,
}

pub fn evaluate(input: &AnalyticsGaGateInput) -> AnalyticsGaGateReport {
    let mut failures = Vec::new();

    if !(input.correctness.cdc_checkpoint_monotonic
        && input.correctness.query_repeatable
        && input.correctness.federated_merge_deterministic)
    {
        failures.push(AnalyticsGaGateFailure {
            code: AnalyticsGaGateCode::CorrectnessFailure,
            detail: "correctness evidence must all pass".to_string(),
        });
    }

    if input.perf.p95_latency_ms > input.perf.max_p95_latency_ms {
        failures.push(AnalyticsGaGateFailure {
            code: AnalyticsGaGateCode::PerfLatencyExceeded,
            detail: format!(
                "p95 latency {:.2}ms exceeds {:.2}ms",
                input.perf.p95_latency_ms, input.perf.max_p95_latency_ms
            ),
        });
    }

    if input.perf.ingest_rows_per_sec < input.perf.min_ingest_rows_per_sec {
        failures.push(AnalyticsGaGateFailure {
            code: AnalyticsGaGateCode::PerfThroughputRegression,
            detail: format!(
                "ingest throughput {:.2} rows/s below {:.2}",
                input.perf.ingest_rows_per_sec, input.perf.min_ingest_rows_per_sec
            ),
        });
    }

    if !(input.durability.backup_restore_roundtrip && input.durability.checkpoint_recovery_proven) {
        failures.push(AnalyticsGaGateFailure {
            code: AnalyticsGaGateCode::DurabilityFailure,
            detail: "durability evidence incomplete".to_string(),
        });
    }

    if !input.residency.policy_enforced || !input.residency.violations.is_empty() {
        failures.push(AnalyticsGaGateFailure {
            code: AnalyticsGaGateCode::ResidencyFailure,
            detail: if input.residency.violations.is_empty() {
                "residency policy not enforced".to_string()
            } else {
                format!(
                    "residency violations present: {}",
                    input.residency.violations.len()
                )
            },
        });
    }

    AnalyticsGaGateReport {
        passed: failures.is_empty(),
        failures,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AnalyticsCorrectnessEvidence, AnalyticsDurabilityEvidence, AnalyticsGaGateCode,
        AnalyticsGaGateInput, AnalyticsPerfEvidence, AnalyticsResidencyEvidence, evaluate,
    };

    #[test]
    fn ga_gate_passes_for_clean_evidence() {
        let report = evaluate(&AnalyticsGaGateInput {
            correctness: AnalyticsCorrectnessEvidence {
                cdc_checkpoint_monotonic: true,
                query_repeatable: true,
                federated_merge_deterministic: true,
            },
            perf: AnalyticsPerfEvidence {
                p95_latency_ms: 22.0,
                max_p95_latency_ms: 30.0,
                ingest_rows_per_sec: 25_000.0,
                min_ingest_rows_per_sec: 20_000.0,
            },
            durability: AnalyticsDurabilityEvidence {
                backup_restore_roundtrip: true,
                checkpoint_recovery_proven: true,
            },
            residency: AnalyticsResidencyEvidence {
                policy_enforced: true,
                violations: Vec::new(),
            },
        });

        assert!(report.passed);
        assert!(report.failures.is_empty());
    }

    #[test]
    fn ga_gate_reports_typed_failures() {
        let report = evaluate(&AnalyticsGaGateInput {
            correctness: AnalyticsCorrectnessEvidence {
                cdc_checkpoint_monotonic: false,
                query_repeatable: true,
                federated_merge_deterministic: true,
            },
            perf: AnalyticsPerfEvidence {
                p95_latency_ms: 99.0,
                max_p95_latency_ms: 30.0,
                ingest_rows_per_sec: 1_000.0,
                min_ingest_rows_per_sec: 20_000.0,
            },
            durability: AnalyticsDurabilityEvidence {
                backup_restore_roundtrip: false,
                checkpoint_recovery_proven: true,
            },
            residency: AnalyticsResidencyEvidence {
                policy_enforced: false,
                violations: Vec::new(),
            },
        });

        assert!(!report.passed);
        assert!(
            report
                .failures
                .iter()
                .any(|failure| failure.code == AnalyticsGaGateCode::CorrectnessFailure)
        );
        assert!(
            report
                .failures
                .iter()
                .any(|failure| failure.code == AnalyticsGaGateCode::PerfLatencyExceeded)
        );
        assert!(
            report
                .failures
                .iter()
                .any(|failure| failure.code == AnalyticsGaGateCode::PerfThroughputRegression)
        );
        assert!(
            report
                .failures
                .iter()
                .any(|failure| failure.code == AnalyticsGaGateCode::DurabilityFailure)
        );
        assert!(
            report
                .failures
                .iter()
                .any(|failure| failure.code == AnalyticsGaGateCode::ResidencyFailure)
        );
    }
}
