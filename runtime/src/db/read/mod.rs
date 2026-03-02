pub mod admission;
pub mod block_cache;
pub mod bloom;
pub mod cache;
pub mod iterator;
pub mod rejection;
pub mod strong;

use crate::db::read::admission::ReadAdmissionController;
use crate::db::read::block_cache::BlockCache;
pub use crate::db::read::block_cache::BlockCacheStats;
use crate::db::read::bloom::NegativeBloom;
use crate::db::read::cache::{PointReadCache, PointReadCacheStats};
use crate::db::read::iterator::{RangeCancellation, RangeIterator};
use crate::db::types::DbError;
use bytes::Bytes;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

pub const RETRY_AFTER_MS: u64 = 25;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointShortcutPolicy {
    KeyOnlyShortcutsEnabled,
    KeyOnlyShortcutsDisabled,
}

impl PointShortcutPolicy {
    fn allows_key_only_shortcuts(self) -> bool {
        matches!(self, Self::KeyOnlyShortcutsEnabled)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadPathStats {
    pub point_cache_hits: u64,
    pub point_cache_misses: u64,
    pub negative_shortcuts: u64,
    pub block_cache: BlockCacheStats,
}

#[derive(Debug)]
pub struct ReadPath {
    admission: Arc<ReadAdmissionController>,
    point_cache: PointReadCache,
    block_cache: BlockCache,
    negative_bloom: NegativeBloom,
    negative_shortcuts: AtomicU64,
}

impl ReadPath {
    pub fn new(
        point_in_flight_limit: usize,
        range_in_flight_limit: usize,
        point_cache_capacity: usize,
        negative_bloom_capacity: usize,
    ) -> Self {
        Self {
            admission: Arc::new(ReadAdmissionController::new(
                point_in_flight_limit,
                range_in_flight_limit,
            )),
            point_cache: PointReadCache::new(point_cache_capacity),
            block_cache: BlockCache::new(1024),
            negative_bloom: NegativeBloom::new(negative_bloom_capacity),
            negative_shortcuts: AtomicU64::new(0),
        }
    }

    pub fn read_point<F>(
        &self,
        user_key: &[u8],
        policy: PointShortcutPolicy,
        loader: F,
    ) -> Result<Option<Vec<u8>>, DbError>
    where
        F: FnOnce() -> Option<Vec<u8>>,
    {
        let _permit = self.admission.acquire_point()?;
        if policy.allows_key_only_shortcuts() {
            if let Some(cached) = self.point_cache.get(user_key) {
                return Ok(Some(cached.to_vec()));
            }
            if self.negative_bloom.probably_absent(user_key) {
                self.negative_shortcuts.fetch_add(1, Ordering::Relaxed);
                return Ok(None);
            }
        }

        let loaded = loader();
        if policy.allows_key_only_shortcuts() {
            match loaded {
                Some(value) => {
                    self.negative_bloom.record_present(user_key);
                    let bytes_value = Bytes::from(value);
                    self.point_cache.insert(user_key, bytes_value.clone());
                    Ok(Some(bytes_value.to_vec()))
                }
                None => {
                    self.negative_bloom.record_absent(user_key);
                    Ok(None)
                }
            }
        } else {
            Ok(loaded)
        }
    }

    /// Like `read_point` but returns the version of the visible value as well.
    /// Used for idempotency lookups where the version is the commit_version.
    /// Does not use the point cache (different return type).
    pub fn read_point_with_version<F>(
        &self,
        user_key: &[u8],
        policy: PointShortcutPolicy,
        loader: F,
    ) -> Result<Option<(u64, Vec<u8>)>, DbError>
    where
        F: FnOnce() -> Option<(u64, Vec<u8>)>,
    {
        let _permit = self.admission.acquire_point()?;
        if policy.allows_key_only_shortcuts() && self.negative_bloom.probably_absent(user_key) {
            self.negative_shortcuts.fetch_add(1, Ordering::Relaxed);
            return Ok(None);
        }
        let loaded = loader();
        if policy.allows_key_only_shortcuts() {
            match loaded {
                Some((version, value)) => {
                    self.negative_bloom.record_present(user_key);
                    Ok(Some((version, value)))
                }
                None => {
                    self.negative_bloom.record_absent(user_key);
                    Ok(None)
                }
            }
        } else {
            Ok(loaded)
        }
    }

    pub fn begin_range(
        &self,
        rows: Vec<(Vec<u8>, Bytes, u64)>,
        cancellation: RangeCancellation,
    ) -> Result<RangeIterator, DbError> {
        let permit = self.admission.acquire_range()?;
        Ok(RangeIterator::new(rows, cancellation, permit))
    }

    pub fn observe_present_key(&self, user_key: &[u8]) {
        self.point_cache.invalidate(user_key);
        self.negative_bloom.record_present(user_key);
    }

    pub fn observe_absent_key(&self, user_key: &[u8]) {
        self.point_cache.invalidate(user_key);
        self.negative_bloom.record_absent(user_key);
    }

    pub fn block_cache(&self) -> &BlockCache {
        &self.block_cache
    }

    pub fn stats(&self) -> ReadPathStats {
        let PointReadCacheStats { hits, misses } = self.point_cache.stats();
        ReadPathStats {
            point_cache_hits: hits,
            point_cache_misses: misses,
            negative_shortcuts: self.negative_shortcuts.load(Ordering::Relaxed),
            block_cache: self.block_cache.stats(),
        }
    }
}
