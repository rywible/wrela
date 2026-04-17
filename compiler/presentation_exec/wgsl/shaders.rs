//! Owns WGSL shader-source assembly for presentation passes.
//! Does not own pass scheduling or shader execution/runtime submission.
//!
//! Key invariants:
//! - emitted shader structs and bindings must stay ABI-compatible with the Rust
//!   runtime helpers that upload/read them.
//! - shader specialization here may optimize a pass, but it must not change the
//!   semantics of the presentation contract.
//!
//! Primary entrypoints:
//! - shader-source builders in this module
//!
//! Failure modes / common pitfalls:
//! - tweaking generated WGSL snippets without keeping ABI helpers in sync yields
//!   compiling shaders that decode the wrong data.

use super::*;

pub(super) fn shade_primary_gpu_shader_source(
    workgroup_size: u32,
    shader_f16_enabled: bool,
) -> Result<String, PresentationExecError> {
    let structs = emit_wgsl_structs(&[
        shade_primary_gpu_config_abi(),
        portable_builtin_record_abi("Hit3").expect("Hit3 abi"),
        portable_builtin_record_abi("Surface").expect("Surface abi"),
        portable_builtin_record_abi("Medium").expect("Medium abi"),
        lighting_inputs_abi(),
    ])?;
    let f16_preamble = wgsl_shader_f16_preamble(shader_f16_enabled);
    let vec3_narrow = wgsl_vec3_narrow_helper(shader_f16_enabled);
    Ok(format!(
        "{f16_preamble}
{structs}

override WG_SIZE: u32 = {workgroup_size}u;

@group({GPU_RUNTIME_PASS_BIND_GROUP_INDEX}) @binding(0)
var<storage, read> config: Abi_ShadePrimaryGpuConfig;

struct HitBuffer {{
  values: array<Abi_Hit3>,
}}
struct SurfaceBuffer {{
  values: array<Abi_Surface>,
}}
struct RadianceBuffer {{
  values: array<vec3<f32>>,
}}
struct MediumBuffer {{
  values: array<Abi_Medium>,
}}
struct OutputBuffer {{
  values: array<vec3<f32>>,
}}

@group({GPU_RUNTIME_PASS_BIND_GROUP_INDEX}) @binding(1)
var<storage, read> primary_hits: HitBuffer;
@group({GPU_RUNTIME_PASS_BIND_GROUP_INDEX}) @binding(2)
var<storage, read> surfaces: SurfaceBuffer;
@group({GPU_RUNTIME_PASS_BIND_GROUP_INDEX}) @binding(3)
var<storage, read> radiance_values: RadianceBuffer;
@group({GPU_RUNTIME_PASS_BIND_GROUP_INDEX}) @binding(4)
var<storage, read> medium_values: MediumBuffer;
@group({GPU_RUNTIME_PASS_BIND_GROUP_INDEX}) @binding(5)
var<storage, read_write> output_values: OutputBuffer;

{vec3_narrow}

fn wr_normalize_or(value: vec3<f32>, fallback: vec3<f32>) -> vec3<f32> {{
  let len_sq = dot(value, value);
  if (len_sq <= 0.0000001) {{
    return fallback;
  }}
  return normalize(value);
}}

fn scaled_index(index: u32, output_width: u32, output_height: u32, source_width: u32, source_height: u32) -> u32 {{
  let x = index % max(output_width, 1u);
  let y = index / max(output_width, 1u);
  let source_x = (x * max(source_width, 1u)) / max(output_width, 1u);
  let source_y = (y * max(source_height, 1u)) / max(output_height, 1u);
  return min(
    source_y * max(source_width, 1u) + source_x,
    max(source_width * source_height, 1u) - 1u
  );
}}

fn shade_ray_direction(index: u32) -> vec3<f32> {{
  let width = max(config.viewport_width, 1u);
  let height = max(config.viewport_height, 1u);
  let x = index % width;
  let y = index / width;
  let uv = vec2<f32>(
    (f32(x) + 0.5 + config.jitter.x) / f32(width),
    (f32(y) + 0.5 + config.jitter.y) / f32(height)
  );
  let forward = wr_normalize_or(config.forward, vec3<f32>(0.0, 0.0, -1.0));
  if (config.legacy_active != 0u) {{
    let right = wr_normalize_or(cross(forward, config.legacy_world_up), vec3<f32>(1.0, 0.0, 0.0));
    let up = wr_normalize_or(cross(right, forward), vec3<f32>(0.0, 1.0, 0.0));
    let aspect = f32(width) / f32(height);
    let screen_x = (uv.x * 2.0 - 1.0) * aspect * config.legacy_view_scale;
    let screen_y = (1.0 - uv.y * 2.0) * config.legacy_view_scale;
    return wr_normalize_or(forward + (right * screen_x) + (up * screen_y), forward);
  }}
  let right = wr_normalize_or(cross(forward, config.up), vec3<f32>(1.0, 0.0, 0.0));
  let up = wr_normalize_or(cross(right, forward), vec3<f32>(0.0, 1.0, 0.0));
  let aspect = f32(width) / f32(height);
  let vertical_scale = tan(radians(config.vertical_fov_degrees) * 0.5);
  let screen_x = (uv.x * 2.0 - 1.0) * aspect * vertical_scale;
  let screen_y = (1.0 - uv.y * 2.0) * vertical_scale;
  return wr_normalize_or(forward + (right * screen_x) + (up * screen_y), forward);
}}

fn clamp_vec3(value: vec3<f32>, min_value: f32, max_value: f32) -> vec3<f32> {{
  return vec3<f32>(
    clamp(value.x, min_value, max_value),
    clamp(value.y, min_value, max_value),
    clamp(value.z, min_value, max_value)
  );
}}

@compute @workgroup_size(WG_SIZE)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {{
  let index = global_id.x;
  if (index >= config.item_count) {{
    return;
  }}
  let hit = primary_hits.values[index];
  let surface = surfaces.values[scaled_index(
    index,
    config.viewport_width,
    config.viewport_height,
    config.surface_width,
    config.surface_height
  )];
  var radiance = vec3<f32>(0.0, 0.0, 0.0);
  if (config.radiance_active != 0u) {{
    radiance = radiance_values.values[scaled_index(
      index,
      config.viewport_width,
      config.viewport_height,
      config.radiance_width,
      config.radiance_height
    )];
  }}
  var medium = medium_values.values[0];
  if (config.medium_active != 0u) {{
    medium = medium_values.values[scaled_index(
      index,
      config.viewport_width,
      config.viewport_height,
      config.medium_width,
      config.medium_height
    )];
  }}
  _ = shade_ray_direction(index);
  if (hit.hit != 0u) {{
    let key_delta = config.lighting.key_light.position - hit.position;
    let key_dir = normalize(key_delta);
    let view_dir = normalize(config.camera_position - hit.position);
    let half_dir = normalize(key_dir + view_dir);
    let distance_to_light = length(key_delta);
    let attenuation = clamp(1.0 - (distance_to_light / max(config.lighting.key_light.range, 0.00001)), 0.0, 1.0);
    let ndotl = max(dot(hit.normal, key_dir), 0.0);
    let ndoth = max(dot(hit.normal, half_dir), 0.0);
    let diffuse = ndotl * attenuation;
    let fill = max(dot(hit.normal, normalize(config.lighting.fill_direction)), 0.0) * config.lighting.fill_strength;
    let roughness = clamp(surface.roughness, 0.0, 1.0);
    let spec_power = mix(48.0, 8.0, roughness);
    let metalness = clamp(surface.metalness, 0.0, 1.0);
    let clearcoat = clamp(surface.clearcoat, 0.0, 1.0);
    let highlight = pow(ndoth, spec_power) * (0.10 + (metalness * 0.25) + (clearcoat * 0.20));
    let lighting_rgb = config.lighting.ambient_color + vec3<f32>(diffuse + fill);
    let direct = clamp_vec3(
      (surface.albedo * lighting_rgb * config.lighting.key_light.intensity)
        + vec3<f32>(highlight * 220.0, highlight * 208.0, highlight * 196.0),
      0.0,
      255.0
    );
    let fog_strength = clamp(medium.density * distance_to_light * 0.18, 0.0, 0.55);
    let fog_color = medium.emission + (radiance * 0.22);
    let radiance_lit = radiance * (0.25 + (highlight * 0.15));
    let lit = direct + surface.emissive + radiance_lit;
    output_values.values[index] = narrow_vec3(mix(lit, fog_color, vec3<f32>(fog_strength)));
  }} else {{
    let miss_fog = clamp(medium.density * 3.0, 0.0, 0.45);
    let miss_mix_color = medium.emission + (radiance * 0.28);
    output_values.values[index] = narrow_vec3(mix(radiance, miss_mix_color, vec3<f32>(miss_fog)));
  }}
}}
"
    ))
}

