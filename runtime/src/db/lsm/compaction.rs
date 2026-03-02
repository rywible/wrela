use crate::db::lsm::sstable::{SsTableEntry, estimated_entry_bytes};
use bytes::Bytes;
use std::collections::HashMap;

#[derive(Debug, Default, Clone)]
pub struct CompactionMetrics {
    pub debt_bytes: u64,
    pub bytes_in: u64,
    pub bytes_out: u64,
    /// Number of tombstone entries elided because `is_full_compaction=true`
    /// and the tombstone was eligible for collection.
    pub tombstones_elided: u64,
}

/// Merge overlapping sorted runs into a single sorted run.
///
/// `runs_newest_to_oldest` must be ordered from newest run (index 0) to oldest.
///
/// ## Tombstone safety contract
///
/// A tombstone entry shadows live entries for the same key that exist in levels
/// NOT included in this compaction. To prevent data resurrection:
///
/// - When `is_full_compaction = false` (partial compaction): tombstones are
///   **always** preserved in the output, even if they would otherwise qualify
///   for collection based on watermark checks. There may be live entries in
///   un-compacted lower levels that depend on the tombstone for shadowing.
///
/// - When `is_full_compaction = true` (all levels covered): tombstones at the
///   output may be elided, because no lower-level live entries can exist after
///   a full-level merge. Elision is opt-in via `tombstone_gc_watermark`.
///
/// `tombstone_gc_watermark`: if `Some(watermark)` AND `is_full_compaction =
/// true`, tombstone entries whose version is <= the watermark are dropped from
/// the output (they can never be seen by active readers and have no lower-level
/// live entries to shadow).
pub fn merge_overlapping_runs(
    runs_newest_to_oldest: &[Vec<SsTableEntry>],
    is_full_compaction: bool,
    tombstone_gc_watermark: Option<u64>,
) -> (Vec<SsTableEntry>, CompactionMetrics) {
    let mut best_per_key: HashMap<Bytes, (usize, SsTableEntry)> = HashMap::new();
    let mut bytes_in = 0u64;

    for (run_idx, run) in runs_newest_to_oldest.iter().enumerate() {
        for entry in run {
            bytes_in = bytes_in.saturating_add(estimated_entry_bytes(entry) as u64);

            match best_per_key.get(&entry.key) {
                None => {
                    best_per_key.insert(entry.key.clone(), (run_idx, entry.clone()));
                }
                Some((best_run_idx, best_entry)) => {
                    if entry.version > best_entry.version
                        || (entry.version == best_entry.version && run_idx < *best_run_idx)
                    {
                        best_per_key.insert(entry.key.clone(), (run_idx, entry.clone()));
                    }
                }
            }
        }
    }

    let mut merged = Vec::with_capacity(best_per_key.len());
    let mut bytes_out = 0u64;
    let mut tombstones_elided = 0u64;

    let can_elide_tombstones = is_full_compaction && tombstone_gc_watermark.is_some();
    let watermark = tombstone_gc_watermark.unwrap_or(0);

    for (_, (_, entry)) in best_per_key {
        if can_elide_tombstones && entry.is_tombstone() && entry.version <= watermark {
            // Safe to drop: full compaction means no lower-level live entries
            // exist, and the tombstone is below the reader watermark.
            tombstones_elided = tombstones_elided.saturating_add(1);
            continue;
        }
        bytes_out = bytes_out.saturating_add(estimated_entry_bytes(&entry) as u64);
        merged.push(entry);
    }
    merged.sort_by(|a, b| a.key.cmp(&b.key));

    let metrics = CompactionMetrics {
        debt_bytes: bytes_in.saturating_sub(bytes_out),
        bytes_in,
        bytes_out,
        tombstones_elided,
    };
    (merged, metrics)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compaction_keeps_newest_version_and_tombstone_over_overlap() {
        let run0_newest = vec![
            SsTableEntry::live(b"a".to_vec(), 7, b"a7".to_vec(), None),
            SsTableEntry::tombstone(b"b".to_vec(), 5),
            SsTableEntry::live(b"d".to_vec(), 2, b"d2".to_vec(), None),
        ];
        let run1_older = vec![
            SsTableEntry::live(b"a".to_vec(), 6, b"a6".to_vec(), None),
            SsTableEntry::live(b"b".to_vec(), 4, b"b4".to_vec(), None),
            SsTableEntry::live(b"c".to_vec(), 3, b"c3".to_vec(), None),
        ];
        let run2_oldest = vec![SsTableEntry::tombstone(b"c".to_vec(), 1)];

        // Partial compaction: tombstones must be preserved.
        let (merged, metrics) =
            merge_overlapping_runs(&[run0_newest, run1_older, run2_oldest], false, None);

        assert_eq!(merged.len(), 4);
        assert_eq!(
            merged[0],
            SsTableEntry::live(b"a".to_vec(), 7, b"a7".to_vec(), None)
        );
        assert_eq!(merged[1], SsTableEntry::tombstone(b"b".to_vec(), 5));
        assert_eq!(
            merged[2],
            SsTableEntry::live(b"c".to_vec(), 3, b"c3".to_vec(), None)
        );
        assert_eq!(
            merged[3],
            SsTableEntry::live(b"d".to_vec(), 2, b"d2".to_vec(), None)
        );
        assert!(metrics.bytes_in > metrics.bytes_out);
        assert_eq!(metrics.tombstones_elided, 0);
    }

    #[test]
    fn partial_compaction_preserves_tombstones_below_watermark() {
        // Even if the tombstone version is below the watermark, partial
        // compaction must keep it to avoid shadowing a live entry in a
        // lower level not included in this merge.
        let run0 = vec![
            SsTableEntry::tombstone(b"x".to_vec(), 3),
            SsTableEntry::live(b"y".to_vec(), 5, b"v".to_vec(), None),
        ];
        let (merged, metrics) = merge_overlapping_runs(
            &[run0],
            false,    // NOT a full compaction
            Some(10), // watermark is above the tombstone version
        );
        assert_eq!(merged.len(), 2, "tombstone must survive partial compaction");
        assert!(merged.iter().any(|e| e.is_tombstone()));
        assert_eq!(metrics.tombstones_elided, 0);
    }

    #[test]
    fn full_compaction_elides_tombstones_below_watermark() {
        // Full compaction: tombstone at version 3, watermark = 10.
        // The tombstone version <= watermark and this is a full compaction,
        // so the tombstone can be dropped.
        let run0_newest = vec![
            SsTableEntry::tombstone(b"deleted".to_vec(), 3),
            SsTableEntry::live(b"alive".to_vec(), 5, b"v".to_vec(), None),
        ];
        let (merged, metrics) = merge_overlapping_runs(
            &[run0_newest],
            true,     // full compaction
            Some(10), // watermark well above tombstone version
        );
        assert_eq!(
            merged.len(),
            1,
            "tombstone must be elided in full compaction"
        );
        assert!(!merged[0].is_tombstone());
        assert_eq!(merged[0].key, &b"alive"[..]);
        assert_eq!(metrics.tombstones_elided, 1);
    }

    #[test]
    fn full_compaction_preserves_tombstone_above_watermark() {
        // Full compaction, but tombstone version > watermark: must be kept
        // (active readers at versions up to the watermark could still need it).
        let run0 = vec![
            SsTableEntry::tombstone(b"k".to_vec(), 20),
            SsTableEntry::live(b"m".to_vec(), 5, b"v".to_vec(), None),
        ];
        let (merged, metrics) = merge_overlapping_runs(
            &[run0],
            true,     // full compaction
            Some(10), // watermark BELOW the tombstone version
        );
        assert_eq!(merged.len(), 2);
        assert!(merged.iter().any(|e| e.is_tombstone()));
        assert_eq!(metrics.tombstones_elided, 0);
    }
}
