#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

use crate::material::{MaterialData, MaterialUniforms};
use crate::mesh::{MeshData, Vertex3D};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn normalize(v: [f32; 3]) -> [f32; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len < 1e-10 {
        [0.0, 1.0, 0.0]
    } else {
        [v[0] / len, v[1] / len, v[2] / len]
    }
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// Simple deterministic hash-based noise for terrain height.
/// Returns a value in [-1, 1].
fn hash_noise(x: f32, z: f32) -> f32 {
    let ix = (x * 137.0 + z * 311.0).sin() * 43758.5453;
    ix.fract() * 2.0 - 1.0
}

/// Smooth noise by bilinear interpolation of hash_noise at integer grid points.
fn smooth_noise(x: f32, z: f32) -> f32 {
    let ix = x.floor();
    let iz = z.floor();
    let fx = x - ix;
    let fz = z - iz;
    // Smoothstep
    let sx = fx * fx * (3.0 - 2.0 * fx);
    let sz = fz * fz * (3.0 - 2.0 * fz);

    let n00 = hash_noise(ix, iz);
    let n10 = hash_noise(ix + 1.0, iz);
    let n01 = hash_noise(ix, iz + 1.0);
    let n11 = hash_noise(ix + 1.0, iz + 1.0);

    let nx0 = n00 * (1.0 - sx) + n10 * sx;
    let nx1 = n01 * (1.0 - sx) + n11 * sx;
    nx0 * (1.0 - sz) + nx1 * sz
}

/// Multi-octave noise for terrain.
/// Output is bounded to approximately [-0.5, 0.5] (sum of amplitudes = 0.5).
fn terrain_height(x: f32, z: f32) -> f32 {
    let mut h = 0.0_f32;
    h += smooth_noise(x * 0.5, z * 0.5) * 0.3;
    h += smooth_noise(x * 1.0, z * 1.0) * 0.15;
    h += smooth_noise(x * 2.0, z * 2.0) * 0.05;
    // Clamp to prevent extreme outliers from hash collisions at certain inputs
    h.clamp(-0.6, 0.6)
}

fn default_vertex() -> Vertex3D {
    Vertex3D {
        position: [0.0; 3],
        normal: [0.0, 1.0, 0.0],
        uv: [0.0; 2],
        joint_indices: [0; 4],
        joint_weights: [1.0, 0.0, 0.0, 0.0],
    }
}

fn make_material(base_color: [f32; 4], metallic: f32, roughness: f32) -> MaterialData {
    MaterialData {
        albedo_image: None,
        normal_image: None,
        orm_image: None,
        uniforms: MaterialUniforms {
            base_color_factor: base_color,
            metallic_factor: metallic,
            roughness_factor: roughness,
            _padding: [0.0; 2],
        },
    }
}

// ---------------------------------------------------------------------------
// Ground plane: 20x20 subdivided grid
// ---------------------------------------------------------------------------

/// Generate a ground plane mesh: 20x20 subdivided grid with gentle height noise.
/// UVs tile 4x across the surface. The plane spans [-10, 10] on X and Z.
/// Produces 800 triangles (20*20*2).
pub fn generate_ground_plane() -> Vec<MeshData> {
    let subdivs = 20_u32;
    let size = 20.0_f32; // total extent: -10 to +10
    let half = size / 2.0;
    let step = size / subdivs as f32;
    let uv_tiles = 4.0_f32;

    let vert_count = ((subdivs + 1) * (subdivs + 1)) as usize;
    let mut vertices = Vec::with_capacity(vert_count);

    // Generate vertex positions with height noise
    for iz in 0..=subdivs {
        for ix in 0..=subdivs {
            let x = -half + ix as f32 * step;
            let z = -half + iz as f32 * step;
            let y = terrain_height(x, z);
            let u = (ix as f32 / subdivs as f32) * uv_tiles;
            let v = (iz as f32 / subdivs as f32) * uv_tiles;
            // Clamp UVs to [0,1] by wrapping fractional part
            let u_clamped = u.fract();
            let v_clamped = v.fract();
            // For tiling UVs we keep the fractional part, but at boundaries (fract=0 for
            // non-zero input), use 1.0 so the last vertex in each tile gets UV=1.
            let u_final = if ix > 0 && u_clamped < 1e-6 { 1.0 } else { u_clamped };
            let v_final = if iz > 0 && v_clamped < 1e-6 { 1.0 } else { v_clamped };

            vertices.push(Vertex3D {
                position: [x, y, z],
                normal: [0.0, 1.0, 0.0], // placeholder, will compute below
                uv: [u_final, v_final],
                joint_indices: [0; 4],
                joint_weights: [1.0, 0.0, 0.0, 0.0],
            });
        }
    }

    // Generate indices (two triangles per quad, CCW winding when viewed from +Y)
    let tri_count = (subdivs * subdivs * 2) as usize;
    let mut indices = Vec::with_capacity(tri_count * 3);
    let row_stride = subdivs + 1;
    for iz in 0..subdivs {
        for ix in 0..subdivs {
            let bl = iz * row_stride + ix;
            let br = bl + 1;
            let tl = bl + row_stride;
            let tr = tl + 1;
            // Triangle 1: bl, tl, br (CCW from above)
            indices.push(bl);
            indices.push(tl);
            indices.push(br);
            // Triangle 2: br, tl, tr
            indices.push(br);
            indices.push(tl);
            indices.push(tr);
        }
    }

    // Compute normals from adjacent triangles
    let mut normal_accum = vec![[0.0_f32; 3]; vertices.len()];
    for tri in indices.chunks(3) {
        let i0 = tri[0] as usize;
        let i1 = tri[1] as usize;
        let i2 = tri[2] as usize;
        let p0 = vertices[i0].position;
        let p1 = vertices[i1].position;
        let p2 = vertices[i2].position;
        let edge1 = sub(p1, p0);
        let edge2 = sub(p2, p0);
        let n = cross(edge1, edge2);
        for &idx in &[i0, i1, i2] {
            normal_accum[idx][0] += n[0];
            normal_accum[idx][1] += n[1];
            normal_accum[idx][2] += n[2];
        }
    }
    for (i, v) in vertices.iter_mut().enumerate() {
        v.normal = normalize(normal_accum[i]);
    }

    vec![MeshData {
        vertices,
        indices,
        material: make_material([0.35, 0.55, 0.2, 1.0], 0.0, 0.85),
    }]
}

// ---------------------------------------------------------------------------
// Cylinder helper (used for trunk, arms, legs)
// ---------------------------------------------------------------------------

struct CylinderParams {
    radial_segments: u32,
    height_segments: u32,
    bottom_radius: f32,
    top_radius: f32,
    height: f32,
    offset: [f32; 3],
    noise_amplitude: f32,
}

fn generate_cylinder(params: &CylinderParams) -> (Vec<Vertex3D>, Vec<u32>) {
    let CylinderParams {
        radial_segments,
        height_segments,
        bottom_radius,
        top_radius,
        height,
        offset,
        noise_amplitude,
    } = *params;

    let rings = height_segments + 1;
    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    for ring in 0..rings {
        let t = ring as f32 / height_segments as f32;
        let y = t * height + offset[1];
        let radius = bottom_radius + (top_radius - bottom_radius) * t;
        let v_coord = t;

        for seg in 0..=radial_segments {
            let theta = (seg % radial_segments) as f32 / radial_segments as f32
                * std::f32::consts::TAU;
            let cos_t = theta.cos();
            let sin_t = theta.sin();

            let noise = if noise_amplitude > 0.0 {
                hash_noise(y * 3.7 + cos_t * 5.3, sin_t * 7.1) * noise_amplitude
            } else {
                0.0
            };

            let r = radius + noise;
            let x = cos_t * r + offset[0];
            let z = sin_t * r + offset[2];

            let u_coord = seg as f32 / radial_segments as f32;

            let normal = normalize([cos_t, 0.0, sin_t]);

            vertices.push(Vertex3D {
                position: [x, y, z],
                normal,
                uv: [u_coord, v_coord],
                joint_indices: [0; 4],
                joint_weights: [1.0, 0.0, 0.0, 0.0],
            });
        }
    }

    let verts_per_ring = radial_segments + 1;
    for ring in 0..height_segments {
        for seg in 0..radial_segments {
            let curr = ring * verts_per_ring + seg;
            let next = curr + 1;
            let curr_up = curr + verts_per_ring;
            let next_up = next + verts_per_ring;

            // CCW winding
            indices.push(curr);
            indices.push(curr_up);
            indices.push(next);

            indices.push(next);
            indices.push(curr_up);
            indices.push(next_up);
        }
    }

    (vertices, indices)
}

// ---------------------------------------------------------------------------
// Icosphere helper (used for foliage, rocks, head)
// ---------------------------------------------------------------------------

fn generate_icosphere(subdivisions: u32, radius: f32, offset: [f32; 3], noise_amp: f32) -> (Vec<Vertex3D>, Vec<u32>) {
    // Base icosahedron
    let phi = (1.0 + 5.0_f32.sqrt()) / 2.0;
    let base_verts: Vec<[f32; 3]> = vec![
        [-1.0,  phi, 0.0],
        [ 1.0,  phi, 0.0],
        [-1.0, -phi, 0.0],
        [ 1.0, -phi, 0.0],
        [0.0, -1.0,  phi],
        [0.0,  1.0,  phi],
        [0.0, -1.0, -phi],
        [0.0,  1.0, -phi],
        [ phi, 0.0, -1.0],
        [ phi, 0.0,  1.0],
        [-phi, 0.0, -1.0],
        [-phi, 0.0,  1.0],
    ];

    let base_tris: Vec<[u32; 3]> = vec![
        [0,11,5], [0,5,1], [0,1,7], [0,7,10], [0,10,11],
        [1,5,9], [5,11,4], [11,10,2], [10,7,6], [7,1,8],
        [3,9,4], [3,4,2], [3,2,6], [3,6,8], [3,8,9],
        [4,9,5], [2,4,11], [6,2,10], [8,6,7], [9,8,1],
    ];

    // Normalize base vertices to unit sphere
    let mut positions: Vec<[f32; 3]> = base_verts.iter().map(|v| normalize(*v)).collect();
    let mut triangles: Vec<[u32; 3]> = base_tris;

    // Subdivide
    use std::collections::HashMap;
    let mut midpoint_cache: HashMap<(u32, u32), u32> = HashMap::new();

    for _ in 0..subdivisions {
        let mut new_tris = Vec::new();
        midpoint_cache.clear();

        for tri in &triangles {
            let a = tri[0];
            let b = tri[1];
            let c = tri[2];

            let ab = get_midpoint(a, b, &mut positions, &mut midpoint_cache);
            let bc = get_midpoint(b, c, &mut positions, &mut midpoint_cache);
            let ca = get_midpoint(c, a, &mut positions, &mut midpoint_cache);

            new_tris.push([a, ab, ca]);
            new_tris.push([b, bc, ab]);
            new_tris.push([c, ca, bc]);
            new_tris.push([ab, bc, ca]);
        }

        triangles = new_tris;
    }

    // Build vertices with displacement
    let mut vertices = Vec::with_capacity(positions.len());
    for p in &positions {
        let noise = if noise_amp > 0.0 {
            hash_noise(p[0] * 10.0, p[1] * 10.0 + p[2] * 7.0) * noise_amp
        } else {
            0.0
        };
        let r = radius + noise;
        let pos = [
            p[0] * r + offset[0],
            p[1] * r + offset[1],
            p[2] * r + offset[2],
        ];
        // Spherical UV mapping
        let u = 0.5 + p[2].atan2(p[0]) / std::f32::consts::TAU;
        let v = 0.5 - p[1].asin() / std::f32::consts::PI;

        vertices.push(Vertex3D {
            position: pos,
            normal: normalize(*p), // will recompute below for displaced
            uv: [u.clamp(0.0, 1.0), v.clamp(0.0, 1.0)],
            joint_indices: [0; 4],
            joint_weights: [1.0, 0.0, 0.0, 0.0],
        });
    }

    let mut indices = Vec::with_capacity(triangles.len() * 3);
    for tri in &triangles {
        indices.push(tri[0]);
        indices.push(tri[1]);
        indices.push(tri[2]);
    }

    // Recompute normals from face contributions (smooth shading)
    if noise_amp > 0.0 {
        let mut normal_accum = vec![[0.0_f32; 3]; vertices.len()];
        for tri in indices.chunks(3) {
            let i0 = tri[0] as usize;
            let i1 = tri[1] as usize;
            let i2 = tri[2] as usize;
            let p0 = vertices[i0].position;
            let p1 = vertices[i1].position;
            let p2 = vertices[i2].position;
            let e1 = sub(p1, p0);
            let e2 = sub(p2, p0);
            let n = cross(e1, e2);
            for &idx in &[i0, i1, i2] {
                normal_accum[idx][0] += n[0];
                normal_accum[idx][1] += n[1];
                normal_accum[idx][2] += n[2];
            }
        }
        for (i, v) in vertices.iter_mut().enumerate() {
            v.normal = normalize(normal_accum[i]);
        }
    }

    (vertices, indices)
}

fn get_midpoint(
    a: u32,
    b: u32,
    positions: &mut Vec<[f32; 3]>,
    cache: &mut std::collections::HashMap<(u32, u32), u32>,
) -> u32 {
    let key = if a < b { (a, b) } else { (b, a) };
    if let Some(&idx) = cache.get(&key) {
        return idx;
    }
    let pa = positions[a as usize];
    let pb = positions[b as usize];
    let mid = normalize([
        (pa[0] + pb[0]) * 0.5,
        (pa[1] + pb[1]) * 0.5,
        (pa[2] + pb[2]) * 0.5,
    ]);
    let idx = positions.len() as u32;
    positions.push(mid);
    cache.insert(key, idx);
    idx
}

// ---------------------------------------------------------------------------
// Box helper (used for torso)
// ---------------------------------------------------------------------------

fn generate_box(half_extents: [f32; 3], offset: [f32; 3]) -> (Vec<Vertex3D>, Vec<u32>) {
    let [hx, hy, hz] = half_extents;
    let [ox, oy, oz] = offset;

    // 6 faces, 4 vertices each = 24 vertices, 36 indices
    let face_data: [([f32; 3], [[f32; 3]; 4], [[f32; 2]; 4]); 6] = [
        // +Y (top)
        ([0.0, 1.0, 0.0], [
            [-hx + ox, hy + oy, -hz + oz],
            [-hx + ox, hy + oy,  hz + oz],
            [ hx + ox, hy + oy,  hz + oz],
            [ hx + ox, hy + oy, -hz + oz],
        ], [[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]]),
        // -Y (bottom)
        ([0.0, -1.0, 0.0], [
            [-hx + ox, -hy + oy,  hz + oz],
            [-hx + ox, -hy + oy, -hz + oz],
            [ hx + ox, -hy + oy, -hz + oz],
            [ hx + ox, -hy + oy,  hz + oz],
        ], [[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]]),
        // +Z (front)
        ([0.0, 0.0, 1.0], [
            [-hx + ox, -hy + oy, hz + oz],
            [ hx + ox, -hy + oy, hz + oz],
            [ hx + ox,  hy + oy, hz + oz],
            [-hx + ox,  hy + oy, hz + oz],
        ], [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]),
        // -Z (back)
        ([0.0, 0.0, -1.0], [
            [ hx + ox, -hy + oy, -hz + oz],
            [-hx + ox, -hy + oy, -hz + oz],
            [-hx + ox,  hy + oy, -hz + oz],
            [ hx + ox,  hy + oy, -hz + oz],
        ], [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]),
        // +X (right)
        ([1.0, 0.0, 0.0], [
            [hx + ox, -hy + oy,  hz + oz],
            [hx + ox, -hy + oy, -hz + oz],
            [hx + ox,  hy + oy, -hz + oz],
            [hx + ox,  hy + oy,  hz + oz],
        ], [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]),
        // -X (left)
        ([-1.0, 0.0, 0.0], [
            [-hx + ox, -hy + oy, -hz + oz],
            [-hx + ox, -hy + oy,  hz + oz],
            [-hx + ox,  hy + oy,  hz + oz],
            [-hx + ox,  hy + oy, -hz + oz],
        ], [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]),
    ];

    let mut vertices = Vec::with_capacity(24);
    let mut indices = Vec::with_capacity(36);

    for (normal, positions, uvs) in &face_data {
        let base = vertices.len() as u32;
        for i in 0..4 {
            vertices.push(Vertex3D {
                position: positions[i],
                normal: *normal,
                uv: uvs[i],
                joint_indices: [0; 4],
                joint_weights: [1.0, 0.0, 0.0, 0.0],
            });
        }
        // Two triangles per face, CCW
        indices.push(base);
        indices.push(base + 1);
        indices.push(base + 2);
        indices.push(base);
        indices.push(base + 2);
        indices.push(base + 3);
    }

    (vertices, indices)
}

// ---------------------------------------------------------------------------
// Tree: trunk (tapered cylinder) + foliage (displaced icosphere)
// ---------------------------------------------------------------------------

/// Generate a tree consisting of a trunk (tapered cylinder) and foliage (displaced icosphere).
/// Returns [trunk_mesh, foliage_mesh].
/// Trunk: 12 radial x 6 height = ~144 tris.
/// Foliage: icosphere with 2 subdivisions, displaced = ~320 tris.
pub fn generate_tree() -> Vec<MeshData> {
    // Trunk
    let (trunk_verts, trunk_indices) = generate_cylinder(&CylinderParams {
        radial_segments: 12,
        height_segments: 6,
        bottom_radius: 0.15,
        top_radius: 0.08,
        height: 2.0,
        offset: [0.0, 0.0, 0.0],
        noise_amplitude: 0.02,
    });

    let trunk_mesh = MeshData {
        vertices: trunk_verts,
        indices: trunk_indices,
        material: make_material([0.4, 0.25, 0.1, 1.0], 0.0, 0.9),
    };

    // Foliage: displaced icosphere centered above trunk
    let (foliage_verts, foliage_indices) = generate_icosphere(
        2,     // subdivisions
        0.9,   // radius
        [0.0, 2.5, 0.0], // offset: above trunk top
        0.15,  // noise displacement
    );

    let foliage_mesh = MeshData {
        vertices: foliage_verts,
        indices: foliage_indices,
        material: make_material([0.2, 0.6, 0.15, 1.0], 0.0, 0.8),
    };

    vec![trunk_mesh, foliage_mesh]
}

// ---------------------------------------------------------------------------
// Rock: displaced icosphere
// ---------------------------------------------------------------------------

/// Generate a rock mesh from a displaced icosphere with 2 subdivisions.
/// ~320 tris.
pub fn generate_rock() -> Vec<MeshData> {
    let (verts, indices) = generate_icosphere(
        2,    // subdivisions
        0.5,  // radius
        [0.0, 0.25, 0.0], // slightly above ground
        0.1,  // noise
    );

    vec![MeshData {
        vertices: verts,
        indices: indices,
        material: make_material([0.5, 0.5, 0.45, 1.0], 0.0, 0.75),
    }]
}

// ---------------------------------------------------------------------------
// Character skinning
// ---------------------------------------------------------------------------

/// Joint positions for a 15-joint humanoid skeleton.
const JOINT_POSITIONS: [[f32; 3]; 15] = [
    [0.0, 0.9, 0.0],    // 0: root/hips
    [0.0, 1.1, 0.0],    // 1: spine
    [0.0, 1.3, 0.0],    // 2: chest
    [0.0, 1.5, 0.0],    // 3: neck
    [0.0, 1.65, 0.0],   // 4: head
    [-0.2, 1.35, 0.0],  // 5: L_shoulder
    [-0.45, 1.35, 0.0], // 6: L_elbow
    [-0.65, 1.35, 0.0], // 7: L_hand
    [0.2, 1.35, 0.0],   // 8: R_shoulder
    [0.45, 1.35, 0.0],  // 9: R_elbow
    [0.65, 1.35, 0.0],  // 10: R_hand
    [-0.1, 0.85, 0.0],  // 11: L_hip
    [-0.1, 0.45, 0.0],  // 12: L_knee
    [0.1, 0.85, 0.0],   // 13: R_hip
    [0.1, 0.45, 0.0],   // 14: R_knee
];

fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let dz = a[2] - b[2];
    (dx * dx + dy * dy + dz * dz).sqrt()
}

