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

/// Build a perspective projection matrix with sub-pixel jitter for TAA.
///
/// `jitter_x` and `jitter_y` are in **pixel** coordinates (e.g. Halton offset
/// divided by resolution). They are translated into NDC and applied as an
/// offset to the projection matrix's translation column.
pub fn mat4_perspective_jittered(
    fov_y: f32,
    aspect: f32,
    near: f32,
    far: f32,
    jitter_x: f32,
    jitter_y: f32,
) -> Mat4 {
    let mut p = mat4_perspective(fov_y, aspect, near, far);
    // jitter_x / jitter_y are already in NDC-pixel space (pixel_offset / resolution).
    // Translate that into clip-space by scaling by 2 (NDC range is [-1,1]).
    p[2][0] += jitter_x * 2.0;
    p[2][1] += jitter_y * 2.0;
    p
}

/// Halton sequence value for a given index and base.
/// Used to generate low-discrepancy sub-pixel jitter offsets for TAA.
pub fn halton(index: u32, base: u32) -> f32 {
    let mut result = 0.0f32;
    let mut f = 1.0f32;
    let mut i = index;
    while i > 0 {
        f /= base as f32;
        result += f * (i % base) as f32;
        i /= base;
    }
    result
}

/// TAA jitter offset for a given frame index, using Halton(2,3) sequence
/// with an 8-frame cycle. Returns (jitter_x, jitter_y) in pixel coordinates,
/// centered around zero (i.e. in [-0.5, 0.5] range).
pub fn taa_jitter(frame_index: u32) -> (f32, f32) {
    let idx = (frame_index % TAA_JITTER_SEQUENCE_LENGTH) + 1; // Halton(0) = 0, so start at 1
    (halton(idx, 2) - 0.5, halton(idx, 3) - 0.5)
}

/// Number of frames in the TAA jitter sequence cycle.
pub const TAA_JITTER_SEQUENCE_LENGTH: u32 = 8;

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

/// Reverse-Z perspective projection matrix (right-handed, near maps to 1.0, far maps to 0.0).
/// This provides better depth precision for distant objects due to the non-linear distribution
/// of floating-point values near zero.
pub fn mat4_perspective_reverse_z(fov_y: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
    let f = 1.0 / (fov_y * 0.5).tan();
    let range_inv = 1.0 / (far - near);
    [
        [f / aspect, 0.0, 0.0, 0.0],
        [0.0, f, 0.0, 0.0],
        [0.0, 0.0, near * range_inv, -1.0],
        [0.0, 0.0, far * near * range_inv, 0.0],
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
        [-(right + left) / rl, -(top + bottom) / tb, -near / fn_, 1.0],
    ]
}

