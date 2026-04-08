use super::class;
use super::list;
use super::math;
use super::value::{TypeId, Value, type_id_raw};
use crate::kernel::metrics;

const BOUNDS2_FIELDS: [&str; 2] = ["min", "max"];
const BOUNDS3_FIELDS: [&str; 2] = ["min", "max"];
const TRANSFORM3_FIELDS: [&str; 2] = ["matrix", "inverse"];

pub fn bounds2_center(bounds: Value) -> Value {
    bounds_center(bounds, TypeId::Vec2, &BOUNDS2_FIELDS)
}

pub fn bounds2_size(bounds: Value) -> Value {
    bounds_size(bounds, TypeId::Vec2, &BOUNDS2_FIELDS)
}

pub fn bounds3_center(bounds: Value) -> Value {
    bounds_center(bounds, TypeId::Vec3, &BOUNDS3_FIELDS)
}

pub fn bounds3_size(bounds: Value) -> Value {
    bounds_size(bounds, TypeId::Vec3, &BOUNDS3_FIELDS)
}

pub fn transform_point(transform: Value, point: Value) -> Value {
    let Some((matrix, _inverse)) = transform3_fields(transform) else {
        return Value::nil();
    };
    let Some(point) = vec3_components(point) else {
        return Value::nil();
    };
    let Some(matrix) = mat4_components(matrix) else {
        return Value::nil();
    };
    let out = mat4_mul_vec4(matrix, [point[0], point[1], point[2], 1.0]);
    vec3_value(out[0], out[1], out[2])
}

pub fn transform_vector(transform: Value, vector: Value) -> Value {
    let Some((matrix, _inverse)) = transform3_fields(transform) else {
        return Value::nil();
    };
    let Some(vector) = vec3_components(vector) else {
        return Value::nil();
    };
    let Some(matrix) = mat4_components(matrix) else {
        return Value::nil();
    };
    let out = mat4_mul_vec4(matrix, [vector[0], vector[1], vector[2], 0.0]);
    vec3_value(out[0], out[1], out[2])
}

pub fn transform_normal(transform: Value, normal: Value) -> Value {
    let Some((_matrix, inverse)) = transform3_fields(transform) else {
        return Value::nil();
    };
    let Some(normal) = vec3_components(normal) else {
        return Value::nil();
    };
    let Some(inverse) = mat4_components(inverse) else {
        return Value::nil();
    };
    let transpose = mat4_transpose(inverse);
    let out = mat4_mul_vec4(transpose, [normal[0], normal[1], normal[2], 0.0]);
    let len_sq = out[0] * out[0] + out[1] * out[1] + out[2] * out[2];
    if len_sq == 0.0 {
        return vec3_value(0.0, 0.0, 0.0);
    }
    let len = len_sq.sqrt();
    vec3_value(out[0] / len, out[1] / len, out[2] / len)
}

pub fn transform3_identity(class_id: u32) -> Value {
    build_transform3(class_id, math::mat4_identity(), math::mat4_identity())
}

pub fn compose_transform3(class_id: u32, left: Value, right: Value) -> Value {
    let Some((left_matrix, left_inverse)) = transform3_fields(left) else {
        return Value::nil();
    };
    let Some((right_matrix, right_inverse)) = transform3_fields(right) else {
        return Value::nil();
    };
    let matrix = math::mat4_mul_mat4(left_matrix, right_matrix);
    let inverse = math::mat4_mul_mat4(right_inverse, left_inverse);
    if matrix.is_nil() || inverse.is_nil() {
        return Value::nil();
    }
    build_transform3(class_id, matrix, inverse)
}

pub fn inverse_transform3(class_id: u32, transform: Value) -> Value {
    let Some((matrix, inverse)) = transform3_fields(transform) else {
        return Value::nil();
    };
    build_transform3(class_id, inverse, matrix)
}

pub fn field_transform_point(transform: Value, point: Value) -> Value {
    let Some(point) = vec3_components(point) else {
        return Value::nil();
    };
    if let Some(translation) = vec3_components(transform) {
        let out = vec3_sub(point, translation);
        return vec3_value(out[0], out[1], out[2]);
    }
    let Some((_matrix, inverse)) = transform3_fields(transform) else {
        return Value::nil();
    };
    let Some(inverse) = mat4_components(inverse) else {
        return Value::nil();
    };
    let out = mat4_mul_vec4(inverse, [point[0], point[1], point[2], 1.0]);
    vec3_value(out[0], out[1], out[2])
}

pub fn field_instance_point(instance: Value, point: Value) -> Value {
    field_transform_point(instance, point)
}

pub fn field_mirror_point(mirror: Value, point: Value) -> Value {
    let Some(point) = vec3_components(point) else {
        return Value::nil();
    };
    let Some(normal) = vec3_components(mirror) else {
        return Value::nil();
    };
    let len_sq = vec3_dot(normal, normal);
    if len_sq == 0.0 {
        return vec3_value(point[0], point[1], point[2]);
    }
    let inv_len = len_sq.sqrt().recip();
    let unit = vec3_scale(normal, inv_len);
    let distance = vec3_dot(point, unit);
    if distance >= 0.0 {
        return vec3_value(point[0], point[1], point[2]);
    }
    let reflected = vec3_sub(point, vec3_scale(unit, 2.0 * distance));
    vec3_value(reflected[0], reflected[1], reflected[2])
}

pub fn field_repeat_point(period: Value, point: Value) -> Value {
    repeat_point(point, period)
}

pub fn field_sweep_coords(path: Value, point: Value) -> Value {
    let Some(path) = vec3_components(path) else {
        return Value::nil();
    };
    let Some(point) = vec3_components(point) else {
        return Value::nil();
    };
    let path_len = vec3_length(path);
    if path_len == 0.0 {
        return vec3_value(point[0], point[2], 0.0);
    }

    let direction = vec3_scale(path, path_len.recip());
    let up = if direction[1].abs() < 0.999 {
        [0.0, 1.0, 0.0]
    } else {
        [1.0, 0.0, 0.0]
    };
    let tangent_u = vec3_normalize(vec3_cross(up, direction));
    let tangent_v = vec3_cross(direction, tangent_u);

    vec3_value(
        vec3_dot(point, tangent_u),
        vec3_dot(point, tangent_v),
        vec3_dot(point, direction),
    )
}

pub fn field_profile_vertices_bounds4(vertices: Value) -> Value {
    let Some(vertices) = list::as_list_ref(vertices) else {
        return Value::nil();
    };
    let vertices = unsafe { &(*vertices).data };
    let Some(first) = vertices.first().and_then(|value| vec2_components(*value)) else {
        return Value::nil();
    };
    let mut min_x = first[0];
    let mut min_y = first[1];
    let mut max_x = first[0];
    let mut max_y = first[1];
    for vertex in vertices.iter().skip(1) {
        let Some(vertex) = vec2_components(*vertex) else {
            return Value::nil();
        };
        min_x = min_x.min(vertex[0]);
        min_y = min_y.min(vertex[1]);
        max_x = max_x.max(vertex[0]);
        max_y = max_y.max(vertex[1]);
    }
    vec4_value(min_x, min_y, max_x, max_y)
}

pub fn translate(offset: Value, point: Value) -> Value {
    field_transform_point(offset, point)
}