/// Assign skinning weights to a vertex based on its position.
/// Finds the 4 nearest joints and weights by inverse distance.
fn compute_skin_weights(pos: [f32; 3], joints: &[[f32; 3]; 15]) -> ([u16; 4], [f32; 4]) {
    // Compute distances to all joints
    let mut dists: Vec<(usize, f32)> = joints
        .iter()
        .enumerate()
        .map(|(i, jp)| (i, distance(pos, *jp)))
        .collect();

    // Sort by distance
    dists.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

    // Take top 4
    let count = dists.len().min(4);
    let mut joint_idx = [0_u16; 4];
    let mut weights = [0.0_f32; 4];

    // Use inverse distance weighting
    let mut total = 0.0_f32;
    for i in 0..count {
        let d = dists[i].1;
        let w = if d < 1e-6 { 1000.0 } else { 1.0 / (d * d) };
        joint_idx[i] = dists[i].0 as u16;
        weights[i] = w;
        total += w;
    }

    // Normalize weights to sum to 1.0
    if total > 0.0 {
        for i in 0..count {
            weights[i] /= total;
        }
    } else {
        weights[0] = 1.0;
    }

    (joint_idx, weights)
}

/// Apply skinning to all vertices in a mesh part.
fn apply_skinning(vertices: &mut [Vertex3D], joints: &[[f32; 3]; 15]) {
    for v in vertices.iter_mut() {
        let (ji, jw) = compute_skin_weights(v.position, joints);
        v.joint_indices = ji;
        v.joint_weights = jw;
    }
}

