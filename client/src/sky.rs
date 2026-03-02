#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

/// Sky gradient pass — renders a fullscreen triangle with a vertical gradient
/// sky (zenith/horizon/ground) plus an HDR sun disc before PBR geometry.
///
/// The shader reconstructs a world-space ray direction from clip coordinates
/// using the inverse view-projection matrix, then mixes sky colors based on
/// the ray's vertical component and adds a Gaussian sun disc.

// ── Uniform data (shared across WASM and non-WASM) ─────────────────────────

/// GPU-uploadable sky uniforms. Must match the WGSL `SkyUniforms` struct layout.
#[repr(C)]
#[derive(Copy, Clone, Debug)]
#[cfg_attr(target_arch = "wasm32", derive(bytemuck::Pod, bytemuck::Zeroable))]
pub struct SkyUniforms {
    pub inv_view_proj: [[f32; 4]; 4], // 64 bytes
    pub sun_direction: [f32; 4],       // 16 bytes (xyz = direction, w = unused)
    pub sun_color: [f32; 4],           // 16 bytes (xyz = HDR color, w = unused)
    pub sky_zenith: [f32; 4],          // 16 bytes
    pub sky_horizon: [f32; 4],         // 16 bytes
    pub sky_ground: [f32; 4],          // 16 bytes
}

impl Default for SkyUniforms {
    fn default() -> Self {
        Self {
            inv_view_proj: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
            sun_direction: [0.3, 0.8, 0.5, 0.0],
            sun_color: [3.0, 2.8, 2.2, 0.0],
            sky_zenith: [0.15, 0.3, 0.65, 1.0],
            sky_horizon: [0.7, 0.75, 0.85, 1.0],
            sky_ground: [0.25, 0.22, 0.18, 1.0],
        }
    }
}

// ── Sky color interpolation (pure function, testable on all targets) ────────

/// Compute the sky color for a given vertical direction component `ray_y`.
///
/// - `ray_y >= 0`: lerp between horizon and zenith based on `ray_y`
/// - `ray_y < 0`: lerp between horizon and ground based on `-ray_y`
///
/// Returns `[r, g, b]` in linear HDR space (before sun disc contribution).
pub fn sky_color_at(ray_y: f32, zenith: [f32; 3], horizon: [f32; 3], ground: [f32; 3]) -> [f32; 3] {
    if ray_y >= 0.0 {
        // Above horizon: blend from horizon to zenith
        let t = ray_y.clamp(0.0, 1.0);
        [
            horizon[0] + (zenith[0] - horizon[0]) * t,
            horizon[1] + (zenith[1] - horizon[1]) * t,
            horizon[2] + (zenith[2] - horizon[2]) * t,
        ]
    } else {
        // Below horizon: blend from horizon to ground
        let t = (-ray_y).clamp(0.0, 1.0);
        [
            horizon[0] + (ground[0] - horizon[0]) * t,
            horizon[1] + (ground[1] - horizon[1]) * t,
            horizon[2] + (ground[2] - horizon[2]) * t,
        ]
    }
}

// ── Non-WASM stub ───────────────────────────────────────────────────────────

#[cfg(not(target_arch = "wasm32"))]
pub struct SkyPass;

#[cfg(not(target_arch = "wasm32"))]
impl SkyPass {
    pub fn new() -> Self {
        Self
    }
}

// ── WASM implementation ─────────────────────────────────────────────────────

#[cfg(target_arch = "wasm32")]
const SKY_SHADER: &str = r#"
struct SkyUniforms {
    inv_view_proj: mat4x4<f32>,
    sun_direction: vec4<f32>,
    sun_color: vec4<f32>,
    sky_zenith: vec4<f32>,
    sky_horizon: vec4<f32>,
    sky_ground: vec4<f32>,
};

@group(0) @binding(0) var<uniform> sky: SkyUniforms;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_sky(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    // Fullscreen triangle: 3 vertices covering the entire screen
    // vertex 0: (-1, -1), vertex 1: (3, -1), vertex 2: (-1, 3)
    var out: VertexOutput;
    let x = f32(i32(vertex_index & 1u) * 4 - 1);
    let y = f32(i32(vertex_index >> 1u) * 4 - 1);
    out.position = vec4<f32>(x, y, 1.0, 1.0);
    // UV: map from clip space to [0,1]
    out.uv = vec2<f32>(x * 0.5 + 0.5, 1.0 - (y * 0.5 + 0.5));
    return out;
}

