/// Mixed cone-union stepping: union of L1 octahedra AND L2 spheres along ray.
/// Returns end of connected safe component from t=0.
pub fn mixed_cone_union_safe_step(
    p0: [f32; 3],
    v: [f32; 3],
    samples: &[([f32; 3], f32)],
    b_cell: [f32; 3],
    l_scalar: f32,
    active_region_exit: f32,
) -> f32 {
    let l_scalar_safe = if l_scalar.is_finite() {
        l_scalar.max(1e-6)
    } else {
        f32::INFINITY
    };
    let max_n = samples.len().min(27);
    let mut intervals: [(f32, f32); 54] = [(0.0, -1.0); 54];
    let mut n_intervals = 0usize;

    for idx in 0..max_n {
        let (x_i, b_i) = samples[idx];

        // L1 octahedron intervals
        let mut t_lo = 0.0_f32;
        let mut t_hi = f32::INFINITY;
        for s in 0u32..8 {
            let sx = if s & 1 != 0 { 1.0_f32 } else { -1.0 };
            let sy = if s & 2 != 0 { 1.0_f32 } else { -1.0 };
            let sz = if s & 4 != 0 { 1.0_f32 } else { -1.0 };
            let a = sx * b_cell[0] * (p0[0] - x_i[0])
                + sy * b_cell[1] * (p0[1] - x_i[1])
                + sz * b_cell[2] * (p0[2] - x_i[2]);
            let m = sx * b_cell[0] * v[0] + sy * b_cell[1] * v[1] + sz * b_cell[2] * v[2];
            if m.abs() < 1e-12 {
                if a > b_i {
                    t_hi = f32::NEG_INFINITY;
                }
            } else if m > 0.0 {
                t_hi = f32::min(t_hi, (b_i - a) / m);
            } else {
                t_lo = f32::max(t_lo, (b_i - a) / m);
            }
        }
        if t_lo <= t_hi {
            intervals[n_intervals] = (t_lo, t_hi);
            n_intervals += 1;
        }

        // L2 sphere interval
        let r = b_i / l_scalar_safe;
        if r > 0.0 {
            let wx = p0[0] - x_i[0];
            let wy = p0[1] - x_i[1];
            let wz = p0[2] - x_i[2];
            let w_dot_v = wx * v[0] + wy * v[1] + wz * v[2];
            let w_dot_w = wx * wx + wy * wy + wz * wz;
            let discriminant = w_dot_v * w_dot_v - (w_dot_w - r * r);
            if discriminant >= 0.0 {
                let sqrt_d = discriminant.sqrt();
                let t_enter = f32::max(0.0, -w_dot_v - sqrt_d);
                let t_exit = -w_dot_v + sqrt_d;
                if t_exit > 0.0 && t_enter <= t_exit {
                    intervals[n_intervals] = (t_enter, t_exit);
                    n_intervals += 1;
                }
            }
        }
    }

    // Grow connected component from t=0
    let mut end = 0.0_f32;
    let mut changed = true;
    while changed {
        changed = false;
        for idx in 0..n_intervals {
            let (lo, hi) = intervals[idx];
            if lo <= end && hi > end {
                end = hi;
                changed = true;
            }
        }
    }
    let pad = 2.0 * f32::EPSILON * end.abs().max(1.0);
    let end_safe = end - pad;
    f32::min(f32::max(end_safe, 0.0), active_region_exit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lipschitz::cone_union::cone_union_safe_step;

    fn sphere_distance(p: [f32; 3]) -> f32 {
        (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt() - 1.0
    }

    fn make_samples() -> Vec<([f32; 3], f32)> {
        let mut samples = Vec::new();
        for ix in -1..=4 {
            for iy in -1..=1 {
                for iz in -1..=1 {
                    let p = [ix as f32 * 0.5, iy as f32 * 0.5, iz as f32 * 0.5];
                    samples.push((p, sphere_distance(p)));
                }
            }
        }
        samples
    }

    #[test]
    fn mixed_at_least_l1_only() {
        let samples = make_samples();
        let b_cell = [1.0, 1.0, 1.0];
        let p0 = [2.5, 0.0, 0.0];
        let v = [-1.0, 0.0, 0.0];
        let region_exit = 10.0;

        let l1_step = cone_union_safe_step(p0, v, &samples, b_cell, region_exit);
        let mixed_step =
            mixed_cone_union_safe_step(p0, v, &samples, b_cell, 1.0, region_exit);

        assert!(
            mixed_step >= l1_step - 1e-4,
            "Mixed {} < L1-only {}",
            mixed_step,
            l1_step
        );
    }

    #[test]
    fn mixed_no_surface_crossing() {
        let samples = make_samples();
        let b_cell = [1.0, 1.0, 1.0];
        let p0 = [2.5, 0.0, 0.0];
        let v = [-1.0, 0.0, 0.0];

        let step = mixed_cone_union_safe_step(p0, v, &samples, b_cell, 1.0, 10.0);

        if step > 0.0 {
            let p_end = [
                p0[0] + step * v[0],
                p0[1] + step * v[1],
                p0[2] + step * v[2],
            ];
            let f_end = sphere_distance(p_end);
            assert!(
                f_end >= -1e-4,
                "Surface crossing at step={}: f={}",
                step,
                f_end
            );
        }
    }

    #[test]
    fn mixed_respects_region_exit() {
        let samples = vec![([0.0, 0.0, 0.0], 100.0)];
        let b_cell = [1.0, 1.0, 1.0];
        let step =
            mixed_cone_union_safe_step([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], &samples, b_cell, 1.0, 5.0);
        assert!(step <= 5.0);
    }
}