/// Merge multiple (vertices, indices) pairs into a single set, adjusting indices.
fn merge_mesh_parts(parts: &[(Vec<Vertex3D>, Vec<u32>)]) -> (Vec<Vertex3D>, Vec<u32>) {
    let total_verts: usize = parts.iter().map(|(v, _)| v.len()).sum();
    let total_indices: usize = parts.iter().map(|(_, i)| i.len()).sum();
    let mut vertices = Vec::with_capacity(total_verts);
    let mut indices = Vec::with_capacity(total_indices);

    for (verts, idxs) in parts {
        let base = vertices.len() as u32;
        vertices.extend_from_slice(verts);
        for &idx in idxs {
            indices.push(base + idx);
        }
    }

    (vertices, indices)
}

// ---------------------------------------------------------------------------
// Player character: humanoid from primitives
// ---------------------------------------------------------------------------

/// Generate a player character mesh from assembled primitives:
/// torso (box), head (sphere), arms (cylinders), legs (cylinders).
/// Includes joint_indices and joint_weights for skinning.
/// ~800-1200 tris.
pub fn generate_player_character() -> Vec<MeshData> {
    let mut parts: Vec<(Vec<Vertex3D>, Vec<u32>)> = Vec::new();

    // Torso: box from hips (y=0.9) to chest (y=1.35)
    let torso = generate_box(
        [0.15, 0.225, 0.1], // half extents
        [0.0, 1.125, 0.0],  // center between hips and chest
    );
    parts.push(torso);

    // Head: icosphere at y=1.65
    let head = generate_icosphere(1, 0.12, [0.0, 1.65, 0.0], 0.0);
    parts.push(head);

    // Neck: small cylinder from chest to head
    let neck = generate_cylinder(&CylinderParams {
        radial_segments: 8,
        height_segments: 2,
        bottom_radius: 0.05,
        top_radius: 0.05,
        height: 0.15,
        offset: [0.0, 1.35, 0.0],
        noise_amplitude: 0.0,
    });
    parts.push(neck);

    // Left upper arm: shoulder to elbow
    let l_upper_arm = generate_cylinder(&CylinderParams {
        radial_segments: 8,
        height_segments: 3,
        bottom_radius: 0.04,
        top_radius: 0.035,
        height: 0.25,
        offset: [-0.2, 1.35, 0.0],
        noise_amplitude: 0.0,
    });
    // Rotate arm horizontal: swap x/y coords
    let l_upper_arm = rotate_part_horizontal(l_upper_arm, [-0.2, 1.35, 0.0], true);
    parts.push(l_upper_arm);

    // Left forearm: elbow to hand
    let l_forearm = generate_cylinder(&CylinderParams {
        radial_segments: 8,
        height_segments: 3,
        bottom_radius: 0.035,
        top_radius: 0.03,
        height: 0.2,
        offset: [-0.45, 1.35, 0.0],
        noise_amplitude: 0.0,
    });
    let l_forearm = rotate_part_horizontal(l_forearm, [-0.45, 1.35, 0.0], true);
    parts.push(l_forearm);

    // Right upper arm
    let r_upper_arm = generate_cylinder(&CylinderParams {
        radial_segments: 8,
        height_segments: 3,
        bottom_radius: 0.04,
        top_radius: 0.035,
        height: 0.25,
        offset: [0.2, 1.35, 0.0],
        noise_amplitude: 0.0,
    });
    let r_upper_arm = rotate_part_horizontal(r_upper_arm, [0.2, 1.35, 0.0], false);
    parts.push(r_upper_arm);

    // Right forearm
    let r_forearm = generate_cylinder(&CylinderParams {
        radial_segments: 8,
        height_segments: 3,
        bottom_radius: 0.035,
        top_radius: 0.03,
        height: 0.2,
        offset: [0.45, 1.35, 0.0],
        noise_amplitude: 0.0,
    });
    let r_forearm = rotate_part_horizontal(r_forearm, [0.45, 1.35, 0.0], false);
    parts.push(r_forearm);

    // Left upper leg: hip to knee
    let l_upper_leg = generate_cylinder(&CylinderParams {
        radial_segments: 8,
        height_segments: 3,
        bottom_radius: 0.06,
        top_radius: 0.05,
        height: 0.4,
        offset: [-0.1, 0.45, 0.0],
        noise_amplitude: 0.0,
    });
    parts.push(l_upper_leg);

    // Left lower leg: knee to ground
    let l_lower_leg = generate_cylinder(&CylinderParams {
        radial_segments: 8,
        height_segments: 3,
        bottom_radius: 0.05,
        top_radius: 0.04,
        height: 0.45,
        offset: [-0.1, 0.0, 0.0],
        noise_amplitude: 0.0,
    });
    parts.push(l_lower_leg);

    // Right upper leg
    let r_upper_leg = generate_cylinder(&CylinderParams {
        radial_segments: 8,
        height_segments: 3,
        bottom_radius: 0.06,
        top_radius: 0.05,
        height: 0.4,
        offset: [0.1, 0.45, 0.0],
        noise_amplitude: 0.0,
    });
    parts.push(r_upper_leg);

    // Right lower leg
    let r_lower_leg = generate_cylinder(&CylinderParams {
        radial_segments: 8,
        height_segments: 3,
        bottom_radius: 0.05,
        top_radius: 0.04,
        height: 0.45,
        offset: [0.1, 0.0, 0.0],
        noise_amplitude: 0.0,
    });
    parts.push(r_lower_leg);

    // Merge all parts
    let (mut vertices, indices) = merge_mesh_parts(&parts);

    // Apply skinning
    apply_skinning(&mut vertices, &JOINT_POSITIONS);

    vec![MeshData {
        vertices,
        indices,
        material: make_material([0.7, 0.55, 0.4, 1.0], 0.0, 0.6),
    }]
}

