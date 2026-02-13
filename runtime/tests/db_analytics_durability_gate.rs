use wrela_runtime::db::analytics::durability::{
    AnalyticsCheckpoint, AnalyticsDurabilityError, build_envelope, plan_recovery,
};
use wrela_runtime::db::analytics::ga_gate::{
    AnalyticsCorrectnessEvidence, AnalyticsDurabilityEvidence, AnalyticsGaGateInput,
    AnalyticsPerfEvidence, AnalyticsResidencyEvidence, evaluate,
};
use wrela_runtime::db::backup::SnapshotMetadata;
use wrela_runtime::db::snapshot::builder::build_manifest;
use wrela_runtime::db::time::hlc::HlTimestamp;

#[test]
fn analytics_durability_recovery_plan_is_checkpoint_aware() {
    let payload = b"analytics-durable-payload";
    let manifest = build_manifest(payload, 88, 4);
    let envelope = build_envelope(
        "orders_analytics",
        "s3://analytics/orders/snapshot",
        SnapshotMetadata {
            last_index: manifest.last_index,
            last_term: manifest.last_term,
            checksum: manifest.checksum,
        },
        AnalyticsCheckpoint {
            stream: "orders".to_string(),
            commit_seq: 140,
            watermark_packed: HlTimestamp {
                physical_ms: 9_999,
                logical: 1,
            }
            .pack(),
        },
    )
    .expect("envelope should build");

    let plan = plan_recovery(&envelope, &manifest, payload, 120).expect("recovery should pass");
    assert_eq!(plan.restore_commit_seq, 140);
    assert_eq!(plan.replay_from_commit_seq, 141);
}

#[test]
fn analytics_durability_recovery_fails_closed_on_regression() {
    let payload = b"analytics-durable-payload";
    let manifest = build_manifest(payload, 88, 4);
    let envelope = build_envelope(
        "orders_analytics",
        "s3://analytics/orders/snapshot",
        SnapshotMetadata {
            last_index: manifest.last_index,
            last_term: manifest.last_term,
            checksum: manifest.checksum,
        },
        AnalyticsCheckpoint {
            stream: "orders".to_string(),
            commit_seq: 15,
            watermark_packed: 1,
        },
    )
    .expect("envelope should build");

    let err = plan_recovery(&envelope, &manifest, payload, 20).expect_err("must fail closed");
    assert!(matches!(
        err,
        AnalyticsDurabilityError::CheckpointRegression {
            expected_at_least: 20,
            actual: 15
        }
    ));
}

#[test]
fn analytics_ga_gate_emits_pass_and_failure_signals() {
    let pass = evaluate(&AnalyticsGaGateInput {
        correctness: AnalyticsCorrectnessEvidence {
            cdc_checkpoint_monotonic: true,
            query_repeatable: true,
            federated_merge_deterministic: true,
        },
        perf: AnalyticsPerfEvidence {
            p95_latency_ms: 20.0,
            max_p95_latency_ms: 30.0,
            ingest_rows_per_sec: 30_000.0,
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
    assert!(pass.passed);

    let fail = evaluate(&AnalyticsGaGateInput {
        correctness: AnalyticsCorrectnessEvidence {
            cdc_checkpoint_monotonic: false,
            query_repeatable: true,
            federated_merge_deterministic: true,
        },
        perf: AnalyticsPerfEvidence {
            p95_latency_ms: 90.0,
            max_p95_latency_ms: 30.0,
            ingest_rows_per_sec: 1_000.0,
            min_ingest_rows_per_sec: 20_000.0,
        },
        durability: AnalyticsDurabilityEvidence {
            backup_restore_roundtrip: false,
            checkpoint_recovery_proven: false,
        },
        residency: AnalyticsResidencyEvidence {
            policy_enforced: false,
            violations: Vec::new(),
        },
    });
    assert!(!fail.passed);
    assert!(fail.failures.len() >= 4);
}
