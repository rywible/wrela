#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

/// Calculate the number of mip levels for a texture of the given dimensions.
pub fn calculate_mip_count(width: u32, height: u32) -> u32 {
    (width.max(height) as f32).log2().floor() as u32 + 1
}

/// Generate mipmap data via CPU-side box-filter downsampling.
/// Returns a Vec of (width, height, rgba_data) for each mip level starting from mip 1
/// (mip 0 is the original data).
pub fn generate_mip_data(width: u32, height: u32, rgba_data: &[u8]) -> Vec<(u32, u32, Vec<u8>)> {
    let mip_count = calculate_mip_count(width, height);
    let mut mips = Vec::new();
    let mut prev_width = width;
    let mut prev_height = height;
    let mut prev_data = rgba_data.to_vec();

    for _ in 1..mip_count {
        let new_width = (prev_width / 2).max(1);
        let new_height = (prev_height / 2).max(1);
        let mut new_data = Vec::with_capacity((new_width * new_height * 4) as usize);

        for y in 0..new_height {
            for x in 0..new_width {
                // Sample up to a 2x2 block from the previous mip level
                let sx = (x * 2).min(prev_width - 1);
                let sy = (y * 2).min(prev_height - 1);
                let sx1 = (sx + 1).min(prev_width - 1);
                let sy1 = (sy + 1).min(prev_height - 1);

                let idx00 = ((sy * prev_width + sx) * 4) as usize;
                let idx10 = ((sy * prev_width + sx1) * 4) as usize;
                let idx01 = ((sy1 * prev_width + sx) * 4) as usize;
                let idx11 = ((sy1 * prev_width + sx1) * 4) as usize;

                // NOTE: This box filter averages in gamma (byte) space, not
                // linear space. For sRGB albedo textures this introduces a
                // slight darkening bias at lower mip levels. The visual impact
                // is modest at 512x512 with anisotropic filtering. A correct
                // implementation would convert sRGB→linear before averaging
                // and linear→sRGB after.
                for c in 0..4 {
                    let sum = prev_data[idx00 + c] as u32
                        + prev_data[idx10 + c] as u32
                        + prev_data[idx01 + c] as u32
                        + prev_data[idx11 + c] as u32;
                    new_data.push((sum / 4) as u8);
                }
            }
        }

        mips.push((new_width, new_height, new_data.clone()));
        prev_width = new_width;
        prev_height = new_height;
        prev_data = new_data;
    }

    mips
}

/// Material uniform data sent to the GPU as a uniform buffer.
/// Matches the WGSL `MaterialUniforms` struct.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
#[cfg_attr(target_arch = "wasm32", derive(bytemuck::Pod, bytemuck::Zeroable))]
pub struct MaterialUniforms {
    pub base_color_factor: [f32; 4],
    pub metallic_factor: f32,
    pub roughness_factor: f32,
    pub _padding: [f32; 2],
}

impl Default for MaterialUniforms {
    fn default() -> Self {
        Self {
            base_color_factor: [1.0, 1.0, 1.0, 1.0],
            metallic_factor: 0.0,
            roughness_factor: 0.5,
            _padding: [0.0; 2],
        }
    }
}

/// CPU-side material data extracted from glTF, before GPU upload.
pub struct MaterialData {
    /// RGBA8 pixels for albedo (base color) texture. None means use default white.
    pub albedo_image: Option<TextureImage>,
    /// RGBA8 pixels for normal map. None means use default flat normal.
    pub normal_image: Option<TextureImage>,
    /// RGBA8 pixels for ORM (occlusion/roughness/metallic) packed texture.
    /// R = AO, G = roughness, B = metallic. None means use default (1.0, 0.5, 0.0).
    pub orm_image: Option<TextureImage>,
    /// Material factor uniforms.
    pub uniforms: MaterialUniforms,
}

impl Default for MaterialData {
    fn default() -> Self {
        Self {
            albedo_image: None,
            normal_image: None,
            orm_image: None,
            uniforms: MaterialUniforms::default(),
        }
    }
}

/// A decoded texture image in RGBA8 format, ready for GPU upload.
#[derive(Clone)]
pub struct TextureImage {
    pub width: u32,
    pub height: u32,
    pub rgba_data: Vec<u8>,
}

