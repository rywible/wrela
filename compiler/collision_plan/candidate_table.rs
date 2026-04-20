use crate::collision_plan::CollisionCandidateGroupingPolicy;
use smol_str::SmolStr;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CollisionCandidateTable {
    pub shared_candidates: Vec<SmolStr>,
    pub item_ranges: Vec<(u32, u32)>,
    pub flat_candidate_indices: Vec<u32>,
    pub total_candidate_count: u64,
    pub total_rejected_candidate_count: u64,
    pub total_pruned_node_count: u64,
    pub overflowed: bool,
    pub overflow_fallback_item_count: u32,
}

impl CollisionCandidateTable {
    pub fn from_shared_candidates(
        shared_candidates: Vec<SmolStr>,
        item_count: usize,
        fixed_capacity: usize,
        total_candidate_count: u64,
        total_rejected_candidate_count: u64,
        total_pruned_node_count: u64,
    ) -> Self {
        let flat_candidate_indices = (0..shared_candidates.len() as u32).collect::<Vec<_>>();
        let overflowed = flat_candidate_indices.len() > fixed_capacity;
        let candidate_count = flat_candidate_indices.len() as u32;
        let item_ranges = if overflowed {
            Vec::new()
        } else {
            (0..item_count).map(|_| (0, candidate_count)).collect()
        };
        Self {
            shared_candidates,
            item_ranges,
            flat_candidate_indices: if overflowed {
                Vec::new()
            } else {
                flat_candidate_indices
            },
            total_candidate_count,
            total_rejected_candidate_count,
            total_pruned_node_count,
            overflowed,
            overflow_fallback_item_count: if overflowed { item_count as u32 } else { 0 },
        }
    }

    pub fn gpu_candidate_spans(&self) -> Vec<u32> {
        if self.overflowed || self.shared_candidates.is_empty() {
            return Vec::new();
        }
        let mut packed = Vec::with_capacity(
            self.item_ranges.len().saturating_mul(2) + self.flat_candidate_indices.len(),
        );
        for (start, len) in &self.item_ranges {
            packed.push(*start);
            packed.push(*len);
        }
        packed.extend(self.flat_candidate_indices.iter().copied());
        packed
    }

    pub fn average_candidate_count(&self, item_count: usize) -> u32 {
        if item_count == 0 {
            0
        } else {
            (self.total_candidate_count as f64 / item_count as f64)
                .round()
                .clamp(0.0, f64::from(u32::MAX)) as u32
        }
    }
}

pub(crate) fn representative_candidate_item_indices(
    item_count: usize,
    policy: CollisionCandidateGroupingPolicy,
) -> Vec<usize> {
    match policy {
        CollisionCandidateGroupingPolicy::PerItem => (0..item_count).collect(),
        CollisionCandidateGroupingPolicy::SharedCandidateDigest => {
            evenly_spaced_item_indices(item_count, 4)
        }
        CollisionCandidateGroupingPolicy::SharedBroadphaseRegion => {
            evenly_spaced_item_indices(item_count, 8)
        }
    }
}

fn evenly_spaced_item_indices(item_count: usize, sample_count: usize) -> Vec<usize> {
    if item_count <= sample_count {
        return (0..item_count).collect();
    }
    if sample_count <= 1 {
        return vec![0];
    }
    let last = item_count - 1;
    let mut indices = Vec::with_capacity(sample_count);
    for slot in 0..sample_count {
        let numerator = slot * last;
        let denominator = sample_count - 1;
        let index = (numerator + denominator / 2) / denominator;
        if indices.last().copied() != Some(index) {
            indices.push(index);
        }
    }
    indices
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn representative_candidate_item_indices_keep_per_item_batches_exact() {
        assert_eq!(
            representative_candidate_item_indices(5, CollisionCandidateGroupingPolicy::PerItem),
            vec![0, 1, 2, 3, 4]
        );
    }

    #[test]
    fn representative_candidate_item_indices_downsample_shared_groupings() {
        assert_eq!(
            representative_candidate_item_indices(
                10,
                CollisionCandidateGroupingPolicy::SharedCandidateDigest
            ),
            vec![0, 3, 6, 9]
        );
        assert_eq!(
            representative_candidate_item_indices(
                17,
                CollisionCandidateGroupingPolicy::SharedBroadphaseRegion
            ),
            vec![0, 2, 5, 7, 9, 11, 14, 16]
        );
    }
}
