use crate::db::rpc::errors::{RpcError, map_db_error};
use crate::db::types::BatchOp;
use crate::db::{read_point, read_range, submit_batch};
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

#[derive(Debug, Clone)]
pub struct WriteBatchRequest {
    pub handle: i64,
    pub ops: Vec<BatchOp>,
    pub idempotency_token: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteBatchResponse {
    pub commit_version: u64,
    pub idempotent_replay: bool,
}

#[derive(Debug, Clone)]
pub struct PointReadRequest {
    pub handle: i64,
    pub namespace: Vec<u8>,
    pub key: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct RangeReadRequest {
    pub handle: i64,
    pub namespace: Vec<u8>,
    pub start_key: Vec<u8>,
    pub end_key: Vec<u8>,
    pub limit: usize,
}

#[derive(Debug)]
pub struct GrpcEdgeService {
    local_node_id: String,
    leader_node_id: String,
    idempotency_results: HashMap<String, IdempotencyRecord>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct IdempotencyRecord {
    commit_version: u64,
    request_fingerprint: u64,
}

fn write_request_fingerprint(req: &WriteBatchRequest) -> u64 {
    let mut hasher = DefaultHasher::new();
    req.handle.hash(&mut hasher);
    req.ops.len().hash(&mut hasher);
    for op in &req.ops {
        match op {
            BatchOp::Put {
                namespace,
                key,
                value,
                expected_version,
            } => {
                1u8.hash(&mut hasher);
                namespace.hash(&mut hasher);
                key.hash(&mut hasher);
                value.hash(&mut hasher);
                expected_version.hash(&mut hasher);
            }
            BatchOp::Delete {
                namespace,
                key,
                expected_version,
            } => {
                2u8.hash(&mut hasher);
                namespace.hash(&mut hasher);
                key.hash(&mut hasher);
                expected_version.hash(&mut hasher);
            }
        }
    }
    hasher.finish()
}

impl GrpcEdgeService {
    pub fn new(local_node_id: impl Into<String>, leader_node_id: impl Into<String>) -> Self {
        Self {
            local_node_id: local_node_id.into(),
            leader_node_id: leader_node_id.into(),
            idempotency_results: HashMap::new(),
        }
    }

    pub fn set_leader_node_id(&mut self, leader_node_id: impl Into<String>) {
        self.leader_node_id = leader_node_id.into();
    }

    pub fn write_batch(&mut self, req: WriteBatchRequest) -> Result<WriteBatchResponse, RpcError> {
        validate_write_batch_request(&req)?;
        if self.local_node_id != self.leader_node_id {
            return Err(RpcError::not_leader(self.leader_node_id.clone()));
        }
        let request_fingerprint = write_request_fingerprint(&req);
        if let Some(token) = req.idempotency_token.as_ref()
            && let Some(record) = self.idempotency_results.get(token)
        {
            if record.request_fingerprint != request_fingerprint {
                return Err(RpcError {
                    code: crate::db::rpc::errors::RpcStatusCode::InvalidArgument,
                    message: format!(
                        "IDEMPOTENCY_TOKEN_REUSE_MISMATCH: token={token} reused with different write payload"
                    ),
                    retry: None,
                    leader: None,
                });
            }
            return Ok(WriteBatchResponse {
                commit_version: record.commit_version,
                idempotent_replay: true,
            });
        }
        let commit_version = submit_batch(req.handle, &req.ops).map_err(map_db_error)?;
        if let Some(token) = req.idempotency_token {
            self.idempotency_results.insert(
                token,
                IdempotencyRecord {
                    commit_version,
                    request_fingerprint,
                },
            );
        }
        Ok(WriteBatchResponse {
            commit_version,
            idempotent_replay: false,
        })
    }

    pub fn point_read(&self, req: PointReadRequest) -> Result<Option<Vec<u8>>, RpcError> {
        read_point(req.handle, req.namespace, req.key).map_err(map_db_error)
    }

