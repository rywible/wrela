const FNV_OFFSET_BASIS_64: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME_64: u64 = 0x0000_0100_0000_01b3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BubbleTier {
    Dormant,
    Warm,
    Active,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BubbleThresholds {
    pub warm_enter_mm: u32,
    pub warm_exit_mm: u32,
    pub active_enter_mm: u32,
    pub active_exit_mm: u32,
}

impl Default for BubbleThresholds {
    fn default() -> Self {
        Self {
            warm_enter_mm: 32_000,
            warm_exit_mm: 36_000,
            active_enter_mm: 18_000,
            active_exit_mm: 22_000,
        }
    }
}

pub fn reconcile_bubble_tier(
    current: BubbleTier,
    viewer_distance_mm: u32,
    thresholds: BubbleThresholds,
) -> BubbleTier {
    match current {
        BubbleTier::Dormant => {
            if viewer_distance_mm <= thresholds.warm_enter_mm {
                BubbleTier::Warm
            } else {
                BubbleTier::Dormant
            }
        }
        BubbleTier::Warm => {
            if viewer_distance_mm <= thresholds.active_enter_mm {
                BubbleTier::Active
            } else if viewer_distance_mm > thresholds.warm_exit_mm {
                BubbleTier::Dormant
            } else {
                BubbleTier::Warm
            }
        }
        BubbleTier::Active => {
            if viewer_distance_mm > thresholds.active_exit_mm {
                BubbleTier::Warm
            } else {
                BubbleTier::Active
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FloraPatchState {
    pub patch_id: u32,
    pub moisture_bp: u16,
    pub nutrient_bp: u16,
    pub bloom_seed: u64,
}

fn fnv1a64_bytes(mut hash: u64, bytes: &[u8]) -> u64 {
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME_64);
    }
    hash
}

pub fn deterministic_patch_hash(patches: &[FloraPatchState]) -> u64 {
    let mut ordered = patches.to_vec();
    ordered.sort_by_key(|patch| patch.patch_id);

    let mut hash = FNV_OFFSET_BASIS_64;
    hash = fnv1a64_bytes(
        hash,
        &(ordered.len().min(u32::MAX as usize) as u32).to_le_bytes(),
    );
    for patch in &ordered {
        hash = fnv1a64_bytes(hash, &patch.patch_id.to_le_bytes());
        hash = fnv1a64_bytes(hash, &patch.moisture_bp.to_le_bytes());
        hash = fnv1a64_bytes(hash, &patch.nutrient_bp.to_le_bytes());
        hash = fnv1a64_bytes(hash, &patch.bloom_seed.to_le_bytes());
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::{
        BubbleThresholds, BubbleTier, FloraPatchState,
        deterministic_patch_hash as compute_deterministic_patch_hash, reconcile_bubble_tier,
    };

    #[test]
    fn bubble_tier_switch_stability() {
        let thresholds = BubbleThresholds::default();
        let mut tier = BubbleTier::Warm;
        let jitter_window_mm = [
            17_900, 18_050, 17_950, 18_120, 18_010, 21_800, 21_900, 21_950, 21_910,
        ];

        let mut transitions = 0usize;
        for distance in jitter_window_mm {
            let next = reconcile_bubble_tier(tier, distance, thresholds);
            if next != tier {
                transitions += 1;
            }
            tier = next;
        }

        assert_eq!(tier, BubbleTier::Active);
        assert!(transitions <= 1);
    }

    #[test]
    fn deterministic_patch_hash() {
        let patch_a = vec![
            FloraPatchState {
                patch_id: 5,
                moisture_bp: 5300,
                nutrient_bp: 4100,
                bloom_seed: 0xAA55AA55,
            },
            FloraPatchState {
                patch_id: 2,
                moisture_bp: 4700,
                nutrient_bp: 5000,
                bloom_seed: 0x10101010,
            },
            FloraPatchState {
                patch_id: 9,
                moisture_bp: 4900,
                nutrient_bp: 5050,
                bloom_seed: 0xDEADBEEF,
            },
        ];
        let patch_b = vec![patch_a[2], patch_a[0], patch_a[1]];

        assert_eq!(
            compute_deterministic_patch_hash(&patch_a),
            compute_deterministic_patch_hash(&patch_b)
        );
    }
}
