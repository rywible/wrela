use std::collections::BTreeMap;
use wrela_runtime::db::analytics::columnar::ColumnarStore;
use wrela_runtime::db::analytics::ingest::IngestPipeline;
use wrela_runtime::db::analytics::operators::{Batch, hash_join_eq};
use wrela_runtime::db::cdc::CdcEmitter;

#[test]
fn cdc_to_columnar_ingest_is_checkpointed_and_resumable() {
    let mut cdc = CdcEmitter::default();
    cdc.emit_put(b"orders".to_vec(), b"o1".to_vec(), b"new".to_vec(), 1);
    cdc.emit_put(b"orders".to_vec(), b"o2".to_vec(), b"paid".to_vec(), 2);

    let mut store = ColumnarStore::default();
    let mut pipeline = IngestPipeline::default();

    let first = pipeline.ingest_stream("orders", &cdc, &mut store, "orders", "status", 10);
    assert_eq!(first.applied_events, 2);
    assert_eq!(pipeline.checkpoint("orders"), Some(2));

    let second = pipeline.ingest_stream("orders", &cdc, &mut store, "orders", "status", 10);
    assert_eq!(second.applied_events, 0);
    assert_eq!(store.scan_column("orders", "status").len(), 2);
}

#[test]
fn vectorized_ops_support_filter_project_aggregate_and_join() {
    let batch = Batch::new(BTreeMap::from([
        (
            "id".to_string(),
            vec![
                Some(b"1".to_vec()),
                Some(b"2".to_vec()),
                Some(b"3".to_vec()),
            ],
        ),
        (
            "region".to_string(),
            vec![
                Some(b"us".to_vec()),
                Some(b"eu".to_vec()),
                Some(b"us".to_vec()),
            ],
        ),
    ]));

    let filtered = batch.filter_eq("region", b"us");
    let projected = filtered.project(&["id", "region"]);
    let agg = projected.aggregate_count_by("region");
    assert_eq!(projected.row_count(), 2);
    assert_eq!(agg.get(b"us".as_slice()), Some(&2));

    let tiers = Batch::new(BTreeMap::from([
        (
            "id".to_string(),
            vec![Some(b"1".to_vec()), Some(b"3".to_vec())],
        ),
        (
            "tier".to_string(),
            vec![Some(b"silver".to_vec()), Some(b"gold".to_vec())],
        ),
    ]));

    let joined = hash_join_eq(
        &projected,
        "id",
        &tiers,
        "id",
        &[("region", "left.region"), ("tier", "right.tier")],
    );
    assert_eq!(joined.row_count(), 2);
}

#[test]
fn compaction_preserves_column_values_and_stats() {
    let mut store = ColumnarStore::default();
    store.append_segment(
        "orders",
        "status",
        vec![Some(b"new".to_vec()), Some(b"paid".to_vec())],
    );
    store.append_segment("orders", "status", vec![Some(b"shipped".to_vec()), None]);

    let before = store.scan_column("orders", "status");
    store.compact_column("orders", "status");
    let after = store.scan_column("orders", "status");
    let stats = store
        .column_stats("orders", "status")
        .expect("stats should be present");

    assert_eq!(before, after);
    assert_eq!(stats.row_count, 4);
    assert_eq!(stats.null_count, 1);
}
