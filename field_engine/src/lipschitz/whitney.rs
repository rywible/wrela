/// Whitney-style quadratic envelope reconstruction (Medium+ quality).
/// Uses stored gradients (with magnitude) and Hessian operator-norm bound
/// for tighter bounds than McShane C^0.
pub fn whitney_c1_envelope(
    p: [f32; 3],
    samples: &[([f32; 3], f32, [f32; 3], f32, f32)], // (position, lower_bound, unit_normal, _unused, gradient_magnitude)
    k_cell: f32, // Hessian operator-norm bound
) -> f32 {
    let mut b_env = f32::NEG_INFINITY;
    let mut max_abs_term = 1.0_f32;
    for &(x_i, b_i, n_i, _, g_mag_i) in samples {
        let dx = [p[0] - x_i[0], p[1] - x_i[1], p[2] - x_i[2]];
        let dot_grad_dx =
            g_mag_i * (n_i[0] * dx[0] + n_i[1] * dx[1] + n_i[2] * dx[2]);
        let dist_sq = dx[0] * dx[0] + dx[1] * dx[1] + dx[2] * dx[2];
        let quad = (k_cell / 2.0) * dist_sq;
        let candidate = b_i + dot_grad_dx - quad;
        b_env = f32::max(b_env, candidate);
        max_abs_term = max_abs_term
            .max(b_i.abs())
            .max(dot_grad_dx.abs())
            .max(quad.abs())
            .max(candidate.abs());
    }
    let pad = 2.0 * f32::EPSILON * max_abs_term.max(b_env.abs());
    b_env - pad
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lipschitz::envelope::lipschitz_envelope;

    fn sphere_distance(p: [f32; 3]) -> f32 {
        (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt() - 1.0
    }

    fn sphere_gradient(p: [f32; 3]) -> ([f32; 3], f32) {
        let r = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
        if r < 1e-10 {
            return ([0.0, 0.0, 1.0], 0.0);
        }
        let n = [p[0] / r, p[1] / r, p[2] / r];
        (n, 1.0) // gradient magnitude is 1 for SDF
    }

    #[test]
    fn whitney_conservative_on_sphere() {
        // Unit sphere, K_cell = 1.0 (curvature of unit sphere)
        let k_cell = 1.0;

        let mut samples = Vec::new();
        for ix in -2..=2 {
            for iy in -2..=2 {
                for iz in -2..=2 {
                    let p = [ix as f32 * 0.5, iy as f32 * 0.5, iz as f32 * 0.5];
                    let d = sphere_distance(p);
                    let (n, g_mag) = sphere_gradient(p);
                    samples.push((p, d, n, 0.0, g_mag));
                }
            }
        }

        // Dense-sample conservatism
        for ix in -10..=10 {
            for iy in -10..=10 {
                for iz in -10..=10 {
                    let p = [ix as f32 * 0.2, iy as f32 * 0.2, iz as f32 * 0.2];
                    let true_val = sphere_distance(p);
                    let env_val = whitney_c1_envelope(p, &samples, k_cell);
                    assert!(
                        env_val <= true_val + 1e-4,
                        "Whitney not conservative at {:?}: env={} > true={}",
                        p, env_val, true_val
                    );
                }
            }
        }
    }

    #[test]
    fn whitney_tighter_than_mcshane() {
        let k_cell = 1.0;
        let b_cell = [1.0, 1.0, 1.0];

        let mut whitney_samples = Vec::new();
        let mut mcshane_samples = Vec::new();
        for ix in -2..=2 {
            for iy in -2..=2 {
                for iz in -2..=2 {
                    let p = [ix as f32 * 0.5, iy as f32 * 0.5, iz as f32 * 0.5];
                    let d = sphere_distance(p);
                    let (n, g_mag) = sphere_gradient(p);
                    whitney_samples.push((p, d, n, 0.0, g_mag));
                    mcshane_samples.push((p, d));
                }
            }
        }

        // Whitney should be tighter (larger lower bound) at many points
        let mut whitney_tighter_count = 0;
        for ix in -5..=5 {
            for iy in -5..=5 {
                for iz in -5..=5 {
                    let p = [ix as f32 * 0.3, iy as f32 * 0.3, iz as f32 * 0.3];
                    let w = whitney_c1_envelope(p, &whitney_samples, k_cell);
                    let m = lipschitz_envelope(p, &mcshane_samples, b_cell);
                    if w > m + 1e-6 {
                        whitney_tighter_count += 1;
                    }
                }
            }
        }
        let total_points = 11 * 11 * 11;
        let fraction = whitney_tighter_count as f64 / total_points as f64;
        // Whitney should be tighter at >= 20% of interior test points
        assert!(
            fraction >= 0.20,
            "Whitney tighter at only {:.1}% of points (need >= 20%)",
            fraction * 100.0
        );
    }

    #[test]
    fn whitney_average_improvement_over_mcshane() {
        let k_cell = 1.0;
        let b_cell = [1.0, 1.0, 1.0];

        let mut whitney_samples = Vec::new();
        let mut mcshane_samples = Vec::new();
        for ix in -3..=3 {
            for iy in -3..=3 {
                for iz in -3..=3 {
                    let p = [ix as f32 * 0.4, iy as f32 * 0.4, iz as f32 * 0.4];
                    let d = sphere_distance(p);
                    let (n, g_mag) = sphere_gradient(p);
                    whitney_samples.push((p, d, n, 0.0, g_mag));
                    mcshane_samples.push((p, d));
                }
            }
        }

        // Measure average improvement where Whitney is tighter
        let mut total_improvement = 0.0_f64;
        let mut tighter_count = 0;
        let mut total_count = 0;
        for ix in -8..=8 {
            for iy in -8..=8 {
                for iz in -8..=8 {
                    let p = [ix as f32 * 0.15, iy as f32 * 0.15, iz as f32 * 0.15];
                    let true_val = sphere_distance(p);
                    let w = whitney_c1_envelope(p, &whitney_samples, k_cell);
                    let m = lipschitz_envelope(p, &mcshane_samples, b_cell);
                    total_count += 1;

                    // Both should be conservative
                    assert!(
                        w <= true_val + 1e-4,
                        "Whitney not conservative at {:?}: w={} > true={}",
                        p, w, true_val
                    );

                    if w > m + 1e-6 {
                        tighter_count += 1;
                        // Measure how much closer Whitney is to true value
                        let gap_m = (true_val - m) as f64;
                        let gap_w = (true_val - w) as f64;
                        if gap_m > 1e-6 {
                            total_improvement += 1.0 - (gap_w / gap_m);
                        }
                    }
                }
            }
        }

        let fraction_tighter = tighter_count as f64 / total_count as f64;
        let avg_improvement = if tighter_count > 0 {
            total_improvement / tighter_count as f64
        } else {
            0.0
        };

        // Whitney should be tighter at a meaningful fraction of points
        assert!(
            fraction_tighter >= 0.15,
            "Whitney tighter at only {:.1}% (need >= 15%)",
            fraction_tighter * 100.0
        );
        // Where tighter, average gap reduction should be meaningful (> 5%)
        assert!(
            avg_improvement >= 0.05,
            "Average improvement only {:.1}% (need >= 5%)",
            avg_improvement * 100.0
        );
    }

    #[test]
    fn whitney_g_mag_less_than_one() {
        // Test with gradient magnitude < 1 (not a true SDF)
        let k_cell = 2.0;
        let samples = vec![
            ([0.0, 0.0, 0.0], 1.0, [1.0, 0.0, 0.0], 0.0, 0.5), // g_mag = 0.5
        ];
        let env = whitney_c1_envelope([0.1, 0.0, 0.0], &samples, k_cell);
        // With g_mag=0.5: b + 0.5*(1.0*0.1) - (2/2)*0.01 = 1 + 0.05 - 0.01 = 1.04
        assert!(env > 0.0, "Should produce positive lower bound");
    }
}
