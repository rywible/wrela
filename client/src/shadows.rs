#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

use crate::camera_math::{Mat4, mat4_identity, mat4_inverse, mat4_mul, mat4_ortho};

/// Maximum 3D instances for shadow rendering (mirrors web.rs MAX_3D_INSTANCES).
const MAX_SHADOW_INSTANCES: usize = 1024;

/// Size of one ModelEntry in the storage buffer (mirrors web.rs MODEL_ENTRY_SIZE).
const MODEL_ENTRY_SIZE: u64 = 128;

/// Number of shadow cascades.
pub const CASCADE_COUNT: usize = 3;

/// Resolution of each cascade in the atlas (width = height per cascade).
pub const CASCADE_RESOLUTION: u32 = 2048;

/// Total atlas width = CASCADE_RESOLUTION * CASCADE_COUNT, height = CASCADE_RESOLUTION.
pub const SHADOW_ATLAS_WIDTH: u32 = CASCADE_RESOLUTION * CASCADE_COUNT as u32;
pub const SHADOW_ATLAS_HEIGHT: u32 = CASCADE_RESOLUTION;

/// Lambda for practical split scheme (0 = linear, 1 = logarithmic).
const SPLIT_LAMBDA: f32 = 0.75;

/// Depth bias constant to prevent shadow acne.
pub const SHADOW_DEPTH_BIAS: i32 = 2;

/// Slope-scale depth bias.
pub const SHADOW_SLOPE_BIAS: f32 = 2.0;

/// Normal bias offset in world units pushed along surface normal in the shader.
pub const SHADOW_NORMAL_BIAS: f32 = 0.02;

/// Mirror of the ModelUniform struct from the main renderer, used for shadow depth pass.
#[repr(C)]
#[derive(Clone, Copy)]
#[cfg(target_arch = "wasm32")]
struct ShadowModelUniform {
    model: [[f32; 4]; 4],
    normal_model: [[f32; 4]; 4],
}

#[cfg(target_arch = "wasm32")]
unsafe impl bytemuck::Pod for ShadowModelUniform {}
#[cfg(target_arch = "wasm32")]
unsafe impl bytemuck::Zeroable for ShadowModelUniform {}

/// GPU-uploadable shadow data.  Must match the WGSL `ShadowData` struct layout.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ShadowData {
    pub light_view_proj: [[[f32; 4]; 4]; CASCADE_COUNT],
    /// cascade_splits.x = split[0] far, cascade_splits.y = split[1] far (== camera far).
    /// Packed into a vec4 for alignment.
    pub cascade_splits: [f32; 4],
    /// Normal bias in .x, padding in .yzw.
    pub bias: [f32; 4],
}

/// Phase 2a interface: when set, the outline/cel passes use this depth
/// instead of the rasterized shadow depth. Phase 1 leaves this as `None`.
/// Phase 2a (field-based ray march) will set this to the ray-marched depth.
pub struct ShadowDepthOverride {
    _private: (), // placeholder, Phase 2a will add TextureView field
}

impl ShadowDepthOverride {
    /// Phase 1 stub: no override available.
    pub fn none() -> Option<Self> {
        None
    }
}

#[cfg(target_arch = "wasm32")]
unsafe impl bytemuck::Pod for ShadowData {}
#[cfg(target_arch = "wasm32")]
unsafe impl bytemuck::Zeroable for ShadowData {}

impl Default for ShadowData {
    fn default() -> Self {
        Self {
            light_view_proj: [[[0.0; 4]; 4]; CASCADE_COUNT],
            cascade_splits: [0.0; 4],
            bias: [SHADOW_NORMAL_BIAS, 0.0, 0.0, 0.0],
        }
    }
}

/// Compute practical split distances for cascaded shadow maps.
/// Returns an array of `CASCADE_COUNT + 1` values:  [near, split0_far, split1_far].
pub fn compute_cascade_splits(near: f32, far: f32) -> [f32; CASCADE_COUNT + 1] {
    let mut splits = [0.0f32; CASCADE_COUNT + 1];
    splits[0] = near;
    for i in 1..=CASCADE_COUNT {
        let t = i as f32 / CASCADE_COUNT as f32;
        let log_split = near * (far / near).powf(t);
        let lin_split = near + (far - near) * t;
        splits[i] = SPLIT_LAMBDA * log_split + (1.0 - SPLIT_LAMBDA) * lin_split;
    }
    splits
}

