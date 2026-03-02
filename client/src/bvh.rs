#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

#[derive(Debug, Clone)]
pub struct Aabb {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

impl Aabb {
    pub fn from_positions(positions: &[[f32; 3]]) -> Self {
        let mut min = [f32::MAX; 3];
        let mut max = [f32::MIN; 3];
        for pos in positions {
            for i in 0..3 {
                min[i] = min[i].min(pos[i]);
                max[i] = max[i].max(pos[i]);
            }
        }
        Aabb { min, max }
    }

    pub fn center(&self) -> [f32; 3] {
        [
            (self.min[0] + self.max[0]) * 0.5,
            (self.min[1] + self.max[1]) * 0.5,
            (self.min[2] + self.max[2]) * 0.5,
        ]
    }

    pub fn extents(&self) -> [f32; 3] {
        [
            self.max[0] - self.min[0],
            self.max[1] - self.min[1],
            self.max[2] - self.min[2],
        ]
    }
}

pub struct BvhNode {
    pub aabb: Aabb,
    pub left: Option<Box<BvhNode>>,
    pub right: Option<Box<BvhNode>>,
    pub scene_node_index: Option<usize>,
}

pub struct Frustum {
    pub planes: [[f32; 4]; 6],
}

impl Frustum {
    /// Extract frustum planes from a view-projection matrix (row-major).
    /// Each plane is [A, B, C, D] such that Ax + By + Cz + D >= 0 for inside.
    pub fn from_view_projection(vp: &[[f32; 4]; 4]) -> Self {
        let m = vp;
        let planes = [
            // Left
            [
                m[0][3] + m[0][0],
                m[1][3] + m[1][0],
                m[2][3] + m[2][0],
                m[3][3] + m[3][0],
            ],
            // Right
            [
                m[0][3] - m[0][0],
                m[1][3] - m[1][0],
                m[2][3] - m[2][0],
                m[3][3] - m[3][0],
            ],
            // Bottom
            [
                m[0][3] + m[0][1],
                m[1][3] + m[1][1],
                m[2][3] + m[2][1],
                m[3][3] + m[3][1],
            ],
            // Top
            [
                m[0][3] - m[0][1],
                m[1][3] - m[1][1],
                m[2][3] - m[2][1],
                m[3][3] - m[3][1],
            ],
            // Near
            [
                m[0][3] + m[0][2],
                m[1][3] + m[1][2],
                m[2][3] + m[2][2],
                m[3][3] + m[3][2],
            ],
            // Far
            [
                m[0][3] - m[0][2],
                m[1][3] - m[1][2],
                m[2][3] - m[2][2],
                m[3][3] - m[3][2],
            ],
        ];
        Frustum { planes }
    }

    /// Test if an AABB is at least partially inside the frustum.
    pub fn test_aabb(&self, aabb: &Aabb) -> bool {
        for plane in &self.planes {
            // Pick the positive vertex (the vertex most in the direction of the plane normal)
            let px = if plane[0] >= 0.0 {
                aabb.max[0]
            } else {
                aabb.min[0]
            };
            let py = if plane[1] >= 0.0 {
                aabb.max[1]
            } else {
                aabb.min[1]
            };
            let pz = if plane[2] >= 0.0 {
                aabb.max[2]
            } else {
                aabb.min[2]
            };
            if plane[0] * px + plane[1] * py + plane[2] * pz + plane[3] < 0.0 {
                return false;
            }
        }
        true
    }
}

pub fn build_bvh(nodes: &[(usize, Aabb)]) -> Option<BvhNode> {
    if nodes.is_empty() {
        return None;
    }
    if nodes.len() == 1 {
        return Some(BvhNode {
            aabb: nodes[0].1.clone(),
            left: None,
            right: None,
            scene_node_index: Some(nodes[0].0),
        });
    }

    let combined = combine_aabbs(nodes.iter().map(|(_, aabb)| aabb));

    // Split along longest axis
    let extent = combined.extents();
    let axis = if extent[0] >= extent[1] && extent[0] >= extent[2] {
        0
    } else if extent[1] >= extent[2] {
        1
    } else {
        2
    };

    let mut sorted: Vec<_> = nodes.to_vec();
    sorted.sort_by(|a, b| {
        let ca = (a.1.min[axis] + a.1.max[axis]) * 0.5;
        let cb = (b.1.min[axis] + b.1.max[axis]) * 0.5;
        ca.partial_cmp(&cb).unwrap_or(std::cmp::Ordering::Equal)
    });

    let mid = sorted.len() / 2;
    Some(BvhNode {
        aabb: combined,
        left: build_bvh(&sorted[..mid]).map(Box::new),
        right: build_bvh(&sorted[mid..]).map(Box::new),
        scene_node_index: None,
    })
}

fn combine_aabbs<'a>(aabbs: impl Iterator<Item = &'a Aabb>) -> Aabb {
    let mut min = [f32::MAX; 3];
    let mut max = [f32::MIN; 3];
    for aabb in aabbs {
        for i in 0..3 {
            min[i] = min[i].min(aabb.min[i]);
            max[i] = max[i].max(aabb.max[i]);
        }
    }
    Aabb { min, max }
}

pub fn cull_visible(bvh: &BvhNode, frustum: &Frustum) -> Vec<usize> {
    let mut visible = Vec::new();
    cull_recursive(bvh, frustum, &mut visible);
    visible
}