@fragment
fn fs_sky(in: VertexOutput) -> @location(0) vec4<f32> {
    // Reconstruct world-space ray direction from UV via clip space
    let clip = vec4<f32>(in.uv.x * 2.0 - 1.0, (1.0 - in.uv.y) * 2.0 - 1.0, 1.0, 1.0);
    let world_pos = sky.inv_view_proj * clip;
    let ray_dir = normalize(world_pos.xyz / world_pos.w);

    // Vertical blend factor
    let ray_y = ray_dir.y;

    // Sky gradient
    var color: vec3<f32>;
    if (ray_y >= 0.0) {
        // Above horizon: blend from horizon to zenith
        let t = clamp(ray_y, 0.0, 1.0);
        color = mix(sky.sky_horizon.rgb, sky.sky_zenith.rgb, t);
    } else {
        // Below horizon: blend from horizon to ground
        let t = clamp(-ray_y, 0.0, 1.0);
        color = mix(sky.sky_horizon.rgb, sky.sky_ground.rgb, t);
    }

    // Sun disc: Gaussian falloff around the sun direction
    let sun_dir = normalize(sky.sun_direction.xyz);
    let cos_angle = dot(ray_dir, sun_dir);
    // Convert cosine of angle to angle, apply Gaussian with ~0.05 rad width
    // For small angles: angle ≈ acos(cos_angle), but we can use 1 - cos_angle as proxy
    // Gaussian: exp(-angle^2 / (2 * sigma^2)) where sigma = 0.05
    // Since angle = acos(cos_angle), for the tight disc we compute directly:
    let angle = acos(clamp(cos_angle, -1.0, 1.0));
    let sigma = 0.05;
    let sun_intensity = exp(-(angle * angle) / (2.0 * sigma * sigma));
    color = color + sky.sun_color.rgb * sun_intensity;

    return vec4<f32>(color, 1.0);
}
"#;

