use crate::db::lsm::sstable::{SsTableEntry, estimated_entry_bytes};
use std::collections::BTreeMap;

#[derive(Debug, Default, Clone)]
pub struct CompactionMetrics {
    pub debt_bytes: u64,
    pub bytes_in: u64,
    pub bytes_out: u64,
}

pub fn merge_overlapping_runs(
    runs_newest_to_oldest: &[Vec<SsTableEntry>],
) -> (Vec<SsTableEntry>, CompactionMetrics) {
    let mut best_per_key: BTreeMap<Vec<u8>, (usize, SsTableEntry)> = BTreeMap::new();
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
    for (_, (_, entry)) in best_per_key {
        bytes_out = bytes_out.saturating_add(estimated_entry_bytes(&entry) as u64);
        merged.push(entry);
    }
    merged.sort_by(|a, b| a.key.cmp(&b.key));

    let metrics = CompactionMetrics {
        debt_bytes: bytes_in.saturating_sub(bytes_out),
        bytes_in,
        bytes_out,
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

        let (merged, metrics) = merge_overlapping_runs(&[run0_newest, run1_older, run2_oldest]);

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
    }
}
