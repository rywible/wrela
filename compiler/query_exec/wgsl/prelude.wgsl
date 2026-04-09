// WGSL query-exec prelude for phase 10.
//
// This file is intentionally standalone and typed. It mirrors the current
// arithmetic and geometry formulas in runtime/src/data/portable.rs and the
// render/query helpers in compiler/query_exec/cpu.rs as closely as WGSL's type
// system allows. Dynamic "nil"/type-error cases from the boxed runtime are not
// representable here, so degenerate typed inputs fall back to finite defaults.

struct FieldLocalFrame {
  point: vec3<f32>,
  instance_id: u32,
  repeat_id: u32,
  terminal_node_id: u32,
}

struct ShapeWinner {
  distance: f32,
  feature_id: u32,
  has_leaf: u32,
  leaf_scene_index: u32,
  leaf_id: u32,
  field_index: u32,
}

const WR_F32_EPSILON: f32 = 1.1920929e-7;
const WR_SURFACE_NORMAL_EPSILON: f32 = 1.0e-6;
const WR_TAU: f32 = 6.283185307179586;
const WR_IDENTITY_HASH_OFFSET: u32 = 0x811c9dc5u;
const WR_IDENTITY_HASH_PRIME: u32 = 0x01000193u;

fn wr_field_local_frame(
  point: vec3<f32>,
  instance_id: u32,
  repeat_id: u32,
  terminal_node_id: u32,
) -> FieldLocalFrame {
  return FieldLocalFrame(point, instance_id, repeat_id, terminal_node_id);
}

fn wr_default_shape_winner() -> ShapeWinner {
  return ShapeWinner(1000000.0, 0u, 0u, 0u, 0u, 0u);
}

fn wr_abs_scalar(value: f32) -> f32 {
  return abs(value);
}

fn wr_vec2_length_sq(value: vec2<f32>) -> f32 {
  return dot(value, value);
}

fn wr_vec3_length_sq(value: vec3<f32>) -> f32 {
  return dot(value, value);
}

fn wr_bounds2_center(min_value: vec2<f32>, max_value: vec2<f32>) -> vec2<f32> {
  return mix(min_value, max_value, vec2<f32>(0.5, 0.5));
}

fn wr_bounds2_size(min_value: vec2<f32>, max_value: vec2<f32>) -> vec2<f32> {
  return max_value - min_value;
}

fn wr_bounds3_center(min_value: vec3<f32>, max_value: vec3<f32>) -> vec3<f32> {
  return mix(min_value, max_value, vec3<f32>(0.5, 0.5, 0.5));
}

fn wr_bounds3_size(min_value: vec3<f32>, max_value: vec3<f32>) -> vec3<f32> {
  return max_value - min_value;
}

fn wr_safe_normalize2(value: vec2<f32>) -> vec2<f32> {
  let len_sq = wr_vec2_length_sq(value);
  if len_sq == 0.0 {
    return vec2<f32>(0.0, 0.0);
  }
  return value * inverseSqrt(len_sq);
}

fn wr_safe_normalize3(value: vec3<f32>) -> vec3<f32> {
  let len_sq = wr_vec3_length_sq(value);
  if len_sq == 0.0 {
    return vec3<f32>(0.0, 0.0, 0.0);
  }
  return value * inverseSqrt(len_sq);
}

fn wr_surface_normalize3(value: vec3<f32>) -> vec3<f32> {
  let len = length(value);
  if len <= WR_SURFACE_NORMAL_EPSILON {
    return vec3<f32>(0.0, 0.0, 1.0);
  }
  return value / len;
}

fn wr_splat_period(period: f32) -> vec3<f32> {
  return vec3<f32>(period, period, period);
}

fn wr_hash_identity_i32(hash: u32, value: i32) -> u32 {
  let zigzag = bitcast<u32>((value << 1) ^ (value >> 31));
  let mixed_lo = (hash ^ zigzag) * WR_IDENTITY_HASH_PRIME;
  return mixed_lo * WR_IDENTITY_HASH_PRIME;
}

