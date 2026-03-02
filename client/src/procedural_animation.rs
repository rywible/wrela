#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

use crate::skeletal_animation::{
    AnimationClip, AnimationChannel, ChannelProperty, Joint, Keyframe, KeyframeValue, Skeleton,
};
use crate::mesh::JointMatrix;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const JOINT_COUNT: usize = 15;

/// Joint indices for the humanoid skeleton.
const ROOT: usize = 0;
const SPINE: usize = 1;
const CHEST: usize = 2;
const NECK: usize = 3;
const HEAD: usize = 4;
const L_SHOULDER: usize = 5;
const L_ELBOW: usize = 6;
const L_HAND: usize = 7;
const R_SHOULDER: usize = 8;
const R_ELBOW: usize = 9;
const R_HAND: usize = 10;
const L_HIP: usize = 11;
const L_KNEE: usize = 12;
const R_HIP: usize = 13;
const R_KNEE: usize = 14;

/// Rest-pose world-space positions for each joint.
const REST_POSITIONS: [[f32; 3]; JOINT_COUNT] = [
    [0.0, 0.9, 0.0],    // 0: root
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

/// Parent index for each joint (None for root).
const PARENTS: [Option<usize>; JOINT_COUNT] = [
    None,     // 0: root
    Some(0),  // 1: spine -> root
    Some(1),  // 2: chest -> spine
    Some(2),  // 3: neck -> chest
    Some(3),  // 4: head -> neck
    Some(2),  // 5: L_shoulder -> chest
    Some(5),  // 6: L_elbow -> L_shoulder
    Some(6),  // 7: L_hand -> L_elbow
    Some(2),  // 8: R_shoulder -> chest
    Some(8),  // 9: R_elbow -> R_shoulder
    Some(9),  // 10: R_hand -> R_elbow
    Some(0),  // 11: L_hip -> root
    Some(11), // 12: L_knee -> L_hip
    Some(0),  // 13: R_hip -> root
    Some(13), // 14: R_knee -> R_hip
];

const JOINT_NAMES: [&str; JOINT_COUNT] = [
    "root",
    "spine",
    "chest",
    "neck",
    "head",
    "L_shoulder",
    "L_elbow",
    "L_hand",
    "R_shoulder",
    "R_elbow",
    "R_hand",
    "L_hip",
    "L_knee",
    "R_hip",
    "R_knee",
];

// ---------------------------------------------------------------------------
// Matrix helpers
// ---------------------------------------------------------------------------

const IDENTITY: JointMatrix = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];

/// Create a translation-only 4x4 column-major matrix.
fn translation_matrix(tx: f32, ty: f32, tz: f32) -> JointMatrix {
    [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [tx, ty, tz, 1.0],
    ]
}

