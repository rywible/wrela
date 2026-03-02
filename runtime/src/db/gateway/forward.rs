use crate::db::rpc::errors::RpcError;
use crate::db::rpc::grpc::{GrpcEdgeService, WriteBatchRequest, WriteBatchResponse};

#[derive(Debug, Clone)]
pub struct ForwardRequest {
    pub target_leader: String,
    pub request: WriteBatchRequest,
}

pub fn forward_to_leader(
    service: &mut GrpcEdgeService,
    forward: ForwardRequest,
) -> Result<WriteBatchResponse, RpcError> {
    service.forward_write_batch(&forward.target_leader, forward.request)
}