/// GPU-resident material with textures, sampler, uniform buffer, and bind group.
#[cfg(target_arch = "wasm32")]
pub struct GpuMaterial {
    pub bind_group: wgpu::BindGroup,
    pub _albedo_texture: wgpu::Texture,
    pub _normal_texture: wgpu::Texture,
    pub _orm_texture: wgpu::Texture,
    pub _sampler: wgpu::Sampler,
    pub _uniform_buffer: wgpu::Buffer,
}

/// Create the bind group layout for material textures (group 2).
///
/// Layout:
///   binding 0: albedo_texture   (FRAGMENT, texture_2d<f32>)
///   binding 1: normal_texture   (FRAGMENT, texture_2d<f32>)
///   binding 2: orm_texture      (FRAGMENT, texture_2d<f32>)
///   binding 3: material_sampler (FRAGMENT, sampler)
///   binding 4: material_uniforms (FRAGMENT, uniform buffer)
#[cfg(target_arch = "wasm32")]
pub fn create_material_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("wrela-3d-material-bind-group-layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 4,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: Some(
                        std::num::NonZeroU64::new(
                            std::mem::size_of::<MaterialUniforms>() as u64,
                        )
                        .expect("MaterialUniforms has non-zero size"),
                    ),
                },
                count: None,
            },
        ],
    })
}

/// Upload a single RGBA8 texture to the GPU and return the texture object.
/// When `is_srgb` is true, the texture format is `Rgba8UnormSrgb` (for albedo/color textures).
/// When `is_srgb` is false, the texture format is `Rgba8Unorm` (for normal maps, ORM, etc.).
/// Mipmaps are generated via CPU box-filter downsampling and uploaded for each mip level.
#[cfg(target_arch = "wasm32")]
fn upload_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    width: u32,
    height: u32,
    data: &[u8],
    label: &str,
    is_srgb: bool,
) -> wgpu::Texture {
    let format = if is_srgb {
        wgpu::TextureFormat::Rgba8UnormSrgb
    } else {
        wgpu::TextureFormat::Rgba8Unorm
    };
    let mip_level_count = calculate_mip_count(width, height);

    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });

    // Write mip level 0 (original data)
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        data,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4 * width),
            rows_per_image: Some(height),
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );

    // Generate and upload mip levels 1..N
    let mips = generate_mip_data(width, height, data);
    for (i, (mip_w, mip_h, mip_data)) in mips.iter().enumerate() {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: (i + 1) as u32,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            mip_data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * mip_w),
                rows_per_image: Some(*mip_h),
            },
            wgpu::Extent3d {
                width: *mip_w,
                height: *mip_h,
                depth_or_array_layers: 1,
            },
        );
    }

    texture
}

/// Create a 1x1 default texture with the given RGBA pixel value.
/// `is_srgb` controls the texture format (sRGB for color data, linear for normal/ORM).
#[cfg(target_arch = "wasm32")]
fn create_default_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pixel: [u8; 4],
    label: &str,
    is_srgb: bool,
) -> wgpu::Texture {
    upload_texture(device, queue, 1, 1, &pixel, label, is_srgb)
}

/// Create default 1x1 textures for meshes without materials.
/// Returns (albedo, normal, orm) textures.
#[cfg(target_arch = "wasm32")]
pub fn create_default_textures(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> (wgpu::Texture, wgpu::Texture, wgpu::Texture) {
    let albedo = create_default_texture(device, queue, [255, 255, 255, 255], "default-albedo", true);
    // Flat normal: (0.5, 0.5, 1.0) in [0,255] = (128, 128, 255)
    let normal = create_default_texture(device, queue, [128, 128, 255, 255], "default-normal", false);
    // ORM default: AO=1.0(255), roughness=0.5(128), metallic=0.0(0)
    let orm = create_default_texture(device, queue, [255, 128, 0, 255], "default-orm", false);
    (albedo, normal, orm)
}

/// Create the default material sampler (linear filtering, repeat wrap).
#[cfg(target_arch = "wasm32")]
pub fn create_material_sampler(device: &wgpu::Device) -> wgpu::Sampler {
    device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("wrela-material-sampler"),
        address_mode_u: wgpu::AddressMode::Repeat,
        address_mode_v: wgpu::AddressMode::Repeat,
        address_mode_w: wgpu::AddressMode::Repeat,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::FilterMode::Linear,
        anisotropy_clamp: 16,
        ..Default::default()
    })
}

