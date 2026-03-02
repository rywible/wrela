pub type Mat4 = [[f32; 4]; 4];

pub fn mat4_identity() -> Mat4 {
    [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

pub fn mat4_perspective(fov_y: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
    let f = 1.0 / (fov_y * 0.5).tan();
    let range_inv = 1.0 / (near - far);
    [
        [f / aspect, 0.0, 0.0, 0.0],
        [0.0, f, 0.0, 0.0],
        [0.0, 0.0, far * range_inv, -1.0],
        [0.0, 0.0, near * far * range_inv, 0.0],
    ]
}

pub fn mat4_look_at(eye: [f32; 3], target: [f32; 3], up: [f32; 3]) -> Mat4 {
    let fwd = vec3_normalize([target[0] - eye[0], target[1] - eye[1], target[2] - eye[2]]);
    let right = vec3_normalize(vec3_cross(fwd, up));
    let cam_up = vec3_cross(right, fwd);

    [
        [right[0], cam_up[0], -fwd[0], 0.0],
        [right[1], cam_up[1], -fwd[1], 0.0],
        [right[2], cam_up[2], -fwd[2], 0.0],
        [
            -vec3_dot(right, eye),
            -vec3_dot(cam_up, eye),
            vec3_dot(fwd, eye),
            1.0,
        ],
    ]
}

pub fn mat4_mul(a: Mat4, b: Mat4) -> Mat4 {
    // Column-major multiplication: result = A * B
    // out[col][row] = sum_k A[k][row] * B[col][k]
    let mut out = [[0.0f32; 4]; 4];
    for col in 0..4 {
        for row in 0..4 {
            out[col][row] = a[0][row] * b[col][0]
                + a[1][row] * b[col][1]
                + a[2][row] * b[col][2]
                + a[3][row] * b[col][3];
        }
    }
    out
}

pub fn mat4_transpose(m: Mat4) -> Mat4 {
    [
        [m[0][0], m[1][0], m[2][0], m[3][0]],
        [m[0][1], m[1][1], m[2][1], m[3][1]],
        [m[0][2], m[1][2], m[2][2], m[3][2]],
        [m[0][3], m[1][3], m[2][3], m[3][3]],
    ]
}

pub fn mat4_inverse(m: Mat4) -> Mat4 {
    let s0 = m[0][0] * m[1][1] - m[1][0] * m[0][1];
    let s1 = m[0][0] * m[1][2] - m[1][0] * m[0][2];
    let s2 = m[0][0] * m[1][3] - m[1][0] * m[0][3];
    let s3 = m[0][1] * m[1][2] - m[1][1] * m[0][2];
    let s4 = m[0][1] * m[1][3] - m[1][1] * m[0][3];
    let s5 = m[0][2] * m[1][3] - m[1][2] * m[0][3];

    let c5 = m[2][2] * m[3][3] - m[3][2] * m[2][3];
    let c4 = m[2][1] * m[3][3] - m[3][1] * m[2][3];
    let c3 = m[2][1] * m[3][2] - m[3][1] * m[2][2];
    let c2 = m[2][0] * m[3][3] - m[3][0] * m[2][3];
    let c1 = m[2][0] * m[3][2] - m[3][0] * m[2][2];
    let c0 = m[2][0] * m[3][1] - m[3][0] * m[2][1];

    let det = s0 * c5 - s1 * c4 + s2 * c3 + s3 * c2 - s4 * c1 + s5 * c0;
    if det.abs() < 1e-12 {
        return mat4_identity();
    }
    let inv_det = 1.0 / det;

    [
        [
            (m[1][1] * c5 - m[1][2] * c4 + m[1][3] * c3) * inv_det,
            (-m[0][1] * c5 + m[0][2] * c4 - m[0][3] * c3) * inv_det,
            (m[3][1] * s5 - m[3][2] * s4 + m[3][3] * s3) * inv_det,
            (-m[2][1] * s5 + m[2][2] * s4 - m[2][3] * s3) * inv_det,
        ],
        [
            (-m[1][0] * c5 + m[1][2] * c2 - m[1][3] * c1) * inv_det,
            (m[0][0] * c5 - m[0][2] * c2 + m[0][3] * c1) * inv_det,
            (-m[3][0] * s5 + m[3][2] * s2 - m[3][3] * s1) * inv_det,
            (m[2][0] * s5 - m[2][2] * s2 + m[2][3] * s1) * inv_det,
        ],
        [
            (m[1][0] * c4 - m[1][1] * c2 + m[1][3] * c0) * inv_det,
            (-m[0][0] * c4 + m[0][1] * c2 - m[0][3] * c0) * inv_det,
            (m[3][0] * s4 - m[3][1] * s2 + m[3][3] * s0) * inv_det,
            (-m[2][0] * s4 + m[2][1] * s2 - m[2][3] * s0) * inv_det,
        ],
        [
            (-m[1][0] * c3 + m[1][1] * c1 - m[1][2] * c0) * inv_det,
            (m[0][0] * c3 - m[0][1] * c1 + m[0][2] * c0) * inv_det,
            (-m[3][0] * s3 + m[3][1] * s1 - m[3][2] * s0) * inv_det,
            (m[2][0] * s3 - m[2][1] * s1 + m[2][2] * s0) * inv_det,
        ],
    ]
}

pub fn mat4_translation(x: f32, y: f32, z: f32) -> Mat4 {
    [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [x, y, z, 1.0],
    ]
}

pub fn mat4_scale(sx: f32, sy: f32, sz: f32) -> Mat4 {
    [
        [sx, 0.0, 0.0, 0.0],
        [0.0, sy, 0.0, 0.0],
        [0.0, 0.0, sz, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

pub fn mat4_rotation_y(angle: f32) -> Mat4 {
    let c = angle.cos();
    let s = angle.sin();
    [
        [c, 0.0, -s, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [s, 0.0, c, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

/// Orthographic projection matrix (right-handed, depth [0,1] for WebGPU).
pub fn mat4_ortho(left: f32, right: f32, bottom: f32, top: f32, near: f32, far: f32) -> Mat4 {
    let rl = right - left;
    let tb = top - bottom;
    let fn_ = far - near;
    [
        [2.0 / rl, 0.0, 0.0, 0.0],
        [0.0, 2.0 / tb, 0.0, 0.0],
        [0.0, 0.0, -1.0 / fn_, 0.0],
        [
            -(right + left) / rl,
            -(top + bottom) / tb,
            -near / fn_,
            1.0,
        ],
    ]
}

/// Transform a 3D point by a 4x4 matrix (column-major). Returns [x, y, z, w].
pub fn mat4_transform_point(m: Mat4, p: [f32; 3]) -> [f32; 4] {
    [
        m[0][0]*p[0] + m[1][0]*p[1] + m[2][0]*p[2] + m[3][0],
        m[0][1]*p[0] + m[1][1]*p[1] + m[2][1]*p[2] + m[3][1],
        m[0][2]*p[0] + m[1][2]*p[1] + m[2][2]*p[2] + m[3][2],
        m[0][3]*p[0] + m[1][3]*p[1] + m[2][3]*p[2] + m[3][3],
    ]
}

fn vec3_dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn vec3_cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn vec3_normalize(v: [f32; 3]) -> [f32; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len < 1e-10 {
        return [0.0, 0.0, 0.0];
    }
    [v[0] / len, v[1] / len, v[2] / len]
}

pub struct OrbitCamera {
    pub azimuth: f32,
    pub elevation: f32,
    pub distance: f32,
    pub target: [f32; 3],
    pub fov_y: f32,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self {
            azimuth: 0.0,
            elevation: 0.4,
            distance: 10.0,
            target: [0.0, 0.0, 0.0],
            fov_y: std::f32::consts::FRAC_PI_4,
        }
    }
}

impl OrbitCamera {
    pub fn eye_position(&self) -> [f32; 3] {
        let cos_elev = self.elevation.cos();
        [
            self.target[0] + self.distance * cos_elev * self.azimuth.sin(),
            self.target[1] + self.distance * self.elevation.sin(),
            self.target[2] + self.distance * cos_elev * self.azimuth.cos(),
        ]
    }

    pub fn view_matrix(&self) -> Mat4 {
        mat4_look_at(self.eye_position(), self.target, [0.0, 1.0, 0.0])
    }

    pub fn projection_matrix(&self, aspect: f32) -> Mat4 {
        mat4_perspective(self.fov_y, aspect, 0.1, 500.0)
    }

    pub fn view_projection(&self, aspect: f32) -> Mat4 {
        mat4_mul(self.projection_matrix(aspect), self.view_matrix())
    }

    pub fn rotate(&mut self, delta_azimuth: f32, delta_elevation: f32) {
        self.azimuth += delta_azimuth;
        self.elevation = (self.elevation + delta_elevation).clamp(-1.4, 1.4);
    }

    pub fn zoom(&mut self, delta: f32) {
        self.distance = (self.distance + delta).clamp(1.0, 200.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_eq(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() < eps
    }

    fn mat4_approx_eq(a: &Mat4, b: &Mat4, eps: f32) -> bool {
        for i in 0..4 {
            for j in 0..4 {
                if !approx_eq(a[i][j], b[i][j], eps) {
                    return false;
                }
            }
        }
        true
    }

    #[test]
    fn mat4_mul_identity_property() {
        let identity = mat4_identity();
        let m = [
            [2.0, 0.0, 0.0, 0.0],
            [0.0, 3.0, 0.0, 0.0],
            [0.0, 0.0, 4.0, 0.0],
            [1.0, 2.0, 3.0, 1.0],
        ];
        let result = mat4_mul(identity, m);
        assert!(mat4_approx_eq(&result, &m, 1e-6), "I * M should equal M");

        let result2 = mat4_mul(m, identity);
        assert!(mat4_approx_eq(&result2, &m, 1e-6), "M * I should equal M");
    }

    #[test]
    fn mat4_perspective_produces_correct_values() {
        let fov = std::f32::consts::FRAC_PI_4;
        let aspect = 16.0 / 9.0;
        let near = 0.1;
        let far = 100.0;
        let p = mat4_perspective(fov, aspect, near, far);

        let f = 1.0 / (fov * 0.5).tan();
        assert!(approx_eq(p[0][0], f / aspect, 1e-5));
        assert!(approx_eq(p[1][1], f, 1e-5));
        assert!(approx_eq(p[2][3], -1.0, 1e-6));
        assert!(approx_eq(p[3][3], 0.0, 1e-6));
        assert!(p[2][2] < 0.0, "z mapping should be negative for RH clip");
    }

    #[test]
    fn mat4_look_at_produces_correct_view_matrix() {
        let eye = [0.0, 0.0, 5.0];
        let target = [0.0, 0.0, 0.0];
        let up = [0.0, 1.0, 0.0];
        let v = mat4_look_at(eye, target, up);

        assert!(approx_eq(v[0][0], 1.0, 1e-5), "right.x should be 1");
        assert!(approx_eq(v[1][1], 1.0, 1e-5), "up.y should be 1");
        assert!(
            approx_eq(v[3][2], -5.0, 1e-5),
            "translation along view axis"
        );
        assert!(approx_eq(v[3][3], 1.0, 1e-6));
    }

    #[test]
    fn mat4_transpose_is_involution() {
        let m = [
            [1.0, 2.0, 3.0, 4.0],
            [5.0, 6.0, 7.0, 8.0],
            [9.0, 10.0, 11.0, 12.0],
            [13.0, 14.0, 15.0, 16.0],
        ];
        let t = mat4_transpose(m);
        let tt = mat4_transpose(t);
        assert!(mat4_approx_eq(&tt, &m, 1e-6));
        assert!(approx_eq(t[0][1], m[1][0], 1e-6));
        assert!(approx_eq(t[2][3], m[3][2], 1e-6));
    }

    #[test]
    fn orbit_camera_view_projection_matrices() {
        let camera = OrbitCamera::default();
        let eye = camera.eye_position();

        assert!(
            eye[1] > 0.0,
            "camera should be above target at positive elevation"
        );
        assert!(camera.distance > 0.0);

        let aspect = 16.0 / 9.0;
        let view = camera.view_matrix();
        let proj = camera.projection_matrix(aspect);
        let vp = camera.view_projection(aspect);
        let manual_vp = mat4_mul(proj, view);
        assert!(mat4_approx_eq(&vp, &manual_vp, 1e-4));
    }

    #[test]
    fn orbit_camera_rotation_clamps_elevation() {
        let mut camera = OrbitCamera::default();
        camera.rotate(0.0, 100.0);
        assert!(camera.elevation <= 1.4 + 1e-6);
        camera.rotate(0.0, -200.0);
        assert!(camera.elevation >= -1.4 - 1e-6);
    }

    #[test]
    fn orbit_camera_zoom_clamps_distance() {
        let mut camera = OrbitCamera::default();
        camera.zoom(-1000.0);
        assert!(camera.distance >= 1.0);
        camera.zoom(10000.0);
        assert!(camera.distance <= 200.0);
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct FrameUniform3DLayout {
        view_proj: [[f32; 4]; 4],
        camera_pos: [f32; 4],
        light_dir: [f32; 4],
        light_color: [f32; 4],
        ambient: [f32; 4],
        time: [f32; 4],
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct ModelUniformLayout {
        model: [[f32; 4]; 4],
        normal_model: [[f32; 4]; 4],
    }

    #[test]
    fn frame_uniform_3d_size_is_144_bytes() {
        assert_eq!(std::mem::size_of::<FrameUniform3DLayout>(), 144);
    }

    #[test]
    fn model_uniform_size_is_128_bytes() {
        assert_eq!(std::mem::size_of::<ModelUniformLayout>(), 128);
    }

    #[test]
    fn mat4_inverse_produces_identity_when_multiplied() {
        let m = mat4_mul(mat4_translation(3.0, -1.0, 7.0), mat4_scale(2.0, 0.5, 3.0));
        let inv = mat4_inverse(m);
        let product = mat4_mul(m, inv);
        let id = mat4_identity();
        assert!(
            mat4_approx_eq(&product, &id, 1e-4),
            "M * M^-1 should produce identity"
        );
    }

    #[test]
    fn mat4_inverse_of_identity_is_identity() {
        let id = mat4_identity();
        let inv = mat4_inverse(id);
        assert!(mat4_approx_eq(&inv, &id, 1e-6));
    }

    #[test]
    fn mat4_mul_is_associative() {
        let a = mat4_translation(1.0, 2.0, 3.0);
        let b = mat4_scale(2.0, 2.0, 2.0);
        let c = mat4_translation(-1.0, 0.0, 1.0);
        let ab_c = mat4_mul(mat4_mul(a, b), c);
        let a_bc = mat4_mul(a, mat4_mul(b, c));
        assert!(mat4_approx_eq(&ab_c, &a_bc, 1e-4));
    }

    #[test]
    fn orbit_camera_eye_distance_matches() {
        let camera = OrbitCamera {
            azimuth: 1.0,
            elevation: 0.3,
            distance: 7.0,
            target: [0.0, 0.0, 0.0],
            fov_y: std::f32::consts::FRAC_PI_4,
        };
        let eye = camera.eye_position();
        let dist = (eye[0] * eye[0] + eye[1] * eye[1] + eye[2] * eye[2]).sqrt();
        assert!(
            approx_eq(dist, 7.0, 1e-4),
            "eye should be 7.0 from origin, got {dist}"
        );
    }

    #[test]
    fn mat4_ortho_maps_center_to_origin() {
        let ortho = mat4_ortho(-10.0, 10.0, -10.0, 10.0, 0.0, 100.0);
        // Center of the box (0, 0, -50) should map near the center of the clip volume
        let center = [0.0, 0.0, 50.0, 1.0];
        let result = [
            ortho[0][0] * center[0] + ortho[1][0] * center[1] + ortho[2][0] * center[2] + ortho[3][0] * center[3],
            ortho[0][1] * center[0] + ortho[1][1] * center[1] + ortho[2][1] * center[2] + ortho[3][1] * center[3],
            ortho[0][2] * center[0] + ortho[1][2] * center[1] + ortho[2][2] * center[2] + ortho[3][2] * center[3],
            ortho[0][3] * center[0] + ortho[1][3] * center[1] + ortho[2][3] * center[2] + ortho[3][3] * center[3],
        ];
        assert!(approx_eq(result[0], 0.0, 1e-5), "x should be 0 at center");
        assert!(approx_eq(result[1], 0.0, 1e-5), "y should be 0 at center");
        assert!(approx_eq(result[3], 1.0, 1e-5), "w should be 1 for ortho");
    }

    #[test]
    fn mat4_ortho_maps_corners_correctly() {
        let ortho = mat4_ortho(-5.0, 5.0, -5.0, 5.0, 0.0, 10.0);
        // Left-bottom-near corner (-5, -5, 0) should map to (-1, -1, 0)
        let corner = [-5.0, -5.0, 0.0, 1.0];
        let result = [
            ortho[0][0] * corner[0] + ortho[1][0] * corner[1] + ortho[2][0] * corner[2] + ortho[3][0] * corner[3],
            ortho[0][1] * corner[0] + ortho[1][1] * corner[1] + ortho[2][1] * corner[2] + ortho[3][1] * corner[3],
            ortho[0][2] * corner[0] + ortho[1][2] * corner[1] + ortho[2][2] * corner[2] + ortho[3][2] * corner[3],
            ortho[0][3] * corner[0] + ortho[1][3] * corner[1] + ortho[2][3] * corner[2] + ortho[3][3] * corner[3],
        ];
        assert!(approx_eq(result[0], -1.0, 1e-5), "left should map to -1, got {}", result[0]);
        assert!(approx_eq(result[1], -1.0, 1e-5), "bottom should map to -1, got {}", result[1]);
        assert!(approx_eq(result[2], 0.0, 1e-5), "near should map to 0 for WebGPU depth, got {}", result[2]);
    }

    #[test]
    fn camera_view_proj_transforms_world_point_correctly() {
        let view = mat4_look_at([0.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]);
        let proj = mat4_perspective(std::f32::consts::FRAC_PI_4, 1.0, 0.1, 100.0);
        let vp = mat4_mul(proj, view);
        let clip = mat4_transform_point(vp, [0.0, 0.0, -5.0]);
        assert!(clip[3] > 0.0, "clip.w must be positive for visible points");
        let ndc_x = clip[0] / clip[3];
        let ndc_y = clip[1] / clip[3];
        assert!(ndc_x.abs() < 0.01, "point should be at screen center X, got {ndc_x}");
        assert!(ndc_y.abs() < 0.01, "point should be at screen center Y, got {ndc_y}");
    }

    #[test]
    fn mat4_transform_point_identity() {
        let id = mat4_identity();
        let result = mat4_transform_point(id, [3.0, 4.0, 5.0]);
        assert!((result[0] - 3.0).abs() < 1e-6);
        assert!((result[1] - 4.0).abs() < 1e-6);
        assert!((result[2] - 5.0).abs() < 1e-6);
        assert!((result[3] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn mat4_transform_point_with_translation() {
        let t = mat4_translation(10.0, 20.0, 30.0);
        let result = mat4_transform_point(t, [1.0, 2.0, 3.0]);
        assert!((result[0] - 11.0).abs() < 1e-6);
        assert!((result[1] - 22.0).abs() < 1e-6);
        assert!((result[2] - 33.0).abs() < 1e-6);
    }

    #[test]
    fn mat4_transform_point_perspective_visible() {
        // A point in front of the camera should have positive clip.w
        let proj = mat4_perspective(std::f32::consts::FRAC_PI_4, 1.0, 0.1, 100.0);
        let view = mat4_look_at([0.0, 5.0, 10.0], [0.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
        let vp = mat4_mul(proj, view);
        let clip = mat4_transform_point(vp, [0.0, 0.0, 0.0]);
        assert!(clip[3] > 0.0, "target point should be visible (w > 0)");
    }
}
