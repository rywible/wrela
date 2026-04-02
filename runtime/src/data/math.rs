use super::object::ObjHeader;
use super::value::{header, TypeId, Value};

#[repr(C)]
pub(crate) struct VecObj {
    pub(crate) header: ObjHeader,
    pub(crate) len: usize,
    pub(crate) data: [f32; 4],
}

#[repr(C)]
pub(crate) struct Mat3Obj {
    pub(crate) header: ObjHeader,
    /// Column-major storage: `data[column * 3 + row]`.
    pub(crate) data: [f32; 9],
}

#[repr(C)]
pub(crate) struct Mat4Obj {
    pub(crate) header: ObjHeader,
    /// Column-major storage: `data[column * 4 + row]`.
    pub(crate) data: [f32; 16],
}

pub(crate) fn as_vec_ref(val: Value) -> Option<*mut VecObj> {
    if !val.is_ptr() {
        return None;
    }
    unsafe {
        let header = &*val.as_ptr();
        if !matches!(
            header.type_id,
            x if x == TypeId::Vec2 as u32
                || x == TypeId::Vec3 as u32
                || x == TypeId::Vec4 as u32
                || x == TypeId::Quat as u32
        ) {
            return None;
        }
    }
    Some(val.as_ptr() as *mut VecObj)
}

pub(crate) fn as_mat3_ref(val: Value) -> Option<*mut Mat3Obj> {
    if !val.is_ptr() {
        return None;
    }
    unsafe {
        if (*val.as_ptr()).type_id != TypeId::Mat3 as u32 {
            return None;
        }
    }
    Some(val.as_ptr() as *mut Mat3Obj)
}

pub(crate) fn as_mat4_ref(val: Value) -> Option<*mut Mat4Obj> {
    if !val.is_ptr() {
        return None;
    }
    unsafe {
        if (*val.as_ptr()).type_id != TypeId::Mat4 as u32 {
            return None;
        }
    }
    Some(val.as_ptr() as *mut Mat4Obj)
}

pub fn vec2_new(x: Value, y: Value) -> Value {
    construct_vec_from_values(TypeId::Vec2, [x, y, Value::nil(), Value::nil()], 2)
}

pub fn vec3_new(x: Value, y: Value, z: Value) -> Value {
    construct_vec_from_values(TypeId::Vec3, [x, y, z, Value::nil()], 3)
}

pub fn vec4_new(x: Value, y: Value, z: Value, w: Value) -> Value {
    construct_vec_from_values(TypeId::Vec4, [x, y, z, w], 4)
}

pub fn quat_new(x: Value, y: Value, z: Value, w: Value) -> Value {
    construct_vec_from_values(TypeId::Quat, [x, y, z, w], 4)
}

pub fn vec_component(val: Value, index: usize) -> Value {
    let Some(vec) = as_vec_ref(val) else {
        return Value::nil();
    };
    unsafe {
        let vec = &*vec;
        if index >= vec.len {
            return Value::nil();
        }
        Value::from_float(vec.data[index] as f64)
    }
}

pub fn mat3_component(val: Value, index: usize) -> Value {
    let Some(mat) = as_mat3_ref(val) else {
        return Value::nil();
    };
    unsafe {
        let mat = &*mat;
        if index >= mat.data.len() {
            return Value::nil();
        }
        Value::from_float(mat.data[index] as f64)
    }
}

pub fn mat4_component(val: Value, index: usize) -> Value {
    let Some(mat) = as_mat4_ref(val) else {
        return Value::nil();
    };
    unsafe {
        let mat = &*mat;
        if index >= mat.data.len() {
            return Value::nil();
        }
        Value::from_float(mat.data[index] as f64)
    }
}

pub fn vec_x(val: Value) -> Value {
    vec_component(val, 0)
}

pub fn vec_y(val: Value) -> Value {
    vec_component(val, 1)
}

pub fn vec_z(val: Value) -> Value {
    vec_component(val, 2)
}

pub fn vec_w(val: Value) -> Value {
    vec_component(val, 3)
}

pub fn quat_x(val: Value) -> Value {
    vec_x(val)
}

pub fn quat_y(val: Value) -> Value {
    vec_y(val)
}

pub fn quat_z(val: Value) -> Value {
    vec_z(val)
}

pub fn quat_w(val: Value) -> Value {
    vec_w(val)
}

pub fn vec_add(a: Value, b: Value) -> Value {
    binary_vec_op(a, b, |left, right| left + right, None::<fn(f32, f32) -> f32>)
}

pub fn vec_sub(a: Value, b: Value) -> Value {
    binary_vec_op(a, b, |left, right| left - right, None::<fn(f32, f32) -> f32>)
}

pub fn vec_mul(a: Value, b: Value) -> Value {
    binary_vec_op(a, b, |left, right| left * right, Some(|left, scalar| left * scalar))
}

pub fn vec_div(a: Value, b: Value) -> Value {
    binary_vec_op(a, b, |left, right| left / right, Some(|left, scalar| left / scalar))
}

pub fn vec_min(a: Value, b: Value) -> Value {
    binary_vec_op(
        a,
        b,
        |left, right| left.min(right),
        Some(|left: f32, scalar: f32| left.min(scalar)),
    )
}