pub fn rotate(rotation: Value, point: Value) -> Value {
    let Some(point) = vec3_components(point) else {
        return Value::nil();
    };
    if let Some((_matrix, inverse)) = transform3_fields(rotation) {
        let Some(inverse) = mat4_components(inverse) else {
            return Value::nil();
        };
        let out = mat4_mul_vec4(inverse, [point[0], point[1], point[2], 0.0]);
        return vec3_value(out[0], out[1], out[2]);
    }
    if let Some(matrix) = mat3_components(rotation) {
        let out = mat3_mul_vec3(mat3_transpose(matrix), point);
        return vec3_value(out[0], out[1], out[2]);
    }
    if let Some(q) = vec4_components(rotation) {
        let out = rotate_vec3_by_quat(point, quat_inverse(q));
        return vec3_value(out[0], out[1], out[2]);
    }
    if let Some(euler) = vec3_components(rotation) {
        let out = rotate_vec3_by_inverse_euler(point, euler);
        return vec3_value(out[0], out[1], out[2]);
    }
    if let Some(angle) = component_f32(rotation) {
        let out = rotate_vec3_y(point, -angle);
        return vec3_value(out[0], out[1], out[2]);
    }
    Value::nil()
}

pub fn field_rotate_point(rotation: Value, point: Value) -> Value {
    let Some(point) = vec3_components(point) else {
        return Value::nil();
    };
    if let Some((_matrix, inverse)) = transform3_fields(rotation) {
        let Some(inverse) = mat4_components(inverse) else {
            return Value::nil();
        };
        let out = mat4_mul_vec4(inverse, [point[0], point[1], point[2], 1.0]);
        return vec3_value(out[0], out[1], out[2]);
    }
    if let Some(matrix) = mat3_components(rotation) {
        let out = mat3_mul_vec3(mat3_transpose(matrix), point);
        return vec3_value(out[0], out[1], out[2]);
    }
    if let Some(q) = vec4_components(rotation) {
        let out = rotate_vec3_by_quat(point, quat_inverse(q));
        return vec3_value(out[0], out[1], out[2]);
    }
    if let Some(euler) = vec3_components(rotation) {
        let out = rotate_vec3_by_inverse_euler(point, euler);
        return vec3_value(out[0], out[1], out[2]);
    }
    if let Some(angle) = component_f32(rotation) {
        let out = rotate_vec3_y(point, -angle);
        return vec3_value(out[0], out[1], out[2]);
    }
    Value::nil()
}

pub fn uniform_scale(scale: Value, point: Value) -> Value {
    let Some(point) = vec3_components(point) else {
        return Value::nil();
    };
    let Some(scale) = component_f32(scale) else {
        return Value::nil();
    };
    if scale == 0.0 {
        return Value::nil();
    }
    vec3_value(point[0] / scale, point[1] / scale, point[2] / scale)
}

pub fn affine_transform(transform: Value, point: Value) -> Value {
    field_transform_point(transform, point)
}

pub fn warp(transform: Value, point: Value) -> Value {
    affine_transform(transform, point)
}

pub fn repeat_linear(period: Value, point: Value) -> Value {
    repeat_point(point, splat_period(period))
}

pub fn repeat_grid(period: Value, point: Value) -> Value {
    repeat_point(point, splat_period(period))
}

pub fn repeat_linear_identity(period: Value, point: Value) -> Value {
    repeat_identity(splat_period(period), point)
}

pub fn repeat_grid_identity(period: Value, point: Value) -> Value {
    repeat_identity(splat_period(period), point)
}

pub fn radial_repeat(period: Value, point: Value) -> Value {
    let Some(point) = vec3_components(point) else {
        return Value::nil();
    };
    let Some(period) = component_f32(period).or_else(|| vec3_components(period).map(|v| v[0]))
    else {
        return Value::nil();
    };
    if period <= 0.0 {
        return vec3_value(point[0], point[1], point[2]);
    }
    let radius = (point[0] * point[0] + point[2] * point[2]).sqrt();
    if radius == 0.0 {
        return vec3_value(0.0, point[1], 0.0);
    }
    let angle = point[2].atan2(point[0]);
    let sector = std::f32::consts::TAU / period.max(1.0);
    let wrapped = (angle + 0.5 * sector).rem_euclid(sector) - 0.5 * sector;
    vec3_value(radius * wrapped.cos(), point[1], radius * wrapped.sin())
}

pub fn radial_repeat_identity(period: Value, point: Value) -> Value {
    let Some(point) = vec3_components(point) else {
        return Value::nil();
    };
    let Some(period) = component_f32(period).or_else(|| vec3_components(period).map(|v| v[0]))
    else {
        return Value::nil();
    };
    if period <= 0.0 {
        return Value::from_int(0);
    }
    let angle = point[2].atan2(point[0]);
    let sector = std::f32::consts::TAU / period.max(1.0);
    let sector_index = ((angle + 0.5 * sector) / sector).floor() as i64;
    finalize_identity_hash(hash_identity_i64(IDENTITY_HASH_OFFSET, sector_index))
}

pub fn mirror_array(mirror: Value, point: Value) -> Value {
    field_mirror_point(mirror, point)
}

pub fn mirror_array_identity(mirror: Value, point: Value) -> Value {
    let Some(point) = vec3_components(point) else {
        return Value::nil();
    };
    let Some(normal) = vec3_components(mirror) else {
        return Value::nil();
    };
    let len_sq = vec3_dot(normal, normal);
    if len_sq == 0.0 {
        return Value::from_int(0);
    }
    let inv_len = len_sq.sqrt().recip();
    let unit = vec3_scale(normal, inv_len);
    let distance = vec3_dot(point, unit);
    let branch_id = if distance >= 0.0 { 1_i64 } else { 2_i64 };
    finalize_identity_hash(hash_identity_i64(IDENTITY_HASH_OFFSET, branch_id))
}

pub fn instance_array(instance: Value, point: Value) -> Value {
    field_transform_point(instance, point)
}

pub fn instance_array_identity(instance: Value, _point: Value) -> Value {
    if let Some(translation) = vec3_components(instance) {
        let mut hash = IDENTITY_HASH_OFFSET;
        hash = hash_identity_f32(hash, translation[0]);
        hash = hash_identity_f32(hash, translation[1]);
        hash = hash_identity_f32(hash, translation[2]);
        return finalize_identity_hash(hash);
    }
    let Some((matrix, inverse)) = transform3_fields(instance) else {
        return Value::from_int(0);
    };
    let Some(matrix) = mat4_components(matrix) else {
        return Value::from_int(0);
    };
    let Some(inverse) = mat4_components(inverse) else {
        return Value::from_int(0);
    };
    let mut hash = IDENTITY_HASH_OFFSET;
    for value in matrix.into_iter().chain(inverse) {
        hash = hash_identity_f32(hash, value);
    }
    finalize_identity_hash(hash)
}

pub fn smooth_union(left: Value, right: Value, k: Value) -> Value {
    metrics::inc_scene_trace_blend_cost();
    smooth_boolean(left, right, k, SmoothBooleanOp::Union)
}

pub fn smooth_intersection(left: Value, right: Value, k: Value) -> Value {
    metrics::inc_scene_trace_blend_cost();
    smooth_boolean(left, right, k, SmoothBooleanOp::Intersection)
}

pub fn smooth_subtract(left: Value, right: Value, k: Value) -> Value {
    metrics::inc_scene_trace_blend_cost();
    smooth_boolean(left, right, k, SmoothBooleanOp::Subtract)
}

