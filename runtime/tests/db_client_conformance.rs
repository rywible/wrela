use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use wrela_runtime::db::rpc::errors::{RpcStatusCode, map_db_error};
use wrela_runtime::db::rpc::grpc::{GrpcEdgeService, WriteBatchRequest};
use wrela_runtime::db::types::{BatchOp, DbError};
use wrela_runtime::db::{close_db, open_db, read_point};

fn temp_dir() -> PathBuf {
    static NEXT_ID: AtomicU64 = AtomicU64::new(1);
    let base = std::env::temp_dir().join(format!(
        "wrela_db_client_conformance_{}_{}_{}",
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

#[test]
fn conformance_not_leader_redirect_contains_retry_and_leader_hints() {
    let dir = temp_dir();
    let handle = open_db(&dir).expect("open db");
    let mut svc = GrpcEdgeService::new("node-a", "node-b");

    let err = svc
        .write_batch(WriteBatchRequest {
            handle,
            ops: vec![BatchOp::Put {
                namespace: b"core".to_vec(),
                key: b"k1".to_vec(),
                value: b"v1".to_vec(),
                expected_version: None,
            }],
            idempotency_token: Some("tok-nl-1".to_string()),
        })
        .expect_err("must return NOT_LEADER");

    assert_eq!(err.code, RpcStatusCode::NotLeader);
    assert_eq!(err.retry.as_ref().map(|hint| hint.retry_after_ms), Some(25));
    assert_eq!(
        err.leader.as_ref().map(|hint| hint.leader_node_id.as_str()),
        Some("node-b")
    );
    assert!(close_db(handle));
}

#[test]
fn conformance_timeout_ambiguity_replay_is_idempotent() {
    let dir = temp_dir();
    let handle = open_db(&dir).expect("open db");
    let mut svc = GrpcEdgeService::new("node-a", "node-a");

    let req = WriteBatchRequest {
        handle,
        ops: vec![BatchOp::Put {
            namespace: b"core".to_vec(),
            key: b"k2".to_vec(),
            value: b"v2".to_vec(),
            expected_version: None,
        }],
        idempotency_token: Some("tok-timeout-1".to_string()),
    };

    let first = svc.write_batch(req.clone()).expect("initial submit");
    let replay = svc.write_batch(req).expect("retry with same token");
    assert_eq!(first.commit_version, replay.commit_version);
    assert!(!first.idempotent_replay);
    assert!(replay.idempotent_replay);

    let value = read_point(handle, b"core".to_vec(), b"k2".to_vec())
        .expect("read")
        .expect("value should exist once");
    assert_eq!(value, b"v2".to_vec());
    assert!(close_db(handle));
}

#[test]
fn conformance_occ_and_retry_after_mapping_tokens_are_stable() {
    let occ = map_db_error(DbError::occ("expected version mismatch"));
    assert_eq!(occ.code, RpcStatusCode::OccMismatch);

    let retry = map_db_error(DbError::limit("queue saturated; RETRY_AFTER_MS=40"));
    assert_eq!(retry.code, RpcStatusCode::RetryAfter);
    assert_eq!(
        retry.retry.as_ref().map(|hint| hint.retry_after_ms),
        Some(40)
    );
}
