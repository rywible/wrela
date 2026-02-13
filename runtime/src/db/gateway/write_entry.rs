use crate::db::gateway::forward::{ForwardRequest, forward_to_leader};
use crate::db::rpc::errors::RpcError;
use crate::db::rpc::grpc::{GrpcEdgeService, WriteBatchRequest, WriteBatchResponse};
use crate::db::shard::map::ShardMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatewayWriteMetrics {
    pub hop_count: u32,
    pub forwarded: bool,
    pub forwarding_latency_ns: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatewayWriteOutcome {
    pub response: WriteBatchResponse,
    pub leader_node_id: String,
    pub metrics: GatewayWriteMetrics,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GatewayWriteError {
    UnknownShard(u32),
    Rpc(RpcError),
}

pub fn write_with_ownership_forwarding(
    local_node_id: &str,
    shard_id: u32,
    shard_map: &ShardMap,
    edge_service: &mut GrpcEdgeService,
    request: WriteBatchRequest,
) -> Result<GatewayWriteOutcome, GatewayWriteError> {
    let assignment = shard_map
        .assignments
        .get(&shard_id)
        .ok_or(GatewayWriteError::UnknownShard(shard_id))?;

    if assignment.leader == local_node_id {
        let response = edge_service
            .write_batch(request)
            .map_err(GatewayWriteError::Rpc)?;
        return Ok(GatewayWriteOutcome {
            response,
            leader_node_id: assignment.leader.clone(),
            metrics: GatewayWriteMetrics {
                hop_count: 0,
                forwarded: false,
                forwarding_latency_ns: 0,
            },
        });
    }

    let start = std::time::Instant::now();
    let response = forward_to_leader(
        edge_service,
        ForwardRequest {
            target_leader: assignment.leader.clone(),
            request,
        },
    )
    .map_err(GatewayWriteError::Rpc)?;

    Ok(GatewayWriteOutcome {
        response,
        leader_node_id: assignment.leader.clone(),
        metrics: GatewayWriteMetrics {
            hop_count: 1,
            forwarded: true,
            forwarding_latency_ns: start.elapsed().as_nanos() as u64,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::rpc::grpc::WriteBatchRequest;
    use crate::db::shard::map::build_initial_shard_map;
    use crate::db::types::BatchOp;
    use crate::db::{close_db, open_db};

    fn temp_dir() -> tempfile::TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    #[test]
    fn forwards_when_local_is_not_leader() {
        let path = temp_dir();
        let handle = open_db(path.path()).expect("open");
        let nodes = vec![
            "node-a".to_string(),
            "node-b".to_string(),
            "node-c".to_string(),
        ];
        let mut map = build_initial_shard_map(&nodes, 1, 3).expect("map");
        map.assignments.get_mut(&0).expect("shard").leader = "node-b".to_string();

        let mut svc = GrpcEdgeService::new("node-b", "node-b");
        let outcome = write_with_ownership_forwarding(
            "node-a",
            0,
            &map,
            &mut svc,
            WriteBatchRequest {
                handle,
                ops: vec![BatchOp::Put {
                    namespace: b"core".to_vec(),
                    key: b"k".to_vec(),
                    value: b"v".to_vec(),
                    expected_version: None,
                }],
                idempotency_token: Some("tok-forward-1".to_string()),
            },
        )
        .expect("forwarded");

        assert!(outcome.metrics.forwarded);
        assert_eq!(outcome.metrics.hop_count, 1);
        assert_eq!(outcome.leader_node_id, "node-b");
        assert!(close_db(handle));
    }
}