pub fn bend(config: Value, point: Value) -> Value {
    metrics::inc_scene_trace_deformation_cost();
    let Some(point) = vec3_components(point) else {
        return Value::nil();
    };
    let amount = scalar_or_first_component(config).unwrap_or(0.0);
    if amount == 0.0 {
        return vec3_value(point[0], point[1], point[2]);
    }
    let angle = amount * point[0];
    let cos = angle.cos();
    let sin = angle.sin();
    vec3_value(
        point[0],
        point[1] * cos - point[2] * sin,
        point[1] * sin + point[2] * cos,
    )
}

pub fn twist(config: Value, point: Value) -> Value {
    metrics::inc_scene_trace_deformation_cost();
    let Some(point) = vec3_components(point) else {
        return Value::nil();
    };
    let amount = scalar_or_first_component(config).unwrap_or(0.0);
    if amount == 0.0 {
        return vec3_value(point[0], point[1], point[2]);
    }
    let angle = amount * point[1];
    let out = rotate_vec3_y(point, angle);
    vec3_value(out[0], out[1], out[2])
}

pub fn taper(config: Value, point: Value) -> Value {
    metrics::inc_scene_trace_deformation_cost();
    let Some(point) = vec3_components(point) else {
        return Value::nil();
    };
    let amount = scalar_or_first_component(config).unwrap_or(0.0);
    let scale = 1.0 + amount * point[1];
    vec3_value(point[0] * scale, point[1], point[2] * scale)
}

pub fn displace(config: Value, point: Value) -> Value {
    metrics::inc_scene_trace_deformation_cost();
    let Some(point) = vec3_components(point) else {
        return Value::nil();
    };
    if let Some(offset) = vec3_components(config) {
        return vec3_value(
            point[0] + offset[0],
            point[1] + offset[1],
            point[2] + offset[2],
        );
    }
    let Some(offset) = component_f32(config) else {
        return Value::nil();
    };
    vec3_value(point[0] + offset, point[1] + offset, point[2] + offset)
}

pub fn field_union(left: Value, right: Value) -> Value {
    metrics::inc_scene_trace_blend_cost();
    let Some(left) = component_f32(left) else {
        return Value::nil();
    };
    let Some(right) = component_f32(right) else {
        return Value::nil();
    };
    Value::from_float(left.min(right) as f64)
}

pub fn field_intersection(left: Value, right: Value) -> Value {
    metrics::inc_scene_trace_blend_cost();
    let Some(left) = component_f32(left) else {
        return Value::nil();
    };
    let Some(right) = component_f32(right) else {
        return Value::nil();
    };
    Value::from_float(left.max(right) as f64)
}

pub fn field_subtract(left: Value, right: Value) -> Value {
    metrics::inc_scene_trace_blend_cost();
    let Some(left) = component_f32(left) else {
        return Value::nil();
    };
    let Some(right) = component_f32(right) else {
        return Value::nil();
    };
    Value::from_float(left.max(-right) as f64)
}

pub fn rounded_box(point: Value, half: Value, radius: Value) -> Value {
    let Some(radius) = component_f32(radius) else {
        return Value::nil();
    };
    let out = box_sdf(point, half);
    let Some(out) = component_f32(out) else {
        return Value::nil();
    };
    Value::from_float((out - radius) as f64)
}

pub fn circle2(point: Value, radius: Value) -> Value {
    let Some(point) = vec2_components(point) else {
        return Value::nil();
    };
    let Some(radius) = component_f32(radius) else {
        return Value::nil();
    };
    Value::from_float(((point[0] * point[0] + point[1] * point[1]).sqrt() - radius) as f64)
}

pub fn rect2(point: Value, half: Value) -> Value {
    let Some(point) = vec2_components(point) else {
        return Value::nil();
    };
    let Some(half) = vec2_components(half) else {
        return Value::nil();
    };
    let qx = point[0].abs() - half[0].abs();
    let qy = point[1].abs() - half[1].abs();
    let ax = qx.max(0.0);
    let ay = qy.max(0.0);
    let outside = (ax * ax + ay * ay).sqrt();
    let inside = qx.max(qy).min(0.0);
    Value::from_float((outside + inside) as f64)
}

pub fn rounded_rect2(point: Value, half: Value, radius: Value) -> Value {
    let Some(radius) = component_f32(radius) else {
        return Value::nil();
    };
    let dist = rect2(point, half);
    let Some(dist) = component_f32(dist) else {
        return Value::nil();
    };
    Value::from_float((dist - radius) as f64)
}

pub fn capsule2(point: Value, a: Value, b: Value, radius: Value) -> Value {
    let Some(point) = vec2_components(point) else {
        return Value::nil();
    };
    let Some(a) = vec2_components(a) else {
        return Value::nil();
    };
    let Some(b) = vec2_components(b) else {
        return Value::nil();
    };
    let Some(radius) = component_f32(radius) else {
        return Value::nil();
    };
    let ab = vec2_sub(b, a);
    let ap = vec2_sub(point, a);
    let denom = vec2_dot(ab, ab);
    let t = if denom == 0.0 {
        0.0
    } else {
        vec2_dot(ap, ab) / denom
    };
    let t = t.clamp(0.0, 1.0);
    let closest = vec2_add(a, vec2_scale(ab, t));
    let delta = vec2_sub(point, closest);
    Value::from_float(((vec2_dot(delta, delta)).sqrt() - radius) as f64)
}

pub fn segment2(point: Value, a: Value, b: Value) -> Value {
    capsule2(point, a, b, Value::from_float(0.0))
}

pub fn polygon2(point: Value, vertices: Value) -> Value {
    let Some(point) = vec2_components(point) else {
        return Value::nil();
    };
    let Some(vertices) = list::as_list_ref(vertices) else {
        return Value::nil();
    };
    let vertices = unsafe { &(*vertices).data };
    if vertices.len() < 3 {
        return Value::nil();
    }
    let mut inside = false;
    let mut best = f32::INFINITY;
    for i in 0..vertices.len() {
        let a = match vec2_components(vertices[i]) {
            Some(value) => value,
            None => return Value::nil(),
        };
        let b = match vec2_components(vertices[(i + 1) % vertices.len()]) {
            Some(value) => value,
            None => return Value::nil(),
        };
        let edge = vec2_sub(b, a);
        let ap = vec2_sub(point, a);
        let denom = vec2_dot(edge, edge);
        if denom > 0.0 {
            let t = (vec2_dot(ap, edge) / denom).clamp(0.0, 1.0);
            let closest = vec2_add(a, vec2_scale(edge, t));
            let delta = vec2_sub(point, closest);
            best = best.min(vec2_dot(delta, delta).sqrt());
        }
        let crosses = ((a[1] > point[1]) != (b[1] > point[1]))
            && (point[0] < (b[0] - a[0]) * (point[1] - a[1]) / (b[1] - a[1] + f32::EPSILON) + a[0]);
        if crosses {
            inside = !inside;
        }
    }
    Value::from_float((if inside { -best } else { best }) as f64)
}

pub fn polyline2(point: Value, vertices: Value) -> Value {
    let Some(point) = vec2_components(point) else {
        return Value::nil();
    };
    let Some(vertices) = list::as_list_ref(vertices) else {
        return Value::nil();
    };
    let vertices = unsafe { &(*vertices).data };
    if vertices.len() < 2 {
        return Value::nil();
    }
    let mut best = f32::INFINITY;
    for pair in vertices.windows(2) {
        let Some(a) = vec2_components(pair[0]) else {
            return Value::nil();
        };
        let Some(b) = vec2_components(pair[1]) else {
            return Value::nil();
        };
        let ab = vec2_sub(b, a);
        let ap = vec2_sub(point, a);
        let denom = vec2_dot(ab, ab);
        let t = if denom == 0.0 {
            0.0
        } else {
            (vec2_dot(ap, ab) / denom).clamp(0.0, 1.0)
        };
        let closest = vec2_add(a, vec2_scale(ab, t));
        let delta = vec2_sub(point, closest);
        best = best.min(vec2_dot(delta, delta).sqrt());
    }
    Value::from_float(best as f64)
}