/// Build frustum corners in NDC, then unproject to world space.
/// Returns 8 corners: [near_tl, near_tr, near_bl, near_br, far_tl, far_tr, far_bl, far_br].
fn frustum_corners_world(inv_view_proj: Mat4) -> [[f32; 3]; 8] {
    let ndc_corners: [[f32; 3]; 8] = [
        // near plane
        [-1.0, 1.0, 0.0],
        [1.0, 1.0, 0.0],
        [-1.0, -1.0, 0.0],
        [1.0, -1.0, 0.0],
        // far plane
        [-1.0, 1.0, 1.0],
        [1.0, 1.0, 1.0],
        [-1.0, -1.0, 1.0],
        [1.0, -1.0, 1.0],
    ];

    let mut world = [[0.0f32; 3]; 8];
    for (i, ndc) in ndc_corners.iter().enumerate() {
        let clip = [ndc[0], ndc[1], ndc[2], 1.0];
        let w = mat4_transform_vec4(inv_view_proj, clip);
        let inv_w = 1.0 / w[3];
        world[i] = [w[0] * inv_w, w[1] * inv_w, w[2] * inv_w];
    }
    world
}

fn mat4_transform_vec4(m: Mat4, v: [f32; 4]) -> [f32; 4] {
    [
        m[0][0] * v[0] + m[1][0] * v[1] + m[2][0] * v[2] + m[3][0] * v[3],
        m[0][1] * v[0] + m[1][1] * v[1] + m[2][1] * v[2] + m[3][1] * v[3],
        m[0][2] * v[0] + m[1][2] * v[1] + m[2][2] * v[2] + m[3][2] * v[3],
        m[0][3] * v[0] + m[1][3] * v[1] + m[2][3] * v[2] + m[3][3] * v[3],
    ]
}

/// Compute a sub-frustum for a cascade between `near_t` and `far_t` (0..1 along the full
/// frustum depth).  Returns 8 world-space corners.
fn cascade_sub_frustum(full_corners: &[[f32; 3]; 8], near_t: f32, far_t: f32) -> [[f32; 3]; 8] {
    let mut sub = [[0.0f32; 3]; 8];
    // near corners (indices 0..4) are the full frustum near plane
    // far corners (indices 4..8) are the full frustum far plane
    // Interpolate along each ray from near to far.
    for i in 0..4 {
        let n = full_corners[i];
        let f = full_corners[i + 4];
        sub[i] = [
            n[0] + (f[0] - n[0]) * near_t,
            n[1] + (f[1] - n[1]) * near_t,
            n[2] + (f[2] - n[2]) * near_t,
        ];
        sub[i + 4] = [
            n[0] + (f[0] - n[0]) * far_t,
            n[1] + (f[1] - n[1]) * far_t,
            n[2] + (f[2] - n[2]) * far_t,
        ];
    }
    sub
}

/// Build a light-space view matrix looking along `light_dir` centred on `center`.
fn light_view_matrix(light_dir: [f32; 3], center: [f32; 3]) -> Mat4 {
    let len =
        (light_dir[0] * light_dir[0] + light_dir[1] * light_dir[1] + light_dir[2] * light_dir[2])
            .sqrt();
    if len < 1e-10 {
        return mat4_identity();
    }
    let dir = [light_dir[0] / len, light_dir[1] / len, light_dir[2] / len];

    // Eye is behind the center along the light direction so that geometry in front is captured.
    let eye = [
        center[0] - dir[0] * 100.0,
        center[1] - dir[1] * 100.0,
        center[2] - dir[2] * 100.0,
    ];

    crate::camera_math::mat4_look_at(eye, center, [0.0, 1.0, 0.0])
}

