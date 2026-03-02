use std::collections::BTreeMap;
use wrela_runtime::db::routing::{RoutingPolicyError, ShardKeyPolicy};

#[test]
fn composite_policy_routes_and_is_stable() {
    let policy = ShardKeyPolicy::new(
        vec!["tenant_id".to_string(), "order_id".to_string()],
        64,
        None,
    )
    .expect("policy");

    let mut row = BTreeMap::new();
    row.insert("tenant_id".to_string(), b"tenant-42".to_vec());
    row.insert("order_id".to_string(), b"order-123".to_vec());

    let route_a = policy.route_row(&row).expect("route");
    let route_b = policy.route_row(&row).expect("route");

    assert_eq!(route_a.shard_id, route_b.shard_id);
    assert_eq!(route_a.shard_key, route_b.shard_key);
    assert!(route_a.shard_id < policy.shard_count());
}

#[test]
fn single_field_policy_requires_non_empty_waiver() {
    let err = ShardKeyPolicy::new(vec!["tenant_id".to_string()], 16, None).expect_err("deny");
    assert_eq!(err, RoutingPolicyError::MissingSingleFieldWaiver);

    let err = ShardKeyPolicy::new(vec!["tenant_id".to_string()], 16, Some("  ".to_string()))
        .expect_err("deny");
    assert_eq!(err, RoutingPolicyError::EmptySingleFieldWaiver);

    let allowed = ShardKeyPolicy::new(
        vec!["tenant_id".to_string()],
        16,
        Some("legacy table with controlled tenant fanout".to_string()),
    )
    .expect("allow");
    assert_eq!(
        allowed.single_field_waiver_reason(),
        Some("legacy table with controlled tenant fanout")
    );
}

#[test]
fn route_requires_all_policy_fields() {
    let policy = ShardKeyPolicy::new(
        vec!["tenant_id".to_string(), "order_id".to_string()],
        32,
        None,
    )
    .expect("policy");

    let row = BTreeMap::from([("tenant_id".to_string(), b"tenant-42".to_vec())]);
    let err = policy
        .route_row(&row)
        .expect_err("missing order_id should fail");
    assert_eq!(
        err,
        RoutingPolicyError::MissingRouteField("order_id".to_string())
    );
}