fn wr_hash_identity_f32(hash: u32, value: f32) -> u32 {
  return (hash ^ bitcast<u32>(value)) * WR_IDENTITY_HASH_PRIME;
}

fn wr_finalize_identity_hash(hash: u32) -> u32 {
  return max(hash, 1u);
}

fn wr_chain_identity_component(current: u32, component: u32) -> u32 {
  if component == 0u {
    return current;
  }
  if current == 0u {
    return component;
  }
  let mixed = (current ^ component) * 16777619u;
  return max(mixed, 1u);
}

fn wr_transform3_identity() -> Transform3 {
  let identity = mat4x4<f32>(
    vec4<f32>(1.0, 0.0, 0.0, 0.0),
    vec4<f32>(0.0, 1.0, 0.0, 0.0),
    vec4<f32>(0.0, 0.0, 1.0, 0.0),
    vec4<f32>(0.0, 0.0, 0.0, 1.0),
  );
  return Transform3(identity, identity);
}

fn wr_compose_transform3(left: Transform3, right: Transform3) -> Transform3 {
  return Transform3(left.matrix * right.matrix, right.inverse * left.inverse);
}

fn wr_inverse_transform3(transform: Transform3) -> Transform3 {
  return Transform3(transform.inverse, transform.matrix);
}

fn wr_transform_point(transform: Transform3, point: vec3<f32>) -> vec3<f32> {
  return (transform.matrix * vec4<f32>(point, 1.0)).xyz;
}

fn wr_transform_vector(transform: Transform3, vector: vec3<f32>) -> vec3<f32> {
  return (transform.matrix * vec4<f32>(vector, 0.0)).xyz;
}

fn wr_transform_normal(transform: Transform3, normal: vec3<f32>) -> vec3<f32> {
  let out = transpose(transform.inverse) * vec4<f32>(normal, 0.0);
  return wr_safe_normalize3(out.xyz);
}

fn wr_field_transform_point(transform: Transform3, point: vec3<f32>) -> vec3<f32> {
  return (transform.inverse * vec4<f32>(point, 1.0)).xyz;
}

fn wr_field_transform_vector(transform: Transform3, vector: vec3<f32>) -> vec3<f32> {
  return (transform.inverse * vec4<f32>(vector, 0.0)).xyz;
}

fn wr_translate(offset: vec3<f32>, point: vec3<f32>) -> vec3<f32> {
  return point - offset;
}

fn wr_uniform_scale(scale: f32, point: vec3<f32>) -> vec3<f32> {
  if scale == 0.0 {
    return vec3<f32>(0.0, 0.0, 0.0);
  }
  return point / scale;
}

fn wr_affine_transform(transform: Transform3, point: vec3<f32>) -> vec3<f32> {
  return wr_field_transform_point(transform, point);
}

fn wr_warp(transform: Transform3, point: vec3<f32>) -> vec3<f32> {
  return wr_affine_transform(transform, point);
}

fn wr_rotate_vec3_x(point: vec3<f32>, angle: f32) -> vec3<f32> {
  let c = cos(angle);
  let s = sin(angle);
  return vec3<f32>(
    point.x,
    point.y * c - point.z * s,
    point.y * s + point.z * c,
  );
}

fn wr_rotate_vec3_y(point: vec3<f32>, angle: f32) -> vec3<f32> {
  let c = cos(angle);
  let s = sin(angle);
  return vec3<f32>(
    point.x * c - point.z * s,
    point.y,
    point.x * s + point.z * c,
  );
}

fn wr_rotate_vec3_z(point: vec3<f32>, angle: f32) -> vec3<f32> {
  let c = cos(angle);
  let s = sin(angle);
  return vec3<f32>(
    point.x * c - point.y * s,
    point.x * s + point.y * c,
    point.z,
  );
}

fn wr_quat_inverse(quat: vec4<f32>) -> vec4<f32> {
  let len_sq = dot(quat, quat);
  if len_sq == 0.0 {
    return vec4<f32>(0.0, 0.0, 0.0, 1.0);
  }
  return vec4<f32>(-quat.xyz / len_sq, quat.w / len_sq);
}