/// Upload material data to the GPU and create a `GpuMaterial` with bind group.
#[cfg(target_arch = "wasm32")]
pub fn upload_material(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    material_data: &MaterialData,
) -> GpuMaterial {
    use wgpu::util::DeviceExt;

    let albedo_texture = match &material_data.albedo_image {
        Some(img) => upload_texture(
            device,
            queue,
            img.width,
            img.height,
            &img.rgba_data,
            "material-albedo",
            true, // sRGB for albedo
        ),
        None => create_default_texture(device, queue, [255, 255, 255, 255], "material-albedo-default", true),
    };

    let normal_texture = match &material_data.normal_image {
        Some(img) => upload_texture(
            device,
            queue,
            img.width,
            img.height,
            &img.rgba_data,
            "material-normal",
            false, // linear for normal maps
        ),
        None => create_default_texture(device, queue, [128, 128, 255, 255], "material-normal-default", false),
    };

    let orm_texture = match &material_data.orm_image {
        Some(img) => upload_texture(
            device,
            queue,
            img.width,
            img.height,
            &img.rgba_data,
            "material-orm",
            false, // linear for ORM
        ),
        None => create_default_texture(device, queue, [255, 128, 0, 255], "material-orm-default", false),
    };

    let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("material-uniform-buffer"),
        contents: bytemuck::bytes_of(&material_data.uniforms),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });

    let albedo_view = albedo_texture.create_view(&wgpu::TextureViewDescriptor::default());
    let normal_view = normal_texture.create_view(&wgpu::TextureViewDescriptor::default());
    let orm_view = orm_texture.create_view(&wgpu::TextureViewDescriptor::default());

    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("wrela-material-bind-group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&albedo_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&normal_view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(&orm_view),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: uniform_buffer.as_entire_binding(),
            },
        ],
    });

    GpuMaterial {
        bind_group,
        _albedo_texture: albedo_texture,
        _normal_texture: normal_texture,
        _orm_texture: orm_texture,
        _sampler: sampler.clone(),
        _uniform_buffer: uniform_buffer,
    }
}