pub fn ellipsoid(point: Value, radii: Value) -> Value {
    let Some(point) = vec3_components(point) else {
        return Value::nil();
    };
    let Some(radii) = vec3_components(radii) else {
        return Value::nil();
    };
    if radii[0] == 0.0 || radii[1] == 0.0 || radii[2] == 0.0 {
        return Value::nil();
    }
    let q0 = (point[0] / radii[0]).powi(2)
        + (point[1] / radii[1]).powi(2)
        + (point[2] / radii[2]).powi(2);
    let q1 = ((point[0] / (radii[0] * radii[0])).powi(2)
        + (point[1] / (radii[1] * radii[1])).powi(2)
        + (point[2] / (radii[2] * radii[2])).powi(2))
    .sqrt();
    if q1 == 0.0 {
        let min_radius = radii[0].abs().min(radii[1].abs()).min(radii[2].abs());
        return Value::from_float((-min_radius) as f64);
    }
    Value::from_float((q0.sqrt() * (q0.sqrt() - 1.0) / q1) as f64)
}

pub fn cone(point: Value, radius: Value, half_height: Value) -> Value {
    let Some(point) = vec3_components(point) else {
        return Value::nil();
    };
    let Some(radius) = component_f32(radius) else {
        return Value::nil();
    };
    let Some(half_height) = component_f32(half_height) else {
        return Value::nil();
    };
    if half_height == 0.0 {
        return Value::nil();
    }
    let radial = (point[0] * point[0] + point[2] * point[2]).sqrt();
    let height = half_height.abs() * 2.0;
    let slope = radius / height;
    Value::from_float(
        (radial - slope * (half_height - point[1])).max(point[1] - half_height) as f64,
    )
}

pub fn capped_cone(
    point: Value,
    radius_bottom: Value,
    radius_top: Value,
    half_height: Value,
) -> Value {
    let Some(point) = vec3_components(point) else {
        return Value::nil();
    };
    let Some(radius_bottom) = component_f32(radius_bottom) else {
        return Value::nil();
    };
    let Some(radius_top) = component_f32(radius_top) else {
        return Value::nil();
    };
    let Some(half_height) = component_f32(half_height) else {
        return Value::nil();
    };
    if half_height == 0.0 {
        return Value::nil();
    }
    let half_height = half_height.abs();
    let q = [(point[0] * point[0] + point[2] * point[2]).sqrt(), point[1]];
    let k1 = [radius_top, half_height];
    let k2 = [radius_top - radius_bottom, 2.0 * half_height];
    let ca = [
        q[0] - q[0].min(if q[1] < 0.0 {
            radius_bottom
        } else {
            radius_top
        }),
        q[1].abs() - half_height,
    ];
    let denom = k2[0] * k2[0] + k2[1] * k2[1];
    if denom == 0.0 {
        return Value::from_float(((q[0] - radius_bottom).max(q[1].abs() - half_height)) as f64);
    }
    let t = (((k1[0] - q[0]) * k2[0] + (k1[1] - q[1]) * k2[1]) / denom).clamp(0.0, 1.0);
    let cb = [q[0] - k1[0] + k2[0] * t, q[1] - k1[1] + k2[1] * t];
    let ca_len_sq = ca[0] * ca[0] + ca[1] * ca[1];
    let cb_len_sq = cb[0] * cb[0] + cb[1] * cb[1];
    let sign = if cb[0] < 0.0 && ca[1] < 0.0 {
        -1.0
    } else {
        1.0
    };
    Value::from_float((sign * ca_len_sq.min(cb_len_sq).sqrt()) as f64)
}

pub fn box_frame(point: Value, half: Value, thickness: Value) -> Value {
    let Some(half) = vec3_components(half) else {
        return Value::nil();
    };
    let Some(thickness) = component_f32(thickness) else {
        return Value::nil();
    };
    let inner = vec3_value(
        (half[0] - thickness).max(0.0),
        (half[1] - thickness).max(0.0),
        (half[2] - thickness).max(0.0),
    );
    let outer = box_sdf(point, vec3_value(half[0], half[1], half[2]));
    let inner_dist = box_sdf(point, inner);
    let Some(outer) = component_f32(outer) else {
        return Value::nil();
    };
    let Some(inner_dist) = component_f32(inner_dist) else {
        return Value::nil();
    };
    Value::from_float(outer.max(-inner_dist) as f64)
}

pub fn slab(point: Value, thickness: Value) -> Value {
    let Some(point) = vec3_components(point) else {
        return Value::nil();
    };
    let Some(thickness) = component_f32(thickness) else {
        return Value::nil();
    };
    Value::from_float((point[1].abs() - thickness.abs() * 0.5) as f64)
}

pub fn triangle_prism(point: Value, half: Value, half_height: Value) -> Value {
    let Some(point) = vec3_components(point) else {
        return Value::nil();
    };
    let Some(half) = vec2_components(half) else {
        return Value::nil();
    };
    let Some(half_height) = component_f32(half_height) else {
        return Value::nil();
    };
    let qx = point[0].abs();
    let qy = point[1].abs();
    let qz = point[2];
    let tri = (qx * 0.866_025_4 + qz * 0.5).max(-qz) - half[0];
    Value::from_float(
        tri.max(qy - half_height.abs())
            .max(point[2].abs() - half[1]) as f64,
    )
}

pub fn hex_prism(point: Value, half: Value, half_height: Value) -> Value {
    let Some(point) = vec3_components(point) else {
        return Value::nil();
    };
    let Some(half) = vec2_components(half) else {
        return Value::nil();
    };
    let Some(half_height) = component_f32(half_height) else {
        return Value::nil();
    };
    let qx = point[0].abs();
    let qy = point[1].abs();
    let qz = point[2].abs();
    let hex = (qx * 0.866_025_4 + qz * 0.5).max(qz) - half[0];
    Value::from_float(hex.max(qy - half_height.abs()).max(qz - half[1]) as f64)
}

pub fn repeat_point(point: Value, period: Value) -> Value {
    let Some(point) = vec3_components(point) else {
        return Value::nil();
    };
    let Some(period) = vec3_components(period) else {
        return Value::nil();
    };
    let out = [
        repeat_axis(point[0], period[0]),
        repeat_axis(point[1], period[1]),
        repeat_axis(point[2], period[2]),
    ];
    vec3_value(out[0], out[1], out[2])
}

pub fn sphere(point: Value, radius: Value) -> Value {
    let Some(point) = vec3_components(point) else {
        return Value::nil();
    };
    let Some(radius) = component_f32(radius) else {
        return Value::nil();
    };
    Value::from_float((vec3_length(point) - radius) as f64)
}

pub fn box_sdf(point: Value, half: Value) -> Value {
    let Some(point) = vec3_components(point) else {
        return Value::nil();
    };
    let Some(half) = vec3_components(half) else {
        return Value::nil();
    };
    let q = [
        point[0].abs() - half[0],
        point[1].abs() - half[1],
        point[2].abs() - half[2],
    ];
    let outside = vec3_length([q[0].max(0.0), q[1].max(0.0), q[2].max(0.0)]);
    let inside = q[0].max(q[1].max(q[2])).min(0.0);
    Value::from_float((outside + inside) as f64)
}

