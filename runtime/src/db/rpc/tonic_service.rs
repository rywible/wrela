//! Tonic gRPC service implementation for WrelaDb.
//!
//! Wraps GrpcEdgeService and converts between protobuf and internal types.

pub mod wrpc {
    tonic::include_proto!("wrpc");
}

use crate::db::rpc::errors::{map_db_error, rpc_error_to_status};
use crate::db::rpc::grpc::{
    GrpcEdgeService, PointReadRequest, SortedRunCatchUpChunkRequest, SortedRunCatchUpChunkResponse,
    WriteBatchRequest, WriteBatchResponse,
};
use crate::db::types::BatchOp;
use bytes::Bytes;
use std::pin::Pin;
use std::sync::{Arc, RwLock};
use std::time::Instant;
use tokio_stream::{Stream, StreamExt};
use tonic::{Request, Response, Status, Streaming};
use wrpc::wrela_db_server::WrelaDb;

/// Service implementation that wraps GrpcEdgeService.
///
/// Read RPCs (point_read, range_read) acquire only a shared read lock, allowing
/// concurrent reads. Leader write RPCs (write_batch, replica_install_sorted_run_chunk)
/// acquire the exclusive write lock. Replica write RPCs (replica_write_batch) use
/// a shared read lock for handle resolution and then submit outside any lock,
/// allowing concurrent follower writes to batch at the WAL group commit layer.
pub struct WrelaDbServiceImpl {
    inner: Arc<RwLock<GrpcEdgeService>>,
}

impl WrelaDbServiceImpl {
    pub fn new(inner: Arc<RwLock<GrpcEdgeService>>) -> Self {
        Self { inner }
    }
}

#[tonic::async_trait]
impl WrelaDb for WrelaDbServiceImpl {
    async fn write_batch(
        &self,
        request: Request<wrpc::WriteBatchRequest>,
    ) -> Result<Response<wrpc::WriteBatchResponse>, Status> {
        let req = proto_to_write_batch_request(request.into_inner());
        let inner = self.inner.clone();
        let resp = tokio::task::spawn_blocking(move || {
            let mut svc = inner
                .write()
                .map_err(|_| Status::internal("service write lock poisoned"))?;
            svc.write_batch(req).map_err(rpc_error_to_status)
        })
        .await
        .map_err(|e| Status::internal(e.to_string()))??;
        Ok(Response::new(write_batch_response_to_proto(resp)))
    }

    async fn replica_write_batch(
        &self,
        request: Request<wrpc::WriteBatchRequest>,
    ) -> Result<Response<wrpc::WriteBatchResponse>, Status> {
        let proto = request.into_inner();
        let wal_payload = proto.wal_payload.clone();
        let ownership_fence = crate::db::OwnershipFence {
            expected_home_epoch: proto.expected_home_epoch,
            expected_shard_map_epoch: proto.expected_shard_map_epoch,
            ownership_token: proto.ownership_token.clone(),
        };
        let req = proto_to_write_batch_request(proto);
        let inner = self.inner.clone();
        let resp = tokio::task::spawn_blocking(move || {
            // Use a read lock to resolve the handle — allows concurrent replica RPCs.
            let handle = {
                let svc = inner
                    .read()
                    .map_err(|_| Status::internal("service read lock poisoned"))?;
                svc.resolve_replica_handle(&req)
                    .map_err(rpc_error_to_status)?
            };
            let write_started = Instant::now();
            let commit_version = if let Some(wal_bytes) = wal_payload {
                // Fast path: write pre-encoded WAL bytes directly and apply to
                // memtable, bypassing the writer-lane queue and WAL re-encoding.
                crate::db::submit_replica_wal_direct_with_ownership_fence(
                    handle,
                    &wal_bytes,
                    &req.ops,
                    ownership_fence.clone(),
                )
                .map_err(|e| rpc_error_to_status(map_db_error(e)))?
            } else {
                // Fallback: route through the writer lane (used when leader does
                // not attach wal_payload, e.g. inline replication path).
                crate::db::submit_batch_replica_local_with_ownership_fence(
                    handle,
                    &req.ops,
                    ownership_fence,
                )
                .map_err(|e| rpc_error_to_status(map_db_error(e)))?
            };
            let follower_wal_fsync_ns =
                write_started.elapsed().as_nanos().min(u64::MAX as u128) as u64;
            Ok::<_, Status>(WriteBatchResponse {
                commit_version,
                idempotent_replay: false,
                follower_wal_fsync_ns: Some(follower_wal_fsync_ns),
            })
        })
        .await
        .map_err(|e| Status::internal(e.to_string()))??;
        Ok(Response::new(write_batch_response_to_proto(resp)))
    }

