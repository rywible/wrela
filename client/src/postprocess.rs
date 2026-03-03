#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

/// Number of bloom mip levels in the downsample/upsample chain.
pub const BLOOM_MIP_COUNT: usize = 6;

#[cfg(not(target_arch = "wasm32"))]
pub struct PostProcessStack {
    bloom_intensity: f32,
    exposure: f32,
    bloom_threshold: f32,
    god_rays_intensity: f32,
    taa_enabled: bool,
    taa_frame_index: u32,
}

#[cfg(not(target_arch = "wasm32"))]
impl PostProcessStack {
    pub fn new() -> Self {
        Self {
            bloom_intensity: 0.0,
            exposure: 1.0,
            bloom_threshold: 1.0,
            god_rays_intensity: 0.0,
            taa_enabled: true,
            taa_frame_index: 0,
        }
    }
    pub fn bloom_intensity(&self) -> f32 {
        self.bloom_intensity
    }
    pub fn set_bloom_intensity(&mut self, v: f32) {
        self.bloom_intensity = v;
    }
    pub fn exposure(&self) -> f32 {
        self.exposure
    }
    pub fn set_exposure(&mut self, v: f32) {
        self.exposure = v;
    }
    pub fn bloom_threshold(&self) -> f32 {
        self.bloom_threshold
    }
    pub fn set_bloom_threshold(&mut self, v: f32) {
        self.bloom_threshold = v;
    }
    pub fn god_rays_intensity(&self) -> f32 {
        self.god_rays_intensity
    }
    pub fn set_god_rays_intensity(&mut self, v: f32) {
        self.god_rays_intensity = v;
    }
    pub fn taa_enabled(&self) -> bool {
        self.taa_enabled
    }
    pub fn set_taa_enabled(&mut self, v: bool) {
        self.taa_enabled = v;
    }
    pub fn taa_frame_index(&self) -> u32 {
        self.taa_frame_index
    }
    pub fn advance_taa_frame(&mut self) {
        self.taa_frame_index = self.taa_frame_index.wrapping_add(1);
    }
    pub fn resize(&mut self, _width: u32, _height: u32) {}
}

#[cfg(target_arch = "wasm32")]
mod wasm_impl {
    use super::BLOOM_MIP_COUNT;
    use bytemuck::{Pod, Zeroable};

    /// Total number of bloom passes: BLOOM_MIP_COUNT downsamples + (BLOOM_MIP_COUNT - 1) upsamples.
    const BLOOM_PASS_COUNT: usize = BLOOM_MIP_COUNT + BLOOM_MIP_COUNT - 1;

    // ---------------------------------------------------------------------------
    // WGSL shaders
    // ---------------------------------------------------------------------------

    /// Bloom downsample with 13-tap filter and brightness threshold extraction.
    /// The first downsample pass (from HDR source) applies the brightness threshold.
    /// Subsequent passes just downsample without thresholding.
    /// `params.bloom_threshold` > 0 signals threshold extraction is active for this pass.
    const BLOOM_DOWNSAMPLE_SHADER: &str = r#"
struct BloomParams {
    texel_size_x: f32,
    texel_size_y: f32,
    bloom_threshold: f32,
    bloom_intensity: f32,
};

@group(0) @binding(0) var source_texture: texture_2d<f32>;
@group(0) @binding(1) var source_sampler: sampler;
@group(0) @binding(2) var<uniform> params: BloomParams;

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

fn extract_bright(color: vec3<f32>, threshold: f32) -> vec3<f32> {
    let brightness = max(color.r, max(color.g, color.b));
    let contribution = max(brightness - threshold, 0.0) / max(brightness, 0.001);
    return color * contribution;
}

@fragment fn fs(in: VsOut) -> @location(0) vec4<f32> {
    let tx = vec2(params.texel_size_x, params.texel_size_y);

    // 13-tap downsampling filter (Jorge Jimenez, Call of Duty: Advanced Warfare)
    // Sample pattern on source texture:
    //   a . b . c
    //   . d . e .
    //   f . g . h
    //   . i . j .
    //   k . l . m
    let a = textureSample(source_texture, source_sampler, in.uv + vec2(-2.0, -2.0) * tx).rgb;
    let b = textureSample(source_texture, source_sampler, in.uv + vec2( 0.0, -2.0) * tx).rgb;
    let c = textureSample(source_texture, source_sampler, in.uv + vec2( 2.0, -2.0) * tx).rgb;
    let d = textureSample(source_texture, source_sampler, in.uv + vec2(-1.0, -1.0) * tx).rgb;
    let e = textureSample(source_texture, source_sampler, in.uv + vec2( 1.0, -1.0) * tx).rgb;
    let f = textureSample(source_texture, source_sampler, in.uv + vec2(-2.0,  0.0) * tx).rgb;
    let g = textureSample(source_texture, source_sampler, in.uv).rgb;
    let h = textureSample(source_texture, source_sampler, in.uv + vec2( 2.0,  0.0) * tx).rgb;
    let i = textureSample(source_texture, source_sampler, in.uv + vec2(-1.0,  1.0) * tx).rgb;
    let j = textureSample(source_texture, source_sampler, in.uv + vec2( 1.0,  1.0) * tx).rgb;
    let k = textureSample(source_texture, source_sampler, in.uv + vec2(-2.0,  2.0) * tx).rgb;
    let l = textureSample(source_texture, source_sampler, in.uv + vec2( 0.0,  2.0) * tx).rgb;
    let m = textureSample(source_texture, source_sampler, in.uv + vec2( 2.0,  2.0) * tx).rgb;

    // 5 overlapping 2x2 box filters, weights sum to 1.0:
    // Center box weight = 0.5, four corner boxes weight = 0.125 each
    let result = (d + e + i + j) * 0.125            // center box (0.5 / 4 samples)
               + (a + b + f + g) * 0.03125          // top-left box (0.125 / 4)
               + (b + c + g + h) * 0.03125          // top-right box
               + (f + g + k + l) * 0.03125          // bottom-left box
               + (g + h + l + m) * 0.03125;         // bottom-right box

    // Apply brightness threshold on the first downsample pass only.
    if (params.bloom_threshold > 0.0) {
        return vec4(extract_bright(result, params.bloom_threshold), 1.0);
    }
    return vec4(result, 1.0);
}
"#;

    /// Bloom upsample with 9-tap tent filter and additive blending.
    /// The pipeline uses additive blend state so the output is added to the
    /// destination (the next-larger mip level's content from the downsample phase).
    const BLOOM_UPSAMPLE_SHADER: &str = r#"
struct BloomParams {
    texel_size_x: f32,
    texel_size_y: f32,
    bloom_threshold: f32,
    bloom_intensity: f32,
};

@group(0) @binding(0) var source_texture: texture_2d<f32>;
@group(0) @binding(1) var source_sampler: sampler;
@group(0) @binding(2) var<uniform> params: BloomParams;

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

@fragment fn fs(in: VsOut) -> @location(0) vec4<f32> {
    let tx = vec2(params.texel_size_x, params.texel_size_y);

    // 9-tap tent filter for smooth upsampling
    // Kernel weights:
    // 1 2 1
    // 2 4 2  / 16
    // 1 2 1
    var color = textureSample(source_texture, source_sampler, in.uv + vec2(-1.0, -1.0) * tx).rgb * 1.0;
    color += textureSample(source_texture, source_sampler, in.uv + vec2( 0.0, -1.0) * tx).rgb * 2.0;
    color += textureSample(source_texture, source_sampler, in.uv + vec2( 1.0, -1.0) * tx).rgb * 1.0;
    color += textureSample(source_texture, source_sampler, in.uv + vec2(-1.0,  0.0) * tx).rgb * 2.0;
    color += textureSample(source_texture, source_sampler, in.uv).rgb * 4.0;
    color += textureSample(source_texture, source_sampler, in.uv + vec2( 1.0,  0.0) * tx).rgb * 2.0;
    color += textureSample(source_texture, source_sampler, in.uv + vec2(-1.0,  1.0) * tx).rgb * 1.0;
    color += textureSample(source_texture, source_sampler, in.uv + vec2( 0.0,  1.0) * tx).rgb * 2.0;
    color += textureSample(source_texture, source_sampler, in.uv + vec2( 1.0,  1.0) * tx).rgb * 1.0;
    color /= 16.0;

    return vec4(color * params.bloom_intensity, 1.0);
}
"#;