#[cfg(target_arch = "wasm32")]
pub struct SkyPass {
    pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

#[cfg(target_arch = "wasm32")]
impl SkyPass {
    pub fn new(device: &wgpu::Device, target_format: wgpu::TextureFormat) -> Self {
        use std::borrow::Cow;

        let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("wrela-sky-shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(SKY_SHADER)),
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("wrela-sky-uniform-buffer"),
            size: std::mem::size_of::<SkyUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("wrela-sky-bind-group-layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: Some(
                            std::num::NonZeroU64::new(
                                std::mem::size_of::<SkyUniforms>() as u64,
                            )
                            .expect("SkyUniforms has non-zero size"),
                        ),
                    },
                    count: None,
                }],
            });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("wrela-sky-bind-group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("wrela-sky-pipeline-layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("wrela-sky-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader_module,
                entry_point: Some("vs_sky"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[], // Fullscreen triangle — no vertex buffers
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None, // Fullscreen triangle — no culling
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: None, // Sky doesn't write depth
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader_module,
                entry_point: Some("fs_sky"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: None, // Sky replaces everything (opaque write)
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview: None,
            cache: None,
        });

        Self {
            pipeline,
            uniform_buffer,
            bind_group,
        }
    }

    /// Render the sky gradient into the target view.
    ///
    /// 1. Writes uniforms to the GPU buffer
    /// 2. Begins a render pass with `LoadOp::Clear` (sky replaces everything)
    /// 3. No depth attachment (sky is always behind geometry)
    /// 4. Draws 3 vertices (fullscreen triangle)
    pub fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        queue: &wgpu::Queue,
        target_view: &wgpu::TextureView,
        uniforms: &SkyUniforms,
    ) {
        // Upload uniforms
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(uniforms));

        // Begin render pass
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("wrela-sky-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.0,
                        g: 0.0,
                        b: 0.0,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
        });

        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.draw(0..3, 0..1); // Fullscreen triangle
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sky_uniforms_size_is_144_bytes() {
        // inv_view_proj: 4x4 floats = 64 bytes
        // sun_direction: 4 floats = 16 bytes
        // sun_color:     4 floats = 16 bytes
        // sky_zenith:    4 floats = 16 bytes
        // sky_horizon:   4 floats = 16 bytes
        // sky_ground:    4 floats = 16 bytes
        // Total: 64 + 16 + 16 + 16 + 16 + 16 = 144 bytes
        assert_eq!(std::mem::size_of::<SkyUniforms>(), 144);
    }

    #[test]
    fn sky_uniforms_default_values() {
        let u = SkyUniforms::default();
        assert_eq!(u.sun_direction, [0.3, 0.8, 0.5, 0.0]);
        assert_eq!(u.sun_color, [3.0, 2.8, 2.2, 0.0]);
        assert_eq!(u.sky_zenith, [0.15, 0.3, 0.65, 1.0]);
        assert_eq!(u.sky_horizon, [0.7, 0.75, 0.85, 1.0]);
        assert_eq!(u.sky_ground, [0.25, 0.22, 0.18, 1.0]);
    }

    #[test]
    fn sky_color_at_zenith() {
        let zenith = [0.15, 0.3, 0.65];
        let horizon = [0.7, 0.75, 0.85];
        let ground = [0.25, 0.22, 0.18];

        // ray_y = 1.0 should return pure zenith
        let color = sky_color_at(1.0, zenith, horizon, ground);
        assert!((color[0] - zenith[0]).abs() < 1e-6);
        assert!((color[1] - zenith[1]).abs() < 1e-6);
        assert!((color[2] - zenith[2]).abs() < 1e-6);
    }

    #[test]
    fn sky_color_at_horizon() {
        let zenith = [0.15, 0.3, 0.65];
        let horizon = [0.7, 0.75, 0.85];
        let ground = [0.25, 0.22, 0.18];

        // ray_y = 0.0 should return pure horizon
        let color = sky_color_at(0.0, zenith, horizon, ground);
        assert!((color[0] - horizon[0]).abs() < 1e-6);
        assert!((color[1] - horizon[1]).abs() < 1e-6);
        assert!((color[2] - horizon[2]).abs() < 1e-6);
    }

    #[test]
    fn sky_color_at_ground() {
        let zenith = [0.15, 0.3, 0.65];
        let horizon = [0.7, 0.75, 0.85];
        let ground = [0.25, 0.22, 0.18];

        // ray_y = -1.0 should return pure ground
        let color = sky_color_at(-1.0, zenith, horizon, ground);
        assert!((color[0] - ground[0]).abs() < 1e-6);
        assert!((color[1] - ground[1]).abs() < 1e-6);
        assert!((color[2] - ground[2]).abs() < 1e-6);
    }

    #[test]
    fn sky_color_at_midpoint_above_horizon() {
        let zenith = [0.0, 0.0, 1.0];
        let horizon = [1.0, 1.0, 1.0];
        let ground = [0.0, 0.0, 0.0];

        // ray_y = 0.5 should return midpoint between horizon and zenith
        let color = sky_color_at(0.5, zenith, horizon, ground);
        assert!((color[0] - 0.5).abs() < 1e-6);
        assert!((color[1] - 0.5).abs() < 1e-6);
        assert!((color[2] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn sky_color_at_midpoint_below_horizon() {
        let zenith = [0.0, 0.0, 1.0];
        let horizon = [1.0, 1.0, 1.0];
        let ground = [0.0, 0.0, 0.0];

        // ray_y = -0.5 should return midpoint between horizon and ground
        let color = sky_color_at(-0.5, zenith, horizon, ground);
        assert!((color[0] - 0.5).abs() < 1e-6);
        assert!((color[1] - 0.5).abs() < 1e-6);
        assert!((color[2] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn sky_color_at_clamps_above_1() {
        let zenith = [0.15, 0.3, 0.65];
        let horizon = [0.7, 0.75, 0.85];
        let ground = [0.25, 0.22, 0.18];

        // ray_y = 2.0 should clamp to 1.0 and return zenith
        let color = sky_color_at(2.0, zenith, horizon, ground);
        assert!((color[0] - zenith[0]).abs() < 1e-6);
        assert!((color[1] - zenith[1]).abs() < 1e-6);
        assert!((color[2] - zenith[2]).abs() < 1e-6);
    }

    #[test]
    fn sky_color_at_clamps_below_neg1() {
        let zenith = [0.15, 0.3, 0.65];
        let horizon = [0.7, 0.75, 0.85];
        let ground = [0.25, 0.22, 0.18];

        // ray_y = -2.0 should clamp to -1.0 and return ground
        let color = sky_color_at(-2.0, zenith, horizon, ground);
        assert!((color[0] - ground[0]).abs() < 1e-6);
        assert!((color[1] - ground[1]).abs() < 1e-6);
        assert!((color[2] - ground[2]).abs() < 1e-6);
    }

    #[test]
    fn non_wasm_stub_compiles() {
        // This test ensures the non-WASM stub struct and methods exist.
        // On WASM targets this test still passes because SkyPass has new() too.
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _pass = SkyPass::new();
        }
    }

    #[test]
    fn sky_uniforms_alignment_is_4_byte_aligned() {
        // All f32 fields, so minimum alignment is 4 bytes
        assert!(std::mem::align_of::<SkyUniforms>() >= 4);
    }

    #[test]
    fn sky_color_at_smooth_transition_across_horizon() {
        // Test several points near the horizon to ensure no discontinuity
        let zenith = [0.15, 0.3, 0.65];
        let horizon = [0.7, 0.75, 0.85];
        let ground = [0.25, 0.22, 0.18];

        let color_above = sky_color_at(0.001, zenith, horizon, ground);
        let color_at = sky_color_at(0.0, zenith, horizon, ground);
        let color_below = sky_color_at(-0.001, zenith, horizon, ground);

        // All three should be very close to the horizon color
        for i in 0..3 {
            assert!(
                (color_above[i] - color_at[i]).abs() < 0.01,
                "channel {i}: above={} at={}",
                color_above[i],
                color_at[i]
            );
            assert!(
                (color_below[i] - color_at[i]).abs() < 0.01,
                "channel {i}: below={} at={}",
                color_below[i],
                color_at[i]
            );
        }
    }
}
