use bytes::Bytes;
use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockCacheStats {
    pub hits: u64,
    pub misses: u64,
    pub entries: u64,
    pub capacity: u64,
}

#[derive(Debug)]
pub struct BlockCache {
    capacity: usize,
    entries: Mutex<HashMap<u64, (Bytes, u64)>>,
    access_counter: AtomicU64,
    hits: AtomicU64,
    misses: AtomicU64,
}

impl BlockCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            entries: Mutex::new(HashMap::new()),
            access_counter: AtomicU64::new(0),
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
        }
    }

    pub fn get(&self, block_id: u64) -> Option<Bytes> {
        let mut entries = self.entries.lock().expect("block cache lock");
        if let Some((data, access)) = entries.get_mut(&block_id) {
            *access = self.access_counter.fetch_add(1, Ordering::Relaxed);
            self.hits.fetch_add(1, Ordering::Relaxed);
            Some(data.clone())
        } else {
            self.misses.fetch_add(1, Ordering::Relaxed);
            None
        }
    }

    pub fn insert(&self, block_id: u64, data: Bytes) {
        let mut entries = self.entries.lock().expect("block cache lock");
        if entries.len() >= self.capacity && !entries.contains_key(&block_id) {
            // Evict the least recently used entry.
            let victim = entries
                .iter()
                .min_by_key(|(_, (_, access))| *access)
                .map(|(k, _)| *k);
            if let Some(victim_key) = victim {
                entries.remove(&victim_key);
            }
        }
        let access = self.access_counter.fetch_add(1, Ordering::Relaxed);
        entries.insert(block_id, (data, access));
    }

    pub fn stats(&self) -> BlockCacheStats {
        let entries = self.entries.lock().expect("block cache lock");
        BlockCacheStats {
            hits: self.hits.load(Ordering::Relaxed),
            misses: self.misses.load(Ordering::Relaxed),
            entries: entries.len() as u64,
            capacity: self.capacity as u64,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_hit_and_miss() {
        let cache = BlockCache::new(4);
        assert!(cache.get(1).is_none());
        cache.insert(1, Bytes::from_static(b"block-1"));
        assert_eq!(cache.get(1), Some(Bytes::from_static(b"block-1")));
        let stats = cache.stats();
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.entries, 1);
    }

    #[test]
    fn lru_eviction_removes_least_recent() {
        let cache = BlockCache::new(2);
        cache.insert(1, Bytes::from_static(b"b1"));
        cache.insert(2, Bytes::from_static(b"b2"));
        // Access block 1 to make it more recent.
        let _ = cache.get(1);
        // Insert block 3 — should evict block 2 (least recently used).
        cache.insert(3, Bytes::from_static(b"b3"));
        assert!(cache.get(1).is_some(), "block 1 should survive");
        assert!(cache.get(2).is_none(), "block 2 should be evicted");
        assert!(cache.get(3).is_some(), "block 3 should be present");
    }
}