    async fn point_read(
        &self,
        request: Request<wrpc::PointReadRequest>,
    ) -> Result<Response<wrpc::PointReadResponse>, Status> {
        let req = proto_to_point_read_request(request.into_inner());
        let inner = self.inner.clone();
        let resp = tokio::task::spawn_blocking(move || {
            let svc = inner
                .read()
                .map_err(|_| Status::internal("service read lock poisoned"))?;
            svc.point_read(req).map_err(rpc_error_to_status)
        })
        .await
        .map_err(|e| Status::internal(e.to_string()))??;
        Ok(Response::new(wrpc::PointReadResponse {
            value: resp.map(Bytes::from),
        }))
    }

    async fn range_read(
        &self,
        request: Request<wrpc::RangeReadRequest>,
    ) -> Result<Response<wrpc::RangeReadResponse>, Status> {
        let req = proto_to_range_read_request(request.into_inner());
        let inner = self.inner.clone();
        let rows = tokio::task::spawn_blocking(move || {
            let svc = inner
                .read()
                .map_err(|_| Status::internal("service read lock poisoned"))?;
            svc.range_read(req).map_err(rpc_error_to_status)
        })
        .await
        .map_err(|e| Status::internal(e.to_string()))??;
        Ok(Response::new(wrpc::RangeReadResponse {
            rows: rows
                .into_iter()
                .map(|(key, value, version)| wrpc::RangeReadRow {
                    key: Bytes::from(key),
                    value: Bytes::from(value),
                    version,
                })
                .collect(),
        }))
    }

    async fn replica_install_sorted_run_chunk(
        &self,
        request: Request<wrpc::SortedRunCatchUpChunkRequest>,
    ) -> Result<Response<wrpc::SortedRunCatchUpChunkResponse>, Status> {
        let req = proto_to_sorted_run_chunk_request(request.into_inner());
        let inner = self.inner.clone();
        let resp = tokio::task::spawn_blocking(move || {
            let mut svc = inner
                .write()
                .map_err(|_| Status::internal("service write lock poisoned"))?;
            svc.replica_install_sorted_run_chunk(req)
                .map_err(rpc_error_to_status)
        })
        .await
        .map_err(|e| Status::internal(e.to_string()))??;
        Ok(Response::new(sorted_run_chunk_response_to_proto(resp)))
    }

    type ReplicateStreamStream =
        Pin<Box<dyn Stream<Item = Result<wrpc::ReplicationStreamAck, Status>> + Send>>;

    async fn replicate_stream(
        &self,
        request: Request<Streaming<wrpc::ReplicationStreamBatch>>,
    ) -> Result<Response<Self::ReplicateStreamStream>, Status> {
        let inner = self.inner.clone();

        // Resolve and validate each batch using the same contract as unary replica writes.
        let mut inbound = request.into_inner();
        let (tx, rx) = tokio::sync::mpsc::channel::<Result<wrpc::ReplicationStreamAck, Status>>(64);

        tokio::spawn(async move {
            // Resolve handle lazily on first batch.
            let mut resolved_handle: Option<i64> = None;

            while let Some(result) = inbound.next().await {
                let batch = match result {
                    Ok(b) => b,
                    Err(e) => {
                        let _ = tx
                            .send(Err(Status::internal(format!("stream recv error: {e}"))))
                            .await;
                        break;
                    }
                };

                let sequence = batch.sequence;
                let inner = inner.clone();
                let tx = tx.clone();

                let wal_payload = batch.wal_payload.clone();
                let ops: Vec<BatchOp> = batch
                    .ops
                    .into_iter()
                    .filter_map(|op| op.op.map(proto_batch_op_to_internal))
                    .collect();
                let validation_req = WriteBatchRequest {
                    handle: batch.handle,
                    ops: ops.clone(),
                    idempotency_token: batch.idempotency_token.filter(|s| !s.is_empty()),
                    expected_home_epoch: batch.expected_home_epoch,
                    expected_shard_map_epoch: batch.expected_shard_map_epoch,
                    ownership_token: batch.ownership_token,
                };
                let ownership_fence = crate::db::OwnershipFence {
                    expected_home_epoch: validation_req.expected_home_epoch,
                    expected_shard_map_epoch: validation_req.expected_shard_map_epoch,
                    ownership_token: validation_req.ownership_token.clone(),
                };
                // Validate + resolve handle before spawning the blocking write task.
                let handle_result = {
                    let inner_ref = inner.clone();
                    match inner_ref.read() {
                        Ok(svc) => svc
                            .resolve_replica_handle(&validation_req)
                            .map_err(rpc_error_to_status),
                        Err(_) => Err(Status::internal("service read lock poisoned")),
                    }
                };
                let handle = match handle_result {
                    Ok(h) => {
                        if let Some(resolved) = resolved_handle {
                            if resolved != h {
                                let _ = tx
                                    .send(Ok(wrpc::ReplicationStreamAck {
                                        sequence,
                                        commit_version: 0,
                                        follower_wal_fsync_ns: None,
                                        error: format!(
                                            "handle resolution mismatch for stream: expected {resolved}, got {h}"
                                        ),
                                    }))
                                    .await;
                                break;
                            }
                        } else {
                            resolved_handle = Some(h);
                        }
                        h
                    }
                    Err(e) => {
                        let _ = tx
                            .send(Ok(wrpc::ReplicationStreamAck {
                                sequence,
                                commit_version: 0,
                                follower_wal_fsync_ns: None,
                                error: format!("handle resolution failed: {e}"),
                            }))
                            .await;
                        break;
                    }
                };

                // Process each batch in spawn_blocking (same as unary path).
                tokio::task::spawn_blocking(move || {
                    let write_started = Instant::now();
                    let commit_result = if !wal_payload.is_empty() {
                        crate::db::submit_replica_wal_direct_with_ownership_fence(
                            handle,
                            &wal_payload,
                            &ops,
                            ownership_fence.clone(),
                        )
                        .map_err(|e| format!("{e}"))
                    } else {
                        crate::db::submit_batch_replica_local_with_ownership_fence(
                            handle,
                            &ops,
                            ownership_fence,
                        )
                        .map_err(|e| format!("{e}"))
                    };
                    let fsync_ns = write_started.elapsed().as_nanos().min(u64::MAX as u128) as u64;
                    let ack = match commit_result {
                        Ok(commit_version) => wrpc::ReplicationStreamAck {
                            sequence,
                            commit_version,
                            follower_wal_fsync_ns: Some(fsync_ns),
                            error: String::new(),
                        },
                        Err(err) => wrpc::ReplicationStreamAck {
                            sequence,
                            commit_version: 0,
                            follower_wal_fsync_ns: Some(fsync_ns),
                            error: err,
                        },
                    };
                    // Best-effort send; if the channel is closed the stream is dead.
                    let _ = tx.blocking_send(Ok(ack));
                });
            }
        });

        let output = tokio_stream::wrappers::ReceiverStream::new(rx);
        Ok(Response::new(Box::pin(output)))
    }
}