fn wr_rotate_vec3_by_quat(point: vec3<f32>, quat: vec4<f32>) -> vec3<f32> {
  let u = quat.xyz;
  let uxv = cross(u, point);
  let uuv = cross(u, uxv);
  return point + 2.0 * (quat.w * uxv + uuv);
}

fn wr_rotate_vec3_by_inverse_euler(point: vec3<f32>, euler: vec3<f32>) -> vec3<f32> {
  let z = wr_rotate_vec3_z(point, -euler.z);
  let y = wr_rotate_vec3_y(z, -euler.y);
  return wr_rotate_vec3_x(y, -euler.x);
}

fn wr_rotate_transform3(rotation: Transform3, point: vec3<f32>) -> vec3<f32> {
  return (rotation.inverse * vec4<f32>(point, 1.0)).xyz;
}

fn wr_rotate_mat3(rotation: mat3x3<f32>, point: vec3<f32>) -> vec3<f32> {
  return transpose(rotation) * point;
}

fn wr_rotate_quat(rotation: vec4<f32>, point: vec3<f32>) -> vec3<f32> {
  return wr_rotate_vec3_by_quat(point, wr_quat_inverse(rotation));
}

fn wr_rotate_euler(rotation: vec3<f32>, point: vec3<f32>) -> vec3<f32> {
  return wr_rotate_vec3_by_inverse_euler(point, rotation);
}

fn wr_rotate_angle(rotation: f32, point: vec3<f32>) -> vec3<f32> {
  return wr_rotate_vec3_y(point, -rotation);
}

fn wr_repeat_axis(coord: f32, period: f32) -> f32 {
  if period <= 0.0 {
    return coord;
  }
  return coord - period * floor(coord / period + 0.5);
}

fn wr_repeat_point(point: vec3<f32>, period: vec3<f32>) -> vec3<f32> {
  return vec3<f32>(
    wr_repeat_axis(point.x, period.x),
    wr_repeat_axis(point.y, period.y),
    wr_repeat_axis(point.z, period.z),
  );
}

fn wr_repeat_identity(point: vec3<f32>, period: vec3<f32>) -> u32 {
  var hash = WR_IDENTITY_HASH_OFFSET;
  var has_repeat = false;
  if period.x > 0.0 {
    has_repeat = true;
    hash = wr_hash_identity_i32(hash, i32(floor(point.x / period.x + 0.5)));
  }
  if period.y > 0.0 {
    has_repeat = true;
    hash = wr_hash_identity_i32(hash, i32(floor(point.y / period.y + 0.5)));
  }
  if period.z > 0.0 {
    has_repeat = true;
    hash = wr_hash_identity_i32(hash, i32(floor(point.z / period.z + 0.5)));
  }
  if !has_repeat {
    return 0u;
  }
  return wr_finalize_identity_hash(hash);
}

fn wr_repeat_linear(period: vec3<f32>, point: vec3<f32>) -> vec3<f32> {
  return wr_repeat_point(point, period);
}

fn wr_repeat_linear_scalar(period: f32, point: vec3<f32>) -> vec3<f32> {
  return wr_repeat_point(point, wr_splat_period(period));
}

fn wr_repeat_grid(period: vec3<f32>, point: vec3<f32>) -> vec3<f32> {
  return wr_repeat_point(point, period);
}

fn wr_repeat_grid_scalar(period: f32, point: vec3<f32>) -> vec3<f32> {
  return wr_repeat_point(point, wr_splat_period(period));
}

fn wr_repeat_linear_identity(period: vec3<f32>, point: vec3<f32>) -> u32 {
  return wr_repeat_identity(point, period);
}

fn wr_repeat_linear_identity_scalar(period: f32, point: vec3<f32>) -> u32 {
  return wr_repeat_identity(point, wr_splat_period(period));
}