pub(super) fn motion_resolve_gpu_shader_source(
    workgroup_size: u32,
) -> Result<String, PresentationExecError> {
    let structs = emit_wgsl_structs(&[
        motion_resolve_gpu_config_abi(),
        portable_builtin_record_abi("Hit3").expect("Hit3 abi"),
        portable_builtin_record_abi("MotionVector").expect("MotionVector abi"),
    ])?;
    Ok(format!(
        "{structs}

override WG_SIZE: u32 = {workgroup_size}u;

@group({GPU_RUNTIME_PASS_BIND_GROUP_INDEX}) @binding(0)
var<storage, read> config: Abi_MotionResolveGpuConfig;

struct HitBuffer {{
  values: array<Abi_Hit3>,
}}
struct MotionBuffer {{
  values: array<Abi_MotionVector>,
}}
struct StatsBuffer {{
  counts: array<atomic<u32>, 3>,
}}

@group({GPU_RUNTIME_PASS_BIND_GROUP_INDEX}) @binding(1)
var<storage, read> current_hits: HitBuffer;
@group({GPU_RUNTIME_PASS_BIND_GROUP_INDEX}) @binding(2)
var<storage, read> previous_hits: HitBuffer;
@group({GPU_RUNTIME_PASS_BIND_GROUP_INDEX}) @binding(3)
var<storage, read_write> output_motion: MotionBuffer;
@group({GPU_RUNTIME_PASS_BIND_GROUP_INDEX}) @binding(4)
var<storage, read_write> stats: StatsBuffer;

fn wr_normalize_or(value: vec3<f32>, fallback: vec3<f32>) -> vec3<f32> {{
  let len_sq = dot(value, value);
  if (len_sq <= 0.0000001) {{
    return fallback;
  }}
  return normalize(value);
}}

fn project_to_previous_sample(point: vec3<f32>) -> vec2<f32> {{
  let forward = wr_normalize_or(config.previous_forward, vec3<f32>(0.0, 0.0, -1.0));
  let right = wr_normalize_or(cross(forward, config.previous_up), vec3<f32>(1.0, 0.0, 0.0));
  let up = wr_normalize_or(cross(right, forward), vec3<f32>(0.0, 1.0, 0.0));
  let rel = point - config.previous_camera_position;
  let depth = dot(rel, forward);
  if (depth <= 0.0001) {{
    return vec2<f32>(-1.0, -1.0);
  }}
  let width = max(config.previous_viewport_width, 1u);
  let height = max(config.previous_viewport_height, 1u);
  let aspect = f32(width) / f32(height);
  let vertical_scale = max(tan(radians(config.previous_vertical_fov_degrees) * 0.5), 0.0001);
  let screen_x = dot(rel, right) / (depth * aspect * vertical_scale);
  let screen_y = dot(rel, up) / (depth * vertical_scale);
  let uv = vec2<f32>((screen_x + 1.0) * 0.5, (1.0 - screen_y) * 0.5);
  return vec2<f32>(
    (uv.x * f32(width)) - 0.5 - config.previous_jitter.x,
    (uv.y * f32(height)) - 0.5 - config.previous_jitter.y
  );
}}

fn sample_in_view(sample: vec2<f32>) -> bool {{
  return sample.x >= 0.0
    && sample.y >= 0.0
    && sample.x < f32(config.previous_viewport_width)
    && sample.y < f32(config.previous_viewport_height);
}}

fn previous_index(sample: vec2<f32>) -> u32 {{
  let x = u32(clamp(round(sample.x), 0.0, f32(max(config.previous_viewport_width, 1u) - 1u)));
  let y = u32(clamp(round(sample.y), 0.0, f32(max(config.previous_viewport_height, 1u) - 1u)));
  return y * max(config.previous_viewport_width, 1u) + x;
}}

fn same_identity(current: Abi_Hit3, previous: Abi_Hit3) -> bool {{
  return current.hit != 0u
    && previous.hit != 0u
    && current.root_shape_id == previous.root_shape_id
    && current.feature_id == previous.feature_id
    && current.instance_id == previous.instance_id
    && current.repeat_id == previous.repeat_id;
}}

@compute @workgroup_size(WG_SIZE)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {{
  let index = global_id.x;
  if (index >= config.item_count) {{
    return;
  }}
  let hit = current_hits.values[index];
  let current_pixel = vec2<f32>(
    f32(index % max(config.viewport_width, 1u)),
    f32(index / max(config.viewport_width, 1u))
  );
  var motion = Abi_MotionVector(
    vec2<f32>(0.0, 0.0),
    vec2<f32>(0.0, 0.0),
    0u,
    0u
  );
  if (hit.hit != 0u) {{
    let previous_sample = project_to_previous_sample(hit.position);
    if (config.history_available != 0u && config.has_history_primary_hit != 0u) {{
      if (sample_in_view(previous_sample)) {{
        let previous_hit = previous_hits.values[previous_index(previous_sample)];
        if (same_identity(hit, previous_hit)) {{
          motion = Abi_MotionVector(previous_sample - current_pixel, previous_sample, 1u, 0u);
          atomicAdd(&stats.counts[0], 1u);
        }} else {{
          motion = Abi_MotionVector(previous_sample - current_pixel, previous_sample, 0u, 1u);
          atomicAdd(&stats.counts[1], 1u);
        }}
      }} else {{
        motion = Abi_MotionVector(previous_sample - current_pixel, previous_sample, 0u, 1u);
        atomicAdd(&stats.counts[1], 1u);
      }}
    }} else {{
      motion = Abi_MotionVector(
        vec2<f32>(0.0, 0.0),
        select(vec2<f32>(0.0, 0.0), previous_sample, all(previous_sample >= vec2<f32>(0.0, 0.0))),
        0u,
        0u
      );
      if (config.history_rejected != 0u) {{
        atomicAdd(&stats.counts[1], 1u);
      }} else {{
        atomicAdd(&stats.counts[2], 1u);
      }}
    }}
  }} else {{
    atomicAdd(&stats.counts[2], 1u);
  }}
  output_motion.values[index] = motion;
}}
"
    ))
}

