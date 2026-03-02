const FNV_OFFSET_BASIS_64: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME_64: u64 = 0x0000_0100_0000_01b3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JointPose {
    pub joint_id: u16,
    pub rotation_q15: [i16; 4],
    pub translation_mm: [i32; 3],
    pub scale_q10: [u16; 3],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoseFrame {
    pub rig_revision: u32,
    pub tick: u64,
    pub phase_q16: i32,
    pub joints: Vec<JointPose>,
}

fn fnv1a64_bytes(mut hash: u64, bytes: &[u8]) -> u64 {
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME_64);
    }
    hash
}

fn fnv1a64_u16(hash: u64, value: u16) -> u64 {
    fnv1a64_bytes(hash, &value.to_le_bytes())
}

fn fnv1a64_i16(hash: u64, value: i16) -> u64 {
    fnv1a64_bytes(hash, &value.to_le_bytes())
}

fn fnv1a64_u32(hash: u64, value: u32) -> u64 {
    fnv1a64_bytes(hash, &value.to_le_bytes())
}

fn fnv1a64_i32(hash: u64, value: i32) -> u64 {
    fnv1a64_bytes(hash, &value.to_le_bytes())
}

fn fnv1a64_u64(hash: u64, value: u64) -> u64 {
    fnv1a64_bytes(hash, &value.to_le_bytes())
}

pub fn pose_hash(frame: &PoseFrame) -> u64 {
    let mut joints = frame.joints.clone();
    joints.sort_by_key(|joint| joint.joint_id);

    let mut hash = FNV_OFFSET_BASIS_64;
    hash = fnv1a64_u32(hash, frame.rig_revision);
    hash = fnv1a64_u64(hash, frame.tick);
    hash = fnv1a64_i32(hash, frame.phase_q16);
    hash = fnv1a64_u32(hash, joints.len().min(u32::MAX as usize) as u32);

    for joint in &joints {
        hash = fnv1a64_u16(hash, joint.joint_id);
        for component in joint.rotation_q15 {
            hash = fnv1a64_i16(hash, component);
        }
        for component in joint.translation_mm {
            hash = fnv1a64_i32(hash, component);
        }
        for component in joint.scale_q10 {
            hash = fnv1a64_u16(hash, component);
        }
    }

    hash
}

#[cfg(test)]
mod tests {
    use super::{JointPose, PoseFrame, pose_hash};

    #[test]
    fn pose_hash_stable() {
        let frame_a = PoseFrame {
            rig_revision: 7,
            tick: 144,
            phase_q16: 12_288,
            joints: vec![
                JointPose {
                    joint_id: 5,
                    rotation_q15: [16384, 0, -4096, 8192],
                    translation_mm: [0, 1260, -220],
                    scale_q10: [1024, 1024, 1024],
                },
                JointPose {
                    joint_id: 1,
                    rotation_q15: [12000, 1024, 2048, 4096],
                    translation_mm: [200, 30, -5],
                    scale_q10: [1020, 1024, 1029],
                },
            ],
        };
        let frame_b = PoseFrame {
            joints: vec![frame_a.joints[1], frame_a.joints[0]],
            ..frame_a.clone()
        };

        let hash = pose_hash(&frame_a);
        for _ in 0..64 {
            assert_eq!(pose_hash(&frame_a), hash);
            assert_eq!(pose_hash(&frame_b), hash);
        }
    }
}