pub fn vec_max(a: Value, b: Value) -> Value {
    binary_vec_op(
        a,
        b,
        |left, right| left.max(right),
        Some(|left: f32, scalar: f32| left.max(scalar)),
    )
}

pub fn vec_clamp(value: Value, min: Value, max: Value) -> Value {
    ternary_vec_op(
        value,
        min,
        max,
        |v, lo, hi| clamp_f32(v, lo, hi),
        |v, lo, hi| clamp_f32(v, lo, hi),
    )
}

pub fn vec_mix(a: Value, b: Value, t: Value) -> Value {
    ternary_vec_op(
        a,
        b,
        t,
        |left, right, mix| left + (right - left) * mix,
        |left, right, mix| left + (right - left) * mix,
    )
}

pub fn vec_abs(val: Value) -> Value {
    unary_vec_op(val, f32::abs, f32::abs)
}

pub fn vec_sign(val: Value) -> Value {
    unary_vec_op(val, sign_f32, sign_f32)
}

pub fn vec_floor(val: Value) -> Value {
    unary_vec_op(val, f32::floor, f32::floor)
}

pub fn vec_ceil(val: Value) -> Value {
    unary_vec_op(val, f32::ceil, f32::ceil)
}

pub fn vec_fract(val: Value) -> Value {
    unary_vec_op(val, f32::fract, f32::fract)
}

pub fn vec_sin(val: Value) -> Value {
    unary_vec_op(val, f32::sin, f32::sin)
}

pub fn vec_cos(val: Value) -> Value {
    unary_vec_op(val, f32::cos, f32::cos)
}

pub fn vec_sqrt(val: Value) -> Value {
    unary_vec_op(val, f32::sqrt, f32::sqrt)
}

pub fn vec_pow(a: Value, b: Value) -> Value {
    binary_vec_op(
        a,
        b,
        |left, right| left.powf(right),
        Some(|left: f32, scalar: f32| left.powf(scalar)),
    )
}

pub fn vec_dot(a: Value, b: Value) -> Value {
    let Some((al, ar)) = vec_pair(a, b) else {
        return Value::nil();
    };
    unsafe {
        let al = &*al;
        let ar = &*ar;
        if al.len != ar.len || al.type_id() != ar.type_id() {
            return Value::nil();
        }
        let mut acc = 0.0f32;
        for idx in 0..al.len {
            acc += al.data[idx] * ar.data[idx];
        }
        Value::from_float(acc as f64)
    }
}

pub fn vec_length(val: Value) -> Value {
    let Some(vec) = as_vec_ref(val) else {
        return Value::nil();
    };
    unsafe {
        let vec = &*vec;
        let mut acc = 0.0f32;
        for idx in 0..vec.len {
            acc += vec.data[idx] * vec.data[idx];
        }
        Value::from_float(acc.sqrt() as f64)
    }
}

pub fn vec_normalize(val: Value) -> Value {
    let Some(vec) = as_vec_ref(val) else {
        return Value::nil();
    };
    unsafe {
        let vec = &*vec;
        let mut acc = 0.0f32;
        for idx in 0..vec.len {
            acc += vec.data[idx] * vec.data[idx];
        }
        let len = acc.sqrt();
        if !len.is_finite() || len == 0.0 {
            return Value::nil();
        }
        let mut data = [0.0f32; 4];
        for idx in 0..vec.len {
            data[idx] = vec.data[idx] / len;
        }
        construct_vec(vec.type_id(), data, vec.len)
    }
}

pub fn vec_distance(a: Value, b: Value) -> Value {
    let Some((al, ar)) = vec_pair(a, b) else {
        return Value::nil();
    };
    unsafe {
        let al = &*al;
        let ar = &*ar;
        if !is_spatial_vec_type(al.type_id()) || al.type_id() != ar.type_id() {
            return Value::nil();
        }
        let mut acc = 0.0f32;
        for idx in 0..al.len {
            let delta = al.data[idx] - ar.data[idx];
            acc += delta * delta;
        }
        Value::from_float(acc.sqrt() as f64)
    }
}

pub fn vec_reflect(incident: Value, normal: Value) -> Value {
    let Some((incident, normal)) = vec_pair(incident, normal) else {
        return Value::nil();
    };
    unsafe {
        let incident = &*incident;
        let normal = &*normal;
        if !is_spatial_vec_type(incident.type_id())
            || incident.type_id() != normal.type_id()
            || incident.len != normal.len
        {
            return Value::nil();
        }
        let mut dot = 0.0f32;
        for idx in 0..incident.len {
            dot += incident.data[idx] * normal.data[idx];
        }
        let scale = 2.0 * dot;
        let mut out = [0.0f32; 4];
        for idx in 0..incident.len {
            out[idx] = incident.data[idx] - scale * normal.data[idx];
        }
        construct_vec(incident.type_id(), out, incident.len)
    }
}

