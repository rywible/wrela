#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

/// Screen-space outline pass — Sobel edge detection on depth and normal G-buffer.
///
/// Runs as a fullscreen fragment pass after cel geometry, compositing ink-style
/// outlines onto the HDR buffer. The pass reads depth + normal textures and a
/// copy of the scene HDR, then outputs the scene with outlines mixed in.

// ── Non-WASM stub ────────────────────────────────────────────────────────

#[cfg(not(target_arch = "wasm32"))]
pub struct OutlinePass {
    pub depth_threshold: f32,
    pub normal_threshold: f32,
    pub thickness: f32,
    pub color: [f32; 3],
}

#[cfg(not(target_arch = "wasm32"))]
impl OutlinePass {
    pub fn new() -> Self {
        Self {
            depth_threshold: 0.3,
            normal_threshold: 0.4,
            thickness: 1.0,
            color: [0.05, 0.03, 0.02],
        }
    }

    pub fn depth_threshold(&self) -> f32 {
        self.depth_threshold
    }
    pub fn set_depth_threshold(&mut self, v: f32) {
        self.depth_threshold = v;
    }
    pub fn normal_threshold(&self) -> f32 {
        self.normal_threshold
    }
    pub fn set_normal_threshold(&mut self, v: f32) {
        self.normal_threshold = v;
    }
    pub fn thickness(&self) -> f32 {
        self.thickness
    }
    pub fn set_thickness(&mut self, v: f32) {
        self.thickness = v;
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Default for OutlinePass {
    fn default() -> Self {
        Self::new()
    }
}

// ── WASM implementation ──────────────────────────────────────────────────

#[cfg(target_arch = "wasm32")]
mod wasm_impl {
    use bytemuck::{Pod, Zeroable};

    /// GPU-uploadable outline uniforms. 32 bytes.
    #[repr(C)]
    #[derive(Clone, Copy, Pod, Zeroable)]
    pub struct OutlineUniforms {
        pub screen_width: f32,
        pub screen_height: f32,
        pub depth_threshold: f32,
        pub normal_threshold: f32,
        pub thickness: f32,
        pub color_r: f32,
        pub color_g: f32,
        pub color_b: f32,
    }

    /// Outline WGSL shader — Sobel 3×3 on depth and normals.
    ///
    /// Bind group layout:
    ///   binding 0: depth texture (texture_depth_2d)
    ///   binding 1: normal G-buffer texture (texture_2d<f32>)
    ///   binding 2: scene HDR copy texture (texture_2d<f32>)
    ///   binding 3: sampler
    ///   binding 4: outline uniform buffer
    pub const OUTLINE_SHADER: &str = r#"
struct OutlineParams {
    screen_width: f32,
    screen_height: f32,
    depth_threshold: f32,
    normal_threshold: f32,
    thickness: f32,
    color_r: f32,
    color_g: f32,
    color_b: f32,
};

@group(0) @binding(0) var depth_tex: texture_depth_2d;
@group(0) @binding(1) var normal_tex: texture_2d<f32>;
@group(0) @binding(2) var scene_tex: texture_2d<f32>;
@group(0) @binding(3) var tex_sampler: sampler;
@group(0) @binding(4) var<uniform> params: OutlineParams;

struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex fn vs(@builtin(vertex_index) vid: u32) -> VsOut {
    var pos = array<vec2<f32>, 3>(vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
    var uv = array<vec2<f32>, 3>(vec2(0.0, 1.0), vec2(2.0, 1.0), vec2(0.0, -1.0));
    var out: VsOut;
    out.position = vec4(pos[vid], 0.0, 1.0);
    out.uv = uv[vid];
    return out;
}

// Sobel 3×3 edge detection on depth buffer.
// Returns the magnitude of the depth gradient.
fn sobel_depth(center: vec2<i32>, step: i32) -> f32 {
    let dims = vec2<i32>(i32(params.screen_width), i32(params.screen_height));

    let tl = textureLoad(depth_tex, clamp(center + vec2(-step, -step), vec2(0), dims - vec2(1)), 0);
    let tc = textureLoad(depth_tex, clamp(center + vec2(0, -step), vec2(0), dims - vec2(1)), 0);
    let tr = textureLoad(depth_tex, clamp(center + vec2(step, -step), vec2(0), dims - vec2(1)), 0);
    let ml = textureLoad(depth_tex, clamp(center + vec2(-step, 0), vec2(0), dims - vec2(1)), 0);
    let mr = textureLoad(depth_tex, clamp(center + vec2(step, 0), vec2(0), dims - vec2(1)), 0);
    let bl = textureLoad(depth_tex, clamp(center + vec2(-step, step), vec2(0), dims - vec2(1)), 0);
    let bc = textureLoad(depth_tex, clamp(center + vec2(0, step), vec2(0), dims - vec2(1)), 0);
    let br = textureLoad(depth_tex, clamp(center + vec2(step, step), vec2(0), dims - vec2(1)), 0);

    // Sobel Gx = [-1,0,1; -2,0,2; -1,0,1]
    let gx = -tl + tr - 2.0 * ml + 2.0 * mr - bl + br;
    // Sobel Gy = [-1,-2,-1; 0,0,0; 1,2,1]
    let gy = -tl - 2.0 * tc - tr + bl + 2.0 * bc + br;

    return sqrt(gx * gx + gy * gy);
}

// Sobel 3×3 edge detection on normal G-buffer.
// Returns the magnitude of the normal gradient.
fn sobel_normal(center: vec2<i32>, step: i32) -> f32 {
    let dims = vec2<i32>(i32(params.screen_width), i32(params.screen_height));

    let tl = textureLoad(normal_tex, clamp(center + vec2(-step, -step), vec2(0), dims - vec2(1)), 0).rgb;
    let tc = textureLoad(normal_tex, clamp(center + vec2(0, -step), vec2(0), dims - vec2(1)), 0).rgb;
    let tr = textureLoad(normal_tex, clamp(center + vec2(step, -step), vec2(0), dims - vec2(1)), 0).rgb;
    let ml = textureLoad(normal_tex, clamp(center + vec2(-step, 0), vec2(0), dims - vec2(1)), 0).rgb;
    let mr = textureLoad(normal_tex, clamp(center + vec2(step, 0), vec2(0), dims - vec2(1)), 0).rgb;
    let bl = textureLoad(normal_tex, clamp(center + vec2(-step, step), vec2(0), dims - vec2(1)), 0).rgb;
    let bc = textureLoad(normal_tex, clamp(center + vec2(0, step), vec2(0), dims - vec2(1)), 0).rgb;
    let br = textureLoad(normal_tex, clamp(center + vec2(step, step), vec2(0), dims - vec2(1)), 0).rgb;

    // Sobel per-channel, then take max magnitude
    let gx = -tl + tr - 2.0 * ml + 2.0 * mr - bl + br;
    let gy = -tl - 2.0 * tc - tr + bl + 2.0 * bc + br;

    return max(length(gx), length(gy));
}

@fragment fn fs(in: VsOut) -> @location(0) vec4<f32> {
    let pixel = vec2<i32>(in.position.xy);
    let step = i32(max(params.thickness, 1.0));

    // Compute edge strength from depth and normals
    let depth_edge = sobel_depth(pixel, step);
    let normal_edge = sobel_normal(pixel, step);

    // Combine edges: max of normalized depth and normal edges
    let edge = max(
        depth_edge / max(params.depth_threshold, 0.001),
        normal_edge / max(params.normal_threshold, 0.001)
    );

    // Read scene color
    let scene_color = textureSample(scene_tex, tex_sampler, in.uv).rgb;

    // Mix outline color based on edge strength
    let outline_color = vec3(params.color_r, params.color_g, params.color_b);
    let result = mix(scene_color, outline_color, saturate(edge));

    return vec4(result, 1.0);
}
"#;

    pub struct OutlinePass {
        // Textures
        pub scene_copy_texture: wgpu::Texture,
        pub scene_copy_view: wgpu::TextureView,

        // Pipeline
        pub pipeline: wgpu::RenderPipeline,

        // Bind group resources
        pub bind_group_layout: wgpu::BindGroupLayout,
        pub bind_group: wgpu::BindGroup,
        pub uniform_buffer: wgpu::Buffer,
        pub sampler: wgpu::Sampler,

        // Parameters
        pub depth_threshold: f32,
        pub normal_threshold: f32,
        pub thickness: f32,
        pub color: [f32; 3],

        // Cached dims
        width: u32,
        height: u32,
    }

    impl OutlinePass {
        pub fn new(
            device: &wgpu::Device,
            width: u32,
            height: u32,
            depth_view: &wgpu::TextureView,
            normal_view: &wgpu::TextureView,
            hdr_format: wgpu::TextureFormat,
        ) -> Self {
            let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("outline_sampler"),
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            });

            let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("outline_uniform_buffer"),
                size: std::mem::size_of::<OutlineUniforms>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

            let (scene_copy_texture, scene_copy_view) =
                Self::create_scene_copy(device, width, height, hdr_format);

            let bind_group_layout =
                device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("outline_bind_group_layout"),
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
                        // normal G-buffer texture
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                                view_dimension: wgpu::TextureViewDimension::D2,
                                multisampled: false,
                            },
                            count: None,
                        },
                        // scene HDR copy texture
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
                        // sampler
                        wgpu::BindGroupLayoutEntry {
                            binding: 3,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                            count: None,
                        },
                        // outline uniforms
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

            let bind_group = Self::create_bind_group(
                device,
                &bind_group_layout,
                depth_view,
                normal_view,
                &scene_copy_view,
                &sampler,
                &uniform_buffer,
            );

            let pipeline = Self::create_pipeline(device, &bind_group_layout, hdr_format);

            Self {
                scene_copy_texture,
                scene_copy_view,
                pipeline,
                bind_group_layout,
                bind_group,
                uniform_buffer,
                sampler,
                depth_threshold: 0.3,
                normal_threshold: 0.4,
                thickness: 1.0,
                color: [0.05, 0.03, 0.02],
                width,
                height,
            }
        }

