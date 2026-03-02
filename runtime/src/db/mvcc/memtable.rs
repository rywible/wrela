//! In-memory mutable table for recent versions (MVCC).
//!
//! **Data structure**: Currently `BTreeMap<Vec<u8>, VersionChain>`. BTreeMap has predictable
//! performance but relatively poor cache locality for small key-value workloads. For a future
//! performance wave, consider evaluating:
//! - **Concurrent skip list** (e.g. crossbeam-skiplist): lockless reads, good for mixed
//!   read/write under the DbEngine RwLock split; would allow lockless point/range reads
//!   from the memtable while writes proceed.
//! - **ART (Adaptive Radix Tree)**: better cache behavior and lower memory for dense key
//!   namespaces; more complex to implement with MVCC version chains.
//! Either change would compound with the existing RwLock split and hot-path optimizations.

use bytes::Bytes;
use smallvec::SmallVec;
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Bound;

#[derive(Debug, Clone)]
pub struct VersionedValue {
    pub version: u64,
    pub value: Option<Bytes>,
}

type VersionChain = SmallVec<[VersionedValue; 1]>;

#[derive(Debug, Default)]
pub struct Memtable {
    rows: BTreeMap<Vec<u8>, VersionChain>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MemtableGcMetrics {
    /// Stale MVCC version entries dropped from existing key chains.
    pub versions_dropped: u64,
    /// Keys removed entirely because their only surviving version was a tombstone
    /// at or below the GC watermark.
    pub tombstone_keys_removed: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MemtableStats {
    pub key_count: u64,
    pub version_count: u64,
    pub tombstone_count: u64,
    pub live_version_count: u64,
    pub total_bytes_estimate: u64,
    pub live_bytes_estimate: u64,
    pub shadow_bytes_estimate: u64,
    pub compaction_debt_bytes_estimate: u64,
}

impl Memtable {
    pub fn apply(&mut self, key: &[u8], version: u64, value: Option<Bytes>) {
        if let Some(entry) = self.rows.get_mut(key) {
            apply_version_chain(entry, version, value);
            return;
        }
        let mut entry = VersionChain::new();
        entry.push(VersionedValue { version, value });
        self.rows.insert(key.to_vec(), entry);
    }

    pub fn apply_owned(&mut self, key: Vec<u8>, version: u64, value: Option<Bytes>) {
        if let Some(entry) = self.rows.get_mut(key.as_slice()) {
            apply_version_chain(entry, version, value);
            return;
        }
        let mut entry = VersionChain::new();
        entry.push(VersionedValue { version, value });
        self.rows.insert(key, entry);
    }

    pub fn latest_version(&self, key: &[u8]) -> Option<u64> {
        self.rows
            .get(key)
            .and_then(|versions| versions.last().map(|entry| entry.version))
    }

    pub fn visible(&self, key: &[u8], read_version: u64) -> Option<&[u8]> {
        self.rows.get(key).and_then(|versions| {
            latest_visible_index(versions, read_version)
                .and_then(|idx| versions.get(idx))
                .and_then(|entry| entry.value.as_deref())
        })
    }

    /// Like `visible` but returns the version of the visible entry as well.
    /// Used for idempotency record lookups where the version is the commit_version.
    pub fn visible_with_version(&self, key: &[u8], read_version: u64) -> Option<(u64, Vec<u8>)> {
        self.rows.get(key).and_then(|versions| {
            let idx = latest_visible_index(versions, read_version)?;
            let entry = versions.get(idx)?;
            let value = entry.value.as_ref().map(|b| b.to_vec())?;
            Some((entry.version, value))
        })
    }

    pub fn range_visible(
        &self,
        start: &[u8],
        end: &[u8],
        read_version: u64,
        limit: usize,
    ) -> Vec<(Vec<u8>, Bytes, u64)> {
        self.rows
            .range::<[u8], _>((Bound::Included(start), Bound::Excluded(end)))
            .filter_map(|(k, versions)| {
                let idx = latest_visible_index(versions, read_version)?;
                let entry = versions.get(idx)?;
                entry
                    .value
                    .as_ref()
                    .map(|val| (k.clone(), val.clone(), entry.version))
            })
            .take(limit)
            .collect()
    }

    /// Prune stale MVCC versions from the memtable.
    ///
    /// For each key, all versions strictly older than the latest version at or
    /// before `gc_watermark` are dropped -- they can never be the visible version
    /// for any read that will be issued going forward (all future reads use a
    /// `read_version >= gc_watermark`).
    ///
    /// If the surviving "anchor" version (latest at or before the watermark) is a
    /// tombstone AND no newer versions exist, the key is removed entirely.
    ///
    /// Returns metrics describing how much was reclaimed.
    pub fn gc_old_versions(&mut self, gc_watermark: u64) -> MemtableGcMetrics {
        let mut metrics = MemtableGcMetrics::default();

        self.rows.retain(|_key, chain| {
            // Find the index of the latest version <= gc_watermark.
            let anchor_idx = chain
                .partition_point(|entry| entry.version <= gc_watermark)
                .checked_sub(1);

            let Some(anchor_idx) = anchor_idx else {
                // All versions are newer than the watermark; nothing to prune.
                return true;
            };

            // Drop all versions strictly before the anchor.
            if anchor_idx > 0 {
                metrics.versions_dropped =
                    metrics.versions_dropped.saturating_add(anchor_idx as u64);
                chain.drain(..anchor_idx);
                // anchor is now at index 0.
            }

            // If the anchor (now at index 0) is a tombstone and is also the
            // last version in the chain, the key is permanently deleted from
            // the perspective of all future reads.
            if chain.len() == 1 && chain[0].value.is_none() {
                metrics.tombstone_keys_removed = metrics.tombstone_keys_removed.saturating_add(1);
                return false;
            }

            true
        });

        metrics
    }

    pub fn stats(&self) -> MemtableStats {
        let mut stats = MemtableStats {
            key_count: self.rows.len() as u64,
            ..MemtableStats::default()
        };
        for (key, versions) in &self.rows {
            for version in versions {
                stats.version_count = stats.version_count.saturating_add(1);
                let approx = estimated_versioned_value_bytes(key, version) as u64;
                stats.total_bytes_estimate = stats.total_bytes_estimate.saturating_add(approx);
                match &version.value {
                    Some(_) => {
                        stats.live_version_count = stats.live_version_count.saturating_add(1);
                    }
                    None => {
                        stats.tombstone_count = stats.tombstone_count.saturating_add(1);
                    }
                }
            }
            if let Some(latest) = versions.last() {
                let latest_est = estimated_versioned_value_bytes(key, latest) as u64;
                stats.live_bytes_estimate = stats.live_bytes_estimate.saturating_add(latest_est);
            }
        }
        stats.shadow_bytes_estimate = stats
            .total_bytes_estimate
            .saturating_sub(stats.live_bytes_estimate);
        stats.compaction_debt_bytes_estimate = stats.shadow_bytes_estimate;
        stats
    }

    pub fn referenced_blob_ids<F>(&self, mut decode: F) -> BTreeSet<u64>
    where
        F: FnMut(&[u8]) -> Option<u64>,
    {
        let mut referenced = BTreeSet::new();
        for versions in self.rows.values() {
            for version in versions {
                if let Some(value) = version.value.as_deref()
                    && let Some(blob_id) = decode(value)
                {
                    referenced.insert(blob_id);
                }
            }
        }
        referenced
    }
}

fn apply_version_chain(entry: &mut VersionChain, version: u64, value: Option<Bytes>) {
    // Fast path: appending a strictly newer version (the common case).
    if let Some(last) = entry.last() {
        if version > last.version {
            entry.push(VersionedValue { version, value });
            return;
        }
        // Equal version: replace in place (idempotent replay / WAL re-apply).
        if version == last.version {
            if let Some(last_mut) = entry.last_mut() {
                last_mut.value = value;
            }
            return;
        }
    } else {
        entry.push(VersionedValue { version, value });
        return;
    }

    // Out-of-order version: find the correct insertion point.
    let insert_at = entry.partition_point(|candidate| candidate.version < version);
    if insert_at < entry.len() && entry[insert_at].version == version {
        // Duplicate version in the middle: replace the existing entry.
        entry[insert_at].value = value;
    } else {
        entry.insert(insert_at, VersionedValue { version, value });
    }
}

fn estimated_versioned_value_bytes(key: &[u8], entry: &VersionedValue) -> usize {
    let value_bytes = entry.value.as_ref().map_or(0, Bytes::len);
    // key bytes + version + tombstone/live marker + optional value bytes.
    key.len()
        .saturating_add(8)
        .saturating_add(1)
        .saturating_add(value_bytes)
}

fn latest_visible_index(versions: &[VersionedValue], read_version: u64) -> Option<usize> {
    let idx = versions.partition_point(|entry| entry.version <= read_version);
    idx.checked_sub(1)
}

#[cfg(test)]
mod tests {
    use super::Memtable;
    use bytes::Bytes;

    #[test]
    fn apply_out_of_order_versions_preserves_latest_lookup() {
        let mut memtable = Memtable::default();
        memtable.apply(b"k", 3, Some(Bytes::from_static(b"v3")));
        memtable.apply(b"k", 1, Some(Bytes::from_static(b"v1")));
        memtable.apply(b"k", 2, Some(Bytes::from_static(b"v2")));
        assert_eq!(memtable.latest_version(b"k"), Some(3));
        assert_eq!(memtable.visible(b"k", 2), Some(&b"v2"[..]));
    }

    #[test]
    fn visible_respects_latest_tombstone() {
        let mut memtable = Memtable::default();
        memtable.apply(b"k", 1, Some(Bytes::from_static(b"v1")));
        memtable.apply(b"k", 2, None);
        assert_eq!(memtable.visible(b"k", 2), None);
    }

    #[test]
    fn stats_estimate_shadow_and_debt_bytes() {
        let mut memtable = Memtable::default();
        memtable.apply(b"a", 1, Some(Bytes::from_static(b"v1")));
        memtable.apply(b"a", 2, Some(Bytes::from_static(b"v2")));
        memtable.apply(b"b", 3, None);

        let stats = memtable.stats();
        assert_eq!(stats.key_count, 2);
        assert_eq!(stats.version_count, 3);
        assert_eq!(stats.live_version_count, 2);
        assert_eq!(stats.tombstone_count, 1);
        assert!(stats.total_bytes_estimate >= stats.live_bytes_estimate);
        assert_eq!(
            stats.shadow_bytes_estimate,
            stats
                .total_bytes_estimate
                .saturating_sub(stats.live_bytes_estimate)
        );
        assert_eq!(
            stats.compaction_debt_bytes_estimate,
            stats.shadow_bytes_estimate
        );
    }

    #[test]
    fn apply_avoids_key_allocation_when_key_exists() {
        let mut memtable = Memtable::default();
        memtable.apply(b"same", 1, Some(Bytes::from_static(b"v1")));
        memtable.apply(b"same", 2, Some(Bytes::from_static(b"v2")));
        assert_eq!(memtable.latest_version(b"same"), Some(2));
        assert_eq!(memtable.visible(b"same", 2), Some(&b"v2"[..]));
    }

    #[test]
    fn gc_prunes_stale_versions_below_watermark() {
        let mut memtable = Memtable::default();
        memtable.apply(b"k", 1, Some(Bytes::from_static(b"v1")));
        memtable.apply(b"k", 2, Some(Bytes::from_static(b"v2")));
        memtable.apply(b"k", 3, Some(Bytes::from_static(b"v3")));
        // Watermark = 2: version 1 can be dropped, versions 2 and 3 are kept.
        let metrics = memtable.gc_old_versions(2);
        assert_eq!(metrics.versions_dropped, 1);
        assert_eq!(metrics.tombstone_keys_removed, 0);
        let chain = memtable.rows.get(&b"k"[..]).expect("key present");
        assert_eq!(chain.len(), 2, "versions 2 and 3 must survive");
        assert_eq!(chain[0].version, 2);
        assert_eq!(chain[1].version, 3);
    }

    #[test]
    fn gc_removes_tombstone_key_when_anchor_is_tombstone_and_no_newer_versions() {
        let mut memtable = Memtable::default();
        memtable.apply(b"del", 1, Some(Bytes::from_static(b"v1")));
        memtable.apply(b"del", 2, None); // tombstone
        memtable.apply(b"live", 1, Some(Bytes::from_static(b"live")));
        let metrics = memtable.gc_old_versions(2);
        // "del" had tombstone at v2 (its only remaining version after GC) -> removed
        assert_eq!(metrics.tombstone_keys_removed, 1);
        assert!(
            !memtable.rows.contains_key(&b"del"[..]),
            "tombstone key should be gone"
        );
        assert!(
            memtable.rows.contains_key(&b"live"[..]),
            "live key must survive"
        );
    }

    #[test]
    fn gc_keeps_tombstone_key_when_newer_version_exists() {
        let mut memtable = Memtable::default();
        memtable.apply(b"k", 1, Some(Bytes::from_static(b"v1")));
        memtable.apply(b"k", 2, None); // tombstone
        memtable.apply(b"k", 3, Some(Bytes::from_static(b"v3"))); // resurrection
        let metrics = memtable.gc_old_versions(2);
        assert_eq!(metrics.tombstone_keys_removed, 0);
        let chain = memtable.rows.get(&b"k"[..]).expect("key must survive");
        // version 1 dropped (below anchor 2), versions 2 (tombstone) and 3 kept
        assert_eq!(chain.len(), 2);
        assert_eq!(chain[0].version, 2);
        assert_eq!(chain[1].version, 3);
    }

    #[test]
    fn gc_does_not_drop_versions_at_or_above_watermark() {
        let mut memtable = Memtable::default();
        memtable.apply(b"k", 5, Some(Bytes::from_static(b"v5")));
        memtable.apply(b"k", 10, Some(Bytes::from_static(b"v10")));
        // Watermark below all versions: nothing to prune.
        let metrics = memtable.gc_old_versions(3);
        assert_eq!(metrics.versions_dropped, 0);
        assert_eq!(metrics.tombstone_keys_removed, 0);
        let chain = memtable.rows.get(&b"k"[..]).expect("key present");
        assert_eq!(chain.len(), 2);
    }

    #[test]
    fn apply_same_version_twice_replaces_not_duplicates() {
        let mut memtable = Memtable::default();
        memtable.apply(b"k", 5, Some(Bytes::from_static(b"first")));
        // Replaying the same version (WAL re-apply / replication convergence).
        memtable.apply(b"k", 5, Some(Bytes::from_static(b"second")));
        // Must have exactly one entry, not two.
        let chain = memtable.rows.get(&b"k"[..]).expect("key present");
        assert_eq!(
            chain.len(),
            1,
            "duplicate version should replace, not append"
        );
        assert_eq!(memtable.visible(b"k", 5), Some(&b"second"[..]));
        assert_eq!(memtable.latest_version(b"k"), Some(5));
    }

    #[test]
    fn apply_same_version_out_of_order_replaces_in_middle() {
        let mut memtable = Memtable::default();
        memtable.apply(b"k", 3, Some(Bytes::from_static(b"v3")));
        memtable.apply(b"k", 1, Some(Bytes::from_static(b"v1-orig")));
        // Re-apply version 1 (out-of-order duplicate).
        memtable.apply(b"k", 1, Some(Bytes::from_static(b"v1-updated")));
        let chain = memtable.rows.get(&b"k"[..]).expect("key present");
        assert_eq!(chain.len(), 2, "dedup should leave exactly two entries");
        assert_eq!(memtable.visible(b"k", 1), Some(&b"v1-updated"[..]));
        assert_eq!(memtable.visible(b"k", 3), Some(&b"v3"[..]));
    }

    #[test]
    fn referenced_blob_ids_collects_ids_from_versions() {
        let mut memtable = Memtable::default();
        memtable.apply(b"a", 1, Some(Bytes::from_static(b"blob:11")));
        memtable.apply(b"a", 2, Some(Bytes::from_static(b"blob:22")));
        memtable.apply(b"b", 1, Some(Bytes::from_static(b"inline")));
        memtable.apply(b"c", 1, None);

        let referenced = memtable.referenced_blob_ids(|value| {
            std::str::from_utf8(value)
                .ok()
                .and_then(|text| text.strip_prefix("blob:"))
                .and_then(|id| id.parse::<u64>().ok())
        });
        assert_eq!(referenced.into_iter().collect::<Vec<_>>(), vec![11, 22]);
    }
}