fn proto_to_write_batch_request(p: wrpc::WriteBatchRequest) -> WriteBatchRequest {
    WriteBatchRequest {
        handle: p.handle,
        ops: p
            .ops
            .into_iter()
            .filter_map(|op| op.op.map(proto_batch_op_to_internal))
            .collect(),
        idempotency_token: p.idempotency_token.filter(|s| !s.is_empty()),
        expected_home_epoch: p.expected_home_epoch,
        expected_shard_map_epoch: p.expected_shard_map_epoch,
        ownership_token: p.ownership_token,
    }
}

fn proto_batch_op_to_internal(op: wrpc::batch_op::Op) -> BatchOp {
    match op {
        wrpc::batch_op::Op::Put(put) => BatchOp::Put {
            // Fields are already bytes::Bytes in the generated code.
            namespace: put.namespace,
            key: put.key,
            value: put.value,
            expected_version: put.expected_version,
        },
        wrpc::batch_op::Op::Delete(del) => BatchOp::Delete {
            namespace: del.namespace,
            key: del.key,
            expected_version: del.expected_version,
        },
    }
}

fn write_batch_response_to_proto(r: WriteBatchResponse) -> wrpc::WriteBatchResponse {
    wrpc::WriteBatchResponse {
        commit_version: r.commit_version,
        idempotent_replay: r.idempotent_replay,
        follower_wal_fsync_ns: r.follower_wal_fsync_ns,
    }
}

fn proto_to_point_read_request(p: wrpc::PointReadRequest) -> PointReadRequest {
    PointReadRequest {
        handle: p.handle,
        namespace: p.namespace.into(),
        key: p.key.into(),
    }
}

fn proto_to_range_read_request(
    p: wrpc::RangeReadRequest,
) -> crate::db::rpc::grpc::RangeReadRequest {
    crate::db::rpc::grpc::RangeReadRequest {
        handle: p.handle,
        namespace: p.namespace.into(),
        start_key: p.start_key.into(),
        end_key: p.end_key.into(),
        limit: p.limit as usize,
    }
}

fn proto_to_sorted_run_chunk_request(
    p: wrpc::SortedRunCatchUpChunkRequest,
) -> SortedRunCatchUpChunkRequest {
    SortedRunCatchUpChunkRequest {
        handle: p.handle,
        term: p.term,
        chunk_stream_id: p.chunk_stream_id,
        chunk_index: p.chunk_index,
        total_chunks: p.total_chunks,
        payload: p.payload.into(),
    }
}

