use crate::db::types::{DbError, MAX_BATCH_OPS};
use std::collections::VecDeque;

#[derive(Debug)]
pub struct DetachedWriterQueue<T> {
    queue: VecDeque<T>,
    max_ops: usize,
}

impl<T> DetachedWriterQueue<T> {
    pub fn new(max_ops: usize) -> Self {
        Self {
            queue: VecDeque::new(),
            max_ops,
        }
    }

    pub fn push(&mut self, item: T) -> Result<(), DbError> {
        let limit = if self.max_ops == 0 {
            MAX_BATCH_OPS
        } else {
            self.max_ops
        };
        if self.queue.len() >= limit {
            return Err(DbError::limit(
                "detached writer queue saturated; RETRY_AFTER_MS=25",
            ));
        }
        self.queue.push_back(item);
        Ok(())
    }

    pub fn pop(&mut self) -> Option<T> {
        self.queue.pop_front()
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }
}

impl<T> Default for DetachedWriterQueue<T> {
    fn default() -> Self {
        Self::new(MAX_BATCH_OPS)
    }
}
