use crate::db::net::interceptor::{intercept_rpc, intercept_rpc_with_pki};
use crate::db::security::authz::{CertIdentity, RpcClass};
use crate::db::security::pki::PkiStore;
use crate::db::security::residency::ResidencyPolicy;
use crate::db::time::watermarks::SafeReadWatermarks;
use crate::db::types::BatchOp;
use crate::db::{
    DbError, cdc, cdc_ack, cdc_checkpoint, cdc_page, cdc_stream_backfill_page, cdc_stream_page,
    close_db, open_db, read_point, read_range, restore_snapshot, snapshot_start, snapshot_status,
    submit_put, txn_abort, txn_begin, txn_commit, txn_prepare,
};
use crate::db::{
    analytics::columnar::ColumnarStore,
    analytics::federation::{FederatedMergeStrategy, FederatedSource},
    analytics::ingest::IngestPipeline,
    analytics::service::{AnalyticsExplain, AnalyticsQueryRequest, AnalyticsQueryResult},
};
use std::path::Path;

pub fn open(path: &Path) -> Result<i64, DbError> {
    open_db(path)
}

pub fn close(handle: i64) -> bool {
    close_db(handle)
}

pub fn put(
    handle: i64,
    namespace: Vec<u8>,
    key: Vec<u8>,
    value: Vec<u8>,
    expected_version: Option<u64>,
) -> Result<u64, DbError> {
    submit_put(handle, namespace, key, value, expected_version)
}

pub fn put_authorized(
    handle: i64,
    namespace: Vec<u8>,
    key: Vec<u8>,
    value: Vec<u8>,
    expected_version: Option<u64>,
    identity: &CertIdentity,
) -> Result<u64, DbError> {
    intercept_rpc(identity, RpcClass::ClientWrite, || {
        submit_put(handle, namespace, key, value, expected_version)
    })
    .map_err(|err| DbError::invalid_argument(format!("unauthorized rpc: {}", err.reason)))?
}

pub fn batch(handle: i64, ops: &[BatchOp]) -> Result<u64, DbError> {
    crate::db::submit_batch(handle, ops)
}

pub fn get(handle: i64, namespace: Vec<u8>, key: Vec<u8>) -> Result<Option<Vec<u8>>, DbError> {
    read_point(handle, namespace, key)
}

pub fn get_authorized(
    handle: i64,
    namespace: Vec<u8>,
    key: Vec<u8>,
    identity: &CertIdentity,
) -> Result<Option<Vec<u8>>, DbError> {
    intercept_rpc(identity, RpcClass::ClientRead, || {
        read_point(handle, namespace, key)
    })
    .map_err(|err| DbError::invalid_argument(format!("unauthorized rpc: {}", err.reason)))?
}

pub fn put_authorized_with_cert(
    handle: i64,
    namespace: Vec<u8>,
    key: Vec<u8>,
    value: Vec<u8>,
    expected_version: Option<u64>,
    identity: &CertIdentity,
    pki: &PkiStore,
    cert_serial: u64,
    now_epoch_s: u64,
) -> Result<u64, DbError> {
    intercept_rpc_with_pki(
        pki,
        cert_serial,
        now_epoch_s,
        identity,
        RpcClass::ClientWrite,
        || submit_put(handle, namespace, key, value, expected_version),
    )
    .map_err(|err| DbError::invalid_argument(format!("unauthorized rpc: {}", err.reason)))?
}

pub fn get_authorized_with_cert(
    handle: i64,
    namespace: Vec<u8>,
    key: Vec<u8>,
    identity: &CertIdentity,
    pki: &PkiStore,
    cert_serial: u64,
    now_epoch_s: u64,
) -> Result<Option<Vec<u8>>, DbError> {
    intercept_rpc_with_pki(
        pki,
        cert_serial,
        now_epoch_s,
        identity,
        RpcClass::ClientRead,
        || read_point(handle, namespace, key),
    )
    .map_err(|err| DbError::invalid_argument(format!("unauthorized rpc: {}", err.reason)))?
}

