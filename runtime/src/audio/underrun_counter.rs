use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Default)]
pub struct UnderrunCounter {
    inner: AtomicU64,
}

impl UnderrunCounter {
    pub fn increment(&self) {
        self.inner.fetch_add(1, Ordering::Relaxed);
    }

    pub fn total(&self) -> u64 {
        self.inner.load(Ordering::Acquire)
    }

    pub fn take(&self) -> u64 {
        self.inner.swap(0, Ordering::AcqRel)
    }
}
