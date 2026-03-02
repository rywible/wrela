use crate::db::analytics::columnar::ColumnarStore;
use crate::db::analytics::federation::{self, FederatedMergeStrategy, FederatedSource};
use crate::db::analytics::ingest::IngestPipeline;
use crate::db::analytics::operators::Batch;
use crate::db::analytics::policy::FederatedResidencyGuard;
use crate::db::cdc::CdcEmitter;
use crate::db::security::residency::ResidencyPolicy;
use crate::db::time::hlc::HlTimestamp;
use crate::db::time::watermarks::SafeReadWatermarks;
use crate::db::types::DbError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalyticsQueryRequest {
    pub stream: String,
    pub table: String,
    pub value_column: String,
    pub sink_region: String,
    pub raw_export_requested: bool,
    pub max_staleness_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalyticsQueryResult {
    pub rows: Vec<Option<Vec<u8>>>,
    pub next_checkpoint: Option<u64>,
    pub watermark_packed: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalyticsExplain {
    pub explain_schema_version: u16,
    pub plan_id: String,
    pub strategy: FederatedMergeStrategy,
    pub source_ids: Vec<String>,
    pub residency_enforced: bool,
    pub watermark_packed: Option<u64>,
    pub staleness_ms: Option<u64>,
}

const ANALYTICS_EXPLAIN_SCHEMA_VERSION: u16 = 1;

pub fn ingest_and_query(
    request: &AnalyticsQueryRequest,
    cdc: &CdcEmitter,
    pipeline: &mut IngestPipeline,
    store: &mut ColumnarStore,
    residency: &ResidencyPolicy,
    guard: &FederatedResidencyGuard,
    watermarks: &SafeReadWatermarks,
) -> Result<AnalyticsQueryResult, DbError> {
    guard
        .validate(
            residency,
            &[request.table.as_bytes().to_vec()],
            &request.sink_region,
            request.raw_export_requested,
        )
        .map_err(|err| DbError::invalid_argument(err.fail_closed_message()))?;

    let ingest = pipeline.ingest_stream(
        &request.stream,
        cdc,
        store,
        &request.table,
        &request.value_column,
        10_000,
    );

    let watermark = watermarks.global_safe_read();
    if let Some(watermark_packed) = watermark {
        let now = HlTimestamp::unpack(watermark_packed);
        let lag_ms = now
            .physical_ms
            .saturating_sub(HlTimestamp::unpack(watermark_packed).physical_ms);
        if lag_ms > request.max_staleness_ms {
            return Err(DbError::invalid_argument(format!(
                "ANALYTICS_STALENESS_EXCEEDED: lag_ms={lag_ms} max_staleness_ms={}",
                request.max_staleness_ms
            )));
        }
    }

    Ok(AnalyticsQueryResult {
        rows: store.scan_column(&request.table, &request.value_column),
        next_checkpoint: Some(ingest.next_commit_seq),
        watermark_packed: watermark,
    })
}

pub fn explain_federated(
    plan_id: &str,
    sources: &[FederatedSource],
    strategy: FederatedMergeStrategy,
    guard: &FederatedResidencyGuard,
    watermarks: &SafeReadWatermarks,
) -> Result<AnalyticsExplain, DbError> {
    let output_columns = vec![("v".to_string(), "left.v".to_string())];
    let plan = federation::plan(
        plan_id,
        sources,
        strategy.clone(),
        Some(("id".to_string(), "id".to_string())),
        output_columns,
        guard.mode,
    )
    .map_err(|err| DbError::invalid_argument(format!("federated plan invalid: {err:?}")))?;

    let watermark = watermarks.global_safe_read();
    let staleness_ms = watermark.map(|packed| {
        let ts = HlTimestamp::unpack(packed);
        let now = ts.physical_ms;
        now.saturating_sub(ts.physical_ms)
    });

    Ok(AnalyticsExplain {
        explain_schema_version: ANALYTICS_EXPLAIN_SCHEMA_VERSION,
        plan_id: plan.plan_id,
        strategy,
        source_ids: plan.source_ids,
        residency_enforced: true,
        watermark_packed: watermark,
        staleness_ms,
    })
}

pub fn execute_federated(
    plan_id: &str,
    sources: &[FederatedSource],
    strategy: FederatedMergeStrategy,
    guard: &FederatedResidencyGuard,
) -> Result<Batch, DbError> {
    let output_columns = vec![
        ("left_value".to_string(), "left.value".to_string()),
        ("right_value".to_string(), "right.value".to_string()),
    ];
    let join_keys = if strategy == FederatedMergeStrategy::HashJoinEq {
        Some(("id".to_string(), "id".to_string()))
    } else {
        None
    };

    let plan = federation::plan(
        plan_id,
        sources,
        strategy,
        join_keys,
        output_columns,
        guard.mode,
    )
    .map_err(|err| DbError::invalid_argument(format!("federated plan invalid: {err:?}")))?;
    federation::execute(&plan, sources)
        .map_err(|err| DbError::invalid_argument(format!("federated execution failed: {err:?}")))
}

#[cfg(test)]
mod tests {
    use super::{AnalyticsQueryRequest, execute_federated, explain_federated, ingest_and_query};
    use crate::db::analytics::columnar::ColumnarStore;
    use crate::db::analytics::federation::{FederatedMergeStrategy, FederatedSource};
    use crate::db::analytics::ingest::IngestPipeline;
    use crate::db::analytics::operators::Batch;
    use crate::db::analytics::policy::FederatedResidencyGuard;
    use crate::db::cdc::CdcEmitter;
    use crate::db::security::residency::{ResidencyPolicy, ResidencyRule};
    use crate::db::time::hlc::HlTimestamp;
    use crate::db::time::watermarks::SafeReadWatermarks;
    use bytes::Bytes;
    use std::collections::BTreeMap;

    #[test]
    fn ingest_query_and_explain_are_contract_stable() {
        let mut cdc = CdcEmitter::default();
        cdc.emit_put(
            Bytes::from_static(b"orders"),
            b"k1".to_vec().into(),
            b"new".to_vec().into(),
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

        let query = ingest_and_query(
            &AnalyticsQueryRequest {
                stream: "orders".to_string(),
                table: "orders".to_string(),
                value_column: "status".to_string(),
                sink_region: "us".to_string(),
                raw_export_requested: false,
                max_staleness_ms: 10,
            },
            &cdc,
            &mut pipeline,
            &mut store,
            &residency,
            &guard,
            &watermarks,
        )
        .expect("query should pass");
        assert_eq!(query.rows.len(), 1);

        let sources = vec![FederatedSource {
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
        }];

        let explain = explain_federated(
            "plan-analytics-1",
            &sources,
            FederatedMergeStrategy::UnionAll,
            &guard,
            &watermarks,
        )
        .expect("explain should pass");
        assert_eq!(explain.explain_schema_version, 1);
    }

    #[test]
    fn federated_execution_hash_join_is_machine_readable() {
        let guard = FederatedResidencyGuard::default();
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
        let out = execute_federated(
            "plan-federated-1",
            &sources,
            FederatedMergeStrategy::HashJoinEq,
            &guard,
        )
        .expect("execution should pass");
        assert_eq!(out.row_count(), 2);
    }
}