/// Compute the light-space view-projection matrix for one cascade.
/// Applies texel snapping to prevent shimmer.
pub fn compute_cascade_matrix(cascade_corners: &[[f32; 3]; 8], light_dir: [f32; 3]) -> Mat4 {
    // Compute frustum center
    let mut center = [0.0f32; 3];
    for c in cascade_corners {
        center[0] += c[0];
        center[1] += c[1];
        center[2] += c[2];
    }
    center[0] /= 8.0;
    center[1] /= 8.0;
    center[2] /= 8.0;

    let light_view = light_view_matrix(light_dir, center);

    // Transform all corners to light space and compute AABB
    let mut min_x = f32::MAX;
    let mut max_x = f32::MIN;
    let mut min_y = f32::MAX;
    let mut max_y = f32::MIN;
    let mut min_z = f32::MAX;
    let mut max_z = f32::MIN;

    for corner in cascade_corners {
        let lv = mat4_transform_vec4(light_view, [corner[0], corner[1], corner[2], 1.0]);
        min_x = min_x.min(lv[0]);
        max_x = max_x.max(lv[0]);
        min_y = min_y.min(lv[1]);
        max_y = max_y.max(lv[1]);
        min_z = min_z.min(lv[2]);
        max_z = max_z.max(lv[2]);
    }

    // Extend the near plane to capture shadow casters behind the camera frustum
    let z_range = max_z - min_z;
    min_z -= z_range * 2.0;

    // Texel snapping: quantize the AABB to shadow-map texel boundaries
    let world_units_per_texel_x = (max_x - min_x) / CASCADE_RESOLUTION as f32;
    let world_units_per_texel_y = (max_y - min_y) / CASCADE_RESOLUTION as f32;

    if world_units_per_texel_x > 1e-10 {
        min_x = (min_x / world_units_per_texel_x).floor() * world_units_per_texel_x;
        max_x = (max_x / world_units_per_texel_x).floor() * world_units_per_texel_x;
    }
    if world_units_per_texel_y > 1e-10 {
        min_y = (min_y / world_units_per_texel_y).floor() * world_units_per_texel_y;
        max_y = (max_y / world_units_per_texel_y).floor() * world_units_per_texel_y;
    }

    let light_proj = mat4_ortho(min_x, max_x, min_y, max_y, min_z, max_z);
    mat4_mul(light_proj, light_view)
}

/// Compute all cascade light-space view-projection matrices and fill ShadowData.
pub fn update_shadow_data(
    camera_view: Mat4,
    camera_proj: Mat4,
    light_dir: [f32; 3],
    near: f32,
    far: f32,
) -> ShadowData {
    let view_proj = mat4_mul(camera_proj, camera_view);
    let inv_vp = mat4_inverse(view_proj);
    let full_corners = frustum_corners_world(inv_vp);

    let splits = compute_cascade_splits(near, far);

    let mut data = ShadowData::default();
    for i in 0..CASCADE_COUNT {
        let near_t = (splits[i] - near) / (far - near);
        let far_t = (splits[i + 1] - near) / (far - near);
        let sub = cascade_sub_frustum(&full_corners, near_t, far_t);
        data.light_view_proj[i] = compute_cascade_matrix(&sub, light_dir);
        data.cascade_splits[i] = splits[i + 1];
    }
    data.bias[0] = SHADOW_NORMAL_BIAS;

    data
}

/// Size of the shadow LightUniform buffer: mat4x4 (64) + wind_params vec4 (16) + wind_dir vec4 (16) = 96 bytes.
pub const SHADOW_LIGHT_UNIFORM_SIZE: u64 = 96;

/// WGSL source for the depth-only shadow pass vertex shader.
pub const SHADOW_DEPTH_SHADER: &str = r#"
struct LightUniform {
    light_view_proj: mat4x4<f32>,
    // Wind parameters: x = time, y = strength, z = turbulence, w = unused
    wind_params: vec4<f32>,
    // Wind direction: xyz = direction, w = unused
    wind_dir: vec4<f32>,
};

struct ModelEntry {
    model: mat4x4<f32>,
    normal_model: mat4x4<f32>,
};

@group(0) @binding(0) var<uniform> light: LightUniform;
@group(1) @binding(0) var<storage, read> model_matrices: array<ModelEntry>;
@group(1) @binding(1) var<storage, read> joint_palette: array<mat4x4<f32>>;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) joint_indices: vec4<u32>,
    @location(4) joint_weights: vec4<f32>,
    @location(5) vertex_color: vec4<f32>,
    @location(6) tangent: vec4<f32>,
    @builtin(instance_index) instance_id: u32,
};