pub(super) fn temporal_resolve_gpu_shader_source(
    workgroup_size: u32,
    shader_f16_enabled: bool,
) -> Result<String, PresentationExecError> {
    let structs = emit_wgsl_structs(&[
        temporal_resolve_gpu_config_abi(),
        portable_builtin_record_abi("MotionVector").expect("MotionVector abi"),
    ])?;
    let f16_preamble = wgsl_shader_f16_preamble(shader_f16_enabled);
    let vec3_narrow = wgsl_vec3_narrow_helper(shader_f16_enabled);
    Ok(format!(
        "{f16_preamble}
{structs}

override WG_SIZE: u32 = {workgroup_size}u;

@group({GPU_RUNTIME_PASS_BIND_GROUP_INDEX}) @binding(0)
var<storage, read> config: Abi_TemporalResolveGpuConfig;

struct ColorBuffer {{
  values: array<vec3<f32>>,
}}
struct MotionBuffer {{
  values: array<Abi_MotionVector>,
}}
struct StatsBuffer {{
  consumed: atomic<u32>,
}}

@group({GPU_RUNTIME_PASS_BIND_GROUP_INDEX}) @binding(1)
var<storage, read> current_color: ColorBuffer;
@group({GPU_RUNTIME_PASS_BIND_GROUP_INDEX}) @binding(2)
var<storage, read> history_color: ColorBuffer;
@group({GPU_RUNTIME_PASS_BIND_GROUP_INDEX}) @binding(3)
var<storage, read> motion_values: MotionBuffer;
@group({GPU_RUNTIME_PASS_BIND_GROUP_INDEX}) @binding(4)
var<storage, read_write> output_color: ColorBuffer;
@group({GPU_RUNTIME_PASS_BIND_GROUP_INDEX}) @binding(5)
var<storage, read_write> stats: StatsBuffer;

{vec3_narrow}

fn previous_index(sample: vec2<f32>) -> u32 {{
  let x = u32(clamp(round(sample.x), 0.0, f32(max(config.width, 1u) - 1u)));
  let y = u32(clamp(round(sample.y), 0.0, f32(max(config.height, 1u) - 1u)));
  return y * max(config.width, 1u) + x;
}}

fn neighborhood_bounds(index: u32) -> array<vec3<f32>, 2> {{
  let width = max(config.width, 1u);
  let height = max(config.height, 1u);
  let x = index % width;
  let y = index / width;
  var clamp_min = vec3<f32>(999999.0, 999999.0, 999999.0);
  var clamp_max = vec3<f32>(-999999.0, -999999.0, -999999.0);
  for (var dy: i32 = -1; dy <= 1; dy = dy + 1) {{
    for (var dx: i32 = -1; dx <= 1; dx = dx + 1) {{
      let sample_x = u32(clamp(i32(x) + dx, 0, i32(width) - 1));
      let sample_y = u32(clamp(i32(y) + dy, 0, i32(height) - 1));
      let sample = current_color.values[sample_y * width + sample_x];
      clamp_min = min(clamp_min, sample);
      clamp_max = max(clamp_max, sample);
    }}
  }}
  return array<vec3<f32>, 2>(clamp_min, clamp_max);
}}

@compute @workgroup_size(WG_SIZE)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {{
  let index = global_id.x;
  if (index >= config.item_count) {{
    return;
  }}
  let current = current_color.values[index];
  let motion = motion_values.values[index];
  let bounds = neighborhood_bounds(index);
  let clamp_min = bounds[0];
  let clamp_max = bounds[1];
  var history = vec3<f32>(0.0, 0.0, 0.0);
  let use_history = motion.valid != 0u && motion.disoccluded == 0u;
  if (motion.valid != 0u) {{
    history = history_color.values[previous_index(motion.previous_sample)];
  }}
  if (use_history) {{
    atomicAdd(&stats.consumed, 1u);
  }}
  let clamped_history = vec3<f32>(
    clamp(history.x, clamp_min.x, clamp_max.x),
    clamp(history.y, clamp_min.y, clamp_max.y),
    clamp(history.z, clamp_min.z, clamp_max.z)
  );
  let history_weight = f32(config.history_weight_numerator) / f32(max(config.history_weight_denominator, 1u));
  let resolved = select(
    current,
    (current * (1.0 - history_weight)) + (clamped_history * history_weight),
    use_history
  );
  output_color.values[index] = narrow_vec3(resolved);
}}
"
    ))
}

