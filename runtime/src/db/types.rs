use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::fmt;

pub const MAX_KEY_BYTES: usize = 1024;
pub const MAX_VALUE_BYTES: usize = 1024 * 1024;
pub const MAX_BATCH_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_BATCH_OPS: usize = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    InvalidArgument,
    LimitExceeded,
    NotFound,
    OccMismatch,
    Io,
    MixedShardBatchUnsupported,
    CrossShardTxnUnsupported,
    SovereigntyWriteDenied,
    SovereigntyReadDenied,
    SovereigntyCheckpointRegionDenied,
    SovereigntyPolicyMissing,
}

#[derive(Debug, Clone)]
pub struct DbError {
    pub code: ErrorCode,
    pub message: String,
}

impl DbError {
    pub fn invalid_argument(message: impl Into<String>) -> Self {
        Self {
            code: ErrorCode::InvalidArgument,
            message: message.into(),
        }
    }

    pub fn limit(message: impl Into<String>) -> Self {
        Self {
            code: ErrorCode::LimitExceeded,
            message: message.into(),
        }
    }

    pub fn io(message: impl Into<String>) -> Self {
        Self {
            code: ErrorCode::Io,
            message: message.into(),
        }
    }

    pub fn occ(message: impl Into<String>) -> Self {
        Self {
            code: ErrorCode::OccMismatch,
            message: message.into(),
        }
    }

    pub fn mixed_shard_batch(message: impl Into<String>) -> Self {
        Self {
            code: ErrorCode::MixedShardBatchUnsupported,
            message: message.into(),
        }
    }

    pub fn cross_shard_txn(message: impl Into<String>) -> Self {
        Self {
            code: ErrorCode::CrossShardTxnUnsupported,
            message: message.into(),
        }
    }

    pub fn sovereignty_write_denied(message: impl Into<String>) -> Self {
        Self {
            code: ErrorCode::SovereigntyWriteDenied,
            message: message.into(),
        }
    }

    pub fn sovereignty_read_denied(message: impl Into<String>) -> Self {
        Self {
            code: ErrorCode::SovereigntyReadDenied,
            message: message.into(),
        }
    }

    pub fn sovereignty_checkpoint_denied(message: impl Into<String>) -> Self {
        Self {
            code: ErrorCode::SovereigntyCheckpointRegionDenied,
            message: message.into(),
        }
    }

    pub fn sovereignty_policy_missing(message: impl Into<String>) -> Self {
        Self {
            code: ErrorCode::SovereigntyPolicyMissing,
            message: message.into(),
        }
    }
}

impl fmt::Display for DbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

impl std::error::Error for DbError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BatchOp {
    Put {
        namespace: Bytes,
        key: Bytes,
        value: Bytes,
        expected_version: Option<u64>,
    },
    Delete {
        namespace: Bytes,
        key: Bytes,
        expected_version: Option<u64>,
    },
}
