#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

/// Screen-Space Ambient Occlusion (SSAO) post-process system.
///
/// Pipeline: depth buffer -> SSAO pass (hemisphere sampling) -> bilateral blur -> R8Unorm AO texture.
/// The final AO texture is multiplied into the scene color during compositing.

// ── Non-WASM stub ────────────────────────────────────────────────────────

#[cfg(not(target_arch = "wasm32"))]
pub struct SsaoSystem {
    enabled: bool,
    radius: f32,
    bias: f32,
    intensity: f32,
}

#[cfg(not(target_arch = "wasm32"))]
impl SsaoSystem {
    pub fn new() -> Self {
        Self {
            enabled: true,
            radius: 0.8,
            bias: 0.025,
            intensity: 0.5,
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn set_enabled(&mut self, _v: bool) {
        self.enabled = _v;
    }

    pub fn radius(&self) -> f32 {
        self.radius
    }

    pub fn set_radius(&mut self, v: f32) {
        self.radius = v;
    }

    pub fn bias(&self) -> f32 {
        self.bias
    }

    pub fn set_bias(&mut self, v: f32) {
        self.bias = v;
    }

    pub fn intensity(&self) -> f32 {
        self.intensity
    }

    pub fn set_intensity(&mut self, v: f32) {
        self.intensity = v;
    }

    pub fn blurred_ao_view(&self) -> Option<&()> {
        None
    }
}

// ── WASM implementation ──────────────────────────────────────────────────

#[cfg(target_arch = "wasm32")]
mod wasm_impl {
    use bytemuck::{Pod, Zeroable};

    const SSAO_KERNEL_SIZE: usize = 32;
    const NOISE_DIM: u32 = 4;

    // ── SSAO shader ──────────────────────────────────────────────────────

    const SSAO_SHADER: &str = r#"
struct SsaoParams {
    projection: mat4x4<f32>,
    inv_projection: mat4x4<f32>,
    screen_dims: vec2<f32>,
    radius: f32,
    bias: f32,
    intensity: f32,
    kernel_size: u32,
    noise_scale_x: f32,
    noise_scale_y: f32,
};

struct SsaoKernel {
    samples: array<vec4<f32>, 32>,
};

@group(0) @binding(0) var depth_tex: texture_depth_2d;
@group(0) @binding(1) var noise_tex: texture_2d<f32>;
@group(0) @binding(2) var noise_sampler: sampler;
@group(0) @binding(3) var<uniform> params: SsaoParams;
@group(0) @binding(4) var<uniform> kernel: SsaoKernel;

@vertex fn vs(@builtin(vertex_index) vid: u32) -> @builtin(position) vec4<f32> {
    // fullscreen triangle
    var pos = array<vec2<f32>, 3>(vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
    return vec4(pos[vid], 0.0, 1.0);
}

fn reconstruct_view_pos(uv: vec2<f32>, depth: f32) -> vec3<f32> {
    // NDC: x,y in [-1,1], z = depth (0..1 for WebGPU)
    let ndc = vec4(uv * 2.0 - 1.0, depth, 1.0);
    // flip y for WebGPU clip space
    let ndc_flipped = vec4(ndc.x, -ndc.y, ndc.z, 1.0);
    let view_pos_h = params.inv_projection * ndc_flipped;
    return view_pos_h.xyz / view_pos_h.w;
}

fn reconstruct_normal(uv: vec2<f32>, center_pos: vec3<f32>) -> vec3<f32> {
    let texel = vec2(1.0 / params.screen_dims.x, 1.0 / params.screen_dims.y);

    let uv_r = uv + vec2(texel.x, 0.0);
    let uv_l = uv - vec2(texel.x, 0.0);
    let uv_t = uv + vec2(0.0, texel.y);
    let uv_b = uv - vec2(0.0, texel.y);

    let dims = vec2<i32>(params.screen_dims);
    let depth_r = textureLoad(depth_tex, vec2<i32>(clamp(vec2<i32>(uv_r * params.screen_dims), vec2(0), dims - vec2(1))), 0);
    let depth_l = textureLoad(depth_tex, vec2<i32>(clamp(vec2<i32>(uv_l * params.screen_dims), vec2(0), dims - vec2(1))), 0);
    let depth_t = textureLoad(depth_tex, vec2<i32>(clamp(vec2<i32>(uv_t * params.screen_dims), vec2(0), dims - vec2(1))), 0);
    let depth_b = textureLoad(depth_tex, vec2<i32>(clamp(vec2<i32>(uv_b * params.screen_dims), vec2(0), dims - vec2(1))), 0);

    let pos_r = reconstruct_view_pos(uv_r, depth_r);
    let pos_l = reconstruct_view_pos(uv_l, depth_l);
    let pos_t = reconstruct_view_pos(uv_t, depth_t);
    let pos_b = reconstruct_view_pos(uv_b, depth_b);

    let ddx = pos_r - pos_l;
    let ddy = pos_t - pos_b;

    return normalize(cross(ddy, ddx));
}

@fragment fn fs(@builtin(position) frag_coord: vec4<f32>) -> @location(0) vec4<f32> {
    let uv = frag_coord.xy / params.screen_dims;
    let pixel = vec2<i32>(frag_coord.xy);
    let depth = textureLoad(depth_tex, pixel, 0);

    // skip skybox / far plane
    if depth >= 1.0 {
        return vec4(1.0, 0.0, 0.0, 1.0);
    }

    let frag_pos = reconstruct_view_pos(uv, depth);
    let normal = reconstruct_normal(uv, frag_pos);

    // random rotation from noise texture (tiled via modulo to avoid non-uniform textureSample)
    let noise_dims = vec2<i32>(textureDimensions(noise_tex));
    let noise_pixel = vec2<i32>(frag_coord.xy) % noise_dims;
    let random_vec = textureLoad(noise_tex, noise_pixel, 0).xyz * 2.0 - 1.0;

    // Gram-Schmidt to build TBN
    let tangent = normalize(random_vec - normal * dot(random_vec, normal));
    let bitangent = cross(normal, tangent);
    let tbn = mat3x3<f32>(tangent, bitangent, normal);

    var occlusion: f32 = 0.0;
    let sample_count = min(params.kernel_size, 32u);

    for (var i: u32 = 0u; i < sample_count; i = i + 1u) {
        let sample_dir = tbn * kernel.samples[i].xyz;
        let sample_pos = frag_pos + sample_dir * params.radius;

        // project sample to screen
        let offset_clip = params.projection * vec4(sample_pos, 1.0);
        var offset_ndc = offset_clip.xy / offset_clip.w;
        // flip y back
        offset_ndc.y = -offset_ndc.y;
        let offset_uv = offset_ndc * 0.5 + 0.5;

        // sample depth at projected position
        let sample_pixel = vec2<i32>(offset_uv * params.screen_dims);
        let dims = vec2<i32>(params.screen_dims);
        let clamped_pixel = clamp(sample_pixel, vec2(0), dims - vec2(1));
        let sample_depth = textureLoad(depth_tex, clamped_pixel, 0);
        let sample_view_z = reconstruct_view_pos(offset_uv, sample_depth).z;

        // range check: only occlude if the sampled geometry is within radius
        let range_check = smoothstep(0.0, 1.0, params.radius / abs(frag_pos.z - sample_view_z));

        // if sample is behind the scene geometry (closer to camera in view space, i.e. larger z in RH),
        // it's occluded
        let is_occluded = select(0.0, 1.0, sample_view_z >= sample_pos.z + params.bias);
        occlusion = occlusion + is_occluded * range_check;
    }

    let ao = 1.0 - (occlusion / f32(sample_count)) * params.intensity;
    return vec4(clamp(ao, 0.0, 1.0), 0.0, 0.0, 1.0);
}
"#;

    // ── Bilateral blur shader ────────────────────────────────────────────

    const BLUR_SHADER: &str = r#"
struct BlurParams {
    screen_dims: vec2<f32>,
    direction: vec2<f32>,  // (1,0) for horizontal, (0,1) for vertical
};

@group(0) @binding(0) var ao_tex: texture_2d<f32>;
@group(0) @binding(1) var depth_tex: texture_depth_2d;
@group(0) @binding(2) var tex_sampler: sampler;
@group(0) @binding(3) var<uniform> blur_params: BlurParams;

@vertex fn vs(@builtin(vertex_index) vid: u32) -> @builtin(position) vec4<f32> {
    var pos = array<vec2<f32>, 3>(vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
    return vec4(pos[vid], 0.0, 1.0);
}

@fragment fn fs(@builtin(position) frag_coord: vec4<f32>) -> @location(0) vec4<f32> {
    let uv = frag_coord.xy / blur_params.screen_dims;
    let texel_size = 1.0 / blur_params.screen_dims;
    let pixel = vec2<i32>(frag_coord.xy);
    let dims = vec2<i32>(blur_params.screen_dims);

    let center_ao = textureLoad(ao_tex, pixel, 0).r;
    let center_depth = textureLoad(depth_tex, clamp(pixel, vec2(0), dims - vec2(1)), 0);

    // Gaussian weights for a 4-tap bilateral blur
    let weights = array<f32, 4>(0.324, 0.232, 0.0855, 0.0205);
    let depth_threshold: f32 = 0.001;

    var result: f32 = center_ao * weights[0];
    var total_weight: f32 = weights[0];

    for (var i: i32 = 1; i < 4; i = i + 1) {
        let offset = blur_params.direction * f32(i);

        // positive direction
        let uv_pos = uv + offset * texel_size;
        let pixel_pos = vec2<i32>(uv_pos * blur_params.screen_dims);
        let clamped_pos = clamp(pixel_pos, vec2(0), dims - vec2(1));
        let ao_pos = textureLoad(ao_tex, clamped_pos, 0).r;
        let depth_pos = textureLoad(depth_tex, clamped_pos, 0);
        let depth_diff_pos = abs(center_depth - depth_pos);
        let w_pos = weights[i] * select(0.0, 1.0, depth_diff_pos < depth_threshold);
        result = result + ao_pos * w_pos;
        total_weight = total_weight + w_pos;

        // negative direction
        let uv_neg = uv - offset * texel_size;
        let pixel_neg = vec2<i32>(uv_neg * blur_params.screen_dims);
        let clamped_neg = clamp(pixel_neg, vec2(0), dims - vec2(1));
        let ao_neg = textureLoad(ao_tex, clamped_neg, 0).r;
        let depth_neg = textureLoad(depth_tex, clamped_neg, 0);
        let depth_diff_neg = abs(center_depth - depth_neg);
        let w_neg = weights[i] * select(0.0, 1.0, depth_diff_neg < depth_threshold);
        result = result + ao_neg * w_neg;
        total_weight = total_weight + w_neg;
    }

    return vec4(result / total_weight, 0.0, 0.0, 1.0);
}
"#;

    // ── SSAO composite shader ────────────────────────────────────────────
    // Multiplies the blurred AO into the scene color as a final compositing step.

    const COMPOSITE_SHADER: &str = r#"
struct CompositeParams {
    screen_dims: vec2<f32>,
    _pad0: f32,
    _pad1: f32,
};

@group(0) @binding(0) var scene_tex: texture_2d<f32>;
@group(0) @binding(1) var ao_tex: texture_2d<f32>;
@group(0) @binding(2) var tex_sampler: sampler;
@group(0) @binding(3) var<uniform> params: CompositeParams;

@vertex fn vs(@builtin(vertex_index) vid: u32) -> @builtin(position) vec4<f32> {
    var pos = array<vec2<f32>, 3>(vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
    return vec4(pos[vid], 0.0, 1.0);
}

@fragment fn fs(@builtin(position) frag_coord: vec4<f32>) -> @location(0) vec4<f32> {
    let uv = frag_coord.xy / params.screen_dims;
    let scene_color = textureSample(scene_tex, tex_sampler, uv);
    let ao = textureSample(ao_tex, tex_sampler, uv).r;
    return vec4(scene_color.rgb * ao, scene_color.a);
}
"#;

    // ── Uniform structs ──────────────────────────────────────────────────

    #[repr(C)]
    #[derive(Clone, Copy, Pod, Zeroable)]
    struct SsaoParams {
        projection: [[f32; 4]; 4],
        inv_projection: [[f32; 4]; 4],
        screen_dims: [f32; 2],
        radius: f32,
        bias: f32,
        intensity: f32,
        kernel_size: u32,
        noise_scale_x: f32,
        noise_scale_y: f32,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Pod, Zeroable)]
    struct SsaoKernel {
        samples: [[f32; 4]; SSAO_KERNEL_SIZE],
    }

    #[repr(C)]
    #[derive(Clone, Copy, Pod, Zeroable)]
    struct BlurParams {
        screen_dims: [f32; 2],
        direction: [f32; 2],
    }

    #[repr(C)]
    #[derive(Clone, Copy, Pod, Zeroable)]
    struct CompositeParams {
        screen_dims: [f32; 2],
        _pad0: f32,
        _pad1: f32,
    }

    // ── Kernel generation ────────────────────────────────────────────────

    fn generate_ssao_kernel() -> SsaoKernel {
        // Deterministic hemisphere kernel using a simple LCG PRNG.
        // Samples are distributed in a hemisphere oriented along +Z,
        // with an accelerating falloff toward the center.
        let mut state: u32 = 0xDEAD_BEEF;
        let mut samples = [[0.0f32; 4]; SSAO_KERNEL_SIZE];

        for i in 0..SSAO_KERNEL_SIZE {
            // LCG next
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            let x = (state as f32 / u32::MAX as f32) * 2.0 - 1.0;
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            let y = (state as f32 / u32::MAX as f32) * 2.0 - 1.0;
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            let z = state as f32 / u32::MAX as f32; // hemisphere: z in [0, 1]

            // normalize
            let len = (x * x + y * y + z * z).sqrt().max(0.0001);
            let mut sx = x / len;
            let mut sy = y / len;
            let mut sz = z / len;

            // accelerating scale: more samples near the origin
            let mut scale = (i as f32) / (SSAO_KERNEL_SIZE as f32);
            scale = lerp(0.1, 1.0, scale * scale);
            sx *= scale;
            sy *= scale;
            sz *= scale;

            samples[i] = [sx, sy, sz, 0.0];
        }

        SsaoKernel { samples }
    }

    fn lerp(a: f32, b: f32, t: f32) -> f32 {
        a + (b - a) * t
    }

    fn generate_noise_data() -> Vec<u8> {
        // 4x4 random rotation vectors stored as RGBA8 (we only use RG for the tangent rotation)
        let mut state: u32 = 0xCAFE_BABE;
        let count = (NOISE_DIM * NOISE_DIM) as usize;
        let mut data = Vec::with_capacity(count * 4);

        for _ in 0..count {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            let x = (state as f32 / u32::MAX as f32) * 2.0 - 1.0;
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            let y = (state as f32 / u32::MAX as f32) * 2.0 - 1.0;

            // normalize the 2D vector, store as unorm
            let len = (x * x + y * y).sqrt().max(0.0001);
            let nx = x / len;
            let ny = y / len;

            // map [-1,1] -> [0,255]
            data.push(((nx * 0.5 + 0.5) * 255.0) as u8);
            data.push(((ny * 0.5 + 0.5) * 255.0) as u8);
            data.push(0u8);
            data.push(255u8);
        }

        data
    }

    // ── Texture helpers ──────────────────────────────────────────────────

    fn create_ao_texture(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        label: &str,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width: width.max(1),
                height: height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        (texture, view)
    }

    fn create_scene_copy_texture(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("ssao_scene_copy"),
            size: wgpu::Extent3d {
                width: width.max(1),
                height: height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        (texture, view)
    }

    // ── SsaoSystem ───────────────────────────────────────────────────────

    pub struct SsaoSystem {
        // AO textures (raw + blurred)
        ao_texture: wgpu::Texture,
        ao_view: wgpu::TextureView,
        blurred_ao_texture: wgpu::Texture,
        blurred_ao_view: wgpu::TextureView,

        // Scene copy for compositing
        scene_copy_texture: wgpu::Texture,
        scene_copy_view: wgpu::TextureView,

        // Noise texture
        _noise_texture: wgpu::Texture,
        noise_view: wgpu::TextureView,

        // Pipelines
        ssao_pipeline: wgpu::RenderPipeline,
        blur_pipeline: wgpu::RenderPipeline,
        composite_pipeline: wgpu::RenderPipeline,

        // Bind group layouts (kept for resize recreation)
        ssao_bgl: wgpu::BindGroupLayout,
        blur_bgl: wgpu::BindGroupLayout,
        composite_bgl: wgpu::BindGroupLayout,

        // Bind groups
        ssao_bind_group: wgpu::BindGroup,
        blur_h_bind_group: wgpu::BindGroup,
        blur_v_bind_group: wgpu::BindGroup,
        composite_bind_group: wgpu::BindGroup,

        // Uniform buffers
        ssao_params_buffer: wgpu::Buffer,
        ssao_kernel_buffer: wgpu::Buffer,
        blur_h_params_buffer: wgpu::Buffer,
        blur_v_params_buffer: wgpu::Buffer,
        composite_params_buffer: wgpu::Buffer,

        // Samplers
        nearest_sampler: wgpu::Sampler,
        linear_sampler: wgpu::Sampler,

        // Cached state
        width: u32,
        height: u32,
        scene_format: wgpu::TextureFormat,

        // User-tweakable parameters
        enabled: bool,
        radius: f32,
        bias: f32,
        intensity: f32,
    }

    impl SsaoSystem {
        pub fn new(
            device: &wgpu::Device,
            queue: &wgpu::Queue,
            width: u32,
            height: u32,
            depth_view: &wgpu::TextureView,
            scene_format: wgpu::TextureFormat,
        ) -> Self {
            let kernel = generate_ssao_kernel();
            let noise_data = generate_noise_data();

            // ── Noise texture ────────────────────────────────────────────
            let noise_texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("ssao_noise_texture"),
                size: wgpu::Extent3d {
                    width: NOISE_DIM,
                    height: NOISE_DIM,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &noise_texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &noise_data,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(NOISE_DIM * 4),
                    rows_per_image: Some(NOISE_DIM),
                },
                wgpu::Extent3d {
                    width: NOISE_DIM,
                    height: NOISE_DIM,
                    depth_or_array_layers: 1,
                },
            );
            let noise_view = noise_texture.create_view(&wgpu::TextureViewDescriptor::default());

            // ── AO textures ──────────────────────────────────────────────
            let (ao_texture, ao_view) = create_ao_texture(device, width, height, "ssao_ao_raw");
            let (blurred_ao_texture, blurred_ao_view) =
                create_ao_texture(device, width, height, "ssao_ao_blurred");

            // ── Scene copy texture ───────────────────────────────────────
            let (scene_copy_texture, scene_copy_view) =
                create_scene_copy_texture(device, width, height, scene_format);

            // ── Samplers ─────────────────────────────────────────────────
            let nearest_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("ssao_nearest_sampler"),
                address_mode_u: wgpu::AddressMode::Repeat,
                address_mode_v: wgpu::AddressMode::Repeat,
                mag_filter: wgpu::FilterMode::Nearest,
                min_filter: wgpu::FilterMode::Nearest,
                ..Default::default()
            });
            let linear_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("ssao_linear_sampler"),
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            });

            // ── Uniform buffers ──────────────────────────────────────────
            let ssao_params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("ssao_params_uniform"),
                size: std::mem::size_of::<SsaoParams>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

            let ssao_kernel_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("ssao_kernel_uniform"),
                size: std::mem::size_of::<SsaoKernel>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            queue.write_buffer(&ssao_kernel_buffer, 0, bytemuck::bytes_of(&kernel));

            let blur_h_params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("ssao_blur_h_params"),
                size: std::mem::size_of::<BlurParams>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

            let blur_v_params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("ssao_blur_v_params"),
                size: std::mem::size_of::<BlurParams>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

            let composite_params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("ssao_composite_params"),
                size: std::mem::size_of::<CompositeParams>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

            // ── Bind group layouts ───────────────────────────────────────

            let ssao_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("ssao_bind_group_layout"),
                entries: &[
                    // depth texture
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
                    // noise texture (Rgba8Unorm is filterable)
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
                    // noise sampler (filtering for textureSample with Repeat tiling)
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                    // params uniform
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    // kernel uniform
                    wgpu::BindGroupLayoutEntry {
                        binding: 4,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });

            let blur_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("ssao_blur_bind_group_layout"),
                entries: &[
                    // AO input texture
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: false },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    // depth texture (for bilateral weighting)
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Depth,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    // sampler
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                        count: None,
                    },
                    // blur params uniform
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });

            let composite_bgl =
                device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("ssao_composite_bind_group_layout"),
                    entries: &[
                        // scene color texture
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
                        // blurred AO texture
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
                        // sampler
                        wgpu::BindGroupLayoutEntry {
                            binding: 2,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                            count: None,
                        },
                        // params uniform
                        wgpu::BindGroupLayoutEntry {
                            binding: 3,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Uniform,
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                    ],
                });

            // ── Pipelines ────────────────────────────────────────────────

            let ssao_pipeline = Self::create_fullscreen_pipeline(
                device,
                "ssao_pass",
                SSAO_SHADER,
                &ssao_bgl,
                wgpu::TextureFormat::R8Unorm,
            );

            let blur_pipeline = Self::create_fullscreen_pipeline(
                device,
                "ssao_blur_pass",
                BLUR_SHADER,
                &blur_bgl,
                wgpu::TextureFormat::R8Unorm,
            );

            let composite_pipeline = Self::create_fullscreen_pipeline(
                device,
                "ssao_composite_pass",
                COMPOSITE_SHADER,
                &composite_bgl,
                scene_format,
            );

            // ── Bind groups ──────────────────────────────────────────────

            let ssao_bind_group = Self::create_ssao_bind_group(
                device,
                &ssao_bgl,
                depth_view,
                &noise_view,
                &nearest_sampler,
                &ssao_params_buffer,
                &ssao_kernel_buffer,
            );

            let blur_h_bind_group = Self::create_blur_bind_group(
                device,
                &blur_bgl,
                &ao_view,
                depth_view,
                &nearest_sampler,
                &blur_h_params_buffer,
                "ssao_blur_h_bg",
            );

            let blur_v_bind_group = Self::create_blur_bind_group(
                device,
                &blur_bgl,
                &blurred_ao_view,
                depth_view,
                &nearest_sampler,
                &blur_v_params_buffer,
                "ssao_blur_v_bg",
            );

            // The vertical blur pass writes the final AO into ao_view,
            // so the composite reads from ao_view.
            let composite_bind_group = Self::create_composite_bind_group(
                device,
                &composite_bgl,
                &scene_copy_view,
                &ao_view,
                &linear_sampler,
                &composite_params_buffer,
            );

            Self {
                ao_texture,
                ao_view,
                blurred_ao_texture,
                blurred_ao_view,
                scene_copy_texture,
                scene_copy_view,
                _noise_texture: noise_texture,
                noise_view,
                ssao_pipeline,
                blur_pipeline,
                composite_pipeline,
                ssao_bgl,
                blur_bgl,
                composite_bgl,
                ssao_bind_group,
                blur_h_bind_group,
                blur_v_bind_group,
                composite_bind_group,
                ssao_params_buffer,
                ssao_kernel_buffer,
                blur_h_params_buffer,
                blur_v_params_buffer,
                composite_params_buffer,
                nearest_sampler,
                linear_sampler,
                width,
                height,
                scene_format,
                enabled: true,
                radius: 0.8,
                bias: 0.025,
                intensity: 0.5,
            }
        }

        // ── Pipeline creation ────────────────────────────────────────────

        fn create_fullscreen_pipeline(
            device: &wgpu::Device,
            label: &str,
            shader_source: &str,
            bind_group_layout: &wgpu::BindGroupLayout,
            target_format: wgpu::TextureFormat,
        ) -> wgpu::RenderPipeline {
            let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(label),
                source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(shader_source)),
            });
            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(label),
                bind_group_layouts: &[bind_group_layout],
                push_constant_ranges: &[],
            });
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("vs"),
                    buffers: &[],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some("fs"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: target_format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            })
        }

        // ── Bind group creation helpers ──────────────────────────────────

        fn create_ssao_bind_group(
            device: &wgpu::Device,
            layout: &wgpu::BindGroupLayout,
            depth_view: &wgpu::TextureView,
            noise_view: &wgpu::TextureView,
            sampler: &wgpu::Sampler,
            params_buffer: &wgpu::Buffer,
            kernel_buffer: &wgpu::Buffer,
        ) -> wgpu::BindGroup {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("ssao_bind_group"),
                layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(depth_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(noise_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: params_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: kernel_buffer.as_entire_binding(),
                    },
                ],
            })
        }

        fn create_blur_bind_group(
            device: &wgpu::Device,
            layout: &wgpu::BindGroupLayout,
            ao_view: &wgpu::TextureView,
            depth_view: &wgpu::TextureView,
            sampler: &wgpu::Sampler,
            params_buffer: &wgpu::Buffer,
            label: &str,
        ) -> wgpu::BindGroup {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(label),
                layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(ao_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(depth_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: params_buffer.as_entire_binding(),
                    },
                ],
            })
        }

        fn create_composite_bind_group(
            device: &wgpu::Device,
            layout: &wgpu::BindGroupLayout,
            scene_view: &wgpu::TextureView,
            ao_view: &wgpu::TextureView,
            sampler: &wgpu::Sampler,
            params_buffer: &wgpu::Buffer,
        ) -> wgpu::BindGroup {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("ssao_composite_bg"),
                layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(scene_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(ao_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: params_buffer.as_entire_binding(),
                    },
                ],
            })
        }

        // ── Resize ───────────────────────────────────────────────────────

        pub fn resize(
            &mut self,
            device: &wgpu::Device,
            width: u32,
            height: u32,
            depth_view: &wgpu::TextureView,
        ) {
            if width == self.width && height == self.height {
                return;
            }
            self.width = width;
            self.height = height;

            let (ao_texture, ao_view) = create_ao_texture(device, width, height, "ssao_ao_raw");
            let (blurred_ao_texture, blurred_ao_view) =
                create_ao_texture(device, width, height, "ssao_ao_blurred");
            let (scene_copy_texture, scene_copy_view) =
                create_scene_copy_texture(device, width, height, self.scene_format);

            self.ssao_bind_group = Self::create_ssao_bind_group(
                device,
                &self.ssao_bgl,
                depth_view,
                &self.noise_view,
                &self.nearest_sampler,
                &self.ssao_params_buffer,
                &self.ssao_kernel_buffer,
            );

            self.blur_h_bind_group = Self::create_blur_bind_group(
                device,
                &self.blur_bgl,
                &ao_view,
                depth_view,
                &self.nearest_sampler,
                &self.blur_h_params_buffer,
                "ssao_blur_h_bg",
            );

            self.blur_v_bind_group = Self::create_blur_bind_group(
                device,
                &self.blur_bgl,
                &blurred_ao_view,
                depth_view,
                &self.nearest_sampler,
                &self.blur_v_params_buffer,
                "ssao_blur_v_bg",
            );

            self.composite_bind_group = Self::create_composite_bind_group(
                device,
                &self.composite_bgl,
                &scene_copy_view,
                &ao_view,
                &self.linear_sampler,
                &self.composite_params_buffer,
            );

            self.ao_texture = ao_texture;
            self.ao_view = ao_view;
            self.blurred_ao_texture = blurred_ao_texture;
            self.blurred_ao_view = blurred_ao_view;
            self.scene_copy_texture = scene_copy_texture;
            self.scene_copy_view = scene_copy_view;
        }

        /// Rebuild bind groups when the depth view is replaced (e.g. after renderer resize).
        pub fn update_depth_view(&mut self, device: &wgpu::Device, depth_view: &wgpu::TextureView) {
            self.ssao_bind_group = Self::create_ssao_bind_group(
                device,
                &self.ssao_bgl,
                depth_view,
                &self.noise_view,
                &self.nearest_sampler,
                &self.ssao_params_buffer,
                &self.ssao_kernel_buffer,
            );

            self.blur_h_bind_group = Self::create_blur_bind_group(
                device,
                &self.blur_bgl,
                &self.ao_view,
                depth_view,
                &self.nearest_sampler,
                &self.blur_h_params_buffer,
                "ssao_blur_h_bg",
            );

            self.blur_v_bind_group = Self::create_blur_bind_group(
                device,
                &self.blur_bgl,
                &self.blurred_ao_view,
                depth_view,
                &self.nearest_sampler,
                &self.blur_v_params_buffer,
                "ssao_blur_v_bg",
            );
        }

        // ── Render ───────────────────────────────────────────────────────

        /// Execute the SSAO pass, blur, and composite.
        ///
        /// `scene_texture` is the surface texture that the 3D scene was rendered into.
        /// `output_view` is where the final composited result should be written (same surface).
        /// `projection` / `inv_projection` are the camera projection matrices in column-major
        /// layout matching FrameUniform3D.
        pub fn render(
            &self,
            encoder: &mut wgpu::CommandEncoder,
            queue: &wgpu::Queue,
            scene_texture: &wgpu::Texture,
            output_view: &wgpu::TextureView,
            projection: [[f32; 4]; 4],
            inv_projection: [[f32; 4]; 4],
        ) {
            if !self.enabled {
                return;
            }

            // Upload SSAO params
            let params = SsaoParams {
                projection,
                inv_projection,
                screen_dims: [self.width as f32, self.height as f32],
                radius: self.radius,
                bias: self.bias,
                intensity: self.intensity,
                kernel_size: SSAO_KERNEL_SIZE as u32,
                noise_scale_x: self.width as f32 / NOISE_DIM as f32,
                noise_scale_y: self.height as f32 / NOISE_DIM as f32,
            };
            queue.write_buffer(&self.ssao_params_buffer, 0, bytemuck::bytes_of(&params));

            // Upload blur params
            let blur_h = BlurParams {
                screen_dims: [self.width as f32, self.height as f32],
                direction: [1.0, 0.0],
            };
            let blur_v = BlurParams {
                screen_dims: [self.width as f32, self.height as f32],
                direction: [0.0, 1.0],
            };
            queue.write_buffer(&self.blur_h_params_buffer, 0, bytemuck::bytes_of(&blur_h));
            queue.write_buffer(&self.blur_v_params_buffer, 0, bytemuck::bytes_of(&blur_v));

            // Upload composite params
            let composite_params = CompositeParams {
                screen_dims: [self.width as f32, self.height as f32],
                _pad0: 0.0,
                _pad1: 0.0,
            };
            queue.write_buffer(
                &self.composite_params_buffer,
                0,
                bytemuck::bytes_of(&composite_params),
            );

            // Copy the scene texture so we can read it during compositing.
            encoder.copy_texture_to_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: scene_texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyTextureInfo {
                    texture: &self.scene_copy_texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::Extent3d {
                    width: self.width.max(1),
                    height: self.height.max(1),
                    depth_or_array_layers: 1,
                },
            );

            // ── Pass 1: SSAO ─────────────────────────────────────────────
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("ssao_pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &self.ao_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                pass.set_pipeline(&self.ssao_pipeline);
                pass.set_bind_group(0, &self.ssao_bind_group, &[]);
                pass.draw(0..3, 0..1);
            }

            // ── Pass 2: Horizontal bilateral blur ────────────────────────
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("ssao_blur_h_pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &self.blurred_ao_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                pass.set_pipeline(&self.blur_pipeline);
                pass.set_bind_group(0, &self.blur_h_bind_group, &[]);
                pass.draw(0..3, 0..1);
            }

            // ── Pass 3: Vertical bilateral blur ──────────────────────────
            // Reads horizontal blur result (blurred_ao), writes final AO to ao_texture.
            // The composite pass then reads from ao_texture.
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("ssao_blur_v_pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &self.ao_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                pass.set_pipeline(&self.blur_pipeline);
                pass.set_bind_group(0, &self.blur_v_bind_group, &[]);
                pass.draw(0..3, 0..1);
            }

            // ── Pass 4: Composite scene * AO ─────────────────────────────
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("ssao_composite_pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: output_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                pass.set_pipeline(&self.composite_pipeline);
                pass.set_bind_group(0, &self.composite_bind_group, &[]);
                pass.draw(0..3, 0..1);
            }
        }

        // ── Accessors ────────────────────────────────────────────────────

        pub fn blurred_ao_view(&self) -> &wgpu::TextureView {
            // After the two-pass blur, the final AO is in ao_view (vertical pass output)
            &self.ao_view
        }

        pub fn enabled(&self) -> bool {
            self.enabled
        }

        pub fn set_enabled(&mut self, v: bool) {
            self.enabled = v;
        }

        pub fn radius(&self) -> f32 {
            self.radius
        }

        pub fn set_radius(&mut self, v: f32) {
            self.radius = v.clamp(0.01, 5.0);
        }

        pub fn bias(&self) -> f32 {
            self.bias
        }

        pub fn set_bias(&mut self, v: f32) {
            self.bias = v.clamp(0.0, 0.5);
        }

        pub fn intensity(&self) -> f32 {
            self.intensity
        }

        pub fn set_intensity(&mut self, v: f32) {
            self.intensity = v.clamp(0.0, 5.0);
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm_impl::SsaoSystem;

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    #[cfg(not(target_arch = "wasm32"))]
    use super::SsaoSystem;

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn stub_creates_successfully() {
        let ssao = SsaoSystem::new();
        assert!(ssao.enabled());
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn stub_default_radius() {
        let ssao = SsaoSystem::new();
        assert!((ssao.radius() - 0.8).abs() < f32::EPSILON);
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn stub_default_bias() {
        let ssao = SsaoSystem::new();
        assert!((ssao.bias() - 0.025).abs() < f32::EPSILON);
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn stub_default_intensity() {
        let ssao = SsaoSystem::new();
        assert!((ssao.intensity() - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn stub_set_enabled_round_trip() {
        let mut ssao = SsaoSystem::new();
        ssao.set_enabled(false);
        assert!(!ssao.enabled());
        ssao.set_enabled(true);
        assert!(ssao.enabled());
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn stub_set_radius_round_trip() {
        let mut ssao = SsaoSystem::new();
        ssao.set_radius(1.5);
        assert!((ssao.radius() - 1.5).abs() < f32::EPSILON);
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn stub_set_bias_round_trip() {
        let mut ssao = SsaoSystem::new();
        ssao.set_bias(0.1);
        assert!((ssao.bias() - 0.1).abs() < f32::EPSILON);
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn stub_set_intensity_round_trip() {
        let mut ssao = SsaoSystem::new();
        ssao.set_intensity(2.0);
        assert!((ssao.intensity() - 2.0).abs() < f32::EPSILON);
    }

    #[test]
    fn ssao_params_uniform_size() {
        // SsaoParams: 2x mat4x4 (128) + 2 f32 (8) + f32 (4) + f32 (4) + f32 (4) + u32 (4) + 2 f32 (8) = 160
        #[repr(C)]
        #[derive(Clone, Copy)]
        struct SsaoParamsLayout {
            projection: [[f32; 4]; 4],
            inv_projection: [[f32; 4]; 4],
            screen_dims: [f32; 2],
            radius: f32,
            bias: f32,
            intensity: f32,
            kernel_size: u32,
            noise_scale_x: f32,
            noise_scale_y: f32,
        }
        assert_eq!(std::mem::size_of::<SsaoParamsLayout>(), 160);
    }

    #[test]
    fn ssao_kernel_size() {
        #[repr(C)]
        #[derive(Clone, Copy)]
        struct SsaoKernelLayout {
            samples: [[f32; 4]; 32],
        }
        assert_eq!(std::mem::size_of::<SsaoKernelLayout>(), 512);
    }

    #[test]
    fn blur_params_size() {
        #[repr(C)]
        #[derive(Clone, Copy)]
        struct BlurParamsLayout {
            screen_dims: [f32; 2],
            direction: [f32; 2],
        }
        assert_eq!(std::mem::size_of::<BlurParamsLayout>(), 16);
    }

    #[test]
    fn composite_params_size() {
        #[repr(C)]
        #[derive(Clone, Copy)]
        struct CompositeParamsLayout {
            screen_dims: [f32; 2],
            _pad0: f32,
            _pad1: f32,
        }
        assert_eq!(std::mem::size_of::<CompositeParamsLayout>(), 16);
    }
}