#[cfg(test)]
pub(super) fn shade_primary_shader_source(
    workgroup_size: u32,
    shader_f16_enabled: bool,
) -> Result<String, PresentationExecError> {
    let structs = emit_wgsl_structs(&[
        crate::query_exec::wgsl::codegen::wgsl_dispatch_config_abi(),
        shade_primary_input_abi(),
    ])?;
    let f16_preamble = wgsl_shader_f16_preamble(shader_f16_enabled);
    let vec3_narrow = wgsl_vec3_narrow_helper(shader_f16_enabled);
    Ok(format!(
        "{f16_preamble}
{structs}

override WG_SIZE: u32 = {workgroup_size}u;

@group({GPU_RUNTIME_PASS_BIND_GROUP_INDEX}) @binding(0)
var<storage, read> dispatch_config: Abi_WgslDispatchConfig;

struct InputBuffer {{
  values: array<Abi_ShadePrimaryInput>,
}}

struct OutputBuffer {{
  values: array<vec3<f32>>,
}}

struct DummyBuffer {{
  values: array<u32>,
}}

@group({GPU_RUNTIME_PASS_BIND_GROUP_INDEX}) @binding(1)
var<storage, read> input_items: InputBuffer;
@group({GPU_RUNTIME_PASS_BIND_GROUP_INDEX}) @binding(2)
var<storage, read_write> output_items: OutputBuffer;
@group({GPU_RUNTIME_PASS_BIND_GROUP_INDEX}) @binding(3)
var<storage, read> dummy_items: DummyBuffer;

{vec3_narrow}

fn clamp_vec3(value: vec3<f32>, min_value: f32, max_value: f32) -> vec3<f32> {{
  return vec3<f32>(
    clamp(value.x, min_value, max_value),
    clamp(value.y, min_value, max_value),
    clamp(value.z, min_value, max_value)
  );
}}

@compute @workgroup_size(WG_SIZE)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {{
  let index = global_id.x;
  _ = dummy_items.values[0];
  if (index >= dispatch_config.item_count) {{
    return;
  }}
  let input = input_items.values[index];
  if (input.hit.hit != 0u) {{
    let key_delta = input.lighting.key_light.position - input.hit.position;
    let key_dir = normalize(key_delta);
    let view_dir = normalize(input.camera_position - input.hit.position);
    let half_dir = normalize(key_dir + view_dir);
    let distance_to_light = length(key_delta);
    let attenuation = clamp(1.0 - (distance_to_light / max(input.lighting.key_light.range, 0.00001)), 0.0, 1.0);
    let ndotl = max(dot(input.hit.normal, key_dir), 0.0);
    let ndoth = max(dot(input.hit.normal, half_dir), 0.0);
    let diffuse = ndotl * attenuation;
    let fill = max(dot(input.hit.normal, normalize(input.lighting.fill_direction)), 0.0) * input.lighting.fill_strength;
    let roughness = clamp(input.surface.roughness, 0.0, 1.0);
    let spec_power = mix(48.0, 8.0, roughness);
    let metalness = clamp(input.surface.metalness, 0.0, 1.0);
    let clearcoat = clamp(input.surface.clearcoat, 0.0, 1.0);
    let highlight = pow(ndoth, spec_power) * (0.10 + (metalness * 0.25) + (clearcoat * 0.20));
    let lighting_rgb = input.lighting.ambient_color + vec3<f32>(diffuse + fill);
    let direct = clamp_vec3(
      (input.surface.albedo * lighting_rgb * input.lighting.key_light.intensity)
        + vec3<f32>(highlight * 220.0, highlight * 208.0, highlight * 196.0),
      0.0,
      255.0,
    );
    let fog_strength = clamp(input.medium.density * distance_to_light * 0.18, 0.0, 0.55);
    let fog_color = input.medium.emission + (input.radiance * 0.22);
    let radiance_lit = input.radiance * (0.25 + (highlight * 0.15));
    let lit = direct + input.surface.emissive + radiance_lit;
    output_items.values[index] = narrow_vec3(mix(lit, fog_color, vec3<f32>(fog_strength)));
  }} else {{
    let miss_fog = clamp(input.medium.density * 3.0, 0.0, 0.45);
    let miss_mix_color = input.medium.emission + (input.radiance * 0.28);
    output_items.values[index] =
      narrow_vec3(mix(input.radiance, miss_mix_color, vec3<f32>(miss_fog)));
  }}
}}
"
    ))
}

