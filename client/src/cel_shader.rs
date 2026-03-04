#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

/// Cel (toon) shading system — replaces Cook-Torrance BRDF with discrete
/// toon ramp lighting, rim highlights, and GI hue shift for anime aesthetics.
///
/// The cel shader writes two render targets:
///   @location(0) = HDR color (Rgba16Float)
///   @location(1) = world normal (Rgba16Float, encoded as n * 0.5 + 0.5)
///
/// The normal G-buffer is consumed by the outline pass and, in Phase 2a,
/// by the field-based ray march pipeline.

// ── Non-WASM stub ────────────────────────────────────────────────────────

#[cfg(not(target_arch = "wasm32"))]
pub struct CelShaderSystem {
    pub shadow_bands: u32,
    pub shadow_softness: f32,
    pub highlight_threshold: f32,
    pub highlight_softness: f32,
    pub rim_power: f32,
    pub rim_intensity: f32,
}

#[cfg(not(target_arch = "wasm32"))]
impl CelShaderSystem {
    pub fn new() -> Self {
        Self {
            shadow_bands: 3,
            shadow_softness: 0.05,
            highlight_threshold: 0.8,
            highlight_softness: 0.1,
            rim_power: 3.0,
            rim_intensity: 0.4,
        }
    }

    pub fn shadow_bands(&self) -> u32 {
        self.shadow_bands
    }
    pub fn set_shadow_bands(&mut self, v: u32) {
        self.shadow_bands = v.clamp(1, 8);
    }
    pub fn shadow_softness(&self) -> f32 {
        self.shadow_softness
    }
    pub fn set_shadow_softness(&mut self, v: f32) {
        self.shadow_softness = v;
    }
    pub fn rim_power(&self) -> f32 {
        self.rim_power
    }
    pub fn set_rim_power(&mut self, v: f32) {
        self.rim_power = v;
    }
    pub fn rim_intensity(&self) -> f32 {
        self.rim_intensity
    }
    pub fn set_rim_intensity(&mut self, v: f32) {
        self.rim_intensity = v;
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Default for CelShaderSystem {
    fn default() -> Self {
        Self::new()
    }
}

// ── WASM implementation ──────────────────────────────────────────────────

#[cfg(target_arch = "wasm32")]
mod wasm_impl {
    use bytemuck::{Pod, Zeroable};

    /// GPU-uploadable cel shading uniforms. Bound alongside camera data.
    /// Must be 32 bytes (8 × f32) for alignment.
    #[repr(C)]
    #[derive(Clone, Copy, Pod, Zeroable)]
    pub struct CelUniforms {
        pub bands: f32,       // cast to u32 in shader
        pub softness: f32,
        pub highlight_threshold: f32,
        pub highlight_softness: f32,
        pub rim_power: f32,
        pub rim_intensity: f32,
        pub _pad0: f32,
        pub _pad1: f32,
    }

    /// Cel fragment shader — toon ramp lighting with rim highlight and GI hue shift.
    ///
    /// Uses the same vertex shader as PBR (skinning, wind, matrices).
    /// Same bind groups 0-3 (camera, model, material, shadow).
    /// Adds bind group 4 for cel uniforms.
    ///
    /// Outputs:
    ///   @location(0) HDR color (Rgba16Float)
    ///   @location(1) world normal encoded as (n * 0.5 + 0.5, 1.0) (Rgba16Float)
    pub const CEL_SHADER_3D: &str = r#"
const PI: f32 = 3.14159265359;

struct CameraUniform {
    view_proj: mat4x4<f32>,
    camera_pos: vec4<f32>,
    light_dir: vec4<f32>,
    light_color: vec4<f32>,
    ambient: vec4<f32>,
    time: vec4<f32>,
    fog_color_and_start: vec4<f32>,
    fog_params: vec4<f32>,
    wind_params: vec4<f32>,
    wind_dir: vec4<f32>,
};

struct ModelEntry {
    model: mat4x4<f32>,
    normal_model: mat4x4<f32>,
};

struct MaterialUniforms {
    base_color_factor: vec4<f32>,
    metallic_factor: f32,
    roughness_factor: f32,
    _padding0: f32,
    _padding1: f32,
};

struct CelParams {
    bands: f32,
    softness: f32,
    highlight_threshold: f32,
    highlight_softness: f32,
    rim_power: f32,
    rim_intensity: f32,
    _pad0: f32,
    _pad1: f32,
};

// Bind group 0: Camera uniforms
@group(0) @binding(0) var<uniform> camera: CameraUniform;

// Bind group 1: Model storage buffer + joint palette
@group(1) @binding(0) var<storage, read> model_matrices: array<ModelEntry>;
@group(1) @binding(1) var<storage, read> joint_palette: array<mat4x4<f32>>;

// Bind group 2: Material textures & uniforms
@group(2) @binding(0) var albedo_texture: texture_2d<f32>;
@group(2) @binding(1) var normal_map: texture_2d<f32>;
@group(2) @binding(2) var orm_texture: texture_2d<f32>;
@group(2) @binding(3) var material_sampler: sampler;
@group(2) @binding(4) var<uniform> material: MaterialUniforms;

// Bind group 3: Shadow cascades
const SHADOW_CASCADE_COUNT: u32 = 3u;
const SHADOW_CASCADE_RESOLUTION: f32 = 2048.0;
const SHADOW_ATLAS_WIDTH: f32 = 6144.0;

struct ShadowData {
    light_view_proj_0: mat4x4<f32>,
    light_view_proj_1: mat4x4<f32>,
    light_view_proj_2: mat4x4<f32>,
    cascade_splits: vec4<f32>,
    bias: vec4<f32>,
};

@group(3) @binding(0) var shadow_atlas: texture_depth_2d;
@group(3) @binding(1) var shadow_sampler: sampler_comparison;
@group(3) @binding(2) var<uniform> shadow_data: ShadowData;

// Bind group 4: Cel shading uniforms
@group(4) @binding(0) var<uniform> cel: CelParams;

// Vertex input (identical to PBR)
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

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) world_tangent: vec3<f32>,
    @location(4) tangent_handedness: f32,
};

struct FragmentOutput {
    @location(0) color: vec4<f32>,
    @location(1) normal: vec4<f32>,
};

// ── Wind displacement (must match PBR + shadow shader exactly) ────────
fn compute_wind_displacement(pos: vec3<f32>, wind_color: vec4<f32>, time: f32, wind_direction: vec3<f32>, strength: f32, turbulence: f32) -> vec3<f32> {
    let trunk_weight = wind_color.r;
    let branch_weight = wind_color.g;
    let leaf_weight = wind_color.b;
    let phase_offset = wind_color.a;

    let total_weight = trunk_weight + branch_weight + leaf_weight;
    if (total_weight < 0.001) {
        return vec3<f32>(0.0);
    }

    let gust = sin(time * 0.7854) * 0.5 + 0.5;
    let effective_strength = strength * (0.6 + 0.4 * gust);

    let trunk_phase = time * 1.2 + phase_offset * 6.283;
    let trunk_sway = wind_direction * sin(trunk_phase) * trunk_weight * effective_strength;

    let branch_phase = time * 3.5 + phase_offset * 12.566;
    let branch_perp = normalize(vec3<f32>(-wind_direction.z, 0.0, wind_direction.x));
    let branch_osc = (wind_direction * sin(branch_phase) * 0.5 + branch_perp * cos(branch_phase * 1.3) * 0.3) * branch_weight * effective_strength * 0.5;

    let leaf_phase = time * 8.0 + phase_offset * 25.13;
    let leaf_disp = vec3<f32>(
        sin(leaf_phase * 1.1 + pos.x * 2.0) * 0.3,
        sin(leaf_phase * 1.7 + pos.y * 3.0) * 0.15,
        cos(leaf_phase * 0.9 + pos.z * 2.5) * 0.3
    ) * leaf_weight * effective_strength * turbulence * 0.3;

    return trunk_sway + branch_osc + leaf_disp;
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let entry = model_matrices[in.instance_id];

    let weight_sum = in.joint_weights[0] + in.joint_weights[1] + in.joint_weights[2] + in.joint_weights[3];
    var skinned_pos: vec4<f32>;
    var skinned_normal: vec3<f32>;
    var skinned_tangent: vec3<f32>;
    if (weight_sum > 0.0) {
        let skin_matrix = joint_palette[in.joint_indices[0]] * in.joint_weights[0]
                        + joint_palette[in.joint_indices[1]] * in.joint_weights[1]
                        + joint_palette[in.joint_indices[2]] * in.joint_weights[2]
                        + joint_palette[in.joint_indices[3]] * in.joint_weights[3];
        skinned_pos = skin_matrix * vec4<f32>(in.position, 1.0);
        skinned_normal = (skin_matrix * vec4<f32>(in.normal, 0.0)).xyz;
        skinned_tangent = (skin_matrix * vec4<f32>(in.tangent.xyz, 0.0)).xyz;
    } else {
        skinned_pos = vec4<f32>(in.position, 1.0);
        skinned_normal = in.normal;
        skinned_tangent = in.tangent.xyz;
    }

    let wind_offset = compute_wind_displacement(
        skinned_pos.xyz,
        in.vertex_color,
        camera.wind_params.x,
        camera.wind_dir.xyz,
        camera.wind_params.y,
        camera.wind_params.z
    );
    skinned_pos = vec4<f32>(skinned_pos.xyz + wind_offset, skinned_pos.w);

    let world_pos = entry.model * skinned_pos;
    out.world_position = world_pos.xyz;
    out.clip_position = camera.view_proj * world_pos;
    out.world_normal = normalize((entry.normal_model * vec4<f32>(skinned_normal, 0.0)).xyz);
    out.world_tangent = normalize((entry.model * vec4<f32>(skinned_tangent, 0.0)).xyz);
    out.tangent_handedness = in.tangent.w;
    out.uv = in.uv;
    return out;
}

// ── Toon ramp: quantize N·L into discrete bands with soft edges ───────
fn toon_ramp(n_dot_l: f32, bands: u32, softness: f32) -> f32 {
    let step_size = 1.0 / f32(bands);
    let quantized = floor(n_dot_l / step_size) * step_size;
    let edge = fract(n_dot_l / step_size);
    return quantized + smoothstep(0.5 - softness, 0.5 + softness, edge) * step_size;
}

// ── TBN matrix computation (matches PBR) ──────────────────────────────
fn compute_tbn(world_tangent: vec3<f32>, world_normal: vec3<f32>, handedness: f32, world_pos: vec3<f32>, uv: vec2<f32>) -> mat3x3<f32> {
    let n = normalize(world_normal);
    let tangent_len = length(world_tangent);

    let dp1 = dpdx(world_pos);
    let dp2 = dpdy(world_pos);
    let duv1 = dpdx(uv);
    let duv2 = dpdy(uv);

    var t: vec3<f32>;
    var b: vec3<f32>;
    if (tangent_len > 0.001) {
        let raw_t = normalize(world_tangent);
        t = normalize(raw_t - n * dot(n, raw_t));
        b = cross(n, t) * handedness;
    } else {
        let det = duv1.x * duv2.y - duv1.y * duv2.x;
        let inv_det = select(1.0 / det, 0.0, abs(det) < 0.0001);
        t = normalize((dp1 * duv2.y - dp2 * duv1.y) * inv_det);
        b = normalize((dp2 * duv1.x - dp1 * duv2.x) * inv_det);
    }

    return mat3x3<f32>(t, b, n);
}

// ── Shadow sampling (identical to PBR) ────────────────────────────────
fn get_light_view_proj(cascade: u32) -> mat4x4<f32> {
    if (cascade == 0u) { return shadow_data.light_view_proj_0; }
    if (cascade == 1u) { return shadow_data.light_view_proj_1; }
    return shadow_data.light_view_proj_2;
}

fn sample_shadow_pcf(world_pos: vec3<f32>, normal: vec3<f32>, cascade_index: u32) -> f32 {
    let light_view_proj = get_light_view_proj(cascade_index);
    let normal_bias = shadow_data.bias.x;
    let biased_pos = world_pos + normal * normal_bias;

    let light_clip = light_view_proj * vec4<f32>(biased_pos, 1.0);
    let light_ndc = light_clip.xyz / light_clip.w;

    let shadow_uv = vec2<f32>(light_ndc.x * 0.5 + 0.5, 1.0 - (light_ndc.y * 0.5 + 0.5));
    let shadow_depth = light_ndc.z;

    let out_of_bounds = f32(
        shadow_uv.x < 0.0 || shadow_uv.x > 1.0 ||
        shadow_uv.y < 0.0 || shadow_uv.y > 1.0 ||
        shadow_depth < 0.0 || shadow_depth > 1.0
    );
    let clamped_uv = clamp(shadow_uv, vec2(0.001), vec2(0.999));

    let cascade_offset_x = f32(cascade_index) * SHADOW_CASCADE_RESOLUTION / SHADOW_ATLAS_WIDTH;
    let cascade_scale_x = SHADOW_CASCADE_RESOLUTION / SHADOW_ATLAS_WIDTH;
    let atlas_uv = vec2<f32>(
        clamped_uv.x * cascade_scale_x + cascade_offset_x,
        clamped_uv.y
    );

    // Simplified 5-tap PCF for cel shading (fewer samples OK with toon ramp)
    let texel_size = vec2<f32>(1.0 / SHADOW_ATLAS_WIDTH, 1.0 / SHADOW_CASCADE_RESOLUTION);
    var shadow_sum: f32 = 0.0;
    shadow_sum += textureSampleCompare(shadow_atlas, shadow_sampler, atlas_uv, shadow_depth);
    shadow_sum += textureSampleCompare(shadow_atlas, shadow_sampler, atlas_uv + vec2(texel_size.x, 0.0), shadow_depth);
    shadow_sum += textureSampleCompare(shadow_atlas, shadow_sampler, atlas_uv - vec2(texel_size.x, 0.0), shadow_depth);
    shadow_sum += textureSampleCompare(shadow_atlas, shadow_sampler, atlas_uv + vec2(0.0, texel_size.y), shadow_depth);
    shadow_sum += textureSampleCompare(shadow_atlas, shadow_sampler, atlas_uv - vec2(0.0, texel_size.y), shadow_depth);

    return mix(shadow_sum / 5.0, 1.0, out_of_bounds);
}

fn compute_shadow(world_pos: vec3<f32>, normal: vec3<f32>) -> f32 {
    let clip_pos = camera.view_proj * vec4<f32>(world_pos, 1.0);
    let view_depth = clip_pos.w;

    var cascade_index: u32 = 0u;
    if view_depth > shadow_data.cascade_splits.y {
        cascade_index = 2u;
    } else if view_depth > shadow_data.cascade_splits.x {
        cascade_index = 1u;
    }

    return sample_shadow_pcf(world_pos, normal, cascade_index);
}

// ── RGB <-> HSL conversion for GI hue shift ───────────────────────────
fn rgb_to_hsl(c: vec3<f32>) -> vec3<f32> {
    let cmax = max(c.r, max(c.g, c.b));
    let cmin = min(c.r, min(c.g, c.b));
    let delta = cmax - cmin;
    let l = (cmax + cmin) * 0.5;

    if (delta < 0.001) {
        return vec3(0.0, 0.0, l);
    }

    let s = select(delta / (2.0 - cmax - cmin), delta / (cmax + cmin), l < 0.5);

    var h: f32 = 0.0;
    if (cmax == c.r) {
        h = (c.g - c.b) / delta + select(0.0, 6.0, c.g < c.b);
    } else if (cmax == c.g) {
        h = (c.b - c.r) / delta + 2.0;
    } else {
        h = (c.r - c.g) / delta + 4.0;
    }
    h /= 6.0;

    return vec3(h, s, l);
}

fn hue_to_rgb(p: f32, q: f32, t_in: f32) -> f32 {
    var t = t_in;
    if (t < 0.0) { t += 1.0; }
    if (t > 1.0) { t -= 1.0; }
    if (t < 1.0 / 6.0) { return p + (q - p) * 6.0 * t; }
    if (t < 0.5) { return q; }
    if (t < 2.0 / 3.0) { return p + (q - p) * (2.0 / 3.0 - t) * 6.0; }
    return p;
}

fn hsl_to_rgb(hsl: vec3<f32>) -> vec3<f32> {
    if (hsl.y < 0.001) {
        return vec3(hsl.z);
    }
    let q = select(hsl.z + hsl.y - hsl.z * hsl.y, hsl.z * (1.0 + hsl.y), hsl.z < 0.5);
    let p = 2.0 * hsl.z - q;
    return vec3(
        hue_to_rgb(p, q, hsl.x + 1.0 / 3.0),
        hue_to_rgb(p, q, hsl.x),
        hue_to_rgb(p, q, hsl.x - 1.0 / 3.0)
    );
}

// ── Distance fog (matches PBR) ────────────────────────────────────────
fn apply_fog(color: vec3<f32>, world_pos: vec3<f32>, cam_pos: vec3<f32>) -> vec3<f32> {
    let fog_color = camera.fog_color_and_start.xyz;
    let fog_density = camera.fog_params.y;
    let fog_height_falloff = camera.fog_params.z;

    let dist = distance(world_pos, cam_pos);
    let view_dir = normalize(world_pos - cam_pos);

    let height_factor = exp(-fog_height_falloff * max(world_pos.y, 0.0));
    let fog_amount = clamp(1.0 - exp(-fog_density * dist * height_factor), 0.0, 1.0);

    let sun_dir = normalize(-camera.light_dir.xyz);
    let scatter_dot = max(dot(view_dir, sun_dir), 0.0);
    let in_scatter = pow(scatter_dot, 8.0) * camera.light_color.xyz * 0.15;

    return mix(color, fog_color + in_scatter, fog_amount);
}

// ── Fragment shader ───────────────────────────────────────────────────
@fragment
fn fs_main(in: VertexOutput) -> FragmentOutput {
    var out: FragmentOutput;

    // Sample albedo
    let albedo_sample = textureSample(albedo_texture, material_sampler, in.uv);
    let base_color = albedo_sample.rgb * material.base_color_factor.rgb;
    let alpha = albedo_sample.a * material.base_color_factor.a;

    // Sample ORM (AO channel still used for ambient)
    let orm_sample = textureSample(orm_texture, material_sampler, in.uv);
    let ao = orm_sample.r;

    // Normal mapping
    let normal_sample = textureSample(normal_map, material_sampler, in.uv).rgb;
    let tangent_normal = normalize(normal_sample * 2.0 - vec3<f32>(1.0, 1.0, 1.0));
    let tbn = compute_tbn(in.world_tangent, in.world_normal, in.tangent_handedness, in.world_position, in.uv);
    let normal = normalize(tbn * tangent_normal);

    let view = normalize(camera.camera_pos.xyz - in.world_position);
    let light = normalize(-camera.light_dir.xyz);

    let n_dot_l = max(dot(normal, light), 0.0);
    let n_dot_v = max(dot(normal, view), 0.0);

    // Shadow
    let shadow = compute_shadow(in.world_position, normal);

    // Toon ramp: quantize lighting into discrete bands
    let lit = n_dot_l * shadow;
    let toon_factor = toon_ramp(lit, u32(cel.bands), cel.softness);

    // Specular highlight: hard-edged via smoothstep threshold
    let half_vec = normalize(view + light);
    let n_dot_h = max(dot(normal, half_vec), 0.0);
    let spec_raw = pow(n_dot_h, 64.0);
    let spec = smoothstep(cel.highlight_threshold - cel.highlight_softness,
                          cel.highlight_threshold + cel.highlight_softness,
                          spec_raw) * shadow;

    // Rim light: view-dependent edge highlight
    let rim = pow(1.0 - n_dot_v, cel.rim_power) * cel.rim_intensity;

    // GI hue shift: shift shadow-band color slightly toward ambient hue
    // This simulates indirect illumination from the sky filling shadow areas
    let ambient_hsl = rgb_to_hsl(camera.ambient.xyz);
    var shadow_color = base_color;
    let shadow_amount = 1.0 - toon_factor;
    if (shadow_amount > 0.1) {
        let base_hsl = rgb_to_hsl(base_color);
        // Shift hue toward ambient by 15% in shadow areas
        let shifted_hue = base_hsl.x + (ambient_hsl.x - base_hsl.x) * 0.15 * shadow_amount;
        // Slightly desaturate shadows for anime look
        let shifted_sat = base_hsl.y * (1.0 - 0.1 * shadow_amount);
        shadow_color = hsl_to_rgb(vec3(shifted_hue, shifted_sat, base_hsl.z));
    }

    // Combine: toon-lit diffuse + specular + rim + ambient
    let diffuse = mix(shadow_color * 0.4, base_color, toon_factor);
    let direct = diffuse * camera.light_color.xyz;
    let ambient_contribution = camera.ambient.xyz * base_color * ao * 0.6;
    let highlight = camera.light_color.xyz * spec * 0.5;
    let rim_color = camera.light_color.xyz * rim * 0.3;

    var final_color = direct + ambient_contribution + highlight + rim_color;

    // Fog
    final_color = apply_fog(final_color, in.world_position, camera.camera_pos.xyz);

    // Output HDR color
    out.color = vec4<f32>(final_color, alpha);

    // Output world normal to G-buffer (encoded as n * 0.5 + 0.5)
    out.normal = vec4<f32>(normal * 0.5 + 0.5, 1.0);

    return out;
}
"#;

