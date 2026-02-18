use crate::db::types::{DbError, ErrorCode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RpcStatusCode {
    InvalidArgument,
    RetryAfter,
    NotLeader,
    OccMismatch,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetryHint {
    pub retry_after_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeaderHint {
    pub leader_node_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
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
}
