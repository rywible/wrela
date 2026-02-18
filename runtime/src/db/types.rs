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
}

impl fmt::Display for DbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

impl std::error::Error for DbError {}

#[derive(Debug, Clone)]
pub enum BatchOp {
    Put {
        namespace: Vec<u8>,
        key: Vec<u8>,
        value: Vec<u8>,
        expected_version: Option<u64>,
    },
    Delete {
        namespace: Vec<u8>,
        key: Vec<u8>,
        expected_version: Option<u64>,
    },
}