/// Rotate a cylinder part to be horizontal (along X axis) instead of vertical (along Y axis).
/// `pivot` is the joint position at the base of the arm.
/// `left` determines direction: true = extends in -X, false = extends in +X.
fn rotate_part_horizontal(
    mut part: (Vec<Vertex3D>, Vec<u32>),
    pivot: [f32; 3],
    left: bool,
) -> (Vec<Vertex3D>, Vec<u32>) {
    let sign = if left { -1.0 } else { 1.0 };
    for v in part.0.iter_mut() {
        // Translate to origin relative to pivot
        let local_x = v.position[0] - pivot[0];
        let local_y = v.position[1] - pivot[1];
        let local_z = v.position[2] - pivot[2];

        // Rotate 90 degrees: Y -> X (or -X), keeping Z
        // The cylinder was generated going up in Y from pivot[1].
        // We want it to go along X from pivot[0].
        v.position[0] = pivot[0] + local_y * sign;
        v.position[1] = pivot[1] + local_x * sign;
        v.position[2] = pivot[2] + local_z;

        // Rotate normal similarly
        let nx = v.normal[0];
        let ny = v.normal[1];
        v.normal[0] = ny * sign;
        v.normal[1] = nx * sign;
        v.normal = normalize(v.normal);
    }
    part
}

