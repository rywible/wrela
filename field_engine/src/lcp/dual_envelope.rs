/// Dual-envelope evaluation: compute conservative lower bound from both
/// L1 (anisotropic) and L2 (Euclidean) certificates, return the tighter one.
pub fn dual_envelope_lower_bound(
    p: [f32; 3],
    samples: &[([f32; 3], f32)],
    b_cell: [f32; 3],
    l_scalar: f32,
    has_l2_certificate: bool,
) -> f32 {
    // Certificate A: weighted L1 envelope
    let mut b_l1 = f32::NEG_INFINITY;
    let mut max_abs_l1 = 1.0_f32;
    for &(x_i, b_i) in samples {
        let d_l1 = b_cell[0] * (p[0] - x_i[0]).abs()
            + b_cell[1] * (p[1] - x_i[1]).abs()
            + b_cell[2] * (p[2] - x_i[2]).abs();
        let candidate = b_i - d_l1;
        b_l1 = f32::max(b_l1, candidate);
        max_abs_l1 = max_abs_l1
            .max(b_i.abs())
            .max(d_l1.abs())
            .max(candidate.abs());
    }
    let pad_l1 = 2.0 * f32::EPSILON * max_abs_l1.max(b_l1.abs());
    let b_l1_padded = b_l1 - pad_l1;

    if !has_l2_certificate || !l_scalar.is_finite() || l_scalar <= 0.0 {
        return b_l1_padded;
    }

    // Certificate B: Euclidean L2 envelope
    let mut b_l2 = f32::NEG_INFINITY;
    let mut max_abs_l2 = 1.0_f32;
    for &(x_i, b_i) in samples {
        let dx = p[0] - x_i[0];
        let dy = p[1] - x_i[1];
        let dz = p[2] - x_i[2];
        let d_l2 = l_scalar * (dx * dx + dy * dy + dz * dz).sqrt();
        let candidate = b_i - d_l2;
        b_l2 = f32::max(b_l2, candidate);
        max_abs_l2 = max_abs_l2
            .max(b_i.abs())
            .max(d_l2.abs())
            .max(candidate.abs());
    }
    let pad_l2 = 2.0 * f32::EPSILON * max_abs_l2.max(b_l2.abs());
    let b_l2_padded = b_l2 - pad_l2;

    // Fusion: max of individually-padded certificates
    f32::max(b_l1_padded, b_l2_padded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lipschitz::envelope::lipschitz_envelope;

    fn sphere_distance(p: [f32; 3]) -> f32 {
        (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt() - 1.0
    }

    fn make_samples() -> Vec<([f32; 3], f32)> {
        let mut samples = Vec::new();
        for ix in -2..=2 {
            for iy in -2..=2 {
                for iz in -2..=2 {
                    let p = [ix as f32 * 0.5, iy as f32 * 0.5, iz as f32 * 0.5];
                    samples.push((p, sphere_distance(p)));
                }
            }
        }
        samples
    }

    #[test]
    fn dual_envelope_at_least_lipschitz_envelope() {
        let samples = make_samples();
        let b_cell = [1.0, 1.0, 1.0];
        let l_scalar = 1.0;

        for ix in -5..=5 {
            for iy in -5..=5 {
                for iz in -5..=5 {
                    let p = [ix as f32 * 0.3, iy as f32 * 0.3, iz as f32 * 0.3];
                    let dual = dual_envelope_lower_bound(p, &samples, b_cell, l_scalar, true);
                    let l1_only = lipschitz_envelope(p, &samples, b_cell);
                    assert!(
                        dual >= l1_only - 1e-6,
                        "Fusion regressed at {:?}: dual={} < l1={}",
                        p, dual, l1_only
                    );
                }
            }
        }
    }

    #[test]
    fn fail_closed_no_l2() {
        let samples = make_samples();
        let b_cell = [1.0, 1.0, 1.0];

        let _with_l2 = dual_envelope_lower_bound([2.0, 0.0, 0.0], &samples, b_cell, 1.0, true);
        let without_l2 = dual_envelope_lower_bound([2.0, 0.0, 0.0], &samples, b_cell, 1.0, false);

        // Without L2, should fall back to L1-only
        let l1_only = lipschitz_envelope([2.0, 0.0, 0.0], &samples, b_cell);
        assert!((without_l2 - l1_only).abs() < 1e-6);
    }

    #[test]
    fn fail_closed_bad_l_scalar() {
        let samples = make_samples();
        let b_cell = [1.0, 1.0, 1.0];

        // Non-finite
        let r = dual_envelope_lower_bound([2.0, 0.0, 0.0], &samples, b_cell, f32::INFINITY, true);
        let l1 = lipschitz_envelope([2.0, 0.0, 0.0], &samples, b_cell);
        assert!((r - l1).abs() < 1e-6);

        // Zero
        let r = dual_envelope_lower_bound([2.0, 0.0, 0.0], &samples, b_cell, 0.0, true);
        assert!((r - l1).abs() < 1e-6);

        // Negative
        let r = dual_envelope_lower_bound([2.0, 0.0, 0.0], &samples, b_cell, -1.0, true);
        assert!((r - l1).abs() < 1e-6);
    }
}
