#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq)]
#[cfg_attr(target_arch = "wasm32", derive(bytemuck::Pod, bytemuck::Zeroable))]
pub struct Vertex3D {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub joint_indices: [u16; 4],
    pub joint_weights: [f32; 4],
}

impl Vertex3D {
    pub const ATTRIBS: [VertexAttribute; 5] = [
        VertexAttribute {
            offset: 0,
            shader_location: 0,
            format: VertexFormat::Float32x3,
        },
        VertexAttribute {
            offset: 12,
            shader_location: 1,
            format: VertexFormat::Float32x3,
        },
        VertexAttribute {
            offset: 24,
            shader_location: 2,
            format: VertexFormat::Float32x2,
        },
        VertexAttribute {
            offset: 32,
            shader_location: 3,
            format: VertexFormat::Uint16x4,
        },
        VertexAttribute {
            offset: 40,
            shader_location: 4,
            format: VertexFormat::Float32x4,
        },
    ];
}

/// Simplified vertex attribute descriptor for non-wasm builds.
#[derive(Copy, Clone, Debug)]
pub struct VertexAttribute {
    pub offset: u64,
    pub shader_location: u32,
    pub format: VertexFormat,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum VertexFormat {
    Float32x2,
    Float32x3,
    Float32x4,
    Uint16x4,
}

pub struct MeshData {
    pub vertices: Vec<Vertex3D>,
    pub indices: Vec<u32>,
    pub material: crate::material::MaterialData,
}

pub type JointMatrix = [[f32; 4]; 4];
pub const MAX_SKINNING_JOINTS: usize = 256;

fn identity_joint_matrix() -> JointMatrix {
    [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

pub fn flatten_skinning_palette_for_upload(
    joint_palette: &[JointMatrix],
    max_joints: usize,
) -> Result<Vec<JointMatrix>, String> {
    if max_joints == 0 {
        return Err("max joint upload bound must be at least 1".to_string());
    }
    if joint_palette.len() > max_joints {
        return Err(format!(
            "pose upload joint count {} exceeds max {}",
            joint_palette.len(),
            max_joints
        ));
    }

    let mut flattened = vec![identity_joint_matrix(); max_joints];
    for (joint_index, matrix) in joint_palette.iter().enumerate() {
        flattened[joint_index] = *matrix;
    }
    Ok(flattened)
}

#[cfg(target_arch = "wasm32")]
pub struct GpuMesh {
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub index_count: u32,
    pub index_format: wgpu::IndexFormat,
}

/// Build an ORM (Occlusion/Roughness/Metallic) packed texture from separate
/// metallic-roughness and occlusion textures.
///
/// Channel packing: R = AO, G = roughness, B = metallic, A = 1.0
fn build_orm_texture(
    metallic_roughness: Option<&crate::material::TextureImage>,
    occlusion: Option<&crate::material::TextureImage>,
) -> Option<crate::material::TextureImage> {
    // If we have metallic-roughness, use its dimensions; otherwise use occlusion dims
    let (width, height) = if let Some(mr) = metallic_roughness {
        (mr.width, mr.height)
    } else if let Some(ao) = occlusion {
        (ao.width, ao.height)
    } else {
        return None;
    };

    let pixel_count = (width * height) as usize;
    let mut rgba_data = Vec::with_capacity(pixel_count * 4);

    for i in 0..pixel_count {
        // glTF metallicRoughnessTexture: G = roughness, B = metallic
        let (roughness, metallic) = if let Some(mr) = metallic_roughness {
            let base = i * 4;
            if base + 2 < mr.rgba_data.len() {
                (mr.rgba_data[base + 1], mr.rgba_data[base + 2])
            } else {
                (128, 0) // default: roughness=0.5, metallic=0.0
            }
        } else {
            (128, 0)
        };

        let ao = if let Some(occ) = occlusion {
            let base = i * 4;
            if base < occ.rgba_data.len() {
                occ.rgba_data[base] // R channel = AO
            } else {
                255
            }
        } else {
            255 // default: full AO
        };

        rgba_data.push(ao);
        rgba_data.push(roughness);
        rgba_data.push(metallic);
        rgba_data.push(255);
    }

    Some(crate::material::TextureImage {
        width,
        height,
        rgba_data,
    })
}

pub struct GlbData {
    pub meshes: Vec<MeshData>,
    pub skeleton: Option<crate::skeletal_animation::Skeleton>,
    pub animation_clips: Vec<crate::skeletal_animation::AnimationClip>,
}

pub fn load_glb_with_animations(data: &[u8]) -> Result<GlbData, String> {
    let meshes = load_glb(data)?;
    let (skeleton, animation_clips) =
        crate::skeletal_animation::load_animations_from_glb(data)?;
    Ok(GlbData {
        meshes,
        skeleton,
        animation_clips,
    })
}

pub fn load_glb(data: &[u8]) -> Result<Vec<MeshData>, String> {
    let glb = gltf::Glb::from_slice(data).map_err(|e| format!("GLB parse error: {e}"))?;
    let gltf_doc =
        gltf::Gltf::from_slice(&glb.json).map_err(|e| format!("glTF JSON error: {e}"))?;
    let bin: &[u8] = glb.bin.as_deref().ok_or("No binary blob in GLB")?;

    // Pre-decode all images referenced in the glTF document.
    let images: Vec<Option<crate::material::TextureImage>> = {
        let mut decoded_images = Vec::new();
        for image in gltf_doc.images() {
            match image.source() {
                gltf::image::Source::View { view, mime_type } => {
                    let start = view.offset();
                    let end = start + view.length();
                    if end <= bin.len() {
                        let image_bytes = &bin[start..end];
                        let decoded = decode_image_bytes(image_bytes, mime_type);
                        decoded_images.push(decoded);
                    } else {
                        decoded_images.push(None);
                    }
                }
                gltf::image::Source::Uri { .. } => {
                    // URI-based images not supported in GLB embedded mode
                    decoded_images.push(None);
                }
            }
        }
        decoded_images
    };

    let mut meshes = Vec::new();

    for mesh in gltf_doc.meshes() {
        for primitive in mesh.primitives() {
            let reader =
                primitive.reader(|_buffer: gltf::Buffer<'_>| -> Option<&[u8]> { Some(bin) });

            let positions: Vec<[f32; 3]> = reader
                .read_positions()
                .ok_or("Missing positions")?
                .collect();

            let normals: Vec<[f32; 3]> = match reader.read_normals() {
                Some(iter) => iter.collect(),
                None => vec![[0.0, 1.0, 0.0]; positions.len()],
            };

            let uvs: Vec<[f32; 2]> = match reader.read_tex_coords(0) {
                Some(tc) => tc.into_f32().collect(),
                None => vec![[0.0, 0.0]; positions.len()],
            };

            let joints: Vec<[u16; 4]> = match reader.read_joints(0) {
                Some(joints) => joints.into_u16().collect(),
                None => vec![[0, 0, 0, 0]; positions.len()],
            };

            let weights: Vec<[f32; 4]> = match reader.read_weights(0) {
                Some(weights) => weights.into_f32().collect(),
                None => vec![[0.0, 0.0, 0.0, 0.0]; positions.len()],
            };

            let vertices: Vec<Vertex3D> = positions
                .iter()
                .enumerate()
                .map(|(i, pos)| Vertex3D {
                    position: *pos,
                    normal: normals.get(i).copied().unwrap_or([0.0, 1.0, 0.0]),
                    uv: uvs.get(i).copied().unwrap_or([0.0, 0.0]),
                    joint_indices: joints.get(i).copied().unwrap_or([0, 0, 0, 0]),
                    joint_weights: weights.get(i).copied().unwrap_or([1.0, 0.0, 0.0, 0.0]),
                })
                .collect();

            let indices: Vec<u32> = match reader.read_indices() {
                Some(idx) => idx.into_u32().collect(),
                None => (0..vertices.len() as u32).collect(),
            };

            // Extract material data
            let material_data = extract_material_data(&primitive, &images);

            meshes.push(MeshData {
                vertices,
                indices,
                material: material_data,
            });
        }
    }
    Ok(meshes)
}

/// Decode raw image bytes (PNG/JPEG) to RGBA8 pixel data using the `image` crate.
fn decode_image_bytes(
    image_bytes: &[u8],
    _mime_type: &str,
) -> Option<crate::material::TextureImage> {
    let img = image::load_from_memory(image_bytes).ok()?;
    let rgba = img.to_rgba8();
    let (w, h) = (rgba.width(), rgba.height());
    Some(crate::material::TextureImage {
        width: w,
        height: h,
        rgba_data: rgba.into_raw(),
    })
}

/// Extract material data from a glTF primitive.
fn extract_material_data(
    primitive: &gltf::Primitive<'_>,
    images: &[Option<crate::material::TextureImage>],
) -> crate::material::MaterialData {
    use crate::material::{MaterialData, MaterialUniforms};

    let mat = primitive.material();
    let pbr = mat.pbr_metallic_roughness();

    let base_color_factor = pbr.base_color_factor();
    let metallic_factor = pbr.metallic_factor();
    let roughness_factor = pbr.roughness_factor();

    let uniforms = MaterialUniforms {
        base_color_factor,
        metallic_factor,
        roughness_factor,
        _padding: [0.0; 2],
    };

    // Extract albedo (base color) texture
    let albedo_image = pbr
        .base_color_texture()
        .and_then(|info| {
            let tex = info.texture();
            let img_index = tex.source().index();
            images.get(img_index).and_then(|opt| opt.as_ref())
        })
        .map(|img| crate::material::TextureImage {
            width: img.width,
            height: img.height,
            rgba_data: img.rgba_data.clone(),
        });

    // Extract normal map texture
    let normal_image = mat
        .normal_texture()
        .and_then(|info| {
            let tex = info.texture();
            let img_index = tex.source().index();
            images.get(img_index).and_then(|opt| opt.as_ref())
        })
        .map(|img| crate::material::TextureImage {
            width: img.width,
            height: img.height,
            rgba_data: img.rgba_data.clone(),
        });

    // Extract metallic-roughness texture
    let mr_image = pbr
        .metallic_roughness_texture()
        .and_then(|info| {
            let tex = info.texture();
            let img_index = tex.source().index();
            images.get(img_index).and_then(|opt| opt.as_ref())
        });

    // Extract occlusion texture
    let occ_image = mat
        .occlusion_texture()
        .and_then(|info| {
            let tex = info.texture();
            let img_index = tex.source().index();
            images.get(img_index).and_then(|opt| opt.as_ref())
        });

    // Build ORM packed texture from metallic-roughness + occlusion
    let orm_image = build_orm_texture(mr_image, occ_image);

    MaterialData {
        albedo_image,
        normal_image,
        orm_image,
        uniforms,
    }
}

#[cfg(target_arch = "wasm32")]
pub fn upload_mesh(device: &wgpu::Device, mesh: &MeshData) -> GpuMesh {
    use wgpu::util::DeviceExt;
    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Vertex Buffer"),
        contents: bytemuck::cast_slice(&mesh.vertices),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Index Buffer"),
        contents: bytemuck::cast_slice(&mesh.indices),
        usage: wgpu::BufferUsages::INDEX,
    });
    GpuMesh {
        vertex_buffer,
        index_buffer,
        index_count: mesh.indices.len() as u32,
        index_format: wgpu::IndexFormat::Uint32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_minimal_glb() -> Vec<u8> {
        // Build a minimal valid GLB with one triangle.
        // GLB = Header(12) + JSON chunk + BIN chunk
        //
        // The JSON describes a single mesh with one primitive that has
        // a positions accessor pointing into the BIN chunk.

        // BIN: 3 positions (3 x vec3<f32> = 36 bytes) + 3 indices (3 x u16 = 6 bytes, padded to 8)
        let positions: [[f32; 3]; 3] = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let indices: [u16; 3] = [0, 1, 2];

        let mut bin_data = Vec::new();
        for pos in &positions {
            for f in pos {
                bin_data.extend_from_slice(&f.to_le_bytes());
            }
        }
        for idx in &indices {
            bin_data.extend_from_slice(&idx.to_le_bytes());
        }
        // Pad to 4-byte alignment
        while bin_data.len() % 4 != 0 {
            bin_data.push(0);
        }

        let json_str = serde_json::json!({
            "asset": { "version": "2.0" },
            "buffers": [{ "byteLength": bin_data.len() }],
            "bufferViews": [
                { "buffer": 0, "byteOffset": 0, "byteLength": 36, "target": 34962 },
                { "buffer": 0, "byteOffset": 36, "byteLength": 6, "target": 34963 }
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
                    "componentType": 5123,
                    "count": 3,
                    "type": "SCALAR"
                }
            ],
            "meshes": [{
                "primitives": [{
                    "attributes": { "POSITION": 0 },
                    "indices": 1
                }]
            }]
        })
        .to_string();

        // Pad JSON to 4-byte alignment
        let mut json_bytes = json_str.into_bytes();
        while json_bytes.len() % 4 != 0 {
            json_bytes.push(b' ');
        }

        let total_length = 12 + 8 + json_bytes.len() + 8 + bin_data.len();

        let mut glb = Vec::with_capacity(total_length);
        // GLB header
        glb.extend_from_slice(b"glTF"); // magic
        glb.extend_from_slice(&2u32.to_le_bytes()); // version
        glb.extend_from_slice(&(total_length as u32).to_le_bytes()); // length

        // JSON chunk
        glb.extend_from_slice(&(json_bytes.len() as u32).to_le_bytes()); // chunk length
        glb.extend_from_slice(&0x4E4F534Au32.to_le_bytes()); // chunk type "JSON"
        glb.extend_from_slice(&json_bytes);

        // BIN chunk
        glb.extend_from_slice(&(bin_data.len() as u32).to_le_bytes()); // chunk length
        glb.extend_from_slice(&0x004E4942u32.to_le_bytes()); // chunk type "BIN\0"
        glb.extend_from_slice(&bin_data);

        glb
    }

    #[test]
    fn load_glb_parses_minimal_triangle() {
        let glb = make_minimal_glb();
        let meshes = load_glb(&glb).expect("should parse minimal GLB");
        assert_eq!(meshes.len(), 1);
        let mesh = &meshes[0];
        assert_eq!(mesh.vertices.len(), 3);
        assert_eq!(mesh.indices.len(), 3);
        assert_eq!(mesh.vertices[0].position, [0.0, 0.0, 0.0]);
        assert_eq!(mesh.vertices[1].position, [1.0, 0.0, 0.0]);
        assert_eq!(mesh.vertices[2].position, [0.0, 1.0, 0.0]);
        // Default normals when not present
        assert_eq!(mesh.vertices[0].normal, [0.0, 1.0, 0.0]);
        // Default UVs when not present
        assert_eq!(mesh.vertices[0].uv, [0.0, 0.0]);
        // Indices should match
        assert_eq!(mesh.indices, vec![0, 1, 2]);
    }

    #[test]
    fn load_glb_rejects_empty_data() {
        let result = load_glb(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn load_glb_rejects_invalid_data() {
        let result = load_glb(b"not a valid glb file");
        assert!(result.is_err());
    }

    #[test]
    fn vertex3d_layout_is_56_bytes() {
        assert_eq!(std::mem::size_of::<Vertex3D>(), 56);
    }

    #[test]
    fn skinned_vertex_layout_contract() {
        assert_eq!(std::mem::size_of::<Vertex3D>(), 56);
        assert_eq!(Vertex3D::ATTRIBS.len(), 5);
        assert_eq!(Vertex3D::ATTRIBS[0].offset, 0);
        assert_eq!(Vertex3D::ATTRIBS[0].shader_location, 0);
        assert_eq!(Vertex3D::ATTRIBS[0].format, VertexFormat::Float32x3);

        assert_eq!(Vertex3D::ATTRIBS[1].offset, 12);
        assert_eq!(Vertex3D::ATTRIBS[1].shader_location, 1);
        assert_eq!(Vertex3D::ATTRIBS[1].format, VertexFormat::Float32x3);

        assert_eq!(Vertex3D::ATTRIBS[2].offset, 24);
        assert_eq!(Vertex3D::ATTRIBS[2].shader_location, 2);
        assert_eq!(Vertex3D::ATTRIBS[2].format, VertexFormat::Float32x2);

        assert_eq!(Vertex3D::ATTRIBS[3].offset, 32);
        assert_eq!(Vertex3D::ATTRIBS[3].shader_location, 3);
        assert_eq!(Vertex3D::ATTRIBS[3].format, VertexFormat::Uint16x4);

        assert_eq!(Vertex3D::ATTRIBS[4].offset, 40);
        assert_eq!(Vertex3D::ATTRIBS[4].shader_location, 4);
        assert_eq!(Vertex3D::ATTRIBS[4].format, VertexFormat::Float32x4);
    }

    #[test]
    fn load_glb_generates_sequential_indices_when_none_provided() {
        // Build a GLB without an indices accessor
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

        let mut json_bytes = json_str.into_bytes();
        while json_bytes.len() % 4 != 0 {
            json_bytes.push(b' ');
        }

        let total_length = 12 + 8 + json_bytes.len() + 8 + bin_data.len();
        let mut glb = Vec::with_capacity(total_length);
        glb.extend_from_slice(b"glTF");
        glb.extend_from_slice(&2u32.to_le_bytes());
        glb.extend_from_slice(&(total_length as u32).to_le_bytes());
        glb.extend_from_slice(&(json_bytes.len() as u32).to_le_bytes());
        glb.extend_from_slice(&0x4E4F534Au32.to_le_bytes());
        glb.extend_from_slice(&json_bytes);
        glb.extend_from_slice(&(bin_data.len() as u32).to_le_bytes());
        glb.extend_from_slice(&0x004E4942u32.to_le_bytes());
        glb.extend_from_slice(&bin_data);

        let meshes = load_glb(&glb).expect("should parse GLB without indices");
        assert_eq!(meshes.len(), 1);
        assert_eq!(meshes[0].indices, vec![0, 1, 2]);
    }

    #[test]
    fn skinning_palette_upload_roundtrip() {
        let joints = vec![
            [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.5, 0.0, 0.0, 1.0],
            ],
            [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.25, 0.0, 1.0],
            ],
        ];

        let flattened = flatten_skinning_palette_for_upload(&joints, 4)
            .expect("flattening skinning palette should pass");
        assert_eq!(flattened.len(), 4);
        assert_eq!(flattened[0], joints[0]);
        assert_eq!(flattened[1], joints[1]);
        assert_eq!(
            flattened[2],
            [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ]
        );

        let error = flatten_skinning_palette_for_upload(&joints, 1)
            .expect_err("joint overflow must fail upload validation");
        assert!(error.contains("exceeds max 1"));
    }
}
