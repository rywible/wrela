/// Lipschitz composition rules (compile-time and runtime).

/// Union: max of individual Lipschitz constants.
pub fn lipschitz_union(a: f32, b: f32) -> f32 {
    f32::max(a, b)
}

/// Polynomial smooth-min: conservative sum bound.
pub fn lipschitz_poly_smin(a: f32, b: f32, _radius: f32) -> f32 {
    a + b
}

/// Log-sum-exp smooth-min: convex combination, no extra term.
pub fn lipschitz_lse_smin(a: f32, b: f32) -> f32 {
    f32::max(a, b)
}

/// R-function smooth Boolean: tight ~3.414x bound.
pub fn lipschitz_r_function(a: f32, b: f32) -> f32 {
    (2.0 + std::f32::consts::SQRT_2) * f32::max(a, b)
}

/// Warp: field composed with domain warp.
pub fn lipschitz_warp(field_l: f32, warp_l: f32) -> f32 {
    field_l * (1.0 + warp_l)
}

/// General composition: chain rule for f(g(x)).
pub fn lipschitz_composition(outer: f32, inner: f32) -> f32 {
    outer * inner
}

/// Scalar fallback from anisotropic bound via L2 norm.
pub fn scalar_fallback_from_bound_l2(b: [f32; 3]) -> f32 {
    (b[0] * b[0] + b[1] * b[1] + b[2] * b[2]).sqrt()
}

/// Scalar fallback from anisotropic bound via L1 norm.
pub fn scalar_fallback_from_bound_l1(b: [f32; 3]) -> f32 {
    b[0] + b[1] + b[2]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn union_takes_max() {
        assert_eq!(lipschitz_union(1.0, 2.0), 2.0);
        assert_eq!(lipschitz_union(3.0, 1.0), 3.0);
    }

    #[test]
    fn poly_smin_is_conservative() {
        // Sum is always >= max
        let a = 1.5;
        let b = 2.5;
        assert!(lipschitz_poly_smin(a, b, 0.1) >= lipschitz_union(a, b));
    }

    #[test]
    fn lse_smin_equals_max() {
        assert_eq!(lipschitz_lse_smin(1.0, 2.0), 2.0);
    }

    #[test]
    fn r_function_multiplier() {
        let a = 1.0;
        let b = 1.0;
        let result = lipschitz_r_function(a, b);
        let expected = (2.0 + std::f32::consts::SQRT_2) * 1.0;
        assert!((result - expected).abs() < 1e-6);
    }

    #[test]
    fn warp_composition() {
        assert!((lipschitz_warp(1.0, 0.5) - 1.5).abs() < 1e-6);
    }

    #[test]
    fn general_composition_chain_rule() {
        assert!((lipschitz_composition(2.0, 3.0) - 6.0).abs() < 1e-6);
    }

    #[test]
    fn scalar_fallback_l2_unit_sphere() {
        let b = [1.0, 1.0, 1.0];
        let l = scalar_fallback_from_bound_l2(b);
        assert!((l - 3.0_f32.sqrt()).abs() < 1e-6);
    }

    #[test]
    fn scalar_fallback_l1() {
        assert_eq!(scalar_fallback_from_bound_l1([1.0, 2.0, 3.0]), 6.0);
    }

    // Algebraic properties
    #[test]
    fn union_commutative() {
        assert_eq!(lipschitz_union(1.0, 2.0), lipschitz_union(2.0, 1.0));
    }

    #[test]
    fn union_associative() {
        let a = lipschitz_union(lipschitz_union(1.0, 2.0), 3.0);
        let b = lipschitz_union(1.0, lipschitz_union(2.0, 3.0));
        assert_eq!(a, b);
    }

    #[test]
    fn poly_smin_commutative() {
        assert_eq!(
            lipschitz_poly_smin(1.0, 2.0, 0.5),
            lipschitz_poly_smin(2.0, 1.0, 0.5)
        );
    }

    #[test]
    fn composition_with_identity() {
        // Identity function has L=1
        assert!((lipschitz_composition(2.0, 1.0) - 2.0).abs() < 1e-6);
    }

    #[test]
    fn l2_greater_equal_single_component() {
        let b = [3.0, 0.0, 0.0];
        assert!((scalar_fallback_from_bound_l2(b) - 3.0).abs() < 1e-6);
    }

    #[test]
    fn l1_greater_equal_l2() {
        let b = [1.0, 2.0, 3.0];
        assert!(scalar_fallback_from_bound_l1(b) >= scalar_fallback_from_bound_l2(b));
    }
}
