use super::types::FieldSample;

/// Safe stepping from a canonical lower bound b (preferred envelope path).
/// This is THE invariant in canonical form.
pub fn safe_step_from_lower_bound(
    b_lower: f32,
    dfdxyz_bound: [f32; 3],
    has_anisotropic_bound: bool,
    lipschitz_fallback: f32,
    ray_dir_unit: [f32; 3],
    distance_to_region_exit: f32,
) -> f32 {
    let v = [
        ray_dir_unit[0].abs(),
        ray_dir_unit[1].abs(),
        ray_dir_unit[2].abs(),
    ];
    let l_dir = if has_anisotropic_bound {
        v[0] * dfdxyz_bound[0] + v[1] * dfdxyz_bound[1] + v[2] * dfdxyz_bound[2]
    } else {
        lipschitz_fallback
    };
    let l_safe = f32::max(l_dir, 1e-6);
    f32::min(f32::max(0.0, b_lower) / l_safe, distance_to_region_exit)
}

/// Compatibility helper for legacy distance+epsilon call sites.
/// Converts to canonical lower bound and delegates.
pub fn safe_step_from_sample(
    sample: &FieldSample,
    ray_dir_unit: [f32; 3],
    distance_to_region_exit: f32,
) -> f32 {
    let b_lower = sample.distance - sample.epsilon;
    safe_step_from_lower_bound(
        b_lower,
        sample.dfdxyz_bound(),
        sample.has_anisotropic_bound(),
        sample.lipschitz,
        ray_dir_unit,
        distance_to_region_exit,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lipschitz::types::{BoundProvenance, RegionValidBound};

    #[test]
    fn sphere_step_on_axis() {
        // Unit sphere, point at distance 2.0 on +x axis, ray toward -x.
        // Lower bound = 2.0 - 1.0 = 1.0 (distance to sphere surface)
        let step = safe_step_from_lower_bound(
            1.0,                  // b_lower
            [1.0, 1.0, 1.0],     // isotropic B for sphere
            true,
            1.0,
            [-1.0, 0.0, 0.0],    // ray along -x
            10.0,
        );
        // L_dir = 1*1 + 0*1 + 0*1 = 1.0, step = 1.0/1.0 = 1.0
        assert!((step - 1.0).abs() < 1e-5);
    }

    #[test]
    fn sphere_step_diagonal() {
        // Diagonal ray: L_dir = (1/√3)*1 + (1/√3)*1 + (1/√3)*1 = √3
        let inv_sqrt3 = 1.0 / 3.0_f32.sqrt();
        let step = safe_step_from_lower_bound(
            1.0,
            [1.0, 1.0, 1.0],
            true,
            1.73,
            [inv_sqrt3, inv_sqrt3, inv_sqrt3],
            10.0,
        );
        let expected = 1.0 / 3.0_f32.sqrt();
        assert!((step - expected).abs() < 1e-5);
    }

    #[test]
    fn negative_lower_bound_returns_zero() {
        let step = safe_step_from_lower_bound(-1.0, [1.0, 1.0, 1.0], true, 1.0, [1.0, 0.0, 0.0], 10.0);
        assert_eq!(step, 0.0);
    }

    #[test]
    fn zero_lower_bound_returns_zero() {
        let step = safe_step_from_lower_bound(0.0, [1.0, 1.0, 1.0], true, 1.0, [1.0, 0.0, 0.0], 10.0);
        assert_eq!(step, 0.0);
    }

    #[test]
    fn clamped_to_region_exit() {
        let step = safe_step_from_lower_bound(100.0, [1.0, 1.0, 1.0], true, 1.0, [1.0, 0.0, 0.0], 5.0);
        assert_eq!(step, 5.0);
    }

    #[test]
    fn fallback_to_scalar_lipschitz() {
        let step = safe_step_from_lower_bound(2.0, [0.0, 0.0, 0.0], false, 2.0, [1.0, 0.0, 0.0], 10.0);
        assert!((step - 1.0).abs() < 1e-5);
    }

    #[test]
    fn safe_step_from_sample_converts_correctly() {
        let sample = FieldSample {
            distance: 3.0,
            region_bound: Some(RegionValidBound::from_analytic([1.0, 1.0, 1.0], false)),
            lipschitz: 1.73,
            lipschitz_provenance: BoundProvenance::Analytic,
            epsilon: 0.1,
        };
        let step = safe_step_from_sample(&sample, [1.0, 0.0, 0.0], 10.0);
        // b_lower = 3.0 - 0.1 = 2.9, L_dir = 1.0, step = 2.9
        assert!((step - 2.9).abs() < 1e-5);
    }
}
