use bytes::Bytes;
use std::collections::BTreeMap;
use wrela_runtime::db::analytics::columnar::ColumnarStore;
use wrela_runtime::db::analytics::federation::{FederatedMergeStrategy, FederatedSource};
use wrela_runtime::db::analytics::ingest::IngestPipeline;
use wrela_runtime::db::analytics::operators::Batch;
use wrela_runtime::db::analytics::policy::FederatedResidencyGuard;
use wrela_runtime::db::analytics::service::AnalyticsQueryRequest;
use wrela_runtime::db::api::core::{
    analytics_execute_federated, analytics_explain, analytics_query,
};
use wrela_runtime::db::cdc::CdcEmitter;
use wrela_runtime::db::security::residency::{ResidencyPolicy, ResidencyRule};
use wrela_runtime::db::time::hlc::HlTimestamp;
use wrela_runtime::db::time::watermarks::SafeReadWatermarks;

#[test]
fn analytics_query_contract_exposes_checkpoint_and_watermark() {
    let mut cdc = CdcEmitter::default();
    cdc.emit_put(
        Bytes::from_static(b"orders"),
        Bytes::from_static(b"o1"),
        Bytes::from_static(b"new"),
        1,
    );

    let mut pipeline = IngestPipeline::default();
    let mut store = ColumnarStore::default();
    let residency = ResidencyPolicy::with_rules(vec![ResidencyRule {
        shard: b"orders".to_vec(),
        allowed_regions: vec!["us".to_string()],
    }]);
    let guard = FederatedResidencyGuard::default();
    let watermarks = SafeReadWatermarks::new();
    watermarks.observe(
        1,
        HlTimestamp {
            physical_ms: 1_000,
            logical: 0,
        }
        .pack(),
    );

    let out = analytics_query(
        &AnalyticsQueryRequest {
            stream: "orders".to_string(),
            table: "orders".to_string(),
            value_column: "status".to_string(),
            sink_region: "us".to_string(),
            raw_export_requested: false,
            max_staleness_ms: 1,
        },
        &cdc,
        &mut pipeline,
        &mut store,
        &residency,
        &guard,
        &watermarks,
    )
    .expect("query should succeed");

    assert_eq!(out.rows.len(), 1);
    assert_eq!(out.next_checkpoint, Some(1));
    assert!(out.watermark_packed.is_some());
}

#[test]
fn analytics_explain_and_execute_federated_are_machine_readable() {
    let guard = FederatedResidencyGuard::default();
    let watermarks = SafeReadWatermarks::new();
    watermarks.observe(
        1,
        HlTimestamp {
            physical_ms: 1_000,
            logical: 0,
        }
        .pack(),
    );

    let sources = vec![
        FederatedSource {
            source_id: "left".to_string(),
            region: "us".to_string(),
            shard: b"orders".to_vec(),
            batch: Batch::new(BTreeMap::from([
                (
                    "id".to_string(),
                    vec![Some(b"1".to_vec()), Some(b"2".to_vec())],
                ),
                (
                    "value".to_string(),
                    vec![Some(b"x".to_vec()), Some(b"y".to_vec())],
                ),
            ])),
        },
        FederatedSource {
            source_id: "right".to_string(),
            region: "eu".to_string(),
            shard: b"payments".to_vec(),
            batch: Batch::new(BTreeMap::from([
                (
                    "id".to_string(),
                    vec![Some(b"2".to_vec()), Some(b"1".to_vec())],
                ),
                (
                    "value".to_string(),
                    vec![Some(b"gold".to_vec()), Some(b"silver".to_vec())],
                ),
            ])),
        },
    ];

    let explain = analytics_explain(
        "plan-public-1",
        &sources,
        FederatedMergeStrategy::HashJoinEq,
        &guard,
        &watermarks,
    )
    .expect("explain should pass");
    assert_eq!(explain.explain_schema_version, 1);
    assert_eq!(explain.source_ids.len(), 2);

    let out = analytics_execute_federated(
        "plan-public-2",
        &sources,
        FederatedMergeStrategy::HashJoinEq,
        &guard,
    )
    .expect("execution should pass");
    assert_eq!(out.row_count(), 2);
}

#[test]
fn analytics_query_denies_raw_export_in_default_mode() {
    let cdc = CdcEmitter::default();
    let mut pipeline = IngestPipeline::default();
    let mut store = ColumnarStore::default();
    let residency = ResidencyPolicy::with_rules(vec![ResidencyRule {
        shard: b"orders".to_vec(),
        allowed_regions: vec!["us".to_string()],
    }]);
    let guard = FederatedResidencyGuard::default();
    let watermarks = SafeReadWatermarks::new();

    let err = analytics_query(
        &AnalyticsQueryRequest {
            stream: "orders".to_string(),
            table: "orders".to_string(),
            value_column: "status".to_string(),
            sink_region: "us".to_string(),
            raw_export_requested: true,
            max_staleness_ms: 1,
        },
        &cdc,
        &mut pipeline,
        &mut store,
        &residency,
        &guard,
        &watermarks,
    )
    .expect_err("raw export should be denied");
    assert!(err.message.contains("FEDERATED_RAW_EXPORT_DENY"));
}
