/// Lipschitz envelope reconstruction (McShane/Whitney extension).

/// Computes the optimal (McShane) conservative lower bound at query point p
/// given stored lower-bound samples and derivative bounds.
/// Includes f32 ULP padding to absorb round-to-nearest arithmetic error.
pub fn lipschitz_envelope(
    p: [f32; 3],
    samples: &[([f32; 3], f32)],
    b_cell: [f32; 3],
) -> f32 {
    let mut b_env = f32::NEG_INFINITY;
    let mut max_abs_term = 1.0_f32;
    for &(x_i, b_i) in samples {
        let d_b = b_cell[0] * (p[0] - x_i[0]).abs()
            + b_cell[1] * (p[1] - x_i[1]).abs()
            + b_cell[2] * (p[2] - x_i[2]).abs();
        let candidate = b_i - d_b;
        b_env = f32::max(b_env, candidate);
        max_abs_term = max_abs_term
            .max(b_i.abs())
            .max(d_b.abs())
            .max(candidate.abs());
    }
    let pad = 2.0 * f32::EPSILON * max_abs_term.max(b_env.abs());
    b_env - pad
}

/// Upper envelope for uncertainty estimation.
/// Returns the tightest upper bound at p from stored upper-bound samples.
pub fn lipschitz_upper_envelope(
    p: [f32; 3],
    samples: &[([f32; 3], f32)],
    b_cell: [f32; 3],
) -> f32 {
    let mut u_env = f32::INFINITY;
    for &(x_i, u_i) in samples {
        let d_b = b_cell[0] * (p[0] - x_i[0]).abs()
            + b_cell[1] * (p[1] - x_i[1]).abs()
            + b_cell[2] * (p[2] - x_i[2]).abs();
        u_env = f32::min(u_env, u_i + d_b);
    }
    u_env
}

/// Separable Lipschitz closure (max-plus cone envelope) for mip construction.
/// Tightens 1D lower-bound array in-place along one axis.
pub fn separable_lipschitz_closure_1d(b: &mut [f32], w: f32) {
    let n = b.len();
    if n == 0 {
        return;
    }
    // Forward pass
    for j in 1..n {
        b[j] = f32::max(b[j], b[j - 1] - w);
    }
    // Backward pass
    for j in (0..n - 1).rev() {
        b[j] = f32::max(b[j], b[j + 1] - w);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sphere_distance(p: [f32; 3], center: [f32; 3], radius: f32) -> f32 {
        let dx = p[0] - center[0];
        let dy = p[1] - center[1];
        let dz = p[2] - center[2];
        (dx * dx + dy * dy + dz * dz).sqrt() - radius
    }

    #[test]
    fn envelope_conservative_on_sphere() {
        // Unit sphere centered at origin. Sample on a grid.
        let center = [0.0, 0.0, 0.0];
        let radius = 1.0;
        let b_cell = [1.0, 1.0, 1.0]; // sphere has L=1

        // Create sample grid
        let mut samples = Vec::new();
        for ix in -2..=2 {
            for iy in -2..=2 {
                for iz in -2..=2 {
                    let p = [ix as f32 * 0.5, iy as f32 * 0.5, iz as f32 * 0.5];
                    let d = sphere_distance(p, center, radius);
                    samples.push((p, d));
                }
            }
        }

        // Dense-sample and verify envelope is conservative (b_env <= f(p))
        for ix in -10..=10 {
            for iy in -10..=10 {
                for iz in -10..=10 {
                    let p = [ix as f32 * 0.2, iy as f32 * 0.2, iz as f32 * 0.2];
                    let true_val = sphere_distance(p, center, radius);
                    let env_val = lipschitz_envelope(p, &samples, b_cell);
                    assert!(
                        env_val <= true_val + 1e-5,
                        "Envelope not conservative at {:?}: env={} > true={}",
                        p,
                        env_val,
                        true_val
                    );
                }
            }
        }
    }

    #[test]
    fn upper_envelope_brackets_value() {
        let center = [0.0, 0.0, 0.0];
        let radius = 1.0;
        let b_cell = [1.0, 1.0, 1.0];

        let mut lower_samples = Vec::new();
        let mut upper_samples = Vec::new();
        for ix in -2..=2 {
            for iy in -2..=2 {
                for iz in -2..=2 {
                    let p = [ix as f32 * 0.5, iy as f32 * 0.5, iz as f32 * 0.5];
                    let d = sphere_distance(p, center, radius);
                    lower_samples.push((p, d));
                    upper_samples.push((p, d));
                }
            }
        }

        // Verify [b_env, u_env] brackets true value
        for ix in -5..=5 {
            for iy in -5..=5 {
                for iz in -5..=5 {
                    let p = [ix as f32 * 0.3, iy as f32 * 0.3, iz as f32 * 0.3];
                    let true_val = sphere_distance(p, center, radius);
                    let b = lipschitz_envelope(p, &lower_samples, b_cell);
                    let u = lipschitz_upper_envelope(p, &upper_samples, b_cell);
                    assert!(
                        b <= true_val + 1e-5,
                        "Lower envelope exceeds true at {:?}",
                        p
                    );
                    assert!(
                        u >= true_val - 1e-5,
                        "Upper envelope below true at {:?}",
                        p
                    );
                }
            }
        }
    }

    #[test]
    fn separable_closure_known_1d() {
        let mut b = vec![0.0, -10.0, -10.0, 5.0, -10.0];
        let w = 2.0;
        separable_lipschitz_closure_1d(&mut b, w);
        // After closure: each element should be max over all propagated cones
        // b[0] = max(0.0, 5.0 - 2*3) = max(0, -1) = 0
        // b[1] = max(-10, 0 - 2, 5.0 - 2*2) = max(-10, -2, 1) = 1
        // b[2] = max(-10, 1 - 2, 5.0 - 2) = max(-10, -1, 3) = 3
        // b[3] = max(5.0, 3 - 2, ...) = 5.0
        // b[4] = max(-10, 5 - 2) = 3
        assert_eq!(b[3], 5.0);
        assert_eq!(b[4], 3.0);
        // Verify max-plus identity: after closure, b[j] >= b[j±1] - w
        for j in 1..b.len() {
            assert!(b[j] >= b[j - 1] - w - 1e-6);
        }
        for j in 0..b.len() - 1 {
            assert!(b[j] >= b[j + 1] - w - 1e-6);
        }
    }

    #[test]
    fn separable_closure_empty() {
        let mut b: Vec<f32> = vec![];
        separable_lipschitz_closure_1d(&mut b, 1.0);
        assert!(b.is_empty());
    }

    #[test]
    fn separable_closure_single() {
        let mut b = vec![3.0];
        separable_lipschitz_closure_1d(&mut b, 1.0);
        assert_eq!(b[0], 3.0);
    }
}