fn wr_repeat_grid_identity(period: vec3<f32>, point: vec3<f32>) -> u32 {
  return wr_repeat_identity(point, period);
}

fn wr_repeat_grid_identity_scalar(period: f32, point: vec3<f32>) -> u32 {
  return wr_repeat_identity(point, wr_splat_period(period));
}

fn wr_radial_repeat_scalar(period: f32, point: vec3<f32>) -> vec3<f32> {
  if period <= 0.0 {
    return point;
  }
  let radius = length(vec2<f32>(point.x, point.z));
  if radius == 0.0 {
    return vec3<f32>(0.0, point.y, 0.0);
  }
  let angle = atan2(point.z, point.x);
  let sector = WR_TAU / max(period, 1.0);
  let wrapped = fract((angle + 0.5 * sector) / sector) * sector - 0.5 * sector;
  return vec3<f32>(radius * cos(wrapped), point.y, radius * sin(wrapped));
}

fn wr_radial_repeat(period: vec3<f32>, point: vec3<f32>) -> vec3<f32> {
  return wr_radial_repeat_scalar(period.x, point);
}

fn wr_radial_repeat_identity_scalar(period: f32, point: vec3<f32>) -> u32 {
  if period <= 0.0 {
    return 0u;
  }
  let angle = atan2(point.z, point.x);
  let sector = WR_TAU / max(period, 1.0);
  let sector_index = i32(floor((angle + 0.5 * sector) / sector));
  return wr_finalize_identity_hash(wr_hash_identity_i32(WR_IDENTITY_HASH_OFFSET, sector_index));
}

fn wr_radial_repeat_identity(period: vec3<f32>, point: vec3<f32>) -> u32 {
  return wr_radial_repeat_identity_scalar(period.x, point);
}

fn wr_field_mirror_point(mirror: vec3<f32>, point: vec3<f32>) -> vec3<f32> {
  let len_sq = dot(mirror, mirror);
  if len_sq == 0.0 {
    return point;
  }
  let unit = mirror * inverseSqrt(len_sq);
  let distance = dot(point, unit);
  if distance >= 0.0 {
    return point;
  }
  return point - unit * (2.0 * distance);
}

fn wr_mirror_array(mirror: vec3<f32>, point: vec3<f32>) -> vec3<f32> {
  return wr_field_mirror_point(mirror, point);
}

fn wr_mirror_array_identity(mirror: vec3<f32>, point: vec3<f32>) -> u32 {
  let len_sq = dot(mirror, mirror);
  if len_sq == 0.0 {
    return 0u;
  }
  let unit = mirror * inverseSqrt(len_sq);
  let distance = dot(point, unit);
  var branch_id = 2;
  if distance >= 0.0 {
    branch_id = 1;
  }
  return wr_finalize_identity_hash(wr_hash_identity_i32(WR_IDENTITY_HASH_OFFSET, branch_id));
}

fn wr_instance_array_translation(translation: vec3<f32>, point: vec3<f32>) -> vec3<f32> {
  return wr_translate(translation, point);
}

fn wr_instance_array(transform: Transform3, point: vec3<f32>) -> vec3<f32> {
  return wr_field_transform_point(transform, point);
}

fn wr_instance_array_identity_translation(translation: vec3<f32>) -> u32 {
  var hash = WR_IDENTITY_HASH_OFFSET;
  hash = wr_hash_identity_f32(hash, translation.x);
  hash = wr_hash_identity_f32(hash, translation.y);
  hash = wr_hash_identity_f32(hash, translation.z);
  return wr_finalize_identity_hash(hash);
}

fn wr_instance_array_identity(transform: Transform3) -> u32 {
  var hash = WR_IDENTITY_HASH_OFFSET;
  for (var column = 0; column < 4; column = column + 1) {
    let matrix_col = transform.matrix[column];
    hash = wr_hash_identity_f32(hash, matrix_col.x);
    hash = wr_hash_identity_f32(hash, matrix_col.y);
    hash = wr_hash_identity_f32(hash, matrix_col.z);
    hash = wr_hash_identity_f32(hash, matrix_col.w);
  }
  for (var column = 0; column < 4; column = column + 1) {
    let inverse_col = transform.inverse[column];
    hash = wr_hash_identity_f32(hash, inverse_col.x);
    hash = wr_hash_identity_f32(hash, inverse_col.y);
    hash = wr_hash_identity_f32(hash, inverse_col.z);
    hash = wr_hash_identity_f32(hash, inverse_col.w);
  }
  return wr_finalize_identity_hash(hash);
}

