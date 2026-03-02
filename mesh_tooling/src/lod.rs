use crate::types::{LodBounds, LodChain, LodLevel, LodLineage, MeshAsset};

const LOD_CHAIN_SCHEMA_VERSION: u32 = 2;
const LOD_CHAIN_KIND: &str = "lod-chain-v2";
const LOD_GENERATOR: &str = "wrela.mesh.lod.v2";

fn deterministic_hash(parts: &[&[u8]]) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for part in parts {
        for byte in *part {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash ^= 0xff;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

fn mesh_bounds(mesh: &MeshAsset) -> Result<LodBounds, String> {
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    let mut found_vertex = false;
    for surface in &mesh.surfaces {
        for vertex in &surface.vertices {
            found_vertex = true;
            for axis in 0..3 {
                min[axis] = min[axis].min(vertex.position[axis]);
                max[axis] = max[axis].max(vertex.position[axis]);
            }
        }
    }
    if !found_vertex {
        return Err("mesh must contain at least one vertex".to_string());
    }
    Ok(LodBounds { min, max })
}

fn mesh_source_hash(mesh: &MeshAsset) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for surface in &mesh.surfaces {
        for vertex in &surface.vertices {
            for value in vertex.position {
                for byte in value.to_bits().to_le_bytes() {
                    hash ^= u64::from(byte);
                    hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
                }
            }
        }
        for index in &surface.indices {
            for byte in index.to_le_bytes() {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
    }
    format!("{hash:016x}")
}

pub fn validate_mesh(mesh: &MeshAsset) -> Result<(), String> {
    if mesh.surfaces.is_empty() {
        return Err("mesh must contain at least one surface".to_string());
    }

    for (surface_index, surface) in mesh.surfaces.iter().enumerate() {
        if surface.indices.len() % 3 != 0 {
            return Err(format!(
                "surface {} indices length {} is not a multiple of 3",
                surface_index,
                surface.indices.len()
            ));
        }

        let vertex_count = surface.vertices.len();
        for (index_position, &vertex_index) in surface.indices.iter().enumerate() {
            if vertex_index as usize >= vertex_count {
                return Err(format!(
                    "surface {} index {} points to vertex {} but vertex count is {}",
                    surface_index, index_position, vertex_index, vertex_count
                ));
            }
        }
    }

    Ok(())
}

pub fn generate_lod_chain(mesh: &MeshAsset, levels: u32) -> Result<LodChain, String> {
    if levels == 0 {
        return Err("levels must be greater than 0".to_string());
    }

    validate_mesh(mesh)?;

    let base_triangle_count: usize = mesh
        .surfaces
        .iter()
        .map(|surface| surface.indices.len() / 3)
        .sum();
    if base_triangle_count == 0 {
        return Err("mesh must contain at least one triangle".to_string());
    }

    let mut lod_levels = Vec::with_capacity(levels as usize);
    let mut current_triangle_count = base_triangle_count;
    let bounds = mesh_bounds(mesh)?;
    let source_hash = mesh_source_hash(mesh);

    for level in 0..levels {
        let ratio_milli =
            ((current_triangle_count as u128 * 1000) / base_triangle_count as u128) as u32;
        let source_level = level.checked_sub(1);
        let level_bytes = level.to_le_bytes();
        let triangle_bytes = (current_triangle_count as u64).to_le_bytes();
        let ratio_bytes = ratio_milli.to_le_bytes();
        let source_level_bytes = source_level.unwrap_or_default().to_le_bytes();
        let min_x_bytes = bounds.min[0].to_bits().to_le_bytes();
        let min_y_bytes = bounds.min[1].to_bits().to_le_bytes();
        let min_z_bytes = bounds.min[2].to_bits().to_le_bytes();
        let max_x_bytes = bounds.max[0].to_bits().to_le_bytes();
        let max_y_bytes = bounds.max[1].to_bits().to_le_bytes();
        let max_z_bytes = bounds.max[2].to_bits().to_le_bytes();
        let source_level_marker = [u8::from(source_level.is_some())];
        let deterministic_hash = deterministic_hash(&[
            mesh.mesh_id.as_bytes(),
            source_hash.as_bytes(),
            level_bytes.as_slice(),
            triangle_bytes.as_slice(),
            ratio_bytes.as_slice(),
            source_level_marker.as_slice(),
            source_level_bytes.as_slice(),
            min_x_bytes.as_slice(),
            min_y_bytes.as_slice(),
            min_z_bytes.as_slice(),
            max_x_bytes.as_slice(),
            max_y_bytes.as_slice(),
            max_z_bytes.as_slice(),
        ]);

        lod_levels.push(LodLevel {
            level,
            triangle_count: current_triangle_count,
            ratio_milli,
            source_level,
            bounds: bounds.clone(),
            deterministic_hash,
        });

        if level + 1 < levels {
            current_triangle_count = (current_triangle_count / 2).max(1);
        }
    }

    Ok(LodChain {
        schema_version: LOD_CHAIN_SCHEMA_VERSION,
        kind: LOD_CHAIN_KIND.to_string(),
        mesh_id: mesh.mesh_id.clone(),
        lineage: LodLineage {
            source_mesh_id: mesh.mesh_id.clone(),
            source_hash,
            generator: LOD_GENERATOR.to_string(),
        },
        levels: lod_levels,
    })
}

#[cfg(test)]
mod tests {
    use super::{generate_lod_chain, validate_mesh};
    use crate::types::{MeshAsset, MeshSurface, MeshVertex};

    fn vertex() -> MeshVertex {
        MeshVertex {
            position: [0.0, 0.0, 0.0],
            normal: [0.0, 1.0, 0.0],
            uv: [0.0, 0.0],
        }
    }

    fn sample_mesh() -> MeshAsset {
        MeshAsset {
            schema_version: 1,
            kind: "mesh_asset".to_string(),
            mesh_id: "mesh_001".to_string(),
            surfaces: vec![MeshSurface {
                id: "surface_001".to_string(),
                material_id: "mat_001".to_string(),
                vertices: vec![vertex(), vertex(), vertex(), vertex()],
                indices: vec![
                    0, 1, 2, 0, 2, 3, 0, 1, 2, 0, 2, 3, 0, 1, 2, 0, 2, 3, 0, 1, 2, 0, 2, 3,
                ],
            }],
        }
    }

    #[test]
    fn validate_mesh_fails_when_no_surfaces_exist() {
        let mesh = MeshAsset {
            schema_version: 1,
            kind: "mesh_asset".to_string(),
            mesh_id: "mesh_empty".to_string(),
            surfaces: Vec::new(),
        };

        let err = validate_mesh(&mesh).expect_err("validation should fail");
        assert!(err.contains("at least one surface"));
    }

    #[test]
    fn validate_mesh_fails_when_indices_are_not_triangles() {
        let mut mesh = sample_mesh();
        mesh.surfaces[0].indices = vec![0, 1];

        let err = validate_mesh(&mesh).expect_err("validation should fail");
        assert!(err.contains("multiple of 3"));
    }

    #[test]
    fn validate_mesh_fails_when_index_is_out_of_range() {
        let mut mesh = sample_mesh();
        mesh.surfaces[0].indices = vec![0, 1, 99];

        let err = validate_mesh(&mesh).expect_err("validation should fail");
        assert!(err.contains("points to vertex"));
    }

    #[test]
    fn generate_lod_chain_is_deterministic() {
        let mesh = sample_mesh();

        let chain_a = generate_lod_chain(&mesh, 5).expect("lod generation should succeed");
        let chain_b = generate_lod_chain(&mesh, 5).expect("lod generation should succeed");

        assert_eq!(chain_a, chain_b);
        assert_eq!(chain_a.schema_version, 2);
        assert_eq!(chain_a.kind, "lod-chain-v2");
        assert_eq!(chain_a.mesh_id, "mesh_001");
        assert_eq!(chain_a.lineage.source_mesh_id, "mesh_001");
        assert!(!chain_a.lineage.source_hash.is_empty());
        assert_eq!(chain_a.lineage.generator, "wrela.mesh.lod.v2");
        assert_eq!(chain_a.levels[0].source_level, None);
        assert_eq!(chain_a.levels[1].source_level, Some(0));
        assert!(
            chain_a
                .levels
                .iter()
                .all(|level| !level.deterministic_hash.is_empty())
        );
        assert!(
            chain_a.levels[0].bounds.min[0] <= chain_a.levels[0].bounds.max[0],
            "lod bounds min should be <= max"
        );

        let counts: Vec<usize> = chain_a
            .levels
            .iter()
            .map(|level| level.triangle_count)
            .collect();
        assert_eq!(counts, vec![8, 4, 2, 1, 1]);

        let ratios: Vec<u32> = chain_a
            .levels
            .iter()
            .map(|level| level.ratio_milli)
            .collect();
        assert_eq!(ratios, vec![1000, 500, 250, 125, 125]);
    }
}