pub fn capsule(point: Value, a: Value, b: Value, radius: Value) -> Value {
    let Some(point) = vec3_components(point) else {
        return Value::nil();
    };
    let Some(a) = vec3_components(a) else {
        return Value::nil();
    };
    let Some(b) = vec3_components(b) else {
        return Value::nil();
    };
    let Some(radius) = component_f32(radius) else {
        return Value::nil();
    };
    let pa = vec3_sub(point, a);
    let ba = vec3_sub(b, a);
    let ba_dot = vec3_dot(ba, ba);
    let h = if ba_dot == 0.0 {
        0.0
    } else {
        (vec3_dot(pa, ba) / ba_dot).clamp(0.0, 1.0)
    };
    let closest = vec3_scale(ba, h);
    Value::from_float((vec3_length(vec3_sub(pa, closest)) - radius) as f64)
}

pub fn cylinder(point: Value, radius: Value, half_height: Value) -> Value {
    let Some(point) = vec3_components(point) else {
        return Value::nil();
    };
    let Some(radius) = component_f32(radius) else {
        return Value::nil();
    };
    let Some(half_height) = component_f32(half_height) else {
        return Value::nil();
    };
    let radial = (point[0] * point[0] + point[2] * point[2]).sqrt() - radius;
    let vertical = point[1].abs() - half_height;
    let outside = (radial.max(0.0).powi(2) + vertical.max(0.0).powi(2)).sqrt();
    let inside = radial.max(vertical).min(0.0);
    Value::from_float((outside + inside) as f64)
}

pub fn plane(point: Value, normal: Value, offset: Value) -> Value {
    let Some(point) = vec3_components(point) else {
        return Value::nil();
    };
    let Some(normal) = vec3_components(normal) else {
        return Value::nil();
    };
    let Some(offset) = component_f32(offset) else {
        return Value::nil();
    };
    let normal_len = vec3_length(normal);
    if normal_len == 0.0 {
        return Value::nil();
    }
    let unit = vec3_scale(normal, 1.0 / normal_len);
    Value::from_float((vec3_dot(point, unit) + offset) as f64)
}

pub fn torus(point: Value, major_radius: Value, minor_radius: Value) -> Value {
    let Some(point) = vec3_components(point) else {
        return Value::nil();
    };
    let Some(major_radius) = component_f32(major_radius) else {
        return Value::nil();
    };
    let Some(minor_radius) = component_f32(minor_radius) else {
        return Value::nil();
    };
    let radial = (point[0] * point[0] + point[2] * point[2]).sqrt() - major_radius;
    let ring = (radial * radial + point[1] * point[1]).sqrt();
    Value::from_float((ring - minor_radius) as f64)
}

fn bounds_center(bounds: Value, expected_type: TypeId, expected_fields: &[&str; 2]) -> Value {
    let Some([min, max]) = named_record_values(bounds, expected_fields) else {
        return Value::nil();
    };
    if type_id_raw(min) != expected_type as u32 || type_id_raw(max) != expected_type as u32 {
        return Value::nil();
    }
    math::vec_mix(min, max, Value::from_float(0.5))
}

fn bounds_size(bounds: Value, expected_type: TypeId, expected_fields: &[&str; 2]) -> Value {
    let Some([min, max]) = named_record_values(bounds, expected_fields) else {
        return Value::nil();
    };
    if type_id_raw(min) != expected_type as u32 || type_id_raw(max) != expected_type as u32 {
        return Value::nil();
    }
    math::vec_sub(max, min)
}

fn transform3_fields(transform: Value) -> Option<(Value, Value)> {
    let values = named_record_values(transform, &TRANSFORM3_FIELDS)?;
    Some((values[0], values[1]))
}

fn mat3_components(val: Value) -> Option<[f32; 9]> {
    if type_id_raw(val) != TypeId::Mat3 as u32 {
        return None;
    }
    let mut out = [0.0f32; 9];
    for idx in 0..9 {
        out[idx] = component_f32(math::mat3_component(val, idx))?;
    }
    Some(out)
}

fn vec2_components(val: Value) -> Option<[f32; 2]> {
    if type_id_raw(val) != TypeId::Vec2 as u32 {
        return None;
    }
    Some([
        component_f32(math::vec_x(val))?,
        component_f32(math::vec_y(val))?,
    ])
}

fn vec4_components(val: Value) -> Option<[f32; 4]> {
    if type_id_raw(val) != TypeId::Vec4 as u32 && type_id_raw(val) != TypeId::Quat as u32 {
        return None;
    }
    Some([
        component_f32(math::vec_x(val))?,
        component_f32(math::vec_y(val))?,
        component_f32(math::vec_z(val))?,
        component_f32(math::vec_w(val))?,
    ])
}

fn splat_period(period: Value) -> Value {
    if let Some(period) = vec3_components(period) {
        return vec3_value(period[0], period[1], period[2]);
    }
    if let Some(period) = component_f32(period) {
        return vec3_value(period, period, period);
    }
    Value::nil()
}

fn scalar_or_first_component(value: Value) -> Option<f32> {
    component_f32(value).or_else(|| {
        vec2_components(value)
            .map(|components| components[0])
            .or_else(|| vec3_components(value).map(|components| components[0]))
            .or_else(|| vec4_components(value).map(|components| components[0]))
    })
}

fn rotate_vec3_y(point: [f32; 3], angle: f32) -> [f32; 3] {
    let cos = angle.cos();
    let sin = angle.sin();
    [
        point[0] * cos - point[2] * sin,
        point[1],
        point[0] * sin + point[2] * cos,
    ]
}

fn rotate_vec3_by_inverse_euler(point: [f32; 3], euler: [f32; 3]) -> [f32; 3] {
    let mut out = point;
    out = rotate_vec3_z(out, -euler[2]);
    out = rotate_vec3_y(out, -euler[1]);
    rotate_vec3_x(out, -euler[0])
}

fn rotate_vec3_x(point: [f32; 3], angle: f32) -> [f32; 3] {
    let cos = angle.cos();
    let sin = angle.sin();
    [
        point[0],
        point[1] * cos - point[2] * sin,
        point[1] * sin + point[2] * cos,
    ]
}

fn rotate_vec3_z(point: [f32; 3], angle: f32) -> [f32; 3] {
    let cos = angle.cos();
    let sin = angle.sin();
    [
        point[0] * cos - point[1] * sin,
        point[0] * sin + point[1] * cos,
        point[2],
    ]
}

fn rotate_vec3_by_quat(point: [f32; 3], quat: [f32; 4]) -> [f32; 3] {
    let qx = quat[0];
    let qy = quat[1];
    let qz = quat[2];
    let qw = quat[3];
    let uxv = [
        qy * point[2] - qz * point[1],
        qz * point[0] - qx * point[2],
        qx * point[1] - qy * point[0],
    ];
    let uuv = [
        qy * uxv[2] - qz * uxv[1],
        qz * uxv[0] - qx * uxv[2],
        qx * uxv[1] - qy * uxv[0],
    ];
    [
        point[0] + 2.0 * (qw * uxv[0] + uuv[0]),
        point[1] + 2.0 * (qw * uxv[1] + uuv[1]),
        point[2] + 2.0 * (qw * uxv[2] + uuv[2]),
    ]
}