    /// FXAA 3.11 – Fast Approximate Anti-Aliasing.
    /// Runs on LDR/sRGB data after tonemapping. Detects edges via luma contrast
    /// among the center pixel and its 4 axis-aligned neighbours, then blends
    /// along the dominant edge direction with sub-pixel quality 0.75.
    const FXAA_SHADER: &str = r#"
const FXAA_EDGE_THRESHOLD: f32 = 0.0625;      // 1/16
const FXAA_EDGE_THRESHOLD_MIN: f32 = 0.03125;  // 1/32
const FXAA_SUBPIX_QUALITY: f32 = 0.75;
const FXAA_SEARCH_STEPS: i32 = 10;
const FXAA_SEARCH_ACCELERATION: f32 = 1.0;

struct FxaaParams {
    rcp_frame_x: f32,
    rcp_frame_y: f32,
    _pad0: f32,
    _pad1: f32,
};

@group(0) @binding(0) var fxaa_texture: texture_2d<f32>;
@group(0) @binding(1) var fxaa_sampler: sampler;
@group(0) @binding(2) var<uniform> params: FxaaParams;

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

fn luma(color: vec3<f32>) -> f32 {
    return 0.299 * color.r + 0.587 * color.g + 0.114 * color.b;
}

@fragment fn fs(in: VsOut) -> @location(0) vec4<f32> {
    let rcp_frame = vec2(params.rcp_frame_x, params.rcp_frame_y);

    // Sample center and 4 direct neighbours
    let rgbM  = textureSampleLevel(fxaa_texture, fxaa_sampler, in.uv, 0.0).rgb;
    let rgbN  = textureSampleLevel(fxaa_texture, fxaa_sampler, in.uv + vec2( 0.0, -1.0) * rcp_frame, 0.0).rgb;
    let rgbS  = textureSampleLevel(fxaa_texture, fxaa_sampler, in.uv + vec2( 0.0,  1.0) * rcp_frame, 0.0).rgb;
    let rgbW  = textureSampleLevel(fxaa_texture, fxaa_sampler, in.uv + vec2(-1.0,  0.0) * rcp_frame, 0.0).rgb;
    let rgbE  = textureSampleLevel(fxaa_texture, fxaa_sampler, in.uv + vec2( 1.0,  0.0) * rcp_frame, 0.0).rgb;

    let lumaM = luma(rgbM);
    let lumaN = luma(rgbN);
    let lumaS = luma(rgbS);
    let lumaW = luma(rgbW);
    let lumaE = luma(rgbE);

    let range_min = min(lumaM, min(min(lumaN, lumaS), min(lumaW, lumaE)));
    let range_max = max(lumaM, max(max(lumaN, lumaS), max(lumaW, lumaE)));
    let range = range_max - range_min;

    // Early exit – no visible aliasing
    if (range < max(FXAA_EDGE_THRESHOLD_MIN, range_max * FXAA_EDGE_THRESHOLD)) {
        return vec4(rgbM, 1.0);
    }

    // Sub-pixel aliasing detection
    let lumaL = (lumaN + lumaS + lumaW + lumaE) * 0.25;
    let range_l = abs(lumaL - lumaM);
    var blend_l = max(0.0, (range_l / range) - 0.25) * (1.0 / 0.75);
    blend_l = min(FXAA_SUBPIX_QUALITY, blend_l);

    // Sample diagonal neighbours for edge direction detection
    let rgbNW = textureSampleLevel(fxaa_texture, fxaa_sampler, in.uv + vec2(-1.0, -1.0) * rcp_frame, 0.0).rgb;
    let rgbNE = textureSampleLevel(fxaa_texture, fxaa_sampler, in.uv + vec2( 1.0, -1.0) * rcp_frame, 0.0).rgb;
    let rgbSW = textureSampleLevel(fxaa_texture, fxaa_sampler, in.uv + vec2(-1.0,  1.0) * rcp_frame, 0.0).rgb;
    let rgbSE = textureSampleLevel(fxaa_texture, fxaa_sampler, in.uv + vec2( 1.0,  1.0) * rcp_frame, 0.0).rgb;

    let lumaNW = luma(rgbNW);
    let lumaNE = luma(rgbNE);
    let lumaSW = luma(rgbSW);
    let lumaSE = luma(rgbSE);

    // Determine horizontal vs vertical edge via Sobel-like filter
    let edge_horz = abs((-2.0 * lumaW) + lumaNW + lumaSW)
                  + abs((-2.0 * lumaM) + lumaN  + lumaS) * 2.0
                  + abs((-2.0 * lumaE) + lumaNE + lumaSE);
    let edge_vert = abs((-2.0 * lumaN) + lumaNW + lumaNE)
                  + abs((-2.0 * lumaM) + lumaW  + lumaE) * 2.0
                  + abs((-2.0 * lumaS) + lumaSW + lumaSE);

    let is_horizontal = edge_horz >= edge_vert;

    // Choose the side of the edge with the steepest gradient
    var luma_neg: f32;
    var luma_pos: f32;
    if (is_horizontal) {
        luma_neg = lumaN;
        luma_pos = lumaS;
    } else {
        luma_neg = lumaW;
        luma_pos = lumaE;
    }

    let gradient_neg = abs(luma_neg - lumaM);
    let gradient_pos = abs(luma_pos - lumaM);

    var step_length: f32;
    var luma_local_avg: f32;
    if (gradient_neg >= gradient_pos) {
        if (is_horizontal) {
            step_length = -rcp_frame.y;
        } else {
            step_length = -rcp_frame.x;
        }
        luma_local_avg = 0.5 * (luma_neg + lumaM);
    } else {
        if (is_horizontal) {
            step_length = rcp_frame.y;
        } else {
            step_length = rcp_frame.x;
        }
        luma_local_avg = 0.5 * (luma_pos + lumaM);
    }

    // Shift UV to the edge boundary (half a pixel step perpendicular to edge)
    var current_uv = in.uv;
    if (is_horizontal) {
        current_uv.y += step_length * 0.5;
    } else {
        current_uv.x += step_length * 0.5;
    }

    // Search along the edge in both directions
    var uv_offset: vec2<f32>;
    if (is_horizontal) {
        uv_offset = vec2(rcp_frame.x, 0.0);
    } else {
        uv_offset = vec2(0.0, rcp_frame.y);
    }

    var uv_neg = current_uv - uv_offset;
    var uv_pos = current_uv + uv_offset;
    let gradient_scaled = 0.25 * max(gradient_neg, gradient_pos);

    var luma_end_neg = luma(textureSampleLevel(fxaa_texture, fxaa_sampler, uv_neg, 0.0).rgb) - luma_local_avg;
    var luma_end_pos = luma(textureSampleLevel(fxaa_texture, fxaa_sampler, uv_pos, 0.0).rgb) - luma_local_avg;

    var reached_neg = abs(luma_end_neg) >= gradient_scaled;
    var reached_pos = abs(luma_end_pos) >= gradient_scaled;

    // Search loop
    for (var i = 1; i < FXAA_SEARCH_STEPS; i = i + 1) {
        if (!reached_neg) {
            uv_neg -= uv_offset * FXAA_SEARCH_ACCELERATION;
            luma_end_neg = luma(textureSampleLevel(fxaa_texture, fxaa_sampler, uv_neg, 0.0).rgb) - luma_local_avg;
            reached_neg = abs(luma_end_neg) >= gradient_scaled;
        }
        if (!reached_pos) {
            uv_pos += uv_offset * FXAA_SEARCH_ACCELERATION;
            luma_end_pos = luma(textureSampleLevel(fxaa_texture, fxaa_sampler, uv_pos, 0.0).rgb) - luma_local_avg;
            reached_pos = abs(luma_end_pos) >= gradient_scaled;
        }
        if (reached_neg && reached_pos) {
            break;
        }
    }

    // Compute the distance to each end
    var dist_neg: f32;
    var dist_pos: f32;
    if (is_horizontal) {
        dist_neg = in.uv.x - uv_neg.x;
        dist_pos = uv_pos.x - in.uv.x;
    } else {
        dist_neg = in.uv.y - uv_neg.y;
        dist_pos = uv_pos.y - in.uv.y;
    }

    let is_closer_neg = dist_neg < dist_pos;
    let luma_end = select(luma_end_pos, luma_end_neg, is_closer_neg);

    // If the luma at the closest end is in the same direction as the
    // local gradient, the pixel is not on the edge side; keep center color.
    if (((lumaM - luma_local_avg) < 0.0) == (luma_end < 0.0)) {
        return vec4(rgbM, 1.0);
    }

    let total_span = dist_neg + dist_pos;
    let closer_dist = min(dist_neg, dist_pos);
    var edge_blend = 0.5 - closer_dist / total_span;
    edge_blend = max(0.0, edge_blend);

    let final_blend = max(edge_blend, blend_l);

    // Blend along the edge direction
    var final_uv = in.uv;
    if (is_horizontal) {
        final_uv.y += step_length * final_blend;
    } else {
        final_uv.x += step_length * final_blend;
    }

    let result = textureSampleLevel(fxaa_texture, fxaa_sampler, final_uv, 0.0).rgb;
    return vec4(result, 1.0);
}
"#;

