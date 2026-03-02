use crate::animation::synth::deterministic_seed;

pub fn macro_transform(seed: u64, pose_macro: &[[i16; 4]]) -> Vec<[i16; 4]> {
    let mut state = seed;
    let mut out = Vec::with_capacity(pose_macro.len());

    for row in pose_macro {
        let mut transformed = [0_i16; 4];
        for (index, value) in row.iter().enumerate() {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let jitter = ((state >> 61) as i16) - 3;
            transformed[index] = value.saturating_add(jitter);
        }
        out.push(transformed);
    }

    out
}

pub fn macro_transform_hash(seed_namespace: &str, pose_macro: &[[i16; 4]]) -> u64 {
    let seed = deterministic_seed(seed_namespace, &["macro_transform", "v1"]);
    let transformed = macro_transform(seed, pose_macro);

    let mut acc = 0_u64;
    for row in transformed {
        for value in row {
            acc = acc.rotate_left(5) ^ ((value as i64 as u64).wrapping_mul(0x9E37_79B1));
        }
    }
    acc
}

pub fn class_signature_vector() -> [u16; 3] {
    [55, 26, 63]
}

#[cfg(test)]
mod tests {
    use super::macro_transform_hash;

    #[test]
    fn macro_transform_stability() {
        let macro_pose = [[4, 9, -3, 10], [8, 2, 6, -4], [0, 1, 3, 5]];

        let hash_a = macro_transform_hash("ancient", &macro_pose);
        let hash_b = macro_transform_hash("ancient", &macro_pose);
        let hash_c = macro_transform_hash("ancient_alt", &macro_pose);

        assert_eq!(
            hash_a, hash_b,
            "transform hash must remain stable for same seed namespace"
        );
        assert_ne!(
            hash_a, hash_c,
            "transform hash must change with seed namespace changes"
        );
    }
}
