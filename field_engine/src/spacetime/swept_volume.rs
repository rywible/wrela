/// Evaluate swept-volume lower bound at a spatial point over shutter interval.
/// Returns the exact minimum over t of the sampled 1D McShane lower envelope.
pub fn swept_volume_lower_bound(
    time_samples: &[(f32, f32)],
    l_time: f32,
    t0: f32,
    t1: f32,
) -> f32 {
    // Quick inside check
    for &(_, g_i) in time_samples {
        if g_i <= 0.0 {
            return g_i;
        }
    }

    // Build candidate set
    let mut candidates: Vec<f32> = vec![t0, t1];
    for &(t_i, _) in time_samples {
        if t_i >= t0 && t_i <= t1 {
            candidates.push(t_i);
        }
    }

    // Pairwise branch intersections
    for &(t_i, g_i) in time_samples {
        for &(t_j, g_j) in time_samples {
            let denom = 2.0 * l_time;
            if denom > 0.0 {
                // left(i) vs right(j)
                let t_lr = (g_j + l_time * t_j - (g_i - l_time * t_i)) / denom;
                if t_lr >= t0 && t_lr <= t1 && t_lr <= t_i && t_lr >= t_j {
                    candidates.push(t_lr);
                }
                // right(i) vs left(j)
                let t_rl = (g_i + l_time * t_i - (g_j - l_time * t_j)) / denom;
                if t_rl >= t0 && t_rl <= t1 && t_rl >= t_i && t_rl <= t_j {
                    candidates.push(t_rl);
                }
            }
        }
    }

    let mut min_val = f32::INFINITY;
    for &t in &candidates {
        let mut h_t = f32::NEG_INFINITY;
        for &(t_i, g_i) in time_samples {
            h_t = f32::max(h_t, g_i - l_time * (t - t_i).abs());
        }
        min_val = f32::min(min_val, h_t);
    }
    min_val
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_sample_outside() {
        // One sample at t=0.5 with g=2.0, L=1.0, interval [0, 1]
        let result = swept_volume_lower_bound(&[(0.5, 2.0)], 1.0, 0.0, 1.0);
        // Min of h(t): at t=0, h = 2.0 - 1.0*0.5 = 1.5
        //              at t=1, h = 2.0 - 1.0*0.5 = 1.5
        //              at t=0.5, h = 2.0
        assert!((result - 1.5).abs() < 1e-5);
    }

    #[test]
    fn sample_inside_returns_negative() {
        let result = swept_volume_lower_bound(&[(0.5, -1.0)], 1.0, 0.0, 1.0);
        assert!(result <= 0.0);
    }

    #[test]
    fn two_samples_valley() {
        // Two samples far apart in value
        let result =
            swept_volume_lower_bound(&[(0.0, 3.0), (1.0, 3.0)], 2.0, 0.0, 1.0);
        // Min is at t=0.5: max(3 - 2*0.5, 3 - 2*0.5) = 2.0
        assert!((result - 2.0).abs() < 1e-5);
    }

    #[test]
    fn linear_sphere_sweep() {
        // Sphere at radius=1, center moving along x. At point x=3:
        // g(t) = |3 - t| - 1  (center at t, radius 1)
        // For t in [0, 1]: g(0) = 2, g(1) = 1
        // L_time = 1 (derivative of distance w.r.t. center motion)
        let samples = vec![(0.0, 2.0), (0.5, 1.5), (1.0, 1.0)];
        let result = swept_volume_lower_bound(&samples, 1.0, 0.0, 1.0);
        // True minimum is g(1) = 1.0
        // McShane envelope minimum should be close to or below 1.0
        assert!(result <= 1.0 + 1e-5);
        assert!(result > 0.0); // Still outside
    }
}
