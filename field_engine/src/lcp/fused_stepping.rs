/// Fused directional bound: take the tighter of L1 and L2 directional bounds.
pub fn fused_directional_bound(
    ray_dir_unit: [f32; 3],
    b_cell: [f32; 3],
    l_scalar: f32,
    has_l2_certificate: bool,
) -> f32 {
    let v = [
        ray_dir_unit[0].abs(),
        ray_dir_unit[1].abs(),
        ray_dir_unit[2].abs(),
    ];
    let l_dir_l1 = v[0] * b_cell[0] + v[1] * b_cell[1] + v[2] * b_cell[2];
    if has_l2_certificate && l_scalar.is_finite() && l_scalar > 0.0 {
        f32::min(l_dir_l1, l_scalar)
    } else {
        l_dir_l1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fused_never_exceeds_l1() {
        let b_cell = [1.0, 2.0, 3.0];
        let dirs: [[f32; 3]; 4] = [
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.577, 0.577, 0.577],
        ];
        for dir in &dirs {
            let l1 = dir[0].abs() * b_cell[0]
                + dir[1].abs() * b_cell[1]
                + dir[2].abs() * b_cell[2];
            let fused = fused_directional_bound(*dir, b_cell, 1.5, true);
            assert!(
                fused <= l1 + 1e-6,
                "Fused {} exceeds L1 {} for dir {:?}",
                fused, l1, dir
            );
        }
    }

    #[test]
    fn fused_with_l2_tighter() {
        // When L_scalar < L_dir_L1, fused should pick L_scalar
        let b_cell = [2.0, 2.0, 2.0];
        let dir = [0.577, 0.577, 0.577]; // L_dir_L1 ≈ 3.46
        let l_scalar = 1.5;
        let fused = fused_directional_bound(dir, b_cell, l_scalar, true);
        assert!((fused - 1.5).abs() < 1e-6);
    }

    #[test]
    fn fused_without_l2() {
        let b_cell = [1.0, 1.0, 1.0];
        let dir = [1.0, 0.0, 0.0];
        let fused = fused_directional_bound(dir, b_cell, 0.5, false);
        assert!((fused - 1.0).abs() < 1e-6); // Falls back to L1
    }

    #[test]
    fn fused_bad_l_scalar() {
        let b_cell = [1.0, 1.0, 1.0];
        let dir = [1.0, 0.0, 0.0];
        // Infinity
        let fused = fused_directional_bound(dir, b_cell, f32::INFINITY, true);
        assert!((fused - 1.0).abs() < 1e-6);
        // Zero
        let fused = fused_directional_bound(dir, b_cell, 0.0, true);
        assert!((fused - 1.0).abs() < 1e-6);
    }
}