pub fn vec_cross(a: Value, b: Value) -> Value {
    let Some((a, b)) = vec_pair(a, b) else {
        return Value::nil();
    };
    unsafe {
        let a = &*a;
        let b = &*b;
        if a.type_id() != TypeId::Vec3 || b.type_id() != TypeId::Vec3 {
            return Value::nil();
        }
        construct_vec(
            TypeId::Vec3,
            [
                a.data[1] * b.data[2] - a.data[2] * b.data[1],
                a.data[2] * b.data[0] - a.data[0] * b.data[2],
                a.data[0] * b.data[1] - a.data[1] * b.data[0],
                0.0,
            ],
            3,
        )
    }
}

pub fn mat3_identity() -> Value {
    construct_mat3([
        1.0, 0.0, 0.0, //
        0.0, 1.0, 0.0, //
        0.0, 0.0, 1.0,
    ])
}

pub fn mat3_from_columns(c0: Value, c1: Value, c2: Value) -> Value {
    let Some(c0) = as_vec_ref(c0) else {
        return Value::nil();
    };
    let Some(c1) = as_vec_ref(c1) else {
        return Value::nil();
    };
    let Some(c2) = as_vec_ref(c2) else {
        return Value::nil();
    };
    unsafe {
        let c0 = &*c0;
        let c1 = &*c1;
        let c2 = &*c2;
        if c0.type_id() != TypeId::Vec3 || c1.type_id() != TypeId::Vec3 || c2.type_id() != TypeId::Vec3 {
            return Value::nil();
        }
        construct_mat3([
            c0.data[0], c0.data[1], c0.data[2], //
            c1.data[0], c1.data[1], c1.data[2], //
            c2.data[0], c2.data[1], c2.data[2],
        ])
    }
}

pub fn mat3_mul_vec3(mat: Value, vec: Value) -> Value {
    let Some(mat) = as_mat3_ref(mat) else {
        return Value::nil();
    };
    let Some(vec) = as_vec_ref(vec) else {
        return Value::nil();
    };
    unsafe {
        let mat = &*mat;
        let vec = &*vec;
        if vec.type_id() != TypeId::Vec3 {
            return Value::nil();
        }
        let out = mat3_apply_vec3(mat, [vec.data[0], vec.data[1], vec.data[2]]);
        construct_vec(
            TypeId::Vec3,
            [
                out[0],
                out[1],
                out[2],
                0.0,
            ],
            3,
        )
    }
}

pub fn mat3_mul_mat3(a: Value, b: Value) -> Value {
    let Some(a) = as_mat3_ref(a) else {
        return Value::nil();
    };
    let Some(b) = as_mat3_ref(b) else {
        return Value::nil();
    };
    unsafe {
        let a = &*a;
        let b = &*b;
        let mut out = [0.0f32; 9];
        for col in 0..3 {
            let base = col * 3;
            let product = mat3_apply_vec3(
                a,
                [b.data[base], b.data[base + 1], b.data[base + 2]],
            );
            out[base] = product[0];
            out[base + 1] = product[1];
            out[base + 2] = product[2];
        }
        construct_mat3(out)
    }
}

pub fn mat3_add(a: Value, b: Value) -> Value {
    binary_mat3_op(a, b, |left, right| left + right)
}

pub fn mat3_sub(a: Value, b: Value) -> Value {
    binary_mat3_op(a, b, |left, right| left - right)
}

pub fn mat3_mul_scalar(mat: Value, scalar: Value) -> Value {
    scalar_mat3_op(mat, scalar, |left, scalar| left * scalar)
}

pub fn mat3_div_scalar(mat: Value, scalar: Value) -> Value {
    scalar_mat3_op(mat, scalar, |left, scalar| left / scalar)
}

pub fn mat4_identity() -> Value {
    construct_mat4([
        1.0, 0.0, 0.0, 0.0, //
        0.0, 1.0, 0.0, 0.0, //
        0.0, 0.0, 1.0, 0.0, //
        0.0, 0.0, 0.0, 1.0,
    ])
}

pub fn mat4_from_columns(c0: Value, c1: Value, c2: Value, c3: Value) -> Value {
    let Some(c0) = as_vec_ref(c0) else {
        return Value::nil();
    };
    let Some(c1) = as_vec_ref(c1) else {
        return Value::nil();
    };
    let Some(c2) = as_vec_ref(c2) else {
        return Value::nil();
    };
    let Some(c3) = as_vec_ref(c3) else {
        return Value::nil();
    };
    unsafe {
        let c0 = &*c0;
        let c1 = &*c1;
        let c2 = &*c2;
        let c3 = &*c3;
        if c0.type_id() != TypeId::Vec4
            || c1.type_id() != TypeId::Vec4
            || c2.type_id() != TypeId::Vec4
            || c3.type_id() != TypeId::Vec4
        {
            return Value::nil();
        }
        construct_mat4([
            c0.data[0], c0.data[1], c0.data[2], c0.data[3], //
            c1.data[0], c1.data[1], c1.data[2], c1.data[3], //
            c2.data[0], c2.data[1], c2.data[2], c2.data[3], //
            c3.data[0], c3.data[1], c3.data[2], c3.data[3],
        ])
    }
}

