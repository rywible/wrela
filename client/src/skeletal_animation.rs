#![cfg_attr(not(test), allow(dead_code))]

use crate::mesh::JointMatrix;

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct AnimationClip {
    pub name: String,
    pub duration_secs: f32,
    pub channels: Vec<AnimationChannel>,
}

#[derive(Debug, Clone)]
pub struct AnimationChannel {
    pub joint_index: usize,
    pub property: ChannelProperty,
    pub keyframes: Vec<Keyframe>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelProperty {
    Translation,
    Rotation,
    Scale,
}

#[derive(Debug, Clone, Copy)]
pub struct Keyframe {
    pub time: f32,
    pub value: KeyframeValue,
}

#[derive(Debug, Clone, Copy)]
pub enum KeyframeValue {
    Vec3([f32; 3]),
    Quat([f32; 4]),
}

#[derive(Debug, Clone)]
pub struct Skeleton {
    pub joints: Vec<Joint>,
    pub inverse_bind_matrices: Vec<JointMatrix>,
}

#[derive(Debug, Clone)]
pub struct Joint {
    pub name: String,
    pub parent_index: Option<usize>,
    pub local_transform: JointMatrix,
}

/// The sampled pose for a single joint at a given time.
#[derive(Debug, Clone, Copy)]
pub struct JointPose {
    pub translation: [f32; 3],
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
}

impl Default for JointPose {
    fn default() -> Self {
        Self {
            translation: [0.0, 0.0, 0.0],
            rotation: [0.0, 0.0, 0.0, 1.0], // identity quaternion
            scale: [1.0, 1.0, 1.0],
        }
    }
}

// ---------------------------------------------------------------------------
// GLB loading
// ---------------------------------------------------------------------------

/// Load skeleton and animation clips from a GLB binary blob.
///
/// Returns `(skeleton, clips)` where `skeleton` is `None` when the GLB has no
/// skin definition.
pub fn load_animations_from_glb(data: &[u8]) -> Result<(Option<Skeleton>, Vec<AnimationClip>), String> {
    let glb = gltf::Glb::from_slice(data).map_err(|e| format!("GLB parse error: {e}"))?;
    let doc = gltf::Gltf::from_slice(&glb.json).map_err(|e| format!("glTF JSON error: {e}"))?;
    let bin: &[u8] = glb.bin.as_deref().ok_or("No binary blob in GLB")?;

    let skeleton = extract_skeleton(&doc, bin);
    let clips = extract_animation_clips(&doc, bin, &skeleton);

    Ok((skeleton, clips))
}

/// Read `count` items of type `T` from a buffer view, interpreting raw bytes.
fn read_accessor_data<T: Copy>(
    accessor: &gltf::Accessor<'_>,
    bin: &[u8],
) -> Vec<T> {
    let view = match accessor.view() {
        Some(v) => v,
        None => return Vec::new(),
    };
    let stride = view.stride().unwrap_or(std::mem::size_of::<T>());
    let offset = view.offset() + accessor.offset();
    let count = accessor.count();
    let mut result = Vec::with_capacity(count);
    for i in 0..count {
        let start = offset + i * stride;
        if start + std::mem::size_of::<T>() <= bin.len() {
            let ptr = bin[start..].as_ptr() as *const T;
            // SAFETY: We verified bounds. The glTF spec guarantees LE encoding
            // which matches all our target platforms (wasm32, x86_64, aarch64).
            result.push(unsafe { std::ptr::read_unaligned(ptr) });
        }
    }
    result
}

fn extract_skeleton(doc: &gltf::Gltf, bin: &[u8]) -> Option<Skeleton> {
    let skin = doc.skins().next()?;

    // Build joint list. The glTF skin.joints() gives us node references in
    // joint-index order.
    let joint_nodes: Vec<gltf::Node<'_>> = skin.joints().collect();
    // Map from glTF node index -> joint index for parent lookups.
    let mut node_to_joint: std::collections::HashMap<usize, usize> =
        std::collections::HashMap::new();
    for (joint_idx, node) in joint_nodes.iter().enumerate() {
        node_to_joint.insert(node.index(), joint_idx);
    }

    let mut joints = Vec::with_capacity(joint_nodes.len());
    for node in &joint_nodes {
        let parent_index = find_parent_joint_index(doc, node.index(), &node_to_joint);
        let local_transform = node_local_transform(node);
        joints.push(Joint {
            name: node.name().unwrap_or("").to_string(),
            parent_index,
            local_transform,
        });
    }

    // Inverse bind matrices.
    let inverse_bind_matrices = if let Some(accessor) = skin.inverse_bind_matrices() {
        read_accessor_data::<JointMatrix>(&accessor, bin)
    } else {
        // Default to identity when not provided.
        vec![identity_matrix(); joints.len()]
    };

    Some(Skeleton {
        joints,
        inverse_bind_matrices,
    })
}

/// Walk the node tree to find a joint's parent joint index. Returns `None` if
/// the parent is not itself a joint (root joint case).
fn find_parent_joint_index(
    doc: &gltf::Gltf,
    node_index: usize,
    node_to_joint: &std::collections::HashMap<usize, usize>,
) -> Option<usize> {
    // glTF doesn't store parent pointers directly; we need to walk all nodes
    // and find who has this node as a child.
    for node in doc.nodes() {
        for child in node.children() {
            if child.index() == node_index {
                // Found the parent node; check if it's also a joint.
                return node_to_joint.get(&node.index()).copied();
            }
        }
    }
    None
}

fn node_local_transform(node: &gltf::Node<'_>) -> JointMatrix {
    let t = node.transform().matrix();
    [
        [t[0][0], t[0][1], t[0][2], t[0][3]],
        [t[1][0], t[1][1], t[1][2], t[1][3]],
        [t[2][0], t[2][1], t[2][2], t[2][3]],
        [t[3][0], t[3][1], t[3][2], t[3][3]],
    ]
}

fn extract_animation_clips(
    doc: &gltf::Gltf,
    bin: &[u8],
    skeleton: &Option<Skeleton>,
) -> Vec<AnimationClip> {
    // Build a node-index-to-joint-index map from the skeleton.
    let node_to_joint: std::collections::HashMap<usize, usize> = match skeleton {
        Some(_skel) => {
            // We need to re-derive the mapping. The joint order comes from
            // the skin, which lists joints in order. We walk the doc skins
            // to get node indices.
            if let Some(skin) = doc.skins().next() {
                skin.joints()
                    .enumerate()
                    .map(|(joint_idx, node)| (node.index(), joint_idx))
                    .collect()
            } else {
                std::collections::HashMap::new()
            }
        }
        None => {
            // No skeleton; use node index directly as joint index so that
            // clips are still loadable for non-skinned animations.
            doc.nodes()
                .map(|n| (n.index(), n.index()))
                .collect()
        }
    };

    let mut clips = Vec::new();
    for anim in doc.animations() {
        let mut channels = Vec::new();
        let mut max_time: f32 = 0.0;

        for channel in anim.channels() {
            let target = channel.target();
            let node_index = target.node().index();
            let joint_index = match node_to_joint.get(&node_index) {
                Some(&idx) => idx,
                None => continue, // channel targets a node not in the skeleton
            };

            let property = match target.property() {
                gltf::animation::Property::Translation => ChannelProperty::Translation,
                gltf::animation::Property::Rotation => ChannelProperty::Rotation,
                gltf::animation::Property::Scale => ChannelProperty::Scale,
                gltf::animation::Property::MorphTargetWeights => continue,
            };

            let sampler = channel.sampler();
            let times: Vec<f32> = read_accessor_data(&sampler.input(), bin);
            let keyframes = read_keyframes(&sampler.output(), bin, &times, property);

            if let Some(last) = times.last() {
                if *last > max_time {
                    max_time = *last;
                }
            }

            channels.push(AnimationChannel {
                joint_index,
                property,
                keyframes,
            });
        }

        clips.push(AnimationClip {
            name: anim.name().unwrap_or("").to_string(),
            duration_secs: max_time,
            channels,
        });
    }

    clips
}

fn read_keyframes(
    output: &gltf::Accessor<'_>,
    bin: &[u8],
    times: &[f32],
    property: ChannelProperty,
) -> Vec<Keyframe> {
    match property {
        ChannelProperty::Translation | ChannelProperty::Scale => {
            let values: Vec<[f32; 3]> = read_accessor_data(output, bin);
            times
                .iter()
                .zip(values.iter())
                .map(|(&time, &value)| Keyframe {
                    time,
                    value: KeyframeValue::Vec3(value),
                })
                .collect()
        }
        ChannelProperty::Rotation => {
            let values: Vec<[f32; 4]> = read_accessor_data(output, bin);
            times
                .iter()
                .zip(values.iter())
                .map(|(&time, &value)| Keyframe {
                    time,
                    value: KeyframeValue::Quat(value),
                })
                .collect()
        }
    }
}

// ---------------------------------------------------------------------------
// Clip evaluation
// ---------------------------------------------------------------------------

/// Sample an animation clip at the given time, producing a pose for each joint
/// referenced by the clip. The returned vector is indexed by joint index; joints
/// not covered by any channel get the identity pose.
pub fn evaluate_clip(clip: &AnimationClip, time: f32, joint_count: usize) -> Vec<JointPose> {
    let mut poses = vec![JointPose::default(); joint_count];

    for channel in &clip.channels {
        if channel.joint_index >= joint_count || channel.keyframes.is_empty() {
            continue;
        }
        let value = sample_channel(&channel.keyframes, time);
        let pose = &mut poses[channel.joint_index];
        match channel.property {
            ChannelProperty::Translation => {
                if let KeyframeValue::Vec3(v) = value {
                    pose.translation = v;
                }
            }
            ChannelProperty::Rotation => {
                if let KeyframeValue::Quat(q) = value {
                    pose.rotation = q;
                }
            }
            ChannelProperty::Scale => {
                if let KeyframeValue::Vec3(v) = value {
                    pose.scale = v;
                }
            }
        }
    }

    poses
}

fn sample_channel(keyframes: &[Keyframe], time: f32) -> KeyframeValue {
    debug_assert!(!keyframes.is_empty());

    // Before first keyframe
    if time <= keyframes[0].time {
        return keyframes[0].value;
    }
    // After last keyframe
    if time >= keyframes[keyframes.len() - 1].time {
        return keyframes[keyframes.len() - 1].value;
    }

    // Binary search for the bracket
    let idx = match keyframes.binary_search_by(|kf| kf.time.partial_cmp(&time).unwrap_or(std::cmp::Ordering::Equal)) {
        Ok(exact) => return keyframes[exact].value,
        Err(insert_pos) => insert_pos,
    };

    let a = &keyframes[idx - 1];
    let b = &keyframes[idx];
    let span = b.time - a.time;
    let t = if span > 0.0 {
        (time - a.time) / span
    } else {
        0.0
    };

    match (&a.value, &b.value) {
        (KeyframeValue::Vec3(va), KeyframeValue::Vec3(vb)) => {
            KeyframeValue::Vec3(lerp_vec3(va, vb, t))
        }
        (KeyframeValue::Quat(qa), KeyframeValue::Quat(qb)) => {
            KeyframeValue::Quat(slerp(qa, qb, t))
        }
        // Mismatched types shouldn't happen with well-formed data; clamp to a.
        _ => a.value,
    }
}

fn lerp_vec3(a: &[f32; 3], b: &[f32; 3], t: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

/// Spherical linear interpolation for unit quaternions.
fn slerp(a: &[f32; 4], b: &[f32; 4], t: f32) -> [f32; 4] {
    let mut dot = a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3];

    // If dot is negative, negate one quaternion to take the short path.
    let mut b_adj = *b;
    if dot < 0.0 {
        b_adj = [-b[0], -b[1], -b[2], -b[3]];
        dot = -dot;
    }

    // For nearly-identical orientations, fall back to nlerp to avoid division
    // by near-zero sin.
    if dot > 0.9995 {
        return normalize_quat(&[
            a[0] + (b_adj[0] - a[0]) * t,
            a[1] + (b_adj[1] - a[1]) * t,
            a[2] + (b_adj[2] - a[2]) * t,
            a[3] + (b_adj[3] - a[3]) * t,
        ]);
    }

    let theta = dot.clamp(-1.0, 1.0).acos();
    let sin_theta = theta.sin();
    let wa = ((1.0 - t) * theta).sin() / sin_theta;
    let wb = (t * theta).sin() / sin_theta;

    [
        a[0] * wa + b_adj[0] * wb,
        a[1] * wa + b_adj[1] * wb,
        a[2] * wa + b_adj[2] * wb,
        a[3] * wa + b_adj[3] * wb,
    ]
}

fn normalize_quat(q: &[f32; 4]) -> [f32; 4] {
    let len = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
    if len < 1e-10 {
        return [0.0, 0.0, 0.0, 1.0];
    }
    let inv = 1.0 / len;
    [q[0] * inv, q[1] * inv, q[2] * inv, q[3] * inv]
}

// ---------------------------------------------------------------------------
// Skinning matrix computation
// ---------------------------------------------------------------------------

/// Compute the final skinning matrices (joint palette) from a skeleton and a
/// sampled pose. The returned matrices are in model-space, pre-multiplied by
/// the inverse bind matrices, ready for GPU upload.
pub fn compute_skinning_matrices(
    skeleton: &Skeleton,
    pose: &[JointPose],
) -> Vec<JointMatrix> {
    let joint_count = skeleton.joints.len();
    let mut world_transforms = vec![identity_matrix(); joint_count];

    // Forward pass: compute world-space transform for each joint by walking
    // the hierarchy from root to leaves. Parents always have a lower index
    // in well-formed glTF skeletons.
    for i in 0..joint_count {
        let local = if i < pose.len() {
            compose_trs(&pose[i])
        } else {
            skeleton.joints[i].local_transform
        };

        world_transforms[i] = match skeleton.joints[i].parent_index {
            Some(parent) => mat4_mul(&world_transforms[parent], &local),
            None => local,
        };
    }

    // Multiply each world transform by the inverse bind matrix.
    let mut result = Vec::with_capacity(joint_count);
    for i in 0..joint_count {
        let ibm = if i < skeleton.inverse_bind_matrices.len() {
            &skeleton.inverse_bind_matrices[i]
        } else {
            &IDENTITY
        };
        result.push(mat4_mul(&world_transforms[i], ibm));
    }

    result
}

/// Compose a TRS (translation, rotation, scale) into a 4x4 column-major matrix.
fn compose_trs(pose: &JointPose) -> JointMatrix {
    let [tx, ty, tz] = pose.translation;
    let [qx, qy, qz, qw] = pose.rotation;
    let [sx, sy, sz] = pose.scale;

    // Rotation matrix from quaternion, scaled by S.
    let x2 = qx + qx;
    let y2 = qy + qy;
    let z2 = qz + qz;
    let xx = qx * x2;
    let xy = qx * y2;
    let xz = qx * z2;
    let yy = qy * y2;
    let yz = qy * z2;
    let zz = qz * z2;
    let wx = qw * x2;
    let wy = qw * y2;
    let wz = qw * z2;

    [
        [(1.0 - (yy + zz)) * sx, (xy + wz) * sx,         (xz - wy) * sx,         0.0],
        [(xy - wz) * sy,         (1.0 - (xx + zz)) * sy,  (yz + wx) * sy,         0.0],
        [(xz + wy) * sz,         (yz - wx) * sz,          (1.0 - (xx + yy)) * sz, 0.0],
        [tx,                     ty,                       tz,                     1.0],
    ]
}

const IDENTITY: JointMatrix = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];

