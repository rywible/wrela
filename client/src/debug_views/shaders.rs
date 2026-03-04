/// WGSL shader source strings for debug views.
/// Follows the same inline pattern as ssao.rs.

/// Step count heatmap debug view shader.
/// Maps step count to a blue->red gradient.
pub const STEP_COUNT_SHADER: &str = r#"
struct DebugViewParams {
    max_steps: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
};

@group(0) @binding(0) var<uniform> params: DebugViewParams;
@group(0) @binding(1) var debug_tex: texture_2d<f32>;
@group(0) @binding(2) var debug_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var pos = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0)
    );
    var out: VertexOutput;
    out.position = vec4<f32>(pos[vertex_index], 0.0, 1.0);
    out.uv = pos[vertex_index] * vec2<f32>(0.5, -0.5) + vec2<f32>(0.5, 0.5);
    return out;
}

@fragment
fn fs(in: VertexOutput) -> @location(0) vec4<f32> {
    let step_count = textureSample(debug_tex, debug_sampler, in.uv).r;
    let t = clamp(step_count / params.max_steps, 0.0, 1.0);
    // Blue (0,0,1) -> Green (0,1,0) -> Red (1,0,0)
    let r = clamp(2.0 * t - 1.0, 0.0, 1.0);
    let g = 1.0 - abs(2.0 * t - 1.0);
    let b = clamp(1.0 - 2.0 * t, 0.0, 1.0);
    return vec4<f32>(r, g, b, 0.7);
}
"#;

/// Bound magnitude debug view shader.
/// Visualizes the scalar Lipschitz bound L.
pub const BOUND_MAGNITUDE_SHADER: &str = r#"
struct DebugViewParams {
    max_bound: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
};

@group(0) @binding(0) var<uniform> params: DebugViewParams;
@group(0) @binding(1) var debug_tex: texture_2d<f32>;
@group(0) @binding(2) var debug_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var pos = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0)
    );
    var out: VertexOutput;
    out.position = vec4<f32>(pos[vertex_index], 0.0, 1.0);
    out.uv = pos[vertex_index] * vec2<f32>(0.5, -0.5) + vec2<f32>(0.5, 0.5);
    return out;
}

@fragment
fn fs(in: VertexOutput) -> @location(0) vec4<f32> {
    let bound = textureSample(debug_tex, debug_sampler, in.uv).r;
    let t = clamp(bound / params.max_bound, 0.0, 1.0);
    // Dark blue (low) to bright yellow (high)
    let r = t;
    let g = t * 0.8;
    let b = 1.0 - t;
    return vec4<f32>(r, g, b, 0.7);
}
"#;

/// B Provenance debug view shader.
/// Color-codes per-brick provenance: Green=Analytic, Cyan=AnalyticBernstein,
/// Blue=Sampled, Yellow=Inflated, Red=Unknown.
pub const B_PROVENANCE_SHADER: &str = r#"
@group(0) @binding(0) var debug_tex: texture_2d<f32>;
@group(0) @binding(1) var debug_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var pos = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0)
    );
    var out: VertexOutput;
    out.position = vec4<f32>(pos[vertex_index], 0.0, 1.0);
    out.uv = pos[vertex_index] * vec2<f32>(0.5, -0.5) + vec2<f32>(0.5, 0.5);
    return out;
}

@fragment
fn fs(in: VertexOutput) -> @location(0) vec4<f32> {
    let provenance_id = textureSample(debug_tex, debug_sampler, in.uv).r;
    // Map provenance ID to color:
    // 0 = Analytic (Green)
    // 1 = AnalyticBernstein (Cyan)
    // 2 = Sampled (Blue)
    // 3 = Inflated (Yellow)
    // 4 = Unknown (Red)
    var color: vec3<f32>;
    let id = u32(provenance_id * 255.0 + 0.5);
    if (id == 0u) {
        color = vec3<f32>(0.2, 0.9, 0.2);  // Green
    } else if (id == 1u) {
        color = vec3<f32>(0.2, 0.9, 0.9);  // Cyan
    } else if (id == 2u) {
        color = vec3<f32>(0.3, 0.3, 0.9);  // Blue
    } else if (id == 3u) {
        color = vec3<f32>(0.9, 0.9, 0.2);  // Yellow
    } else {
        color = vec3<f32>(0.9, 0.2, 0.2);  // Red (Unknown)
    }
    return vec4<f32>(color, 0.7);
}
"#;

/// Placeholder shader for unimplemented debug views.
/// Returns a uniform gray to indicate the view is not yet functional.
pub const PLACEHOLDER_SHADER: &str = r#"
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var pos = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0)
    );
    var out: VertexOutput;
    out.position = vec4<f32>(pos[vertex_index], 0.0, 1.0);
    out.uv = pos[vertex_index] * vec2<f32>(0.5, -0.5) + vec2<f32>(0.5, 0.5);
    return out;
}

@fragment
fn fs(in: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(0.3, 0.3, 0.3, 0.5);
}
"#;