fn wr_field_union(left: f32, right: f32) -> f32 {
  return min(left, right);
}

fn wr_field_intersection(left: f32, right: f32) -> f32 {
  return max(left, right);
}

fn wr_field_subtract(left: f32, right: f32) -> f32 {
  return max(left, -right);
}

fn wr_smooth_union_core(left: f32, right: f32, k: f32) -> f32 {
  if k <= 0.0 {
    return min(left, right);
  }
  let h = clamp(0.5 + 0.5 * (right - left) / k, 0.0, 1.0);
  return right + (left - right) * h - k * h * (1.0 - h);
}

fn wr_smooth_union(left: f32, right: f32, k: f32) -> f32 {
  return wr_smooth_union_core(left, right, k);
}

fn wr_smooth_intersection(left: f32, right: f32, k: f32) -> f32 {
  return -wr_smooth_union_core(-left, -right, k);
}

fn wr_smooth_subtract(left: f32, right: f32, k: f32) -> f32 {
  return wr_smooth_union_core(left, -right, k);
}

fn wr_bend(amount: f32, point: vec3<f32>) -> vec3<f32> {
  if amount == 0.0 {
    return point;
  }
  let angle = amount * point.x;
  let c = cos(angle);
  let s = sin(angle);
  return vec3<f32>(
    point.x,
    point.y * c - point.z * s,
    point.y * s + point.z * c,
  );
}

fn wr_twist(amount: f32, point: vec3<f32>) -> vec3<f32> {
  if amount == 0.0 {
    return point;
  }
  return wr_rotate_vec3_y(point, amount * point.y);
}

fn wr_taper(amount: f32, point: vec3<f32>) -> vec3<f32> {
  let scale = 1.0 + amount * point.y;
  return vec3<f32>(point.x * scale, point.y, point.z * scale);
}

fn wr_displace_scalar(offset: f32, point: vec3<f32>) -> vec3<f32> {
  return point + vec3<f32>(offset, offset, offset);
}

fn wr_displace(offset: vec3<f32>, point: vec3<f32>) -> vec3<f32> {
  return point + offset;
}

fn wr_profile_bounds4_init(vertex: vec2<f32>) -> vec4<f32> {
  return vec4<f32>(vertex.x, vertex.y, vertex.x, vertex.y);
}

fn wr_profile_bounds4_extend(bounds: vec4<f32>, vertex: vec2<f32>) -> vec4<f32> {
  return vec4<f32>(
    min(bounds.x, vertex.x),
    min(bounds.y, vertex.y),
    max(bounds.z, vertex.x),
    max(bounds.w, vertex.y),
  );
}

fn wr_profile_bounds4_center(bounds: vec4<f32>) -> vec2<f32> {
  return wr_bounds2_center(bounds.xy, bounds.zw);
}

fn wr_profile_bounds4_size(bounds: vec4<f32>) -> vec2<f32> {
  return wr_bounds2_size(bounds.xy, bounds.zw);
}

fn wr_field_sweep_coords(path: vec3<f32>, point: vec3<f32>) -> vec3<f32> {
  let path_len = length(path);
  if path_len == 0.0 {
    return vec3<f32>(point.x, point.z, 0.0);
  }
  let direction = path / path_len;
  var up = vec3<f32>(1.0, 0.0, 0.0);
  if abs(direction.y) < 0.999 {
    up = vec3<f32>(0.0, 1.0, 0.0);
  }
  let tangent_u = wr_safe_normalize3(cross(up, direction));
  let tangent_v = cross(direction, tangent_u);
  return vec3<f32>(
    dot(point, tangent_u),
    dot(point, tangent_v),
    dot(point, direction),
  );
}