pub fn scan(
    handle: i64,
    namespace: Vec<u8>,
    start_key: Vec<u8>,
    end_key: Vec<u8>,
    limit: usize,
) -> Result<Vec<(Vec<u8>, Vec<u8>, u64)>, DbError> {
    read_range(handle, namespace, start_key, end_key, limit)
}

pub fn poll_cdc(
    handle: i64,
    after_commit_seq: u64,
    limit: usize,
    shard_filter: Option<Vec<u8>>,
) -> Result<cdc::CdcPage, DbError> {
    cdc_page(handle, after_commit_seq, limit, shard_filter)
}

pub fn poll_cdc_stream(
    handle: i64,
    stream: String,
    limit: usize,
    shard_filter: Option<Vec<u8>>,
) -> Result<cdc::CdcPage, DbError> {
    cdc_stream_page(handle, stream, limit, shard_filter)
}

pub fn poll_cdc_backfill_then_tail(
    handle: i64,
    stream: String,
    backfill_start_inclusive: u64,
    limit: usize,
    shard_filter: Option<Vec<u8>>,
) -> Result<cdc::CdcPage, DbError> {
    cdc_stream_backfill_page(
        handle,
        stream,
        backfill_start_inclusive,
        limit,
        shard_filter,
    )
}

pub fn poll_cdc_for_sink(
    handle: i64,
    after_commit_seq: u64,
    limit: usize,
    shard_filter: Option<Vec<u8>>,
    sink_region: &str,
    policy: &ResidencyPolicy,
) -> Result<cdc::CdcPage, DbError> {
    let page = cdc_page(handle, after_commit_seq, limit, shard_filter)?;
    for event in &page.events {
        policy
            .authorize_egress(&event.shard, sink_region)
            .map_err(|err| DbError::invalid_argument(err.fail_closed_message()))?;
    }
    Ok(page)
}

pub fn poll_cdc_authorized(
    handle: i64,
    after_commit_seq: u64,
    limit: usize,
    shard_filter: Option<Vec<u8>>,
    identity: &CertIdentity,
) -> Result<cdc::CdcPage, DbError> {
    intercept_rpc(identity, RpcClass::ClientRead, || {
        cdc_page(handle, after_commit_seq, limit, shard_filter)
    })
    .map_err(|err| DbError::invalid_argument(format!("unauthorized rpc: {}", err.reason)))?
}

pub fn ack_cdc(handle: i64, stream: String, commit_seq: u64) -> Result<u64, DbError> {
    cdc_ack(handle, stream, commit_seq)
}

pub fn ack_cdc_authorized(
    handle: i64,
    stream: String,
    commit_seq: u64,
    identity: &CertIdentity,
) -> Result<u64, DbError> {
    intercept_rpc(identity, RpcClass::ClientWrite, || {
        cdc_ack(handle, stream, commit_seq)
    })
    .map_err(|err| DbError::invalid_argument(format!("unauthorized rpc: {}", err.reason)))?
}

pub fn get_cdc_checkpoint(handle: i64, stream: String) -> Result<Option<u64>, DbError> {
    cdc_checkpoint(handle, stream)
}

pub fn begin_txn(handle: i64) -> Result<u64, DbError> {
    txn_begin(handle)
}

pub fn prepare_txn(handle: i64, txn: u64) -> Result<(), DbError> {
    txn_prepare(handle, txn)
}

pub fn commit_txn(handle: i64, txn: u64) -> Result<(), DbError> {
    txn_commit(handle, txn)
}

pub fn abort_txn(handle: i64, txn: u64) -> Result<(), DbError> {
    txn_abort(handle, txn)
}

pub fn start_snapshot(handle: i64) -> Result<u64, DbError> {
    snapshot_start(handle)
}

pub fn get_snapshot_status(handle: i64, snapshot: u64) -> Result<u8, DbError> {
    snapshot_status(handle, snapshot)
}

pub fn restore(handle: i64, snapshot: u64) -> Result<(), DbError> {
    restore_snapshot(handle, snapshot)
}

