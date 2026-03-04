/// Safe spacetime step along a trajectory p(t) = p0 + v*t.
/// Returns how far in TIME the point can advance without crossing a surface.
pub fn safe_step_spacetime_along_path(
    b_lower: f32,
    dfdxyz_bound: [f32; 3],
    b_time: f32,
    has_spacetime_bound: bool,
    velocity: [f32; 3],
    dt_remaining: f32,
    dt_to_bound_region_exit: f32,
) -> f32 {
    if !has_spacetime_bound {
        return 0.0;
    }
    let l_path = dfdxyz_bound[0] * velocity[0].abs()
        + dfdxyz_bound[1] * velocity[1].abs()
        + dfdxyz_bound[2] * velocity[2].abs()
        + b_time;
    let l_safe = l_path.max(1e-6);
    (b_lower.max(0.0) / l_safe)
        .min(dt_remaining)
        .min(dt_to_bound_region_exit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fail_closed_no_spacetime_bound() {
        let dt = safe_step_spacetime_along_path(
            1.0,
            [1.0, 1.0, 1.0],
            1.0,
            false, // no spacetime bound
            [1.0, 0.0, 0.0],
            10.0,
            10.0,
        );
        assert_eq!(dt, 0.0);
    }

    #[test]
    fn simple_linear_motion() {
        // b_lower = 2.0, velocity = (1,0,0), B = (1,1,1), Bt = 0.5
        // L_path = 1*1 + 1*0 + 1*0 + 0.5 = 1.5
        // dt = 2.0 / 1.5 ≈ 1.333
        let dt = safe_step_spacetime_along_path(
            2.0,
            [1.0, 1.0, 1.0],
            0.5,
            true,
            [1.0, 0.0, 0.0],
            10.0,
            10.0,
        );
        assert!((dt - 2.0 / 1.5).abs() < 1e-5);
    }

    #[test]
    fn clamped_to_dt_remaining() {
        let dt = safe_step_spacetime_along_path(
            100.0,
            [1.0, 1.0, 1.0],
            0.5,
            true,
            [1.0, 0.0, 0.0],
            0.5,
            10.0,
        );
        assert_eq!(dt, 0.5);
    }

    #[test]
    fn clamped_to_region_exit() {
        let dt = safe_step_spacetime_along_path(
            100.0,
            [1.0, 1.0, 1.0],
            0.5,
            true,
            [1.0, 0.0, 0.0],
            10.0,
            0.3,
        );
        assert_eq!(dt, 0.3);
    }

    #[test]
    fn negative_b_lower_returns_zero() {
        let dt = safe_step_spacetime_along_path(
            -1.0,
            [1.0, 1.0, 1.0],
            0.5,
            true,
            [1.0, 0.0, 0.0],
            10.0,
            10.0,
        );
        assert_eq!(dt, 0.0);
    }
}