fn wr_profile_cap_distance(profile_distance: f32, axial_distance: f32) -> f32 {
  let outside = length(vec2<f32>(max(profile_distance, 0.0), max(axial_distance, 0.0)));
  let inside = min(max(profile_distance, axial_distance), 0.0);
  return outside + inside;
}

fn wr_circle2(point: vec2<f32>, radius: f32) -> f32 {
  return length(point) - radius;
}

fn wr_rect2(point: vec2<f32>, half: vec2<f32>) -> f32 {
  let q = abs(point) - abs(half);
  let outside = length(max(q, vec2<f32>(0.0, 0.0)));
  let inside = min(max(q.x, q.y), 0.0);
  return outside + inside;
}

fn wr_rounded_rect2(point: vec2<f32>, half: vec2<f32>, radius: f32) -> f32 {
  return wr_rect2(point, half) - radius;
}

fn wr_capsule2(point: vec2<f32>, a: vec2<f32>, b: vec2<f32>, radius: f32) -> f32 {
  let ab = b - a;
  let ap = point - a;
  let denom = dot(ab, ab);
  var t = 0.0;
  if denom != 0.0 {
    t = clamp(dot(ap, ab) / denom, 0.0, 1.0);
  }
  let closest = a + ab * t;
  return length(point - closest) - radius;
}