pub(super) fn copy_vec3_shader_source(
    workgroup_size: u32,
    shader_f16_enabled: bool,
) -> Result<String, PresentationExecError> {
    let structs =
        emit_wgsl_structs(&[crate::query_exec::wgsl::codegen::wgsl_dispatch_config_abi()])?;
    let f16_preamble = wgsl_shader_f16_preamble(shader_f16_enabled);
    let vec3_narrow = wgsl_vec3_narrow_helper(shader_f16_enabled);
    Ok(format!(
        "{f16_preamble}
{structs}

override WG_SIZE: u32 = {workgroup_size}u;

@group({GPU_RUNTIME_PASS_BIND_GROUP_INDEX}) @binding(0)
var<storage, read> dispatch_config: Abi_WgslDispatchConfig;

struct InputBuffer {{
  values: array<vec3<f32>>,
}}

struct OutputBuffer {{
  values: array<vec3<f32>>,
}}

struct DummyBuffer {{
  values: array<u32>,
}}

@group({GPU_RUNTIME_PASS_BIND_GROUP_INDEX}) @binding(1)
var<storage, read> input_items: InputBuffer;
@group({GPU_RUNTIME_PASS_BIND_GROUP_INDEX}) @binding(2)
var<storage, read_write> output_items: OutputBuffer;
@group({GPU_RUNTIME_PASS_BIND_GROUP_INDEX}) @binding(3)
var<storage, read> dummy_items: DummyBuffer;

{vec3_narrow}

@compute @workgroup_size(WG_SIZE)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {{
  let index = global_id.x;
  _ = dummy_items.values[0];
  if (index >= dispatch_config.item_count) {{
    return;
  }}
  output_items.values[index] = narrow_vec3(input_items.values[index]);
}}
"
    ))
}