    /// TAA (Temporal Anti-Aliasing) resolve shader.
    ///
    /// Reads current jittered HDR color and depth, reprojects to find the
    /// history sample, applies YCoCg neighborhood clamping, and blends 10%
    /// current + 90% history. Handles disocclusion by increasing current
    /// weight when reprojected UV is out of bounds or depth differs greatly.
    const TAA_RESOLVE_SHADER: &str = r#"
struct TaaParams {
    screen_dims: vec2<f32>,
    jitter_x: f32,
    jitter_y: f32,
    // Current and previous view-projection matrices for reprojection
    curr_view_proj: mat4x4<f32>,
    prev_view_proj: mat4x4<f32>,
    inv_curr_view_proj: mat4x4<f32>,
};

@group(0) @binding(0) var current_color: texture_2d<f32>;
@group(0) @binding(1) var depth_tex: texture_depth_2d;
@group(0) @binding(2) var history_color: texture_2d<f32>;
@group(0) @binding(3) var taa_sampler: sampler;
@group(0) @binding(4) var<uniform> params: TaaParams;

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

// RGB <-> YCoCg conversion for neighborhood clamping
fn rgb_to_ycocg(rgb: vec3<f32>) -> vec3<f32> {
    let y  = 0.25 * rgb.r + 0.5 * rgb.g + 0.25 * rgb.b;
    let co = 0.5 * rgb.r - 0.5 * rgb.b;
    let cg = -0.25 * rgb.r + 0.5 * rgb.g - 0.25 * rgb.b;
    return vec3(y, co, cg);
}

fn ycocg_to_rgb(ycocg: vec3<f32>) -> vec3<f32> {
    let y  = ycocg.x;
    let co = ycocg.y;
    let cg = ycocg.z;
    let r = y + co - cg;
    let g = y + cg;
    let b = y - co - cg;
    return vec3(r, g, b);
}

@fragment fn fs(in: VsOut) -> @location(0) vec4<f32> {
    let pixel = vec2<i32>(in.position.xy);
    let dims = vec2<i32>(params.screen_dims);

    // Remove jitter from UV to get the unjittered position for the current frame
    let unjittered_uv = in.uv - vec2(params.jitter_x, params.jitter_y) / params.screen_dims;

    // Sample current color (jittered)
    let current = textureLoad(current_color, pixel, 0).rgb;

    // Sample depth and reconstruct world position for reprojection
    let depth = textureLoad(depth_tex, clamp(pixel, vec2(0), dims - vec2(1)), 0);

    // Reconstruct clip-space position
    let ndc = vec4(unjittered_uv * 2.0 - 1.0, depth, 1.0);
    let ndc_flipped = vec4(ndc.x, -ndc.y, ndc.z, 1.0);

    // World position from current inverse view-proj
    let world_h = params.inv_curr_view_proj * ndc_flipped;
    let world_pos = world_h.xyz / world_h.w;

    // Reproject to previous frame
    let prev_clip = params.prev_view_proj * vec4(world_pos, 1.0);
    let prev_ndc = prev_clip.xyz / prev_clip.w;
    let history_uv = vec2(prev_ndc.x * 0.5 + 0.5, 1.0 - (prev_ndc.y * 0.5 + 0.5));

    // Check for disocclusion: reprojected UV out of bounds
    let out_of_bounds = history_uv.x < 0.0 || history_uv.x > 1.0 ||
                        history_uv.y < 0.0 || history_uv.y > 1.0;

    // Sample history at reprojected UV
    var history = textureSampleLevel(history_color, taa_sampler, history_uv, 0.0).rgb;

    // ── YCoCg neighborhood clamping ──────────────────────────────────
    // Build a 3x3 neighborhood AABB in YCoCg space from the current frame
    var color_min = vec3(1e10);
    var color_max = vec3(-1e10);

    for (var dy: i32 = -1; dy <= 1; dy = dy + 1) {
        for (var dx: i32 = -1; dx <= 1; dx = dx + 1) {
            let sp = clamp(pixel + vec2(dx, dy), vec2(0), dims - vec2(1));
            let neighbor = textureLoad(current_color, sp, 0).rgb;
            let neighbor_ycocg = rgb_to_ycocg(neighbor);
            color_min = min(color_min, neighbor_ycocg);
            color_max = max(color_max, neighbor_ycocg);
        }
    }

    // Clamp history in YCoCg space
    let history_ycocg = rgb_to_ycocg(history);
    let clamped_ycocg = clamp(history_ycocg, color_min, color_max);
    history = ycocg_to_rgb(clamped_ycocg);

    // ── Blend ────────────────────────────────────────────────────────
    // Standard TAA: 10% current + 90% history.
    // On disocclusion, use more current frame weight.
    var blend_factor = 0.1;
    if (out_of_bounds || depth >= 1.0) {
        blend_factor = 1.0;
    }

    let resolved = mix(history, current, blend_factor);
    return vec4(max(resolved, vec3(0.0)), 1.0);
}
"#;

    /// Final tonemapping: combines HDR scene with bloom, applies ACES filmic
    /// tonemapping, exposure control, and gamma correction.
    const TONEMAP_SHADER: &str = r#"
struct PostProcessParams {
    bloom_intensity: f32,
    exposure: f32,
    screen_width: f32,
    screen_height: f32,
};

@group(0) @binding(0) var hdr_texture: texture_2d<f32>;
@group(0) @binding(1) var bloom_texture: texture_2d<f32>;
@group(0) @binding(2) var source_sampler: sampler;
@group(0) @binding(3) var<uniform> params: PostProcessParams;

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

fn aces_tonemap(color: vec3<f32>) -> vec3<f32> {
    return saturate((color * (2.51 * color + 0.03)) / (color * (2.43 * color + 0.59) + 0.14));
}

@fragment fn fs(in: VsOut) -> @location(0) vec4<f32> {
    let hdr = textureSample(hdr_texture, source_sampler, in.uv).rgb;
    let bloom = textureSample(bloom_texture, source_sampler, in.uv).rgb;
    let combined = hdr + bloom;

    // Apply exposure before tonemapping
    let exposed = combined * params.exposure;

    // ACES filmic tonemapping
    let tonemapped = aces_tonemap(exposed);

    // Gamma correction (linear -> sRGB)
    let gamma_corrected = pow(tonemapped, vec3(1.0 / 2.2));

    return vec4(gamma_corrected, 1.0);
}
"#;

    // ---------------------------------------------------------------------------
    // God Rays (Volumetric Light Shafts) shader
    // ---------------------------------------------------------------------------