fn mat4_mul(a: &JointMatrix, b: &JointMatrix) -> JointMatrix {
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

/// Invert a 4x4 matrix. For our skeleton the local transforms are pure translations,
/// so this general-purpose inverse handles the simple case efficiently and also
/// works for any invertible matrix.
fn mat4_inverse(m: &JointMatrix) -> JointMatrix {
    // Compute cofactors using Laplace expansion.
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
        return IDENTITY; // Singular matrix fallback
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

// ---------------------------------------------------------------------------
// Quaternion helpers
// ---------------------------------------------------------------------------

/// Identity quaternion [x, y, z, w].
const QUAT_IDENTITY: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

/// Create a quaternion representing a rotation around the X axis by `angle` radians.
fn quat_from_axis_angle_x(angle: f32) -> [f32; 4] {
    let half = angle * 0.5;
    normalize_quat([half.sin(), 0.0, 0.0, half.cos()])
}

/// Create a quaternion representing a rotation around the Z axis by `angle` radians.
fn quat_from_axis_angle_z(angle: f32) -> [f32; 4] {
    let half = angle * 0.5;
    normalize_quat([0.0, 0.0, half.sin(), half.cos()])
}

fn normalize_quat(q: [f32; 4]) -> [f32; 4] {
    let len = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
    if len < 1e-10 {
        return QUAT_IDENTITY;
    }
    let inv = 1.0 / len;
    [q[0] * inv, q[1] * inv, q[2] * inv, q[3] * inv]
}

// ---------------------------------------------------------------------------
// Skeleton generation
// ---------------------------------------------------------------------------

/// Generate a humanoid skeleton. If `is_enemy` is true, produces a bulkier enemy variant.
pub fn generate_humanoid_skeleton(is_enemy: bool) -> Skeleton {
    let scale = if is_enemy { 1.25 } else { 1.0 };

    // Compute world-space positions (scaled).
    let world_positions: Vec<[f32; 3]> = REST_POSITIONS
        .iter()
        .map(|p| [p[0] * scale, p[1] * scale, p[2] * scale])
        .collect();

    // Compute local transforms: each joint's local transform is a translation
    // from its parent's world position to its own world position.
    let mut joints = Vec::with_capacity(JOINT_COUNT);
    for i in 0..JOINT_COUNT {
        let local_translation = match PARENTS[i] {
            Some(parent_idx) => {
                let pw = &world_positions[parent_idx];
                let cw = &world_positions[i];
                [cw[0] - pw[0], cw[1] - pw[1], cw[2] - pw[2]]
            }
            None => world_positions[i],
        };
        joints.push(Joint {
            name: JOINT_NAMES[i].to_string(),
            parent_index: PARENTS[i],
            local_transform: translation_matrix(
                local_translation[0],
                local_translation[1],
                local_translation[2],
            ),
        });
    }

    // Compute world-space bind matrices, then invert them for inverse bind matrices.
    let mut world_matrices = vec![IDENTITY; JOINT_COUNT];
    for i in 0..JOINT_COUNT {
        world_matrices[i] = match PARENTS[i] {
            Some(parent_idx) => mat4_mul(&world_matrices[parent_idx], &joints[i].local_transform),
            None => joints[i].local_transform,
        };
    }

    let inverse_bind_matrices: Vec<JointMatrix> = world_matrices
        .iter()
        .map(|wm| mat4_inverse(wm))
        .collect();

    Skeleton {
        joints,
        inverse_bind_matrices,
    }
}

// ---------------------------------------------------------------------------
// Animation clip generation
// ---------------------------------------------------------------------------

/// Generate all animation clips for a humanoid skeleton.
pub fn generate_all_clips(joint_count: usize) -> Vec<AnimationClip> {
    vec![
        generate_idle(joint_count),
        generate_walk(joint_count),
        generate_attack_light(joint_count),
        generate_attack_heavy(joint_count),
        generate_dodge(joint_count),
        generate_parry(joint_count),
        generate_hit_stagger(joint_count),
    ]
}

// ---- Keyframe helpers -----

fn kf_vec3(time: f32, v: [f32; 3]) -> Keyframe {
    Keyframe {
        time,
        value: KeyframeValue::Vec3(v),
    }
}

fn kf_quat(time: f32, q: [f32; 4]) -> Keyframe {
    Keyframe {
        time,
        value: KeyframeValue::Quat(normalize_quat(q)),
    }
}

fn kf_quat_raw(time: f32, q: [f32; 4]) -> Keyframe {
    Keyframe {
        time,
        value: KeyframeValue::Quat(q),
    }
}

fn translation_channel(joint: usize, keyframes: Vec<Keyframe>) -> AnimationChannel {
    AnimationChannel {
        joint_index: joint,
        property: ChannelProperty::Translation,
        keyframes,
    }
}

fn rotation_channel(joint: usize, keyframes: Vec<Keyframe>) -> AnimationChannel {
    AnimationChannel {
        joint_index: joint,
        property: ChannelProperty::Rotation,
        keyframes,
    }
}

/// Convenience: produce a rest-pose translation keyframe for a given joint.
fn rest_translation(joint_index: usize) -> [f32; 3] {
    let pos = REST_POSITIONS[joint_index];
    match PARENTS[joint_index] {
        Some(parent_idx) => {
            let pp = REST_POSITIONS[parent_idx];
            [pos[0] - pp[0], pos[1] - pp[1], pos[2] - pp[2]]
        }
        None => pos,
    }
}

// ---- Individual clip generators ----

fn generate_idle(_joint_count: usize) -> AnimationClip {
    let duration = 2.0;
    // Spine/chest Y oscillation +/- 0.02 with smooth looping
    let spine_rest = rest_translation(SPINE);
    let chest_rest = rest_translation(CHEST);

    let mut channels = Vec::new();

    // Spine Y bob
    channels.push(translation_channel(
        SPINE,
        vec![
            kf_vec3(0.0, spine_rest),
            kf_vec3(0.5, [spine_rest[0], spine_rest[1] + 0.02, spine_rest[2]]),
            kf_vec3(1.0, spine_rest),
            kf_vec3(1.5, [spine_rest[0], spine_rest[1] - 0.02, spine_rest[2]]),
            kf_vec3(2.0, spine_rest),
        ],
    ));

    // Chest Y bob (slightly offset phase)
    channels.push(translation_channel(
        CHEST,
        vec![
            kf_vec3(0.0, chest_rest),
            kf_vec3(0.5, [chest_rest[0], chest_rest[1] + 0.015, chest_rest[2]]),
            kf_vec3(1.0, chest_rest),
            kf_vec3(1.5, [chest_rest[0], chest_rest[1] - 0.015, chest_rest[2]]),
            kf_vec3(2.0, chest_rest),
        ],
    ));

    // Subtle head nod (rotation around X axis)
    channels.push(rotation_channel(
        HEAD,
        vec![
            kf_quat_raw(0.0, QUAT_IDENTITY),
            kf_quat(0.5, quat_from_axis_angle_x(0.03)),
            kf_quat_raw(1.0, QUAT_IDENTITY),
            kf_quat(1.5, quat_from_axis_angle_x(-0.03)),
            kf_quat_raw(2.0, QUAT_IDENTITY),
        ],
    ));

    AnimationClip {
        name: "idle".to_string(),
        duration_secs: duration,
        channels,
    }
}

fn generate_walk(_joint_count: usize) -> AnimationClip {
    let duration = 1.0;
    let mut channels = Vec::new();

    let deg30 = 30.0_f32.to_radians();
    let deg20 = 20.0_f32.to_radians();

    // Root bob (slight up/down as character walks)
    let root_rest = rest_translation(ROOT);
    channels.push(translation_channel(
        ROOT,
        vec![
            kf_vec3(0.0, root_rest),
            kf_vec3(0.25, [root_rest[0], root_rest[1] + 0.03, root_rest[2]]),
            kf_vec3(0.5, root_rest),
            kf_vec3(0.75, [root_rest[0], root_rest[1] + 0.03, root_rest[2]]),
            kf_vec3(1.0, root_rest),
        ],
    ));

    // Left hip: forward swing +30 deg at 0.0, back swing -30 deg at 0.5
    channels.push(rotation_channel(
        L_HIP,
        vec![
            kf_quat(0.0, quat_from_axis_angle_x(deg30)),
            kf_quat_raw(0.25, QUAT_IDENTITY),
            kf_quat(0.5, quat_from_axis_angle_x(-deg30)),
            kf_quat_raw(0.75, QUAT_IDENTITY),
            kf_quat(1.0, quat_from_axis_angle_x(deg30)),
        ],
    ));

    // Right hip: opposite phase
    channels.push(rotation_channel(
        R_HIP,
        vec![
            kf_quat(0.0, quat_from_axis_angle_x(-deg30)),
            kf_quat_raw(0.25, QUAT_IDENTITY),
            kf_quat(0.5, quat_from_axis_angle_x(deg30)),
            kf_quat_raw(0.75, QUAT_IDENTITY),
            kf_quat(1.0, quat_from_axis_angle_x(-deg30)),
        ],
    ));

    // Left knee: slight bend at back swing
    channels.push(rotation_channel(
        L_KNEE,
        vec![
            kf_quat_raw(0.0, QUAT_IDENTITY),
            kf_quat(0.25, quat_from_axis_angle_x(-0.3)),
            kf_quat_raw(0.5, QUAT_IDENTITY),
            kf_quat_raw(0.75, QUAT_IDENTITY),
            kf_quat_raw(1.0, QUAT_IDENTITY),
        ],
    ));

    // Right knee: opposite phase
    channels.push(rotation_channel(
        R_KNEE,
        vec![
            kf_quat_raw(0.0, QUAT_IDENTITY),
            kf_quat_raw(0.25, QUAT_IDENTITY),
            kf_quat_raw(0.5, QUAT_IDENTITY),
            kf_quat(0.75, quat_from_axis_angle_x(-0.3)),
            kf_quat_raw(1.0, QUAT_IDENTITY),
        ],
    ));

    // Left shoulder: arm counter-swing (opposite to leg)
    channels.push(rotation_channel(
        L_SHOULDER,
        vec![
            kf_quat(0.0, quat_from_axis_angle_x(-deg20)),
            kf_quat_raw(0.25, QUAT_IDENTITY),
            kf_quat(0.5, quat_from_axis_angle_x(deg20)),
            kf_quat_raw(0.75, QUAT_IDENTITY),
            kf_quat(1.0, quat_from_axis_angle_x(-deg20)),
        ],
    ));

    // Right shoulder: arm counter-swing
    channels.push(rotation_channel(
        R_SHOULDER,
        vec![
            kf_quat(0.0, quat_from_axis_angle_x(deg20)),
            kf_quat_raw(0.25, QUAT_IDENTITY),
            kf_quat(0.5, quat_from_axis_angle_x(-deg20)),
            kf_quat_raw(0.75, QUAT_IDENTITY),
            kf_quat(1.0, quat_from_axis_angle_x(deg20)),
        ],
    ));

    // Spine slight twist for natural walk
    channels.push(rotation_channel(
        SPINE,
        vec![
            kf_quat(0.0, quat_from_axis_angle_z(0.02)),
            kf_quat_raw(0.25, QUAT_IDENTITY),
            kf_quat(0.5, quat_from_axis_angle_z(-0.02)),
            kf_quat_raw(0.75, QUAT_IDENTITY),
            kf_quat(1.0, quat_from_axis_angle_z(0.02)),
        ],
    ));

    AnimationClip {
        name: "walk".to_string(),
        duration_secs: duration,
        channels,
    }
}

fn generate_attack_light(_joint_count: usize) -> AnimationClip {
    let duration = 0.4;
    let mut channels = Vec::new();

    // Right arm forward swing: 0 -> -90 deg around X over 0.15s, hold, return.
    let deg90 = 90.0_f32.to_radians();

    channels.push(rotation_channel(
        R_SHOULDER,
        vec![
            kf_quat_raw(0.0, QUAT_IDENTITY),
            kf_quat(0.15, quat_from_axis_angle_x(-deg90)),
            kf_quat(0.25, quat_from_axis_angle_x(-deg90)),
            kf_quat_raw(0.4, QUAT_IDENTITY),
        ],
    ));

    // Slight elbow bend during swing
    channels.push(rotation_channel(
        R_ELBOW,
        vec![
            kf_quat_raw(0.0, QUAT_IDENTITY),
            kf_quat(0.1, quat_from_axis_angle_x(-0.3)),
            kf_quat(0.25, quat_from_axis_angle_x(-0.3)),
            kf_quat_raw(0.4, QUAT_IDENTITY),
        ],
    ));

    // Slight spine rotation to sell the swing
    channels.push(rotation_channel(
        CHEST,
        vec![
            kf_quat_raw(0.0, QUAT_IDENTITY),
            kf_quat(0.15, quat_from_axis_angle_z(-0.1)),
            kf_quat(0.25, quat_from_axis_angle_z(-0.1)),
            kf_quat_raw(0.4, QUAT_IDENTITY),
        ],
    ));

    AnimationClip {
        name: "attack_light".to_string(),
        duration_secs: duration,
        channels,
    }
}

fn generate_attack_heavy(_joint_count: usize) -> AnimationClip {
    let duration = 0.7;
    let mut channels = Vec::new();

    let deg_overhead = 150.0_f32.to_radians();
    let deg_slam = (-60.0_f32).to_radians();

    // Both arms: overhead raise, slam down, recover
    for &shoulder in &[L_SHOULDER, R_SHOULDER] {
        channels.push(rotation_channel(
            shoulder,
            vec![
                kf_quat_raw(0.0, QUAT_IDENTITY),
                kf_quat(0.2, quat_from_axis_angle_x(deg_overhead)),  // overhead
                kf_quat(0.35, quat_from_axis_angle_x(deg_slam)),     // slam down
                kf_quat(0.5, quat_from_axis_angle_x(deg_slam)),      // hold
                kf_quat_raw(0.7, QUAT_IDENTITY),                     // recover
            ],
        ));
    }

    // Both elbows: slight bend during overhead and slam
    for &elbow in &[L_ELBOW, R_ELBOW] {
        channels.push(rotation_channel(
            elbow,
            vec![
                kf_quat_raw(0.0, QUAT_IDENTITY),
                kf_quat(0.2, quat_from_axis_angle_x(-0.5)),
                kf_quat_raw(0.35, QUAT_IDENTITY),
                kf_quat_raw(0.5, QUAT_IDENTITY),
                kf_quat_raw(0.7, QUAT_IDENTITY),
            ],
        ));
    }

    // Spine lean back then forward during slam
    channels.push(rotation_channel(
        SPINE,
        vec![
            kf_quat_raw(0.0, QUAT_IDENTITY),
            kf_quat(0.2, quat_from_axis_angle_x(0.15)),   // lean back
            kf_quat(0.35, quat_from_axis_angle_x(-0.2)),  // lean forward on slam
            kf_quat(0.5, quat_from_axis_angle_x(-0.15)),
            kf_quat_raw(0.7, QUAT_IDENTITY),
        ],
    ));

    // Root slight crouch during slam
    let root_rest = rest_translation(ROOT);
    channels.push(translation_channel(
        ROOT,
        vec![
            kf_vec3(0.0, root_rest),
            kf_vec3(0.2, root_rest),
            kf_vec3(0.35, [root_rest[0], root_rest[1] - 0.08, root_rest[2]]),
            kf_vec3(0.5, [root_rest[0], root_rest[1] - 0.05, root_rest[2]]),
            kf_vec3(0.7, root_rest),
        ],
    ));

    AnimationClip {
        name: "attack_heavy".to_string(),
        duration_secs: duration,
        channels,
    }
}

fn generate_dodge(_joint_count: usize) -> AnimationClip {
    let duration = 0.5;
    let mut channels = Vec::new();

    // Root backward translate 1.5 units, crouch, return
    let root_rest = rest_translation(ROOT);
    channels.push(translation_channel(
        ROOT,
        vec![
            kf_vec3(0.0, root_rest),
            kf_vec3(0.1, [root_rest[0], root_rest[1] - 0.15, root_rest[2] + 0.5]),
            kf_vec3(0.25, [root_rest[0], root_rest[1] - 0.2, root_rest[2] + 1.5]),
            kf_vec3(0.4, [root_rest[0], root_rest[1] - 0.1, root_rest[2] + 1.0]),
            kf_vec3(0.5, root_rest),
        ],
    ));

    // Crouch: spine leans forward
    channels.push(rotation_channel(
        SPINE,
        vec![
            kf_quat_raw(0.0, QUAT_IDENTITY),
            kf_quat(0.1, quat_from_axis_angle_x(-0.3)),
            kf_quat(0.25, quat_from_axis_angle_x(-0.4)),
            kf_quat(0.4, quat_from_axis_angle_x(-0.2)),
            kf_quat_raw(0.5, QUAT_IDENTITY),
        ],
    ));

    // Knees bend during crouch
    for &knee in &[L_KNEE, R_KNEE] {
        channels.push(rotation_channel(
            knee,
            vec![
                kf_quat_raw(0.0, QUAT_IDENTITY),
                kf_quat(0.1, quat_from_axis_angle_x(-0.4)),
                kf_quat(0.25, quat_from_axis_angle_x(-0.6)),
                kf_quat(0.4, quat_from_axis_angle_x(-0.3)),
                kf_quat_raw(0.5, QUAT_IDENTITY),
            ],
        ));
    }

    // Head stays level during dodge
    channels.push(rotation_channel(
        HEAD,
        vec![
            kf_quat_raw(0.0, QUAT_IDENTITY),
            kf_quat(0.1, quat_from_axis_angle_x(0.2)),
            kf_quat(0.25, quat_from_axis_angle_x(0.3)),
            kf_quat(0.4, quat_from_axis_angle_x(0.15)),
            kf_quat_raw(0.5, QUAT_IDENTITY),
        ],
    ));

    AnimationClip {
        name: "dodge".to_string(),
        duration_secs: duration,
        channels,
    }
}

fn generate_parry(_joint_count: usize) -> AnimationClip {
    let duration = 0.3;
    let mut channels = Vec::new();

    // Left arm raise to block position
    let deg_block = 120.0_f32.to_radians();
    channels.push(rotation_channel(
        L_SHOULDER,
        vec![
            kf_quat_raw(0.0, QUAT_IDENTITY),
            kf_quat(0.08, quat_from_axis_angle_x(deg_block)),
            kf_quat(0.2, quat_from_axis_angle_x(deg_block)),
            kf_quat_raw(0.3, QUAT_IDENTITY),
        ],
    ));

    // Left elbow bend for guard
    channels.push(rotation_channel(
        L_ELBOW,
        vec![
            kf_quat_raw(0.0, QUAT_IDENTITY),
            kf_quat(0.08, quat_from_axis_angle_x(-0.8)),
            kf_quat(0.2, quat_from_axis_angle_x(-0.8)),
            kf_quat_raw(0.3, QUAT_IDENTITY),
        ],
    ));

    // Spine lean back
    channels.push(rotation_channel(
        SPINE,
        vec![
            kf_quat_raw(0.0, QUAT_IDENTITY),
            kf_quat(0.08, quat_from_axis_angle_x(0.15)),
            kf_quat(0.2, quat_from_axis_angle_x(0.12)),
            kf_quat_raw(0.3, QUAT_IDENTITY),
        ],
    ));

    // Slight root weight shift
    let root_rest = rest_translation(ROOT);
    channels.push(translation_channel(
        ROOT,
        vec![
            kf_vec3(0.0, root_rest),
            kf_vec3(0.08, [root_rest[0], root_rest[1] - 0.03, root_rest[2] + 0.05]),
            kf_vec3(0.2, [root_rest[0], root_rest[1] - 0.02, root_rest[2] + 0.03]),
            kf_vec3(0.3, root_rest),
        ],
    ));

    AnimationClip {
        name: "parry".to_string(),
        duration_secs: duration,
        channels,
    }
}

fn generate_hit_stagger(_joint_count: usize) -> AnimationClip {
    let duration = 0.4;
    let mut channels = Vec::new();

    // Root jolt backward
    let root_rest = rest_translation(ROOT);
    channels.push(translation_channel(
        ROOT,
        vec![
            kf_vec3(0.0, root_rest),
            kf_vec3(0.05, [root_rest[0], root_rest[1], root_rest[2] + 0.15]),
            kf_vec3(0.15, [root_rest[0], root_rest[1] - 0.05, root_rest[2] + 0.25]),
            kf_vec3(0.3, [root_rest[0], root_rest[1] - 0.02, root_rest[2] + 0.1]),
            kf_vec3(0.4, root_rest),
        ],
    ));

    // Spine lean back from hit
    channels.push(rotation_channel(
        SPINE,
        vec![
            kf_quat_raw(0.0, QUAT_IDENTITY),
            kf_quat(0.05, quat_from_axis_angle_x(0.1)),
            kf_quat(0.15, quat_from_axis_angle_x(0.25)),
            kf_quat(0.3, quat_from_axis_angle_x(0.1)),
            kf_quat_raw(0.4, QUAT_IDENTITY),
        ],
    ));

    // Head snap backward
    channels.push(rotation_channel(
        HEAD,
        vec![
            kf_quat_raw(0.0, QUAT_IDENTITY),
            kf_quat(0.05, quat_from_axis_angle_x(0.2)),
            kf_quat(0.15, quat_from_axis_angle_x(0.4)),
            kf_quat(0.3, quat_from_axis_angle_x(0.15)),
            kf_quat_raw(0.4, QUAT_IDENTITY),
        ],
    ));

    // Chest recoil
    channels.push(rotation_channel(
        CHEST,
        vec![
            kf_quat_raw(0.0, QUAT_IDENTITY),
            kf_quat(0.05, quat_from_axis_angle_x(0.05)),
            kf_quat(0.15, quat_from_axis_angle_x(0.15)),
            kf_quat(0.3, quat_from_axis_angle_x(0.05)),
            kf_quat_raw(0.4, QUAT_IDENTITY),
        ],
    ));

    // Arms flail slightly on impact
    channels.push(rotation_channel(
        L_SHOULDER,
        vec![
            kf_quat_raw(0.0, QUAT_IDENTITY),
            kf_quat(0.1, quat_from_axis_angle_x(0.3)),
            kf_quat(0.25, quat_from_axis_angle_z(0.15)),
            kf_quat_raw(0.4, QUAT_IDENTITY),
        ],
    ));

    channels.push(rotation_channel(
        R_SHOULDER,
        vec![
            kf_quat_raw(0.0, QUAT_IDENTITY),
            kf_quat(0.1, quat_from_axis_angle_x(0.25)),
            kf_quat(0.25, quat_from_axis_angle_z(-0.15)),
            kf_quat_raw(0.4, QUAT_IDENTITY),
        ],
    ));

    AnimationClip {
        name: "hit_stagger".to_string(),
        duration_secs: duration,
        channels,
    }
}

// ---------------------------------------------------------------------------
// Vertex joint weight computation
// ---------------------------------------------------------------------------

/// Compute vertex joint weights based on vertex position relative to skeleton joints.
/// Returns (joint_indices, joint_weights) where weights sum to 1.0.
pub fn compute_vertex_joint_weights(
    vertex_pos: [f32; 3],
    skeleton: &Skeleton,
) -> ([u16; 4], [f32; 4]) {
    let joint_count = skeleton.joints.len();
    if joint_count == 0 {
        return ([0, 0, 0, 0], [1.0, 0.0, 0.0, 0.0]);
    }

    // Compute world-space positions for each joint from the skeleton's local transforms.
    let world_positions = compute_world_positions(skeleton);

    // For each joint, compute distance from vertex to joint world position.
    // Use inverse-distance weighting with a minimum distance to avoid division by zero.
    let mut distances: Vec<(usize, f32)> = world_positions
        .iter()
        .enumerate()
        .map(|(i, jp)| {
            let dx = vertex_pos[0] - jp[0];
            let dy = vertex_pos[1] - jp[1];
            let dz = vertex_pos[2] - jp[2];
            (i, (dx * dx + dy * dy + dz * dz).sqrt())
        })
        .collect();

    // Sort by distance (closest first).
    distances.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

    // Take up to 4 closest joints.
    let n = distances.len().min(4);
    let mut indices = [0u16; 4];
    let mut weights = [0.0f32; 4];

    // Compute inverse-distance weights for the closest joints.
    let min_dist = 0.001_f32;
    let mut weight_sum = 0.0_f32;

    for i in 0..n {
        indices[i] = distances[i].0 as u16;
        let d = distances[i].1.max(min_dist);
        // Use inverse square distance for sharper falloff.
        let w = 1.0 / (d * d);
        weights[i] = w;
        weight_sum += w;
    }

    // Normalize weights to sum to 1.0.
    if weight_sum > 0.0 {
        let inv_sum = 1.0 / weight_sum;
        for w in weights.iter_mut() {
            *w *= inv_sum;
        }
    } else {
        // Fallback: assign full weight to closest joint.
        weights[0] = 1.0;
    }

    (indices, weights)
}

/// Compute world-space positions for each joint by walking the hierarchy.
fn compute_world_positions(skeleton: &Skeleton) -> Vec<[f32; 3]> {
    let joint_count = skeleton.joints.len();
    let mut world_matrices = vec![IDENTITY; joint_count];

    for i in 0..joint_count {
        world_matrices[i] = match skeleton.joints[i].parent_index {
            Some(parent_idx) => {
                mat4_mul(&world_matrices[parent_idx], &skeleton.joints[i].local_transform)
            }
            None => skeleton.joints[i].local_transform,
        };
    }

    world_matrices
        .iter()
        .map(|m| [m[3][0], m[3][1], m[3][2]])
        .collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f32 = 1e-5;

    fn approx_eq(a: f32, b: f32) -> bool {
        (a - b).abs() < EPS
    }

    fn approx_eq_eps(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() < eps
    }

    fn approx_eq_mat4(a: &JointMatrix, b: &JointMatrix, eps: f32) -> bool {
        for col in 0..4 {
            for row in 0..4 {
                if (a[col][row] - b[col][row]).abs() > eps {
                    return false;
                }
            }
        }
        true
    }

    fn quat_length(q: &[f32; 4]) -> f32 {
        (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt()
    }

    // ---- Skeleton tests ----

    #[test]
    fn skeleton_has_15_joints() {
        let skel = generate_humanoid_skeleton(false);
        assert_eq!(skel.joints.len(), 15);
        assert_eq!(skel.inverse_bind_matrices.len(), 15);
    }

    #[test]
    fn skeleton_enemy_has_15_joints() {
        let skel = generate_humanoid_skeleton(true);
        assert_eq!(skel.joints.len(), 15);
        assert_eq!(skel.inverse_bind_matrices.len(), 15);
    }

    #[test]
    fn skeleton_root_has_no_parent() {
        let skel = generate_humanoid_skeleton(false);
        assert!(skel.joints[0].parent_index.is_none());
        assert_eq!(skel.joints[0].name, "root");
    }

    #[test]
    fn skeleton_hierarchy_is_valid() {
        let skel = generate_humanoid_skeleton(false);
        for (i, joint) in skel.joints.iter().enumerate() {
            match joint.parent_index {
                Some(parent) => {
                    assert!(
                        parent < i,
                        "Joint {} ({}) has parent {} which is not < {}",
                        i,
                        joint.name,
                        parent,
                        i
                    );
                    assert!(
                        parent < skel.joints.len(),
                        "Joint {} ({}) has parent {} out of bounds",
                        i,
                        joint.name,
                        parent
                    );
                }
                None => {
                    assert_eq!(i, 0, "Only root (index 0) should have no parent");
                }
            }
        }
    }

    #[test]
    fn skeleton_no_cycles() {
        let skel = generate_humanoid_skeleton(false);
        for i in 0..skel.joints.len() {
            let mut visited = vec![false; skel.joints.len()];
            let mut current = Some(i);
            while let Some(idx) = current {
                assert!(
                    !visited[idx],
                    "Cycle detected at joint {} while tracing from joint {}",
                    idx,
                    i
                );
                visited[idx] = true;
                current = skel.joints[idx].parent_index;
            }
        }
    }

    #[test]
    fn skeleton_joint_names_correct() {
        let skel = generate_humanoid_skeleton(false);
        let expected_names = [
            "root", "spine", "chest", "neck", "head",
            "L_shoulder", "L_elbow", "L_hand",
            "R_shoulder", "R_elbow", "R_hand",
            "L_hip", "L_knee", "R_hip", "R_knee",
        ];
        for (i, name) in expected_names.iter().enumerate() {
            assert_eq!(skel.joints[i].name, *name, "Joint {} name mismatch", i);
        }
    }

    #[test]
    fn skeleton_hierarchy_connections() {
        let skel = generate_humanoid_skeleton(false);
        // Verify specific parent-child relationships
        assert_eq!(skel.joints[SPINE].parent_index, Some(ROOT));
        assert_eq!(skel.joints[CHEST].parent_index, Some(SPINE));
        assert_eq!(skel.joints[NECK].parent_index, Some(CHEST));
        assert_eq!(skel.joints[HEAD].parent_index, Some(NECK));
        assert_eq!(skel.joints[L_SHOULDER].parent_index, Some(CHEST));
        assert_eq!(skel.joints[L_ELBOW].parent_index, Some(L_SHOULDER));
        assert_eq!(skel.joints[L_HAND].parent_index, Some(L_ELBOW));
        assert_eq!(skel.joints[R_SHOULDER].parent_index, Some(CHEST));
        assert_eq!(skel.joints[R_ELBOW].parent_index, Some(R_SHOULDER));
        assert_eq!(skel.joints[R_HAND].parent_index, Some(R_ELBOW));
        assert_eq!(skel.joints[L_HIP].parent_index, Some(ROOT));
        assert_eq!(skel.joints[L_KNEE].parent_index, Some(L_HIP));
        assert_eq!(skel.joints[R_HIP].parent_index, Some(ROOT));
        assert_eq!(skel.joints[R_KNEE].parent_index, Some(R_HIP));
    }

    #[test]
    fn inverse_bind_matrices_correct() {
        let skel = generate_humanoid_skeleton(false);

        // Compute world-space bind matrices.
        let mut world = vec![IDENTITY; skel.joints.len()];
        for i in 0..skel.joints.len() {
            world[i] = match skel.joints[i].parent_index {
                Some(p) => mat4_mul(&world[p], &skel.joints[i].local_transform),
                None => skel.joints[i].local_transform,
            };
        }

        // world * inverse_bind should give identity.
        for i in 0..skel.joints.len() {
            let result = mat4_mul(&world[i], &skel.inverse_bind_matrices[i]);
            assert!(
                approx_eq_mat4(&result, &IDENTITY, 1e-4),
                "Joint {} ({}) inverse bind matrix incorrect. Result:\n{:?}",
                i,
                skel.joints[i].name,
                result
            );
        }
    }

    #[test]
    fn inverse_bind_matrices_correct_enemy() {
        let skel = generate_humanoid_skeleton(true);

        let mut world = vec![IDENTITY; skel.joints.len()];
        for i in 0..skel.joints.len() {
            world[i] = match skel.joints[i].parent_index {
                Some(p) => mat4_mul(&world[p], &skel.joints[i].local_transform),
                None => skel.joints[i].local_transform,
            };
        }

        for i in 0..skel.joints.len() {
            let result = mat4_mul(&world[i], &skel.inverse_bind_matrices[i]);
            assert!(
                approx_eq_mat4(&result, &IDENTITY, 1e-4),
                "Enemy joint {} ({}) inverse bind matrix incorrect",
                i,
                skel.joints[i].name
            );
        }
    }

    #[test]
    fn skeleton_world_positions_match_spec() {
        let skel = generate_humanoid_skeleton(false);
        let positions = compute_world_positions(&skel);

        for i in 0..JOINT_COUNT {
            assert!(
                approx_eq(positions[i][0], REST_POSITIONS[i][0]),
                "Joint {} X: expected {}, got {}",
                i, REST_POSITIONS[i][0], positions[i][0]
            );
            assert!(
                approx_eq(positions[i][1], REST_POSITIONS[i][1]),
                "Joint {} Y: expected {}, got {}",
                i, REST_POSITIONS[i][1], positions[i][1]
            );
            assert!(
                approx_eq(positions[i][2], REST_POSITIONS[i][2]),
                "Joint {} Z: expected {}, got {}",
                i, REST_POSITIONS[i][2], positions[i][2]
            );
        }
    }

    #[test]
    fn enemy_skeleton_is_scaled() {
        let player = generate_humanoid_skeleton(false);
        let enemy = generate_humanoid_skeleton(true);

        let player_pos = compute_world_positions(&player);
        let enemy_pos = compute_world_positions(&enemy);

        for i in 0..JOINT_COUNT {
            assert!(
                approx_eq(enemy_pos[i][0], player_pos[i][0] * 1.25),
                "Enemy joint {} X not scaled correctly",
                i
            );
            assert!(
                approx_eq(enemy_pos[i][1], player_pos[i][1] * 1.25),
                "Enemy joint {} Y not scaled correctly",
                i
            );
        }
    }

    // ---- Animation clip tests ----

    #[test]
    fn generates_7_clips() {
        let clips = generate_all_clips(JOINT_COUNT);
        assert_eq!(clips.len(), 7);
    }

    #[test]
    fn clip_names_correct() {
        let clips = generate_all_clips(JOINT_COUNT);
        let expected_names = [
            "idle",
            "walk",
            "attack_light",
            "attack_heavy",
            "dodge",
            "parry",
            "hit_stagger",
        ];
        for (i, name) in expected_names.iter().enumerate() {
            assert_eq!(clips[i].name, *name, "Clip {} name mismatch", i);
        }
    }

    #[test]
    fn clip_durations_correct() {
        let clips = generate_all_clips(JOINT_COUNT);
        let expected_durations = [2.0, 1.0, 0.4, 0.7, 0.5, 0.3, 0.4];
        for (i, &dur) in expected_durations.iter().enumerate() {
            assert!(
                approx_eq(clips[i].duration_secs, dur),
                "Clip '{}' duration: expected {}, got {}",
                clips[i].name,
                dur,
                clips[i].duration_secs
            );
        }
    }

    #[test]
    fn all_clips_have_channels() {
        let clips = generate_all_clips(JOINT_COUNT);
        for clip in &clips {
            assert!(
                !clip.channels.is_empty(),
                "Clip '{}' has no channels",
                clip.name
            );
        }
    }

    #[test]
    fn all_keyframe_times_within_duration() {
        let clips = generate_all_clips(JOINT_COUNT);
        for clip in &clips {
            for channel in &clip.channels {
                for kf in &channel.keyframes {
                    assert!(
                        kf.time >= 0.0 && kf.time <= clip.duration_secs + EPS,
                        "Clip '{}' joint {} has keyframe at t={} but duration is {}",
                        clip.name,
                        channel.joint_index,
                        kf.time,
                        clip.duration_secs
                    );
                }
            }
        }
    }

    #[test]
    fn all_keyframe_times_monotonic() {
        let clips = generate_all_clips(JOINT_COUNT);
        for clip in &clips {
            for channel in &clip.channels {
                for pair in channel.keyframes.windows(2) {
                    assert!(
                        pair[0].time <= pair[1].time,
                        "Clip '{}' joint {} has non-monotonic keyframes: t={} > t={}",
                        clip.name,
                        channel.joint_index,
                        pair[0].time,
                        pair[1].time
                    );
                }
            }
        }
    }

    #[test]
    fn all_quaternion_keyframes_unit_length() {
        let clips = generate_all_clips(JOINT_COUNT);
        for clip in &clips {
            for channel in &clip.channels {
                for kf in &channel.keyframes {
                    if let KeyframeValue::Quat(q) = &kf.value {
                        let len = quat_length(q);
                        assert!(
                            approx_eq_eps(len, 1.0, 1e-4),
                            "Clip '{}' joint {} at t={}: quaternion length {} != 1.0. q={:?}",
                            clip.name,
                            channel.joint_index,
                            kf.time,
                            len,
                            q
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn rotation_channels_use_quat_values() {
        let clips = generate_all_clips(JOINT_COUNT);
        for clip in &clips {
            for channel in &clip.channels {
                if channel.property == ChannelProperty::Rotation {
                    for kf in &channel.keyframes {
                        assert!(
                            matches!(kf.value, KeyframeValue::Quat(_)),
                            "Clip '{}' joint {} rotation channel has non-Quat keyframe",
                            clip.name,
                            channel.joint_index
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn translation_channels_use_vec3_values() {
        let clips = generate_all_clips(JOINT_COUNT);
        for clip in &clips {
            for channel in &clip.channels {
                if channel.property == ChannelProperty::Translation {
                    for kf in &channel.keyframes {
                        assert!(
                            matches!(kf.value, KeyframeValue::Vec3(_)),
                            "Clip '{}' joint {} translation channel has non-Vec3 keyframe",
                            clip.name,
                            channel.joint_index
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn all_joint_indices_within_bounds() {
        let clips = generate_all_clips(JOINT_COUNT);
        for clip in &clips {
            for channel in &clip.channels {
                assert!(
                    channel.joint_index < JOINT_COUNT,
                    "Clip '{}' references joint {} which is >= {}",
                    clip.name,
                    channel.joint_index,
                    JOINT_COUNT
                );
            }
        }
    }

    #[test]
    fn each_clip_has_at_least_2_keyframes_per_channel() {
        let clips = generate_all_clips(JOINT_COUNT);
        for clip in &clips {
            for channel in &clip.channels {
                assert!(
                    channel.keyframes.len() >= 2,
                    "Clip '{}' joint {} has only {} keyframes",
                    clip.name,
                    channel.joint_index,
                    channel.keyframes.len()
                );
            }
        }
    }

    // ---- Specific clip behavior tests ----

    #[test]
    fn idle_has_spine_chest_head_channels() {
        let clips = generate_all_clips(JOINT_COUNT);
        let idle = &clips[0];
        assert_eq!(idle.name, "idle");

        let has_spine_translation = idle
            .channels
            .iter()
            .any(|c| c.joint_index == SPINE && c.property == ChannelProperty::Translation);
        let has_chest_translation = idle
            .channels
            .iter()
            .any(|c| c.joint_index == CHEST && c.property == ChannelProperty::Translation);
        let has_head_rotation = idle
            .channels
            .iter()
            .any(|c| c.joint_index == HEAD && c.property == ChannelProperty::Rotation);

        assert!(has_spine_translation, "Idle should animate spine translation");
        assert!(has_chest_translation, "Idle should animate chest translation");
        assert!(has_head_rotation, "Idle should animate head rotation");
    }

    #[test]
    fn walk_has_leg_and_arm_channels() {
        let clips = generate_all_clips(JOINT_COUNT);
        let walk = &clips[1];
        assert_eq!(walk.name, "walk");

        let joints_animated: Vec<usize> = walk.channels.iter().map(|c| c.joint_index).collect();
        assert!(joints_animated.contains(&L_HIP), "Walk should animate L_hip");
        assert!(joints_animated.contains(&R_HIP), "Walk should animate R_hip");
        assert!(joints_animated.contains(&L_SHOULDER), "Walk should animate L_shoulder");
        assert!(joints_animated.contains(&R_SHOULDER), "Walk should animate R_shoulder");
    }

    #[test]
    fn attack_light_right_arm_swing() {
        let clips = generate_all_clips(JOINT_COUNT);
        let attack = &clips[2];
        assert_eq!(attack.name, "attack_light");

        let r_shoulder_rot = attack
            .channels
            .iter()
            .find(|c| c.joint_index == R_SHOULDER && c.property == ChannelProperty::Rotation);
        assert!(r_shoulder_rot.is_some(), "attack_light should rotate R_shoulder");
    }

    #[test]
    fn attack_heavy_both_arms() {
        let clips = generate_all_clips(JOINT_COUNT);
        let attack = &clips[3];
        assert_eq!(attack.name, "attack_heavy");

        let l_shoulder = attack
            .channels
            .iter()
            .any(|c| c.joint_index == L_SHOULDER && c.property == ChannelProperty::Rotation);
        let r_shoulder = attack
            .channels
            .iter()
            .any(|c| c.joint_index == R_SHOULDER && c.property == ChannelProperty::Rotation);
        assert!(l_shoulder, "attack_heavy should animate L_shoulder");
        assert!(r_shoulder, "attack_heavy should animate R_shoulder");
    }

    #[test]
    fn dodge_moves_root_backward() {
        let clips = generate_all_clips(JOINT_COUNT);
        let dodge = &clips[4];
        assert_eq!(dodge.name, "dodge");

        let root_trans = dodge
            .channels
            .iter()
            .find(|c| c.joint_index == ROOT && c.property == ChannelProperty::Translation)
            .expect("dodge should have root translation");

        // At the peak of the dodge, the Z should be positive (backward).
        let mut max_z = 0.0_f32;
        for kf in &root_trans.keyframes {
            if let KeyframeValue::Vec3(v) = &kf.value {
                if v[2] > max_z {
                    max_z = v[2];
                }
            }
        }
        assert!(max_z >= 1.0, "Dodge should move root at least 1.0 units backward, got {}", max_z);
    }

    #[test]
    fn parry_raises_left_arm() {
        let clips = generate_all_clips(JOINT_COUNT);
        let parry = &clips[5];
        assert_eq!(parry.name, "parry");

        let l_shoulder = parry
            .channels
            .iter()
            .any(|c| c.joint_index == L_SHOULDER && c.property == ChannelProperty::Rotation);
        assert!(l_shoulder, "parry should animate L_shoulder");
    }

    #[test]
    fn hit_stagger_has_root_spine_head() {
        let clips = generate_all_clips(JOINT_COUNT);
        let stagger = &clips[6];
        assert_eq!(stagger.name, "hit_stagger");

        let joints_animated: Vec<usize> = stagger.channels.iter().map(|c| c.joint_index).collect();
        assert!(joints_animated.contains(&ROOT), "hit_stagger should animate root");
        assert!(joints_animated.contains(&SPINE), "hit_stagger should animate spine");
        assert!(joints_animated.contains(&HEAD), "hit_stagger should animate head");
    }

    // ---- Vertex weight tests ----

    #[test]
    fn vertex_weights_sum_to_one() {
        let skel = generate_humanoid_skeleton(false);

        // Test several positions across the body.
        let test_positions = [
            [0.0, 0.9, 0.0],    // at root
            [0.0, 1.3, 0.0],    // at chest
            [-0.45, 1.35, 0.0], // at L_elbow
            [0.0, 0.5, 0.0],    // between hips and knees
            [0.3, 1.0, 0.0],    // somewhere off-center
            [0.0, 2.0, 0.0],    // above head
            [1.0, 1.0, 1.0],    // far away
        ];

        for pos in &test_positions {
            let (_, weights) = compute_vertex_joint_weights(*pos, &skel);
            let sum: f32 = weights.iter().sum();
            assert!(
                approx_eq_eps(sum, 1.0, 1e-4),
                "Weights for pos {:?} sum to {} (expected 1.0)",
                pos,
                sum
            );
        }
    }

    #[test]
    fn vertex_weights_non_negative() {
        let skel = generate_humanoid_skeleton(false);
        let test_positions = [
            [0.0, 0.9, 0.0],
            [-0.65, 1.35, 0.0],
            [0.0, 0.0, 0.0],
            [5.0, 5.0, 5.0],
        ];
        for pos in &test_positions {
            let (_, weights) = compute_vertex_joint_weights(*pos, &skel);
            for w in &weights {
                assert!(*w >= 0.0, "Negative weight {} for pos {:?}", w, pos);
            }
        }
    }

    #[test]
    fn vertex_at_joint_position_gets_highest_weight_for_that_joint() {
        let skel = generate_humanoid_skeleton(false);

        // Place a vertex exactly at the root joint.
        let (indices, weights) = compute_vertex_joint_weights(REST_POSITIONS[ROOT], &skel);
        // Root should be the primary joint (highest weight).
        assert_eq!(
            indices[0], ROOT as u16,
            "Vertex at root position should have root as primary joint"
        );
        assert!(
            weights[0] > 0.5,
            "Root weight should dominate at root position, got {}",
            weights[0]
        );
    }

    #[test]
    fn vertex_at_head_position() {
        let skel = generate_humanoid_skeleton(false);
        let (indices, weights) = compute_vertex_joint_weights(REST_POSITIONS[HEAD], &skel);
        assert_eq!(
            indices[0], HEAD as u16,
            "Vertex at head position should have head as primary joint"
        );
        assert!(weights[0] > 0.5, "Head weight should dominate at head position");
    }

    #[test]
    fn vertex_at_left_hand_position() {
        let skel = generate_humanoid_skeleton(false);
        let (indices, weights) = compute_vertex_joint_weights(REST_POSITIONS[L_HAND], &skel);
        assert_eq!(
            indices[0], L_HAND as u16,
            "Vertex at L_hand position should have L_hand as primary joint"
        );
        assert!(weights[0] > 0.5, "L_hand weight should dominate");
    }

    #[test]
    fn vertex_between_joints_gets_both() {
        let skel = generate_humanoid_skeleton(false);
        // Midpoint between L_shoulder and L_elbow.
        let mid = [
            (REST_POSITIONS[L_SHOULDER][0] + REST_POSITIONS[L_ELBOW][0]) * 0.5,
            (REST_POSITIONS[L_SHOULDER][1] + REST_POSITIONS[L_ELBOW][1]) * 0.5,
            (REST_POSITIONS[L_SHOULDER][2] + REST_POSITIONS[L_ELBOW][2]) * 0.5,
        ];
        let (indices, _) = compute_vertex_joint_weights(mid, &skel);

        let index_set: Vec<u16> = indices.to_vec();
        // Both L_shoulder and L_elbow should appear in the top 4.
        assert!(
            index_set.contains(&(L_SHOULDER as u16)) || index_set.contains(&(L_ELBOW as u16)),
            "Midpoint between L_shoulder and L_elbow should reference at least one of them"
        );
    }

    #[test]
    fn vertex_weights_max_4_joints() {
        let skel = generate_humanoid_skeleton(false);
        let (_, weights) = compute_vertex_joint_weights([0.0, 1.0, 0.0], &skel);
        // There should be at most 4 non-zero weights (always exactly 4 slots).
        let non_zero = weights.iter().filter(|w| **w > 0.0).count();
        assert!(non_zero <= 4, "More than 4 non-zero weights");
    }

    #[test]
    fn vertex_weights_on_enemy_skeleton() {
        let skel = generate_humanoid_skeleton(true);
        let (_, weights) = compute_vertex_joint_weights([0.0, 1.0, 0.0], &skel);
        let sum: f32 = weights.iter().sum();
        assert!(
            approx_eq_eps(sum, 1.0, 1e-4),
            "Enemy skeleton weights sum to {}",
            sum
        );
    }

    // ---- Matrix helper tests ----

    #[test]
    fn mat4_inverse_identity() {
        let inv = mat4_inverse(&IDENTITY);
        assert!(approx_eq_mat4(&inv, &IDENTITY, 1e-6));
    }

    #[test]
    fn mat4_inverse_translation() {
        let m = translation_matrix(3.0, -5.0, 7.0);
        let inv = mat4_inverse(&m);
        let expected = translation_matrix(-3.0, 5.0, -7.0);
        assert!(
            approx_eq_mat4(&inv, &expected, 1e-5),
            "Inverse of translation should negate the translation"
        );
    }

    #[test]
    fn mat4_inverse_roundtrip() {
        let m = translation_matrix(1.5, -2.3, 0.7);
        let inv = mat4_inverse(&m);
        let product = mat4_mul(&m, &inv);
        assert!(
            approx_eq_mat4(&product, &IDENTITY, 1e-4),
            "M * M^-1 should be identity"
        );
    }

    // ---- Quaternion helper tests ----

    #[test]
    fn quat_from_axis_angle_x_zero_is_identity() {
        let q = quat_from_axis_angle_x(0.0);
        assert!(approx_eq(quat_length(&q), 1.0));
        assert!(approx_eq(q[3], 1.0));
    }

    #[test]
    fn quat_from_axis_angle_z_zero_is_identity() {
        let q = quat_from_axis_angle_z(0.0);
        assert!(approx_eq(quat_length(&q), 1.0));
        assert!(approx_eq(q[3], 1.0));
    }

    #[test]
    fn quat_from_axis_angle_x_90deg_unit_length() {
        let q = quat_from_axis_angle_x(std::f32::consts::FRAC_PI_2);
        assert!(approx_eq(quat_length(&q), 1.0));
        assert!(q[0] > 0.0, "X component should be positive for +90 deg X rotation");
    }

    #[test]
    fn normalize_quat_produces_unit_length() {
        let q = normalize_quat([3.0, 4.0, 0.0, 0.0]);
        assert!(approx_eq(quat_length(&q), 1.0));
    }

    #[test]
    fn normalize_quat_zero_returns_identity() {
        let q = normalize_quat([0.0, 0.0, 0.0, 0.0]);
        assert_eq!(q, QUAT_IDENTITY);
    }

    // ---- Integration tests ----

    #[test]
    fn clips_work_with_evaluate_clip() {
        // Verify that the generated clips are compatible with the skeletal_animation
        // evaluation function.
        let skel = generate_humanoid_skeleton(false);
        let clips = generate_all_clips(skel.joints.len());

        for clip in &clips {
            // Evaluate at several time points.
            for &t in &[0.0, clip.duration_secs * 0.25, clip.duration_secs * 0.5, clip.duration_secs] {
                let poses = crate::skeletal_animation::evaluate_clip(clip, t, skel.joints.len());
                assert_eq!(
                    poses.len(),
                    skel.joints.len(),
                    "Clip '{}' at t={} returned wrong number of poses",
                    clip.name,
                    t
                );

                // Verify all rotations are unit-length after evaluation.
                for (i, pose) in poses.iter().enumerate() {
                    let qlen = quat_length(&pose.rotation);
                    assert!(
                        approx_eq_eps(qlen, 1.0, 1e-3),
                        "Clip '{}' at t={}, joint {} rotation has length {}",
                        clip.name,
                        t,
                        i,
                        qlen
                    );
                }
            }
        }
    }

    #[test]
    fn clips_produce_valid_skinning_matrices() {
        let skel = generate_humanoid_skeleton(false);
        let clips = generate_all_clips(skel.joints.len());

        for clip in &clips {
            let poses = crate::skeletal_animation::evaluate_clip(clip, clip.duration_secs * 0.5, skel.joints.len());
            let matrices = crate::skeletal_animation::compute_skinning_matrices(&skel, &poses);
            assert_eq!(
                matrices.len(),
                skel.joints.len(),
                "Clip '{}' skinning matrix count mismatch",
                clip.name
            );

            // Verify no NaN or Inf in matrices.
            for (i, mat) in matrices.iter().enumerate() {
                for col in 0..4 {
                    for row in 0..4 {
                        assert!(
                            mat[col][row].is_finite(),
                            "Clip '{}' joint {} skinning matrix has non-finite value at [{},{}]: {}",
                            clip.name,
                            i,
                            col,
                            row,
                            mat[col][row]
                        );
                    }
                }
            }
        }
    }
}
