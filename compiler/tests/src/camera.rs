//! A frame's camera as a program reports it (the examples' `test_camera`), and the vector
//! arithmetic the image tests do with it: a pixel's world point from its depth, and a world
//! point's pixel.

/// The camera a frame was drawn by (an example's `CameraReport`): its reversed-depth
/// perspective, its jitter in clip units, and the scene's size.
#[derive(Clone, Copy, Debug)]
pub struct Cam {
    pub eye: [f64; 3],
    pub forward: [f64; 3],
    pub right: [f64; 3],
    pub up: [f64; 3],
    pub tan_half: f64,
    pub aspect: f64,
    pub near: f64,
    pub jitter: [f64; 2],
    pub screen: [f64; 2],
}

pub fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub fn add_scaled(a: [f64; 3], b: [f64; 3], k: f64) -> [f64; 3] {
    [a[0] + b[0] * k, a[1] + b[1] * k, a[2] + b[2] * k]
}

pub fn len(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}

impl Cam {
    /// The camera from a `test_camera` export's floats: eye, forward, right, up, the tangent of
    /// half the vertical field of view, the aspect, the near plane, the jitter and the scene's
    /// size.
    pub fn of(v: &[f64]) -> Cam {
        let v3 = |i: usize| [v[i], v[i + 1], v[i + 2]];
        Cam {
            eye: v3(0),
            forward: v3(3),
            right: v3(6),
            up: v3(9),
            tan_half: v[12],
            aspect: v[13],
            near: v[14],
            jitter: [v[15], v[16]],
            screen: [v[17], v[18]],
        }
    }

    /// The ray through pixel `px` (scene pixels, its centre at +0.5), unit length.
    pub fn ray(&self, px: [f64; 2]) -> [f64; 3] {
        let ndc = [
            px[0] / self.screen[0] * 2.0 - 1.0 - self.jitter[0],
            1.0 - px[1] / self.screen[1] * 2.0 - self.jitter[1],
        ];
        let mut ray = add_scaled(self.forward, self.right, ndc[0] * self.tan_half * self.aspect);
        ray = add_scaled(ray, self.up, ndc[1] * self.tan_half);
        let l = len(ray);
        ray.map(|x| x / l)
    }

    /// How far along pixel `px`'s ray its depth `z` (reversed) is.
    pub fn distance_at(&self, px: [f64; 2], z: f64) -> f64 {
        let ray = self.ray(px);
        (self.near / z.max(1e-12)) / dot(ray, self.forward)
    }

    /// The world point at pixel `px` (scene pixels, its centre at +0.5) of depth `z` (reversed).
    pub fn world_at(&self, px: [f64; 2], z: f64) -> [f64; 3] {
        let ray = self.ray(px);
        let dist = self.near / z.max(1e-12);
        add_scaled(self.eye, ray, dist / dot(ray, self.forward))
    }

    /// Where `p` falls on a target `size` wide and high (pixels, jittered as this frame drew),
    /// and its depth there (reversed); none behind the eye.
    pub fn project(&self, p: [f64; 3], size: [f64; 2]) -> Option<([f64; 2], f64)> {
        let v = sub(p, self.eye);
        let z = dot(v, self.forward);
        if z <= self.near {
            return None;
        }
        let x = dot(v, self.right) / (z * self.tan_half * self.aspect) + self.jitter[0];
        let y = dot(v, self.up) / (z * self.tan_half) + self.jitter[1];
        Some(([(x + 1.0) * 0.5 * size[0], (1.0 - y) * 0.5 * size[1]], self.near / z))
    }
}