fn sorted_run_chunk_response_to_proto(
    r: SortedRunCatchUpChunkResponse,
) -> wrpc::SortedRunCatchUpChunkResponse {
    wrpc::SortedRunCatchUpChunkResponse {
        accepted: r.accepted,
        next_chunk_index: r.next_chunk_index,
        rejection_reason: r.rejection_reason,
    }
}

/// Convert internal WriteBatchRequest to proto for client calls.
/// The `wal_payload` field is left empty here; the caller attaches pre-encoded
/// WAL bytes after conversion when replicating via the fast path.
pub fn write_batch_request_to_proto(req: WriteBatchRequest) -> wrpc::WriteBatchRequest {
    wrpc::WriteBatchRequest {
        handle: req.handle,
        ops: req
            .ops
            .into_iter()
            .map(internal_batch_op_to_proto)
            .collect(),
        idempotency_token: req.idempotency_token,
        wal_payload: None,
        expected_home_epoch: req.expected_home_epoch,
        expected_shard_map_epoch: req.expected_shard_map_epoch,
        ownership_token: req.ownership_token,
    }
}

fn internal_batch_op_to_proto(op: BatchOp) -> wrpc::BatchOp {
    let op = match op {
        BatchOp::Put {
            namespace,
            key,
            value,
            expected_version,
        } => wrpc::batch_op::Op::Put(wrpc::PutOp {
            // Bytes fields in generated code use bytes::Bytes (via tonic_build .bytes(["."])),
            // so this is a zero-copy Arc bump rather than a heap allocation.
            namespace,
            key,
            value,
            expected_version,
        }),
        BatchOp::Delete {
            namespace,
            key,
            expected_version,
        } => wrpc::batch_op::Op::Delete(wrpc::DeleteOp {
            namespace,
            key,
            expected_version,
        }),
    };
    wrpc::BatchOp { op: Some(op) }
}

/// Convert internal PointReadRequest to proto for client calls.
pub fn point_read_request_to_proto(req: PointReadRequest) -> wrpc::PointReadRequest {
    wrpc::PointReadRequest {
        handle: req.handle,
        namespace: Bytes::from(req.namespace),
        key: Bytes::from(req.key),
    }
}

/// Convert internal RangeReadRequest to proto for client calls.
pub fn range_read_request_to_proto(
    req: crate::db::rpc::grpc::RangeReadRequest,
) -> wrpc::RangeReadRequest {
    wrpc::RangeReadRequest {
        handle: req.handle,
        namespace: Bytes::from(req.namespace),
        start_key: Bytes::from(req.start_key),
        end_key: Bytes::from(req.end_key),
        limit: req.limit as u64,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;

    #[test]
    fn write_batch_proto_conversion_preserves_required_fence_fields() {
        let internal = WriteBatchRequest {
            handle: 41,
            ops: vec![crate::db::types::BatchOp::Put {
                namespace: Bytes::from_static(b"core"),
                key: Bytes::from_static(b"k"),
                value: Bytes::from_static(b"v"),
                expected_version: None,
            }],
            idempotency_token: Some("tok-41".to_string()),
            expected_home_epoch: 7,
            expected_shard_map_epoch: 11,
            ownership_token: "ownership-41".to_string(),
        };

        let proto = write_batch_request_to_proto(internal.clone());
        assert_eq!(proto.expected_home_epoch, 7);
        assert_eq!(proto.expected_shard_map_epoch, 11);
        assert_eq!(proto.ownership_token, "ownership-41");

        let roundtrip = proto_to_write_batch_request(proto);
        assert_eq!(roundtrip, internal);
    }

    #[test]
    fn sorted_run_chunk_proto_conversion_preserves_payload_and_indexes() {
        let proto = wrpc::SortedRunCatchUpChunkRequest {
            handle: 41,
            term: 9,
            chunk_stream_id: 777,
            chunk_index: 3,
            total_chunks: 8,
            payload: Bytes::from_static(b"chunk-payload"),
        };
        let internal = proto_to_sorted_run_chunk_request(proto);
        assert_eq!(internal.handle, 41);
        assert_eq!(internal.term, 9);
        assert_eq!(internal.chunk_stream_id, 777);
        assert_eq!(internal.chunk_index, 3);
        assert_eq!(internal.total_chunks, 8);
        assert_eq!(internal.payload, b"chunk-payload".to_vec());
    }

    #[test]
    fn sorted_run_chunk_response_conversion_preserves_rejection_metadata() {
        let proto = sorted_run_chunk_response_to_proto(SortedRunCatchUpChunkResponse {
            accepted: false,
            next_chunk_index: 5,
            rejection_reason: "SORTED_RUN_OUT_OF_ORDER_CHUNK".to_string(),
        });
        assert!(!proto.accepted);
        assert_eq!(proto.next_chunk_index, 5);
        assert_eq!(proto.rejection_reason, "SORTED_RUN_OUT_OF_ORDER_CHUNK");
    }
}