#[cfg(test)]
pub(super) fn temporal_resolve_shader_source(
    contract: &TemporalResolvePassContract,
    workgroup_size: u32,
    shader_f16_enabled: bool,
) -> Result<String, PresentationExecError> {
    let structs = emit_wgsl_structs(&[
        crate::query_exec::wgsl::codegen::wgsl_dispatch_config_abi(),
        temporal_resolve_input_abi(),
    ])?;
    let history_weight = contract.history_weight_numerator as f32
        / contract.history_weight_denominator.max(1) as f32;
    let f16_preamble = wgsl_shader_f16_preamble(shader_f16_enabled);
    let vec3_narrow = wgsl_vec3_narrow_helper(shader_f16_enabled);
    Ok(format!(
        "{f16_preamble}
{structs}

override WG_SIZE: u32 = {workgroup_size}u;

@group({GPU_RUNTIME_PASS_BIND_GROUP_INDEX}) @binding(0)
var<storage, read> dispatch_config: Abi_WgslDispatchConfig;

struct InputBuffer {{
  values: array<Abi_TemporalResolveInput>,
}}

struct OutputBuffer {{
  values: array<vec3<f32>>,
}}

struct DummyBuffer {{
  values: array<u32>,
}}

@group({GPU_RUNTIME_PASS_BIND_GROUP_INDEX}) @binding(1)
var<storage, read> input_items: InputBuffer;
@group({GPU_RUNTIME_PASS_BIND_GROUP_INDEX}) @binding(2)
var<storage, read_write> output_items: OutputBuffer;
@group({GPU_RUNTIME_PASS_BIND_GROUP_INDEX}) @binding(3)
var<storage, read> dummy_items: DummyBuffer;

{vec3_narrow}

fn clamp_vec3(value: vec3<f32>, min_value: vec3<f32>, max_value: vec3<f32>) -> vec3<f32> {{
  return vec3<f32>(
    clamp(value.x, min_value.x, max_value.x),
    clamp(value.y, min_value.y, max_value.y),
    clamp(value.z, min_value.z, max_value.z)
  );
}}

@compute @workgroup_size(WG_SIZE)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {{
  let index = global_id.x;
  _ = dummy_items.values[0];
  if (index >= dispatch_config.item_count) {{
    return;
  }}
  let input = input_items.values[index];
  if (input.use_history != 0u) {{
    let clamped_history = clamp_vec3(input.history_color, input.clamp_min, input.clamp_max);
    output_items.values[index] =
      narrow_vec3(mix(input.current_color, clamped_history, vec3<f32>({history_weight})));
  }} else {{
    output_items.values[index] = narrow_vec3(input.current_color);
  }}
}}
"
    ))
}