fn identity_matrix() -> JointMatrix {
    IDENTITY
}

fn mat4_mul(a: &JointMatrix, b: &JointMatrix) -> JointMatrix {
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ---- helpers ----------------------------------------------------------

    fn approx_eq_f32(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() < eps
    }

    fn approx_eq_vec3(a: &[f32; 3], b: &[f32; 3], eps: f32) -> bool {
        approx_eq_f32(a[0], b[0], eps)
            && approx_eq_f32(a[1], b[1], eps)
            && approx_eq_f32(a[2], b[2], eps)
    }

    fn approx_eq_quat(a: &[f32; 4], b: &[f32; 4], eps: f32) -> bool {
        // Quaternions q and -q represent the same rotation.
        let same = approx_eq_f32(a[0], b[0], eps)
            && approx_eq_f32(a[1], b[1], eps)
            && approx_eq_f32(a[2], b[2], eps)
            && approx_eq_f32(a[3], b[3], eps);
        let neg = approx_eq_f32(a[0], -b[0], eps)
            && approx_eq_f32(a[1], -b[1], eps)
            && approx_eq_f32(a[2], -b[2], eps)
            && approx_eq_f32(a[3], -b[3], eps);
        same || neg
    }

    fn approx_eq_mat4(a: &JointMatrix, b: &JointMatrix, eps: f32) -> bool {
        for r in 0..4 {
            for c in 0..4 {
                if !approx_eq_f32(a[r][c], b[r][c], eps) {
                    return false;
                }
            }
        }
        true
    }

    // ---- unit tests -------------------------------------------------------

    #[test]
    fn joint_pose_default_is_identity() {
        let p = JointPose::default();
        assert_eq!(p.translation, [0.0, 0.0, 0.0]);
        assert_eq!(p.rotation, [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(p.scale, [1.0, 1.0, 1.0]);
    }

    #[test]
    fn compose_trs_identity() {
        let p = JointPose::default();
        let m = compose_trs(&p);
        assert!(approx_eq_mat4(&m, &IDENTITY, 1e-6));
    }

    #[test]
    fn compose_trs_translation_only() {
        let p = JointPose {
            translation: [1.0, 2.0, 3.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
        };
        let m = compose_trs(&p);
        assert!(approx_eq_f32(m[3][0], 1.0, 1e-6));
        assert!(approx_eq_f32(m[3][1], 2.0, 1e-6));
        assert!(approx_eq_f32(m[3][2], 3.0, 1e-6));
        // Upper 3x3 should be identity.
        for r in 0..3 {
            for c in 0..3 {
                let expected = if r == c { 1.0 } else { 0.0 };
                assert!(approx_eq_f32(m[r][c], expected, 1e-6));
            }
        }
    }

    #[test]
    fn compose_trs_scale_only() {
        let p = JointPose {
            translation: [0.0, 0.0, 0.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [2.0, 3.0, 4.0],
        };
        let m = compose_trs(&p);
        assert!(approx_eq_f32(m[0][0], 2.0, 1e-6));
        assert!(approx_eq_f32(m[1][1], 3.0, 1e-6));
        assert!(approx_eq_f32(m[2][2], 4.0, 1e-6));
    }

    #[test]
    fn mat4_mul_identity() {
        let a = identity_matrix();
        let b = [
            [1.0, 2.0, 3.0, 4.0],
            [5.0, 6.0, 7.0, 8.0],
            [9.0, 10.0, 11.0, 12.0],
            [13.0, 14.0, 15.0, 16.0],
        ];
        let result = mat4_mul(&a, &b);
        assert!(approx_eq_mat4(&result, &b, 1e-6));

        let result2 = mat4_mul(&b, &a);
        assert!(approx_eq_mat4(&result2, &b, 1e-6));
    }

    #[test]
    fn lerp_vec3_endpoints() {
        let a = [1.0, 2.0, 3.0];
        let b = [4.0, 5.0, 6.0];
        assert!(approx_eq_vec3(&lerp_vec3(&a, &b, 0.0), &a, 1e-6));
        assert!(approx_eq_vec3(&lerp_vec3(&a, &b, 1.0), &b, 1e-6));
    }

    #[test]
    fn lerp_vec3_midpoint() {
        let a = [0.0, 0.0, 0.0];
        let b = [2.0, 4.0, 6.0];
        let mid = lerp_vec3(&a, &b, 0.5);
        assert!(approx_eq_vec3(&mid, &[1.0, 2.0, 3.0], 1e-6));
    }

    #[test]
    fn slerp_identity_quaternions() {
        let id = [0.0, 0.0, 0.0, 1.0];
        let result = slerp(&id, &id, 0.5);
        assert!(approx_eq_quat(&result, &id, 1e-5));
    }

    #[test]
    fn slerp_endpoints() {
        let a = [0.0, 0.0, 0.0, 1.0];
        let b = normalize_quat(&[0.0, 0.707107, 0.0, 0.707107]);
        let start = slerp(&a, &b, 0.0);
        let end = slerp(&a, &b, 1.0);
        assert!(approx_eq_quat(&start, &a, 1e-5));
        assert!(approx_eq_quat(&end, &b, 1e-5));
    }

    #[test]
    fn slerp_opposite_quaternions_short_path() {
        // q and -q represent the same rotation; slerp should take the short path.
        let a = [0.0, 0.0, 0.0, 1.0];
        let b = [0.0, 0.0, 0.0, -1.0]; // same rotation, negated
        let mid = slerp(&a, &b, 0.5);
        assert!(approx_eq_quat(&mid, &a, 1e-4));
    }

    #[test]
    fn normalize_quat_unit() {
        let q = normalize_quat(&[3.0, 0.0, 0.0, 4.0]);
        let len = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
        assert!(approx_eq_f32(len, 1.0, 1e-6));
        assert!(approx_eq_f32(q[0], 0.6, 1e-5));
        assert!(approx_eq_f32(q[3], 0.8, 1e-5));
    }

    #[test]
    fn normalize_quat_zero_returns_identity() {
        let q = normalize_quat(&[0.0, 0.0, 0.0, 0.0]);
        assert_eq!(q, [0.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn sample_channel_single_keyframe() {
        let kfs = vec![Keyframe {
            time: 0.5,
            value: KeyframeValue::Vec3([1.0, 2.0, 3.0]),
        }];
        match sample_channel(&kfs, 0.0) {
            KeyframeValue::Vec3(v) => assert_eq!(v, [1.0, 2.0, 3.0]),
            _ => panic!("expected Vec3"),
        }
        match sample_channel(&kfs, 1.0) {
            KeyframeValue::Vec3(v) => assert_eq!(v, [1.0, 2.0, 3.0]),
            _ => panic!("expected Vec3"),
        }
    }

    #[test]
    fn sample_channel_interpolation() {
        let kfs = vec![
            Keyframe {
                time: 0.0,
                value: KeyframeValue::Vec3([0.0, 0.0, 0.0]),
            },
            Keyframe {
                time: 1.0,
                value: KeyframeValue::Vec3([2.0, 4.0, 6.0]),
            },
        ];
        match sample_channel(&kfs, 0.5) {
            KeyframeValue::Vec3(v) => {
                assert!(approx_eq_vec3(&v, &[1.0, 2.0, 3.0], 1e-6));
            }
            _ => panic!("expected Vec3"),
        }
    }

    #[test]
    fn sample_channel_clamps_before_first() {
        let kfs = vec![
            Keyframe {
                time: 1.0,
                value: KeyframeValue::Vec3([10.0, 20.0, 30.0]),
            },
            Keyframe {
                time: 2.0,
                value: KeyframeValue::Vec3([40.0, 50.0, 60.0]),
            },
        ];
        match sample_channel(&kfs, 0.0) {
            KeyframeValue::Vec3(v) => assert_eq!(v, [10.0, 20.0, 30.0]),
            _ => panic!("expected Vec3"),
        }
    }

    #[test]
    fn sample_channel_clamps_after_last() {
        let kfs = vec![
            Keyframe {
                time: 0.0,
                value: KeyframeValue::Vec3([0.0, 0.0, 0.0]),
            },
            Keyframe {
                time: 1.0,
                value: KeyframeValue::Vec3([5.0, 5.0, 5.0]),
            },
        ];
        match sample_channel(&kfs, 99.0) {
            KeyframeValue::Vec3(v) => assert_eq!(v, [5.0, 5.0, 5.0]),
            _ => panic!("expected Vec3"),
        }
    }

    #[test]
    fn evaluate_clip_basic() {
        let clip = AnimationClip {
            name: "test".to_string(),
            duration_secs: 1.0,
            channels: vec![
                AnimationChannel {
                    joint_index: 0,
                    property: ChannelProperty::Translation,
                    keyframes: vec![
                        Keyframe {
                            time: 0.0,
                            value: KeyframeValue::Vec3([0.0, 0.0, 0.0]),
                        },
                        Keyframe {
                            time: 1.0,
                            value: KeyframeValue::Vec3([10.0, 0.0, 0.0]),
                        },
                    ],
                },
                AnimationChannel {
                    joint_index: 1,
                    property: ChannelProperty::Scale,
                    keyframes: vec![
                        Keyframe {
                            time: 0.0,
                            value: KeyframeValue::Vec3([1.0, 1.0, 1.0]),
                        },
                        Keyframe {
                            time: 1.0,
                            value: KeyframeValue::Vec3([2.0, 2.0, 2.0]),
                        },
                    ],
                },
            ],
        };

        let poses = evaluate_clip(&clip, 0.5, 2);
        assert_eq!(poses.len(), 2);
        assert!(approx_eq_vec3(&poses[0].translation, &[5.0, 0.0, 0.0], 1e-5));
        assert!(approx_eq_vec3(&poses[1].scale, &[1.5, 1.5, 1.5], 1e-5));
    }

    #[test]
    fn evaluate_clip_ignores_out_of_range_joints() {
        let clip = AnimationClip {
            name: "test".to_string(),
            duration_secs: 1.0,
            channels: vec![AnimationChannel {
                joint_index: 99, // beyond joint_count
                property: ChannelProperty::Translation,
                keyframes: vec![Keyframe {
                    time: 0.0,
                    value: KeyframeValue::Vec3([1.0, 2.0, 3.0]),
                }],
            }],
        };
        let poses = evaluate_clip(&clip, 0.0, 2);
        assert_eq!(poses.len(), 2);
        // Both should be default since the channel was out of range.
        assert_eq!(poses[0].translation, [0.0, 0.0, 0.0]);
        assert_eq!(poses[1].translation, [0.0, 0.0, 0.0]);
    }

    #[test]
    fn compute_skinning_matrices_single_root_joint() {
        let skeleton = Skeleton {
            joints: vec![Joint {
                name: "root".to_string(),
                parent_index: None,
                local_transform: IDENTITY,
            }],
            inverse_bind_matrices: vec![IDENTITY],
        };
        let pose = vec![JointPose {
            translation: [1.0, 0.0, 0.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
        }];
        let matrices = compute_skinning_matrices(&skeleton, &pose);
        assert_eq!(matrices.len(), 1);
        // world = TRS(1,0,0), ibm = identity => result should have translation (1,0,0)
        assert!(approx_eq_f32(matrices[0][3][0], 1.0, 1e-6));
        assert!(approx_eq_f32(matrices[0][3][1], 0.0, 1e-6));
        assert!(approx_eq_f32(matrices[0][3][2], 0.0, 1e-6));
    }

    #[test]
    fn compute_skinning_matrices_parent_child_chain() {
        let skeleton = Skeleton {
            joints: vec![
                Joint {
                    name: "root".to_string(),
                    parent_index: None,
                    local_transform: IDENTITY,
                },
                Joint {
                    name: "child".to_string(),
                    parent_index: Some(0),
                    local_transform: IDENTITY,
                },
            ],
            inverse_bind_matrices: vec![IDENTITY, IDENTITY],
        };
        let pose = vec![
            JointPose {
                translation: [1.0, 0.0, 0.0],
                rotation: [0.0, 0.0, 0.0, 1.0],
                scale: [1.0, 1.0, 1.0],
            },
            JointPose {
                translation: [0.0, 2.0, 0.0],
                rotation: [0.0, 0.0, 0.0, 1.0],
                scale: [1.0, 1.0, 1.0],
            },
        ];
        let matrices = compute_skinning_matrices(&skeleton, &pose);
        assert_eq!(matrices.len(), 2);
        // Root: translate (1,0,0)
        assert!(approx_eq_f32(matrices[0][3][0], 1.0, 1e-6));
        assert!(approx_eq_f32(matrices[0][3][1], 0.0, 1e-6));
        // Child: world = root_world * child_local = T(1,0,0) * T(0,2,0) = T(1,2,0)
        assert!(approx_eq_f32(matrices[1][3][0], 1.0, 1e-6));
        assert!(approx_eq_f32(matrices[1][3][1], 2.0, 1e-6));
        assert!(approx_eq_f32(matrices[1][3][2], 0.0, 1e-6));
    }

    #[test]
    fn compute_skinning_matrices_with_inverse_bind() {
        // Set up so that inverse bind undoes the bind-pose offset.
        // Bind pose: joint at T(5,0,0). IBM = inverse of T(5,0,0) = T(-5,0,0).
        let ibm: JointMatrix = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [-5.0, 0.0, 0.0, 1.0],
        ];
        let skeleton = Skeleton {
            joints: vec![Joint {
                name: "bone".to_string(),
                parent_index: None,
                local_transform: IDENTITY,
            }],
            inverse_bind_matrices: vec![ibm],
        };
        // Animate joint to T(5,0,0) (same as bind pose) => skinning matrix = identity.
        let pose = vec![JointPose {
            translation: [5.0, 0.0, 0.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
        }];
        let matrices = compute_skinning_matrices(&skeleton, &pose);
        assert!(approx_eq_mat4(&matrices[0], &IDENTITY, 1e-5));
    }

    #[test]
    fn load_animations_from_glb_no_animations() {
        // Build a minimal GLB with no animations and no skin.
        let glb = build_minimal_glb_no_anim();
        let (skeleton, clips) =
            load_animations_from_glb(&glb).expect("should parse minimal GLB without animations");
        assert!(skeleton.is_none());
        assert!(clips.is_empty());
    }

    #[test]
    fn load_animations_from_glb_rejects_empty() {
        let result = load_animations_from_glb(&[]);
        assert!(result.is_err());
    }

    // ---- GLB builder helpers -----------------------------------------------

    fn build_minimal_glb_no_anim() -> Vec<u8> {
        let positions: [[f32; 3]; 3] = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];

        let mut bin_data = Vec::new();
        for pos in &positions {
            for f in pos {
                bin_data.extend_from_slice(&f.to_le_bytes());
            }
        }
        while bin_data.len() % 4 != 0 {
            bin_data.push(0);
        }

        let json_str = serde_json::json!({
            "asset": { "version": "2.0" },
            "buffers": [{ "byteLength": bin_data.len() }],
            "bufferViews": [
                { "buffer": 0, "byteOffset": 0, "byteLength": 36, "target": 34962 }
            ],
            "accessors": [
                {
                    "bufferView": 0,
                    "componentType": 5126,
                    "count": 3,
                    "type": "VEC3",
                    "max": [1.0, 1.0, 0.0],
                    "min": [0.0, 0.0, 0.0]
                }
            ],
            "meshes": [{
                "primitives": [{
                    "attributes": { "POSITION": 0 }
                }]
            }]
        })
        .to_string();

        encode_glb(&json_str, &bin_data)
    }

    fn encode_glb(json_str: &str, bin_data: &[u8]) -> Vec<u8> {
        let mut json_bytes = json_str.as_bytes().to_vec();
        while json_bytes.len() % 4 != 0 {
            json_bytes.push(b' ');
        }

        let mut padded_bin = bin_data.to_vec();
        while padded_bin.len() % 4 != 0 {
            padded_bin.push(0);
        }

        let total_length = 12 + 8 + json_bytes.len() + 8 + padded_bin.len();
        let mut glb = Vec::with_capacity(total_length);
        glb.extend_from_slice(b"glTF");
        glb.extend_from_slice(&2u32.to_le_bytes());
        glb.extend_from_slice(&(total_length as u32).to_le_bytes());

        glb.extend_from_slice(&(json_bytes.len() as u32).to_le_bytes());
        glb.extend_from_slice(&0x4E4F534Au32.to_le_bytes());
        glb.extend_from_slice(&json_bytes);

        glb.extend_from_slice(&(padded_bin.len() as u32).to_le_bytes());
        glb.extend_from_slice(&0x004E4942u32.to_le_bytes());
        glb.extend_from_slice(&padded_bin);

        glb
    }

    #[test]
    fn load_animations_from_glb_with_skeleton_and_clip() {
        // Build a GLB with:
        //   - 2 nodes (joint 0 = root, joint 1 = child of root)
        //   - 1 skin referencing both joints
        //   - 1 animation with a translation channel on joint 0
        //
        // Binary layout:
        //   [0..36]   positions (3 x vec3<f32>)
        //   [36..164] inverse bind matrices (2 x mat4<f32> = 128 bytes)
        //   [164..172] animation times (2 x f32 = 8 bytes)
        //   [172..196] animation translations (2 x vec3<f32> = 24 bytes)

        let positions: [[f32; 3]; 3] = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let ibm_0: [[f32; 4]; 4] = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        let ibm_1 = ibm_0;
        let anim_times: [f32; 2] = [0.0, 1.0];
        let anim_translations: [[f32; 3]; 2] = [[0.0, 0.0, 0.0], [10.0, 0.0, 0.0]];

        let mut bin_data = Vec::new();
        // positions [0..36]
        for pos in &positions {
            for f in pos {
                bin_data.extend_from_slice(&f.to_le_bytes());
            }
        }
        // ibm [36..164]
        for mat in &[ibm_0, ibm_1] {
            for row in mat {
                for f in row {
                    bin_data.extend_from_slice(&f.to_le_bytes());
                }
            }
        }
        // animation times [164..172]
        for t in &anim_times {
            bin_data.extend_from_slice(&t.to_le_bytes());
        }
        // animation translations [172..196]
        for v in &anim_translations {
            for f in v {
                bin_data.extend_from_slice(&f.to_le_bytes());
            }
        }

        while bin_data.len() % 4 != 0 {
            bin_data.push(0);
        }

        let json_str = serde_json::json!({
            "asset": { "version": "2.0" },
            "buffers": [{ "byteLength": bin_data.len() }],
            "bufferViews": [
                { "buffer": 0, "byteOffset": 0, "byteLength": 36 },
                { "buffer": 0, "byteOffset": 36, "byteLength": 128 },
                { "buffer": 0, "byteOffset": 164, "byteLength": 8 },
                { "buffer": 0, "byteOffset": 172, "byteLength": 24 }
            ],
            "accessors": [
                {
                    "bufferView": 0,
                    "componentType": 5126,
                    "count": 3,
                    "type": "VEC3",
                    "max": [1.0, 1.0, 0.0],
                    "min": [0.0, 0.0, 0.0]
                },
                {
                    "bufferView": 1,
                    "componentType": 5126,
                    "count": 2,
                    "type": "MAT4"
                },
                {
                    "bufferView": 2,
                    "componentType": 5126,
                    "count": 2,
                    "type": "SCALAR",
                    "min": [0.0],
                    "max": [1.0]
                },
                {
                    "bufferView": 3,
                    "componentType": 5126,
                    "count": 2,
                    "type": "VEC3"
                }
            ],
            "nodes": [
                { "name": "root", "children": [1], "translation": [0.0, 0.0, 0.0] },
                { "name": "child", "translation": [0.0, 1.0, 0.0] }
            ],
            "skins": [{
                "joints": [0, 1],
                "inverseBindMatrices": 1
            }],
            "meshes": [{
                "primitives": [{
                    "attributes": { "POSITION": 0 }
                }]
            }],
            "animations": [{
                "name": "walk",
                "channels": [{
                    "sampler": 0,
                    "target": { "node": 0, "path": "translation" }
                }],
                "samplers": [{
                    "input": 2,
                    "output": 3,
                    "interpolation": "LINEAR"
                }]
            }]
        })
        .to_string();

        let glb = encode_glb(&json_str, &bin_data);
        let (skeleton, clips) =
            load_animations_from_glb(&glb).expect("should parse GLB with skeleton and animation");

        // Skeleton assertions
        let skel = skeleton.expect("should have skeleton");
        assert_eq!(skel.joints.len(), 2);
        assert_eq!(skel.joints[0].name, "root");
        assert!(skel.joints[0].parent_index.is_none());
        assert_eq!(skel.joints[1].name, "child");
        assert_eq!(skel.joints[1].parent_index, Some(0));
        assert_eq!(skel.inverse_bind_matrices.len(), 2);

        // Animation assertions
        assert_eq!(clips.len(), 1);
        assert_eq!(clips[0].name, "walk");
        assert!(approx_eq_f32(clips[0].duration_secs, 1.0, 1e-6));
        assert_eq!(clips[0].channels.len(), 1);
        assert_eq!(clips[0].channels[0].joint_index, 0);
        assert_eq!(clips[0].channels[0].property, ChannelProperty::Translation);
        assert_eq!(clips[0].channels[0].keyframes.len(), 2);

        // Evaluate at midpoint
        let poses = evaluate_clip(&clips[0], 0.5, skel.joints.len());
        assert!(approx_eq_vec3(
            &poses[0].translation,
            &[5.0, 0.0, 0.0],
            1e-4
        ));
    }

    #[test]
    fn sample_channel_rotation_slerp() {
        let id = [0.0f32, 0.0, 0.0, 1.0];
        // 90 degrees around Y
        let half_angle = std::f32::consts::FRAC_PI_4;
        let rot_90_y = [0.0, half_angle.sin(), 0.0, half_angle.cos()];

        let kfs = vec![
            Keyframe {
                time: 0.0,
                value: KeyframeValue::Quat(id),
            },
            Keyframe {
                time: 1.0,
                value: KeyframeValue::Quat(rot_90_y),
            },
        ];

        // At t=0.5, we should get ~45 degrees around Y
        let result = sample_channel(&kfs, 0.5);
        if let KeyframeValue::Quat(q) = result {
            let len = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
            assert!(approx_eq_f32(len, 1.0, 1e-4), "slerp result should be unit length");
            // y component should be positive
            assert!(q[1] > 0.0, "should have positive Y rotation component");
            // w component should be positive (< 90 degrees)
            assert!(q[3] > 0.0, "w should be positive for rotation < 180 degrees");
        } else {
            panic!("expected Quat");
        }
    }

    #[test]
    fn sample_channel_multiple_keyframes() {
        let kfs = vec![
            Keyframe {
                time: 0.0,
                value: KeyframeValue::Vec3([0.0, 0.0, 0.0]),
            },
            Keyframe {
                time: 1.0,
                value: KeyframeValue::Vec3([10.0, 0.0, 0.0]),
            },
            Keyframe {
                time: 2.0,
                value: KeyframeValue::Vec3([10.0, 20.0, 0.0]),
            },
            Keyframe {
                time: 3.0,
                value: KeyframeValue::Vec3([0.0, 20.0, 30.0]),
            },
        ];

        // First segment midpoint
        match sample_channel(&kfs, 0.5) {
            KeyframeValue::Vec3(v) => assert!(approx_eq_vec3(&v, &[5.0, 0.0, 0.0], 1e-5)),
            _ => panic!("expected Vec3"),
        }
        // Second segment midpoint
        match sample_channel(&kfs, 1.5) {
            KeyframeValue::Vec3(v) => assert!(approx_eq_vec3(&v, &[10.0, 10.0, 0.0], 1e-5)),
            _ => panic!("expected Vec3"),
        }
        // Third segment midpoint
        match sample_channel(&kfs, 2.5) {
            KeyframeValue::Vec3(v) => assert!(approx_eq_vec3(&v, &[5.0, 20.0, 15.0], 1e-5)),
            _ => panic!("expected Vec3"),
        }
    }

    #[test]
    fn evaluate_clip_empty_channels() {
        let clip = AnimationClip {
            name: "empty".to_string(),
            duration_secs: 0.0,
            channels: vec![],
        };
        let poses = evaluate_clip(&clip, 0.0, 4);
        assert_eq!(poses.len(), 4);
        for p in &poses {
            assert_eq!(p.translation, [0.0, 0.0, 0.0]);
            assert_eq!(p.rotation, [0.0, 0.0, 0.0, 1.0]);
            assert_eq!(p.scale, [1.0, 1.0, 1.0]);
        }
    }
}