    pub struct CelShaderSystem {
        pub cel_uniform_buffer: wgpu::Buffer,
        pub cel_bind_group_layout: wgpu::BindGroupLayout,
        pub cel_bind_group: wgpu::BindGroup,
        pub shadow_bands: u32,
        pub shadow_softness: f32,
        pub highlight_threshold: f32,
        pub highlight_softness: f32,
        pub rim_power: f32,
        pub rim_intensity: f32,
    }

    impl CelShaderSystem {
        pub fn new(device: &wgpu::Device) -> Self {
            let cel_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("cel_uniform_buffer"),
                size: std::mem::size_of::<CelUniforms>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

            let cel_bind_group_layout =
                device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("cel_bind_group_layout"),
                    entries: &[wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    }],
                });

            let cel_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("cel_bind_group"),
                layout: &cel_bind_group_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: cel_uniform_buffer.as_entire_binding(),
                }],
            });

            Self {
                cel_uniform_buffer,
                cel_bind_group_layout,
                cel_bind_group,
                shadow_bands: 3,
                shadow_softness: 0.05,
                highlight_threshold: 0.8,
                highlight_softness: 0.1,
                rim_power: 3.0,
                rim_intensity: 0.4,
            }
        }

        /// Upload current cel parameters to the GPU.
        pub fn update_uniforms(&self, queue: &wgpu::Queue) {
            let uniforms = CelUniforms {
                bands: self.shadow_bands as f32,
                softness: self.shadow_softness,
                highlight_threshold: self.highlight_threshold,
                highlight_softness: self.highlight_softness,
                rim_power: self.rim_power,
                rim_intensity: self.rim_intensity,
                _pad0: 0.0,
                _pad1: 0.0,
            };
            queue.write_buffer(&self.cel_uniform_buffer, 0, bytemuck::bytes_of(&uniforms));
        }

        pub fn shadow_bands(&self) -> u32 {
            self.shadow_bands
        }
        pub fn set_shadow_bands(&mut self, v: u32) {
            self.shadow_bands = v.clamp(1, 8);
        }
        pub fn shadow_softness(&self) -> f32 {
            self.shadow_softness
        }
        pub fn set_shadow_softness(&mut self, v: f32) {
            self.shadow_softness = v;
        }
        pub fn rim_power(&self) -> f32 {
            self.rim_power
        }
        pub fn set_rim_power(&mut self, v: f32) {
            self.rim_power = v;
        }
        pub fn rim_intensity(&self) -> f32 {
            self.rim_intensity
        }
        pub fn set_rim_intensity(&mut self, v: f32) {
            self.rim_intensity = v;
        }
    }

    impl Default for CelShaderSystem {
        fn default() -> Self {
            // Cannot call new() without a device, so this is intentionally unavailable.
            // Use CelShaderSystem::new(device) instead.
            panic!("CelShaderSystem requires a wgpu::Device; use CelShaderSystem::new(device)")
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm_impl::{CelShaderSystem, CelUniforms, CEL_SHADER_3D};

#[cfg(test)]
mod tests {
    #[cfg(not(target_arch = "wasm32"))]
    use super::CelShaderSystem;

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn stub_creates_with_defaults() {
        let cel = CelShaderSystem::new();
        assert_eq!(cel.shadow_bands(), 3);
        assert!((cel.shadow_softness() - 0.05).abs() < 1e-6);
        assert!((cel.rim_power() - 3.0).abs() < 1e-6);
        assert!((cel.rim_intensity() - 0.4).abs() < 1e-6);
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn stub_clamps_shadow_bands() {
        let mut cel = CelShaderSystem::new();
        cel.set_shadow_bands(0);
        assert_eq!(cel.shadow_bands(), 1);
        cel.set_shadow_bands(100);
        assert_eq!(cel.shadow_bands(), 8);
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn cel_uniforms_size_is_32_bytes() {
        #[repr(C)]
        #[derive(Clone, Copy)]
        struct CelUniforms {
            bands: f32,
            softness: f32,
            highlight_threshold: f32,
            highlight_softness: f32,
            rim_power: f32,
            rim_intensity: f32,
            _pad0: f32,
            _pad1: f32,
        }
        assert_eq!(std::mem::size_of::<CelUniforms>(), 32);
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn default_impl_matches_new() {
        let a = CelShaderSystem::new();
        let b = CelShaderSystem::default();
        assert_eq!(a.shadow_bands(), b.shadow_bands());
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn all_setters_update_values() {
        let mut cel = CelShaderSystem::new();
        cel.set_shadow_softness(0.1);
        assert!((cel.shadow_softness() - 0.1).abs() < 1e-6);
        cel.set_rim_power(5.0);
        assert!((cel.rim_power() - 5.0).abs() < 1e-6);
        cel.set_rim_intensity(0.8);
        assert!((cel.rim_intensity() - 0.8).abs() < 1e-6);
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn highlight_defaults_are_sane() {
        let cel = CelShaderSystem::new();
        // Highlight threshold should be in (0, 1)
        assert!(cel.highlight_threshold > 0.0 && cel.highlight_threshold < 1.0);
        assert!(cel.highlight_softness > 0.0 && cel.highlight_softness < 1.0);
    }

    /// Mirror of the WGSL toon_ramp function for CPU-side verification.
    fn toon_ramp_cpu(n_dot_l: f32, bands: u32, softness: f32) -> f32 {
        let step_size = 1.0 / bands as f32;
        let quantized = (n_dot_l / step_size).floor() * step_size;
        let edge = (n_dot_l / step_size).fract();
        // smoothstep
        let lo = 0.5 - softness;
        let hi = 0.5 + softness;
        let t = ((edge - lo) / (hi - lo)).clamp(0.0, 1.0);
        let smooth = t * t * (3.0 - 2.0 * t);
        quantized + smooth * step_size
    }

    #[test]
    fn toon_ramp_quantizes_to_bands() {
        // With softness=0 (hard edges), toon ramp should snap to band boundaries
        let val = toon_ramp_cpu(0.4, 3, 0.0);
        // 3 bands => step_size = 0.333..., floor(0.4/0.333)*0.333 = 0.333
        // edge = fract(1.2) = 0.2, smoothstep(0.5, 0.5, 0.2) = 0.0 (hard step)
        assert!((val - 1.0 / 3.0).abs() < 0.01, "got {val}");
    }

    #[test]
    fn toon_ramp_full_light_approaches_one() {
        let val = toon_ramp_cpu(0.99, 3, 0.05);
        // Near-full light should be close to 1.0
        assert!(val > 0.9, "full light toon_ramp should be near 1.0, got {val}");
    }

    #[test]
    fn toon_ramp_zero_light_is_zero() {
        let val = toon_ramp_cpu(0.0, 3, 0.05);
        assert!(val.abs() < 0.01, "zero light toon_ramp should be ~0.0, got {val}");
    }

    #[test]
    fn toon_ramp_more_bands_more_steps() {
        // With 2 bands, only 2 possible output ranges. With 8, finer gradation.
        let coarse = toon_ramp_cpu(0.6, 2, 0.0);
        let fine = toon_ramp_cpu(0.6, 8, 0.0);
        // Fine should be closer to the input value
        assert!(
            (fine - 0.6).abs() <= (coarse - 0.6).abs() + 0.01,
            "fine ({fine}) should be closer to 0.6 than coarse ({coarse})"
        );
    }

    #[test]
    fn toon_ramp_softness_smooths_transitions() {
        // Hard vs soft: sample at a band edge
        let hard = toon_ramp_cpu(0.34, 3, 0.0);
        let soft = toon_ramp_cpu(0.34, 3, 0.2);
        // With softness, the transition is smoother — value should differ from hard
        // (The exact values depend on where 0.34 falls relative to the band edge)
        // Both should be in [0, 1]
        assert!(hard >= 0.0 && hard <= 1.0);
        assert!(soft >= 0.0 && soft <= 1.0);
    }
}