fn quat_inverse(quat: [f32; 4]) -> [f32; 4] {
    let len_sq = quat[0] * quat[0] + quat[1] * quat[1] + quat[2] * quat[2] + quat[3] * quat[3];
    if len_sq == 0.0 {
        return [0.0, 0.0, 0.0, 1.0];
    }
    [
        -quat[0] / len_sq,
        -quat[1] / len_sq,
        -quat[2] / len_sq,
        quat[3] / len_sq,
    ]
}

enum SmoothBooleanOp {
    Union,
    Intersection,
    Subtract,
}

fn smooth_boolean(left: Value, right: Value, k: Value, op: SmoothBooleanOp) -> Value {
    let Some(left) = component_f32(left) else {
        return Value::nil();
    };
    let Some(right) = component_f32(right) else {
        return Value::nil();
    };
    let Some(k) = component_f32(k) else {
        return Value::nil();
    };
    if k <= 0.0 {
        return match op {
            SmoothBooleanOp::Union => Value::from_float(left.min(right) as f64),
            SmoothBooleanOp::Intersection => Value::from_float(left.max(right) as f64),
            SmoothBooleanOp::Subtract => Value::from_float(left.max(-right) as f64),
        };
    }
    let h = (0.5 + 0.5 * (right - left) / k).clamp(0.0, 1.0);
    let union = right + (left - right) * h - k * h * (1.0 - h);
    match op {
        SmoothBooleanOp::Union => Value::from_float(union as f64),
        SmoothBooleanOp::Intersection => {
            let inv = smooth_boolean(
                Value::from_float((-left) as f64),
                Value::from_float((-right) as f64),
                Value::from_float(k as f64),
                SmoothBooleanOp::Union,
            );
            let Some(inv) = component_f32(inv) else {
                return Value::nil();
            };
            Value::from_float((-inv) as f64)
        }
        SmoothBooleanOp::Subtract => smooth_boolean(
            Value::from_float(left as f64),
            Value::from_float((-right) as f64),
            Value::from_float(k as f64),
            SmoothBooleanOp::Union,
        ),
    }
}

fn named_record_values<const N: usize>(obj: Value, expected: &[&str; N]) -> Option<[Value; N]> {
    let obj = crate::kernel::actor::actor_backing_instance(obj).unwrap_or(obj);
    let (_, fields) = class::class_type_and_fields(obj)?;
    if fields.len() != expected.len() {
        return None;
    }
    let mut values = [Value::nil(); N];
    for (idx, name) in expected.iter().enumerate() {
        let value = fields
            .iter()
            .find(|(field_name, _)| field_name.as_slice() == name.as_bytes())
            .map(|(_, value)| *value)?;
        values[idx] = value;
    }
    Some(values)
}

fn build_transform3(class_id: u32, matrix: Value, inverse: Value) -> Value {
    if matrix.is_nil() || inverse.is_nil() {
        return Value::nil();
    }
    build_class_value(class_id, &TRANSFORM3_FIELDS, [matrix, inverse])
}

fn build_class_value<const N: usize>(
    class_id: u32,
    field_names: &[&str; N],
    field_values: [Value; N],
) -> Value {
    let names: Vec<*const u8> = field_names.iter().map(|name| name.as_ptr()).collect();
    let lens: Vec<usize> = field_names.iter().map(|name| name.len()).collect();
    let obj = class::class_new(class_id, names.as_ptr(), lens.as_ptr(), N);
    if obj.is_nil() {
        return Value::nil();
    }
    for (idx, value) in field_values.into_iter().enumerate() {
        class::class_set_slot(obj, std::ptr::null(), 0, idx as u32, value);
    }
    obj
}

fn vec3_components(val: Value) -> Option<[f32; 3]> {
    if type_id_raw(val) != TypeId::Vec3 as u32 {
        return None;
    }
    Some([
        component_f32(math::vec_x(val))?,
        component_f32(math::vec_y(val))?,
        component_f32(math::vec_z(val))?,
    ])
}

fn mat4_components(val: Value) -> Option<[f32; 16]> {
    if type_id_raw(val) != TypeId::Mat4 as u32 {
        return None;
    }
    let mut out = [0.0f32; 16];
    for idx in 0..16 {
        out[idx] = component_f32(math::mat4_component(val, idx))?;
    }
    Some(out)
}

fn vec3_value(x: f32, y: f32, z: f32) -> Value {
    math::vec3_new(
        Value::from_float(x as f64),
        Value::from_float(y as f64),
        Value::from_float(z as f64),
    )
}

fn vec4_value(x: f32, y: f32, z: f32, w: f32) -> Value {
    math::vec4_new(
        Value::from_float(x as f64),
        Value::from_float(y as f64),
        Value::from_float(z as f64),
        Value::from_float(w as f64),
    )
}

fn vec2_add(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [a[0] + b[0], a[1] + b[1]]
}