/// Transform a 3D point by a 4x4 matrix (column-major). Returns [x, y, z, w].
pub fn mat4_transform_point(m: Mat4, p: [f32; 3]) -> [f32; 4] {
    [
        m[0][0] * p[0] + m[1][0] * p[1] + m[2][0] * p[2] + m[3][0],
        m[0][1] * p[0] + m[1][1] * p[1] + m[2][1] * p[2] + m[3][1],
        m[0][2] * p[0] + m[1][2] * p[1] + m[2][2] * p[2] + m[3][2],
        m[0][3] * p[0] + m[1][3] * p[1] + m[2][3] * p[2] + m[3][3],
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

/// Camera framing mode: exploration is wider/higher, combat is tighter/lower.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CameraMode {
    Exploration,
    Combat,
}

impl Default for CameraMode {
    fn default() -> Self {
        CameraMode::Exploration
    }
}

/// Camera mode parameters: distance, elevation, fov.
#[derive(Clone, Copy, Debug)]
struct CameraModeParams {
    distance: f32,
    elevation: f32,
    fov_y: f32,
}

impl CameraModeParams {
    fn exploration() -> Self {
        Self {
            distance: 8.0,   // closer follow for cinematic feel
            elevation: 0.25, // lower angle to emphasize tree height
            fov_y: std::f32::consts::FRAC_PI_4,
        }
    }

    fn combat() -> Self {
        Self {
            distance: 6.0,  // tighter framing in combat
            elevation: 0.18, // even lower for imposing feel
            fov_y: std::f32::consts::FRAC_PI_4 - 0.03, // slightly narrower
        }
    }

    fn lerp(a: &Self, b: &Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        Self {
            distance: a.distance + (b.distance - a.distance) * t,
            elevation: a.elevation + (b.elevation - a.elevation) * t,
            fov_y: a.fov_y + (b.fov_y - a.fov_y) * t,
        }
    }
}

/// Elevation clamp bounds to keep the hero readable on screen.
pub const MIN_ELEVATION: f32 = 0.1;
pub const MAX_ELEVATION: f32 = 0.6;

pub struct OrbitCamera {
    pub azimuth: f32,
    pub elevation: f32,
    pub distance: f32,
    pub target: [f32; 3],
    pub fov_y: f32,
    /// Current camera mode.
    mode: CameraMode,
    /// Blend factor for mode transitions (0.0 = previous mode, 1.0 = current mode).
    mode_blend: f32,
    /// Parameters for the mode we are blending FROM.
    blend_from: CameraModeParams,
    /// Parameters for the mode we are blending TO.
    blend_to: CameraModeParams,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        let params = CameraModeParams::exploration();
        Self {
            azimuth: 0.0,
            elevation: params.elevation,
            distance: params.distance,
            target: [0.0, 0.0, 0.0],
            fov_y: params.fov_y,
            mode: CameraMode::Exploration,
            mode_blend: 1.0,
            blend_from: params,
            blend_to: params,
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
        mat4_perspective_reverse_z(self.fov_y, aspect, 0.1, 500.0)
    }

    /// Jittered projection for TAA. `jitter_x` and `jitter_y` are in pixel
    /// coordinates (typically from `taa_jitter()`), divided by the render
    /// resolution before being passed here.
    pub fn projection_matrix_jittered(
        &self,
        aspect: f32,
        jitter_x: f32,
        jitter_y: f32,
    ) -> Mat4 {
        mat4_perspective_jittered(self.fov_y, aspect, 0.1, 500.0, jitter_x, jitter_y)
    }

    pub fn view_projection(&self, aspect: f32) -> Mat4 {
        mat4_mul(self.projection_matrix(aspect), self.view_matrix())
    }

    /// View-projection with TAA sub-pixel jitter applied.
    pub fn view_projection_jittered(
        &self,
        aspect: f32,
        jitter_x: f32,
        jitter_y: f32,
    ) -> Mat4 {
        mat4_mul(
            self.projection_matrix_jittered(aspect, jitter_x, jitter_y),
            self.view_matrix(),
        )
    }

    pub fn rotate(&mut self, delta_azimuth: f32, delta_elevation: f32) {
        self.azimuth += delta_azimuth;
        self.elevation = (self.elevation + delta_elevation).clamp(MIN_ELEVATION, MAX_ELEVATION);
    }

    pub fn zoom(&mut self, delta: f32) {
        self.distance = (self.distance + delta).clamp(1.0, 200.0);
    }

    /// Smoothly follow a target position with exponential damping.
    /// `convergence_rate` controls how fast the camera catches up (~5.0 is good).
    pub fn smooth_follow(&mut self, target_pos: [f32; 3], dt: f32, convergence_rate: f32) {
        let smooth_factor = 1.0 - (-convergence_rate * dt).exp();
        self.target[0] += (target_pos[0] - self.target[0]) * smooth_factor;
        self.target[1] += (target_pos[1] - self.target[1]) * smooth_factor;
        self.target[2] += (target_pos[2] - self.target[2]) * smooth_factor;
    }

    /// Switch camera mode with a smooth blend transition.
    pub fn set_mode(&mut self, mode: CameraMode) {
        if mode == self.mode {
            return;
        }
        // Capture current blended params as the "from" state
        self.blend_from = CameraModeParams {
            distance: self.distance,
            elevation: self.elevation,
            fov_y: self.fov_y,
        };
        self.blend_to = match mode {
            CameraMode::Exploration => CameraModeParams::exploration(),
            CameraMode::Combat => CameraModeParams::combat(),
        };
        self.mode = mode;
        self.mode_blend = 0.0;
    }

    /// Advance mode transition blend. Call each frame with dt.
    /// `blend_speed` controls transition speed (~3.0 = ~0.3s transition).
    pub fn update_mode_blend(&mut self, dt: f32, blend_speed: f32) {
        if self.mode_blend >= 1.0 {
            return;
        }
        self.mode_blend = (self.mode_blend + dt * blend_speed).min(1.0);

        // Smooth step for a nicer ease curve
        let t = self.mode_blend;
        let smooth_t = t * t * (3.0 - 2.0 * t);

        let blended = CameraModeParams::lerp(&self.blend_from, &self.blend_to, smooth_t);
        self.distance = blended.distance;
        self.elevation = blended.elevation.clamp(MIN_ELEVATION, MAX_ELEVATION);
        self.fov_y = blended.fov_y;
    }

    /// Current camera mode.
    pub fn mode(&self) -> CameraMode {
        self.mode
    }

    /// Whether mode transition is complete.
    pub fn mode_blend_complete(&self) -> bool {
        self.mode_blend >= 1.0
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
        assert!(camera.elevation <= MAX_ELEVATION + 1e-6);
        camera.rotate(0.0, -200.0);
        assert!(camera.elevation >= MIN_ELEVATION - 1e-6);
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
        fog_color_and_start: [f32; 4],
        fog_params: [f32; 4],
        wind_params: [f32; 4],
        wind_dir: [f32; 4],
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct ModelUniformLayout {
        model: [[f32; 4]; 4],
        normal_model: [[f32; 4]; 4],
    }

    #[test]
    fn frame_uniform_3d_size_is_208_bytes() {
        // 64 (view_proj) + 10 * 16 (vec4 fields) = 64 + 144 = 224 -- wrong
        // Actually: mat4(64) + 9 * vec4(16 each) = 64 + 144 = 208
        assert_eq!(std::mem::size_of::<FrameUniform3DLayout>(), 208);
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
        let mut camera = OrbitCamera::default();
        camera.azimuth = 1.0;
        camera.elevation = 0.3;
        camera.distance = 7.0;
        camera.target = [0.0, 0.0, 0.0];
        camera.fov_y = std::f32::consts::FRAC_PI_4;
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
            ortho[0][0] * center[0]
                + ortho[1][0] * center[1]
                + ortho[2][0] * center[2]
                + ortho[3][0] * center[3],
            ortho[0][1] * center[0]
                + ortho[1][1] * center[1]
                + ortho[2][1] * center[2]
                + ortho[3][1] * center[3],
            ortho[0][2] * center[0]
                + ortho[1][2] * center[1]
                + ortho[2][2] * center[2]
                + ortho[3][2] * center[3],
            ortho[0][3] * center[0]
                + ortho[1][3] * center[1]
                + ortho[2][3] * center[2]
                + ortho[3][3] * center[3],
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
            ortho[0][0] * corner[0]
                + ortho[1][0] * corner[1]
                + ortho[2][0] * corner[2]
                + ortho[3][0] * corner[3],
            ortho[0][1] * corner[0]
                + ortho[1][1] * corner[1]
                + ortho[2][1] * corner[2]
                + ortho[3][1] * corner[3],
            ortho[0][2] * corner[0]
                + ortho[1][2] * corner[1]
                + ortho[2][2] * corner[2]
                + ortho[3][2] * corner[3],
            ortho[0][3] * corner[0]
                + ortho[1][3] * corner[1]
                + ortho[2][3] * corner[2]
                + ortho[3][3] * corner[3],
        ];
        assert!(
            approx_eq(result[0], -1.0, 1e-5),
            "left should map to -1, got {}",
            result[0]
        );
        assert!(
            approx_eq(result[1], -1.0, 1e-5),
            "bottom should map to -1, got {}",
            result[1]
        );
        assert!(
            approx_eq(result[2], 0.0, 1e-5),
            "near should map to 0 for WebGPU depth, got {}",
            result[2]
        );
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
        assert!(
            ndc_x.abs() < 0.01,
            "point should be at screen center X, got {ndc_x}"
        );
        assert!(
            ndc_y.abs() < 0.01,
            "point should be at screen center Y, got {ndc_y}"
        );
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

    #[test]
    fn halton_base2_first_values() {
        // Halton(1,2) = 0.5, Halton(2,2) = 0.25, Halton(3,2) = 0.75
        assert!(approx_eq(halton(1, 2), 0.5, 1e-6));
        assert!(approx_eq(halton(2, 2), 0.25, 1e-6));
        assert!(approx_eq(halton(3, 2), 0.75, 1e-6));
    }

    #[test]
    fn halton_base3_first_values() {
        // Halton(1,3) = 1/3, Halton(2,3) = 2/3
        assert!(approx_eq(halton(1, 3), 1.0 / 3.0, 1e-6));
        assert!(approx_eq(halton(2, 3), 2.0 / 3.0, 1e-6));
    }

    #[test]
    fn halton_zero_is_zero() {
        assert!(approx_eq(halton(0, 2), 0.0, 1e-6));
        assert!(approx_eq(halton(0, 3), 0.0, 1e-6));
    }

    #[test]
    fn taa_jitter_is_centered() {
        // All jitter values should be in [-0.5, 0.5]
        for i in 0..TAA_JITTER_SEQUENCE_LENGTH {
            let (jx, jy) = taa_jitter(i);
            assert!(jx >= -0.5 && jx <= 0.5, "jitter_x out of range: {jx}");
            assert!(jy >= -0.5 && jy <= 0.5, "jitter_y out of range: {jy}");
        }
    }

    #[test]
    fn taa_jitter_cycles() {
        // Frame N and frame N+8 should produce the same jitter
        let (jx0, jy0) = taa_jitter(0);
        let (jx8, jy8) = taa_jitter(TAA_JITTER_SEQUENCE_LENGTH);
        assert!(approx_eq(jx0, jx8, 1e-6));
        assert!(approx_eq(jy0, jy8, 1e-6));
    }

    #[test]
    fn perspective_jittered_zero_jitter_matches_standard() {
        let fov = std::f32::consts::FRAC_PI_4;
        let aspect = 16.0 / 9.0;
        let near = 0.1;
        let far = 100.0;
        let standard = mat4_perspective(fov, aspect, near, far);
        let jittered = mat4_perspective_jittered(fov, aspect, near, far, 0.0, 0.0);
        assert!(
            mat4_approx_eq(&standard, &jittered, 1e-6),
            "Zero jitter should produce identical projection matrix"
        );
    }

    #[test]
    fn perspective_jittered_shifts_ndc() {
        let fov = std::f32::consts::FRAC_PI_4;
        let aspect = 1.0;
        let near = 0.1;
        let far = 100.0;
        // A small jitter of 0.5 pixels / 1000 pixels should shift the center
        let jx = 0.5 / 1000.0;
        let jy = 0.5 / 1000.0;
        let jittered = mat4_perspective_jittered(fov, aspect, near, far, jx, jy);
        let standard = mat4_perspective(fov, aspect, near, far);

        // Only the [2][0] and [2][1] elements should differ
        assert!(
            !approx_eq(jittered[2][0], standard[2][0], 1e-8),
            "jittered [2][0] should differ from standard"
        );
        assert!(
            !approx_eq(jittered[2][1], standard[2][1], 1e-8),
            "jittered [2][1] should differ from standard"
        );
        // Other elements should be identical
        assert!(approx_eq(jittered[0][0], standard[0][0], 1e-8));
        assert!(approx_eq(jittered[1][1], standard[1][1], 1e-8));
        assert!(approx_eq(jittered[2][2], standard[2][2], 1e-8));
    }

    #[test]
    fn orbit_camera_jittered_projection() {
        let camera = OrbitCamera::default();
        let aspect = 16.0 / 9.0;
        let (jx, jy) = taa_jitter(0);
        let width = 1920.0f32;
        let height = 1080.0f32;
        let proj = camera.projection_matrix_jittered(aspect, jx / width, jy / height);
        // Should be valid (non-zero diagonal)
        assert!(proj[0][0].abs() > 0.01);
        assert!(proj[1][1].abs() > 0.01);
    }

    // -----------------------------------------------------------------------
    // Smooth follow tests
    // -----------------------------------------------------------------------

    #[test]
    fn smooth_follow_moves_toward_target() {
        let mut camera = OrbitCamera::default();
        camera.target = [0.0, 0.0, 0.0];
        let goal = [10.0, 5.0, 3.0];
        camera.smooth_follow(goal, 0.016, 5.0);
        // Should have moved toward the goal
        assert!(camera.target[0] > 0.0, "x should move toward goal");
        assert!(camera.target[1] > 0.0, "y should move toward goal");
        assert!(camera.target[2] > 0.0, "z should move toward goal");
        // But not all the way there in one frame
        assert!(camera.target[0] < 10.0, "x should not overshoot");
    }

    #[test]
    fn smooth_follow_converges_with_large_dt() {
        let mut camera = OrbitCamera::default();
        camera.target = [0.0, 0.0, 0.0];
        let goal = [10.0, 5.0, 3.0];
        // Large dt => should nearly converge
        camera.smooth_follow(goal, 10.0, 5.0);
        assert!(
            approx_eq(camera.target[0], 10.0, 0.01),
            "should converge with large dt, got {}",
            camera.target[0]
        );
        assert!(approx_eq(camera.target[1], 5.0, 0.01));
        assert!(approx_eq(camera.target[2], 3.0, 0.01));
    }

    #[test]
    fn smooth_follow_no_movement_when_at_target() {
        let mut camera = OrbitCamera::default();
        camera.target = [5.0, 3.0, 1.0];
        let goal = [5.0, 3.0, 1.0];
        camera.smooth_follow(goal, 0.016, 5.0);
        assert!(approx_eq(camera.target[0], 5.0, 1e-6));
        assert!(approx_eq(camera.target[1], 3.0, 1e-6));
        assert!(approx_eq(camera.target[2], 1.0, 1e-6));
    }

    // -----------------------------------------------------------------------
    // Camera mode tests
    // -----------------------------------------------------------------------

    #[test]
    fn camera_default_mode_is_exploration() {
        let camera = OrbitCamera::default();
        assert_eq!(camera.mode(), CameraMode::Exploration);
        assert!(camera.mode_blend_complete());
    }

    #[test]
    fn camera_default_distance_is_cinematic() {
        let camera = OrbitCamera::default();
        // Exploration default: distance=8.0, elevation=0.25
        assert!(
            approx_eq(camera.distance, 8.0, 0.01),
            "default distance should be 8.0 (cinematic), got {}",
            camera.distance
        );
        assert!(
            approx_eq(camera.elevation, 0.25, 0.01),
            "default elevation should be 0.25 (low angle), got {}",
            camera.elevation
        );
    }

    #[test]
    fn set_mode_combat_starts_blend() {
        let mut camera = OrbitCamera::default();
        camera.set_mode(CameraMode::Combat);
        assert_eq!(camera.mode(), CameraMode::Combat);
        assert!(!camera.mode_blend_complete());
    }

    #[test]
    fn set_mode_same_is_noop() {
        let mut camera = OrbitCamera::default();
        camera.set_mode(CameraMode::Exploration);
        assert!(camera.mode_blend_complete());
    }

    #[test]
    fn mode_blend_completes_after_enough_time() {
        let mut camera = OrbitCamera::default();
        camera.set_mode(CameraMode::Combat);
        // Blend at speed 3.0 for 1 second -> mode_blend should reach 1.0
        for _ in 0..100 {
            camera.update_mode_blend(0.016, 3.0);
        }
        assert!(
            camera.mode_blend_complete(),
            "mode blend should complete after ~1.6s"
        );
        // Should be at combat params
        assert!(
            approx_eq(camera.distance, 6.0, 0.1),
            "combat distance should be ~6.0, got {}",
            camera.distance
        );
        assert!(
            camera.elevation >= MIN_ELEVATION - 1e-6,
            "elevation should be clamped above MIN_ELEVATION"
        );
        assert!(
            camera.elevation <= MAX_ELEVATION + 1e-6,
            "elevation should be clamped below MAX_ELEVATION"
        );
    }

    #[test]
    fn mode_blend_interpolates_smoothly() {
        let mut camera = OrbitCamera::default();
        let initial_distance = camera.distance;
        camera.set_mode(CameraMode::Combat);
        camera.update_mode_blend(0.1, 3.0); // partial blend
        // Distance should be between exploration (8.0) and combat (6.0)
        assert!(
            camera.distance < initial_distance,
            "distance should decrease toward combat"
        );
        assert!(
            camera.distance > 6.0,
            "distance should not yet be at combat target"
        );
    }

    #[test]
    fn elevation_clamp_enforced_in_mode_blend() {
        let mut camera = OrbitCamera::default();
        // Force elevation to something out of range to verify clamp
        camera.elevation = 2.0;
        camera.set_mode(CameraMode::Combat);
        camera.update_mode_blend(10.0, 100.0); // instant blend
        assert!(
            camera.elevation >= MIN_ELEVATION - 1e-6,
            "elevation must be >= MIN_ELEVATION after blend"
        );
        assert!(
            camera.elevation <= MAX_ELEVATION + 1e-6,
            "elevation must be <= MAX_ELEVATION after blend"
        );
    }
}
