use crate::db::read::RETRY_AFTER_MS;
use crate::db::read::admission::ReadAdmissionPermit;
use crate::db::types::DbError;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug, Clone)]
pub struct RangeCancellation {
    cancelled: Arc<AtomicBool>,
}

impl RangeCancellation {
    pub fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

impl Default for RangeCancellation {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
pub struct RangeIterator {
    rows: Vec<(Vec<u8>, Vec<u8>, u64)>,
    offset: usize,
    cancellation: RangeCancellation,
    _permit: ReadAdmissionPermit,
}

impl RangeIterator {
    pub(super) fn new(
        rows: Vec<(Vec<u8>, Vec<u8>, u64)>,
        cancellation: RangeCancellation,
        permit: ReadAdmissionPermit,
    ) -> Self {
        Self {
            rows,
            offset: 0,
            cancellation,
            _permit: permit,
        }
    }

    pub fn try_next(&mut self) -> Result<Option<(Vec<u8>, Vec<u8>, u64)>, DbError> {
        if self.cancellation.is_cancelled() {
            return Err(DbError::limit(format!(
                "range iterator cancelled; RETRY_AFTER_MS={RETRY_AFTER_MS}"
            )));
        }
        if self.offset >= self.rows.len() {
            return Ok(None);
        }
        let row = self.rows[self.offset].clone();
        self.offset += 1;
        Ok(Some(row))
    }
}