pub fn mat4_mul_vec4(mat: Value, vec: Value) -> Value {
    let Some(mat) = as_mat4_ref(mat) else {
        return Value::nil();
    };
    let Some(vec) = as_vec_ref(vec) else {
        return Value::nil();
    };
    unsafe {
        let mat = &*mat;
        let vec = &*vec;
        if vec.type_id() != TypeId::Vec4 {
            return Value::nil();
        }
        let out = mat4_apply_vec4(mat, [vec.data[0], vec.data[1], vec.data[2], vec.data[3]]);
        construct_vec(
            TypeId::Vec4,
            [out[0], out[1], out[2], out[3]],
            4,
        )
    }
}

pub fn mat4_mul_mat4(a: Value, b: Value) -> Value {
    let Some(a) = as_mat4_ref(a) else {
        return Value::nil();
    };
    let Some(b) = as_mat4_ref(b) else {
        return Value::nil();
    };
    unsafe {
        let a = &*a;
        let b = &*b;
        let mut out = [0.0f32; 16];
        for col in 0..4 {
            let base = col * 4;
            let product = mat4_apply_vec4(
                a,
                [b.data[base], b.data[base + 1], b.data[base + 2], b.data[base + 3]],
            );
            out[base] = product[0];
            out[base + 1] = product[1];
            out[base + 2] = product[2];
            out[base + 3] = product[3];
        }
        construct_mat4(out)
    }
}

pub fn mat4_add(a: Value, b: Value) -> Value {
    binary_mat4_op(a, b, |left, right| left + right)
}

pub fn mat4_sub(a: Value, b: Value) -> Value {
    binary_mat4_op(a, b, |left, right| left - right)
}

pub fn mat4_mul_scalar(mat: Value, scalar: Value) -> Value {
    scalar_mat4_op(mat, scalar, |left, scalar| left * scalar)
}

pub fn mat4_div_scalar(mat: Value, scalar: Value) -> Value {
    scalar_mat4_op(mat, scalar, |left, scalar| left / scalar)
}

pub unsafe fn drop_vec(ptr: *mut ObjHeader) {
    let vec = ptr as *mut VecObj;
    unsafe {
        drop(Box::from_raw(vec));
    }
}

pub unsafe fn drop_mat3(ptr: *mut ObjHeader) {
    let mat = ptr as *mut Mat3Obj;
    unsafe {
        drop(Box::from_raw(mat));
    }
}

pub unsafe fn drop_mat4(ptr: *mut ObjHeader) {
    let mat = ptr as *mut Mat4Obj;
    unsafe {
        drop(Box::from_raw(mat));
    }
}

fn construct_vec_from_values(type_id: TypeId, data: [Value; 4], len: usize) -> Value {
    let mut out = [0.0f32; 4];
    for idx in 0..len {
        let Some(component) = numeric_value(data[idx]) else {
            return Value::nil();
        };
        out[idx] = component;
    }
    construct_vec(type_id, out, len)
}

fn construct_vec(type_id: TypeId, data: [f32; 4], len: usize) -> Value {
    let obj = Box::new(VecObj {
        header: header(type_id),
        len,
        data,
    });
    Value::from_ptr(Box::into_raw(obj) as *mut ObjHeader)
}

fn construct_mat3(data: [f32; 9]) -> Value {
    let obj = Box::new(Mat3Obj {
        header: header(TypeId::Mat3),
        data,
    });
    Value::from_ptr(Box::into_raw(obj) as *mut ObjHeader)
}

fn construct_mat4(data: [f32; 16]) -> Value {
    let obj = Box::new(Mat4Obj {
        header: header(TypeId::Mat4),
        data,
    });
    Value::from_ptr(Box::into_raw(obj) as *mut ObjHeader)
}

fn numeric_value(val: Value) -> Option<f32> {
    if let Some(int) = crate::value::int_value(val) {
        return Some(int as f32);
    }
    if val.is_float() {
        return Some(val.as_float() as f32);
    }
    None
}

fn vec_pair(a: Value, b: Value) -> Option<(*mut VecObj, *mut VecObj)> {
    let a = as_vec_ref(a)?;
    let b = as_vec_ref(b)?;
    Some((a, b))
}

fn mat3_pair(a: Value, b: Value) -> Option<(*mut Mat3Obj, *mut Mat3Obj)> {
    let a = as_mat3_ref(a)?;
    let b = as_mat3_ref(b)?;
    Some((a, b))
}

fn mat4_pair(a: Value, b: Value) -> Option<(*mut Mat4Obj, *mut Mat4Obj)> {
    let a = as_mat4_ref(a)?;
    let b = as_mat4_ref(b)?;
    Some((a, b))
}

fn unary_vec_op(
    val: Value,
    scalar_op: impl Fn(f32) -> f32 + Copy,
    vec_op: impl Fn(f32) -> f32 + Copy,
) -> Value {
    if let Some(vec) = as_vec_ref(val) {
        unsafe {
            let vec = &*vec;
            let mut out = [0.0f32; 4];
            for idx in 0..vec.len {
                out[idx] = vec_op(vec.data[idx]);
            }
            return construct_vec(vec.type_id(), out, vec.len);
        }
    }
    numeric_value(val)
        .map(|scalar| Value::from_float(scalar_op(scalar) as f64))
        .unwrap_or(Value::nil())
}

