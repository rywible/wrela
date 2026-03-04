/// Connected cone-union stepping: compute farthest safe step along ray
/// by marching through the union of weighted-L1 balls from envelope stencil.
/// Returns the end of the connected safe component containing t=0.
pub fn cone_union_safe_step(
    p0: [f32; 3],
    v: [f32; 3],
    samples: &[([f32; 3], f32)],
    b_cell: [f32; 3],
    active_region_exit: f32,
) -> f32 {
    let mut intervals: [(f32, f32); 27] = [(0.0, 0.0); 27];
    let n = samples.len().min(27);

    for idx in 0..n {
        let (x_i, b_i) = samples[idx];
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
        intervals[idx] = if t_lo <= t_hi {
            (t_lo, t_hi)
        } else {
            (0.0, -1.0)
        };
    }

    // Grow connected component from t=0
    let mut end = 0.0_f32;
    let mut changed = true;
    while changed {
        changed = false;
        for idx in 0..n {
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
    use crate::lipschitz::stepping::safe_step_from_lower_bound;
    use crate::lipschitz::envelope::lipschitz_envelope;

    fn sphere_distance(p: [f32; 3]) -> f32 {
        (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt() - 1.0
    }

    #[test]
    fn cone_union_at_least_standard_step() {
        let b_cell = [1.0, 1.0, 1.0];
        let p0 = [3.0, 0.0, 0.0];
        let v = [-1.0, 0.0, 0.0];
        let region_exit = 10.0;

        let mut samples = Vec::new();
        for ix in -2..=4 {
            for iy in -2..=2 {
                for iz in -2..=2 {
                    let sp = [ix as f32 * 0.5, iy as f32 * 0.5, iz as f32 * 0.5];
                    let d = sphere_distance(sp);
                    samples.push((sp, d));
                }
            }
        }

        let cone_step = cone_union_safe_step(p0, v, &samples, b_cell, region_exit);
        let b_env = lipschitz_envelope(p0, &samples, b_cell);
        let std_step = safe_step_from_lower_bound(b_env, b_cell, true, 1.73, v, region_exit);

        // cone_union should be at least as good as standard stepping
        // (allowing for conservative pad differences)
        assert!(
            cone_step >= std_step - 0.01,
            "cone_union {} < std_step {}",
            cone_step,
            std_step
        );
    }

    #[test]
    fn cone_union_respects_region_exit() {
        let b_cell = [1.0, 1.0, 1.0];
        let samples = vec![([0.0, 0.0, 0.0], 100.0)]; // very far from surface
        let step = cone_union_safe_step([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], &samples, b_cell, 5.0);
        assert!(step <= 5.0);
    }

    #[test]
    fn cone_union_no_surface_crossing() {
        let b_cell = [1.0, 1.0, 1.0];
        let p0 = [2.5, 0.0, 0.0];
        let v = [-1.0, 0.0, 0.0];

        let mut samples = Vec::new();
        for ix in -1..=4 {
            for iy in -1..=1 {
                for iz in -1..=1 {
                    let sp = [ix as f32 * 0.5, iy as f32 * 0.5, iz as f32 * 0.5];
                    let d = sphere_distance(sp);
                    samples.push((sp, d));
                }
            }
        }

        let step = cone_union_safe_step(p0, v, &samples, b_cell, 10.0);

        // Verify no surface crossing: f(p0 + step*v) >= 0
        if step > 0.0 {
            let p_end = [p0[0] + step * v[0], p0[1] + step * v[1], p0[2] + step * v[2]];
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
    fn cone_union_returns_zero_at_surface() {
        let b_cell = [1.0, 1.0, 1.0];
        // Point on the surface
        let samples = vec![([1.0, 0.0, 0.0], 0.0)];
        let step = cone_union_safe_step([1.0, 0.0, 0.0], [1.0, 0.0, 0.0], &samples, b_cell, 10.0);
        assert_eq!(step, 0.0);
    }
}
