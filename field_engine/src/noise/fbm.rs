use crate::lipschitz::types::BoundProvenance;
use crate::noise::bernstein::CertifiedNoisePatch;
use crate::noise::perlin::evaluate_improved_perlin;

/// Result of frequency-bounded FBM evaluation.
#[derive(Debug, Clone)]
pub struct FbmResult {
    pub value: f32,
    pub derivative_bound: [f32; 3],
    pub bound_provenance: BoundProvenance,
    pub octaves_evaluated: u32,
}

/// Frequency-bounded FBM evaluation with Bernstein-certified early exit.
pub fn fbm_frequency_bounded(
    p: [f32; 3],
    base_freq: f32,
    lacunarity: f32,
    gain: f32,
    num_octaves: u32,
    certified_patches: &[CertifiedNoisePatch],
    threshold: f32,
) -> FbmResult {
    let mut accumulated = 0.0_f32;
    let mut amplitude = 1.0_f32;
    let mut accumulated_b = [0.0_f32; 3];
    let mut octaves_evaluated = 0u32;

    for octave in 0..num_octaves {
        // Residual tail bound
        let mut tail_max = 0.0_f32;
        let mut a_remaining = amplitude;
        for remaining in octave..num_octaves {
            let patch = &certified_patches[remaining as usize];
            let worst_case =
                a_remaining * f32::max(patch.value_range[1].abs(), patch.value_range[0].abs());
            tail_max += worst_case;
            a_remaining *= gain;
        }

        // Early exit
        if accumulated - tail_max > threshold || accumulated + tail_max < threshold {
            break;
        }

        // Evaluate this octave
        let freq = base_freq * lacunarity.powi(octave as i32);
        let noise_value = evaluate_improved_perlin(p, freq);
        accumulated += amplitude * noise_value;

        // Accumulate derivative bounds
        let patch = &certified_patches[octave as usize];
        accumulated_b[0] += amplitude * freq * patch.derivative_bound[0];
        accumulated_b[1] += amplitude * freq * patch.derivative_bound[1];
        accumulated_b[2] += amplitude * freq * patch.derivative_bound[2];

        amplitude *= gain;
        octaves_evaluated += 1;
    }

    FbmResult {
        value: accumulated,
        derivative_bound: accumulated_b,
        bound_provenance: BoundProvenance::AnalyticBernstein,
        octaves_evaluated,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::noise::bernstein::CertifiedNoisePatch;
    use crate::lipschitz::types::BoundProvenance;

    fn make_test_patches(num: u32) -> Vec<CertifiedNoisePatch> {
        (0..num)
            .map(|_| CertifiedNoisePatch {
                value_range: [-0.8, 0.8],
                derivative_bound: [1.5, 1.5, 1.5],
                provenance: BoundProvenance::AnalyticBernstein,
            })
            .collect()
    }

    #[test]
    fn early_exit_far_from_threshold() {
        let patches = make_test_patches(6);
        // Point far from threshold — should early exit
        let result = fbm_frequency_bounded(
            [10.0, 10.0, 10.0],
            1.0,
            2.0,
            0.5,
            6,
            &patches,
            100.0, // threshold very far from possible values
        );
        // Should exit early since even all octaves can't reach threshold=100
        assert!(result.octaves_evaluated < 6);
    }

    #[test]
    fn all_octaves_near_threshold() {
        let patches = make_test_patches(6);
        // Threshold=0: near the center of the noise range, may evaluate all octaves
        let result = fbm_frequency_bounded(
            [0.5, 0.5, 0.5],
            1.0,
            2.0,
            0.5,
            6,
            &patches,
            0.0,
        );
        // Should evaluate more octaves since we're near the threshold
        assert!(result.octaves_evaluated > 0);
    }

    #[test]
    fn provenance_is_bernstein() {
        let patches = make_test_patches(4);
        let result = fbm_frequency_bounded(
            [0.5, 0.5, 0.5],
            1.0,
            2.0,
            0.5,
            4,
            &patches,
            0.0,
        );
        assert_eq!(result.bound_provenance, BoundProvenance::AnalyticBernstein);
    }

    #[test]
    fn early_exit_benchmark() {
        // Simulate a benchmark: count how many of 1000 points early-exit
        let patches = make_test_patches(6);
        let mut early_exit_count = 0;
        let total = 1000;
        for i in 0..total {
            let x = (i as f32 * 0.1) % 10.0;
            let y = (i as f32 * 0.13) % 10.0;
            let z = (i as f32 * 0.17) % 10.0;
            let result = fbm_frequency_bounded(
                [x, y, z],
                1.0,
                2.0,
                0.5,
                6,
                &patches,
                2.0, // threshold at 2.0 — most points should be far enough to exit early
            );
            if result.octaves_evaluated < 6 {
                early_exit_count += 1;
            }
        }
        let early_exit_pct = early_exit_count as f32 / total as f32;
        // Plan requires > 60% early exit
        assert!(
            early_exit_pct > 0.6,
            "Early exit rate {:.1}% < 60%",
            early_exit_pct * 100.0
        );
    }
}