        pub fn resize(
            &mut self,
            device: &wgpu::Device,
            width: u32,
            height: u32,
            depth_view: &wgpu::TextureView,
            normal_view: &wgpu::TextureView,
            hdr_format: wgpu::TextureFormat,
        ) {
            if width == self.width && height == self.height {
                return;
            }
            self.width = width;
            self.height = height;

            let (scene_copy_texture, scene_copy_view) =
                Self::create_scene_copy(device, width, height, hdr_format);

            self.bind_group = Self::create_bind_group(
                device,
                &self.bind_group_layout,
                depth_view,
                normal_view,
                &scene_copy_view,
                &self.sampler,
                &self.uniform_buffer,
            );

            self.scene_copy_texture = scene_copy_texture;
            self.scene_copy_view = scene_copy_view;
        }

        /// Encode the outline pass into the command encoder.
        /// Copies HDR to scene_copy, then runs outline shader writing back to HDR.
        pub fn render(
            &self,
            encoder: &mut wgpu::CommandEncoder,
            queue: &wgpu::Queue,
            hdr_texture: &wgpu::Texture,
            hdr_view: &wgpu::TextureView,
        ) {
            // Upload uniforms
            let uniforms = OutlineUniforms {
                screen_width: self.width as f32,
                screen_height: self.height as f32,
                depth_threshold: self.depth_threshold,
                normal_threshold: self.normal_threshold,
                thickness: self.thickness,
                color_r: self.color[0],
                color_g: self.color[1],
                color_b: self.color[2],
            };
            queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));

            // Copy HDR -> scene_copy (read-after-write barrier handled by wgpu)
            encoder.copy_texture_to_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: hdr_texture,
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

            // Run outline shader: read scene_copy + depth + normals, write to HDR
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("outline_pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: hdr_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &self.bind_group, &[]);
                pass.draw(0..3, 0..1);
            }
        }

        fn create_scene_copy(
            device: &wgpu::Device,
            width: u32,
            height: u32,
            format: wgpu::TextureFormat,
        ) -> (wgpu::Texture, wgpu::TextureView) {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("outline_scene_copy"),
                size: wgpu::Extent3d {
                    width: width.max(1),
                    height: height.max(1),
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            (texture, view)
        }

        fn create_bind_group(
            device: &wgpu::Device,
            layout: &wgpu::BindGroupLayout,
            depth_view: &wgpu::TextureView,
            normal_view: &wgpu::TextureView,
            scene_copy_view: &wgpu::TextureView,
            sampler: &wgpu::Sampler,
            uniform_buffer: &wgpu::Buffer,
        ) -> wgpu::BindGroup {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("outline_bind_group"),
                layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(depth_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(normal_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(scene_copy_view),
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
            })
        }

        fn create_pipeline(
            device: &wgpu::Device,
            bind_group_layout: &wgpu::BindGroupLayout,
            target_format: wgpu::TextureFormat,
        ) -> wgpu::RenderPipeline {
            let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("outline_shader"),
                source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(OUTLINE_SHADER)),
            });
            let pipeline_layout =
                device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("outline_pipeline_layout"),
                    bind_group_layouts: &[bind_group_layout],
                    push_constant_ranges: &[],
                });
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("outline_pipeline"),
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

        pub fn depth_threshold(&self) -> f32 {
            self.depth_threshold
        }
        pub fn set_depth_threshold(&mut self, v: f32) {
            self.depth_threshold = v;
        }
        pub fn normal_threshold(&self) -> f32 {
            self.normal_threshold
        }
        pub fn set_normal_threshold(&mut self, v: f32) {
            self.normal_threshold = v;
        }
        pub fn thickness(&self) -> f32 {
            self.thickness
        }
        pub fn set_thickness(&mut self, v: f32) {
            self.thickness = v;
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm_impl::{OutlinePass, OutlineUniforms, OUTLINE_SHADER};

#[cfg(test)]
mod tests {
    #[cfg(not(target_arch = "wasm32"))]
    use super::OutlinePass;

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn stub_creates_with_defaults() {
        let outline = OutlinePass::new();
        assert!((outline.depth_threshold() - 0.3).abs() < 1e-6);
        assert!((outline.normal_threshold() - 0.4).abs() < 1e-6);
        assert!((outline.thickness() - 1.0).abs() < 1e-6);
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn stub_setters_work() {
        let mut outline = OutlinePass::new();
        outline.set_depth_threshold(0.5);
        assert!((outline.depth_threshold() - 0.5).abs() < 1e-6);
        outline.set_normal_threshold(0.6);
        assert!((outline.normal_threshold() - 0.6).abs() < 1e-6);
        outline.set_thickness(2.0);
        assert!((outline.thickness() - 2.0).abs() < 1e-6);
    }

    #[test]
    fn outline_uniforms_size_is_32_bytes() {
        #[repr(C)]
        #[derive(Clone, Copy)]
        struct OutlineUniforms {
            screen_width: f32,
            screen_height: f32,
            depth_threshold: f32,
            normal_threshold: f32,
            thickness: f32,
            color_r: f32,
            color_g: f32,
            color_b: f32,
        }
        assert_eq!(std::mem::size_of::<OutlineUniforms>(), 32);
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn default_impl_matches_new() {
        let a = OutlinePass::new();
        let b = OutlinePass::default();
        assert!((a.depth_threshold() - b.depth_threshold()).abs() < 1e-6);
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn default_color_is_dark() {
        let outline = OutlinePass::new();
        // Outline color should be near-black for ink-style edges
        assert!(outline.color[0] < 0.2);
        assert!(outline.color[1] < 0.2);
        assert!(outline.color[2] < 0.2);
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn thresholds_are_positive() {
        let outline = OutlinePass::new();
        assert!(outline.depth_threshold() > 0.0);
        assert!(outline.normal_threshold() > 0.0);
        assert!(outline.thickness() > 0.0);
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn combat_fx_parry_white_outline() {
        let mut outline = OutlinePass::new();
        // Simulate parry flash: set color to white
        outline.color = [1.0, 1.0, 1.0];
        assert!((outline.color[0] - 1.0).abs() < 1e-6);
        assert!((outline.color[1] - 1.0).abs() < 1e-6);
        assert!((outline.color[2] - 1.0).abs() < 1e-6);
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn combat_fx_hitstop_thicker_outline() {
        let mut outline = OutlinePass::new();
        assert!((outline.thickness() - 1.0).abs() < 1e-6);
        // Simulate hitstop: increase thickness
        outline.set_thickness(1.5);
        assert!((outline.thickness() - 1.5).abs() < 1e-6);
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn resonance_glow_lightens_outline() {
        let mut outline = OutlinePass::new();
        let base_color = outline.color;
        // Simulate resonance glow at intensity 0.35
        let glow = 0.35_f32;
        outline.color = [
            0.05 + glow * 0.95,
            0.03 + glow * 0.97,
            0.02 + glow * 0.98,
        ];
        // Color should be lighter than base
        assert!(outline.color[0] > base_color[0]);
        assert!(outline.color[1] > base_color[1]);
        assert!(outline.color[2] > base_color[2]);
    }
}