    /// Radial blur god rays: projects sun position onto screen space, creates an
    /// occluder mask from the HDR scene, then performs a radial blur (16 samples)
    /// from the sun screen position. Output is additively blended onto the HDR buffer.
    const GOD_RAYS_SHADER: &str = r#"
struct GodRaysParams {
    sun_screen_pos: vec2<f32>,  // Sun position in UV space [0,1]
    intensity: f32,
    decay: f32,
    density: f32,
    weight: f32,
    num_samples: f32,
    threshold: f32,
};

@group(0) @binding(0) var source_texture: texture_2d<f32>;
@group(0) @binding(1) var source_sampler: sampler;
@group(0) @binding(2) var<uniform> params: GodRaysParams;

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

@fragment fn fs(in: VsOut) -> @location(0) vec4<f32> {
    let num_samples = i32(params.num_samples);
    let delta_uv = (in.uv - params.sun_screen_pos) * params.density / params.num_samples;

    var uv = in.uv;
    var illumination_decay = 1.0;
    var color = vec3(0.0, 0.0, 0.0);

    for (var i = 0; i < num_samples; i = i + 1) {
        uv -= delta_uv;
        // Clamp to valid UV range to avoid sampling outside texture
        let sample_uv = clamp(uv, vec2(0.001), vec2(0.999));
        var sample_color = textureSample(source_texture, source_sampler, sample_uv).rgb;

        // Threshold: only keep bright areas (sky behind occluders + sun)
        let brightness = max(sample_color.r, max(sample_color.g, sample_color.b));
        let bright_mask = smoothstep(params.threshold, params.threshold + 0.5, brightness);
        sample_color *= bright_mask;

        sample_color *= illumination_decay * params.weight;
        color += sample_color;
        illumination_decay *= params.decay;
    }

    return vec4(color * params.intensity, 1.0);
}
"#;

    /// Simple pass-through shader for compositing god rays back onto the HDR buffer.
    /// Reads from the half-res god rays texture and outputs the color (pipeline
    /// blend state handles additive compositing).
    const GOD_RAYS_COMPOSITE_SHADER: &str = r#"
struct GodRaysParams {
    sun_screen_pos: vec2<f32>,
    intensity: f32,
    decay: f32,
    density: f32,
    weight: f32,
    num_samples: f32,
    threshold: f32,
};

@group(0) @binding(0) var source_texture: texture_2d<f32>;
@group(0) @binding(1) var source_sampler: sampler;
@group(0) @binding(2) var<uniform> params: GodRaysParams;

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

@fragment fn fs(in: VsOut) -> @location(0) vec4<f32> {
    let color = textureSample(source_texture, source_sampler, in.uv).rgb;
    return vec4(color, 1.0);
}
"#;

    // ---------------------------------------------------------------------------
    // Uniform structs
    // ---------------------------------------------------------------------------

    /// God rays uniform: sun screen-space position and tuning parameters.
    #[repr(C)]
    #[derive(Clone, Copy, Pod, Zeroable)]
    pub(crate) struct GodRaysUniforms {
        pub sun_screen_pos: [f32; 2],
        pub intensity: f32,
        pub decay: f32,
        pub density: f32,
        pub weight: f32,
        pub num_samples: f32,
        pub threshold: f32,
    }

    /// Per-bloom-pass uniform: carries the texel size of the *source* texture
    /// for the current downsample/upsample pass, plus threshold/intensity.
    #[repr(C)]
    #[derive(Clone, Copy, Pod, Zeroable)]
    struct BloomPassUniforms {
        texel_size_x: f32,
        texel_size_y: f32,
        bloom_threshold: f32,
        bloom_intensity: f32,
    }

    /// Final tonemap pass uniform.
    #[repr(C)]
    #[derive(Clone, Copy, Pod, Zeroable)]
    pub(crate) struct PostProcessUniforms {
        pub bloom_intensity: f32,
        pub exposure: f32,
        pub screen_width: f32,
        pub screen_height: f32,
    }

    /// FXAA pass uniform: carries reciprocal frame dimensions.
    #[repr(C)]
    #[derive(Clone, Copy, Pod, Zeroable)]
    struct FxaaUniforms {
        rcp_frame_x: f32,
        rcp_frame_y: f32,
        _pad0: f32,
        _pad1: f32,
    }

    /// TAA resolve pass uniform.
    #[repr(C)]
    #[derive(Clone, Copy, Pod, Zeroable)]
    struct TaaUniforms {
        screen_dims: [f32; 2],
        jitter_x: f32,
        jitter_y: f32,
        curr_view_proj: [[f32; 4]; 4],
        prev_view_proj: [[f32; 4]; 4],
        inv_curr_view_proj: [[f32; 4]; 4],
    }

    // ---------------------------------------------------------------------------
    // Texture helpers
    // ---------------------------------------------------------------------------

