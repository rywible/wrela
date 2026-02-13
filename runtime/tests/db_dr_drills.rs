use wrela_runtime::db::drill::{DrillMeasurement, DrillThresholds, evaluate_drill, report_json};

#[test]
fn drill_suite_reports_machine_readable_outcomes_for_ci() {
    let report = evaluate_drill(
        DrillMeasurement {
            pre_outage_commit_seq: 4_000,
            recovered_commit_seq: 4_001,
            outage_started_ms: 1_000,
            recovered_ms: 2_900,
            degraded_network: true,
            partial_failure: true,
        },
        DrillThresholds {
            max_rpo_commits: 2,
            max_rto_ms: 2_500,
        },
    );

    assert!(report.rpo_pass);
    assert!(report.rto_pass);
    assert!(report.overall_pass);

    let json = report_json(&report).expect("serialize report");
    assert!(json.contains("\"overall_pass\": true"));
    println!("DRILL_REPORT_JSON:{}", json.replace('\n', ""));
}

#[test]
fn drill_gate_fails_when_rpo_or_rto_contract_is_violated() {
    let report = evaluate_drill(
        DrillMeasurement {
            pre_outage_commit_seq: 10,
            recovered_commit_seq: 20,
            outage_started_ms: 5_000,
            recovered_ms: 9_000,
            degraded_network: false,
            partial_failure: false,
        },
        DrillThresholds {
            max_rpo_commits: 1,
            max_rto_ms: 1_000,
        },
    );

    assert!(!report.overall_pass);
    assert!(!report.rpo_pass);
    assert!(!report.rto_pass);
}