fn vec2_sub(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

fn vec2_scale(v: [f32; 2], s: f32) -> [f32; 2] {
    [v[0] * s, v[1] * s]
}

fn vec2_dot(a: [f32; 2], b: [f32; 2]) -> f32 {
    a[0] * b[0] + a[1] * b[1]
}

fn vec3_sub(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    [left[0] - right[0], left[1] - right[1], left[2] - right[2]]
}

fn vec3_scale(value: [f32; 3], scale: f32) -> [f32; 3] {
    [value[0] * scale, value[1] * scale, value[2] * scale]
}

fn vec3_dot(left: [f32; 3], right: [f32; 3]) -> f32 {
    left[0] * right[0] + left[1] * right[1] + left[2] * right[2]
}

fn vec3_length(value: [f32; 3]) -> f32 {
    vec3_dot(value, value).sqrt()
}

fn vec3_normalize(value: [f32; 3]) -> [f32; 3] {
    let len = vec3_length(value);
    if len == 0.0 {
        return [0.0, 0.0, 0.0];
    }
    vec3_scale(value, len.recip())
}

fn vec3_cross(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    [
        left[1] * right[2] - left[2] * right[1],
        left[2] * right[0] - left[0] * right[2],
        left[0] * right[1] - left[1] * right[0],
    ]
}

const IDENTITY_HASH_OFFSET: u32 = 0x811c9dc5;
const IDENTITY_HASH_PRIME: u32 = 0x0100_0193;

fn hash_identity_i64(hash: u32, value: i64) -> u32 {
    let zigzag = ((value << 1) ^ (value >> 63)) as u64;
    let lo = zigzag as u32;
    let hi = (zigzag >> 32) as u32;
    let mixed_lo = (hash ^ lo).wrapping_mul(IDENTITY_HASH_PRIME);
    (mixed_lo ^ hi).wrapping_mul(IDENTITY_HASH_PRIME)
}

fn hash_identity_f32(hash: u32, value: f32) -> u32 {
    (hash ^ value.to_bits()).wrapping_mul(IDENTITY_HASH_PRIME)
}

fn finalize_identity_hash(hash: u32) -> Value {
    Value::from_int(hash.max(1) as i64)
}

fn repeat_axis(coord: f32, period: f32) -> f32 {
    if period <= 0.0 {
        return coord;
    }
    coord - period * (coord / period + 0.5).floor()
}

fn repeat_identity(period: Value, point: Value) -> Value {
    let Some(point) = vec3_components(point) else {
        return Value::nil();
    };
    let Some(period) = vec3_components(period) else {
        return Value::nil();
    };
    let mut hash = IDENTITY_HASH_OFFSET;
    let mut has_repeat = false;
    for (coord, step) in point.into_iter().zip(period) {
        if step > 0.0 {
            has_repeat = true;
            let cell = (coord / step + 0.5).floor() as i64;
            hash = hash_identity_i64(hash, cell);
        }
    }
    if !has_repeat {
        return Value::from_int(0);
    }
    finalize_identity_hash(hash)
}

fn mat3_mul_vec3(matrix: [f32; 9], vector: [f32; 3]) -> [f32; 3] {
    [
        matrix[0] * vector[0] + matrix[3] * vector[1] + matrix[6] * vector[2],
        matrix[1] * vector[0] + matrix[4] * vector[1] + matrix[7] * vector[2],
        matrix[2] * vector[0] + matrix[5] * vector[1] + matrix[8] * vector[2],
    ]
}

fn mat3_transpose(matrix: [f32; 9]) -> [f32; 9] {
    [
        matrix[0], matrix[3], matrix[6], matrix[1], matrix[4], matrix[7], matrix[2], matrix[5],
        matrix[8],
    ]
}

fn mat4_mul_vec4(matrix: [f32; 16], vector: [f32; 4]) -> [f32; 4] {
    [
        matrix[0] * vector[0]
            + matrix[4] * vector[1]
            + matrix[8] * vector[2]
            + matrix[12] * vector[3],
        matrix[1] * vector[0]
            + matrix[5] * vector[1]
            + matrix[9] * vector[2]
            + matrix[13] * vector[3],
        matrix[2] * vector[0]
            + matrix[6] * vector[1]
            + matrix[10] * vector[2]
            + matrix[14] * vector[3],
        matrix[3] * vector[0]
            + matrix[7] * vector[1]
            + matrix[11] * vector[2]
            + matrix[15] * vector[3],
    ]
}

fn mat4_transpose(matrix: [f32; 16]) -> [f32; 16] {
    [
        matrix[0], matrix[4], matrix[8], matrix[12], matrix[1], matrix[5], matrix[9], matrix[13],
        matrix[2], matrix[6], matrix[10], matrix[14], matrix[3], matrix[7], matrix[11], matrix[15],
    ]
}

fn component_f32(val: Value) -> Option<f32> {
    if let Some(int) = crate::value::int_value(val) {
        return Some(int as f32);
    }
    if val.is_float() {
        return Some(val.as_float() as f32);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vec2_value(x: f32, y: f32) -> Value {
        math::vec2_new(Value::from_float(x as f64), Value::from_float(y as f64))
    }

    fn bounds2_value(min: Value, max: Value) -> Value {
        build_class_value(1001, &BOUNDS2_FIELDS, [min, max])
    }

    fn bounds3_value(min: Value, max: Value) -> Value {
        build_class_value(1002, &BOUNDS3_FIELDS, [min, max])
    }

    #[test]
    fn bounds_helpers_work() {
        let bounds2 = bounds2_value(
            math::vec2_new(Value::from_int(1), Value::from_int(3)),
            math::vec2_new(Value::from_int(5), Value::from_int(7)),
        );
        let center2 = bounds2_center(bounds2);
        let size2 = bounds2_size(bounds2);
        assert_eq!(type_id_raw(center2), TypeId::Vec2 as u32);
        assert_eq!(type_id_raw(size2), TypeId::Vec2 as u32);
        assert_eq!(math::vec_x(center2).as_float(), 3.0);
        assert_eq!(math::vec_y(center2).as_float(), 5.0);
        assert_eq!(math::vec_x(size2).as_float(), 4.0);
        assert_eq!(math::vec_y(size2).as_float(), 4.0);

        let bounds3 = bounds3_value(
            math::vec3_new(Value::from_int(-1), Value::from_int(2), Value::from_int(3)),
            math::vec3_new(Value::from_int(3), Value::from_int(6), Value::from_int(9)),
        );
        let center3 = bounds3_center(bounds3);
        let size3 = bounds3_size(bounds3);
        assert_eq!(type_id_raw(center3), TypeId::Vec3 as u32);
        assert_eq!(type_id_raw(size3), TypeId::Vec3 as u32);
        assert_eq!(math::vec_x(center3).as_float(), 1.0);
        assert_eq!(math::vec_y(center3).as_float(), 4.0);
        assert_eq!(math::vec_z(center3).as_float(), 6.0);
        assert_eq!(math::vec_x(size3).as_float(), 4.0);
        assert_eq!(math::vec_y(size3).as_float(), 4.0);
        assert_eq!(math::vec_z(size3).as_float(), 6.0);
    }

    #[test]
    fn transform_helpers_work() {
        let identity = transform3_identity(3001);
        let (_, identity_fields) = class::class_type_and_fields(identity).expect("identity fields");
        assert_eq!(identity_fields.len(), 2);
        let matrix = identity_fields
            .iter()
            .find(|(name, _)| name.as_slice() == b"matrix")
            .map(|(_, value)| *value)
            .expect("matrix field");
        let inverse = identity_fields
            .iter()
            .find(|(name, _)| name.as_slice() == b"inverse")
            .map(|(_, value)| *value)
            .expect("inverse field");
        assert_eq!(type_id_raw(matrix), TypeId::Mat4 as u32);
        assert_eq!(type_id_raw(inverse), TypeId::Mat4 as u32);

        let point = math::vec3_new(Value::from_int(2), Value::from_int(3), Value::from_int(4));
        let point_out = transform_point(identity, point);
        assert_eq!(math::vec_x(point_out).as_float(), 2.0);
        assert_eq!(math::vec_y(point_out).as_float(), 3.0);
        assert_eq!(math::vec_z(point_out).as_float(), 4.0);

        let vector_out = transform_vector(identity, point);
        assert_eq!(math::vec_x(vector_out).as_float(), 2.0);
        assert_eq!(math::vec_y(vector_out).as_float(), 3.0);
        assert_eq!(math::vec_z(vector_out).as_float(), 4.0);

        let normal = math::vec3_new(Value::from_int(0), Value::from_int(3), Value::from_int(4));
        let normal_out = transform_normal(identity, normal);
        assert_eq!(math::vec_x(normal_out).as_float(), 0.0);
        assert!((math::vec_y(normal_out).as_float() - 0.6).abs() < 1e-6);
        assert!((math::vec_z(normal_out).as_float() - 0.8).abs() < 1e-6);

        let composed = compose_transform3(3001, identity, identity);
        let composed_fields = class::class_type_and_fields(composed)
            .map(|(_, fields)| fields)
            .expect("composed fields");
        assert_eq!(composed_fields.len(), 2);

        let inverted = inverse_transform3(3001, identity);
        let inverted_fields = class::class_type_and_fields(inverted)
            .map(|(_, fields)| fields)
            .expect("inverted fields");
        assert_eq!(inverted_fields.len(), 2);
    }

    #[test]
    fn transform_helpers_reject_bad_layouts() {
        let bad = build_class_value(
            3002,
            &TRANSFORM3_FIELDS,
            [math::mat3_identity(), math::mat3_identity()],
        );
        assert!(
            transform_point(
                bad,
                math::vec3_new(Value::from_int(1), Value::from_int(2), Value::from_int(3))
            )
            .is_nil()
        );
        assert!(transform3_identity(3002).is_ptr());
    }

    #[test]
    fn transform_normal_zero_vector_matches_reference_semantics() {
        let identity = transform3_identity(3003);
        let zero = math::vec3_new(Value::from_int(0), Value::from_int(0), Value::from_int(0));
        let out = transform_normal(identity, zero);
        assert_eq!(math::vec_x(out).as_float(), 0.0);
        assert_eq!(math::vec_y(out).as_float(), 0.0);
        assert_eq!(math::vec_z(out).as_float(), 0.0);
    }

    #[test]
    fn field_composition_helpers_work() {
        let union = field_union(Value::from_float(0.75), Value::from_float(-0.25));
        let intersection = field_intersection(Value::from_float(0.75), Value::from_float(-0.25));
        let subtract = field_subtract(Value::from_float(0.25), Value::from_float(-0.5));

        assert_eq!(union.as_float(), -0.25);
        assert_eq!(intersection.as_float(), 0.75);
        assert_eq!(subtract.as_float(), 0.5);
    }

    #[test]
    fn field_primitives_work() {
        let sphere_dist = sphere(vec3_value(0.0, 0.0, 2.0), Value::from_float(1.0));
        assert!((sphere_dist.as_float() - 1.0).abs() < 1.0e-6);

        let box_dist = box_sdf(vec3_value(2.0, 0.0, 0.0), vec3_value(1.0, 1.0, 1.0));
        assert!((box_dist.as_float() - 1.0).abs() < 1.0e-6);

        let capsule_dist = capsule(
            vec3_value(0.0, 0.0, 1.5),
            vec3_value(0.0, -1.0, 0.0),
            vec3_value(0.0, 1.0, 0.0),
            Value::from_float(0.5),
        );
        assert!((capsule_dist.as_float() - 1.0).abs() < 1.0e-6);

        let cylinder_dist = cylinder(
            vec3_value(2.0, 0.0, 0.0),
            Value::from_float(1.0),
            Value::from_float(1.0),
        );
        assert!((cylinder_dist.as_float() - 1.0).abs() < 1.0e-6);

        let plane_dist = plane(
            vec3_value(0.0, 2.0, 0.0),
            vec3_value(0.0, 1.0, 0.0),
            Value::from_float(-1.0),
        );
        assert!((plane_dist.as_float() - 1.0).abs() < 1.0e-6);

        let torus_dist = torus(
            vec3_value(2.5, 0.0, 0.0),
            Value::from_float(2.0),
            Value::from_float(0.5),
        );
        assert!(torus_dist.as_float().abs() < 1.0e-6);
    }

    #[test]
    fn field_point_helpers_work() {
        let point = vec3_value(3.0, -2.0, 1.0);
        let translated = field_transform_point(vec3_value(1.0, 2.0, 3.0), point);
        assert_eq!(math::vec_x(translated).as_float(), 2.0);
        assert_eq!(math::vec_y(translated).as_float(), -4.0);
        assert_eq!(math::vec_z(translated).as_float(), -2.0);

        let rotated = field_rotate_point(
            Value::from_float(std::f32::consts::FRAC_PI_2 as f64),
            vec3_value(1.0, 0.0, 0.0),
        );
        assert!(math::vec_x(rotated).as_float().abs() < 0.0001);
        assert!((math::vec_z(rotated).as_float() + 1.0).abs() < 0.0001);

        let mirrored = field_mirror_point(vec3_value(1.0, 0.0, 0.0), vec3_value(-2.0, 1.0, 0.0));
        assert_eq!(math::vec_x(mirrored).as_float(), 2.0);
        assert_eq!(math::vec_y(mirrored).as_float(), 1.0);

        let repeated = field_repeat_point(vec3_value(2.0, 0.0, 0.0), vec3_value(3.25, 0.5, 0.0));
        assert!((math::vec_x(repeated).as_float() + 0.75).abs() < 0.0001);
        assert_eq!(math::vec_y(repeated).as_float(), 0.5);
    }

    #[test]
    fn phase5_helpers_execute() {
        let point = vec3_value(3.0, -2.0, 1.0);
        let translated = translate(vec3_value(1.0, 2.0, 3.0), point);
        assert_eq!(math::vec_x(translated).as_float(), 2.0);
        assert_eq!(math::vec_y(translated).as_float(), -4.0);
        assert_eq!(math::vec_z(translated).as_float(), -2.0);

        let scaled = uniform_scale(Value::from_float(2.0), vec3_value(4.0, -2.0, 6.0));
        assert_eq!(math::vec_x(scaled).as_float(), 2.0);
        assert_eq!(math::vec_y(scaled).as_float(), -1.0);
        assert_eq!(math::vec_z(scaled).as_float(), 3.0);

        let rotated = rotate(
            Value::from_float(std::f32::consts::FRAC_PI_2 as f64),
            vec3_value(1.0, 0.0, 0.0),
        );
        assert!(math::vec_x(rotated).as_float().abs() < 0.0001);
        assert!((math::vec_z(rotated).as_float() + 1.0).abs() < 0.0001);

        let smooth = smooth_union(
            Value::from_float(1.0),
            Value::from_float(0.0),
            Value::from_float(0.5),
        );
        assert!(smooth.as_float() <= 1.0);

        let bent = bend(Value::from_float(0.25), vec3_value(1.0, 2.0, 3.0));
        assert!(math::vec_y(bent).as_float() != 2.0 || math::vec_z(bent).as_float() != 3.0);

        assert!(
            !rounded_box(
                vec3_value(2.0, 0.0, 0.0),
                vec3_value(1.0, 1.0, 1.0),
                Value::from_float(0.25)
            )
            .is_nil()
        );
        assert!(
            (circle2(vec2_value(2.0, 0.0), Value::from_float(1.0)).as_float() - 1.0).abs() < 1.0e-6
        );
        assert!(
            (rect2(vec2_value(2.0, 0.0), vec2_value(1.0, 1.0)).as_float() - 1.0).abs() < 1.0e-6
        );
        assert!(
            (rounded_rect2(
                vec2_value(2.0, 0.0),
                vec2_value(1.0, 1.0),
                Value::from_float(0.25)
            )
            .as_float()
                - 0.75)
                .abs()
                < 1.0e-6
        );
        assert!(
            (capsule2(
                vec2_value(2.0, 0.0),
                vec2_value(-1.0, 0.0),
                vec2_value(1.0, 0.0),
                Value::from_float(0.25)
            )
            .as_float()
                - 0.75)
                .abs()
                < 1.0e-6
        );
        assert!(
            (segment2(
                vec2_value(2.0, 0.0),
                vec2_value(-1.0, 0.0),
                vec2_value(1.0, 0.0)
            )
            .as_float()
                - 1.0)
                .abs()
                < 1.0e-6
        );
        let poly = list::list_new_local(4);
        list::list_set(poly, 0, vec2_value(-1.0, -1.0));
        list::list_set(poly, 1, vec2_value(1.0, -1.0));
        list::list_set(poly, 2, vec2_value(1.0, 1.0));
        list::list_set(poly, 3, vec2_value(-1.0, 1.0));
        assert!(polygon2(vec2_value(2.0, 0.0), poly).as_float() > 0.0);
        assert!(polyline2(vec2_value(2.0, 0.0), poly).as_float() > 0.0);
        assert!(!ellipsoid(vec3_value(1.0, 0.0, 0.0), vec3_value(2.0, 1.0, 1.0)).is_nil());
        assert!(
            !triangle_prism(
                vec3_value(0.1, 0.1, 0.1),
                vec2_value(1.0, 1.0),
                Value::from_float(0.5)
            )
            .is_nil()
        );
        assert!(
            !hex_prism(
                vec3_value(0.1, 0.1, 0.1),
                vec2_value(1.0, 1.0),
                Value::from_float(0.5)
            )
            .is_nil()
        );
    }
}
