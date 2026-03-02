use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DrillThresholds {
    pub max_rpo_commits: u64,
    pub max_rto_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DrillMeasurement {
    pub pre_outage_commit_seq: u64,
    pub recovered_commit_seq: u64,
    pub outage_started_ms: u64,
    pub recovered_ms: u64,
    pub degraded_network: bool,
    pub partial_failure: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DrillReport {
    pub rpo_commits: u64,
    pub rto_ms: u64,
    pub thresholds: DrillThresholds,
    pub rpo_pass: bool,
    pub rto_pass: bool,
    pub overall_pass: bool,
    pub degraded_network: bool,
    pub partial_failure: bool,
}

pub fn evaluate_drill(measurement: DrillMeasurement, thresholds: DrillThresholds) -> DrillReport {
    let rpo_commits = measurement
        .recovered_commit_seq
        .saturating_sub(measurement.pre_outage_commit_seq);
    let rto_ms = measurement
        .recovered_ms
        .saturating_sub(measurement.outage_started_ms);
    let rpo_pass = rpo_commits <= thresholds.max_rpo_commits;
    let rto_pass = rto_ms <= thresholds.max_rto_ms;

    DrillReport {
        rpo_commits,
        rto_ms,
        thresholds,
        rpo_pass,
        rto_pass,
        overall_pass: rpo_pass && rto_pass,
        degraded_network: measurement.degraded_network,
        partial_failure: measurement.partial_failure,
    }
}

pub fn report_json(report: &DrillReport) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drill_fails_when_rto_exceeds_threshold() {
        let report = evaluate_drill(
            DrillMeasurement {
                pre_outage_commit_seq: 100,
                recovered_commit_seq: 101,
                outage_started_ms: 10_000,
                recovered_ms: 14_000,
                degraded_network: false,
                partial_failure: false,
            },
            DrillThresholds {
                max_rpo_commits: 2,
                max_rto_ms: 3_000,
            },
        );

        assert!(report.rpo_pass);
        assert!(!report.rto_pass);
        assert!(!report.overall_pass);
    }
}