// Wind displacement function (must match main shader exactly for shadow consistency)
fn compute_wind_displacement(pos: vec3<f32>, wind_color: vec4<f32>, time: f32, wind_direction: vec3<f32>, strength: f32, turbulence: f32) -> vec3<f32> {
    let trunk_weight = wind_color.r;
    let branch_weight = wind_color.g;
    let leaf_weight = wind_color.b;
    let phase_offset = wind_color.a;

    // If no wind weights, skip entirely
    let total_weight = trunk_weight + branch_weight + leaf_weight;
    if (total_weight < 0.001) {
        return vec3<f32>(0.0);
    }

    // Gust cycle: slow sine modulation (period ~8 seconds)
    let gust = sin(time * 0.7854) * 0.5 + 0.5;
    let effective_strength = strength * (0.6 + 0.4 * gust);

    // Trunk sway: slow, large displacement along wind direction
    let trunk_phase = time * 1.2 + phase_offset * 6.283;
    let trunk_sway = wind_direction * sin(trunk_phase) * trunk_weight * effective_strength;

    // Branch oscillation: medium frequency, perpendicular-ish
    let branch_phase = time * 3.5 + phase_offset * 12.566;
    let branch_perp = normalize(vec3<f32>(-wind_direction.z, 0.0, wind_direction.x));
    let branch_osc = (wind_direction * sin(branch_phase) * 0.5 + branch_perp * cos(branch_phase * 1.3) * 0.3) * branch_weight * effective_strength * 0.5;

    // Leaf flutter: high frequency, small chaotic displacement
    let leaf_phase = time * 8.0 + phase_offset * 25.13;
    let leaf_disp = vec3<f32>(
        sin(leaf_phase * 1.1 + pos.x * 2.0) * 0.3,
        sin(leaf_phase * 1.7 + pos.y * 3.0) * 0.15,
        cos(leaf_phase * 0.9 + pos.z * 2.5) * 0.3
    ) * leaf_weight * effective_strength * turbulence * 0.3;

    return trunk_sway + branch_osc + leaf_disp;
}

@vertex
fn vs_shadow(in: VertexInput) -> @builtin(position) vec4<f32> {
    let weight_sum = in.joint_weights[0] + in.joint_weights[1]
                   + in.joint_weights[2] + in.joint_weights[3];
    var skinned_pos: vec4<f32>;
    if (weight_sum > 0.0) {
        let skin_matrix = joint_palette[in.joint_indices[0]] * in.joint_weights[0]
                        + joint_palette[in.joint_indices[1]] * in.joint_weights[1]
                        + joint_palette[in.joint_indices[2]] * in.joint_weights[2]
                        + joint_palette[in.joint_indices[3]] * in.joint_weights[3];
        skinned_pos = skin_matrix * vec4<f32>(in.position, 1.0);
    } else {
        skinned_pos = vec4<f32>(in.position, 1.0);
    }

    // Apply wind displacement (must match main shader for shadow consistency)
    let wind_offset = compute_wind_displacement(
        skinned_pos.xyz,
        in.vertex_color,
        light.wind_params.x,
        light.wind_dir.xyz,
        light.wind_params.y,
        light.wind_params.z
    );
    skinned_pos = vec4<f32>(skinned_pos.xyz + wind_offset, skinned_pos.w);

    let world_pos = model_matrices[in.instance_id].model * skinned_pos;
    return light.light_view_proj * world_pos;
}
"#;

#[cfg(target_arch = "wasm32")]
pub struct ShadowSystem {
    pub atlas_texture: wgpu::Texture,
    pub atlas_view: wgpu::TextureView,
    pub cascade_views: Vec<wgpu::TextureView>,
    pub shadow_sampler: wgpu::Sampler,
    pub shadow_bind_group_layout: wgpu::BindGroupLayout,
    pub shadow_bind_group: wgpu::BindGroup,
    pub shadow_data_buffer: wgpu::Buffer,
    pub shadow_depth_pipeline: wgpu::RenderPipeline,
    pub light_uniform_buffer: wgpu::Buffer,
    pub light_bind_group_layout: wgpu::BindGroupLayout,
    pub light_bind_group: wgpu::BindGroup,
    pub shadow_data: ShadowData,
}

#[cfg(target_arch = "wasm32")]
impl ShadowSystem {
    pub fn new(device: &wgpu::Device, model_bind_group_layout: &wgpu::BindGroupLayout) -> Self {
        use std::borrow::Cow;

        // Create shadow atlas texture
        let atlas_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("wrela-shadow-atlas"),
            size: wgpu::Extent3d {
                width: SHADOW_ATLAS_WIDTH,
                height: SHADOW_ATLAS_HEIGHT,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });

        let atlas_view = atlas_texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("wrela-shadow-atlas-view"),
            ..Default::default()
        });

        // Create per-cascade views using viewport offsets at render time
        // (we render to the full texture but use viewport to target each cascade)
        let cascade_views: Vec<wgpu::TextureView> = Vec::new();

        // Comparison sampler for shadow sampling
        let shadow_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("wrela-shadow-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });

        // Shadow data uniform buffer
        let shadow_data = ShadowData::default();
        let shadow_data_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("wrela-shadow-data-uniform"),
            size: std::mem::size_of::<ShadowData>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Bind group layout for shadow sampling in PBR shader (group 3)
        let shadow_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("wrela-shadow-bind-group-layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Depth,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: Some(
                                std::num::NonZeroU64::new(std::mem::size_of::<ShadowData>() as u64)
                                    .expect("ShadowData has non-zero size"),
                            ),
                        },
                        count: None,
                    },
                ],
            });

        let shadow_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("wrela-shadow-bind-group"),
            layout: &shadow_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&atlas_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&shadow_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: shadow_data_buffer.as_entire_binding(),
                },
            ],
        });

        // Light uniform buffer for shadow depth pass (mat4 + wind_params + wind_dir)
        let light_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("wrela-shadow-light-uniform"),
            size: SHADOW_LIGHT_UNIFORM_SIZE,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let light_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("wrela-shadow-light-bind-group-layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: Some(
                            std::num::NonZeroU64::new(SHADOW_LIGHT_UNIFORM_SIZE)
                                .expect("SHADOW_LIGHT_UNIFORM_SIZE > 0"),
                        ),
                    },
                    count: None,
                }],
            });

        let light_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("wrela-shadow-light-bind-group"),
            layout: &light_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: light_uniform_buffer.as_entire_binding(),
            }],
        });

        // Shadow depth render pipeline
        let shadow_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("wrela-shadow-depth-pipeline-layout"),
                bind_group_layouts: &[&light_bind_group_layout, model_bind_group_layout],
                push_constant_ranges: &[],
            });

        let shadow_shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("wrela-shadow-depth-shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(SHADOW_DEPTH_SHADER)),
        });

        let vertex_buffer_layout = wgpu::VertexBufferLayout {
            array_stride: 88,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: 12,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: 24,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: 32,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Uint16x4,
                },
                wgpu::VertexAttribute {
                    offset: 40,
                    shader_location: 4,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 56,
                    shader_location: 5,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 72,
                    shader_location: 6,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        };

        let shadow_depth_pipeline =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("wrela-shadow-depth-pipeline"),
                layout: Some(&shadow_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shadow_shader_module,
                    entry_point: Some("vs_shadow"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &[vertex_buffer_layout],
                },
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    // Render front faces to depth for shadow mapping
                    cull_mode: Some(wgpu::Face::Front),
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth32Float,
                    depth_write_enabled: true,
                    depth_compare: wgpu::CompareFunction::Less,
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState {
                        constant: SHADOW_DEPTH_BIAS,
                        slope_scale: SHADOW_SLOPE_BIAS,
                        clamp: 0.0,
                    },
                }),
                multisample: wgpu::MultisampleState::default(),
                fragment: None,
                multiview: None,
                cache: None,
            });

        Self {
            atlas_texture,
            atlas_view,
            cascade_views,
            shadow_sampler,
            shadow_bind_group_layout,
            shadow_bind_group,
            shadow_data_buffer,
            shadow_depth_pipeline,
            light_uniform_buffer,
            light_bind_group_layout,
            light_bind_group,
            shadow_data,
        }
    }

    /// Update cascade matrices and upload ShadowData to the GPU.
    pub fn update_cascades(
        &mut self,
        queue: &wgpu::Queue,
        camera_view: Mat4,
        camera_proj: Mat4,
        light_dir: [f32; 3],
        near: f32,
        far: f32,
    ) {
        self.shadow_data = update_shadow_data(camera_view, camera_proj, light_dir, near, far);
        queue.write_buffer(
            &self.shadow_data_buffer,
            0,
            bytemuck::bytes_of(&self.shadow_data),
        );
    }

    /// Encode shadow depth passes for all cascades into the command encoder.
    /// `mesh_instances` is the list of (mesh_index, model_matrix) pairs.
    /// `meshes` is the GPU mesh array.
    /// `model_uniform_buffer` is the storage buffer for model matrices.
    /// `model_bind_group` is the bind group for model storage (group 1).
    /// `wind_params` = [time, strength, turbulence, 0.0].
    /// `wind_dir` = [dir_x, dir_y, dir_z, 0.0].
    pub fn encode_shadow_passes(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        queue: &wgpu::Queue,
        meshes: &[crate::mesh::GpuMesh],
        mesh_instances: &[(usize, [[f32; 4]; 4])],
        model_uniform_buffer: &wgpu::Buffer,
        model_bind_group: &wgpu::BindGroup,
        wind_params: [f32; 4],
        wind_dir: [f32; 4],
    ) {
        let total = mesh_instances.len().min(MAX_SHADOW_INSTANCES);

        // Sort by mesh_index for batched instanced draws.
        let mut sorted_indices: Vec<usize> = (0..total)
            .filter(|&i| mesh_instances[i].0 < meshes.len())
            .collect();
        sorted_indices.sort_by_key(|&i| mesh_instances[i].0);

        // Upload model matrices in sorted order into the storage buffer.
        for (slot, &orig_idx) in sorted_indices.iter().enumerate() {
            let (_, model_matrix) = &mesh_instances[orig_idx];
            let normal_model =
                crate::camera_math::mat4_transpose(crate::camera_math::mat4_inverse(*model_matrix));
            let model_uniform = ShadowModelUniform {
                model: *model_matrix,
                normal_model,
            };
            let offset = slot as u64 * MODEL_ENTRY_SIZE;
            queue.write_buffer(
                model_uniform_buffer,
                offset,
                bytemuck::bytes_of(&model_uniform),
            );
        }

        for cascade in 0..CASCADE_COUNT {
            // Upload the light view-proj + wind parameters for this cascade
            let light_vp = self.shadow_data.light_view_proj[cascade];
            queue.write_buffer(
                &self.light_uniform_buffer,
                0,
                bytemuck::cast_slice(&light_vp),
            );
            // Upload wind_params at offset 64 (after mat4)
            queue.write_buffer(
                &self.light_uniform_buffer,
                64,
                bytemuck::cast_slice(&wind_params),
            );
            // Upload wind_dir at offset 80
            queue.write_buffer(
                &self.light_uniform_buffer,
                80,
                bytemuck::cast_slice(&wind_dir),
            );

            let viewport_x = (cascade as u32 * CASCADE_RESOLUTION) as f32;

            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("wrela-shadow-depth-pass"),
                    color_attachments: &[],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &self.atlas_view,
                        depth_ops: Some(wgpu::Operations {
                            load: if cascade == 0 {
                                wgpu::LoadOp::Clear(1.0)
                            } else {
                                wgpu::LoadOp::Load
                            },
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    occlusion_query_set: None,
                    timestamp_writes: None,
                });

                pass.set_viewport(
                    viewport_x,
                    0.0,
                    CASCADE_RESOLUTION as f32,
                    CASCADE_RESOLUTION as f32,
                    0.0,
                    1.0,
                );

                pass.set_pipeline(&self.shadow_depth_pipeline);
                pass.set_bind_group(0, &self.light_bind_group, &[]);
                pass.set_bind_group(1, model_bind_group, &[]);

                // Issue one instanced draw call per unique mesh batch.
                let mut batch_start: usize = 0;
                while batch_start < sorted_indices.len() {
                    let mesh_index = mesh_instances[sorted_indices[batch_start]].0;
                    let mut batch_end = batch_start + 1;
                    while batch_end < sorted_indices.len()
                        && mesh_instances[sorted_indices[batch_end]].0 == mesh_index
                    {
                        batch_end += 1;
                    }

                    let mesh = &meshes[mesh_index];
                    let first_instance = batch_start as u32;
                    let count = (batch_end - batch_start) as u32;

                    pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                    pass.set_index_buffer(mesh.index_buffer.slice(..), mesh.index_format);
                    pass.draw_indexed(
                        0..mesh.index_count,
                        0,
                        first_instance..first_instance + count,
                    );

                    batch_start = batch_end;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cascade_splits_monotonically_increase() {
        let splits = compute_cascade_splits(0.1, 500.0);
        assert_eq!(splits.len(), CASCADE_COUNT + 1);
        for i in 0..CASCADE_COUNT {
            assert!(
                splits[i] < splits[i + 1],
                "split[{}]={} should be < split[{}]={}",
                i,
                splits[i],
                i + 1,
                splits[i + 1]
            );
        }
        assert!((splits[0] - 0.1).abs() < 1e-6);
        assert!((splits[CASCADE_COUNT] - 500.0).abs() < 1.0);
    }

    #[test]
    fn cascade_splits_near_equals_input_near() {
        let splits = compute_cascade_splits(0.5, 100.0);
        assert!((splits[0] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn cascade_splits_far_is_near_camera_far() {
        let splits = compute_cascade_splits(0.1, 500.0);
        // Last split should be the camera far distance (or very close to it)
        assert!((splits[CASCADE_COUNT] - 500.0).abs() < 1.0);
    }

    #[test]
    fn shadow_data_default_has_zero_matrices() {
        let data = ShadowData::default();
        for cascade in 0..CASCADE_COUNT {
            for row in &data.light_view_proj[cascade] {
                for val in row {
                    assert!(*val == 0.0);
                }
            }
        }
    }

    #[test]
    fn shadow_data_size_is_correct() {
        // CASCADE_COUNT * mat4 (64 bytes each) + vec4 (16 bytes) + vec4 (16 bytes)
        let expected = CASCADE_COUNT * 64 + 16 + 16;
        assert_eq!(std::mem::size_of::<ShadowData>(), expected);
    }

    #[test]
    fn compute_cascade_matrix_returns_valid_matrix() {
        let corners: [[f32; 3]; 8] = [
            [-1.0, -1.0, -1.0],
            [1.0, -1.0, -1.0],
            [-1.0, 1.0, -1.0],
            [1.0, 1.0, -1.0],
            [-1.0, -1.0, 1.0],
            [1.0, -1.0, 1.0],
            [-1.0, 1.0, 1.0],
            [1.0, 1.0, 1.0],
        ];
        let light_dir = [0.3, -0.8, 0.5];
        let mat = compute_cascade_matrix(&corners, light_dir);

        // Matrix should not be all zeros
        let mut has_nonzero = false;
        for row in &mat {
            for val in row {
                if *val != 0.0 {
                    has_nonzero = true;
                }
            }
        }
        assert!(has_nonzero, "cascade matrix should have non-zero values");
    }

    #[test]
    fn update_shadow_data_fills_cascade_splits() {
        let view =
            crate::camera_math::mat4_look_at([0.0, 5.0, 10.0], [0.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
        let proj = crate::camera_math::mat4_perspective(
            std::f32::consts::FRAC_PI_4,
            16.0 / 9.0,
            0.1,
            500.0,
        );
        let light_dir = [0.3, -0.8, 0.5];

        let data = update_shadow_data(view, proj, light_dir, 0.1, 500.0);

        // Splits should be populated and monotonically increasing
        assert!(data.cascade_splits[0] > 0.1);
        assert!(data.cascade_splits[0] < 500.0);
        for i in 1..CASCADE_COUNT {
            assert!(
                data.cascade_splits[i] > data.cascade_splits[i - 1]
                    || (data.cascade_splits[i] - 500.0).abs() < 1.0,
                "cascade_splits[{i}] should be > cascade_splits[{}]",
                i - 1
            );
        }

        // Light view proj matrices should be non-zero
        for cascade in 0..CASCADE_COUNT {
            let mut nonzero = false;
            for row in &data.light_view_proj[cascade] {
                for val in row {
                    if *val != 0.0 {
                        nonzero = true;
                    }
                }
            }
            assert!(
                nonzero,
                "cascade {cascade} light_view_proj should be non-zero"
            );
        }
    }

    #[test]
    fn shadow_depth_override_none_returns_none() {
        let result = ShadowDepthOverride::none();
        assert!(result.is_none());
    }

    #[test]
    fn shadow_depth_override_is_zero_sized() {
        // The stub should be minimal — just a unit struct placeholder
        assert_eq!(std::mem::size_of::<ShadowDepthOverride>(), 0);
    }
}
