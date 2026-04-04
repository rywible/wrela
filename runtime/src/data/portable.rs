use super::class;
use super::math;
use super::value::{TypeId, Value, type_id_raw};

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

pub fn field_union(left: Value, right: Value) -> Value {
    let Some(left) = component_f32(left) else {
        return Value::nil();
    };
    let Some(right) = component_f32(right) else {
        return Value::nil();
    };
    Value::from_float(left.min(right) as f64)
}

pub fn field_intersection(left: Value, right: Value) -> Value {
    let Some(left) = component_f32(left) else {
        return Value::nil();
    };
    let Some(right) = component_f32(right) else {
        return Value::nil();
    };
    Value::from_float(left.max(right) as f64)
}

pub fn field_subtract(left: Value, right: Value) -> Value {
    let Some(left) = component_f32(left) else {
        return Value::nil();
    };
    let Some(right) = component_f32(right) else {
        return Value::nil();
    };
    Value::from_float(left.max(-right) as f64)
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

fn repeat_axis(coord: f32, period: f32) -> f32 {
    if period <= 0.0 {
        return coord;
    }
    coord - period * (coord / period + 0.5).floor()
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

        let mirrored = field_mirror_point(vec3_value(1.0, 0.0, 0.0), vec3_value(-2.0, 1.0, 0.0));
        assert_eq!(math::vec_x(mirrored).as_float(), 2.0);
        assert_eq!(math::vec_y(mirrored).as_float(), 1.0);

        let repeated = field_repeat_point(vec3_value(2.0, 0.0, 0.0), vec3_value(3.25, 0.5, 0.0));
        assert!((math::vec_x(repeated).as_float() + 0.75).abs() < 0.0001);
        assert_eq!(math::vec_y(repeated).as_float(), 0.5);
    }
}
