use crate::db::rpc::errors::{RpcError, map_db_error};
use crate::db::types::BatchOp;
use crate::db::{
    IDEMPOTENCY_NAMESPACE, OwnershipFence, read_point, read_point_with_version, read_range,
    replica_install_sorted_run_chunk, submit_batch_replica_local_with_ownership_fence,
    submit_batch_with_ownership_fence,
};
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WriteBatchRequest {
    pub handle: i64,
    pub ops: Vec<BatchOp>,
    pub idempotency_token: Option<String>,
    pub expected_home_epoch: u64,
    pub expected_shard_map_epoch: u64,
    pub ownership_token: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WriteBatchResponse {
    pub commit_version: u64,
    pub idempotent_replay: bool,
    /// Follower-side WAL write duration in nanoseconds. Populated by followers
    /// on replica_write_batch so the leader can separate disk time from RPC overhead.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub follower_wal_fsync_ns: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PointReadRequest {
    pub handle: i64,
    pub namespace: Vec<u8>,
    pub key: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RangeReadRequest {
    pub handle: i64,
    pub namespace: Vec<u8>,
    pub start_key: Vec<u8>,
    pub end_key: Vec<u8>,
    pub limit: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SortedRunCatchUpChunkRequest {
    pub handle: i64,
    pub term: u64,
    pub chunk_stream_id: u64,
    pub chunk_index: u64,
    pub total_chunks: u64,
    pub payload: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SortedRunCatchUpChunkResponse {
    pub accepted: bool,
    pub next_chunk_index: u64,
    pub rejection_reason: String,
}

pub struct GrpcEdgeService {
    local_node_id: String,
    leader_node_id: String,
    remote_write_transport: Option<RemoteWriteTransport>,
    bound_handle: Option<i64>,
}

/// Encoded idempotency value: request_fingerprint (8 bytes LE) + created_at_epoch_s (8 bytes LE).
const IDEMPOTENCY_VALUE_LEN: usize = 16;

fn encode_idempotency_value(request_fingerprint: u64, created_at_epoch_s: u64) -> Vec<u8> {
    let mut buf = Vec::with_capacity(IDEMPOTENCY_VALUE_LEN);
    buf.extend_from_slice(&request_fingerprint.to_le_bytes());
    buf.extend_from_slice(&created_at_epoch_s.to_le_bytes());
    buf
}

fn decode_idempotency_value(value: &[u8]) -> Option<(u64, u64)> {
    if value.len() != IDEMPOTENCY_VALUE_LEN {
        return None;
    }
    let fp = u64::from_le_bytes(value[0..8].try_into().ok()?);
    let created = u64::from_le_bytes(value[8..16].try_into().ok()?);
    Some((fp, created))
}

pub type RemoteWriteTransport =
    Arc<dyn Fn(&str, WriteBatchRequest) -> Result<WriteBatchResponse, RpcError> + Send + Sync>;

fn write_request_fingerprint(req: &WriteBatchRequest) -> u64 {
    let mut hasher = DefaultHasher::new();
    req.expected_home_epoch.hash(&mut hasher);
    req.expected_shard_map_epoch.hash(&mut hasher);
    req.ownership_token.hash(&mut hasher);
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
            remote_write_transport: None,
            bound_handle: None,
        }
    }

    pub fn set_leader_node_id(&mut self, leader_node_id: impl Into<String>) {
        self.leader_node_id = leader_node_id.into();
    }

    pub fn set_remote_write_transport(&mut self, transport: Option<RemoteWriteTransport>) {
        self.remote_write_transport = transport;
    }

    pub fn bind_handle(&mut self, handle: i64) {
        if handle > 0 {
            self.bound_handle = Some(handle);
        }
    }

    fn effective_handle(&self, requested_handle: i64) -> Result<i64, RpcError> {
        let handle = self.bound_handle.unwrap_or(requested_handle);
        if handle <= 0 {
            return Err(RpcError {
                code: crate::db::rpc::errors::RpcStatusCode::InvalidArgument,
                message: "rpc request requires a valid db handle".to_string(),
                retry: None,
                leader: None,
            });
        }
        Ok(handle)
    }

    /// Validate and resolve the DB handle for a replica write batch.
    /// Only needs `&self` (read access) so concurrent replica RPCs can proceed in parallel.
    pub fn resolve_replica_handle(&self, req: &WriteBatchRequest) -> Result<i64, RpcError> {
        validate_write_batch_request(req)?;
        self.effective_handle(req.handle)
    }

    pub fn forward_write_batch(
        &mut self,
        target_leader_node_id: &str,
        req: WriteBatchRequest,
    ) -> Result<WriteBatchResponse, RpcError> {
        if target_leader_node_id == self.local_node_id {
            let prior_leader = self.leader_node_id.clone();
            self.leader_node_id = self.local_node_id.clone();
            let result = self.write_batch(req);
            self.leader_node_id = prior_leader;
            return result;
        }

        if let Some(transport) = self.remote_write_transport.clone() {
            return transport(target_leader_node_id, req);
        }

        Err(RpcError {
            code: crate::db::rpc::errors::RpcStatusCode::Unavailable,
            message: format!(
                "FORWARD_UNAVAILABLE: no remote transport registered for leader {}",
                target_leader_node_id
            ),
            retry: Some(crate::db::rpc::errors::RetryHint { retry_after_ms: 25 }),
            leader: Some(crate::db::rpc::errors::LeaderHint {
                leader_node_id: target_leader_node_id.to_string(),
            }),
        })
    }

    pub fn write_batch(&mut self, req: WriteBatchRequest) -> Result<WriteBatchResponse, RpcError> {
        self.apply_write_batch(req, true)
    }

    pub fn write_batch_replica(
        &mut self,
        req: WriteBatchRequest,
    ) -> Result<WriteBatchResponse, RpcError> {
        self.apply_write_batch(req, false)
    }

    pub fn point_read(&self, req: PointReadRequest) -> Result<Option<Vec<u8>>, RpcError> {
        let handle = self.effective_handle(req.handle)?;
        read_point(handle, req.namespace, req.key).map_err(map_db_error)
    }

    pub fn range_read(
        &self,
        req: RangeReadRequest,
    ) -> Result<Vec<(Vec<u8>, Vec<u8>, u64)>, RpcError> {
        let handle = self.effective_handle(req.handle)?;
        read_range(handle, req.namespace, req.start_key, req.end_key, req.limit)
            .map_err(map_db_error)
    }

    pub fn replica_install_sorted_run_chunk(
        &mut self,
        req: SortedRunCatchUpChunkRequest,
    ) -> Result<SortedRunCatchUpChunkResponse, RpcError> {
        let handle = self.effective_handle(req.handle)?;
        let status = replica_install_sorted_run_chunk(
            handle,
            req.term,
            req.chunk_stream_id,
            req.chunk_index,
            req.total_chunks,
            req.payload,
        )
        .map_err(map_db_error)?;
        Ok(SortedRunCatchUpChunkResponse {
            accepted: status.accepted,
            next_chunk_index: status.next_chunk_index,
            rejection_reason: status.rejection_reason.unwrap_or_default(),
        })
    }
}

impl GrpcEdgeService {
    fn apply_write_batch(
        &mut self,
        req: WriteBatchRequest,
        enforce_leader: bool,
    ) -> Result<WriteBatchResponse, RpcError> {
        validate_write_batch_request(&req)?;
        if enforce_leader && self.local_node_id != self.leader_node_id {
            return Err(RpcError::not_leader(self.leader_node_id.clone()));
        }
        let handle = self.effective_handle(req.handle)?;
        let ownership_fence = OwnershipFence {
            expected_home_epoch: req.expected_home_epoch,
            expected_shard_map_epoch: req.expected_shard_map_epoch,
            ownership_token: req.ownership_token.clone(),
        };
        // Replica writes skip idempotency handling: the leader already persisted
        // the idempotency record and the follower must apply the batch as-is to
        // avoid injecting a cross-namespace op that triggers MIXED_SHARD_BATCH.
        if !enforce_leader {
            let commit_version =
                submit_batch_replica_local_with_ownership_fence(handle, &req.ops, ownership_fence)
                    .map_err(map_db_error)?;
            return Ok(WriteBatchResponse {
                commit_version,
                idempotent_replay: false,
                follower_wal_fsync_ns: None,
            });
        }
        // Idempotency: check replicated store (survives leader failover).
        if let Some(token) = req.idempotency_token.as_ref() {
            let request_fingerprint = write_request_fingerprint(&req);
            let token_bytes = token.as_bytes().to_vec();
            let existing = read_point_with_version(
                handle,
                IDEMPOTENCY_NAMESPACE.to_vec(),
                token_bytes.clone(),
            )
            .map_err(map_db_error)?;
            if let Some((commit_version, value)) = existing {
                if let Some((stored_fp, _stored_created)) = decode_idempotency_value(&value) {
                    if stored_fp != request_fingerprint {
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
                        commit_version,
                        idempotent_replay: true,
                        follower_wal_fsync_ns: None,
                    });
                }
            }
            let now_s = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let idempotency_value = encode_idempotency_value(request_fingerprint, now_s);
            let mut ops_to_submit = req.ops.clone();
            // Leader always adds the put so it gets replicated. Follower receives batch from leader
            // which already has the put; in single-node or tests we may receive replica write
            // without the put, so add it if not already present (last op is idempotency put for this token).
            let already_has_put = ops_to_submit
                .last()
                .and_then(|op| {
                    if let BatchOp::Put { namespace, key, .. } = op {
                        (namespace.as_ref() == IDEMPOTENCY_NAMESPACE
                            && key.as_ref() == token_bytes.as_slice())
                        .then_some(())
                    } else {
                        None
                    }
                })
                .is_some();
            if !already_has_put {
                ops_to_submit.push(BatchOp::Put {
                    namespace: Bytes::from_static(IDEMPOTENCY_NAMESPACE),
                    key: Bytes::from(token_bytes),
                    value: Bytes::from(idempotency_value),
                    expected_version: None,
                });
            }
            let commit_version = if enforce_leader {
                submit_batch_with_ownership_fence(handle, &ops_to_submit, ownership_fence)
            } else {
                submit_batch_replica_local_with_ownership_fence(
                    handle,
                    &ops_to_submit,
                    ownership_fence,
                )
            }
            .map_err(map_db_error)?;
            return Ok(WriteBatchResponse {
                commit_version,
                idempotent_replay: false,
                follower_wal_fsync_ns: None,
            });
        }
        let commit_version = if enforce_leader {
            submit_batch_with_ownership_fence(handle, &req.ops, ownership_fence)
        } else {
            submit_batch_replica_local_with_ownership_fence(handle, &req.ops, ownership_fence)
        }
        .map_err(map_db_error)?;
        Ok(WriteBatchResponse {
            commit_version,
            idempotent_replay: false,
            follower_wal_fsync_ns: None,
        })
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
    if req.expected_home_epoch == 0 {
        return Err(RpcError {
            code: crate::db::rpc::errors::RpcStatusCode::InvalidArgument,
            message: "expected_home_epoch must be greater than zero".to_string(),
            retry: None,
            leader: None,
        });
    }
    if req.expected_shard_map_epoch == 0 {
        return Err(RpcError {
            code: crate::db::rpc::errors::RpcStatusCode::InvalidArgument,
            message: "expected_shard_map_epoch must be greater than zero".to_string(),
            retry: None,
            leader: None,
        });
    }
    if req.ownership_token.trim().is_empty() {
        return Err(RpcError {
            code: crate::db::rpc::errors::RpcStatusCode::InvalidArgument,
            message: "ownership_token must not be blank".to_string(),
            retry: None,
            leader: None,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{close_db, open_db, read_point, resolve_owner};
    use bytes::Bytes;
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

    fn ownership_fence_for(handle: i64, key: &[u8]) -> (u64, u64, String) {
        let owner = resolve_owner(handle, b"core".to_vec(), key.to_vec()).expect("resolve owner");
        (
            owner.home_epoch,
            owner.shard_map_epoch,
            owner.ownership_token,
        )
    }

    #[test]
    fn write_batch_returns_not_leader_with_hint() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        let mut svc = GrpcEdgeService::new("node-a", "node-b");
        let (expected_home_epoch, expected_shard_map_epoch, ownership_token) =
            ownership_fence_for(handle, b"k");
        let err = svc
            .write_batch(WriteBatchRequest {
                handle,
                ops: vec![BatchOp::Put {
                    namespace: Bytes::from_static(b"core"),
                    key: Bytes::from_static(b"k"),
                    value: Bytes::from_static(b"v"),
                    expected_version: None,
                }],
                idempotency_token: None,
                expected_home_epoch,
                expected_shard_map_epoch,
                ownership_token,
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
        let (expected_home_epoch, expected_shard_map_epoch, ownership_token) =
            ownership_fence_for(handle, b"k");
        let req = WriteBatchRequest {
            handle,
            ops: vec![BatchOp::Put {
                namespace: Bytes::from_static(b"core"),
                key: Bytes::from_static(b"k"),
                value: Bytes::from_static(b"v"),
                expected_version: None,
            }],
            idempotency_token: Some("tok-1".to_string()),
            expected_home_epoch,
            expected_shard_map_epoch,
            ownership_token,
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
        let (expected_home_epoch, expected_shard_map_epoch, ownership_token) =
            ownership_fence_for(handle, b"k2");
        let req = WriteBatchRequest {
            handle,
            ops: vec![BatchOp::Put {
                namespace: Bytes::from_static(b"core"),
                key: Bytes::from_static(b"k2"),
                value: Bytes::from_static(b"v2"),
                expected_version: None,
            }],
            idempotency_token: Some("tok-2".to_string()),
            expected_home_epoch,
            expected_shard_map_epoch,
            ownership_token,
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
        let (expected_home_epoch, expected_shard_map_epoch, ownership_token) =
            ownership_fence_for(handle, b"k3");
        let first_req = WriteBatchRequest {
            handle,
            ops: vec![BatchOp::Put {
                namespace: Bytes::from_static(b"core"),
                key: Bytes::from_static(b"k3"),
                value: Bytes::from_static(b"v3"),
                expected_version: None,
            }],
            idempotency_token: Some("tok-3".to_string()),
            expected_home_epoch,
            expected_shard_map_epoch,
            ownership_token: ownership_token.clone(),
        };
        let second_req = WriteBatchRequest {
            handle,
            ops: vec![BatchOp::Put {
                namespace: Bytes::from_static(b"core"),
                key: Bytes::from_static(b"k4"),
                value: Bytes::from_static(b"v4"),
                expected_version: None,
            }],
            idempotency_token: Some("tok-3".to_string()),
            expected_home_epoch,
            expected_shard_map_epoch,
            ownership_token,
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
        let (expected_home_epoch, expected_shard_map_epoch, ownership_token) =
            ownership_fence_for(handle, b"k5");
        let err = svc
            .write_batch(WriteBatchRequest {
                handle,
                ops: vec![BatchOp::Put {
                    namespace: Bytes::from_static(b"core"),
                    key: Bytes::from_static(b"k5"),
                    value: Bytes::from_static(b"v5"),
                    expected_version: None,
                }],
                idempotency_token: Some("   ".to_string()),
                expected_home_epoch,
                expected_shard_map_epoch,
                ownership_token,
            })
            .expect_err("blank token must fail");
        assert!(matches!(
            err.code,
            crate::db::rpc::errors::RpcStatusCode::InvalidArgument
        ));
        assert!(err.message.contains("must not be blank"));
        assert!(close_db(handle));
    }

    #[test]
    fn write_batch_rejects_zero_fence_epochs() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        let mut svc = GrpcEdgeService::new("node-a", "node-a");
        let (_home_epoch, expected_shard_map_epoch, ownership_token) =
            ownership_fence_for(handle, b"k-fence-epoch");
        let err = svc
            .write_batch(WriteBatchRequest {
                handle,
                ops: vec![BatchOp::Put {
                    namespace: Bytes::from_static(b"core"),
                    key: Bytes::from_static(b"k-fence-epoch"),
                    value: Bytes::from_static(b"v-fence-epoch"),
                    expected_version: None,
                }],
                idempotency_token: None,
                expected_home_epoch: 0,
                expected_shard_map_epoch,
                ownership_token,
            })
            .expect_err("zero expected_home_epoch must fail");
        assert!(matches!(
            err.code,
            crate::db::rpc::errors::RpcStatusCode::InvalidArgument
        ));
        assert!(err.message.contains("expected_home_epoch"));
        assert!(close_db(handle));
    }

    #[test]
    fn write_batch_rejects_stale_ownership_fence() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        let mut svc = GrpcEdgeService::new("node-a", "node-a");
        let (expected_home_epoch, expected_shard_map_epoch, ownership_token) =
            ownership_fence_for(handle, b"k-fence-stale");
        let err = svc
            .write_batch(WriteBatchRequest {
                handle,
                ops: vec![BatchOp::Put {
                    namespace: Bytes::from_static(b"core"),
                    key: Bytes::from_static(b"k-fence-stale"),
                    value: Bytes::from_static(b"v-fence-stale"),
                    expected_version: None,
                }],
                idempotency_token: None,
                expected_home_epoch: expected_home_epoch.saturating_add(1),
                expected_shard_map_epoch,
                ownership_token,
            })
            .expect_err("stale ownership fence must fail");
        assert!(matches!(
            err.code,
            crate::db::rpc::errors::RpcStatusCode::InvalidArgument
        ));
        assert!(
            err.message.contains("HOME_EPOCH_FENCE_VIOLATION")
                || err.message.contains("OWNERSHIP_TOKEN_FENCE_VIOLATION")
                || err.message.contains("DIRECTORY_EPOCH_STALE")
        );
        assert!(close_db(handle));
    }

    #[test]
    fn write_batch_replica_rejects_blank_ownership_token() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        let mut svc = GrpcEdgeService::new("node-a", "node-b");
        svc.bind_handle(handle);
        let (expected_home_epoch, expected_shard_map_epoch, _ownership_token) =
            ownership_fence_for(handle, b"k-fence-owner");
        let err = svc
            .write_batch_replica(WriteBatchRequest {
                handle: 0,
                ops: vec![BatchOp::Put {
                    namespace: Bytes::from_static(b"core"),
                    key: Bytes::from_static(b"k-fence-owner"),
                    value: Bytes::from_static(b"v-fence-owner"),
                    expected_version: None,
                }],
                idempotency_token: None,
                expected_home_epoch,
                expected_shard_map_epoch,
                ownership_token: "   ".to_string(),
            })
            .expect_err("blank ownership token must fail");
        assert!(matches!(
            err.code,
            crate::db::rpc::errors::RpcStatusCode::InvalidArgument
        ));
        assert!(err.message.contains("ownership_token"));
        assert!(close_db(handle));
    }

    #[test]
    fn bound_handle_allows_remote_requests_without_shared_handle_ids() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        let mut svc = GrpcEdgeService::new("node-a", "node-a");
        svc.bind_handle(handle);
        let (expected_home_epoch, expected_shard_map_epoch, ownership_token) =
            ownership_fence_for(handle, b"k-bound");

        let first = svc
            .write_batch(WriteBatchRequest {
                handle: 0,
                ops: vec![BatchOp::Put {
                    namespace: Bytes::from_static(b"core"),
                    key: Bytes::from_static(b"k-bound"),
                    value: Bytes::from_static(b"v-bound"),
                    expected_version: None,
                }],
                idempotency_token: Some("tok-bound-1".to_string()),
                expected_home_epoch,
                expected_shard_map_epoch,
                ownership_token: ownership_token.clone(),
            })
            .expect("bound handle write");
        let replay = svc
            .write_batch(WriteBatchRequest {
                handle: 999_999,
                ops: vec![BatchOp::Put {
                    namespace: Bytes::from_static(b"core"),
                    key: Bytes::from_static(b"k-bound"),
                    value: Bytes::from_static(b"v-bound"),
                    expected_version: None,
                }],
                idempotency_token: Some("tok-bound-1".to_string()),
                expected_home_epoch,
                expected_shard_map_epoch,
                ownership_token,
            })
            .expect("bound replay");
        assert_eq!(first.commit_version, replay.commit_version);
        assert!(replay.idempotent_replay);
        let value = read_point(handle, b"core".to_vec(), b"k-bound".to_vec())
            .expect("read")
            .expect("value");
        assert_eq!(value, b"v-bound".to_vec());
        assert!(close_db(handle));
    }

    #[test]
    fn write_batch_replica_applies_even_when_service_is_not_leader() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        let mut svc = GrpcEdgeService::new("node-a", "node-b");
        svc.bind_handle(handle);
        let (expected_home_epoch, expected_shard_map_epoch, ownership_token) =
            ownership_fence_for(handle, b"k-replica");

        let applied = svc
            .write_batch_replica(WriteBatchRequest {
                handle: 0,
                ops: vec![BatchOp::Put {
                    namespace: Bytes::from_static(b"core"),
                    key: Bytes::from_static(b"k-replica"),
                    value: Bytes::from_static(b"v-replica"),
                    expected_version: None,
                }],
                idempotency_token: Some("tok-replica-1".to_string()),
                expected_home_epoch,
                expected_shard_map_epoch,
                ownership_token: ownership_token.clone(),
            })
            .expect("replica write");
        assert!(applied.commit_version > 0);
        let replay = svc
            .write_batch_replica(WriteBatchRequest {
                handle: 12345,
                ops: vec![BatchOp::Put {
                    namespace: Bytes::from_static(b"core"),
                    key: Bytes::from_static(b"k-replica"),
                    value: Bytes::from_static(b"v-replica"),
                    expected_version: None,
                }],
                idempotency_token: Some("tok-replica-1".to_string()),
                expected_home_epoch,
                expected_shard_map_epoch,
                ownership_token,
            })
            .expect("replica replay");
        assert!(
            replay.commit_version >= applied.commit_version,
            "replica replay should produce a new version (idempotency dedup is leader-only)"
        );
        let value = read_point(handle, b"core".to_vec(), b"k-replica".to_vec())
            .expect("read")
            .expect("value");
        assert_eq!(value, b"v-replica".to_vec());
        assert!(close_db(handle));
    }

    #[test]
    fn idempotency_is_replicated_via_db() {
        // Idempotency records are stored in the __idempotency namespace and
        // survive in the DB (replicated with the batch); replay returns same commit_version.
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        let mut svc = GrpcEdgeService::new("node-a", "node-a");
        svc.bind_handle(handle);
        let (expected_home_epoch, expected_shard_map_epoch, ownership_token) =
            ownership_fence_for(handle, b"k-db-idem");
        let req = WriteBatchRequest {
            handle: 0,
            ops: vec![BatchOp::Put {
                namespace: Bytes::from_static(b"core"),
                key: Bytes::from_static(b"k-db-idem"),
                value: Bytes::from_static(b"v-db-idem"),
                expected_version: None,
            }],
            idempotency_token: Some("tok-db-1".to_string()),
            expected_home_epoch,
            expected_shard_map_epoch,
            ownership_token,
        };
        let first = svc.write_batch(req.clone()).expect("first write");
        let second = svc.write_batch(req).expect("replay");
        assert_eq!(first.commit_version, second.commit_version);
        assert!(second.idempotent_replay);
        assert!(close_db(handle));
    }
}
