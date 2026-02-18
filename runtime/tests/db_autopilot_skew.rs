use std::collections::BTreeMap;
use wrela_runtime::db::autopilot::{DEFAULT_MAX_SKEW_RATIO, SkewPolicyError, evaluate_shard_skew};

#[test]
fn default_threshold_accepts_balanced_projection() {
    let loads = BTreeMap::from([
        ("shard-a".to_string(), 1200),
        ("shard-b".to_string(), 1000),
        ("shard-c".to_string(), 1100),
    ]);

    let decision = evaluate_shard_skew(&loads, DEFAULT_MAX_SKEW_RATIO).expect("decision");
    assert!(decision.passes);
    assert_eq!(decision.hottest_shard, "shard-a");
    assert_eq!(decision.total_load, 3300);
}

#[test]
fn default_threshold_rejects_hotspot_projection() {
    let loads = BTreeMap::from([
        ("shard-a".to_string(), 5000),
        ("shard-b".to_string(), 700),
        ("shard-c".to_string(), 600),
    ]);

    let decision = evaluate_shard_skew(&loads, DEFAULT_MAX_SKEW_RATIO).expect("decision");
    assert!(!decision.passes);
    assert!(decision.max_to_mean_ratio > DEFAULT_MAX_SKEW_RATIO);
}

#[test]
fn empty_projection_is_invalid() {
    let loads = BTreeMap::new();
    let err = evaluate_shard_skew(&loads, DEFAULT_MAX_SKEW_RATIO).expect_err("must fail");
    assert_eq!(err, SkewPolicyError::EmptyShardSet);
}