fn binary_vec_op<F, G>(a: Value, b: Value, vec_op: F, scalar_op: Option<G>) -> Value
where
    F: Fn(f32, f32) -> f32 + Copy,
    G: Fn(f32, f32) -> f32 + Copy,
{
    match (as_vec_ref(a), as_vec_ref(b)) {
        (Some(a), Some(b)) => unsafe {
            let a = &*a;
            let b = &*b;
            if a.type_id() != b.type_id() || a.len != b.len {
                return Value::nil();
            }
            let mut out = [0.0f32; 4];
            for idx in 0..a.len {
                out[idx] = vec_op(a.data[idx], b.data[idx]);
            }
            construct_vec(a.type_id(), out, a.len)
        },
        (Some(a), None) => scalar_vec_op(a, b, scalar_op),
        (None, Some(b)) => scalar_vec_op(b, a, scalar_op),
        _ => {
            let Some(scalar_op) = scalar_op else {
                return Value::nil();
            };
            let Some(left) = numeric_value(a) else {
                return Value::nil();
            };
            let Some(right) = numeric_value(b) else {
                return Value::nil();
            };
            Value::from_float(scalar_op(left, right) as f64)
        }
    }
}

fn ternary_vec_op<F, G>(a: Value, b: Value, c: Value, vec_op: F, scalar_op: G) -> Value
where
    F: Fn(f32, f32, f32) -> f32 + Copy,
    G: Fn(f32, f32, f32) -> f32 + Copy,
{
    let a_vec = as_vec_ref(a);
    let b_vec = as_vec_ref(b);
    let c_vec = as_vec_ref(c);

    if let Some(a_ptr) = a_vec {
        unsafe {
            let a = &*a_ptr;
            if let Some(b_ptr) = b_vec {
                let b = &*b_ptr;
                if a.type_id() != b.type_id() || a.len != b.len {
                    return Value::nil();
                }
            }
            if let Some(c_ptr) = c_vec {
                let c = &*c_ptr;
                if a.type_id() != c.type_id() || a.len != c.len {
                    return Value::nil();
                }
            }
            let mut out = [0.0f32; 4];
            for idx in 0..a.len {
                let b_component = if let Some(b_ptr) = b_vec {
                    (&*b_ptr).data[idx]
                } else {
                    match numeric_value(b) {
                        Some(value) => value,
                        None => return Value::nil(),
                    }
                };
                let c_component = if let Some(c_ptr) = c_vec {
                    (&*c_ptr).data[idx]
                } else {
                    match numeric_value(c) {
                        Some(value) => value,
                        None => return Value::nil(),
                    }
                };
                out[idx] = vec_op(a.data[idx], b_component, c_component);
            }
            return construct_vec(a.type_id(), out, a.len);
        }
    }

    let Some(a_scalar) = numeric_value(a) else {
        return Value::nil();
    };
    let Some(b_scalar) = numeric_value(b) else {
        return Value::nil();
    };
    let Some(c_scalar) = numeric_value(c) else {
        return Value::nil();
    };
    Value::from_float(scalar_op(a_scalar, b_scalar, c_scalar) as f64)
}

fn scalar_vec_op<F>(vec: *mut VecObj, scalar: Value, scalar_op: Option<F>) -> Value
where
    F: Fn(f32, f32) -> f32 + Copy,
{
    let Some(scalar) = numeric_value(scalar) else {
        return Value::nil();
    };
    let Some(scalar_op) = scalar_op else {
        return Value::nil();
    };
    unsafe {
        let vec = &*vec;
        let mut out = [0.0f32; 4];
        for idx in 0..vec.len {
            out[idx] = scalar_op(vec.data[idx], scalar);
        }
        construct_vec(vec.type_id(), out, vec.len)
    }
}

fn mat3_apply_vec3(mat: &Mat3Obj, vec: [f32; 3]) -> [f32; 3] {
    let mut out = [0.0f32; 3];
    for row in 0..3 {
        out[row] = mat.data[row] * vec[0]
            + mat.data[3 + row] * vec[1]
            + mat.data[6 + row] * vec[2];
    }
    out
}

fn mat4_apply_vec4(mat: &Mat4Obj, vec: [f32; 4]) -> [f32; 4] {
    let mut out = [0.0f32; 4];
    for row in 0..4 {
        out[row] = mat.data[row] * vec[0]
            + mat.data[4 + row] * vec[1]
            + mat.data[8 + row] * vec[2]
            + mat.data[12 + row] * vec[3];
    }
    out
}

fn binary_mat3_op(a: Value, b: Value, op: impl Fn(f32, f32) -> f32) -> Value {
    let Some((a, b)) = mat3_pair(a, b) else {
        return Value::nil();
    };
    unsafe {
        let a = &*a;
        let b = &*b;
        let mut out = [0.0f32; 9];
        for (idx, value) in out.iter_mut().enumerate() {
            *value = op(a.data[idx], b.data[idx]);
        }
        construct_mat3(out)
    }
}

