use sha2::{Digest, Sha256};

pub fn deterministic_mesh_hash(vertices: &[[i32; 3]], triangles: &[[u32; 3]]) -> String {
    let mut hasher = Sha256::new();
    hasher.update((vertices.len() as u64).to_le_bytes());
    for vertex in vertices {
        for coord in vertex {
            hasher.update(coord.to_le_bytes());
        }
    }

    hasher.update((triangles.len() as u64).to_le_bytes());
    for tri in triangles {
        for index in tri {
            hasher.update(index.to_le_bytes());
        }
    }

    let digest = hasher.finalize();
    let mut out = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(&mut out, "{byte:02x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::deterministic_mesh_hash as compute_mesh_hash;

    #[test]
    fn deterministic_mesh_hash() {
        let vertices = [[0, 0, 0], [100, 0, 0], [0, 100, 0]];
        let triangles = [[0, 1, 2]];

        let hash_a = compute_mesh_hash(&vertices, &triangles);
        let hash_b = compute_mesh_hash(&vertices, &triangles);

        assert_eq!(hash_a, hash_b, "same mesh data must hash identically");

        let changed_vertices = [[0, 0, 0], [101, 0, 0], [0, 100, 0]];
        let hash_c = compute_mesh_hash(&changed_vertices, &triangles);
        assert_ne!(hash_a, hash_c, "hash must change when geometry changes");
    }
}
