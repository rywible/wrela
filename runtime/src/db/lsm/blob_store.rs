use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValuePlacement {
    Inline(Vec<u8>),
    BlobRef { blob_id: u64, len_bytes: u32 },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BlobGcMetrics {
    pub gc_runs: u64,
    pub reclaimed_bytes: u64,
}

#[derive(Debug, Default)]
pub struct BlobStore {
    next_blob_id: u64,
    blobs: BTreeMap<u64, Vec<u8>>,
    metrics: BlobGcMetrics,
}

impl BlobStore {
    pub fn separate_value(&mut self, value: Vec<u8>, threshold_bytes: usize) -> ValuePlacement {
        if value.len() < threshold_bytes.max(1) {
            return ValuePlacement::Inline(value);
        }
        let blob_id = self.next_blob_id;
        self.next_blob_id = self.next_blob_id.saturating_add(1);
        let len_bytes = value.len().min(u32::MAX as usize) as u32;
        self.blobs.insert(blob_id, value);
        ValuePlacement::BlobRef { blob_id, len_bytes }
    }

    pub fn read(&self, placement: &ValuePlacement) -> Option<Vec<u8>> {
        match placement {
            ValuePlacement::Inline(value) => Some(value.clone()),
            ValuePlacement::BlobRef { blob_id, .. } => self.blobs.get(blob_id).cloned(),
        }
    }

    pub fn gc_unreferenced(&mut self, referenced_blob_ids: &BTreeSet<u64>) -> u64 {
        self.metrics.gc_runs = self.metrics.gc_runs.saturating_add(1);
        let mut reclaimed = 0u64;
        self.blobs.retain(|blob_id, payload| {
            if referenced_blob_ids.contains(blob_id) {
                return true;
            }
            reclaimed = reclaimed.saturating_add(payload.len() as u64);
            false
        });
        self.metrics.reclaimed_bytes = self.metrics.reclaimed_bytes.saturating_add(reclaimed);
        reclaimed
    }

    pub fn metrics(&self) -> BlobGcMetrics {
        self.metrics
    }

    pub fn blob_count(&self) -> usize {
        self.blobs.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_values_stay_inline() {
        let mut store = BlobStore::default();
        let placement = store.separate_value(b"tiny".to_vec(), 16);
        assert_eq!(placement, ValuePlacement::Inline(b"tiny".to_vec()));
        assert_eq!(store.blob_count(), 0);
    }

    #[test]
    fn large_values_externalize_and_round_trip() {
        let mut store = BlobStore::default();
        let placement = store.separate_value(vec![7u8; 64], 32);
        let ValuePlacement::BlobRef { blob_id, len_bytes } = placement else {
            panic!("expected blob ref");
        };
        assert_eq!(blob_id, 0);
        assert_eq!(len_bytes, 64);
        assert_eq!(store.blob_count(), 1);
        assert_eq!(
            store.read(&ValuePlacement::BlobRef { blob_id, len_bytes }),
            Some(vec![7u8; 64])
        );
    }

    #[test]
    fn gc_reclaims_only_unreferenced_blobs() {
        let mut store = BlobStore::default();
        let keep = store.separate_value(vec![1u8; 40], 8);
        let drop = store.separate_value(vec![2u8; 20], 8);
        let keep_id = match keep {
            ValuePlacement::BlobRef { blob_id, .. } => blob_id,
            _ => panic!("expected blob ref"),
        };
        let drop_id = match drop {
            ValuePlacement::BlobRef { blob_id, .. } => blob_id,
            _ => panic!("expected blob ref"),
        };
        let referenced = BTreeSet::from([keep_id]);
        let reclaimed = store.gc_unreferenced(&referenced);
        assert_eq!(reclaimed, 20);
        assert_eq!(store.blob_count(), 1);
        assert!(
            store
                .read(&ValuePlacement::BlobRef {
                    blob_id: drop_id,
                    len_bytes: 20
                })
                .is_none()
        );
        let metrics = store.metrics();
        assert_eq!(metrics.gc_runs, 1);
        assert_eq!(metrics.reclaimed_bytes, 20);
    }
}
