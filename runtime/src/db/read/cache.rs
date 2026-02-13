use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PointReadCacheStats {
    pub hits: u64,
    pub misses: u64,
}

#[derive(Debug)]
pub struct PointReadCache {
    capacity: usize,
    entries: Mutex<HashMap<Vec<u8>, Vec<u8>>>,
    hits: AtomicU64,
    misses: AtomicU64,
}

impl PointReadCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            entries: Mutex::new(HashMap::new()),
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
        }
    }

    pub fn get(&self, key: &[u8]) -> Option<Vec<u8>> {
        let maybe = self
            .entries
            .lock()
            .expect("point cache lock")
            .get(key)
            .cloned();
        if maybe.is_some() {
            self.hits.fetch_add(1, Ordering::Relaxed);
        } else {
            self.misses.fetch_add(1, Ordering::Relaxed);
        }
        maybe
    }

    pub fn insert(&self, key: &[u8], value: Vec<u8>) {
        let mut entries = self.entries.lock().expect("point cache lock");
        if entries.len() >= self.capacity
            && !entries.contains_key(key)
            && let Some(victim) = entries.keys().next().cloned()
        {
            entries.remove(&victim);
        }
        entries.insert(key.to_vec(), value);
    }

    pub fn invalidate(&self, key: &[u8]) {
        self.entries.lock().expect("point cache lock").remove(key);
    }

    pub fn stats(&self) -> PointReadCacheStats {
        PointReadCacheStats {
            hits: self.hits.load(Ordering::Relaxed),
            misses: self.misses.load(Ordering::Relaxed),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracks_hit_and_miss_counters() {
        let cache = PointReadCache::new(8);
        assert_eq!(cache.get(b"k1"), None);
        cache.insert(b"k1", b"v1".to_vec());
        assert_eq!(cache.get(b"k1"), Some(b"v1".to_vec()));
        assert_eq!(cache.get(b"k1"), Some(b"v1".to_vec()));

        let stats = cache.stats();
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.hits, 2);
    }
}