    fn create_hdr_texture(
        device: &wgpu::Device,
        width: u32,
        height: u32,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("hdr_render_target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        (texture, view)
    }

    /// Creates the bloom mip chain: BLOOM_MIP_COUNT textures at successively
    /// halved resolutions (starting from width/2 x height/2).
    fn create_bloom_mip_chain(
        device: &wgpu::Device,
        width: u32,
        height: u32,
    ) -> (Vec<wgpu::Texture>, Vec<wgpu::TextureView>, Vec<(u32, u32)>) {
        let mut textures = Vec::with_capacity(BLOOM_MIP_COUNT);
        let mut views = Vec::with_capacity(BLOOM_MIP_COUNT);
        let mut sizes = Vec::with_capacity(BLOOM_MIP_COUNT);

        let mut w = (width / 2).max(1);
        let mut h = (height / 2).max(1);

        for level in 0..BLOOM_MIP_COUNT {
            let tex = device.create_texture(&wgpu::TextureDescriptor {
                label: Some(&format!("bloom_mip_{level}")),
                size: wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba16Float,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
            sizes.push((w, h));
            textures.push(tex);
            views.push(view);

            w = (w / 2).max(1);
            h = (h / 2).max(1);
        }

        (textures, views, sizes)
    }

    /// Creates the intermediate LDR texture that sits between tonemap and FXAA.
    /// Tonemap writes tonemapped sRGB data here; FXAA reads it and writes to
    /// the final swap-chain surface.
    fn create_fxaa_intermediate_texture(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("fxaa_intermediate"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        (texture, view)
    }

    /// Creates the TAA history and resolved textures (same format as HDR target).
    fn create_taa_textures(
        device: &wgpu::Device,
        width: u32,
        height: u32,
    ) -> (wgpu::Texture, wgpu::TextureView, wgpu::Texture, wgpu::TextureView) {
        let create = |label: &str| {
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
                format: wgpu::TextureFormat::Rgba16Float,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC
                    | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            (texture, view)
        };
        let (history_tex, history_view) = create("taa_history");
        let (resolved_tex, resolved_view) = create("taa_resolved");
        (history_tex, history_view, resolved_tex, resolved_view)
    }

    // ---------------------------------------------------------------------------
    // Bind group layouts
    // ---------------------------------------------------------------------------

    fn create_taa_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("taa_bind_group_layout"),
            entries: &[
                // current HDR color
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
                // depth texture
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
                // history texture
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
                // TAA uniforms
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
        })
    }

    fn create_taa_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        current_hdr_view: &wgpu::TextureView,
        depth_view: &wgpu::TextureView,
        history_view: &wgpu::TextureView,
        sampler: &wgpu::Sampler,
        uniform_buffer: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("taa_bind_group"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(current_hdr_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(depth_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(history_view),
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

    fn create_bloom_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bloom_bind_group_layout"),
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
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        })
    }

    fn create_tonemap_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("tonemap_bind_group_layout"),
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
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
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
        })
    }

    fn create_fxaa_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("fxaa_bind_group_layout"),
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
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        })
    }

    fn create_god_rays_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("god_rays_bind_group_layout"),
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
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        })
    }

    // ---------------------------------------------------------------------------
    // Pipeline creation
    // ---------------------------------------------------------------------------

    fn create_fullscreen_pipeline(
        device: &wgpu::Device,
        label: &str,
        shader_source: &str,
        bind_group_layout: &wgpu::BindGroupLayout,
        target_format: wgpu::TextureFormat,
        blend: Option<wgpu::BlendState>,
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
                    blend,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        })
    }

    fn create_sampler(device: &wgpu::Device) -> wgpu::Sampler {
        device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("postprocess_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        })
    }

    // ---------------------------------------------------------------------------
    // Bind group creation
    // ---------------------------------------------------------------------------

    fn create_bloom_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        source_view: &wgpu::TextureView,
        sampler: &wgpu::Sampler,
        uniform_buffer: &wgpu::Buffer,
        label: &str,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(label),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(source_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: uniform_buffer.as_entire_binding(),
                },
            ],
        })
    }

    fn create_tonemap_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        hdr_view: &wgpu::TextureView,
        bloom_view: &wgpu::TextureView,
        sampler: &wgpu::Sampler,
        uniform_buffer: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("tonemap_bind_group"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(hdr_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(bloom_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: uniform_buffer.as_entire_binding(),
                },
            ],
        })
    }

    fn create_fxaa_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        source_view: &wgpu::TextureView,
        sampler: &wgpu::Sampler,
        uniform_buffer: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("fxaa_bind_group"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(source_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: uniform_buffer.as_entire_binding(),
                },
            ],
        })
    }

    fn create_god_rays_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        source_view: &wgpu::TextureView,
        sampler: &wgpu::Sampler,
        uniform_buffer: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("god_rays_bind_group"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(source_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: uniform_buffer.as_entire_binding(),
                },
            ],
        })
    }

    /// Creates a half-resolution HDR texture for god rays processing.
    fn create_god_rays_texture(
        device: &wgpu::Device,
        width: u32,
        height: u32,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let w = (width / 2).max(1);
        let h = (height / 2).max(1);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("god_rays_texture"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        (texture, view)
    }

    // ---------------------------------------------------------------------------
    // Per-pass uniform buffers and bind groups
    // ---------------------------------------------------------------------------

    /// Create one uniform buffer per bloom pass. Each buffer holds the
    /// per-pass texel size, threshold, and intensity so that all passes can
    /// be recorded into a single command encoder without data races.
    fn create_bloom_uniform_buffers(device: &wgpu::Device) -> Vec<wgpu::Buffer> {
        let mut buffers = Vec::with_capacity(BLOOM_PASS_COUNT);
        for idx in 0..BLOOM_PASS_COUNT {
            buffers.push(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(&format!("bloom_pass_uniform_{idx}")),
                size: std::mem::size_of::<BloomPassUniforms>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
        }
        buffers
    }

    /// Build downsample bind groups: pass i reads from the previous mip level
    /// (or HDR source for i=0) and uses its own dedicated uniform buffer.
    fn build_downsample_bind_groups(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        hdr_view: &wgpu::TextureView,
        mip_views: &[wgpu::TextureView],
        sampler: &wgpu::Sampler,
        uniform_buffers: &[wgpu::Buffer],
    ) -> Vec<wgpu::BindGroup> {
        let mut groups = Vec::with_capacity(BLOOM_MIP_COUNT);
        for i in 0..BLOOM_MIP_COUNT {
            let source = if i == 0 { hdr_view } else { &mip_views[i - 1] };
            groups.push(create_bloom_bind_group(
                device,
                layout,
                source,
                sampler,
                &uniform_buffers[i],
                &format!("bloom_down_bg_{i}"),
            ));
        }
        groups
    }

    /// Build upsample bind groups: pass k reads from mip[source_mip] (smaller)
    /// and targets mip[target_mip] (larger). Each uses its own uniform buffer
    /// at offset BLOOM_MIP_COUNT + k in the uniform buffer array.
    fn build_upsample_bind_groups(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        mip_views: &[wgpu::TextureView],
        sampler: &wgpu::Sampler,
        uniform_buffers: &[wgpu::Buffer],
    ) -> Vec<wgpu::BindGroup> {
        let mut groups = Vec::with_capacity(BLOOM_MIP_COUNT - 1);
        for k in 0..(BLOOM_MIP_COUNT - 1) {
            // Upsample pass k: target_mip = N-2-k, source_mip = N-1-k
            let source_mip = BLOOM_MIP_COUNT - 1 - k;
            let buffer_idx = BLOOM_MIP_COUNT + k;
            groups.push(create_bloom_bind_group(
                device,
                layout,
                &mip_views[source_mip],
                sampler,
                &uniform_buffers[buffer_idx],
                &format!("bloom_up_bg_{}", BLOOM_MIP_COUNT - 2 - k),
            ));
        }
        groups
    }

    // ---------------------------------------------------------------------------
    // PostProcessStack
    // ---------------------------------------------------------------------------

    pub struct PostProcessStack {
        // HDR render target (scene renders into this)
        hdr_texture: wgpu::Texture,
        hdr_view: wgpu::TextureView,

        // God rays resources
        god_rays_texture: wgpu::Texture,
        god_rays_view: wgpu::TextureView,
        god_rays_pipeline: wgpu::RenderPipeline,
        god_rays_composite_pipeline: wgpu::RenderPipeline,
        god_rays_uniform_buffer: wgpu::Buffer,
        god_rays_bind_group: wgpu::BindGroup,
        god_rays_composite_bind_group: wgpu::BindGroup,
        god_rays_bgl: wgpu::BindGroupLayout,
        god_rays_intensity: f32,

        // Bloom mip chain
        bloom_mip_textures: Vec<wgpu::Texture>,
        bloom_mip_views: Vec<wgpu::TextureView>,
        bloom_mip_sizes: Vec<(u32, u32)>,

        // FXAA intermediate texture (tonemap writes here, FXAA reads it)
        fxaa_intermediate_texture: wgpu::Texture,
        fxaa_intermediate_view: wgpu::TextureView,

        // Pipelines
        bloom_downsample_pipeline: wgpu::RenderPipeline,
        bloom_upsample_pipeline: wgpu::RenderPipeline,
        tonemap_pipeline: wgpu::RenderPipeline,
        fxaa_pipeline: wgpu::RenderPipeline,

        // Per-pass uniform buffers (one per bloom pass to avoid data races)
        bloom_uniform_buffers: Vec<wgpu::Buffer>,
        tonemap_uniform_buffer: wgpu::Buffer,
        fxaa_uniform_buffer: wgpu::Buffer,

        // Bind groups
        downsample_bind_groups: Vec<wgpu::BindGroup>,
        upsample_bind_groups: Vec<wgpu::BindGroup>,
        tonemap_bind_group: wgpu::BindGroup,
        fxaa_bind_group: wgpu::BindGroup,

        // Shared resources
        sampler: wgpu::Sampler,
        bloom_bgl: wgpu::BindGroupLayout,
        tonemap_bgl: wgpu::BindGroupLayout,
        fxaa_bgl: wgpu::BindGroupLayout,
        surface_format: wgpu::TextureFormat,

        // TAA resources
        taa_history_texture: wgpu::Texture,
        taa_history_view: wgpu::TextureView,
        taa_resolved_texture: wgpu::Texture,
        taa_resolved_view: wgpu::TextureView,
        taa_pipeline: wgpu::RenderPipeline,
        taa_uniform_buffer: wgpu::Buffer,
        taa_bind_group: wgpu::BindGroup,
        taa_bgl: wgpu::BindGroupLayout,
        taa_enabled: bool,
        taa_frame_index: u32,
        prev_view_proj: [[f32; 4]; 4],

        // Tuning parameters
        bloom_intensity: f32,
        bloom_threshold: f32,
        exposure: f32,
        width: u32,
        height: u32,
    }

    impl PostProcessStack {
        pub fn new(
            device: &wgpu::Device,
            width: u32,
            height: u32,
            surface_format: wgpu::TextureFormat,
            depth_view: &wgpu::TextureView,
        ) -> Self {
            let (hdr_texture, hdr_view) = create_hdr_texture(device, width, height);
            let (bloom_mip_textures, bloom_mip_views, bloom_mip_sizes) =
                create_bloom_mip_chain(device, width, height);

            let bloom_uniform_buffers = create_bloom_uniform_buffers(device);

            let tonemap_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("tonemap_uniforms"),
                size: std::mem::size_of::<PostProcessUniforms>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

            let bloom_bgl = create_bloom_bind_group_layout(device);
            let tonemap_bgl = create_tonemap_bind_group_layout(device);
            let sampler = create_sampler(device);

            // Downsample pipeline: no blending, just overwrite
            let bloom_downsample_pipeline = create_fullscreen_pipeline(
                device,
                "bloom_downsample",
                BLOOM_DOWNSAMPLE_SHADER,
                &bloom_bgl,
                wgpu::TextureFormat::Rgba16Float,
                None,
            );

            // Upsample pipeline: ADDITIVE blending so bloom accumulates from
            // smaller mips into larger ones on top of their downsample content
            let bloom_upsample_pipeline = create_fullscreen_pipeline(
                device,
                "bloom_upsample",
                BLOOM_UPSAMPLE_SHADER,
                &bloom_bgl,
                wgpu::TextureFormat::Rgba16Float,
                Some(wgpu::BlendState {
                    color: wgpu::BlendComponent {
                        src_factor: wgpu::BlendFactor::One,
                        dst_factor: wgpu::BlendFactor::One,
                        operation: wgpu::BlendOperation::Add,
                    },
                    alpha: wgpu::BlendComponent {
                        src_factor: wgpu::BlendFactor::One,
                        dst_factor: wgpu::BlendFactor::One,
                        operation: wgpu::BlendOperation::Add,
                    },
                }),
            );

            // Tonemap pipeline: now writes to FXAA intermediate texture (same sRGB format)
            let tonemap_pipeline = create_fullscreen_pipeline(
                device,
                "tonemap",
                TONEMAP_SHADER,
                &tonemap_bgl,
                surface_format,
                None,
            );

            // FXAA resources
            let fxaa_bgl = create_fxaa_bind_group_layout(device);
            let (fxaa_intermediate_texture, fxaa_intermediate_view) =
                create_fxaa_intermediate_texture(device, width, height, surface_format);

            let fxaa_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("fxaa_uniforms"),
                size: std::mem::size_of::<FxaaUniforms>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

            // FXAA pipeline: reads tonemapped LDR, writes to final swap-chain surface
            let fxaa_pipeline = create_fullscreen_pipeline(
                device,
                "fxaa",
                FXAA_SHADER,
                &fxaa_bgl,
                surface_format,
                None,
            );

            let downsample_bind_groups = build_downsample_bind_groups(
                device,
                &bloom_bgl,
                &hdr_view,
                &bloom_mip_views,
                &sampler,
                &bloom_uniform_buffers,
            );

            let upsample_bind_groups = build_upsample_bind_groups(
                device,
                &bloom_bgl,
                &bloom_mip_views,
                &sampler,
                &bloom_uniform_buffers,
            );

            // Tonemap reads from HDR + bloom mip 0 (the fully composited bloom)
            let tonemap_bind_group = create_tonemap_bind_group(
                device,
                &tonemap_bgl,
                &hdr_view,
                &bloom_mip_views[0],
                &sampler,
                &tonemap_uniform_buffer,
            );

            // FXAA reads from the tonemap intermediate texture
            let fxaa_bind_group = create_fxaa_bind_group(
                device,
                &fxaa_bgl,
                &fxaa_intermediate_view,
                &sampler,
                &fxaa_uniform_buffer,
            );

            // ── God rays resources ────────────────────────────────────────
            let god_rays_bgl = create_god_rays_bind_group_layout(device);
            let (god_rays_texture, god_rays_view) = create_god_rays_texture(device, width, height);

            let god_rays_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("god_rays_uniforms"),
                size: std::mem::size_of::<GodRaysUniforms>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

            // God rays extraction + radial blur: writes to half-res god rays texture
            let god_rays_pipeline = create_fullscreen_pipeline(
                device,
                "god_rays",
                GOD_RAYS_SHADER,
                &god_rays_bgl,
                wgpu::TextureFormat::Rgba16Float,
                None,
            );

            // God rays composite: additively blends half-res god rays onto the full HDR buffer
            let god_rays_composite_pipeline = create_fullscreen_pipeline(
                device,
                "god_rays_composite",
                GOD_RAYS_COMPOSITE_SHADER,
                &god_rays_bgl,
                wgpu::TextureFormat::Rgba16Float,
                Some(wgpu::BlendState {
                    color: wgpu::BlendComponent {
                        src_factor: wgpu::BlendFactor::One,
                        dst_factor: wgpu::BlendFactor::One,
                        operation: wgpu::BlendOperation::Add,
                    },
                    alpha: wgpu::BlendComponent {
                        src_factor: wgpu::BlendFactor::One,
                        dst_factor: wgpu::BlendFactor::One,
                        operation: wgpu::BlendOperation::Add,
                    },
                }),
            );

            // God rays extraction reads from HDR texture
            let god_rays_bind_group = create_god_rays_bind_group(
                device,
                &god_rays_bgl,
                &hdr_view,
                &sampler,
                &god_rays_uniform_buffer,
            );

            // God rays composite reads from god rays half-res texture
            let god_rays_composite_bind_group = create_god_rays_bind_group(
                device,
                &god_rays_bgl,
                &god_rays_view,
                &sampler,
                &god_rays_uniform_buffer,
            );

            // ── TAA resources ─────────────────────────────────────────────
            let taa_bgl = create_taa_bind_group_layout(device);
            let (taa_history_texture, taa_history_view, taa_resolved_texture, taa_resolved_view) =
                create_taa_textures(device, width, height);

            let taa_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("taa_uniforms"),
                size: std::mem::size_of::<TaaUniforms>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

            let taa_pipeline = create_fullscreen_pipeline(
                device,
                "taa_resolve",
                TAA_RESOLVE_SHADER,
                &taa_bgl,
                wgpu::TextureFormat::Rgba16Float,
                None,
            );

            let taa_bind_group = create_taa_bind_group(
                device,
                &taa_bgl,
                &hdr_view,
                depth_view,
                &taa_history_view,
                &sampler,
                &taa_uniform_buffer,
            );

            Self {
                hdr_texture,
                hdr_view,
                god_rays_texture,
                god_rays_view,
                god_rays_pipeline,
                god_rays_composite_pipeline,
                god_rays_uniform_buffer,
                god_rays_bind_group,
                god_rays_composite_bind_group,
                god_rays_bgl,
                god_rays_intensity: 0.4,
                bloom_mip_textures,
                bloom_mip_views,
                bloom_mip_sizes,
                fxaa_intermediate_texture,
                fxaa_intermediate_view,
                bloom_downsample_pipeline,
                bloom_upsample_pipeline,
                tonemap_pipeline,
                fxaa_pipeline,
                bloom_uniform_buffers,
                tonemap_uniform_buffer,
                fxaa_uniform_buffer,
                downsample_bind_groups,
                upsample_bind_groups,
                tonemap_bind_group,
                fxaa_bind_group,
                sampler,
                bloom_bgl,
                tonemap_bgl,
                fxaa_bgl,
                surface_format,
                taa_history_texture,
                taa_history_view,
                taa_resolved_texture,
                taa_resolved_view,
                taa_pipeline,
                taa_uniform_buffer,
                taa_bind_group,
                taa_bgl,
                taa_enabled: true,
                taa_frame_index: 0,
                prev_view_proj: crate::camera_math::mat4_identity(),
                bloom_intensity: 0.3,
                bloom_threshold: 1.0,
                exposure: 1.0,
                width,
                height,
            }
        }

        /// Returns the HDR Rgba16Float texture that the 3D scene renders into.
        pub fn hdr_texture(&self) -> &wgpu::Texture {
            &self.hdr_texture
        }

        /// Returns the HDR Rgba16Float texture view that the 3D scene should
        /// render into (instead of the surface directly).
        pub fn hdr_target_view(&self) -> &wgpu::TextureView {
            &self.hdr_view
        }

        pub fn resize(
            &mut self,
            device: &wgpu::Device,
            width: u32,
            height: u32,
            _surface_format: wgpu::TextureFormat,
            depth_view: &wgpu::TextureView,
        ) {
            if width == self.width && height == self.height {
                return;
            }
            self.width = width;
            self.height = height;

            let (hdr_texture, hdr_view) = create_hdr_texture(device, width, height);
            let (bloom_mip_textures, bloom_mip_views, bloom_mip_sizes) =
                create_bloom_mip_chain(device, width, height);

            self.downsample_bind_groups = build_downsample_bind_groups(
                device,
                &self.bloom_bgl,
                &hdr_view,
                &bloom_mip_views,
                &self.sampler,
                &self.bloom_uniform_buffers,
            );

            self.upsample_bind_groups = build_upsample_bind_groups(
                device,
                &self.bloom_bgl,
                &bloom_mip_views,
                &self.sampler,
                &self.bloom_uniform_buffers,
            );

            self.tonemap_bind_group = create_tonemap_bind_group(
                device,
                &self.tonemap_bgl,
                &hdr_view,
                &bloom_mip_views[0],
                &self.sampler,
                &self.tonemap_uniform_buffer,
            );

            // Rebuild FXAA intermediate texture and bind group
            let (fxaa_intermediate_texture, fxaa_intermediate_view) =
                create_fxaa_intermediate_texture(device, width, height, self.surface_format);
            self.fxaa_bind_group = create_fxaa_bind_group(
                device,
                &self.fxaa_bgl,
                &fxaa_intermediate_view,
                &self.sampler,
                &self.fxaa_uniform_buffer,
            );
            self.fxaa_intermediate_texture = fxaa_intermediate_texture;
            self.fxaa_intermediate_view = fxaa_intermediate_view;

            // Rebuild god rays textures and bind groups
            let (god_rays_texture, god_rays_view) = create_god_rays_texture(device, width, height);
            self.god_rays_bind_group = create_god_rays_bind_group(
                device,
                &self.god_rays_bgl,
                &hdr_view,
                &self.sampler,
                &self.god_rays_uniform_buffer,
            );
            self.god_rays_composite_bind_group = create_god_rays_bind_group(
                device,
                &self.god_rays_bgl,
                &god_rays_view,
                &self.sampler,
                &self.god_rays_uniform_buffer,
            );
            self.god_rays_texture = god_rays_texture;
            self.god_rays_view = god_rays_view;

            // Rebuild TAA textures and bind group
            let (taa_history_texture, taa_history_view, taa_resolved_texture, taa_resolved_view) =
                create_taa_textures(device, width, height);
            self.taa_bind_group = create_taa_bind_group(
                device,
                &self.taa_bgl,
                &hdr_view,
                depth_view,
                &taa_history_view,
                &self.sampler,
                &self.taa_uniform_buffer,
            );
            self.taa_history_texture = taa_history_texture;
            self.taa_history_view = taa_history_view;
            self.taa_resolved_texture = taa_resolved_texture;
            self.taa_resolved_view = taa_resolved_view;

            self.hdr_texture = hdr_texture;
            self.hdr_view = hdr_view;
            self.bloom_mip_textures = bloom_mip_textures;
            self.bloom_mip_views = bloom_mip_views;
            self.bloom_mip_sizes = bloom_mip_sizes;
        }

        /// Execute the full post-process pipeline:
        /// 0. God rays radial blur (HDR -> half-res -> additive composite back to HDR)
        /// 0.5. TAA resolve (HDR + depth + history -> resolved, copy to history + HDR)
        /// 1. Downsample chain (HDR -> mip0 -> mip1 -> ... -> mip5)
        /// 2. Upsample chain (mip5 -> mip4 -> ... -> mip0, additive)
        /// 3. Tonemap (HDR + bloom mip0 -> FXAA intermediate)
        /// 4. FXAA (FXAA intermediate -> sRGB surface)
        ///
        /// `sun_screen_pos` is the sun position in UV space [0,1]. Pass `None`
        /// to skip god rays (e.g., when the sun is behind the camera).
        /// `view_proj` is the current frame's (unjittered) view-projection matrix.
        pub fn render(
            &mut self,
            encoder: &mut wgpu::CommandEncoder,
            queue: &wgpu::Queue,
            output_view: &wgpu::TextureView,
            sun_screen_pos: Option<[f32; 2]>,
            view_proj: [[f32; 4]; 4],
        ) {
            // ------------------------------------------------------------------
            // God Rays pass (before bloom, on HDR buffer)
            // ------------------------------------------------------------------
            if let Some(sun_pos) = sun_screen_pos {
                if self.god_rays_intensity > 0.001 {
                    let god_rays_uniforms = GodRaysUniforms {
                        sun_screen_pos: sun_pos,
                        intensity: self.god_rays_intensity,
                        decay: 0.96,
                        density: 0.9,
                        weight: 0.6,
                        num_samples: 64.0,
                        threshold: 0.8,
                    };
                    queue.write_buffer(
                        &self.god_rays_uniform_buffer,
                        0,
                        bytemuck::bytes_of(&god_rays_uniforms),
                    );

                    // Pass 1: Radial blur from HDR into half-res god rays texture
                    {
                        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("god_rays_radial_blur"),
                            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                view: &self.god_rays_view,
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
                        pass.set_pipeline(&self.god_rays_pipeline);
                        pass.set_bind_group(0, &self.god_rays_bind_group, &[]);
                        pass.draw(0..3, 0..1);
                    }

                    // Pass 2: Additive composite god rays onto HDR buffer
                    {
                        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("god_rays_composite"),
                            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                view: &self.hdr_view,
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
                        pass.set_pipeline(&self.god_rays_composite_pipeline);
                        pass.set_bind_group(0, &self.god_rays_composite_bind_group, &[]);
                        pass.draw(0..3, 0..1);
                    }
                }
            }

            // ------------------------------------------------------------------
            // TAA resolve pass (after god rays, before bloom)
            // ------------------------------------------------------------------
            if self.taa_enabled {
                let (jx, jy) = crate::camera_math::taa_jitter(self.taa_frame_index);
                let jitter_ndc_x = jx / self.width as f32;
                let jitter_ndc_y = jy / self.height as f32;

                let inv_curr_vp = crate::camera_math::mat4_inverse(view_proj);

                let taa_uniforms = TaaUniforms {
                    screen_dims: [self.width as f32, self.height as f32],
                    jitter_x: jitter_ndc_x,
                    jitter_y: jitter_ndc_y,
                    curr_view_proj: view_proj,
                    prev_view_proj: self.prev_view_proj,
                    inv_curr_view_proj: inv_curr_vp,
                };
                queue.write_buffer(&self.taa_uniform_buffer, 0, bytemuck::bytes_of(&taa_uniforms));

                // TAA resolve: HDR + depth + history -> taa_resolved_texture
                {
                    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("taa_resolve_pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &self.taa_resolved_view,
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
                    pass.set_pipeline(&self.taa_pipeline);
                    pass.set_bind_group(0, &self.taa_bind_group, &[]);
                    pass.draw(0..3, 0..1);
                }

                // Copy resolved -> history for next frame
                encoder.copy_texture_to_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &self.taa_resolved_texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::TexelCopyTextureInfo {
                        texture: &self.taa_history_texture,
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

                // Copy resolved -> HDR so bloom reads the TAA-resolved result
                encoder.copy_texture_to_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &self.taa_resolved_texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::TexelCopyTextureInfo {
                        texture: &self.hdr_texture,
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

                // Update state for next frame
                self.prev_view_proj = view_proj;
                self.taa_frame_index = self.taa_frame_index.wrapping_add(1);
            }

            // ------------------------------------------------------------------
            // Write all bloom uniform data upfront (before any render passes)
            // ------------------------------------------------------------------
            for i in 0..BLOOM_MIP_COUNT {
                let (src_w, src_h) = if i == 0 {
                    (self.width, self.height)
                } else {
                    self.bloom_mip_sizes[i - 1]
                };
                let uniforms = BloomPassUniforms {
                    texel_size_x: 1.0 / src_w as f32,
                    texel_size_y: 1.0 / src_h as f32,
                    bloom_threshold: if i == 0 { self.bloom_threshold } else { 0.0 },
                    bloom_intensity: self.bloom_intensity,
                };
                queue.write_buffer(
                    &self.bloom_uniform_buffers[i],
                    0,
                    bytemuck::bytes_of(&uniforms),
                );
            }

            for k in 0..(BLOOM_MIP_COUNT - 1) {
                let source_mip = BLOOM_MIP_COUNT - 1 - k;
                let (src_w, src_h) = self.bloom_mip_sizes[source_mip];
                let uniforms = BloomPassUniforms {
                    texel_size_x: 1.0 / src_w as f32,
                    texel_size_y: 1.0 / src_h as f32,
                    bloom_threshold: 0.0,
                    bloom_intensity: self.bloom_intensity,
                };
                let buffer_idx = BLOOM_MIP_COUNT + k;
                queue.write_buffer(
                    &self.bloom_uniform_buffers[buffer_idx],
                    0,
                    bytemuck::bytes_of(&uniforms),
                );
            }

            // Write tonemap uniforms
            {
                let uniforms = PostProcessUniforms {
                    bloom_intensity: self.bloom_intensity,
                    exposure: self.exposure,
                    screen_width: self.width as f32,
                    screen_height: self.height as f32,
                };
                queue.write_buffer(
                    &self.tonemap_uniform_buffer,
                    0,
                    bytemuck::bytes_of(&uniforms),
                );
            }

            // Write FXAA uniforms
            {
                let uniforms = FxaaUniforms {
                    rcp_frame_x: 1.0 / self.width as f32,
                    rcp_frame_y: 1.0 / self.height as f32,
                    _pad0: 0.0,
                    _pad1: 0.0,
                };
                queue.write_buffer(&self.fxaa_uniform_buffer, 0, bytemuck::bytes_of(&uniforms));
            }

            // ------------------------------------------------------------------
            // Downsample chain
            // ------------------------------------------------------------------
            for i in 0..BLOOM_MIP_COUNT {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some(&format!("bloom_downsample_{i}")),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &self.bloom_mip_views[i],
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
                pass.set_pipeline(&self.bloom_downsample_pipeline);
                pass.set_bind_group(0, &self.downsample_bind_groups[i], &[]);
                pass.draw(0..3, 0..1);
            }

            // ------------------------------------------------------------------
            // Upsample chain (additive blend back up)
            // ------------------------------------------------------------------
            for k in 0..(BLOOM_MIP_COUNT - 1) {
                let target_mip = BLOOM_MIP_COUNT - 2 - k;

                // Use LoadOp::Load to preserve existing content (the downsample
                // result) so the additive blend accumulates correctly.
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some(&format!("bloom_upsample_{target_mip}")),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &self.bloom_mip_views[target_mip],
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
                pass.set_pipeline(&self.bloom_upsample_pipeline);
                pass.set_bind_group(0, &self.upsample_bind_groups[k], &[]);
                pass.draw(0..3, 0..1);
            }

            // ------------------------------------------------------------------
            // Tonemap pass: combine HDR + bloom mip 0 -> FXAA intermediate
            // ------------------------------------------------------------------
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("tonemap_pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &self.fxaa_intermediate_view,
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
                pass.set_pipeline(&self.tonemap_pipeline);
                pass.set_bind_group(0, &self.tonemap_bind_group, &[]);
                pass.draw(0..3, 0..1);
            }

            // ------------------------------------------------------------------
            // FXAA pass: anti-alias tonemapped LDR -> final sRGB output
            // ------------------------------------------------------------------
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("fxaa_pass"),
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
                pass.set_pipeline(&self.fxaa_pipeline);
                pass.set_bind_group(0, &self.fxaa_bind_group, &[]);
                pass.draw(0..3, 0..1);
            }
        }

        pub fn set_bloom_intensity(&mut self, v: f32) {
            self.bloom_intensity = v;
        }

        pub fn bloom_intensity(&self) -> f32 {
            self.bloom_intensity
        }

        pub fn set_bloom_threshold(&mut self, v: f32) {
            self.bloom_threshold = v;
        }

        pub fn bloom_threshold(&self) -> f32 {
            self.bloom_threshold
        }

        pub fn set_exposure(&mut self, v: f32) {
            self.exposure = v;
        }

        pub fn exposure(&self) -> f32 {
            self.exposure
        }

        pub fn set_god_rays_intensity(&mut self, v: f32) {
            self.god_rays_intensity = v;
        }

        pub fn god_rays_intensity(&self) -> f32 {
            self.god_rays_intensity
        }

        pub fn taa_enabled(&self) -> bool {
            self.taa_enabled
        }

        pub fn set_taa_enabled(&mut self, v: bool) {
            self.taa_enabled = v;
        }

        pub fn taa_frame_index(&self) -> u32 {
            self.taa_frame_index
        }

        pub fn advance_taa_frame(&mut self) {
            self.taa_frame_index = self.taa_frame_index.wrapping_add(1);
        }

        /// Rebuild the TAA bind group when the depth view changes (e.g. after renderer resize).
        pub fn update_depth_view(&mut self, device: &wgpu::Device, depth_view: &wgpu::TextureView) {
            self.taa_bind_group = create_taa_bind_group(
                device,
                &self.taa_bgl,
                &self.hdr_view,
                depth_view,
                &self.taa_history_view,
                &self.sampler,
                &self.taa_uniform_buffer,
            );
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm_impl::PostProcessStack;

#[cfg(test)]
mod tests {
    #[cfg(not(target_arch = "wasm32"))]
    use super::PostProcessStack;

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn stub_creates_successfully() {
        let _stack = PostProcessStack::new();
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn stub_default_bloom_intensity_is_zero() {
        let stack = PostProcessStack::new();
        assert!((stack.bloom_intensity() - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn stub_bloom_intensity_round_trip() {
        let mut stack = PostProcessStack::new();
        stack.set_bloom_intensity(0.75);
        assert!((stack.bloom_intensity() - 0.75).abs() < f32::EPSILON);
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn stub_default_exposure_is_one() {
        let stack = PostProcessStack::new();
        assert!((stack.exposure() - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn stub_default_bloom_threshold_is_one() {
        let stack = PostProcessStack::new();
        assert!((stack.bloom_threshold() - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn postprocess_uniforms_size_is_16_bytes() {
        #[repr(C)]
        #[derive(Clone, Copy)]
        struct PostProcessUniforms {
            bloom_intensity: f32,
            exposure: f32,
            screen_width: f32,
            screen_height: f32,
        }
        assert_eq!(std::mem::size_of::<PostProcessUniforms>(), 16);
    }

    #[test]
    fn bloom_pass_uniforms_size_is_16_bytes() {
        #[repr(C)]
        #[derive(Clone, Copy)]
        struct BloomPassUniforms {
            texel_size_x: f32,
            texel_size_y: f32,
            bloom_threshold: f32,
            bloom_intensity: f32,
        }
        assert_eq!(std::mem::size_of::<BloomPassUniforms>(), 16);
    }

    #[test]
    fn bloom_mip_count_is_six() {
        assert_eq!(super::BLOOM_MIP_COUNT, 6);
    }

    #[test]
    fn bloom_pass_count_is_eleven() {
        // 6 downsamples + 5 upsamples = 11 total bloom passes
        assert_eq!(super::BLOOM_MIP_COUNT + super::BLOOM_MIP_COUNT - 1, 11);
    }

    #[test]
    fn fxaa_uniforms_size_is_16_bytes() {
        #[repr(C)]
        #[derive(Clone, Copy)]
        struct FxaaUniforms {
            rcp_frame_x: f32,
            rcp_frame_y: f32,
            _pad0: f32,
            _pad1: f32,
        }
        assert_eq!(std::mem::size_of::<FxaaUniforms>(), 16);
    }

    #[test]
    fn god_rays_uniforms_size_is_32_bytes() {
        #[repr(C)]
        #[derive(Clone, Copy)]
        struct GodRaysUniforms {
            sun_screen_pos: [f32; 2],
            intensity: f32,
            decay: f32,
            density: f32,
            weight: f32,
            num_samples: f32,
            threshold: f32,
        }
        assert_eq!(std::mem::size_of::<GodRaysUniforms>(), 32);
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn stub_god_rays_intensity_round_trip() {
        let mut stack = PostProcessStack::new();
        stack.set_god_rays_intensity(0.5);
        assert!((stack.god_rays_intensity() - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn stub_default_god_rays_intensity_is_zero() {
        let stack = PostProcessStack::new();
        assert!((stack.god_rays_intensity() - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn stub_taa_enabled_by_default() {
        let stack = PostProcessStack::new();
        assert!(stack.taa_enabled());
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn stub_taa_toggle() {
        let mut stack = PostProcessStack::new();
        stack.set_taa_enabled(false);
        assert!(!stack.taa_enabled());
        stack.set_taa_enabled(true);
        assert!(stack.taa_enabled());
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn stub_taa_frame_index_advances() {
        let mut stack = PostProcessStack::new();
        assert_eq!(stack.taa_frame_index(), 0);
        stack.advance_taa_frame();
        assert_eq!(stack.taa_frame_index(), 1);
        stack.advance_taa_frame();
        assert_eq!(stack.taa_frame_index(), 2);
    }

    #[test]
    fn taa_uniforms_size_is_208_bytes() {
        // TaaUniforms: 2 f32 (screen_dims) + 2 f32 (jitter) + 3 * mat4x4 (3*64) = 200
        // Actually: 2+2+64+64+64 = 8+192 = 200 bytes
        #[repr(C)]
        #[derive(Clone, Copy)]
        struct TaaUniforms {
            screen_dims: [f32; 2],
            jitter_x: f32,
            jitter_y: f32,
            curr_view_proj: [[f32; 4]; 4],
            prev_view_proj: [[f32; 4]; 4],
            inv_curr_view_proj: [[f32; 4]; 4],
        }
        assert_eq!(std::mem::size_of::<TaaUniforms>(), 208);
    }
}