fn cull_recursive(node: &BvhNode, frustum: &Frustum, visible: &mut Vec<usize>) {
    if !frustum.test_aabb(&node.aabb) {
        return;
    }
    if let Some(idx) = node.scene_node_index {
        visible.push(idx);
    }
    if let Some(left) = &node.left {
        cull_recursive(left, frustum, visible);
    }
    if let Some(right) = &node.right {
        cull_recursive(right, frustum, visible);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{identity_mat4, mat4_multiply, perspective_mat4};

    #[test]
    fn aabb_from_positions_computes_bounds() {
        let positions = [
            [-1.0, -2.0, -3.0],
            [4.0, 5.0, 6.0],
            [0.0, 0.0, 0.0],
        ];
        let aabb = Aabb::from_positions(&positions);
        assert_eq!(aabb.min, [-1.0, -2.0, -3.0]);
        assert_eq!(aabb.max, [4.0, 5.0, 6.0]);
    }

    #[test]
    fn aabb_center_and_extents() {
        let aabb = Aabb {
            min: [0.0, 0.0, 0.0],
            max: [10.0, 20.0, 30.0],
        };
        assert_eq!(aabb.center(), [5.0, 10.0, 15.0]);
        assert_eq!(aabb.extents(), [10.0, 20.0, 30.0]);
    }

    #[test]
    fn build_bvh_empty_returns_none() {
        assert!(build_bvh(&[]).is_none());
    }

    #[test]
    fn build_bvh_single_node() {
        let aabb = Aabb {
            min: [0.0; 3],
            max: [1.0; 3],
        };
        let bvh = build_bvh(&[(42, aabb)]).expect("single node BVH");
        assert_eq!(bvh.scene_node_index, Some(42));
        assert!(bvh.left.is_none());
        assert!(bvh.right.is_none());
    }

    #[test]
    fn build_bvh_multiple_nodes_splits() {
        let nodes: Vec<(usize, Aabb)> = (0..4)
            .map(|i| {
                let x = i as f32 * 10.0;
                (
                    i,
                    Aabb {
                        min: [x, 0.0, 0.0],
                        max: [x + 1.0, 1.0, 1.0],
                    },
                )
            })
            .collect();

        let bvh = build_bvh(&nodes).expect("multi-node BVH");
        assert!(bvh.scene_node_index.is_none()); // Internal node
        assert!(bvh.left.is_some());
        assert!(bvh.right.is_some());
        // Combined AABB should span all nodes
        assert!((bvh.aabb.min[0] - 0.0).abs() < 1e-6);
        assert!((bvh.aabb.max[0] - 31.0).abs() < 1e-6);
    }

    #[test]
    fn frustum_culling_accepts_visible_aabb() {
        // Create a simple perspective-like VP matrix looking down -Z
        let proj = perspective_mat4(
            std::f32::consts::FRAC_PI_4,
            1.0,
            0.1,
            100.0,
        );
        let view = identity_mat4();
        let vp = mat4_multiply(&view, &proj);
        let frustum = Frustum::from_view_projection(&vp);

        // AABB at origin, should be visible (directly in front of camera)
        let aabb = Aabb {
            min: [-1.0, -1.0, -5.0],
            max: [1.0, 1.0, -1.0],
        };
        assert!(frustum.test_aabb(&aabb));
    }

    #[test]
    fn frustum_culling_rejects_aabb_behind_camera() {
        let proj = perspective_mat4(
            std::f32::consts::FRAC_PI_4,
            1.0,
            0.1,
            100.0,
        );
        let view = identity_mat4();
        let vp = mat4_multiply(&view, &proj);
        let frustum = Frustum::from_view_projection(&vp);

        // AABB behind camera (positive Z)
        let aabb = Aabb {
            min: [-1.0, -1.0, 5.0],
            max: [1.0, 1.0, 10.0],
        };
        assert!(!frustum.test_aabb(&aabb));
    }

    #[test]
    fn frustum_culling_rejects_far_away_aabb() {
        let proj = perspective_mat4(
            std::f32::consts::FRAC_PI_4,
            1.0,
            0.1,
            10.0,
        );
        let view = identity_mat4();
        let vp = mat4_multiply(&view, &proj);
        let frustum = Frustum::from_view_projection(&vp);

        // AABB far beyond far plane
        let aabb = Aabb {
            min: [-1.0, -1.0, -500.0],
            max: [1.0, 1.0, -200.0],
        };
        assert!(!frustum.test_aabb(&aabb));
    }

    #[test]
    fn cull_visible_returns_visible_nodes() {
        let proj = perspective_mat4(
            std::f32::consts::FRAC_PI_4,
            1.0,
            0.1,
            100.0,
        );
        let view = identity_mat4();
        let vp = mat4_multiply(&view, &proj);
        let frustum = Frustum::from_view_projection(&vp);

        let nodes = vec![
            // Visible: in front of camera
            (
                0,
                Aabb {
                    min: [-1.0, -1.0, -5.0],
                    max: [1.0, 1.0, -1.0],
                },
            ),
            // Not visible: behind camera
            (
                1,
                Aabb {
                    min: [-1.0, -1.0, 5.0],
                    max: [1.0, 1.0, 10.0],
                },
            ),
            // Visible: also in front
            (
                2,
                Aabb {
                    min: [-0.5, -0.5, -3.0],
                    max: [0.5, 0.5, -2.0],
                },
            ),
        ];

        let bvh = build_bvh(&nodes).expect("build BVH");
        let visible = cull_visible(&bvh, &frustum);
        assert!(visible.contains(&0));
        assert!(!visible.contains(&1));
        assert!(visible.contains(&2));
    }
}