    pub fn range_read(
        &self,
        req: RangeReadRequest,
    ) -> Result<Vec<(Vec<u8>, Vec<u8>, u64)>, RpcError> {
        read_range(
            req.handle,
            req.namespace,
            req.start_key,
            req.end_key,
            req.limit,
        )
        .map_err(map_db_error)
    }
}

fn validate_write_batch_request(req: &WriteBatchRequest) -> Result<(), RpcError> {
    if req.ops.is_empty() {
        return Err(RpcError {
            code: crate::db::rpc::errors::RpcStatusCode::InvalidArgument,
            message: "write batch requires at least one operation".to_string(),
            retry: None,
            leader: None,
        });
    }
    if let Some(token) = req.idempotency_token.as_ref()
        && token.trim().is_empty()
    {
        return Err(RpcError {
            code: crate::db::rpc::errors::RpcStatusCode::InvalidArgument,
            message: "idempotency token must not be blank".to_string(),
            retry: None,
            leader: None,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{close_db, open_db, read_point};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn temp_dir() -> PathBuf {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        let base = std::env::temp_dir().join(format!(
            "wrela_db_rpc_test_{}_{}_{}",
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
    fn write_batch_returns_not_leader_with_hint() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        let mut svc = GrpcEdgeService::new("node-a", "node-b");
        let err = svc
            .write_batch(WriteBatchRequest {
                handle,
                ops: vec![BatchOp::Put {
                    namespace: b"core".to_vec(),
                    key: b"k".to_vec(),
                    value: b"v".to_vec(),
                    expected_version: None,
                }],
                idempotency_token: None,
            })
            .expect_err("must fail on follower");
        assert!(matches!(
            err.code,
            crate::db::rpc::errors::RpcStatusCode::NotLeader
        ));
        assert_eq!(
            err.leader.as_ref().map(|hint| hint.leader_node_id.as_str()),
            Some("node-b")
        );
        assert!(close_db(handle));
    }

    #[test]
    fn write_batch_idempotency_token_replays_same_commit_without_duplicate_apply() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        let mut svc = GrpcEdgeService::new("node-a", "node-a");
        let req = WriteBatchRequest {
            handle,
            ops: vec![BatchOp::Put {
                namespace: b"core".to_vec(),
                key: b"k".to_vec(),
                value: b"v".to_vec(),
                expected_version: None,
            }],
            idempotency_token: Some("tok-1".to_string()),
        };
        let first = svc.write_batch(req.clone()).expect("first write");
        let second = svc.write_batch(req).expect("replay write");
        assert_eq!(first.commit_version, second.commit_version);
        assert!(!first.idempotent_replay);
        assert!(second.idempotent_replay);
        let value = read_point(handle, b"core".to_vec(), b"k".to_vec())
            .expect("read")
            .expect("value");
        assert_eq!(value, b"v".to_vec());
        assert!(close_db(handle));
    }

    #[test]
    fn write_retry_after_leader_change_succeeds_with_same_token() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        let mut svc = GrpcEdgeService::new("node-a", "node-b");
        let req = WriteBatchRequest {
            handle,
            ops: vec![BatchOp::Put {
                namespace: b"core".to_vec(),
                key: b"k2".to_vec(),
                value: b"v2".to_vec(),
                expected_version: None,
            }],
            idempotency_token: Some("tok-2".to_string()),
        };
        let err = svc
            .write_batch(req.clone())
            .expect_err("first write must hit not leader");
        assert!(matches!(
            err.code,
            crate::db::rpc::errors::RpcStatusCode::NotLeader
        ));
        svc.set_leader_node_id("node-a");
        let applied = svc.write_batch(req.clone()).expect("leader write");
        let replay = svc.write_batch(req).expect("idempotent replay");
        assert!(!applied.idempotent_replay, "redirect must not cache token");
        assert_eq!(applied.commit_version, replay.commit_version);
        assert!(replay.idempotent_replay);
        assert!(close_db(handle));
    }

    #[test]
    fn write_batch_reused_token_with_different_payload_is_rejected() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        let mut svc = GrpcEdgeService::new("node-a", "node-a");
        let first_req = WriteBatchRequest {
            handle,
            ops: vec![BatchOp::Put {
                namespace: b"core".to_vec(),
                key: b"k3".to_vec(),
                value: b"v3".to_vec(),
                expected_version: None,
            }],
            idempotency_token: Some("tok-3".to_string()),
        };
        let second_req = WriteBatchRequest {
            handle,
            ops: vec![BatchOp::Put {
                namespace: b"core".to_vec(),
                key: b"k4".to_vec(),
                value: b"v4".to_vec(),
                expected_version: None,
            }],
            idempotency_token: Some("tok-3".to_string()),
        };

        let first = svc.write_batch(first_req).expect("first write");
        assert!(!first.idempotent_replay);

        let err = svc
            .write_batch(second_req)
            .expect_err("mismatched token payload must fail");
        assert!(matches!(
            err.code,
            crate::db::rpc::errors::RpcStatusCode::InvalidArgument
        ));
        assert!(err.message.contains("IDEMPOTENCY_TOKEN_REUSE_MISMATCH"));

        let original = read_point(handle, b"core".to_vec(), b"k3".to_vec())
            .expect("read")
            .expect("value");
        assert_eq!(original, b"v3".to_vec());
        let replayed = read_point(handle, b"core".to_vec(), b"k4".to_vec()).expect("read");
        assert!(replayed.is_none(), "second write must not apply");
        assert!(close_db(handle));
    }

    #[test]
    fn write_batch_rejects_blank_idempotency_token() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        let mut svc = GrpcEdgeService::new("node-a", "node-a");
        let err = svc
            .write_batch(WriteBatchRequest {
                handle,
                ops: vec![BatchOp::Put {
                    namespace: b"core".to_vec(),
                    key: b"k5".to_vec(),
                    value: b"v5".to_vec(),
                    expected_version: None,
                }],
                idempotency_token: Some("   ".to_string()),
            })
            .expect_err("blank token must fail");
        assert!(matches!(
            err.code,
            crate::db::rpc::errors::RpcStatusCode::InvalidArgument
        ));
        assert!(err.message.contains("must not be blank"));
        assert!(close_db(handle));
    }
}
