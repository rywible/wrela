/// Anisotropic derivative-bound rules.

/// Component-wise addition of derivative bounds.
pub fn bound_add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

/// Component-wise maximum of derivative bounds.
pub fn bound_max(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0].max(b[0]), a[1].max(b[1]), a[2].max(b[2])]
}

/// Chain rule with absolute Jacobian bound A (column-major).
/// B_out[i] = sum_j A[j][i] * B_in[j]
pub fn bound_chain(a_abs_jacobian: [[f32; 3]; 3], b_in: [f32; 3]) -> [f32; 3] {
    [
        a_abs_jacobian[0][0] * b_in[0]
            + a_abs_jacobian[1][0] * b_in[1]
            + a_abs_jacobian[2][0] * b_in[2],
        a_abs_jacobian[0][1] * b_in[0]
            + a_abs_jacobian[1][1] * b_in[1]
            + a_abs_jacobian[2][1] * b_in[2],
        a_abs_jacobian[0][2] * b_in[0]
            + a_abs_jacobian[1][2] * b_in[1]
            + a_abs_jacobian[2][2] * b_in[2],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bound_add_basic() {
        assert_eq!(bound_add([1.0, 2.0, 3.0], [4.0, 5.0, 6.0]), [5.0, 7.0, 9.0]);
    }

    #[test]
    fn bound_max_basic() {
        assert_eq!(
            bound_max([1.0, 5.0, 3.0], [4.0, 2.0, 6.0]),
            [4.0, 5.0, 6.0]
        );
    }

    #[test]
    fn bound_chain_identity() {
        let identity = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        let b = [1.0, 2.0, 3.0];
        let result = bound_chain(identity, b);
        assert!((result[0] - 1.0).abs() < 1e-6);
        assert!((result[1] - 2.0).abs() < 1e-6);
        assert!((result[2] - 3.0).abs() < 1e-6);
    }

    #[test]
    fn bound_chain_scaling() {
        let scale = [[2.0, 0.0, 0.0], [0.0, 3.0, 0.0], [0.0, 0.0, 4.0]];
        let b = [1.0, 1.0, 1.0];
        let result = bound_chain(scale, b);
        assert!((result[0] - 2.0).abs() < 1e-6);
        assert!((result[1] - 3.0).abs() < 1e-6);
        assert!((result[2] - 4.0).abs() < 1e-6);
    }

    #[test]
    fn bound_add_commutative() {
        let a = [1.0, 2.0, 3.0];
        let b = [4.0, 5.0, 6.0];
        assert_eq!(bound_add(a, b), bound_add(b, a));
    }
}