/// Create a default `GpuMaterial` using the pre-created default textures.
#[cfg(target_arch = "wasm32")]
pub fn create_default_gpu_material(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
) -> GpuMaterial {
    upload_material(
        device,
        queue,
        layout,
        sampler,
        &MaterialData::default(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn material_uniforms_default_values() {
        let u = MaterialUniforms::default();
        assert_eq!(u.base_color_factor, [1.0, 1.0, 1.0, 1.0]);
        assert_eq!(u.metallic_factor, 0.0);
        assert_eq!(u.roughness_factor, 0.5);
    }

    #[test]
    fn material_uniforms_size_is_32_bytes() {
        // 4 floats (base_color) + 1 float (metallic) + 1 float (roughness) + 2 floats (padding) = 8 floats = 32 bytes
        assert_eq!(std::mem::size_of::<MaterialUniforms>(), 32);
    }

    #[test]
    fn material_data_default_has_no_textures() {
        let d = MaterialData::default();
        assert!(d.albedo_image.is_none());
        assert!(d.normal_image.is_none());
        assert!(d.orm_image.is_none());
    }

    #[test]
    fn texture_image_stores_rgba8() {
        let img = TextureImage {
            width: 2,
            height: 2,
            rgba_data: vec![255; 2 * 2 * 4],
        };
        assert_eq!(img.rgba_data.len(), 16);
    }

    // --- Mip count calculation tests ---

    #[test]
    fn mip_count_512x512() {
        // log2(512) = 9, so 9 + 1 = 10
        assert_eq!(calculate_mip_count(512, 512), 10);
    }

    #[test]
    fn mip_count_256x256() {
        // log2(256) = 8, so 8 + 1 = 9
        assert_eq!(calculate_mip_count(256, 256), 9);
    }

    #[test]
    fn mip_count_1x1() {
        // log2(1) = 0, so 0 + 1 = 1
        assert_eq!(calculate_mip_count(1, 1), 1);
    }

    #[test]
    fn mip_count_1024x512() {
        // max(1024, 512) = 1024, log2(1024) = 10, so 10 + 1 = 11
        assert_eq!(calculate_mip_count(1024, 512), 11);
    }

    #[test]
    fn mip_count_2x2() {
        // log2(2) = 1, so 1 + 1 = 2
        assert_eq!(calculate_mip_count(2, 2), 2);
    }

    #[test]
    fn mip_count_non_power_of_two() {
        // 300: log2(300) = 8.22..., floor = 8, so 8 + 1 = 9
        assert_eq!(calculate_mip_count(300, 300), 9);
    }

    // --- Mipmap data generation tests ---

    #[test]
    fn generate_mip_data_4x4_produces_correct_chain() {
        // 4x4 -> mip_count = 3 (4x4, 2x2, 1x1)
        // So generate_mip_data returns 2 mip levels (mip 1 and mip 2)
        let width = 4u32;
        let height = 4u32;
        let data = vec![128u8; (width * height * 4) as usize];
        let mips = generate_mip_data(width, height, &data);

        assert_eq!(mips.len(), 2); // mip 1 (2x2) and mip 2 (1x1)

        // Mip 1: 2x2
        assert_eq!(mips[0].0, 2);
        assert_eq!(mips[0].1, 2);
        assert_eq!(mips[0].2.len(), (2 * 2 * 4) as usize);

        // Mip 2: 1x1
        assert_eq!(mips[1].0, 1);
        assert_eq!(mips[1].1, 1);
        assert_eq!(mips[1].2.len(), (1 * 1 * 4) as usize);
    }

    #[test]
    fn generate_mip_data_2x2_box_filter_averages() {
        // 2x2 texture with known pixel values:
        //  (0,0) = [100, 200, 50, 255]
        //  (1,0) = [200, 100, 150, 255]
        //  (0,1) = [50, 50, 200, 255]
        //  (1,1) = [150, 150, 100, 255]
        let data: Vec<u8> = vec![
            100, 200, 50, 255, // (0,0)
            200, 100, 150, 255, // (1,0)
            50, 50, 200, 255, // (0,1)
            150, 150, 100, 255, // (1,1)
        ];
        let mips = generate_mip_data(2, 2, &data);

        assert_eq!(mips.len(), 1); // only mip 1 (1x1)
        assert_eq!(mips[0].0, 1);
        assert_eq!(mips[0].1, 1);

        // Average of all 4 pixels per channel:
        // R: (100+200+50+150)/4 = 125
        // G: (200+100+50+150)/4 = 125
        // B: (50+150+200+100)/4 = 125
        // A: (255+255+255+255)/4 = 255
        assert_eq!(mips[0].2, vec![125, 125, 125, 255]);
    }

    #[test]
    fn generate_mip_data_1x1_returns_empty() {
        // 1x1 texture: mip_count = 1, so no additional mip levels
        let data = vec![255, 128, 0, 255];
        let mips = generate_mip_data(1, 1, &data);
        assert_eq!(mips.len(), 0);
    }

    #[test]
    fn generate_mip_data_8x8_chain_length() {
        // 8x8 -> mip_count = 4 (8x8, 4x4, 2x2, 1x1)
        // generate_mip_data returns 3 levels
        let data = vec![100u8; 8 * 8 * 4];
        let mips = generate_mip_data(8, 8, &data);

        assert_eq!(mips.len(), 3);
        assert_eq!((mips[0].0, mips[0].1), (4, 4));
        assert_eq!((mips[1].0, mips[1].1), (2, 2));
        assert_eq!((mips[2].0, mips[2].1), (1, 1));
    }

    #[test]
    fn generate_mip_data_non_square() {
        // 4x2 -> max(4,2)=4, log2(4)=2, mip_count=3
        // Mip 1: 2x1, Mip 2: 1x1
        let data = vec![64u8; 4 * 2 * 4];
        let mips = generate_mip_data(4, 2, &data);

        assert_eq!(mips.len(), 2);
        assert_eq!((mips[0].0, mips[0].1), (2, 1));
        assert_eq!((mips[1].0, mips[1].1), (1, 1));
    }

    #[test]
    fn generate_mip_data_uniform_texture_preserves_values() {
        // A uniform color texture should produce identical mips at every level
        let val = 42u8;
        let data = vec![val; 16 * 16 * 4];
        let mips = generate_mip_data(16, 16, &data);

        for (w, h, mip_data) in &mips {
            assert_eq!(mip_data.len(), (*w * *h * 4) as usize);
            for &byte in mip_data {
                assert_eq!(byte, val);
            }
        }
    }
}