// ---------------------------------------------------------------------------
// Enemy character: bulkier/hunched humanoid
// ---------------------------------------------------------------------------

/// Generate an enemy character mesh: similar to player but bulkier and hunched.
/// Includes joint_indices and joint_weights for skinning.
/// ~800-1200 tris.
pub fn generate_enemy_character() -> Vec<MeshData> {
    let mut parts: Vec<(Vec<Vertex3D>, Vec<u32>)> = Vec::new();

    // Bulkier torso
    let torso = generate_box(
        [0.2, 0.25, 0.15], // wider, deeper
        [0.0, 1.1, 0.05],  // slightly hunched forward
    );
    parts.push(torso);

    // Larger head, positioned slightly forward (hunched)
    let head = generate_icosphere(1, 0.15, [0.0, 1.6, 0.08], 0.02);
    parts.push(head);

    // Thicker neck
    let neck = generate_cylinder(&CylinderParams {
        radial_segments: 8,
        height_segments: 2,
        bottom_radius: 0.07,
        top_radius: 0.06,
        height: 0.15,
        offset: [0.0, 1.35, 0.04],
        noise_amplitude: 0.0,
    });
    parts.push(neck);

    // Bulkier arms
    let l_upper_arm = generate_cylinder(&CylinderParams {
        radial_segments: 8,
        height_segments: 3,
        bottom_radius: 0.06,
        top_radius: 0.05,
        height: 0.28,
        offset: [-0.22, 1.35, 0.0],
        noise_amplitude: 0.0,
    });
    let l_upper_arm = rotate_part_horizontal(l_upper_arm, [-0.22, 1.35, 0.0], true);
    parts.push(l_upper_arm);

    let l_forearm = generate_cylinder(&CylinderParams {
        radial_segments: 8,
        height_segments: 3,
        bottom_radius: 0.05,
        top_radius: 0.045,
        height: 0.22,
        offset: [-0.5, 1.35, 0.0],
        noise_amplitude: 0.0,
    });
    let l_forearm = rotate_part_horizontal(l_forearm, [-0.5, 1.35, 0.0], true);
    parts.push(l_forearm);

    let r_upper_arm = generate_cylinder(&CylinderParams {
        radial_segments: 8,
        height_segments: 3,
        bottom_radius: 0.06,
        top_radius: 0.05,
        height: 0.28,
        offset: [0.22, 1.35, 0.0],
        noise_amplitude: 0.0,
    });
    let r_upper_arm = rotate_part_horizontal(r_upper_arm, [0.22, 1.35, 0.0], false);
    parts.push(r_upper_arm);

    let r_forearm = generate_cylinder(&CylinderParams {
        radial_segments: 8,
        height_segments: 3,
        bottom_radius: 0.05,
        top_radius: 0.045,
        height: 0.22,
        offset: [0.5, 1.35, 0.0],
        noise_amplitude: 0.0,
    });
    let r_forearm = rotate_part_horizontal(r_forearm, [0.5, 1.35, 0.0], false);
    parts.push(r_forearm);

    // Thicker legs
    let l_upper_leg = generate_cylinder(&CylinderParams {
        radial_segments: 8,
        height_segments: 3,
        bottom_radius: 0.08,
        top_radius: 0.07,
        height: 0.4,
        offset: [-0.12, 0.45, 0.0],
        noise_amplitude: 0.0,
    });
    parts.push(l_upper_leg);

    let l_lower_leg = generate_cylinder(&CylinderParams {
        radial_segments: 8,
        height_segments: 3,
        bottom_radius: 0.065,
        top_radius: 0.05,
        height: 0.45,
        offset: [-0.12, 0.0, 0.0],
        noise_amplitude: 0.0,
    });
    parts.push(l_lower_leg);

    let r_upper_leg = generate_cylinder(&CylinderParams {
        radial_segments: 8,
        height_segments: 3,
        bottom_radius: 0.08,
        top_radius: 0.07,
        height: 0.4,
        offset: [0.12, 0.45, 0.0],
        noise_amplitude: 0.0,
    });
    parts.push(r_upper_leg);

    let r_lower_leg = generate_cylinder(&CylinderParams {
        radial_segments: 8,
        height_segments: 3,
        bottom_radius: 0.065,
        top_radius: 0.05,
        height: 0.45,
        offset: [0.12, 0.0, 0.0],
        noise_amplitude: 0.0,
    });
    parts.push(r_lower_leg);

    // Merge all parts
    let (mut vertices, indices) = merge_mesh_parts(&parts);

    // Apply skinning with same skeleton
    apply_skinning(&mut vertices, &JOINT_POSITIONS);

    vec![MeshData {
        vertices,
        indices,
        material: make_material([0.3, 0.15, 0.2, 1.0], 0.0, 0.7),
    }]
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const EPSILON: f32 = 1e-4;

    /// Verify all normals are unit-length.
    fn assert_normals_unit_length(vertices: &[Vertex3D], label: &str) {
        for (i, v) in vertices.iter().enumerate() {
            let len = (v.normal[0] * v.normal[0]
                + v.normal[1] * v.normal[1]
                + v.normal[2] * v.normal[2])
                .sqrt();
            assert!(
                (len - 1.0).abs() < EPSILON,
                "{label}: vertex {i} normal length = {len}, expected 1.0"
            );
        }
    }

    /// Verify all UVs are in [0, 1].
    fn assert_uvs_in_range(vertices: &[Vertex3D], label: &str) {
        for (i, v) in vertices.iter().enumerate() {
            assert!(
                v.uv[0] >= -EPSILON && v.uv[0] <= 1.0 + EPSILON,
                "{label}: vertex {i} u = {}, expected [0, 1]",
                v.uv[0]
            );
            assert!(
                v.uv[1] >= -EPSILON && v.uv[1] <= 1.0 + EPSILON,
                "{label}: vertex {i} v = {}, expected [0, 1]",
                v.uv[1]
            );
        }
    }

    /// Verify all indices reference valid vertices.
    fn assert_indices_valid(indices: &[u32], vertex_count: usize, label: &str) {
        for (i, &idx) in indices.iter().enumerate() {
            assert!(
                (idx as usize) < vertex_count,
                "{label}: index {i} = {idx}, but only {vertex_count} vertices"
            );
        }
    }

    /// Verify no degenerate triangles (zero area).
    fn assert_no_degenerate_triangles(vertices: &[Vertex3D], indices: &[u32], label: &str) {
        assert!(
            indices.len() % 3 == 0,
            "{label}: index count {} not a multiple of 3",
            indices.len()
        );
        for tri_idx in (0..indices.len()).step_by(3) {
            let i0 = indices[tri_idx] as usize;
            let i1 = indices[tri_idx + 1] as usize;
            let i2 = indices[tri_idx + 2] as usize;
            let p0 = vertices[i0].position;
            let p1 = vertices[i1].position;
            let p2 = vertices[i2].position;
            let e1 = sub(p1, p0);
            let e2 = sub(p2, p0);
            let c = cross(e1, e2);
            let area_sq = c[0] * c[0] + c[1] * c[1] + c[2] * c[2];
            assert!(
                area_sq > 1e-14,
                "{label}: degenerate triangle at index {tri_idx} (area^2 = {area_sq})"
            );
        }
    }

    /// Count triangles from index buffer.
    fn triangle_count(indices: &[u32]) -> usize {
        indices.len() / 3
    }

    /// Validate a complete MeshData.
    fn validate_mesh(mesh: &MeshData, label: &str) {
        assert!(
            !mesh.vertices.is_empty(),
            "{label}: vertices must not be empty"
        );
        assert!(
            !mesh.indices.is_empty(),
            "{label}: indices must not be empty"
        );
        assert_normals_unit_length(&mesh.vertices, label);
        assert_uvs_in_range(&mesh.vertices, label);
        assert_indices_valid(&mesh.indices, mesh.vertices.len(), label);
        assert_no_degenerate_triangles(&mesh.vertices, &mesh.indices, label);
    }

    /// Verify skinning weights sum to 1.0 and at least one weight is non-zero.
    fn assert_skin_weights_valid(vertices: &[Vertex3D], label: &str) {
        for (i, v) in vertices.iter().enumerate() {
            let sum: f32 = v.joint_weights.iter().sum();
            assert!(
                (sum - 1.0).abs() < 0.01,
                "{label}: vertex {i} joint weights sum = {sum}, expected 1.0"
            );
            let has_nonzero = v.joint_weights.iter().any(|&w| w > 0.0);
            assert!(
                has_nonzero,
                "{label}: vertex {i} has all-zero joint weights"
            );
            // Verify joint indices are within skeleton range
            for &ji in &v.joint_indices {
                assert!(
                    (ji as usize) < 15,
                    "{label}: vertex {i} references joint {ji}, but skeleton only has 15 joints"
                );
            }
        }
    }

    // -----------------------------------------------------------------------
    // Ground plane tests
    // -----------------------------------------------------------------------

    #[test]
    fn ground_plane_produces_mesh() {
        let meshes = generate_ground_plane();
        assert_eq!(meshes.len(), 1);
        validate_mesh(&meshes[0], "ground_plane");
    }

    #[test]
    fn ground_plane_triangle_count() {
        let meshes = generate_ground_plane();
        let tris = triangle_count(&meshes[0].indices);
        assert_eq!(tris, 800, "ground plane should have 800 triangles (20x20x2)");
    }

    #[test]
    fn ground_plane_has_height_variation() {
        let meshes = generate_ground_plane();
        let heights: Vec<f32> = meshes[0].vertices.iter().map(|v| v.position[1]).collect();
        let min_h = heights.iter().cloned().fold(f32::INFINITY, f32::min);
        let max_h = heights.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        assert!(
            (max_h - min_h) > 0.01,
            "ground plane should have height variation, got range [{min_h}, {max_h}]"
        );
    }

    // -----------------------------------------------------------------------
    // Tree tests
    // -----------------------------------------------------------------------

    #[test]
    fn tree_produces_two_meshes() {
        let meshes = generate_tree();
        assert_eq!(meshes.len(), 2, "tree should produce [trunk, foliage]");
        validate_mesh(&meshes[0], "tree_trunk");
        validate_mesh(&meshes[1], "tree_foliage");
    }

    #[test]
    fn tree_trunk_is_tapered() {
        let meshes = generate_tree();
        let trunk = &meshes[0];
        // Bottom ring vertices should be wider than top ring vertices
        let bottom_verts: Vec<f32> = trunk
            .vertices
            .iter()
            .filter(|v| v.position[1] < 0.1)
            .map(|v| (v.position[0] * v.position[0] + v.position[2] * v.position[2]).sqrt())
            .collect();
        let top_verts: Vec<f32> = trunk
            .vertices
            .iter()
            .filter(|v| v.position[1] > 1.9)
            .map(|v| (v.position[0] * v.position[0] + v.position[2] * v.position[2]).sqrt())
            .collect();

        if !bottom_verts.is_empty() && !top_verts.is_empty() {
            let avg_bottom: f32 = bottom_verts.iter().sum::<f32>() / bottom_verts.len() as f32;
            let avg_top: f32 = top_verts.iter().sum::<f32>() / top_verts.len() as f32;
            assert!(
                avg_bottom > avg_top,
                "trunk should be tapered: bottom avg radius {avg_bottom} > top avg radius {avg_top}"
            );
        }
    }

    #[test]
    fn tree_foliage_above_trunk() {
        let meshes = generate_tree();
        let foliage = &meshes[1];
        let avg_y: f32 =
            foliage.vertices.iter().map(|v| v.position[1]).sum::<f32>()
                / foliage.vertices.len() as f32;
        assert!(
            avg_y > 1.5,
            "foliage center should be above trunk, avg_y = {avg_y}"
        );
    }

    // -----------------------------------------------------------------------
    // Rock tests
    // -----------------------------------------------------------------------

    #[test]
    fn rock_produces_mesh() {
        let meshes = generate_rock();
        assert_eq!(meshes.len(), 1);
        validate_mesh(&meshes[0], "rock");
    }

    #[test]
    fn rock_triangle_budget() {
        let meshes = generate_rock();
        let tris = triangle_count(&meshes[0].indices);
        assert!(
            tris >= 160 && tris <= 400,
            "rock should have 160-400 triangles, got {tris}"
        );
    }

    // -----------------------------------------------------------------------
    // Player character tests
    // -----------------------------------------------------------------------

    #[test]
    fn player_produces_mesh() {
        let meshes = generate_player_character();
        assert_eq!(meshes.len(), 1);
        validate_mesh(&meshes[0], "player");
    }

    #[test]
    fn player_has_valid_skinning() {
        let meshes = generate_player_character();
        assert_skin_weights_valid(&meshes[0].vertices, "player");
    }

    #[test]
    fn player_triangle_budget() {
        let meshes = generate_player_character();
        let tris = triangle_count(&meshes[0].indices);
        assert!(
            tris >= 400 && tris <= 1500,
            "player should have 400-1500 triangles, got {tris}"
        );
    }

    #[test]
    fn player_is_humanoid_shaped() {
        let meshes = generate_player_character();
        let verts = &meshes[0].vertices;
        // Should span vertically from near 0 to about 1.8
        let min_y = verts.iter().map(|v| v.position[1]).fold(f32::INFINITY, f32::min);
        let max_y = verts
            .iter()
            .map(|v| v.position[1])
            .fold(f32::NEG_INFINITY, f32::max);
        assert!(min_y < 0.1, "player should reach near ground, min_y = {min_y}");
        assert!(max_y > 1.5, "player should be tall, max_y = {max_y}");

        // Should have some width (arms)
        let min_x = verts.iter().map(|v| v.position[0]).fold(f32::INFINITY, f32::min);
        let max_x = verts
            .iter()
            .map(|v| v.position[0])
            .fold(f32::NEG_INFINITY, f32::max);
        assert!(
            (max_x - min_x) > 0.3,
            "player should have arm span, width = {}",
            max_x - min_x
        );
    }

    // -----------------------------------------------------------------------
    // Enemy character tests
    // -----------------------------------------------------------------------

    #[test]
    fn enemy_produces_mesh() {
        let meshes = generate_enemy_character();
        assert_eq!(meshes.len(), 1);
        validate_mesh(&meshes[0], "enemy");
    }

    #[test]
    fn enemy_has_valid_skinning() {
        let meshes = generate_enemy_character();
        assert_skin_weights_valid(&meshes[0].vertices, "enemy");
    }

    #[test]
    fn enemy_triangle_budget() {
        let meshes = generate_enemy_character();
        let tris = triangle_count(&meshes[0].indices);
        assert!(
            tris >= 400 && tris <= 1500,
            "enemy should have 400-1500 triangles, got {tris}"
        );
    }

    #[test]
    fn enemy_is_bulkier_than_player() {
        let player = generate_player_character();
        let enemy = generate_enemy_character();

        // Enemy torso should be wider
        let player_width: f32 = {
            let torso_verts: Vec<&Vertex3D> = player[0]
                .vertices
                .iter()
                .filter(|v| v.position[1] > 0.9 && v.position[1] < 1.4)
                .collect();
            if torso_verts.is_empty() {
                0.0
            } else {
                let min_x = torso_verts.iter().map(|v| v.position[0]).fold(f32::INFINITY, f32::min);
                let max_x = torso_verts
                    .iter()
                    .map(|v| v.position[0])
                    .fold(f32::NEG_INFINITY, f32::max);
                max_x - min_x
            }
        };

        let enemy_width: f32 = {
            let torso_verts: Vec<&Vertex3D> = enemy[0]
                .vertices
                .iter()
                .filter(|v| v.position[1] > 0.9 && v.position[1] < 1.4)
                .collect();
            if torso_verts.is_empty() {
                0.0
            } else {
                let min_x = torso_verts.iter().map(|v| v.position[0]).fold(f32::INFINITY, f32::min);
                let max_x = torso_verts
                    .iter()
                    .map(|v| v.position[0])
                    .fold(f32::NEG_INFINITY, f32::max);
                max_x - min_x
            }
        };

        assert!(
            enemy_width > player_width,
            "enemy torso width ({enemy_width}) should be > player torso width ({player_width})"
        );
    }

    // -----------------------------------------------------------------------
    // Total triangle budget
    // -----------------------------------------------------------------------

    #[test]
    fn total_triangle_budget_under_10k() {
        let ground = generate_ground_plane();
        let tree = generate_tree();
        let rock = generate_rock();
        let player = generate_player_character();
        let enemy = generate_enemy_character();

        let total: usize = ground.iter().map(|m| triangle_count(&m.indices)).sum::<usize>()
            + tree.iter().map(|m| triangle_count(&m.indices)).sum::<usize>()
            + rock.iter().map(|m| triangle_count(&m.indices)).sum::<usize>()
            + player.iter().map(|m| triangle_count(&m.indices)).sum::<usize>()
            + enemy.iter().map(|m| triangle_count(&m.indices)).sum::<usize>();

        assert!(
            total < 10_000,
            "total triangles across all meshes = {total}, budget is < 10,000"
        );
    }

    // -----------------------------------------------------------------------
    // Helper unit tests
    // -----------------------------------------------------------------------

    #[test]
    fn normalize_unit_vector() {
        let n = normalize([3.0, 4.0, 0.0]);
        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        assert!((len - 1.0).abs() < EPSILON);
        assert!((n[0] - 0.6).abs() < EPSILON);
        assert!((n[1] - 0.8).abs() < EPSILON);
    }

    #[test]
    fn normalize_zero_vector_returns_up() {
        let n = normalize([0.0, 0.0, 0.0]);
        assert_eq!(n, [0.0, 1.0, 0.0]);
    }

    #[test]
    fn terrain_height_is_bounded() {
        // Test over the actual ground plane extent [-10, 10] and well beyond
        for ix in -100..100 {
            for iz in -100..100 {
                let x = ix as f32 * 0.5;
                let z = iz as f32 * 0.5;
                let h = terrain_height(x, z);
                assert!(
                    h.abs() <= 0.61,
                    "terrain height at ({x}, {z}) = {h}, should be bounded to [-0.6, 0.6]",
                );
            }
        }
    }

    #[test]
    fn compute_skin_weights_sum_to_one() {
        let test_positions = [
            [0.0, 0.9, 0.0],   // at root
            [0.0, 1.65, 0.0],  // at head
            [-0.65, 1.35, 0.0], // at left hand
            [0.0, 0.0, 0.0],   // at ground (between feet)
            [0.5, 1.0, 0.5],   // arbitrary position
        ];

        for pos in &test_positions {
            let (ji, jw) = compute_skin_weights(*pos, &JOINT_POSITIONS);
            let sum: f32 = jw.iter().sum();
            assert!(
                (sum - 1.0).abs() < 0.01,
                "weights at {:?} sum to {sum}, expected 1.0",
                pos
            );
            // At least one weight should be non-zero
            assert!(jw.iter().any(|&w| w > 0.0));
            // All joint indices should be valid
            for &j in &ji {
                assert!((j as usize) < 15, "invalid joint index {j}");
            }
        }
    }

    #[test]
    fn icosphere_subdivision_count() {
        // Icosahedron has 20 faces. Each subdivision multiplies by 4.
        // subdiv 0: 20 faces
        // subdiv 1: 80 faces
        // subdiv 2: 320 faces
        let (_, indices) = generate_icosphere(0, 1.0, [0.0; 3], 0.0);
        assert_eq!(triangle_count(&indices), 20);

        let (_, indices) = generate_icosphere(1, 1.0, [0.0; 3], 0.0);
        assert_eq!(triangle_count(&indices), 80);

        let (_, indices) = generate_icosphere(2, 1.0, [0.0; 3], 0.0);
        assert_eq!(triangle_count(&indices), 320);
    }

    #[test]
    fn cylinder_triangle_count() {
        let (_, indices) = generate_cylinder(&CylinderParams {
            radial_segments: 12,
            height_segments: 6,
            bottom_radius: 1.0,
            top_radius: 0.5,
            height: 2.0,
            offset: [0.0; 3],
            noise_amplitude: 0.0,
        });
        // 12 radial * 6 height * 2 tris per quad = 144
        assert_eq!(triangle_count(&indices), 144);
    }

    #[test]
    fn box_triangle_count() {
        let (_, indices) = generate_box([1.0, 1.0, 1.0], [0.0; 3]);
        // 6 faces * 2 tris = 12
        assert_eq!(triangle_count(&indices), 12);
    }

    #[test]
    fn box_normals_are_axis_aligned() {
        let (vertices, _) = generate_box([1.0, 1.0, 1.0], [0.0; 3]);
        for v in &vertices {
            // Each normal should have exactly one component with abs=1, others=0
            let abs_sum = v.normal[0].abs() + v.normal[1].abs() + v.normal[2].abs();
            assert!(
                (abs_sum - 1.0).abs() < EPSILON,
                "box normal {:?} is not axis-aligned",
                v.normal
            );
        }
    }

    #[test]
    fn merge_mesh_parts_preserves_geometry() {
        let part1 = (
            vec![
                Vertex3D { position: [0.0, 0.0, 0.0], ..default_vertex() },
                Vertex3D { position: [1.0, 0.0, 0.0], ..default_vertex() },
                Vertex3D { position: [0.0, 1.0, 0.0], ..default_vertex() },
            ],
            vec![0, 1, 2],
        );
        let part2 = (
            vec![
                Vertex3D { position: [2.0, 0.0, 0.0], ..default_vertex() },
                Vertex3D { position: [3.0, 0.0, 0.0], ..default_vertex() },
                Vertex3D { position: [2.0, 1.0, 0.0], ..default_vertex() },
            ],
            vec![0, 1, 2],
        );

        let (merged_verts, merged_indices) = merge_mesh_parts(&[part1, part2]);
        assert_eq!(merged_verts.len(), 6);
        assert_eq!(merged_indices.len(), 6);
        // Second part's indices should be offset by 3
        assert_eq!(merged_indices[3], 3);
        assert_eq!(merged_indices[4], 4);
        assert_eq!(merged_indices[5], 5);
    }

    #[test]
    fn ground_material_is_green() {
        let meshes = generate_ground_plane();
        let color = meshes[0].material.uniforms.base_color_factor;
        // Green channel should be dominant
        assert!(color[1] > color[0], "ground should be greenish");
        assert!(color[1] > color[2], "ground should be greenish");
    }

    #[test]
    fn tree_trunk_material_is_brown() {
        let meshes = generate_tree();
        let color = meshes[0].material.uniforms.base_color_factor;
        // Red channel should be higher than blue
        assert!(color[0] > color[2], "trunk should be brownish");
    }

    #[test]
    fn all_meshes_have_ccw_winding() {
        // For non-displaced meshes, verify that the face normals computed from
        // CCW winding generally agree with the vertex normals.
        let ground = generate_ground_plane();
        let mesh = &ground[0];
        let mut agree_count = 0;
        let mut total = 0;
        for tri in mesh.indices.chunks(3) {
            let i0 = tri[0] as usize;
            let i1 = tri[1] as usize;
            let i2 = tri[2] as usize;
            let p0 = mesh.vertices[i0].position;
            let p1 = mesh.vertices[i1].position;
            let p2 = mesh.vertices[i2].position;
            let e1 = sub(p1, p0);
            let e2 = sub(p2, p0);
            let face_normal = normalize(cross(e1, e2));
            let avg_vert_normal = normalize([
                (mesh.vertices[i0].normal[0]
                    + mesh.vertices[i1].normal[0]
                    + mesh.vertices[i2].normal[0])
                    / 3.0,
                (mesh.vertices[i0].normal[1]
                    + mesh.vertices[i1].normal[1]
                    + mesh.vertices[i2].normal[1])
                    / 3.0,
                (mesh.vertices[i0].normal[2]
                    + mesh.vertices[i1].normal[2]
                    + mesh.vertices[i2].normal[2])
                    / 3.0,
            ]);
            let dot = face_normal[0] * avg_vert_normal[0]
                + face_normal[1] * avg_vert_normal[1]
                + face_normal[2] * avg_vert_normal[2];
            if dot > 0.0 {
                agree_count += 1;
            }
            total += 1;
        }
        let agreement_ratio = agree_count as f32 / total as f32;
        assert!(
            agreement_ratio > 0.95,
            "CCW winding agreement ratio = {agreement_ratio}, expected > 0.95"
        );
    }
}