pub(super) fn wgsl_shader_f16_preamble(shader_f16_enabled: bool) -> &'static str {
    if shader_f16_enabled {
        "enable f16;"
    } else {
        ""
    }
}

pub(super) fn wgsl_vec3_narrow_helper(shader_f16_enabled: bool) -> &'static str {
    if shader_f16_enabled {
        r#"
fn narrow_vec3(value: vec3<f32>) -> vec3<f32> {
  let narrowed = vec3<f16>(value);
  return vec3<f32>(narrowed);
}
"#
    } else {
        r#"
fn narrow_vec3(value: vec3<f32>) -> vec3<f32> {
  return value;
}
"#
    }
}

pub(super) fn emit_wgsl_structs(
    roots: &[PortableAbiType],
) -> Result<String, PresentationExecError> {
    let prefixed = roots
        .iter()
        .cloned()
        .map(prefix_abi_name)
        .collect::<Vec<_>>();
    portable_abi_emit_wgsl_structs(&prefixed).map_err(|err| {
        PresentationExecError::UnsupportedPlan {
            message: err.to_string(),
        }
    })
}

pub(super) fn prefix_abi_name(abi: PortableAbiType) -> PortableAbiType {
    match abi {
        PortableAbiType::Struct {
            name,
            class_id,
            fields,
        } => PortableAbiType::Struct {
            name: SmolStr::new(format!("Abi_{name}")),
            class_id,
            fields: fields
                .into_iter()
                .map(|field| PortableStructField {
                    name: field.name,
                    ty: prefix_abi_name(field.ty),
                })
                .collect(),
        },
        PortableAbiType::Array(inner, len) => {
            PortableAbiType::Array(Box::new(prefix_abi_name(*inner)), len)
        }
        other => other,
    }
}

pub(super) fn hit_flag(value: &KernelValue) -> Result<bool, PresentationExecError> {
    match field(expect_struct(value, "Hit3")?, "hit")? {
        KernelValue::Bool(value) => Ok(*value),
        other => Err(PresentationExecError::TypeMismatch {
            expected: "Boolean".to_string(),
            found: format!("{other:?}"),
        }),
    }
}

pub(super) fn hit_distance(value: &KernelValue) -> Result<f32, PresentationExecError> {
    expect_f32(field(expect_struct(value, "Hit3")?, "distance")?)
}