fn scalar_mat3_op(mat: Value, scalar: Value, op: impl Fn(f32, f32) -> f32) -> Value {
    let Some(mat) = as_mat3_ref(mat) else {
        return Value::nil();
    };
    let Some(scalar) = numeric_value(scalar) else {
        return Value::nil();
    };
    unsafe {
        let mat = &*mat;
        let mut out = [0.0f32; 9];
        for (idx, value) in out.iter_mut().enumerate() {
            *value = op(mat.data[idx], scalar);
        }
        construct_mat3(out)
    }
}

fn binary_mat4_op(a: Value, b: Value, op: impl Fn(f32, f32) -> f32) -> Value {
    let Some((a, b)) = mat4_pair(a, b) else {
        return Value::nil();
    };
    unsafe {
        let a = &*a;
        let b = &*b;
        let mut out = [0.0f32; 16];
        for (idx, value) in out.iter_mut().enumerate() {
            *value = op(a.data[idx], b.data[idx]);
        }
        construct_mat4(out)
    }
}

fn scalar_mat4_op(mat: Value, scalar: Value, op: impl Fn(f32, f32) -> f32) -> Value {
    let Some(mat) = as_mat4_ref(mat) else {
        return Value::nil();
    };
    let Some(scalar) = numeric_value(scalar) else {
        return Value::nil();
    };
    unsafe {
        let mat = &*mat;
        let mut out = [0.0f32; 16];
        for (idx, value) in out.iter_mut().enumerate() {
            *value = op(mat.data[idx], scalar);
        }
        construct_mat4(out)
    }
}

fn clamp_f32(value: f32, min: f32, max: f32) -> f32 {
    if min <= max {
        value.max(min).min(max)
    } else {
        value.max(max).min(min)
    }
}

fn sign_f32(value: f32) -> f32 {
    if value > 0.0 {
        1.0
    } else if value < 0.0 {
        -1.0
    } else {
        0.0
    }
}

fn is_spatial_vec_type(type_id: TypeId) -> bool {
    matches!(type_id, TypeId::Vec2 | TypeId::Vec3 | TypeId::Vec4)
}

trait VecObjExt {
    fn type_id(&self) -> TypeId;
}

