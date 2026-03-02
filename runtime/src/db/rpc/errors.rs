use crate::db::types::{DbError, ErrorCode};
use serde::{Deserialize, Serialize};
use tonic::{Code, Status};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RpcStatusCode {
    InvalidArgument,
    RetryAfter,
    NotLeader,
    OccMismatch,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetryHint {
    pub retry_after_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeaderHint {
    pub leader_node_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RpcError {
    pub code: RpcStatusCode,
    pub message: String,
    pub retry: Option<RetryHint>,
    pub leader: Option<LeaderHint>,
}

impl RpcError {
    pub fn not_leader(leader_node_id: impl Into<String>) -> Self {
        let leader_node_id = leader_node_id.into();
        Self {
            code: RpcStatusCode::NotLeader,
            message: format!("NOT_LEADER: redirect to {}", leader_node_id),
            retry: Some(RetryHint { retry_after_ms: 25 }),
            leader: Some(LeaderHint { leader_node_id }),
        }
    }
}

fn parse_retry_after_ms(message: &str) -> Option<u64> {
    let marker = "RETRY_AFTER_MS=";
    let idx = message.find(marker)?;
    let tail = &message[idx + marker.len()..];
    let digits: String = tail.chars().take_while(|ch| ch.is_ascii_digit()).collect();
    if digits.is_empty() {
        return None;
    }
    digits.parse::<u64>().ok()
}

/// Extracts the leader node ID from a `NOT_LEADER: redirect to <id>` message.
fn parse_leader_node_id(message: &str) -> Option<String> {
    let prefix = "NOT_LEADER: redirect to ";
    let tail = message.strip_prefix(prefix)?;
    let node_id = tail.trim();
    if node_id.is_empty() {
        return None;
    }
    Some(node_id.to_string())
}

pub fn map_db_error(err: DbError) -> RpcError {
    match err.code {
        ErrorCode::InvalidArgument => RpcError {
            code: RpcStatusCode::InvalidArgument,
            message: err.message,
            retry: None,
            leader: None,
        },
        ErrorCode::OccMismatch => RpcError {
            code: RpcStatusCode::OccMismatch,
            message: err.message,
            retry: None,
            leader: None,
        },
        ErrorCode::LimitExceeded => {
            let retry_after_ms = parse_retry_after_ms(&err.message).unwrap_or(25);
            RpcError {
                code: RpcStatusCode::RetryAfter,
                message: err.message,
                retry: Some(RetryHint { retry_after_ms }),
                leader: None,
            }
        }
        ErrorCode::Io | ErrorCode::NotFound => RpcError {
            code: RpcStatusCode::Unavailable,
            message: err.message,
            retry: Some(RetryHint { retry_after_ms: 25 }),
            leader: None,
        },
        ErrorCode::MixedShardBatchUnsupported
        | ErrorCode::CrossShardTxnUnsupported
        | ErrorCode::SovereigntyWriteDenied
        | ErrorCode::SovereigntyReadDenied
        | ErrorCode::SovereigntyCheckpointRegionDenied
        | ErrorCode::SovereigntyPolicyMissing => RpcError {
            code: RpcStatusCode::InvalidArgument,
            message: err.message,
            retry: None,
            leader: None,
        },
    }
}

/// Maps RpcError to tonic Status for gRPC transport.
pub fn rpc_error_to_status(err: RpcError) -> Status {
    let code = match err.code {
        RpcStatusCode::InvalidArgument => Code::InvalidArgument,
        RpcStatusCode::RetryAfter => Code::ResourceExhausted,
        RpcStatusCode::NotLeader => Code::FailedPrecondition,
        RpcStatusCode::OccMismatch => Code::Aborted,
        RpcStatusCode::Unavailable => Code::Unavailable,
    };
    Status::new(code, err.message)
}

/// Maps tonic Status from gRPC transport back to RpcError.
///
/// `NotLeader` responses include the leader node ID in the message as
/// `"NOT_LEADER: redirect to <node_id>"`. This is parsed back into the
/// `leader` hint field so callers can perform directed redirects rather than
/// blind retries.
pub fn status_to_rpc_error(status: tonic::Status) -> RpcError {
    let code = match status.code() {
        Code::InvalidArgument => RpcStatusCode::InvalidArgument,
        Code::ResourceExhausted => RpcStatusCode::RetryAfter,
        Code::FailedPrecondition => RpcStatusCode::NotLeader,
        Code::Aborted => RpcStatusCode::OccMismatch,
        Code::Unavailable => RpcStatusCode::Unavailable,
        _ => RpcStatusCode::Unavailable,
    };
    let message = status.message().to_string();
    let leader = if matches!(code, RpcStatusCode::NotLeader) {
        parse_leader_node_id(&message).map(|leader_node_id| LeaderHint { leader_node_id })
    } else {
        None
    };
    RpcError {
        code,
        retry: if matches!(
            status.code(),
            Code::Unavailable | Code::ResourceExhausted | Code::FailedPrecondition
        ) {
            Some(RetryHint { retry_after_ms: 25 })
        } else {
            None
        },
        leader,
        message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_limit_error_to_retry_after_with_parsed_hint() {
        let err = DbError::limit("queue full; RETRY_AFTER_MS=40");
        let mapped = map_db_error(err);
        assert_eq!(mapped.code, RpcStatusCode::RetryAfter);
        assert_eq!(mapped.retry.as_ref().map(|h| h.retry_after_ms), Some(40));
    }

    #[test]
    fn maps_occ_error_to_occ_status() {
        let err = DbError::occ("expected version mismatch");
        let mapped = map_db_error(err);
        assert_eq!(mapped.code, RpcStatusCode::OccMismatch);
        assert!(mapped.retry.is_none());
    }

    #[test]
    fn not_leader_error_contains_redirect_hint() {
        let err = RpcError::not_leader("node-b");
        assert_eq!(err.code, RpcStatusCode::NotLeader);
        assert_eq!(
            err.leader.as_ref().map(|hint| hint.leader_node_id.as_str()),
            Some("node-b")
        );
    }

    #[test]
    fn status_to_rpc_error_preserves_leader_hint_round_trip() {
        let original = RpcError::not_leader("node-c");
        let status = rpc_error_to_status(original);
        let recovered = status_to_rpc_error(status);
        assert_eq!(recovered.code, RpcStatusCode::NotLeader);
        assert_eq!(
            recovered.leader.as_ref().map(|h| h.leader_node_id.as_str()),
            Some("node-c"),
            "leader hint must survive gRPC status round-trip"
        );
        assert!(recovered.retry.is_some());
    }

    #[test]
    fn status_to_rpc_error_non_leader_errors_have_no_leader_hint() {
        let err = map_db_error(DbError::occ("version mismatch"));
        let status = rpc_error_to_status(err);
        let recovered = status_to_rpc_error(status);
        assert_eq!(recovered.code, RpcStatusCode::OccMismatch);
        assert!(recovered.leader.is_none());
    }
}