pub fn analytics_query(
    request: &AnalyticsQueryRequest,
    cdc: &crate::db::cdc::CdcEmitter,
    pipeline: &mut IngestPipeline,
    store: &mut ColumnarStore,
    residency: &ResidencyPolicy,
    guard: &crate::db::analytics::policy::FederatedResidencyGuard,
    watermarks: &SafeReadWatermarks,
) -> Result<AnalyticsQueryResult, DbError> {
    crate::db::analytics::service::ingest_and_query(
        request, cdc, pipeline, store, residency, guard, watermarks,
    )
}

pub fn analytics_explain(
    plan_id: &str,
    sources: &[FederatedSource],
    strategy: FederatedMergeStrategy,
    guard: &crate::db::analytics::policy::FederatedResidencyGuard,
    watermarks: &SafeReadWatermarks,
) -> Result<AnalyticsExplain, DbError> {
    crate::db::analytics::service::explain_federated(plan_id, sources, strategy, guard, watermarks)
}

pub fn analytics_execute_federated(
    plan_id: &str,
    sources: &[FederatedSource],
    strategy: FederatedMergeStrategy,
    guard: &crate::db::analytics::policy::FederatedResidencyGuard,
) -> Result<crate::db::analytics::operators::Batch, DbError> {
    crate::db::analytics::service::execute_federated(plan_id, sources, strategy, guard)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::security::authz::MembershipRole;
    use crate::db::security::pki::PkiStore;
    use crate::db::security::residency::{ResidencyErrorToken, ResidencyPolicy, ResidencyRule};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn temp_dir() -> PathBuf {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        let base = std::env::temp_dir().join(format!(
            "wrela_db_api_test_{}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("unix epoch")
                .as_nanos(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&base).expect("create temp dir");
        base
    }

    fn id(role: MembershipRole) -> CertIdentity {
        CertIdentity {
            cluster_id: "cluster-a".to_string(),
            node_id: "node-1".to_string(),
            role,
        }
    }

    #[test]
    fn authorized_put_denies_before_db_lookup_when_role_disallowed() {
        let identity = id(MembershipRole::Learner);
        let err = put_authorized(
            -1,
            b"core".to_vec(),
            b"k".to_vec(),
            b"v".to_vec(),
            None,
            &identity,
        )
        .expect_err("must deny write for learner");
        assert!(err.message.contains("unauthorized rpc"));
    }

    #[test]
    fn authorized_api_allows_role_and_round_trips() {
        let dir = temp_dir();
        let handle = open(&dir).expect("open");
        let identity = id(MembershipRole::Gateway);
        put_authorized(
            handle,
            b"core".to_vec(),
            b"k".to_vec(),
            b"v".to_vec(),
            None,
            &identity,
        )
        .expect("authorized put");
        let value = get_authorized(handle, b"core".to_vec(), b"k".to_vec(), &identity)
            .expect("authorized get")
            .expect("value");
        assert_eq!(value, b"v".to_vec());
        assert!(close(handle));
    }

    #[test]
    fn authorized_api_with_cert_denies_revoked_cert() {
        let dir = temp_dir();
        let handle = open(&dir).expect("open");
        let identity = id(MembershipRole::Gateway);
        let mut pki = PkiStore::default();
        let cert = pki.issue_cert("cluster-a".to_string(), "node-1".to_string(), 100, 60);
        assert!(pki.revoke(cert.serial));
        let err = put_authorized_with_cert(
            handle,
            b"core".to_vec(),
            b"k".to_vec(),
            b"v".to_vec(),
            None,
            &identity,
            &pki,
            cert.serial,
            120,
        )
        .expect_err("revoked cert must deny");
        assert!(err.message.contains("unauthorized rpc"));
        assert!(close(handle));
    }

    #[test]
    fn authorized_api_with_cert_allows_valid_cert() {
        let dir = temp_dir();
        let handle = open(&dir).expect("open");
        let identity = id(MembershipRole::Gateway);
        let mut pki = PkiStore::default();
        let cert = pki.issue_cert("cluster-a".to_string(), "node-1".to_string(), 100, 60);
        put_authorized_with_cert(
            handle,
            b"core".to_vec(),
            b"k".to_vec(),
            b"v".to_vec(),
            None,
            &identity,
            &pki,
            cert.serial,
            120,
        )
        .expect("cert-authorized put");
        let value = get_authorized_with_cert(
            handle,
            b"core".to_vec(),
            b"k".to_vec(),
            &identity,
            &pki,
            cert.serial,
            120,
        )
        .expect("cert-authorized get")
        .expect("value");
        assert_eq!(value, b"v".to_vec());
        assert!(close(handle));
    }

    #[test]
    fn poll_cdc_authorized_supports_resume_cursor() {
        let dir = temp_dir();
        let handle = open(&dir).expect("open");
        let identity = id(MembershipRole::Gateway);
        put_authorized(
            handle,
            b"core".to_vec(),
            b"k1".to_vec(),
            b"v1".to_vec(),
            None,
            &identity,
        )
        .expect("put 1");
        put_authorized(
            handle,
            b"core".to_vec(),
            b"k2".to_vec(),
            b"v2".to_vec(),
            None,
            &identity,
        )
        .expect("put 2");

        let first = poll_cdc_authorized(handle, 0, 1, None, &identity).expect("poll first");
        assert_eq!(first.events.len(), 1);
        let second = poll_cdc_authorized(handle, first.next_commit_seq, 8, None, &identity)
            .expect("poll second");
        assert_eq!(second.events.len(), 1);
        assert!(
            second.events[0].commit_seq > first.events[0].commit_seq,
            "cursor must advance"
        );
        assert!(close(handle));
    }

    #[test]
    fn poll_cdc_authorized_denies_non_reader_role() {
        let dir = temp_dir();
        let handle = open(&dir).expect("open");
        let identity = id(MembershipRole::Learner);
        let page = poll_cdc_authorized(handle, 0, 8, None, &identity).expect("learner can read");
        assert!(page.events.is_empty());

        let bad = CertIdentity {
            cluster_id: "cluster-a".to_string(),
            node_id: "node-1".to_string(),
            role: MembershipRole::Voter,
        };
        // Reads are allowed for voters; use malformed identity to assert fail-closed behavior.
        let malformed = CertIdentity {
            cluster_id: "".to_string(),
            node_id: bad.node_id,
            role: bad.role,
        };
        let err = poll_cdc_authorized(handle, 0, 8, None, &malformed).expect_err("must deny");
        assert!(err.message.contains("unauthorized rpc"));
        assert!(close(handle));
    }

    #[test]
    fn cdc_ack_authorized_enforces_monotonic_checkpoint() {
        let dir = temp_dir();
        let handle = open(&dir).expect("open");
        let id = id(MembershipRole::Gateway);
        assert_eq!(
            ack_cdc_authorized(handle, "orders".to_string(), 10, &id).expect("ack"),
            10
        );
        assert_eq!(
            ack_cdc_authorized(handle, "orders".to_string(), 8, &id).expect("stale ack"),
            10
        );
        assert_eq!(
            get_cdc_checkpoint(handle, "orders".to_string()).expect("checkpoint"),
            Some(10)
        );
        assert!(close(handle));
    }

    #[test]
    fn poll_cdc_stream_uses_checkpoint_cursor() {
        let dir = temp_dir();
        let handle = open(&dir).expect("open");
        let id = id(MembershipRole::Gateway);
        put_authorized(
            handle,
            b"core".to_vec(),
            b"k1".to_vec(),
            b"v1".to_vec(),
            None,
            &id,
        )
        .expect("put 1");
        put_authorized(
            handle,
            b"core".to_vec(),
            b"k2".to_vec(),
            b"v2".to_vec(),
            None,
            &id,
        )
        .expect("put 2");

        assert_eq!(ack_cdc(handle, "orders".to_string(), 1).expect("ack"), 1);
        let page = poll_cdc_stream(handle, "orders".to_string(), 8, None).expect("stream page");
        assert_eq!(page.events.len(), 1);
        assert_eq!(page.events[0].commit_seq, 2);
        assert!(close(handle));
    }

    #[test]
    fn poll_cdc_backfill_then_tail_prefers_checkpoint_after_ack() {
        let dir = temp_dir();
        let handle = open(&dir).expect("open");
        let id = id(MembershipRole::Gateway);
        put_authorized(
            handle,
            b"core".to_vec(),
            b"k1".to_vec(),
            b"v1".to_vec(),
            None,
            &id,
        )
        .expect("put 1");
        put_authorized(
            handle,
            b"core".to_vec(),
            b"k2".to_vec(),
            b"v2".to_vec(),
            None,
            &id,
        )
        .expect("put 2");

        let first =
            poll_cdc_backfill_then_tail(handle, "orders".to_string(), 2, 8, None).expect("first");
        assert_eq!(first.events.len(), 1);
        assert_eq!(first.events[0].commit_seq, 2);
        assert_eq!(ack_cdc(handle, "orders".to_string(), 2).expect("ack"), 2);
        let second =
            poll_cdc_backfill_then_tail(handle, "orders".to_string(), 1, 8, None).expect("tail");
        assert!(second.events.is_empty());
        assert!(close(handle));
    }

    #[test]
    fn poll_cdc_for_sink_denies_cross_residency_egress() {
        let dir = temp_dir();
        let handle = open(&dir).expect("open");
        let id = id(MembershipRole::Gateway);
        put_authorized(
            handle,
            b"core".to_vec(),
            b"k1".to_vec(),
            b"v1".to_vec(),
            None,
            &id,
        )
        .expect("put");

        let policy = ResidencyPolicy::with_rules(vec![ResidencyRule {
            shard: b"core".to_vec(),
            allowed_regions: vec!["us".to_string()],
        }]);
        let err = poll_cdc_for_sink(handle, 0, 8, None, "eu", &policy).expect_err("must deny");
        assert_eq!(
            err.message.split(':').next().expect("token prefix"),
            ResidencyErrorToken::EgressDeny.as_str()
        );
        assert!(err.message.contains("sink_region=eu"));
        assert!(close(handle));
    }

    #[test]
    fn poll_cdc_for_sink_denies_when_residency_policy_unsat() {
        let dir = temp_dir();
        let handle = open(&dir).expect("open");
        let id = id(MembershipRole::Gateway);
        put_authorized(
            handle,
            b"core".to_vec(),
            b"k1".to_vec(),
            b"v1".to_vec(),
            None,
            &id,
        )
        .expect("put");

        let policy = ResidencyPolicy::with_rules(vec![ResidencyRule {
            shard: b"aux".to_vec(),
            allowed_regions: vec!["us".to_string()],
        }]);
        let err =
            poll_cdc_for_sink(handle, 0, 8, None, "us", &policy).expect_err("must fail closed");
        assert_eq!(
            err.message.split(':').next().expect("token prefix"),
            ResidencyErrorToken::EgressPolicyUnsat.as_str()
        );
        assert!(err.message.contains("shard=core has no egress rule"));
        assert!(close(handle));
    }

    #[test]
    fn poll_cdc_for_sink_allows_when_policy_matches_region() {
        let dir = temp_dir();
        let handle = open(&dir).expect("open");
        let id = id(MembershipRole::Gateway);
        put_authorized(
            handle,
            b"core".to_vec(),
            b"k1".to_vec(),
            b"v1".to_vec(),
            None,
            &id,
        )
        .expect("put");

        let policy = ResidencyPolicy::with_rules(vec![ResidencyRule {
            shard: b"core".to_vec(),
            allowed_regions: vec!["us".to_string(), "eu".to_string()],
        }]);
        let page = poll_cdc_for_sink(handle, 0, 8, None, "eu", &policy).expect("allowed");
        assert_eq!(page.events.len(), 1);
        assert!(close(handle));
    }

    #[test]
    fn poll_cdc_page_meets_correctness_gate() {
        let dir = temp_dir();
        let handle = open(&dir).expect("open");
        let id = id(MembershipRole::Gateway);
        put_authorized(
            handle,
            b"core".to_vec(),
            b"k1".to_vec(),
            b"v1".to_vec(),
            None,
            &id,
        )
        .expect("put 1");
        put_authorized(
            handle,
            b"core".to_vec(),
            b"k2".to_vec(),
            b"v2".to_vec(),
            None,
            &id,
        )
        .expect("put 2");

        let page = poll_cdc(handle, 0, 8, None).expect("poll");
        crate::db::cdc::evaluate_cdc_correctness_gate(&page, 0).expect("correct page");
        assert!(close(handle));
    }
}