fn wr_segment2(point: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> f32 {
  return wr_capsule2(point, a, b, 0.0);
}

fn wr_polygon2_edge_distance(point: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> f32 {
  return wr_segment2(point, a, b);
}

fn wr_polygon2_edge_crosses(point: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> bool {
  return ((a.y > point.y) != (b.y > point.y)) &&
    (point.x < (b.x - a.x) * (point.y - a.y) / (b.y - a.y + WR_F32_EPSILON) + a.x);
}

fn wr_polygon2_finalize(best: f32, inside: bool) -> f32 {
  if inside {
    return -best;
  }
  return best;
}

fn wr_polyline2_edge_distance(point: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> f32 {
  return wr_segment2(point, a, b);
}

fn wr_sphere(point: vec3<f32>, radius: f32) -> f32 {
  return length(point) - radius;
}

fn wr_box(point: vec3<f32>, half: vec3<f32>) -> f32 {
  let q = abs(point) - abs(half);
  let outside = length(max(q, vec3<f32>(0.0, 0.0, 0.0)));
  let inside = min(max(q.x, max(q.y, q.z)), 0.0);
  return outside + inside;
}

fn wr_rounded_box(point: vec3<f32>, half: vec3<f32>, radius: f32) -> f32 {
  return wr_box(point, half) - radius;
}

fn wr_capsule(point: vec3<f32>, a: vec3<f32>, b: vec3<f32>, radius: f32) -> f32 {
  let pa = point - a;
  let ba = b - a;
  let ba_dot = dot(ba, ba);
  var h = 0.0;
  if ba_dot != 0.0 {
    h = clamp(dot(pa, ba) / ba_dot, 0.0, 1.0);
  }
  let closest = ba * h;
  return length(pa - closest) - radius;
}

fn wr_cylinder(point: vec3<f32>, radius: f32, half_height: f32) -> f32 {
  let radial = length(vec2<f32>(point.x, point.z)) - radius;
  let vertical = abs(point.y) - half_height;
  let outside = length(vec2<f32>(max(radial, 0.0), max(vertical, 0.0)));
  let inside = min(max(radial, vertical), 0.0);
  return outside + inside;
}

fn wr_plane(point: vec3<f32>, normal: vec3<f32>, offset: f32) -> f32 {
  let normal_len = length(normal);
  if normal_len == 0.0 {
    return 0.0;
  }
  return dot(point, normal / normal_len) + offset;
}

fn wr_torus(point: vec3<f32>, major_radius: f32, minor_radius: f32) -> f32 {
  let radial = length(vec2<f32>(point.x, point.z)) - major_radius;
  return length(vec2<f32>(radial, point.y)) - minor_radius;
}

fn wr_ellipsoid(point: vec3<f32>, radii: vec3<f32>) -> f32 {
  if radii.x == 0.0 || radii.y == 0.0 || radii.z == 0.0 {
    return 0.0;
  }
  let q0 = pow(point.x / radii.x, 2.0) +
    pow(point.y / radii.y, 2.0) +
    pow(point.z / radii.z, 2.0);
  let q1 = sqrt(
    pow(point.x / (radii.x * radii.x), 2.0) +
    pow(point.y / (radii.y * radii.y), 2.0) +
    pow(point.z / (radii.z * radii.z), 2.0)
  );
  if q1 == 0.0 {
    return -min(abs(radii.x), min(abs(radii.y), abs(radii.z)));
  }
  let root_q0 = sqrt(q0);
  return root_q0 * (root_q0 - 1.0) / q1;
}

fn wr_cone(point: vec3<f32>, radius: f32, half_height: f32) -> f32 {
  if half_height == 0.0 {
    return 0.0;
  }
  let radial = length(vec2<f32>(point.x, point.z));
  let height = abs(half_height) * 2.0;
  let slope = radius / height;
  return max(radial - slope * (half_height - point.y), point.y - half_height);
}

fn wr_capped_cone(point: vec3<f32>, radius_bottom: f32, radius_top: f32, half_height: f32) -> f32 {
  if half_height == 0.0 {
    return 0.0;
  }
  let hh = abs(half_height);
  let q = vec2<f32>(length(vec2<f32>(point.x, point.z)), point.y);
  let k1 = vec2<f32>(radius_top, hh);
  let k2 = vec2<f32>(radius_top - radius_bottom, 2.0 * hh);
  var clamped_radius = radius_top;
  if q.y < 0.0 {
    clamped_radius = radius_bottom;
  }
  let ca = vec2<f32>(
    q.x - min(q.x, clamped_radius),
    abs(q.y) - hh,
  );
  let denom = dot(k2, k2);
  if denom == 0.0 {
    return max(q.x - radius_bottom, abs(q.y) - hh);
  }
  let t = clamp(dot(k1 - q, k2) / denom, 0.0, 1.0);
  let cb = q - k1 + k2 * t;
  let ca_len_sq = dot(ca, ca);
  let cb_len_sq = dot(cb, cb);
  var sign = 1.0;
  if cb.x < 0.0 && ca.y < 0.0 {
    sign = -1.0;
  }
  return sign * sqrt(min(ca_len_sq, cb_len_sq));
}

fn wr_box_frame(point: vec3<f32>, half: vec3<f32>, thickness: f32) -> f32 {
  let inner = max(half - vec3<f32>(thickness, thickness, thickness), vec3<f32>(0.0, 0.0, 0.0));
  let outer = wr_box(point, half);
  let inner_dist = wr_box(point, inner);
  return max(outer, -inner_dist);
}

fn wr_slab(point: vec3<f32>, thickness: f32) -> f32 {
  return abs(point.y) - abs(thickness) * 0.5;
}

fn wr_triangle_prism(point: vec3<f32>, half: vec2<f32>, half_height: f32) -> f32 {
  let qx = abs(point.x);
  let qy = abs(point.y);
  let qz = point.z;
  let tri = max(qx * 0.8660254 + qz * 0.5, -qz) - half.x;
  return max(tri, max(qy - abs(half_height), abs(point.z) - half.y));
}

fn wr_hex_prism(point: vec3<f32>, half: vec2<f32>, half_height: f32) -> f32 {
  let qx = abs(point.x);
  let qy = abs(point.y);
  let qz = abs(point.z);
  let hex = max(qx * 0.8660254 + qz * 0.5, qz) - half.x;
  return max(hex, max(qy - abs(half_height), qz - half.y));
}

fn wr_default_medium() -> Medium {
  return Medium(0.0, vec3<f32>(0.0, 0.0, 0.0), 0.0);
}

fn wr_medium(density: f32, emission: vec3<f32>, anisotropy: f32) -> Medium {
  return Medium(density, emission, anisotropy);
}

fn wr_combine_medium_values(current: Medium, next: Medium) -> Medium {
  let density = current.density + next.density;
  let emission = current.emission + next.emission;
  var anisotropy = 0.0;
  if density > 0.0 {
    anisotropy = (current.anisotropy * current.density + next.anisotropy * next.density) / density;
  }
  return Medium(density, emission, anisotropy);
}

fn wr_normalize3(value: vec3<f32>) -> vec3<f32> {
  let len = length(value);
  if len <= WR_SURFACE_NORMAL_EPSILON {
    return vec3<f32>(0.0, 0.0, 1.0);
  }
  return value / len;
}

fn wr_stable_surface_frame(position: vec3<f32>, normal: vec3<f32>) -> Transform3 {
  let unit_normal = wr_surface_normalize3(normal);
  let world_up = vec3<f32>(0.0, 1.0, 0.0);
  let world_right = vec3<f32>(1.0, 0.0, 0.0);
  let tangent_seed = cross(world_up, unit_normal);
  var tangent = wr_surface_normalize3(tangent_seed);
  if dot(tangent_seed, tangent_seed) <= WR_SURFACE_NORMAL_EPSILON {
    tangent = wr_surface_normalize3(cross(world_right, unit_normal));
  }
  let bitangent = cross(unit_normal, tangent);
  let inverse_translation = vec4<f32>(
    -dot(tangent, position),
    -dot(bitangent, position),
    -dot(unit_normal, position),
    1.0,
  );
  let matrix = mat4x4<f32>(
    vec4<f32>(tangent, 0.0),
    vec4<f32>(bitangent, 0.0),
    vec4<f32>(unit_normal, 0.0),
    vec4<f32>(position, 1.0),
  );
  let inverse = mat4x4<f32>(
    vec4<f32>(tangent.x, bitangent.x, unit_normal.x, 0.0),
    vec4<f32>(tangent.y, bitangent.y, unit_normal.y, 0.0),
    vec4<f32>(tangent.z, bitangent.z, unit_normal.z, 0.0),
    inverse_translation,
  );
  return Transform3(matrix, inverse);
}

fn wr_default_actor_handle() -> ActorHandle {
  return ActorHandle(0u, 0u);
}

fn wr_default_payload() -> Payload {
  return Payload(0u, 0u, wr_default_actor_handle());
}

fn wr_default_surface() -> Surface {
  return Surface(
    vec3<f32>(0.0, 0.0, 0.0),
    0.0,
    0.0,
    0.0,
    0.0,
    0.0,
    vec3<f32>(0.0, 0.0, 0.0),
  );
}

fn wr_hit_value(
  hit: bool,
  distance: f32,
  position: vec3<f32>,
  normal: vec3<f32>,
  local_position: vec3<f32>,
  local_normal: vec3<f32>,
  steps: i32,
  feature_id: u32,
  instance_id: u32,
  repeat_id: u32,
  root_shape_id: u32,
  payload: Payload,
) -> Hit3 {
  let shading_frame = wr_stable_surface_frame(position, normal);
  return Hit3(
    hit,
    distance,
    position,
    normal,
    local_position,
    local_normal,
    shading_frame,
    steps,
    feature_id,
    instance_id,
    repeat_id,
    root_shape_id,
    payload,
  );
}

fn wr_default_hit(origin: vec3<f32>) -> Hit3 {
  return wr_hit_value(
    false,
    0.0,
    origin,
    vec3<f32>(0.0, 0.0, 1.0),
    origin,
    vec3<f32>(0.0, 0.0, 1.0),
    0,
    0u,
    0u,
    0u,
    0u,
    wr_default_payload(),
  );
}

fn wr_occlusion_result_from_hit(hit: Hit3) -> OcclusionResult {
  return OcclusionResult(hit.hit, hit.distance, hit.steps);
}
