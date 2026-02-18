use crate::db::read::RETRY_AFTER_MS;
use crate::db::types::DbError;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug, Clone, Copy)]
enum ReadKind {
    Point,
    Range,
}

#[derive(Debug)]
pub struct ReadAdmissionController {
    point_limit: usize,
    range_limit: usize,
    point_in_flight: AtomicUsize,
    range_in_flight: AtomicUsize,
}

impl ReadAdmissionController {
    pub fn new(point_limit: usize, range_limit: usize) -> Self {
        Self {
            point_limit: point_limit.max(1),
            range_limit: range_limit.max(1),
            point_in_flight: AtomicUsize::new(0),
            range_in_flight: AtomicUsize::new(0),
        }
    }

    pub fn acquire_point(self: &Arc<Self>) -> Result<ReadAdmissionPermit, DbError> {
        self.acquire(ReadKind::Point)
    }

    pub fn acquire_range(self: &Arc<Self>) -> Result<ReadAdmissionPermit, DbError> {
        self.acquire(ReadKind::Range)
    }

    fn acquire(self: &Arc<Self>, kind: ReadKind) -> Result<ReadAdmissionPermit, DbError> {
        let (counter, limit, label) = match kind {
            ReadKind::Point => (&self.point_in_flight, self.point_limit, "point"),
            ReadKind::Range => (&self.range_in_flight, self.range_limit, "range"),
        };

        loop {
            let current = counter.load(Ordering::Acquire);
            if current >= limit {
                return Err(DbError::limit(format!(
                    "read admission rejected ({label}); RETRY_AFTER_MS={RETRY_AFTER_MS}"
                )));
            }
            if counter
                .compare_exchange(current, current + 1, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                return Ok(ReadAdmissionPermit {
                    controller: Arc::clone(self),
                    kind,
                });
            }
        }
    }

    fn release(&self, kind: ReadKind) {
        match kind {
            ReadKind::Point => {
                self.point_in_flight.fetch_sub(1, Ordering::AcqRel);
            }
            ReadKind::Range => {
                self.range_in_flight.fetch_sub(1, Ordering::AcqRel);
            }
        }
    }
}

#[derive(Debug)]
pub struct ReadAdmissionPermit {
    controller: Arc<ReadAdmissionController>,
    kind: ReadKind,
}

impl Drop for ReadAdmissionPermit {
    fn drop(&mut self) {
        self.controller.release(self.kind);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::types::ErrorCode;

    #[test]
    fn enforces_point_and_range_limits_independently() {
        let controller = Arc::new(ReadAdmissionController::new(1, 1));

        let point_permit = controller.acquire_point().expect("point permit");
        let range_permit = controller.acquire_range().expect("range permit");

        let point_err = controller
            .acquire_point()
            .expect_err("point should be rejected");
        assert_eq!(point_err.code, ErrorCode::LimitExceeded);
        assert!(point_err.message.contains("RETRY_AFTER_MS=25"));

        let range_err = controller
            .acquire_range()
            .expect_err("range should be rejected");
        assert_eq!(range_err.code, ErrorCode::LimitExceeded);
        assert!(range_err.message.contains("RETRY_AFTER_MS=25"));

        drop(point_permit);
        drop(range_permit);

        controller.acquire_point().expect("point capacity restored");
        controller.acquire_range().expect("range capacity restored");
    }
}
