/// Compute Bt (time-derivative bound) for a rigidly moving object.
pub fn compute_bt_rigid(
    b_obj: [f32; 3],
    v_world: [f32; 3],
    omega_world: [f32; 3],
    r_transpose: [[f32; 3]; 3],
    aabb_radius: f32,
) -> f32 {
    // dx'/dt = -R^T v - ω' × x'  (ω' = R^T ω in body frame)
    let rt_v = [
        r_transpose[0][0] * v_world[0]
            + r_transpose[0][1] * v_world[1]
            + r_transpose[0][2] * v_world[2],
        r_transpose[1][0] * v_world[0]
            + r_transpose[1][1] * v_world[1]
            + r_transpose[1][2] * v_world[2],
        r_transpose[2][0] * v_world[0]
            + r_transpose[2][1] * v_world[1]
            + r_transpose[2][2] * v_world[2],
    ];
    let omega_body = [
        r_transpose[0][0] * omega_world[0]
            + r_transpose[0][1] * omega_world[1]
            + r_transpose[0][2] * omega_world[2],
        r_transpose[1][0] * omega_world[0]
            + r_transpose[1][1] * omega_world[1]
            + r_transpose[1][2] * omega_world[2],
        r_transpose[2][0] * omega_world[0]
            + r_transpose[2][1] * omega_world[1]
            + r_transpose[2][2] * omega_world[2],
    ];
    // Per-component cross product bound
    let dxdt_bound = [
        rt_v[0].abs() + omega_body[1].abs() * aabb_radius + omega_body[2].abs() * aabb_radius,
        rt_v[1].abs() + omega_body[2].abs() * aabb_radius + omega_body[0].abs() * aabb_radius,
        rt_v[2].abs() + omega_body[0].abs() * aabb_radius + omega_body[1].abs() * aabb_radius,
    ];
    b_obj[0] * dxdt_bound[0] + b_obj[1] * dxdt_bound[1] + b_obj[2] * dxdt_bound[2]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pure_translation() {
        // Identity rotation, no angular velocity, unit SDF
        let identity = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        let b_obj = [1.0, 1.0, 1.0];
        let v_world = [1.0, 0.0, 0.0];
        let omega_world = [0.0, 0.0, 0.0];

        let bt = compute_bt_rigid(b_obj, v_world, omega_world, identity, 1.0);
        // dxdt_bound = [1, 0, 0], dot with b_obj = 1
        assert!((bt - 1.0).abs() < 1e-6);
    }

    #[test]
    fn pure_rotation() {
        let identity = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        let b_obj = [1.0, 1.0, 1.0];
        let v_world = [0.0, 0.0, 0.0];
        let omega_world = [0.0, 0.0, 1.0]; // rotate around z
        let aabb_radius = 1.0;

        let bt = compute_bt_rigid(b_obj, v_world, omega_world, identity, aabb_radius);
        // omega_body = (0,0,1)
        // dxdt = [|ω_y|*r + |ω_z|*r, |ω_z|*r + |ω_x|*r, |ω_x|*r + |ω_y|*r]
        //       = [0 + 1, 1 + 0, 0 + 0] = [1, 1, 0]
        // Bt = 1*1 + 1*1 + 1*0 = 2
        assert!((bt - 2.0).abs() < 1e-6);
    }

    #[test]
    fn zero_motion_zero_bt() {
        let identity = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        let bt = compute_bt_rigid(
            [1.0, 1.0, 1.0],
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0],
            identity,
            1.0,
        );
        assert_eq!(bt, 0.0);
    }

    #[test]
    fn translation_plus_rotation() {
        let identity = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        let b_obj = [1.0, 1.0, 1.0];
        let v_world = [1.0, 0.0, 0.0];
        let omega_world = [0.0, 0.0, 1.0];
        let aabb_radius = 1.0;

        let bt = compute_bt_rigid(b_obj, v_world, omega_world, identity, aabb_radius);
        // dxdt = [1 + 0 + 1, 0 + 1 + 0, 0 + 0 + 0] = [2, 1, 0]
        // Bt = 1*2 + 1*1 + 1*0 = 3
        assert!((bt - 3.0).abs() < 1e-6);
    }
}
