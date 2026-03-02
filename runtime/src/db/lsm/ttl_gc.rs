use crate::db::lsm::sstable::SsTableEntry;
use bytes::Bytes;
use std::collections::BTreeMap;

#[derive(Debug, Default, Clone)]
pub struct TtlGcMetrics {
    pub tombstones_reclaimed: u64,
    pub ttl_expired_reclaimed: u64,
}

pub fn can_reclaim_entry(
    entry: &SsTableEntry,
    now_ms: u64,
    snapshot_version_watermark: u64,
) -> bool {
    if entry.version > snapshot_version_watermark {
        return false;
    }
    entry.is_tombstone() || entry.is_ttl_expired(now_ms)
}

pub fn reclaim_tombstones_and_ttl(
    entries: Vec<SsTableEntry>,
    now_ms: u64,
    snapshot_version_watermark: u64,
) -> (Vec<SsTableEntry>, TtlGcMetrics) {
    let mut histories: BTreeMap<Bytes, Vec<SsTableEntry>> = BTreeMap::new();
    for entry in entries {
        histories.entry(entry.key.clone()).or_default().push(entry);
    }

    let mut kept = Vec::new();
    let mut metrics = TtlGcMetrics::default();

    for (_, mut versions) in histories {
        versions.sort_by(|a, b| b.version.cmp(&a.version));
        let newest = versions.first().expect("non-empty versions");

        if can_reclaim_entry(newest, now_ms, snapshot_version_watermark) {
            if newest.is_tombstone() {
                metrics.tombstones_reclaimed = metrics.tombstones_reclaimed.saturating_add(1);
            } else {
                metrics.ttl_expired_reclaimed = metrics.ttl_expired_reclaimed.saturating_add(1);
            }
            continue;
        }

        kept.extend(versions.into_iter());
    }

    kept.sort_by(|a, b| a.key.cmp(&b.key).then_with(|| b.version.cmp(&a.version)));
    (kept, metrics)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tombstone_gc_drops_shadowed_history_without_resurrection() {
        let input = vec![
            SsTableEntry::tombstone(b"k".to_vec(), 10),
            SsTableEntry::live(b"k".to_vec(), 9, b"old".to_vec(), None),
            SsTableEntry::live(b"keep".to_vec(), 11, b"v".to_vec(), None),
        ];

        let (kept, metrics) = reclaim_tombstones_and_ttl(input, 0, 10);
        assert_eq!(metrics.tombstones_reclaimed, 1);
        assert_eq!(metrics.ttl_expired_reclaimed, 0);
        assert_eq!(kept.len(), 1);
        assert_eq!(
            kept[0],
            SsTableEntry::live(b"keep".to_vec(), 11, b"v".to_vec(), None)
        );
    }

    #[test]
    fn ttl_gc_drops_shadowed_history_without_resurrection() {
        let input = vec![
            SsTableEntry::live(b"k".to_vec(), 8, b"exp".to_vec(), Some(50)),
            SsTableEntry::live(b"k".to_vec(), 7, b"older".to_vec(), None),
            SsTableEntry::live(b"keep".to_vec(), 11, b"v".to_vec(), None),
        ];

        let (kept, metrics) = reclaim_tombstones_and_ttl(input, 100, 8);
        assert_eq!(metrics.tombstones_reclaimed, 0);
        assert_eq!(metrics.ttl_expired_reclaimed, 1);
        assert_eq!(kept.len(), 1);
        assert_eq!(
            kept[0],
            SsTableEntry::live(b"keep".to_vec(), 11, b"v".to_vec(), None)
        );
    }

    #[test]
    fn watermark_blocks_gc_when_snapshot_can_still_see_entry() {
        let input = vec![
            SsTableEntry::tombstone(b"k".to_vec(), 10),
            SsTableEntry::live(b"k".to_vec(), 9, b"old".to_vec(), None),
        ];

        let (kept, metrics) = reclaim_tombstones_and_ttl(input, 0, 9);
        assert_eq!(metrics.tombstones_reclaimed, 0);
        assert_eq!(metrics.ttl_expired_reclaimed, 0);
        assert_eq!(kept.len(), 2);
    }
}
