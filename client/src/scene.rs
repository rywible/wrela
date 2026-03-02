#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

use crate::bvh::Aabb;

pub struct SceneNode {
    pub mesh_handle: Option<usize>,
    pub local_transform: [[f32; 4]; 4],
    pub world_transform: [[f32; 4]; 4],
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    pub aabb: Aabb,
}

pub struct Scene {
    pub nodes: Vec<SceneNode>,
    #[cfg(target_arch = "wasm32")]
    pub meshes: Vec<crate::mesh::GpuMesh>,
}

impl Scene {
    #[cfg(target_arch = "wasm32")]
    pub fn new() -> Self {
        Scene {
            nodes: Vec::new(),
            meshes: Vec::new(),
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn new() -> Self {
        Scene {
            nodes: Vec::new(),
        }
    }

    pub fn add_node(
        &mut self,
        mesh_handle: Option<usize>,
        local_transform: [[f32; 4]; 4],
        aabb: Aabb,
        parent: Option<usize>,
    ) -> usize {
        let index = self.nodes.len();
        self.nodes.push(SceneNode {
            mesh_handle,
            local_transform,
            world_transform: identity_mat4(),
            parent,
            children: Vec::new(),
            aabb,
        });
        if let Some(parent_idx) = parent {
            if parent_idx < self.nodes.len() - 1 {
                self.nodes[parent_idx].children.push(index);
            }
        }
        index
    }

    pub fn update_world_transforms(&mut self) {
        let root_nodes: Vec<usize> = self
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.parent.is_none())
            .map(|(i, _)| i)
            .collect();
        for root in root_nodes {
            self.propagate_transform(root, &identity_mat4());
        }
    }

    fn propagate_transform(&mut self, node_idx: usize, parent_world: &[[f32; 4]; 4]) {
        let local = self.nodes[node_idx].local_transform;
        let world = mat4_multiply(parent_world, &local);
        self.nodes[node_idx].world_transform = world;
        let children = self.nodes[node_idx].children.clone();
        for child in children {
            self.propagate_transform(child, &world);
        }
    }
}

pub fn identity_mat4() -> [[f32; 4]; 4] {
    [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

pub fn translation_mat4(x: f32, y: f32, z: f32) -> [[f32; 4]; 4] {
    [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [x, y, z, 1.0],
    ]
}

pub fn scale_mat4(sx: f32, sy: f32, sz: f32) -> [[f32; 4]; 4] {
    [
        [sx, 0.0, 0.0, 0.0],
        [0.0, sy, 0.0, 0.0],
        [0.0, 0.0, sz, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

/// 2D scene-graph matrix multiply. Self-consistent with `translation_mat4` below.
/// NOTE: Do NOT use for WebGPU/3D operations — use `camera_math::mat4_mul` instead.
pub fn mat4_multiply(a: &[[f32; 4]; 4], b: &[[f32; 4]; 4]) -> [[f32; 4]; 4] {
    let mut result = [[0.0f32; 4]; 4];
    for i in 0..4 {
        for j in 0..4 {
            for k in 0..4 {
                result[i][j] += a[i][k] * b[k][j];
            }
        }
    }
    result
}

/// Transpose of the inverse of the upper-left 3x3 of a model matrix,
/// extended to 4x4. Used for transforming normals correctly under
/// non-uniform scaling.
pub fn normal_matrix(model: &[[f32; 4]; 4]) -> [[f32; 4]; 4] {
    // Extract 3x3
    let a = model[0][0];
    let b = model[0][1];
    let c = model[0][2];
    let d = model[1][0];
    let e = model[1][1];
    let f = model[1][2];
    let g = model[2][0];
    let h = model[2][1];
    let i = model[2][2];

    let det = a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g);
    if det.abs() < 1e-10 {
        return identity_mat4();
    }
    let inv_det = 1.0 / det;

    // Inverse 3x3 then transpose
    [
        [
            (e * i - f * h) * inv_det,
            (f * g - d * i) * inv_det,
            (d * h - e * g) * inv_det,
            0.0,
        ],
        [
            (c * h - b * i) * inv_det,
            (a * i - c * g) * inv_det,
            (b * g - a * h) * inv_det,
            0.0,
        ],
        [
            (b * f - c * e) * inv_det,
            (c * d - a * f) * inv_det,
            (a * e - b * d) * inv_det,
            0.0,
        ],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

pub fn perspective_mat4(fov_y_rad: f32, aspect: f32, near: f32, far: f32) -> [[f32; 4]; 4] {
    let f = 1.0 / (fov_y_rad * 0.5).tan();
    let nf = 1.0 / (near - far);
    [
        [f / aspect, 0.0, 0.0, 0.0],
        [0.0, f, 0.0, 0.0],
        [0.0, 0.0, (far + near) * nf, -1.0],
        [0.0, 0.0, 2.0 * far * near * nf, 0.0],
    ]
}

pub fn look_at_mat4(eye: [f32; 3], target: [f32; 3], up: [f32; 3]) -> [[f32; 4]; 4] {
    let f = normalize_vec3(sub_vec3(target, eye));
    let s = normalize_vec3(cross_vec3(f, up));
    let u = cross_vec3(s, f);

    [
        [s[0], u[0], -f[0], 0.0],
        [s[1], u[1], -f[1], 0.0],
        [s[2], u[2], -f[2], 0.0],
        [-dot_vec3(s, eye), -dot_vec3(u, eye), dot_vec3(f, eye), 1.0],
    ]
}

fn sub_vec3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross_vec3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot_vec3(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn normalize_vec3(v: [f32; 3]) -> [f32; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len < 1e-10 {
        return [0.0, 0.0, 0.0];
    }
    [v[0] / len, v[1] / len, v[2] / len]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_multiply_is_identity() {
        let id = identity_mat4();
        let result = mat4_multiply(&id, &id);
        for i in 0..4 {
            for j in 0..4 {
                let expected = if i == j { 1.0 } else { 0.0 };
                assert!(
                    (result[i][j] - expected).abs() < 1e-6,
                    "result[{i}][{j}] = {} expected {expected}",
                    result[i][j]
                );
            }
        }
    }

    #[test]
    fn translation_matrix_sets_position() {
        let t = translation_mat4(3.0, 5.0, 7.0);
        assert_eq!(t[3][0], 3.0);
        assert_eq!(t[3][1], 5.0);
        assert_eq!(t[3][2], 7.0);
        assert_eq!(t[3][3], 1.0);
        // Diagonal should be 1
        assert_eq!(t[0][0], 1.0);
        assert_eq!(t[1][1], 1.0);
        assert_eq!(t[2][2], 1.0);
    }

    #[test]
    fn scale_matrix_sets_diagonal() {
        let s = scale_mat4(2.0, 3.0, 4.0);
        assert_eq!(s[0][0], 2.0);
        assert_eq!(s[1][1], 3.0);
        assert_eq!(s[2][2], 4.0);
        assert_eq!(s[3][3], 1.0);
    }

    #[test]
    fn transform_propagation_composes_parent_child() {
        let mut scene = Scene::new();
        let parent_transform = translation_mat4(10.0, 0.0, 0.0);
        let child_transform = translation_mat4(0.0, 5.0, 0.0);

        let parent_aabb = Aabb {
            min: [-1.0, -1.0, -1.0],
            max: [1.0, 1.0, 1.0],
        };

        let parent_idx = scene.add_node(None, parent_transform, parent_aabb.clone(), None);
        let child_idx = scene.add_node(None, child_transform, parent_aabb.clone(), Some(parent_idx));

        scene.update_world_transforms();

        // Parent world transform should be the parent local
        let pw = &scene.nodes[parent_idx].world_transform;
        assert!((pw[3][0] - 10.0).abs() < 1e-6);
        assert!((pw[3][1] - 0.0).abs() < 1e-6);

        // Child world transform should compose parent + child
        let cw = &scene.nodes[child_idx].world_transform;
        assert!((cw[3][0] - 10.0).abs() < 1e-6);
        assert!((cw[3][1] - 5.0).abs() < 1e-6);
    }

    #[test]
    fn transform_propagation_with_multiple_roots() {
        let mut scene = Scene::new();
        let aabb = Aabb {
            min: [0.0; 3],
            max: [1.0; 3],
        };

        let root_a = scene.add_node(None, translation_mat4(1.0, 0.0, 0.0), aabb.clone(), None);
        let root_b = scene.add_node(None, translation_mat4(0.0, 2.0, 0.0), aabb.clone(), None);

        scene.update_world_transforms();

        assert!((scene.nodes[root_a].world_transform[3][0] - 1.0).abs() < 1e-6);
        assert!((scene.nodes[root_b].world_transform[3][1] - 2.0).abs() < 1e-6);
    }

    #[test]
    fn normal_matrix_identity_for_identity_model() {
        let id = identity_mat4();
        let nm = normal_matrix(&id);
        for i in 0..4 {
            for j in 0..4 {
                let expected = if i == j { 1.0 } else { 0.0 };
                assert!(
                    (nm[i][j] - expected).abs() < 1e-6,
                    "nm[{i}][{j}] = {} expected {expected}",
                    nm[i][j]
                );
            }
        }
    }

    #[test]
    fn normal_matrix_handles_uniform_scale() {
        let s = scale_mat4(2.0, 2.0, 2.0);
        let nm = normal_matrix(&s);
        // For uniform scale of 2, normal matrix should be scale of 1/2
        assert!((nm[0][0] - 0.5).abs() < 1e-5);
        assert!((nm[1][1] - 0.5).abs() < 1e-5);
        assert!((nm[2][2] - 0.5).abs() < 1e-5);
    }

    #[test]
    fn perspective_matrix_produces_valid_projection() {
        let fov = std::f32::consts::FRAC_PI_4;
        let proj = perspective_mat4(fov, 16.0 / 9.0, 0.1, 100.0);
        // [0][0] and [1][1] should be non-zero positive
        assert!(proj[0][0] > 0.0);
        assert!(proj[1][1] > 0.0);
        // [2][3] should be -1 for standard perspective
        assert!((proj[2][3] - (-1.0)).abs() < 1e-6);
    }

    #[test]
    fn look_at_identity_looking_down_neg_z() {
        let view = look_at_mat4([0.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]);
        // Looking down -Z from origin, should approximate identity
        // The forward vector aligns with -Z, so the view matrix third row
        // should be [0,0,1,...] (negated forward)
        assert!((view[0][0] - 1.0).abs() < 1e-5);
        assert!((view[1][1] - 1.0).abs() < 1e-5);
        assert!((view[2][2] - 1.0).abs() < 1e-5);
    }
}