impl VecObjExt for VecObj {
    fn type_id(&self) -> TypeId {
        match self.header.type_id {
            x if x == TypeId::Vec2 as u32 => TypeId::Vec2,
            x if x == TypeId::Vec3 as u32 => TypeId::Vec3,
            x if x == TypeId::Vec4 as u32 => TypeId::Vec4,
            x if x == TypeId::Quat as u32 => TypeId::Quat,
            _ => TypeId::Unknown,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{wr_approx_eq, wr_value_eq};
    use std::collections::hash_map::DefaultHasher;
    use std::hash::Hasher;

    fn hash_value(val: Value) -> u64 {
        let mut hasher = DefaultHasher::new();
        crate::value::value_hash(val, &mut hasher);
        hasher.finish()
    }

    #[test]
    fn constructors_and_component_getters_work() {
        let v = vec3_new(Value::from_int(1), Value::from_float(2.5), Value::from_int(3));
        assert!(wr_value_eq(vec_x(v), Value::from_float(1.0)).is_bool());
        assert!(wr_value_eq(vec_y(v), Value::from_float(2.5)).is_bool());
        assert!(wr_value_eq(vec_z(v), Value::from_float(3.0)).is_bool());

        let q = quat_new(
            Value::from_float(0.1),
            Value::from_float(0.2),
            Value::from_float(0.3),
            Value::from_float(0.4),
        );
        assert!(wr_value_eq(quat_x(q), Value::from_float(0.1)).is_bool());
        assert!(wr_value_eq(quat_w(q), Value::from_float(0.4)).is_bool());
    }

    #[test]
    fn vec2_surface_works() {
        let base = vec2_new(Value::from_int(3), Value::from_int(4));
        assert!(wr_value_eq(vec_x(base), Value::from_float(3.0)).is_bool());
        assert!(wr_value_eq(vec_y(base), Value::from_float(4.0)).is_bool());

        let len = vec_length(base);
        assert!(wr_approx_eq(len, Value::from_float(5.0), Value::from_float(1e-6)).as_bool());

        let unit = vec_normalize(base);
        let axis = vec2_new(Value::from_float(0.6), Value::from_float(0.8));
        let dot = vec_dot(unit, axis);
        assert!(wr_approx_eq(dot, Value::from_float(1.0), Value::from_float(1e-5)).as_bool());

        let shifted = vec_add(base, vec2_new(Value::from_int(1), Value::from_int(-1)));
        let restored = vec_div(vec_mul(shifted, Value::from_float(0.5)), Value::from_float(0.5));
        assert!(wr_value_eq(vec_x(restored), Value::from_float(4.0)).is_bool());
        assert!(wr_value_eq(vec_y(restored), Value::from_float(3.0)).is_bool());
    }

    #[test]
    fn vector_intrinsics_work() {
        let left = vec3_new(Value::from_float(-3.5), Value::from_float(2.0), Value::from_float(9.0));
        let right = vec3_new(Value::from_float(1.0), Value::from_float(4.0), Value::from_float(-2.0));
        let min_v = vec_min(left, right);
        let max_v = vec_max(left, right);
        assert!(wr_value_eq(vec_x(min_v), Value::from_float(-3.5)).is_bool());
        assert!(wr_value_eq(vec_y(max_v), Value::from_float(4.0)).is_bool());

        let clamped = vec_clamp(
            vec3_new(Value::from_float(-2.0), Value::from_float(0.4), Value::from_float(3.0)),
            Value::from_float(-1.0),
            Value::from_float(1.0),
        );
        assert!(wr_value_eq(vec_x(clamped), Value::from_float(-1.0)).is_bool());
        assert!(wr_value_eq(vec_z(clamped), Value::from_float(1.0)).is_bool());

        let mixed = vec_mix(
            vec3_new(Value::from_float(0.0), Value::from_float(10.0), Value::from_float(20.0)),
            vec3_new(Value::from_float(10.0), Value::from_float(20.0), Value::from_float(30.0)),
            Value::from_float(0.25),
        );
        assert!(wr_approx_eq(vec_x(mixed), Value::from_float(2.5), Value::from_float(1e-6)).as_bool());
        assert!(wr_approx_eq(vec_y(mixed), Value::from_float(12.5), Value::from_float(1e-6)).as_bool());

        let rooted = vec_sqrt(vec4_new(
            Value::from_float(1.0),
            Value::from_float(4.0),
            Value::from_float(9.0),
            Value::from_float(16.0),
        ));
        assert!(wr_value_eq(vec_w(rooted), Value::from_float(4.0)).is_bool());

        let signed = vec_sign(vec3_new(
            Value::from_float(-3.0),
            Value::from_float(0.0),
            Value::from_float(4.0),
        ));
        assert!(wr_value_eq(vec_x(signed), Value::from_float(-1.0)).is_bool());
        assert!(wr_value_eq(vec_y(signed), Value::from_float(0.0)).is_bool());
        assert!(wr_value_eq(vec_z(signed), Value::from_float(1.0)).is_bool());

        let trig = vec_sin(vec2_new(
            Value::from_float(0.0),
            Value::from_float(std::f32::consts::FRAC_PI_2 as f64),
        ));
        assert!(wr_approx_eq(vec_y(trig), Value::from_float(1.0), Value::from_float(1e-6)).as_bool());

        let dist = vec_distance(
            vec3_new(Value::from_float(0.0), Value::from_float(0.0), Value::from_float(0.0)),
            vec3_new(Value::from_float(0.0), Value::from_float(3.0), Value::from_float(4.0)),
        );
        assert!(wr_approx_eq(dist, Value::from_float(5.0), Value::from_float(1e-6)).as_bool());

        let reflected = vec_reflect(
            vec3_new(Value::from_float(1.0), Value::from_float(-1.0), Value::from_float(0.0)),
            vec3_new(Value::from_float(0.0), Value::from_float(1.0), Value::from_float(0.0)),
        );
        assert!(wr_approx_eq(vec_x(reflected), Value::from_float(1.0), Value::from_float(1e-6)).as_bool());
        assert!(wr_approx_eq(vec_y(reflected), Value::from_float(1.0), Value::from_float(1e-6)).as_bool());
    }

    #[test]
    fn scalar_intrinsics_work() {
        assert!(wr_approx_eq(
            vec_min(Value::from_float(2.0), Value::from_float(3.0)),
            Value::from_float(2.0),
            Value::from_float(1e-6)
        )
        .as_bool());
        assert!(wr_approx_eq(
            vec_max(Value::from_float(2.0), Value::from_float(3.0)),
            Value::from_float(3.0),
            Value::from_float(1e-6)
        )
        .as_bool());
        assert!(wr_approx_eq(
            vec_clamp(Value::from_float(5.0), Value::from_float(0.0), Value::from_float(4.0)),
            Value::from_float(4.0),
            Value::from_float(1e-6)
        )
        .as_bool());
        assert!(wr_approx_eq(
            vec_mix(
                Value::from_float(10.0),
                Value::from_float(20.0),
                Value::from_float(0.25),
            ),
            Value::from_float(12.5),
            Value::from_float(1e-6)
        )
        .as_bool());
    }

    #[test]
    fn vector_arithmetic_and_norms_work() {
        let a = vec3_new(Value::from_int(1), Value::from_int(2), Value::from_int(3));
        let b = vec3_new(Value::from_int(4), Value::from_int(5), Value::from_int(6));
        let sum = vec_add(a, b);
        assert!(wr_value_eq(vec_x(sum), Value::from_float(5.0)).is_bool());
        assert!(wr_value_eq(vec_y(sum), Value::from_float(7.0)).is_bool());
        assert!(wr_value_eq(vec_z(sum), Value::from_float(9.0)).is_bool());

        let dot = vec_dot(a, b);
        assert!(wr_value_eq(dot, Value::from_float(32.0)).is_bool());

        let len = vec_length(a);
        assert!(wr_approx_eq(
            len,
            Value::from_float(14.0_f32.sqrt() as f64),
            Value::from_float(1e-6)
        )
        .is_bool());

        let norm = vec_normalize(a);
        assert!(wr_approx_eq(vec_length(norm), Value::from_float(1.0), Value::from_float(1e-6)).is_bool());
    }

    #[test]
    fn vector_cross_and_scalar_ops_work() {
        let a = vec3_new(Value::from_int(1), Value::from_int(0), Value::from_int(0));
        let b = vec3_new(Value::from_int(0), Value::from_int(1), Value::from_int(0));
        let cross = vec_cross(a, b);
        assert!(wr_value_eq(vec_z(cross), Value::from_float(1.0)).is_bool());

        let scaled = vec_mul(a, Value::from_float(2.5));
        assert!(wr_value_eq(vec_x(scaled), Value::from_float(2.5)).is_bool());

        let divided = vec_div(scaled, Value::from_float(2.5));
        assert!(wr_value_eq(vec_x(divided), Value::from_float(1.0)).is_bool());
    }

    #[test]
    fn quaternion_and_matrix_operations_work() {
        let q = quat_new(
            Value::from_float(0.0),
            Value::from_float(3.0),
            Value::from_float(4.0),
            Value::from_float(0.0),
        );
        assert!(wr_approx_eq(vec_length(q), Value::from_float(5.0), Value::from_float(1e-6)).is_bool());
        let normalized = vec_normalize(q);
        assert!(wr_approx_eq(vec_length(normalized), Value::from_float(1.0), Value::from_float(1e-6)).is_bool());

        let c0 = vec3_new(Value::from_int(1), Value::from_int(0), Value::from_int(0));
        let c1 = vec3_new(Value::from_int(0), Value::from_int(1), Value::from_int(0));
        let c2 = vec3_new(Value::from_int(0), Value::from_int(0), Value::from_int(1));
        let m3 = mat3_from_columns(c0, c1, c2);
        let v3 = vec3_new(Value::from_int(1), Value::from_int(2), Value::from_int(3));
        let out3 = mat3_mul_vec3(m3, v3);
        assert!(wr_value_eq(vec_x(out3), Value::from_float(1.0)).is_bool());
        assert!(wr_value_eq(vec_z(out3), Value::from_float(3.0)).is_bool());

        let m3_scaled = mat3_div_scalar(mat3_mul_scalar(m3, Value::from_float(2.0)), Value::from_float(2.0));
        assert!(wr_value_eq(vec_x(mat3_mul_vec3(m3_scaled, v3)), Value::from_float(1.0)).is_bool());

        let c0 = vec4_new(Value::from_int(1), Value::from_int(0), Value::from_int(0), Value::from_int(0));
        let c1 = vec4_new(Value::from_int(0), Value::from_int(1), Value::from_int(0), Value::from_int(0));
        let c2 = vec4_new(Value::from_int(0), Value::from_int(0), Value::from_int(1), Value::from_int(0));
        let c3 = vec4_new(Value::from_int(4), Value::from_int(5), Value::from_int(6), Value::from_int(1));
        let m = mat4_from_columns(c0, c1, c2, c3);
        let v = vec4_new(Value::from_int(1), Value::from_int(2), Value::from_int(3), Value::from_int(1));
        let out = mat4_mul_vec4(m, v);
        assert!(wr_value_eq(vec_x(out), Value::from_float(5.0)).is_bool());
        assert!(wr_value_eq(vec_y(out), Value::from_float(7.0)).is_bool());
        assert!(wr_value_eq(vec_z(out), Value::from_float(9.0)).is_bool());
        assert!(wr_value_eq(vec_w(out), Value::from_float(1.0)).is_bool());

        let id = mat4_identity();
        let product = mat4_mul_mat4(id, m);
        assert!(wr_value_eq(vec_x(mat4_mul_vec4(product, v)), vec_x(out)).is_bool());
    }

    #[test]
    fn vec_and_mat_values_participate_in_equality_hashing_and_approx() {
        let a = vec4_new(Value::from_int(1), Value::from_int(2), Value::from_int(3), Value::from_int(4));
        let b = vec4_new(Value::from_int(1), Value::from_int(2), Value::from_int(3), Value::from_int(4));
        let c = vec4_new(Value::from_int(1), Value::from_int(2), Value::from_int(3), Value::from_int(5));
        assert!(wr_value_eq(a, b).is_bool());
        assert!(!wr_value_eq(a, c).as_bool());
        assert_eq!(hash_value(a), hash_value(b));

        let approx = vec4_new(
            Value::from_float(1.0),
            Value::from_float(2.0),
            Value::from_float(3.0001),
            Value::from_float(4.0),
        );
        assert!(wr_approx_eq(a, approx, Value::from_float(0.001)).as_bool());

        let m0 = mat3_identity();
        let m1 = mat3_identity();
        assert!(wr_value_eq(m0, m1).is_bool());
        assert_eq!(hash_value(m0), hash_value(m1));

        let m4_0 = mat4_identity();
        let m4_1 = mat4_identity();
        assert!(wr_value_eq(m4_0, m4_1).is_bool());
        assert_eq!(hash_value(m4_0), hash_value(m4_1));
    }
}
