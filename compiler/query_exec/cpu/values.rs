//! Owns CPU-oracle `KernelValue` construction, coercion, and builtin-shape
//! semantics.
//! Does not own high-level query traversal or backend dispatch policy.
//!
//! Key invariants:
//! - value construction here is part of the CPU oracle, so builtin semantics
//!   must stay aligned with what lowering and GPU backends claim to implement.
//! - coercions may normalize representation details, but they must not erase
//!   authored meaning or contract guarantees.
//!
//! Primary entrypoints:
//! - builtin/value construction and coercion helpers in this module
//!
//! Failure modes / common pitfalls:
//! - splitting these semantics across smaller helpers before the typed adapters
//!   stabilize would fragment CPU/GPU parity invariants.
//!
//! Phase 53 explicit size-cap exception: this module remains above the 2,500-line
//! target because it centralizes the CPU oracle's KernelValue construction,
//! coercion, and builtin-shape semantics in one place.
use super::*;

pub(super) fn construct_builtin_record(
    name: &str,
    args: &[KernelValue],
) -> Result<KernelValue, QueryExecError> {
    let Some(record) = portable::builtin_record_by_function(name) else {
        return Err(QueryExecError::Unsupported {
            message: format!("unknown builtin record constructor '{name}'"),
        });
    };
    construct_builtin_record_value(record, args)
}

pub(super) fn default_actor_handle() -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("ActorHandle"),
        fields: vec![
            (SmolStr::new("id"), KernelValue::U32(0)),
            (SmolStr::new("generation"), KernelValue::U32(0)),
        ],
    })
}

pub(crate) fn default_payload() -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("Payload"),
        fields: vec![
            (SmolStr::new("entity_id"), KernelValue::U32(0)),
            (SmolStr::new("material_id"), KernelValue::U32(0)),
            (SmolStr::new("actor"), default_actor_handle()),
        ],
    })
}

pub(crate) fn default_surface() -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("Surface"),
        fields: vec![
            (SmolStr::new("albedo"), KernelValue::Vec3([0.0, 0.0, 0.0])),
            (SmolStr::new("roughness"), KernelValue::F32(0.0)),
            (SmolStr::new("metalness"), KernelValue::F32(0.0)),
            (SmolStr::new("clearcoat"), KernelValue::F32(0.0)),
            (SmolStr::new("clearcoat_roughness"), KernelValue::F32(0.0)),
            (SmolStr::new("sheen"), KernelValue::F32(0.0)),
            (SmolStr::new("emissive"), KernelValue::Vec3([0.0, 0.0, 0.0])),
        ],
    })
}

pub(crate) fn default_medium() -> KernelValue {
    medium_value(0.0, [0.0, 0.0, 0.0], 0.0)
}

pub(super) fn default_builtin_record_value(name: &str) -> Result<KernelValue, QueryExecError> {
    Ok(match name {
        "ActorHandle" => default_actor_handle(),
        "Payload" => default_payload(),
        "Surface" => default_surface(),
        "Medium" => default_medium(),
        "Transform3" => transform3_identity_value(),
        "Hit3" => default_hit([0.0, 0.0, 0.0]),
        other => {
            let record =
                portable::builtin_record(other).ok_or_else(|| QueryExecError::Unsupported {
                    message: format!("unknown builtin record constructor '{other}'"),
                })?;
            let fields = record
                .fields
                .iter()
                .map(|field| {
                    Ok((
                        SmolStr::new(field.name),
                        default_builtin_field_value(field.ty)?,
                    ))
                })
                .collect::<Result<Vec<_>, QueryExecError>>()?;
            KernelValue::Struct(KernelStructValue {
                name: SmolStr::new(record.name),
                fields,
            })
        }
    })
}

pub(super) fn default_builtin_field_value(
    ty: portable::PortableBuiltinType,
) -> Result<KernelValue, QueryExecError> {
    use portable::PortableBuiltinAtom as Atom;
    use portable::PortableBuiltinType::{Atom as BuiltinAtom, Named as BuiltinNamed};

    match ty {
        BuiltinAtom(Atom::Bool) => Ok(KernelValue::Bool(false)),
        BuiltinAtom(Atom::I32) => Ok(KernelValue::I32(0)),
        BuiltinAtom(Atom::U32) => Ok(KernelValue::U32(0)),
        BuiltinAtom(Atom::F32) => Ok(KernelValue::F32(0.0)),
        BuiltinAtom(Atom::Vec2) => Ok(KernelValue::Vec2([0.0, 0.0])),
        BuiltinAtom(Atom::Vec3) => Ok(KernelValue::Vec3([0.0, 0.0, 0.0])),
        BuiltinAtom(Atom::Vec4) => Ok(KernelValue::Vec4([0.0, 0.0, 0.0, 0.0])),
        BuiltinAtom(Atom::Quat) => Ok(KernelValue::Quat([0.0, 0.0, 0.0, 0.0])),
        BuiltinAtom(Atom::Mat3) => Ok(KernelValue::Mat3([0.0; 9])),
        BuiltinAtom(Atom::Mat4) => Ok(KernelValue::Mat4([0.0; 16])),
        BuiltinNamed(name) => default_builtin_record_value(name),
    }
}

pub(super) fn construct_builtin_record_value(
    record: &portable::PortableBuiltinRecord,
    args: &[KernelValue],
) -> Result<KernelValue, QueryExecError> {
    let fields = record
        .fields
        .iter()
        .enumerate()
        .map(|(index, field)| {
            Ok((
                SmolStr::new(field.name),
                match args.get(index) {
                    Some(value) => value.clone(),
                    None => default_builtin_field_value(field.ty)?,
                },
            ))
        })
        .collect::<Result<Vec<_>, QueryExecError>>()?;
    Ok(KernelValue::Struct(KernelStructValue {
        name: SmolStr::new(record.name),
        fields,
    }))
}

pub(crate) fn medium_value(density: f32, emission: [f32; 3], anisotropy: f32) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("Medium"),
        fields: vec![
            (SmolStr::new("density"), KernelValue::F32(density)),
            (SmolStr::new("emission"), KernelValue::Vec3(emission)),
            (SmolStr::new("anisotropy"), KernelValue::F32(anisotropy)),
        ],
    })
}

pub(super) fn polygon_profile_distance(
    point: [f32; 2],
    vertices: &KernelValue,
    closed: bool,
) -> Result<f32, QueryExecError> {
    let KernelValue::Array(items) = vertices else {
        return Err(QueryExecError::TypeMismatch {
            expected: "Array<Vec2>".to_string(),
            found: format!("{vertices:?}"),
        });
    };
    let vertices = items
        .iter()
        .map(|value| match value {
            KernelValue::Vec2(value) => Ok(*value),
            other => Err(QueryExecError::TypeMismatch {
                expected: "Vec2".to_string(),
                found: format!("{other:?}"),
            }),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let min_len = if closed { 3 } else { 2 };
    if vertices.len() < min_len {
        return Err(QueryExecError::Unsupported {
            message: format!(
                "{} expects at least {min_len} vertices",
                if closed { "polygon2" } else { "polyline2" }
            ),
        });
    }
    let mut best = f32::INFINITY;
    if closed {
        let mut inside = false;
        for index in 0..vertices.len() {
            let a = vertices[index];
            let b = vertices[(index + 1) % vertices.len()];
            best = best.min(segment_distance_2d(point, a, b));
            let crosses = ((a[1] > point[1]) != (b[1] > point[1]))
                && (point[0]
                    < (b[0] - a[0]) * (point[1] - a[1]) / (b[1] - a[1] + f32::EPSILON) + a[0]);
            if crosses {
                inside = !inside;
            }
        }
        Ok(if inside { -best } else { best })
    } else {
        for pair in vertices.windows(2) {
            best = best.min(segment_distance_2d(point, pair[0], pair[1]));
        }
        Ok(best)
    }
}

pub(super) fn segment_distance_2d(point: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let edge = [b[0] - a[0], b[1] - a[1]];
    let ap = [point[0] - a[0], point[1] - a[1]];
    let denom = edge[0] * edge[0] + edge[1] * edge[1];
    let t = if denom == 0.0 {
        0.0
    } else {
        ((ap[0] * edge[0] + ap[1] * edge[1]) / denom).clamp(0.0, 1.0)
    };
    let closest = [a[0] + edge[0] * t, a[1] + edge[1] * t];
    let delta = [point[0] - closest[0], point[1] - closest[1]];
    (delta[0] * delta[0] + delta[1] * delta[1]).sqrt()
}

pub(super) fn distance_result(distance: f32) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("DistanceResult"),
        fields: vec![(SmolStr::new("distance"), KernelValue::F32(distance))],
    })
}

pub(super) fn normal_result(normal: [f32; 3]) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("NormalResult"),
        fields: vec![(SmolStr::new("normal"), KernelValue::Vec3(normal))],
    })
}

pub(super) fn occlusion_result(occluded: bool, distance: f32, steps: i32) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("OcclusionResult"),
        fields: vec![
            (SmolStr::new("occluded"), KernelValue::Bool(occluded)),
            (SmolStr::new("distance"), KernelValue::F32(distance)),
            (SmolStr::new("steps"), KernelValue::I32(steps)),
        ],
    })
}

pub(super) fn transform3_identity_value() -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("Transform3"),
        fields: vec![
            (SmolStr::new("matrix"), KernelValue::Mat4(mat4_identity())),
            (SmolStr::new("inverse"), KernelValue::Mat4(mat4_identity())),
        ],
    })
}

pub(crate) fn hit_value(
    hit: bool,
    distance: f32,
    position: [f32; 3],
    normal: [f32; 3],
    local_position: [f32; 3],
    local_normal: [f32; 3],
    steps: i32,
    feature_id: u32,
    instance_id: u32,
    repeat_id: u32,
    root_shape_id: u32,
    payload: KernelValue,
) -> KernelValue {
    let shading_frame = stable_surface_frame(position, normal);
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("Hit3"),
        fields: vec![
            (SmolStr::new("hit"), KernelValue::Bool(hit)),
            (SmolStr::new("distance"), KernelValue::F32(distance)),
            (SmolStr::new("position"), KernelValue::Vec3(position)),
            (SmolStr::new("normal"), KernelValue::Vec3(normal)),
            (
                SmolStr::new("local_position"),
                KernelValue::Vec3(local_position),
            ),
            (
                SmolStr::new("local_normal"),
                KernelValue::Vec3(local_normal),
            ),
            (SmolStr::new("shading_frame"), shading_frame),
            (SmolStr::new("steps"), KernelValue::I32(steps)),
            (SmolStr::new("feature_id"), KernelValue::U32(feature_id)),
            (SmolStr::new("instance_id"), KernelValue::U32(instance_id)),
            (SmolStr::new("repeat_id"), KernelValue::U32(repeat_id)),
            (
                SmolStr::new("root_shape_id"),
                KernelValue::U32(root_shape_id),
            ),
            (SmolStr::new("payload"), payload),
        ],
    })
}

pub(crate) fn default_hit(origin: [f32; 3]) -> KernelValue {
    hit_value(
        false,
        0.0,
        origin,
        [0.0, 0.0, 1.0],
        origin,
        [0.0, 0.0, 1.0],
        0,
        0,
        0,
        0,
        0,
        default_payload(),
    )
}

pub(super) fn mat4_identity() -> [f32; 16] {
    [
        1.0, 0.0, 0.0, 0.0, //
        0.0, 1.0, 0.0, 0.0, //
        0.0, 0.0, 1.0, 0.0, //
        0.0, 0.0, 0.0, 1.0,
    ]
}

pub(super) fn compose_transform3_value(
    args: &[KernelValue],
) -> Result<KernelValue, QueryExecError> {
    let [left, right] = args else {
        return Err(QueryExecError::Unsupported {
            message: "compose_transform3 expects two arguments".to_string(),
        });
    };
    let left = expect_struct_ref(left, "Transform3")?;
    let right = expect_struct_ref(right, "Transform3")?;
    let left_matrix = expect_struct_mat4(left, "matrix")?;
    let left_inverse = expect_struct_mat4(left, "inverse")?;
    let right_matrix = expect_struct_mat4(right, "matrix")?;
    let right_inverse = expect_struct_mat4(right, "inverse")?;
    Ok(KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("Transform3"),
        fields: vec![
            (
                SmolStr::new("matrix"),
                KernelValue::Mat4(mul_mat4(left_matrix, right_matrix)),
            ),
            (
                SmolStr::new("inverse"),
                KernelValue::Mat4(mul_mat4(right_inverse, left_inverse)),
            ),
        ],
    }))
}

pub(super) fn inverse_transform3_value(
    args: &[KernelValue],
) -> Result<KernelValue, QueryExecError> {
    let [transform] = args else {
        return Err(QueryExecError::Unsupported {
            message: "inverse_transform3 expects one argument".to_string(),
        });
    };
    let transform = expect_struct_ref(transform, "Transform3")?;
    Ok(KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("Transform3"),
        fields: vec![
            (
                SmolStr::new("matrix"),
                KernelValue::Mat4(expect_struct_mat4(transform, "inverse")?),
            ),
            (
                SmolStr::new("inverse"),
                KernelValue::Mat4(expect_struct_mat4(transform, "matrix")?),
            ),
        ],
    }))
}

pub(super) fn mul_mat4(left: [f32; 16], right: [f32; 16]) -> [f32; 16] {
    let mut out = [0.0; 16];
    for column in 0..4 {
        for row in 0..4 {
            out[column * 4 + row] = left[row] * right[column * 4]
                + left[4 + row] * right[column * 4 + 1]
                + left[8 + row] * right[column * 4 + 2]
                + left[12 + row] * right[column * 4 + 3];
        }
    }
    out
}

pub(super) fn unary_componentwise(
    args: &[KernelValue],
    name: &str,
    f: impl Fn(f32) -> f32,
) -> Result<KernelValue, QueryExecError> {
    let [value] = args else {
        return Err(QueryExecError::Unsupported {
            message: format!("{name} expects one argument"),
        });
    };
    map_components(value, name, |value, _| f(value))
}

pub(super) fn binary_componentwise(
    args: &[KernelValue],
    name: &str,
    f: impl Fn(f32, f32) -> f32,
) -> Result<KernelValue, QueryExecError> {
    let [lhs, rhs] = args else {
        return Err(QueryExecError::Unsupported {
            message: format!("{name} expects two arguments"),
        });
    };
    map_pair_components(lhs, rhs, name, |lhs, rhs, _| f(lhs, rhs))
}

pub(super) fn ternary_componentwise(
    args: &[KernelValue],
    name: &str,
    f: impl Fn(f32, f32, f32) -> f32,
) -> Result<KernelValue, QueryExecError> {
    let [a, b, c] = args else {
        return Err(QueryExecError::Unsupported {
            message: format!("{name} expects three arguments"),
        });
    };
    map_triple_components(a, b, c, name, |a, b, c, _| f(a, b, c))
}

pub(super) fn distance_builtin(args: &[KernelValue]) -> Result<KernelValue, QueryExecError> {
    let [lhs, rhs] = args else {
        return Err(QueryExecError::Unsupported {
            message: "distance expects two arguments".to_string(),
        });
    };
    let lhs = kernel_components(lhs, "distance")?;
    let rhs = broadcast_components(rhs, lhs.len(), "distance")?;
    let sum = lhs
        .iter()
        .zip(rhs.iter())
        .map(|(lhs, rhs)| {
            let delta = lhs - rhs;
            delta * delta
        })
        .sum::<f32>();
    Ok(KernelValue::F32(sum.sqrt()))
}

pub(super) fn dot_builtin(args: &[KernelValue]) -> Result<KernelValue, QueryExecError> {
    let [lhs, rhs] = args else {
        return Err(QueryExecError::Unsupported {
            message: "dot expects two arguments".to_string(),
        });
    };
    let lhs = kernel_components(lhs, "dot")?;
    let rhs = broadcast_components(rhs, lhs.len(), "dot")?;
    Ok(KernelValue::F32(
        lhs.iter().zip(rhs.iter()).map(|(lhs, rhs)| lhs * rhs).sum(),
    ))
}

pub(super) fn length_builtin(args: &[KernelValue]) -> Result<KernelValue, QueryExecError> {
    let [value] = args else {
        return Err(QueryExecError::Unsupported {
            message: "length expects one argument".to_string(),
        });
    };
    let components = kernel_components(value, "length")?;
    let len_sq = components
        .iter()
        .map(|component| component * component)
        .sum::<f32>();
    Ok(KernelValue::F32(len_sq.sqrt()))
}

pub(super) fn normalize_builtin(args: &[KernelValue]) -> Result<KernelValue, QueryExecError> {
    let [value] = args else {
        return Err(QueryExecError::Unsupported {
            message: "normalize expects one argument".to_string(),
        });
    };
    let components = kernel_components(value, "normalize")?;
    let len_sq = components
        .iter()
        .map(|component| component * component)
        .sum::<f32>();
    if len_sq == 0.0 {
        return same_kind_from_components(value, &vec![0.0; components.len()], "normalize");
    }
    let len = len_sq.sqrt();
    let normalized = components
        .into_iter()
        .map(|component| component / len)
        .collect::<Vec<_>>();
    same_kind_from_components(value, &normalized, "normalize")
}

pub(super) fn cross_builtin(args: &[KernelValue]) -> Result<KernelValue, QueryExecError> {
    let [lhs, rhs] = args else {
        return Err(QueryExecError::Unsupported {
            message: "cross expects two arguments".to_string(),
        });
    };
    let lhs = expect_vec3_like(lhs, "cross")?;
    let rhs = expect_vec3_like(rhs, "cross")?;
    Ok(KernelValue::Vec3([
        lhs[1] * rhs[2] - lhs[2] * rhs[1],
        lhs[2] * rhs[0] - lhs[0] * rhs[2],
        lhs[0] * rhs[1] - lhs[1] * rhs[0],
    ]))
}

pub(super) fn reflect_builtin(args: &[KernelValue]) -> Result<KernelValue, QueryExecError> {
    let [incident, normal] = args else {
        return Err(QueryExecError::Unsupported {
            message: "reflect expects two arguments".to_string(),
        });
    };
    let incident_components = kernel_components(incident, "reflect")?;
    let normal_components = broadcast_components(normal, incident_components.len(), "reflect")?;
    let dot = incident_components
        .iter()
        .zip(normal_components.iter())
        .map(|(lhs, rhs)| lhs * rhs)
        .sum::<f32>();
    let reflected = incident_components
        .iter()
        .zip(normal_components.iter())
        .map(|(incident, normal)| incident - 2.0 * dot * normal)
        .collect::<Vec<_>>();
    same_kind_from_components(incident, &reflected, "reflect")
}

pub(super) fn map_components(
    value: &KernelValue,
    name: &str,
    f: impl Fn(f32, usize) -> f32,
) -> Result<KernelValue, QueryExecError> {
    let components = kernel_components(value, name)?;
    let mapped = components
        .iter()
        .enumerate()
        .map(|(index, value)| f(*value, index))
        .collect::<Vec<_>>();
    same_kind_from_components(value, &mapped, name)
}

pub(super) fn map_pair_components(
    lhs: &KernelValue,
    rhs: &KernelValue,
    name: &str,
    f: impl Fn(f32, f32, usize) -> f32,
) -> Result<KernelValue, QueryExecError> {
    let lhs_components = kernel_components(lhs, name)?;
    let rhs_components = broadcast_components(rhs, lhs_components.len(), name)?;
    let mapped = lhs_components
        .iter()
        .zip(rhs_components.iter())
        .enumerate()
        .map(|(index, (lhs, rhs))| f(*lhs, *rhs, index))
        .collect::<Vec<_>>();
    same_kind_from_components(lhs, &mapped, name)
}

pub(super) fn map_triple_components(
    a: &KernelValue,
    b: &KernelValue,
    c: &KernelValue,
    name: &str,
    f: impl Fn(f32, f32, f32, usize) -> f32,
) -> Result<KernelValue, QueryExecError> {
    let a_components = kernel_components(a, name)?;
    let b_components = broadcast_components(b, a_components.len(), name)?;
    let c_components = broadcast_components(c, a_components.len(), name)?;
    let mapped = a_components
        .iter()
        .zip(b_components.iter())
        .zip(c_components.iter())
        .enumerate()
        .map(|(index, ((a, b), c))| f(*a, *b, *c, index))
        .collect::<Vec<_>>();
    same_kind_from_components(a, &mapped, name)
}

pub(super) fn kernel_components(
    value: &KernelValue,
    name: &str,
) -> Result<Vec<f32>, QueryExecError> {
    match value {
        KernelValue::I32(value) => Ok(vec![*value as f32]),
        KernelValue::U32(value) => Ok(vec![*value as f32]),
        KernelValue::F32(value) => Ok(vec![*value]),
        KernelValue::Vec2(value) => Ok(value.to_vec()),
        KernelValue::Vec3(value) => Ok(value.to_vec()),
        KernelValue::Vec4(value) | KernelValue::Quat(value) => Ok(value.to_vec()),
        other => Err(QueryExecError::TypeMismatch {
            expected: format!("{name}: scalar or vector"),
            found: format!("{other:?}"),
        }),
    }
}

pub(super) fn broadcast_components(
    value: &KernelValue,
    target_len: usize,
    name: &str,
) -> Result<Vec<f32>, QueryExecError> {
    let components = kernel_components(value, name)?;
    if components.len() == target_len {
        return Ok(components);
    }
    if components.len() == 1 {
        return Ok(vec![components[0]; target_len]);
    }
    Err(QueryExecError::TypeMismatch {
        expected: format!("{name}: broadcastable to {target_len} lanes"),
        found: format!("{value:?}"),
    })
}

pub(super) fn same_kind_from_components(
    prototype: &KernelValue,
    components: &[f32],
    name: &str,
) -> Result<KernelValue, QueryExecError> {
    match prototype {
        KernelValue::I32(_) => Ok(KernelValue::I32(components[0] as i32)),
        KernelValue::U32(_) => Ok(KernelValue::U32(components[0].max(0.0) as u32)),
        KernelValue::F32(_) => Ok(KernelValue::F32(components[0])),
        KernelValue::Vec2(_) => Ok(KernelValue::Vec2([components[0], components[1]])),
        KernelValue::Vec3(_) => Ok(KernelValue::Vec3([
            components[0],
            components[1],
            components[2],
        ])),
        KernelValue::Vec4(_) => Ok(KernelValue::Vec4([
            components[0],
            components[1],
            components[2],
            components[3],
        ])),
        KernelValue::Quat(_) => Ok(KernelValue::Quat([
            components[0],
            components[1],
            components[2],
            components[3],
        ])),
        other => Err(QueryExecError::TypeMismatch {
            expected: format!("{name}: scalar or vector"),
            found: format!("{other:?}"),
        }),
    }
}

pub(super) fn expect_vec3_like(
    value: &KernelValue,
    name: &str,
) -> Result<[f32; 3], QueryExecError> {
    match value {
        KernelValue::Vec3(value) => Ok(*value),
        other => Err(QueryExecError::TypeMismatch {
            expected: format!("{name}: Vec3"),
            found: format!("{other:?}"),
        }),
    }
}

pub(super) fn literal_to_kernel(literal: &Literal) -> KernelValue {
    match literal {
        Literal::Integer(value) => KernelValue::I32(*value as i32),
        Literal::Float(value) => KernelValue::F32(*value as f32),
        Literal::Boolean(value) => KernelValue::Bool(*value),
        Literal::Nil => KernelValue::Nothing,
        Literal::String(_) => KernelValue::Nothing,
    }
}

pub(super) fn eval_unary_value(
    op: UnaryOp,
    value: KernelValue,
) -> Result<KernelValue, QueryExecError> {
    match (op, value) {
        (UnaryOp::Neg, KernelValue::I32(value)) => Ok(KernelValue::I32(-value)),
        (UnaryOp::Neg, KernelValue::F32(value)) => Ok(KernelValue::F32(-value)),
        (UnaryOp::Neg, KernelValue::Vec2([x, y])) => Ok(KernelValue::Vec2([-x, -y])),
        (UnaryOp::Neg, KernelValue::Vec3([x, y, z])) => Ok(KernelValue::Vec3([-x, -y, -z])),
        (UnaryOp::Neg, KernelValue::Vec4([x, y, z, w])) => Ok(KernelValue::Vec4([-x, -y, -z, -w])),
        (UnaryOp::Neg, KernelValue::Quat([x, y, z, w])) => Ok(KernelValue::Quat([-x, -y, -z, -w])),
        (UnaryOp::Neg, KernelValue::Mat3(values)) => Ok(KernelValue::Mat3([
            -values[0], -values[1], -values[2], -values[3], -values[4], -values[5], -values[6],
            -values[7], -values[8],
        ])),
        (UnaryOp::Neg, KernelValue::Mat4(values)) => Ok(KernelValue::Mat4([
            -values[0],
            -values[1],
            -values[2],
            -values[3],
            -values[4],
            -values[5],
            -values[6],
            -values[7],
            -values[8],
            -values[9],
            -values[10],
            -values[11],
            -values[12],
            -values[13],
            -values[14],
            -values[15],
        ])),
        (UnaryOp::Not, KernelValue::Bool(value)) => Ok(KernelValue::Bool(!value)),
        (UnaryOp::BitNot, KernelValue::I32(value)) => Ok(KernelValue::I32(!value)),
        (UnaryOp::BitNot, KernelValue::U32(value)) => Ok(KernelValue::U32(!value)),
        (_, value) => Err(QueryExecError::Unsupported {
            message: format!("unary op {op:?} does not support {value:?}"),
        }),
    }
}

pub(super) fn eval_binary_value(
    op: BinaryOp,
    lhs: KernelValue,
    rhs: KernelValue,
) -> Result<KernelValue, QueryExecError> {
    match (op, lhs, rhs) {
        (BinaryOp::Add, KernelValue::I32(lhs), KernelValue::I32(rhs)) => {
            Ok(KernelValue::I32(lhs.saturating_add(rhs)))
        }
        (BinaryOp::Sub, KernelValue::I32(lhs), KernelValue::I32(rhs)) => {
            Ok(KernelValue::I32(lhs.saturating_sub(rhs)))
        }
        (BinaryOp::Mul, KernelValue::I32(lhs), KernelValue::I32(rhs)) => {
            Ok(KernelValue::I32(lhs.saturating_mul(rhs)))
        }
        (BinaryOp::Div, KernelValue::I32(lhs), KernelValue::I32(rhs)) => {
            Ok(KernelValue::I32(lhs.checked_div(rhs).unwrap_or(0)))
        }
        (BinaryOp::Eq, KernelValue::I32(lhs), KernelValue::I32(rhs)) => {
            Ok(KernelValue::Bool(lhs == rhs))
        }
        (BinaryOp::Eq, KernelValue::U32(lhs), KernelValue::U32(rhs)) => {
            Ok(KernelValue::Bool(lhs == rhs))
        }
        (BinaryOp::Eq, KernelValue::F32(lhs), KernelValue::F32(rhs)) => {
            Ok(KernelValue::Bool((lhs - rhs).abs() < f32::EPSILON))
        }
        (BinaryOp::Eq, KernelValue::Bool(lhs), KernelValue::Bool(rhs)) => {
            Ok(KernelValue::Bool(lhs == rhs))
        }
        (BinaryOp::Ne, lhs, rhs) => {
            let KernelValue::Bool(eq) = eval_binary_value(BinaryOp::Eq, lhs, rhs)? else {
                return Err(QueryExecError::Unsupported {
                    message: "binary Ne expected boolean equality result".to_string(),
                });
            };
            Ok(KernelValue::Bool(!eq))
        }
        (BinaryOp::And, KernelValue::Bool(lhs), KernelValue::Bool(rhs)) => {
            Ok(KernelValue::Bool(lhs && rhs))
        }
        (BinaryOp::Or, KernelValue::Bool(lhs), KernelValue::Bool(rhs)) => {
            Ok(KernelValue::Bool(lhs || rhs))
        }
        (BinaryOp::Lt, KernelValue::I32(lhs), KernelValue::I32(rhs)) => {
            Ok(KernelValue::Bool(lhs < rhs))
        }
        (BinaryOp::Le, KernelValue::I32(lhs), KernelValue::I32(rhs)) => {
            Ok(KernelValue::Bool(lhs <= rhs))
        }
        (BinaryOp::Gt, KernelValue::I32(lhs), KernelValue::I32(rhs)) => {
            Ok(KernelValue::Bool(lhs > rhs))
        }
        (BinaryOp::Ge, KernelValue::I32(lhs), KernelValue::I32(rhs)) => {
            Ok(KernelValue::Bool(lhs >= rhs))
        }
        (BinaryOp::Lt, KernelValue::F32(lhs), KernelValue::F32(rhs)) => {
            Ok(KernelValue::Bool(lhs < rhs))
        }
        (BinaryOp::Le, KernelValue::F32(lhs), KernelValue::F32(rhs)) => {
            Ok(KernelValue::Bool(lhs <= rhs))
        }
        (BinaryOp::Gt, KernelValue::F32(lhs), KernelValue::F32(rhs)) => {
            Ok(KernelValue::Bool(lhs > rhs))
        }
        (BinaryOp::Ge, KernelValue::F32(lhs), KernelValue::F32(rhs)) => {
            Ok(KernelValue::Bool(lhs >= rhs))
        }
        (BinaryOp::Add, KernelValue::F32(lhs), KernelValue::F32(rhs)) => {
            Ok(KernelValue::F32(lhs + rhs))
        }
        (BinaryOp::Sub, KernelValue::F32(lhs), KernelValue::F32(rhs)) => {
            Ok(KernelValue::F32(lhs - rhs))
        }
        (BinaryOp::Mul, KernelValue::F32(lhs), KernelValue::F32(rhs)) => {
            Ok(KernelValue::F32(lhs * rhs))
        }
        (BinaryOp::Div, KernelValue::F32(lhs), KernelValue::F32(rhs)) => {
            Ok(KernelValue::F32(lhs / rhs))
        }
        (BinaryOp::Add, KernelValue::Vec2(lhs), KernelValue::Vec2(rhs)) => Ok(
            runtime_binary_value(KernelValue::Vec2(lhs), KernelValue::Vec2(rhs), wr_vec_add)?,
        ),
        (BinaryOp::Sub, KernelValue::Vec2(lhs), KernelValue::Vec2(rhs)) => Ok(
            runtime_binary_value(KernelValue::Vec2(lhs), KernelValue::Vec2(rhs), wr_vec_sub)?,
        ),
        (BinaryOp::Mul, KernelValue::Vec2(lhs), KernelValue::Vec2(rhs)) => Ok(
            runtime_binary_value(KernelValue::Vec2(lhs), KernelValue::Vec2(rhs), wr_vec_mul)?,
        ),
        (BinaryOp::Div, KernelValue::Vec2(lhs), KernelValue::Vec2(rhs)) => Ok(
            runtime_binary_value(KernelValue::Vec2(lhs), KernelValue::Vec2(rhs), wr_vec_div)?,
        ),
        (BinaryOp::Add, KernelValue::Vec3(lhs), KernelValue::Vec3(rhs)) => Ok(
            runtime_binary_value(KernelValue::Vec3(lhs), KernelValue::Vec3(rhs), wr_vec_add)?,
        ),
        (BinaryOp::Sub, KernelValue::Vec3(lhs), KernelValue::Vec3(rhs)) => Ok(
            runtime_binary_value(KernelValue::Vec3(lhs), KernelValue::Vec3(rhs), wr_vec_sub)?,
        ),
        (BinaryOp::Mul, KernelValue::Vec3(lhs), KernelValue::Vec3(rhs)) => Ok(
            runtime_binary_value(KernelValue::Vec3(lhs), KernelValue::Vec3(rhs), wr_vec_mul)?,
        ),
        (BinaryOp::Div, KernelValue::Vec3(lhs), KernelValue::Vec3(rhs)) => Ok(
            runtime_binary_value(KernelValue::Vec3(lhs), KernelValue::Vec3(rhs), wr_vec_div)?,
        ),
        (BinaryOp::Add, KernelValue::Vec4(lhs), KernelValue::Vec4(rhs)) => Ok(
            runtime_binary_value(KernelValue::Vec4(lhs), KernelValue::Vec4(rhs), wr_vec_add)?,
        ),
        (BinaryOp::Sub, KernelValue::Vec4(lhs), KernelValue::Vec4(rhs)) => Ok(
            runtime_binary_value(KernelValue::Vec4(lhs), KernelValue::Vec4(rhs), wr_vec_sub)?,
        ),
        (BinaryOp::Mul, KernelValue::Vec4(lhs), KernelValue::Vec4(rhs)) => Ok(
            runtime_binary_value(KernelValue::Vec4(lhs), KernelValue::Vec4(rhs), wr_vec_mul)?,
        ),
        (BinaryOp::Div, KernelValue::Vec4(lhs), KernelValue::Vec4(rhs)) => Ok(
            runtime_binary_value(KernelValue::Vec4(lhs), KernelValue::Vec4(rhs), wr_vec_div)?,
        ),
        (op @ (BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div), lhs, rhs)
            if is_componentwise_numeric(&lhs) && is_componentwise_numeric(&rhs) =>
        {
            eval_componentwise_binary(op, lhs, rhs)
        }
        (op, lhs, rhs) => Err(QueryExecError::Unsupported {
            message: format!("binary op {op:?} does not support {lhs:?} and {rhs:?}"),
        }),
    }
}

pub(super) fn is_componentwise_numeric(value: &KernelValue) -> bool {
    matches!(
        value,
        KernelValue::F32(_)
            | KernelValue::Vec2(_)
            | KernelValue::Vec3(_)
            | KernelValue::Vec4(_)
            | KernelValue::Quat(_)
    )
}

pub(super) fn eval_componentwise_binary(
    op: BinaryOp,
    lhs: KernelValue,
    rhs: KernelValue,
) -> Result<KernelValue, QueryExecError> {
    let lhs_lane_count = kernel_components(&lhs, "componentwise binary")?.len();
    let rhs_lane_count = kernel_components(&rhs, "componentwise binary")?.len();
    let target_len = lhs_lane_count.max(rhs_lane_count);
    let lhs_components = broadcast_components(&lhs, target_len, "componentwise binary")?;
    let rhs_components = broadcast_components(&rhs, target_len, "componentwise binary")?;
    let mapped = lhs_components
        .iter()
        .zip(rhs_components.iter())
        .map(|(lhs, rhs)| match op {
            BinaryOp::Add => lhs + rhs,
            BinaryOp::Sub => lhs - rhs,
            BinaryOp::Mul => lhs * rhs,
            BinaryOp::Div => lhs / rhs,
            _ => unreachable!("componentwise helper only handles arithmetic"),
        })
        .collect::<Vec<_>>();
    let prototype = if lhs_lane_count >= rhs_lane_count {
        &lhs
    } else {
        &rhs
    };
    same_kind_from_components(prototype, &mapped, "componentwise binary")
}

pub(super) fn eval_member_value(
    base: KernelValue,
    member: &SmolStr,
) -> Result<KernelValue, QueryExecError> {
    match base {
        KernelValue::Struct(value) => value
            .fields
            .iter()
            .find(|(name, _)| name == member)
            .map(|(_, value)| value.clone())
            .ok_or_else(|| QueryExecError::Unsupported {
                message: format!(
                    "struct '{}' does not contain member '{}'",
                    value.name, member
                ),
            }),
        KernelValue::Vec2(value) => vector_member(&value, member, "xy"),
        KernelValue::Vec3(value) => vector_member(&value, member, "xyz"),
        KernelValue::Vec4(value) | KernelValue::Quat(value) => {
            vector_member(&value, member, "xyzw")
        }
        other => Err(QueryExecError::Unsupported {
            message: format!("member access is not implemented for {other:?}"),
        }),
    }
}

pub(super) fn eval_index_value(
    base: KernelValue,
    index: KernelValue,
) -> Result<KernelValue, QueryExecError> {
    let index = match index {
        KernelValue::I32(value) if value >= 0 => value as usize,
        KernelValue::U32(value) => value as usize,
        other => {
            return Err(QueryExecError::TypeMismatch {
                expected: "array/vector index".to_string(),
                found: format!("{other:?}"),
            });
        }
    };
    match base {
        KernelValue::Array(items) => {
            items
                .get(index)
                .cloned()
                .ok_or_else(|| QueryExecError::Unsupported {
                    message: format!("index {index} is out of bounds"),
                })
        }
        KernelValue::Vec2(values) => {
            values
                .get(index)
                .copied()
                .map(KernelValue::F32)
                .ok_or_else(|| QueryExecError::Unsupported {
                    message: format!("index {index} is out of bounds"),
                })
        }
        KernelValue::Vec3(values) => {
            values
                .get(index)
                .copied()
                .map(KernelValue::F32)
                .ok_or_else(|| QueryExecError::Unsupported {
                    message: format!("index {index} is out of bounds"),
                })
        }
        KernelValue::Vec4(values) | KernelValue::Quat(values) => values
            .get(index)
            .copied()
            .map(KernelValue::F32)
            .ok_or_else(|| QueryExecError::Unsupported {
                message: format!("index {index} is out of bounds"),
            }),
        other => Err(QueryExecError::Unsupported {
            message: format!("indexing is not implemented for {other:?}"),
        }),
    }
}

pub(super) fn vector_member<const N: usize>(
    values: &[f32; N],
    member: &SmolStr,
    alphabet: &str,
) -> Result<KernelValue, QueryExecError> {
    let Some(index) = alphabet.find(member.as_str()) else {
        return Err(QueryExecError::Unsupported {
            message: format!("unknown vector member '{member}'"),
        });
    };
    values
        .get(index)
        .copied()
        .map(KernelValue::F32)
        .ok_or_else(|| QueryExecError::Unsupported {
            message: format!("unknown vector member '{member}'"),
        })
}

pub(super) fn value_label(value: &KernelValue) -> String {
    match value {
        KernelValue::Nothing => "Nothing".to_string(),
        KernelValue::Bool(_) => "Bool".to_string(),
        KernelValue::I32(_) => "I32".to_string(),
        KernelValue::U32(_) => "U32".to_string(),
        KernelValue::F32(_) => "F32".to_string(),
        KernelValue::Vec2(_) => "Vec2".to_string(),
        KernelValue::Vec3(_) => "Vec3".to_string(),
        KernelValue::Vec4(_) => "Vec4".to_string(),
        KernelValue::Mat3(_) => "Mat3".to_string(),
        KernelValue::Mat4(_) => "Mat4".to_string(),
        KernelValue::Quat(_) => "Quat".to_string(),
        KernelValue::Array(_) => "Array".to_string(),
        KernelValue::Struct(value) => value.name.to_string(),
        KernelValue::Capture(name) => format!("Capture({name})"),
        KernelValue::DispatchBackend(_) => "DispatchBackend".to_string(),
        KernelValue::GpuBuffer(_) => "GpuBuffer".to_string(),
        KernelValue::GpuAtomicI32(_) => "GpuAtomicI32".to_string(),
        KernelValue::GpuAtomicU32(_) => "GpuAtomicU32".to_string(),
    }
}

pub(super) fn default_shape_winner() -> ShapeWinner {
    ShapeWinner {
        distance: 1_000_000.0,
        feature_id: 0,
        leaf: None,
    }
}

pub(super) fn chain_identity_component(current: u32, component: u32) -> u32 {
    if component == 0 {
        return current;
    }
    if current == 0 {
        return component;
    }
    let mixed = (current ^ component).wrapping_mul(16_777_619);
    if mixed == 0 { 1 } else { mixed }
}

pub(super) fn add3(lhs: [f32; 3], rhs: [f32; 3]) -> [f32; 3] {
    [lhs[0] + rhs[0], lhs[1] + rhs[1], lhs[2] + rhs[2]]
}

pub(super) fn mul3_scalar(value: [f32; 3], scalar: f32) -> [f32; 3] {
    [value[0] * scalar, value[1] * scalar, value[2] * scalar]
}

pub(super) fn transform_certified_normal(
    kind: TransformKind,
    config: &KernelValue,
    normal: [f32; 3],
) -> Result<[f32; 3], QueryExecError> {
    match kind {
        TransformKind::Translate | TransformKind::UniformScale => Ok(normal),
        TransformKind::Rotate => {
            let rotation = match config {
                KernelValue::F32(angle) => KernelValue::F32(-angle),
                KernelValue::Vec3(rotation) => {
                    KernelValue::Vec3([-rotation[0], -rotation[1], -rotation[2]])
                }
                other => {
                    return Err(QueryExecError::TypeMismatch {
                        expected: "rotate normal parameter: Float or Vec3".to_string(),
                        found: value_label(other),
                    });
                }
            };
            let transformed = runtime_binary_value(rotation, KernelValue::Vec3(normal), wr_rotate)?;
            expect_vec3(Some(&transformed), "transformed normal")
        }
        other => Err(QueryExecError::Unsupported {
            message: format!("certified normal transform does not support {other:?}"),
        }),
    }
}

pub(super) fn smooth_blended_normal(
    kind: SmoothKind,
    smoothing: f32,
    left_distance: f32,
    left_normal: [f32; 3],
    right_distance: f32,
    right_normal: [f32; 3],
) -> [f32; 3] {
    if smoothing <= 0.0 {
        return left_normal;
    }
    let h = (0.5 + 0.5 * (right_distance - left_distance) / smoothing).clamp(0.0, 1.0);
    let rhs = match kind {
        SmoothKind::Subtract => mul3_scalar(right_normal, -1.0),
        SmoothKind::Union | SmoothKind::Intersection => right_normal,
    };
    normalize3(add3(mul3_scalar(left_normal, h), mul3_scalar(rhs, 1.0 - h)))
}

pub(super) fn empty_support_bounds() -> SupportBounds {
    SupportBounds {
        min: [0.0, 0.0, 0.0],
        max: [0.0, 0.0, 0.0],
    }
}

pub(super) fn normalize_support_bounds(bounds: SupportBounds) -> SupportBounds {
    SupportBounds {
        min: [
            bounds.min[0].min(bounds.max[0]),
            bounds.min[1].min(bounds.max[1]),
            bounds.min[2].min(bounds.max[2]),
        ],
        max: [
            bounds.min[0].max(bounds.max[0]),
            bounds.min[1].max(bounds.max[1]),
            bounds.min[2].max(bounds.max[2]),
        ],
    }
}

pub(super) fn merge_union_support_bounds(lhs: SupportBounds, rhs: SupportBounds) -> SupportBounds {
    SupportBounds {
        min: [
            lhs.min[0].min(rhs.min[0]),
            lhs.min[1].min(rhs.min[1]),
            lhs.min[2].min(rhs.min[2]),
        ],
        max: [
            lhs.max[0].max(rhs.max[0]),
            lhs.max[1].max(rhs.max[1]),
            lhs.max[2].max(rhs.max[2]),
        ],
    }
}

pub(super) fn merge_intersection_support_bounds(
    lhs: SupportBounds,
    rhs: SupportBounds,
) -> SupportBounds {
    normalize_support_bounds(SupportBounds {
        min: [
            lhs.min[0].max(rhs.min[0]),
            lhs.min[1].max(rhs.min[1]),
            lhs.min[2].max(rhs.min[2]),
        ],
        max: [
            lhs.max[0].min(rhs.max[0]),
            lhs.max[1].min(rhs.max[1]),
            lhs.max[2].min(rhs.max[2]),
        ],
    })
}

pub(super) fn ray_support_interval_for_bounds(
    bounds: SupportBounds,
    origin: [f32; 3],
    direction: [f32; 3],
) -> RaySupportProbe {
    let mut start_t = f32::NEG_INFINITY;
    let mut end_t = f32::INFINITY;
    for axis in 0..3 {
        let direction_component = direction[axis];
        if direction_component.abs() <= f32::EPSILON {
            if origin[axis] < bounds.min[axis] || origin[axis] > bounds.max[axis] {
                return RaySupportProbe::Rejected;
            }
            continue;
        }
        let inv_direction = 1.0 / direction_component;
        let mut entry_t = (bounds.min[axis] - origin[axis]) * inv_direction;
        let mut exit_t = (bounds.max[axis] - origin[axis]) * inv_direction;
        if entry_t > exit_t {
            std::mem::swap(&mut entry_t, &mut exit_t);
        }
        start_t = start_t.max(entry_t);
        end_t = end_t.min(exit_t);
        if start_t > end_t {
            return RaySupportProbe::Rejected;
        }
    }
    RaySupportProbe::Interval(RaySupportInterval {
        start_t,
        end_t,
        starts_inside: start_t <= 0.0 && end_t >= 0.0,
        conservative: true,
    })
}

pub(super) fn ray_support_interval_for_sphere(
    center: [f32; 3],
    radius: f32,
    origin: [f32; 3],
    direction: [f32; 3],
) -> RaySupportProbe {
    let oc = [
        origin[0] - center[0],
        origin[1] - center[1],
        origin[2] - center[2],
    ];
    let a = dot3(direction, direction);
    let c = dot3(oc, oc) - radius * radius;
    if a <= f32::EPSILON {
        return if c <= 0.0 {
            RaySupportProbe::Interval(RaySupportInterval {
                start_t: f32::NEG_INFINITY,
                end_t: f32::INFINITY,
                starts_inside: true,
                conservative: true,
            })
        } else {
            RaySupportProbe::Rejected
        };
    }
    let b = dot3(oc, direction);
    let discriminant = b * b - a * c;
    if discriminant < 0.0 {
        return RaySupportProbe::Rejected;
    }
    let sqrt_discriminant = discriminant.sqrt();
    let mut start_t = (-b - sqrt_discriminant) / a;
    let mut end_t = (-b + sqrt_discriminant) / a;
    if start_t > end_t {
        std::mem::swap(&mut start_t, &mut end_t);
    }
    RaySupportProbe::Interval(RaySupportInterval {
        start_t,
        end_t,
        starts_inside: c <= 0.0,
        conservative: true,
    })
}

pub(super) fn ray_support_interval_for_periodic_bounds(
    bounds: SupportBounds,
    period: [f32; 3],
    origin: [f32; 3],
    direction: [f32; 3],
) -> RaySupportProbe {
    let bounds = normalize_support_bounds(bounds);
    let mut start_t = f32::NEG_INFINITY;
    for axis in 0..3 {
        let Some(axis_start_t) = periodic_axis_start_t(
            bounds.min[axis],
            bounds.max[axis],
            period[axis].abs(),
            origin[axis],
            direction[axis],
        ) else {
            return RaySupportProbe::Rejected;
        };
        start_t = start_t.max(axis_start_t);
    }
    RaySupportProbe::Interval(RaySupportInterval {
        start_t,
        end_t: f32::INFINITY,
        starts_inside: start_t <= 0.0,
        conservative: true,
    })
}

pub(super) fn periodic_axis_start_t(
    min: f32,
    max: f32,
    period: f32,
    origin: f32,
    direction: f32,
) -> Option<f32> {
    if period <= f32::EPSILON {
        return axis_interval_start_t(min, max, origin, direction);
    }
    let width = (max - min).abs();
    if width >= period - f32::EPSILON {
        return Some(f32::NEG_INFINITY);
    }
    let midpoint = (min + max) * 0.5;
    let local_origin = wrap_periodic_coordinate(origin, period, midpoint);
    if direction.abs() <= f32::EPSILON {
        return (local_origin >= min && local_origin <= max).then_some(f32::NEG_INFINITY);
    }
    if direction > 0.0 {
        if local_origin < min {
            Some((min - local_origin) / direction)
        } else if local_origin <= max {
            Some(f32::NEG_INFINITY)
        } else {
            Some((min + period - local_origin) / direction)
        }
    } else if local_origin > max {
        Some((max - local_origin) / direction)
    } else if local_origin >= min {
        Some(f32::NEG_INFINITY)
    } else {
        Some((max - period - local_origin) / direction)
    }
}

pub(super) fn wrap_periodic_coordinate(coord: f32, period: f32, midpoint: f32) -> f32 {
    coord - period * ((coord - midpoint) / period).round()
}

pub(super) fn axis_interval_start_t(
    min: f32,
    max: f32,
    origin: f32,
    direction: f32,
) -> Option<f32> {
    if direction.abs() <= f32::EPSILON {
        return (origin >= min && origin <= max).then_some(f32::NEG_INFINITY);
    }
    let mut start_t = (min - origin) / direction;
    let mut end_t = (max - origin) / direction;
    if start_t > end_t {
        std::mem::swap(&mut start_t, &mut end_t);
    }
    (start_t <= end_t).then_some(start_t)
}

pub(super) fn ray_support_interval_for_radial_repeat_bounds(
    bounds: SupportBounds,
    period: f32,
    origin: [f32; 3],
    direction: [f32; 3],
) -> RaySupportProbe {
    if period <= f32::EPSILON {
        return RaySupportProbe::Unavailable;
    }
    let bounds = normalize_support_bounds(bounds);
    let max_radius = support_bounds_corners(bounds)
        .iter()
        .map(|corner| (corner[0] * corner[0] + corner[2] * corner[2]).sqrt())
        .fold(0.0f32, f32::max);
    if max_radius <= f32::EPSILON {
        let collapsed = SupportBounds {
            min: [0.0, bounds.min[1], 0.0],
            max: [0.0, bounds.max[1], 0.0],
        };
        return ray_support_interval_for_bounds(collapsed, origin, direction);
    }
    ray_support_interval_for_bounds(
        SupportBounds {
            min: [-max_radius, bounds.min[1], -max_radius],
            max: [max_radius, bounds.max[1], max_radius],
        },
        origin,
        direction,
    )
}

pub(super) fn merge_union_support_probe(
    lhs: RaySupportProbe,
    rhs: RaySupportProbe,
) -> RaySupportProbe {
    match (lhs, rhs) {
        (RaySupportProbe::Unavailable, _) | (_, RaySupportProbe::Unavailable) => {
            RaySupportProbe::Unavailable
        }
        (RaySupportProbe::Rejected, probe) | (probe, RaySupportProbe::Rejected) => probe,
        (RaySupportProbe::Interval(lhs), RaySupportProbe::Interval(rhs)) => {
            RaySupportProbe::Interval(RaySupportInterval {
                start_t: lhs.start_t.min(rhs.start_t),
                end_t: lhs.end_t.max(rhs.end_t),
                starts_inside: lhs.starts_inside || rhs.starts_inside,
                conservative: lhs.conservative || rhs.conservative,
            })
        }
    }
}

pub(super) fn merge_intersection_support_probe(
    lhs: RaySupportProbe,
    rhs: RaySupportProbe,
) -> RaySupportProbe {
    match (lhs, rhs) {
        (RaySupportProbe::Rejected, _) | (_, RaySupportProbe::Rejected) => {
            RaySupportProbe::Rejected
        }
        (RaySupportProbe::Unavailable, _) | (_, RaySupportProbe::Unavailable) => {
            RaySupportProbe::Unavailable
        }
        (RaySupportProbe::Interval(lhs), RaySupportProbe::Interval(rhs)) => {
            let start_t = lhs.start_t.max(rhs.start_t);
            let end_t = lhs.end_t.min(rhs.end_t);
            if start_t > end_t {
                RaySupportProbe::Rejected
            } else {
                RaySupportProbe::Interval(RaySupportInterval {
                    start_t,
                    end_t,
                    starts_inside: lhs.starts_inside && rhs.starts_inside,
                    conservative: lhs.conservative || rhs.conservative,
                })
            }
        }
    }
}

pub(super) fn reflect_support_bounds(bounds: SupportBounds, normal: [f32; 3]) -> SupportBounds {
    let mut reflected = None;
    for corner in support_bounds_corners(bounds) {
        let point = reflect_point_across_plane(normal, corner);
        let point_bounds = SupportBounds {
            min: point,
            max: point,
        };
        reflected = Some(match reflected {
            Some(current) => merge_union_support_bounds(current, point_bounds),
            None => point_bounds,
        });
    }
    reflected.unwrap_or(bounds)
}

pub(super) fn transform_value_support_bounds(
    value: &KernelValue,
    bounds: SupportBounds,
) -> Result<Option<SupportBounds>, QueryExecError> {
    match value {
        KernelValue::Vec3(offset) => Ok(Some(SupportBounds {
            min: add3(bounds.min, *offset),
            max: add3(bounds.max, *offset),
        })),
        KernelValue::Struct(transform) if transform.name.as_str() == "Transform3" => {
            let matrix = expect_struct_mat4(transform, "matrix")?;
            let mut transformed = None;
            for corner in support_bounds_corners(bounds) {
                let point = mat4_mul_point(matrix, corner);
                let point_bounds = SupportBounds {
                    min: point,
                    max: point,
                };
                transformed = Some(match transformed {
                    Some(current) => merge_union_support_bounds(current, point_bounds),
                    None => point_bounds,
                });
            }
            Ok(transformed)
        }
        _ => Ok(None),
    }
}

pub(super) fn support_bounds_corners(bounds: SupportBounds) -> [[f32; 3]; 8] {
    [
        [bounds.min[0], bounds.min[1], bounds.min[2]],
        [bounds.min[0], bounds.min[1], bounds.max[2]],
        [bounds.min[0], bounds.max[1], bounds.min[2]],
        [bounds.min[0], bounds.max[1], bounds.max[2]],
        [bounds.max[0], bounds.min[1], bounds.min[2]],
        [bounds.max[0], bounds.min[1], bounds.max[2]],
        [bounds.max[0], bounds.max[1], bounds.min[2]],
        [bounds.max[0], bounds.max[1], bounds.max[2]],
    ]
}

pub(super) fn reflect_point_across_plane(normal: [f32; 3], point: [f32; 3]) -> [f32; 3] {
    let unit = normalize3(normal);
    let distance = dot3(point, unit);
    [
        point[0] - 2.0 * distance * unit[0],
        point[1] - 2.0 * distance * unit[1],
        point[2] - 2.0 * distance * unit[2],
    ]
}

pub(super) fn reflect_vector_across_plane(normal: [f32; 3], vector: [f32; 3]) -> [f32; 3] {
    let unit = normalize3(normal);
    let distance = dot3(vector, unit);
    [
        vector[0] - 2.0 * distance * unit[0],
        vector[1] - 2.0 * distance * unit[1],
        vector[2] - 2.0 * distance * unit[2],
    ]
}

pub(super) fn reflect_ray_across_plane(
    normal: [f32; 3],
    origin: [f32; 3],
    direction: [f32; 3],
) -> ([f32; 3], [f32; 3]) {
    (
        reflect_point_across_plane(normal, origin),
        reflect_vector_across_plane(normal, direction),
    )
}

pub(super) fn instance_array_local_ray(
    config: &KernelValue,
    origin: [f32; 3],
    direction: [f32; 3],
) -> Result<Option<([f32; 3], [f32; 3])>, QueryExecError> {
    match config {
        KernelValue::Vec3(translation) => Ok(Some((
            [
                origin[0] - translation[0],
                origin[1] - translation[1],
                origin[2] - translation[2],
            ],
            direction,
        ))),
        KernelValue::Struct(transform) if transform.name.as_str() == "Transform3" => {
            let inverse = expect_struct_mat4(transform, "inverse")?;
            Ok(Some((
                mat4_mul_point(inverse, origin),
                mat4_mul_vector(inverse, direction),
            )))
        }
        _ => Ok(None),
    }
}

pub(super) fn pure_translation_local_ray(
    config: &KernelValue,
    origin: [f32; 3],
    direction: [f32; 3],
) -> Result<Option<([f32; 3], [f32; 3])>, QueryExecError> {
    match config {
        KernelValue::Vec3(translation) => Ok(Some((
            [
                origin[0] - translation[0],
                origin[1] - translation[1],
                origin[2] - translation[2],
            ],
            direction,
        ))),
        KernelValue::Struct(transform) if transform.name.as_str() == "Transform3" => {
            let matrix = expect_struct_mat4(transform, "matrix")?;
            let inverse = expect_struct_mat4(transform, "inverse")?;
            if !transform3_is_pure_translation(matrix, inverse) {
                return Ok(None);
            }
            Ok(Some((
                [
                    origin[0] + inverse[12],
                    origin[1] + inverse[13],
                    origin[2] + inverse[14],
                ],
                direction,
            )))
        }
        _ => Ok(None),
    }
}

pub(super) fn transform3_is_pure_translation(matrix: [f32; 16], inverse: [f32; 16]) -> bool {
    const EPSILON: f32 = 1e-5;
    let identity_linear = [
        (0, 1.0),
        (1, 0.0),
        (2, 0.0),
        (3, 0.0),
        (4, 0.0),
        (5, 1.0),
        (6, 0.0),
        (7, 0.0),
        (8, 0.0),
        (9, 0.0),
        (10, 1.0),
        (11, 0.0),
        (15, 1.0),
    ];
    let linear_ok = identity_linear.iter().all(|(index, expected)| {
        (matrix[*index] - *expected).abs() <= EPSILON
            && (inverse[*index] - *expected).abs() <= EPSILON
    });
    let inverse_translation_ok = (matrix[12] + inverse[12]).abs() <= EPSILON
        && (matrix[13] + inverse[13]).abs() <= EPSILON
        && (matrix[14] + inverse[14]).abs() <= EPSILON;
    linear_ok && inverse_translation_ok
}

pub(super) fn mat4_mul_point(matrix: [f32; 16], point: [f32; 3]) -> [f32; 3] {
    let x = matrix[0] * point[0] + matrix[4] * point[1] + matrix[8] * point[2] + matrix[12];
    let y = matrix[1] * point[0] + matrix[5] * point[1] + matrix[9] * point[2] + matrix[13];
    let z = matrix[2] * point[0] + matrix[6] * point[1] + matrix[10] * point[2] + matrix[14];
    let w = matrix[3] * point[0] + matrix[7] * point[1] + matrix[11] * point[2] + matrix[15];
    if w.abs() > f32::EPSILON {
        [x / w, y / w, z / w]
    } else {
        [x, y, z]
    }
}

pub(super) fn mat4_mul_vector(matrix: [f32; 16], vector: [f32; 3]) -> [f32; 3] {
    [
        matrix[0] * vector[0] + matrix[4] * vector[1] + matrix[8] * vector[2],
        matrix[1] * vector[0] + matrix[5] * vector[1] + matrix[9] * vector[2],
        matrix[2] * vector[0] + matrix[6] * vector[1] + matrix[10] * vector[2],
    ]
}

pub(super) fn merge_world_support_summaries(items: &[SupportSummaryParts]) -> SupportSummaryParts {
    if items.is_empty() {
        return SupportSummaryParts {
            support_class: SupportClass::Unknown,
            semantics: DistanceSemantics::ConservativeLowerBound,
            has_bounds: false,
            opaque_boundary: false,
            can_coarse_support_prune: false,
            bounds: empty_support_bounds(),
        };
    }

    let support_class = if items
        .iter()
        .any(|item| matches!(item.support_class, SupportClass::Unbounded))
    {
        SupportClass::Unbounded
    } else if items
        .iter()
        .any(|item| matches!(item.support_class, SupportClass::Periodic))
    {
        SupportClass::Periodic
    } else if items
        .iter()
        .any(|item| matches!(item.support_class, SupportClass::Unknown))
    {
        SupportClass::Unknown
    } else {
        SupportClass::Bounded
    };
    let semantics = if items
        .iter()
        .any(|item| matches!(item.semantics, DistanceSemantics::UnknownOpaque))
    {
        DistanceSemantics::UnknownOpaque
    } else if items.len() == 1 {
        items[0].semantics
    } else {
        DistanceSemantics::ConservativeLowerBound
    };
    let has_bounds = items.iter().all(|item| item.has_bounds);
    let bounds = if has_bounds {
        items
            .iter()
            .map(|item| item.bounds)
            .reduce(merge_union_support_bounds)
            .unwrap_or_else(empty_support_bounds)
    } else {
        empty_support_bounds()
    };
    let opaque_boundary = items.iter().any(|item| item.opaque_boundary);
    let can_coarse_support_prune = !opaque_boundary
        && matches!(support_class, SupportClass::Bounded)
        && items.iter().all(|item| item.can_coarse_support_prune);
    SupportSummaryParts {
        support_class,
        semantics,
        has_bounds,
        opaque_boundary,
        can_coarse_support_prune,
        bounds,
    }
}

pub(super) fn support_class_code(class: SupportClass) -> u32 {
    match class {
        SupportClass::Unknown => 0,
        SupportClass::Bounded => 1,
        SupportClass::Periodic => 2,
        SupportClass::Unbounded => 3,
    }
}

pub(super) fn distance_semantics_code(semantics: DistanceSemantics) -> u32 {
    match semantics {
        DistanceSemantics::ExactSignedDistance => 0,
        DistanceSemantics::ConservativeLowerBound => 1,
        DistanceSemantics::UnknownOpaque => 2,
    }
}

pub(super) fn support_summary_value(summary: SupportSummaryParts) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("SupportSummaryResult"),
        fields: vec![
            (
                SmolStr::new("support_class"),
                KernelValue::U32(support_class_code(summary.support_class)),
            ),
            (
                SmolStr::new("semantics"),
                KernelValue::U32(distance_semantics_code(summary.semantics)),
            ),
            (
                SmolStr::new("has_bounds"),
                KernelValue::Bool(summary.has_bounds),
            ),
            (
                SmolStr::new("opaque_boundary"),
                KernelValue::Bool(summary.opaque_boundary),
            ),
            (
                SmolStr::new("can_coarse_support_prune"),
                KernelValue::Bool(summary.can_coarse_support_prune),
            ),
            (SmolStr::new("min"), KernelValue::Vec3(summary.bounds.min)),
            (SmolStr::new("max"), KernelValue::Vec3(summary.bounds.max)),
        ],
    })
}

pub(super) fn dot3(lhs: [f32; 3], rhs: [f32; 3]) -> f32 {
    lhs[0] * rhs[0] + lhs[1] * rhs[1] + lhs[2] * rhs[2]
}

pub(super) fn same_observability_certificate_metadata(
    lhs: &RayStepCertificateMetadata,
    rhs: &RayStepCertificateMetadata,
) -> bool {
    lhs.guarantee == rhs.guarantee
        && lhs.proof_family == rhs.proof_family
        && lhs.subject == rhs.subject
        && lhs.subject_kind == rhs.subject_kind
        && lhs.reusable_by == rhs.reusable_by
        && lhs.invalidation_reasons == rhs.invalidation_reasons
}

impl AnalyticPrimitiveRay {
    pub(super) fn solve(&self, start_t: f32, max_t: f32) -> Option<f32> {
        match self.primitive {
            AnalyticPrimitive::Sphere { radius } => solve_ray_sphere(
                self.local_origin,
                self.local_direction,
                [0.0, 0.0, 0.0],
                radius,
                start_t,
                max_t,
            ),
            AnalyticPrimitive::Plane { normal, offset } => solve_ray_plane(
                self.local_origin,
                self.local_direction,
                normal,
                offset,
                start_t,
                max_t,
            ),
            AnalyticPrimitive::Slab { thickness } => solve_ray_slab(
                self.local_origin,
                self.local_direction,
                thickness,
                start_t,
                max_t,
            ),
            AnalyticPrimitive::Box { half } => solve_ray_aabb(
                self.local_origin,
                self.local_direction,
                [-half[0].abs(), -half[1].abs(), -half[2].abs()],
                [half[0].abs(), half[1].abs(), half[2].abs()],
                start_t,
                max_t,
            ),
            AnalyticPrimitive::Capsule { a, b, radius } => solve_ray_capsule(
                self.local_origin,
                self.local_direction,
                a,
                b,
                radius,
                start_t,
                max_t,
            ),
            AnalyticPrimitive::Cylinder {
                radius,
                half_height,
            } => solve_ray_cylinder(
                self.local_origin,
                self.local_direction,
                radius,
                half_height,
                start_t,
                max_t,
            ),
        }
    }
}

pub(super) fn solve_ray_sphere(
    origin: [f32; 3],
    direction: [f32; 3],
    center: [f32; 3],
    radius: f32,
    start_t: f32,
    max_t: f32,
) -> Option<f32> {
    let oc = [
        origin[0] - center[0],
        origin[1] - center[1],
        origin[2] - center[2],
    ];
    let a = dot3(direction, direction);
    if a <= f32::EPSILON {
        return None;
    }
    let b = 2.0 * dot3(oc, direction);
    let c = dot3(oc, oc) - radius * radius;
    let discriminant = b * b - 4.0 * a * c;
    if discriminant < 0.0 {
        return None;
    }
    let root = discriminant.sqrt();
    let inv = 1.0 / (2.0 * a);
    select_first_valid_t([(-b - root) * inv, (-b + root) * inv], start_t, max_t)
}

pub(super) fn solve_ray_plane(
    origin: [f32; 3],
    direction: [f32; 3],
    normal: [f32; 3],
    offset: f32,
    start_t: f32,
    max_t: f32,
) -> Option<f32> {
    let normal_len = dot3(normal, normal).sqrt();
    if normal_len <= f32::EPSILON {
        return None;
    }
    let unit = [
        normal[0] / normal_len,
        normal[1] / normal_len,
        normal[2] / normal_len,
    ];
    let denom = dot3(unit, direction);
    if denom.abs() <= f32::EPSILON {
        return None;
    }
    let t = -(dot3(origin, unit) + offset) / denom;
    (t >= start_t && t <= max_t).then_some(t)
}

pub(super) fn solve_ray_slab(
    origin: [f32; 3],
    direction: [f32; 3],
    thickness: f32,
    start_t: f32,
    max_t: f32,
) -> Option<f32> {
    let half = thickness.abs() * 0.5;
    if direction[1].abs() <= f32::EPSILON {
        return None;
    }
    select_first_valid_t(
        [
            (half - origin[1]) / direction[1],
            (-half - origin[1]) / direction[1],
        ],
        start_t,
        max_t,
    )
}

pub(super) fn solve_ray_aabb(
    origin: [f32; 3],
    direction: [f32; 3],
    min: [f32; 3],
    max: [f32; 3],
    start_t: f32,
    max_t: f32,
) -> Option<f32> {
    let mut entry = f32::NEG_INFINITY;
    let mut exit = f32::INFINITY;
    for axis in 0..3 {
        if direction[axis].abs() <= f32::EPSILON {
            if origin[axis] < min[axis] || origin[axis] > max[axis] {
                return None;
            }
            continue;
        }
        let inv = 1.0 / direction[axis];
        let mut axis_entry = (min[axis] - origin[axis]) * inv;
        let mut axis_exit = (max[axis] - origin[axis]) * inv;
        if axis_entry > axis_exit {
            std::mem::swap(&mut axis_entry, &mut axis_exit);
        }
        entry = entry.max(axis_entry);
        exit = exit.min(axis_exit);
        if entry > exit {
            return None;
        }
    }
    let t = if entry >= start_t { entry } else { exit };
    (t >= start_t && t <= max_t).then_some(t)
}

pub(super) fn solve_ray_cylinder(
    origin: [f32; 3],
    direction: [f32; 3],
    radius: f32,
    half_height: f32,
    start_t: f32,
    max_t: f32,
) -> Option<f32> {
    let mut candidates = Vec::new();
    let a = direction[0] * direction[0] + direction[2] * direction[2];
    let b = origin[0] * direction[0] + origin[2] * direction[2];
    let c = origin[0] * origin[0] + origin[2] * origin[2] - radius * radius;
    if a > f32::EPSILON {
        let discriminant = b * b - a * c;
        if discriminant >= 0.0 {
            let root = discriminant.sqrt();
            for t in [(-b - root) / a, (-b + root) / a] {
                let y = origin[1] + direction[1] * t;
                if y.abs() <= half_height + 1e-4 {
                    candidates.push(t);
                }
            }
        }
    }
    if direction[1].abs() > f32::EPSILON {
        for y in [-half_height, half_height] {
            let t = (y - origin[1]) / direction[1];
            let x = origin[0] + direction[0] * t;
            let z = origin[2] + direction[2] * t;
            if x * x + z * z <= radius * radius + 1e-4 {
                candidates.push(t);
            }
        }
    }
    select_first_valid_t(candidates, start_t, max_t)
}

pub(super) fn solve_ray_capsule(
    origin: [f32; 3],
    direction: [f32; 3],
    a: [f32; 3],
    b: [f32; 3],
    radius: f32,
    start_t: f32,
    max_t: f32,
) -> Option<f32> {
    let ba = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let oa = [origin[0] - a[0], origin[1] - a[1], origin[2] - a[2]];
    let baba = dot3(ba, ba);
    let bard = dot3(ba, direction);
    let baoa = dot3(ba, oa);
    let rdoa = dot3(direction, oa);
    let oaoa = dot3(oa, oa);
    let a_coef = baba - bard * bard;
    let b_coef = baba * rdoa - baoa * bard;
    let c_coef = baba * oaoa - baoa * baoa - radius * radius * baba;
    let mut candidates = Vec::new();
    if a_coef.abs() > f32::EPSILON {
        let discriminant = b_coef * b_coef - a_coef * c_coef;
        if discriminant >= 0.0 {
            let root = discriminant.sqrt();
            for t in [(-b_coef - root) / a_coef, (-b_coef + root) / a_coef] {
                let y = baoa + t * bard;
                if y >= 0.0 && y <= baba {
                    candidates.push(t);
                }
            }
        }
    }
    candidates.extend(
        [
            solve_ray_sphere(origin, direction, a, radius, start_t, max_t),
            solve_ray_sphere(origin, direction, b, radius, start_t, max_t),
        ]
        .into_iter()
        .flatten(),
    );
    select_first_valid_t(candidates, start_t, max_t)
}

pub(super) fn select_first_valid_t(
    candidates: impl IntoIterator<Item = f32>,
    start_t: f32,
    max_t: f32,
) -> Option<f32> {
    candidates
        .into_iter()
        .filter(|t| t.is_finite() && *t >= start_t && *t <= max_t)
        .min_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal))
}

pub(super) fn ray_aabb_interval(
    origin: [f32; 3],
    direction: [f32; 3],
    min: [f32; 3],
    max: [f32; 3],
    start_t: f32,
    max_t: f32,
) -> Option<(f32, f32)> {
    let mut entry = start_t.max(0.0);
    let mut exit = max_t;
    for axis in 0..3 {
        if direction[axis].abs() <= f32::EPSILON {
            if origin[axis] < min[axis] || origin[axis] > max[axis] {
                return None;
            }
            continue;
        }
        let inv = 1.0 / direction[axis];
        let mut axis_entry = (min[axis] - origin[axis]) * inv;
        let mut axis_exit = (max[axis] - origin[axis]) * inv;
        if axis_entry > axis_exit {
            std::mem::swap(&mut axis_entry, &mut axis_exit);
        }
        entry = entry.max(axis_entry);
        exit = exit.min(axis_exit);
        if entry > exit {
            return None;
        }
    }
    (entry <= exit).then_some((entry, exit))
}

pub(super) fn axis_aligned_repeat_axis(period: [f32; 3]) -> Option<usize> {
    const AXIS_EPSILON: f32 = 1e-5;
    let active = (0..3)
        .filter(|axis| period[*axis].abs() > AXIS_EPSILON)
        .collect::<Vec<_>>();
    (active.len() == 1).then_some(active[0])
}

pub(super) fn axis_aligned_repeat_linear_cells(
    bounds: SupportBounds,
    axis: usize,
    period: f32,
    origin: [f32; 3],
    direction: [f32; 3],
    start_t: f32,
    max_t: f32,
) -> Vec<([f32; 3], f32, f32)> {
    const MAX_REPEAT_CELLS: i32 = 256;
    const SUPPORT_EPSILON: f32 = 1e-4;

    let period_abs = period.abs();
    if period_abs <= f32::EPSILON || axis > 2 {
        return Vec::new();
    }

    let width = (bounds.max[axis] - bounds.min[axis]).abs();
    if width > period_abs + SUPPORT_EPSILON {
        return Vec::new();
    }

    let axis_start = origin[axis] + direction[axis] * start_t;
    let axis_end = origin[axis] + direction[axis] * max_t;
    let ray_min = axis_start.min(axis_end);
    let ray_max = axis_start.max(axis_end);
    let cell_min = ((ray_min - bounds.max[axis]) / period_abs).floor() as i32 - 1;
    let cell_max = ((ray_max - bounds.min[axis]) / period_abs).ceil() as i32 + 1;
    if cell_max < cell_min || cell_max - cell_min > MAX_REPEAT_CELLS {
        return Vec::new();
    }

    let mut cells = Vec::new();
    for cell in cell_min..=cell_max {
        let offset_value = cell as f32 * period_abs;
        let mut offset = [0.0; 3];
        offset[axis] = offset_value;
        let mut shifted_min = bounds.min;
        let mut shifted_max = bounds.max;
        shifted_min[axis] += offset_value;
        shifted_max[axis] += offset_value;
        if let Some((entry_t, exit_t)) =
            ray_aabb_interval(origin, direction, shifted_min, shifted_max, start_t, max_t)
        {
            cells.push((offset, entry_t, exit_t));
        }
    }

    cells.sort_by(|left, right| {
        left.1
            .partial_cmp(&right.1)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    cells
}

pub(super) fn ray_parameter_lipschitz_bound(direction: [f32; 3]) -> f32 {
    dot3(direction, direction).sqrt().max(1e-4)
}

pub(super) fn relaxed_step_factor(previous_distance: Option<f32>, current_distance: f32) -> f32 {
    match previous_distance {
        Some(previous) if current_distance >= previous => 2.0,
        Some(previous) if current_distance >= previous * 0.5 => 1.5,
        Some(_) => 1.2,
        None => 1.25,
    }
}

pub(super) fn adaptive_hit_epsilon(base: f32, travel: f32, scale: f32) -> f32 {
    base.max(travel.abs() * 0.000_01)
        .max(scale.abs() * 0.000_001)
}

pub(super) fn adaptive_hit_epsilon_with_gradient(
    base: f32,
    travel: f32,
    scale: f32,
    gradient_mag: f32,
) -> f32 {
    let gradient_term = if gradient_mag > f32::EPSILON {
        1.0 / gradient_mag
    } else {
        2.0
    };
    adaptive_hit_epsilon(base, travel, scale)
        .max(base * (1.0 + scale.min(8.0) * 0.01 + gradient_term.min(4.0) * 0.02))
}

pub(super) fn support_box_lower_bound(
    min: [f32; 3],
    max: [f32; 3],
    point: [f32; 3],
) -> Result<f32, QueryExecError> {
    let center = [
        (min[0] + max[0]) * 0.5,
        (min[1] + max[1]) * 0.5,
        (min[2] + max[2]) * 0.5,
    ];
    let half = [
        (max[0] - min[0]).abs() * 0.5,
        (max[1] - min[1]).abs() * 0.5,
        (max[2] - min[2]).abs() * 0.5,
    ];
    runtime_binary_f32_from_values(
        KernelValue::Vec3([
            point[0] - center[0],
            point[1] - center[1],
            point[2] - center[2],
        ]),
        KernelValue::Vec3(half),
        wr_box,
    )
}

pub(super) fn support_sphere_lower_bound(center: [f32; 3], radius: f32, point: [f32; 3]) -> f32 {
    let dx = point[0] - center[0];
    let dy = point[1] - center[1];
    let dz = point[2] - center[2];
    (dx * dx + dy * dy + dz * dz).sqrt() - radius
}

pub(super) fn cross3(lhs: [f32; 3], rhs: [f32; 3]) -> [f32; 3] {
    [
        lhs[1] * rhs[2] - lhs[2] * rhs[1],
        lhs[2] * rhs[0] - lhs[0] * rhs[2],
        lhs[0] * rhs[1] - lhs[1] * rhs[0],
    ]
}

pub(crate) fn normalize3(value: [f32; 3]) -> [f32; 3] {
    let len = (value[0] * value[0] + value[1] * value[1] + value[2] * value[2]).sqrt();
    if len <= f32::EPSILON {
        [0.0, 0.0, 1.0]
    } else {
        [value[0] / len, value[1] / len, value[2] / len]
    }
}

pub(super) fn stable_surface_frame(position: [f32; 3], normal: [f32; 3]) -> KernelValue {
    let unit_normal = normalize3(normal);
    let world_up = [0.0, 1.0, 0.0];
    let world_right = [1.0, 0.0, 0.0];
    let tangent_seed = cross3(world_up, unit_normal);
    let tangent = if tangent_seed == [0.0, 0.0, 0.0] {
        normalize3(cross3(world_right, unit_normal))
    } else {
        normalize3(tangent_seed)
    };
    let bitangent = cross3(unit_normal, tangent);
    let inverse_translation = [
        -dot3(tangent, position),
        -dot3(bitangent, position),
        -dot3(unit_normal, position),
        1.0,
    ];
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("Transform3"),
        fields: vec![
            (
                SmolStr::new("matrix"),
                KernelValue::Mat4([
                    tangent[0],
                    tangent[1],
                    tangent[2],
                    0.0,
                    bitangent[0],
                    bitangent[1],
                    bitangent[2],
                    0.0,
                    unit_normal[0],
                    unit_normal[1],
                    unit_normal[2],
                    0.0,
                    position[0],
                    position[1],
                    position[2],
                    1.0,
                ]),
            ),
            (
                SmolStr::new("inverse"),
                KernelValue::Mat4([
                    tangent[0],
                    bitangent[0],
                    unit_normal[0],
                    0.0,
                    tangent[1],
                    bitangent[1],
                    unit_normal[1],
                    0.0,
                    tangent[2],
                    bitangent[2],
                    unit_normal[2],
                    0.0,
                    inverse_translation[0],
                    inverse_translation[1],
                    inverse_translation[2],
                    inverse_translation[3],
                ]),
            ),
        ],
    })
}

pub(super) fn length_of(value: &KernelValue) -> Result<f32, QueryExecError> {
    let components = kernel_components(value, "length")?;
    Ok((components
        .iter()
        .map(|component| component * component)
        .sum::<f32>())
    .sqrt())
}

pub(crate) fn combine_medium_values(
    current: KernelValue,
    next: KernelValue,
) -> Result<KernelValue, QueryExecError> {
    let current = expect_struct_ref(&current, "Medium")?;
    let next = expect_struct_ref(&next, "Medium")?;
    let current_density = expect_struct_f32(current, "density")?;
    let current_emission = expect_struct_vec3(current, "emission")?;
    let current_anisotropy = expect_struct_f32(current, "anisotropy")?;
    let next_density = expect_struct_f32(next, "density")?;
    let next_emission = expect_struct_vec3(next, "emission")?;
    let next_anisotropy = expect_struct_f32(next, "anisotropy")?;
    let density = current_density + next_density;
    let emission = add3(current_emission, next_emission);
    let anisotropy = if density > 0.0 {
        (current_anisotropy * current_density + next_anisotropy * next_density) / density
    } else {
        0.0
    };
    Ok(medium_value(density, emission, anisotropy))
}

pub(crate) fn kernel_to_runtime(value: &KernelValue) -> Result<RuntimeValue, QueryExecError> {
    match value {
        KernelValue::Nothing => Ok(RuntimeValue::nil()),
        KernelValue::Bool(value) => Ok(RuntimeValue::from_bool(*value)),
        KernelValue::I32(value) => Ok(RuntimeValue::from_int(*value as i64)),
        KernelValue::U32(value) => Ok(RuntimeValue::from_int(*value as i64)),
        KernelValue::F32(value) => Ok(RuntimeValue::from_float(*value as f64)),
        KernelValue::Vec2([x, y]) => Ok(wr_vec2_new(
            RuntimeValue::from_float(*x as f64),
            RuntimeValue::from_float(*y as f64),
        )),
        KernelValue::Vec3([x, y, z]) => Ok(wr_vec3_new(
            RuntimeValue::from_float(*x as f64),
            RuntimeValue::from_float(*y as f64),
            RuntimeValue::from_float(*z as f64),
        )),
        KernelValue::Vec4([x, y, z, w]) => Ok(wr_vec4_new(
            RuntimeValue::from_float(*x as f64),
            RuntimeValue::from_float(*y as f64),
            RuntimeValue::from_float(*z as f64),
            RuntimeValue::from_float(*w as f64),
        )),
        KernelValue::Quat([x, y, z, w]) => Ok(wr_quat_new(
            RuntimeValue::from_float(*x as f64),
            RuntimeValue::from_float(*y as f64),
            RuntimeValue::from_float(*z as f64),
            RuntimeValue::from_float(*w as f64),
        )),
        KernelValue::Mat3(values) => Ok(wr_mat3_from_columns(
            kernel_to_runtime(&KernelValue::Vec3([values[0], values[1], values[2]]))?,
            kernel_to_runtime(&KernelValue::Vec3([values[3], values[4], values[5]]))?,
            kernel_to_runtime(&KernelValue::Vec3([values[6], values[7], values[8]]))?,
        )),
        KernelValue::Mat4(values) => Ok(wr_mat4_from_columns(
            kernel_to_runtime(&KernelValue::Vec4([
                values[0], values[1], values[2], values[3],
            ]))?,
            kernel_to_runtime(&KernelValue::Vec4([
                values[4], values[5], values[6], values[7],
            ]))?,
            kernel_to_runtime(&KernelValue::Vec4([
                values[8], values[9], values[10], values[11],
            ]))?,
            kernel_to_runtime(&KernelValue::Vec4([
                values[12], values[13], values[14], values[15],
            ]))?,
        )),
        KernelValue::Struct(value) => kernel_struct_to_runtime(value),
        KernelValue::Array(items) => {
            let list = wr_list_new_local(0);
            for item in items {
                wr_list_push(list, kernel_to_runtime(item)?);
            }
            Ok(list)
        }
        KernelValue::Capture(_)
        | KernelValue::DispatchBackend(_)
        | KernelValue::GpuBuffer(_)
        | KernelValue::GpuAtomicI32(_)
        | KernelValue::GpuAtomicU32(_) => Err(QueryExecError::Unsupported {
            message: format!("cannot convert runtime math value from {value:?}"),
        }),
    }
}

pub(super) fn kernel_struct_to_runtime(
    value: &KernelStructValue,
) -> Result<RuntimeValue, QueryExecError> {
    let names = value
        .fields
        .iter()
        .map(|(name, _)| name.as_bytes().as_ptr())
        .collect::<Vec<_>>();
    let lens = value
        .fields
        .iter()
        .map(|(name, _)| name.len())
        .collect::<Vec<_>>();
    let obj = wr_class_new(
        TypeId::UserBase as u32,
        names.as_ptr(),
        lens.as_ptr(),
        names.len(),
    );
    for (index, (_, field_value)) in value.fields.iter().enumerate() {
        wr_class_set_slot(
            obj,
            std::ptr::null(),
            0,
            index,
            kernel_to_runtime(field_value)?,
        );
    }
    Ok(obj)
}

pub(crate) fn runtime_to_kernel_value(value: RuntimeValue) -> Result<KernelValue, QueryExecError> {
    match wr_type_id(value) as u32 {
        id if id == TypeId::Nil as u32 => Ok(KernelValue::Nothing),
        id if id == TypeId::Boolean as u32 => Ok(KernelValue::Bool(value.as_bool())),
        id if id == TypeId::Integer as u32 => Ok(KernelValue::I32(value.as_int() as i32)),
        id if id == TypeId::Float as u32 => Ok(KernelValue::F32(value.as_float() as f32)),
        id if id == TypeId::List as u32 => {
            let len = wr_list_len(value).as_int();
            let mut items = Vec::with_capacity(len.max(0) as usize);
            for index in 0..len {
                items.push(runtime_to_kernel_value(wr_list_get(value, index as usize))?);
            }
            Ok(KernelValue::Array(items))
        }
        id if id == TypeId::Vec2 as u32 => Ok(KernelValue::Vec2([
            component_as_f32(wr_vec_component(value, RuntimeValue::from_int(0)))?,
            component_as_f32(wr_vec_component(value, RuntimeValue::from_int(1)))?,
        ])),
        id if id == TypeId::Vec3 as u32 => Ok(KernelValue::Vec3([
            component_as_f32(wr_vec_component(value, RuntimeValue::from_int(0)))?,
            component_as_f32(wr_vec_component(value, RuntimeValue::from_int(1)))?,
            component_as_f32(wr_vec_component(value, RuntimeValue::from_int(2)))?,
        ])),
        id if id == TypeId::Vec4 as u32 => Ok(KernelValue::Vec4([
            component_as_f32(wr_vec_component(value, RuntimeValue::from_int(0)))?,
            component_as_f32(wr_vec_component(value, RuntimeValue::from_int(1)))?,
            component_as_f32(wr_vec_component(value, RuntimeValue::from_int(2)))?,
            component_as_f32(wr_vec_component(value, RuntimeValue::from_int(3)))?,
        ])),
        id if id == TypeId::Quat as u32 => Ok(KernelValue::Quat([
            component_as_f32(wr_vec_component(value, RuntimeValue::from_int(0)))?,
            component_as_f32(wr_vec_component(value, RuntimeValue::from_int(1)))?,
            component_as_f32(wr_vec_component(value, RuntimeValue::from_int(2)))?,
            component_as_f32(wr_vec_component(value, RuntimeValue::from_int(3)))?,
        ])),
        id if id == TypeId::Mat3 as u32 => runtime_to_kernel_mat3(value),
        id if id == TypeId::Mat4 as u32 => runtime_to_kernel_mat4(value),
        other => Err(QueryExecError::Unsupported {
            message: format!("runtime object conversion is not implemented for type id {other}"),
        }),
    }
}

pub(super) fn runtime_to_kernel_mat3(value: RuntimeValue) -> Result<KernelValue, QueryExecError> {
    Ok(KernelValue::Mat3([
        component_as_f32(wr_mat3_component(value, RuntimeValue::from_int(0)))?,
        component_as_f32(wr_mat3_component(value, RuntimeValue::from_int(1)))?,
        component_as_f32(wr_mat3_component(value, RuntimeValue::from_int(2)))?,
        component_as_f32(wr_mat3_component(value, RuntimeValue::from_int(3)))?,
        component_as_f32(wr_mat3_component(value, RuntimeValue::from_int(4)))?,
        component_as_f32(wr_mat3_component(value, RuntimeValue::from_int(5)))?,
        component_as_f32(wr_mat3_component(value, RuntimeValue::from_int(6)))?,
        component_as_f32(wr_mat3_component(value, RuntimeValue::from_int(7)))?,
        component_as_f32(wr_mat3_component(value, RuntimeValue::from_int(8)))?,
    ]))
}

pub(super) fn runtime_to_kernel_mat4(value: RuntimeValue) -> Result<KernelValue, QueryExecError> {
    Ok(KernelValue::Mat4([
        component_as_f32(wr_mat4_component(value, RuntimeValue::from_int(0)))?,
        component_as_f32(wr_mat4_component(value, RuntimeValue::from_int(1)))?,
        component_as_f32(wr_mat4_component(value, RuntimeValue::from_int(2)))?,
        component_as_f32(wr_mat4_component(value, RuntimeValue::from_int(3)))?,
        component_as_f32(wr_mat4_component(value, RuntimeValue::from_int(4)))?,
        component_as_f32(wr_mat4_component(value, RuntimeValue::from_int(5)))?,
        component_as_f32(wr_mat4_component(value, RuntimeValue::from_int(6)))?,
        component_as_f32(wr_mat4_component(value, RuntimeValue::from_int(7)))?,
        component_as_f32(wr_mat4_component(value, RuntimeValue::from_int(8)))?,
        component_as_f32(wr_mat4_component(value, RuntimeValue::from_int(9)))?,
        component_as_f32(wr_mat4_component(value, RuntimeValue::from_int(10)))?,
        component_as_f32(wr_mat4_component(value, RuntimeValue::from_int(11)))?,
        component_as_f32(wr_mat4_component(value, RuntimeValue::from_int(12)))?,
        component_as_f32(wr_mat4_component(value, RuntimeValue::from_int(13)))?,
        component_as_f32(wr_mat4_component(value, RuntimeValue::from_int(14)))?,
        component_as_f32(wr_mat4_component(value, RuntimeValue::from_int(15)))?,
    ]))
}

pub(super) fn component_as_f32(value: RuntimeValue) -> Result<f32, QueryExecError> {
    if value.is_float() {
        Ok(value.as_float() as f32)
    } else {
        Ok(value.as_int() as f32)
    }
}

pub(super) fn runtime_unary_builtin(
    args: &[KernelValue],
    f: extern "C" fn(RuntimeValue) -> RuntimeValue,
) -> Result<KernelValue, QueryExecError> {
    let [value] = args else {
        return Err(QueryExecError::Unsupported {
            message: "builtin expected one argument".to_string(),
        });
    };
    runtime_to_kernel_value(f(kernel_to_runtime(value)?))
}

pub(super) fn runtime_binary_builtin(
    args: &[KernelValue],
    f: extern "C" fn(RuntimeValue, RuntimeValue) -> RuntimeValue,
) -> Result<KernelValue, QueryExecError> {
    let [lhs, rhs] = args else {
        return Err(QueryExecError::Unsupported {
            message: "builtin expected two arguments".to_string(),
        });
    };
    runtime_to_kernel_value(f(kernel_to_runtime(lhs)?, kernel_to_runtime(rhs)?))
}

pub(super) fn runtime_ternary_builtin(
    args: &[KernelValue],
    f: extern "C" fn(RuntimeValue, RuntimeValue, RuntimeValue) -> RuntimeValue,
) -> Result<KernelValue, QueryExecError> {
    let [a, b, c] = args else {
        return Err(QueryExecError::Unsupported {
            message: "builtin expected three arguments".to_string(),
        });
    };
    runtime_to_kernel_value(f(
        kernel_to_runtime(a)?,
        kernel_to_runtime(b)?,
        kernel_to_runtime(c)?,
    ))
}

pub(super) fn runtime_binary_value(
    lhs: KernelValue,
    rhs: KernelValue,
    f: extern "C" fn(RuntimeValue, RuntimeValue) -> RuntimeValue,
) -> Result<KernelValue, QueryExecError> {
    runtime_to_kernel_value(f(kernel_to_runtime(&lhs)?, kernel_to_runtime(&rhs)?))
}

pub(super) fn runtime_binary_f32(
    lhs: f32,
    rhs: f32,
    f: extern "C" fn(RuntimeValue, RuntimeValue) -> RuntimeValue,
) -> Result<f32, QueryExecError> {
    match runtime_to_kernel_value(f(
        RuntimeValue::from_float(lhs as f64),
        RuntimeValue::from_float(rhs as f64),
    ))? {
        KernelValue::F32(value) => Ok(value),
        value => Err(QueryExecError::TypeMismatch {
            expected: "F32".to_string(),
            found: format!("{value:?}"),
        }),
    }
}

pub(super) fn runtime_binary_f32_from_values(
    lhs: KernelValue,
    rhs: KernelValue,
    f: extern "C" fn(RuntimeValue, RuntimeValue) -> RuntimeValue,
) -> Result<f32, QueryExecError> {
    match runtime_binary_value(lhs, rhs, f)? {
        KernelValue::F32(value) => Ok(value),
        value => Err(QueryExecError::TypeMismatch {
            expected: "F32".to_string(),
            found: format!("{value:?}"),
        }),
    }
}

pub(super) fn runtime_ternary_f32_from_values(
    a: KernelValue,
    b: KernelValue,
    c: KernelValue,
    f: extern "C" fn(RuntimeValue, RuntimeValue, RuntimeValue) -> RuntimeValue,
) -> Result<f32, QueryExecError> {
    match runtime_to_kernel_value(f(
        kernel_to_runtime(&a)?,
        kernel_to_runtime(&b)?,
        kernel_to_runtime(&c)?,
    ))? {
        KernelValue::F32(value) => Ok(value),
        value => Err(QueryExecError::TypeMismatch {
            expected: "F32".to_string(),
            found: format!("{value:?}"),
        }),
    }
}

pub(super) fn runtime_ternary_f32(
    a: f32,
    b: f32,
    c: f32,
    f: extern "C" fn(RuntimeValue, RuntimeValue, RuntimeValue) -> RuntimeValue,
) -> Result<f32, QueryExecError> {
    runtime_ternary_f32_from_values(
        KernelValue::F32(a),
        KernelValue::F32(b),
        KernelValue::F32(c),
        f,
    )
}

pub(super) fn runtime_quaternary_f32(
    a: KernelValue,
    b: KernelValue,
    c: KernelValue,
    d: KernelValue,
    f: extern "C" fn(RuntimeValue, RuntimeValue, RuntimeValue, RuntimeValue) -> RuntimeValue,
) -> Result<f32, QueryExecError> {
    match runtime_to_kernel_value(f(
        kernel_to_runtime(&a)?,
        kernel_to_runtime(&b)?,
        kernel_to_runtime(&c)?,
        kernel_to_runtime(&d)?,
    ))? {
        KernelValue::F32(value) => Ok(value),
        value => Err(QueryExecError::TypeMismatch {
            expected: "F32".to_string(),
            found: format!("{value:?}"),
        }),
    }
}

pub(super) fn expect_array<'a>(
    value: Option<&'a KernelValue>,
    label: &str,
) -> Result<&'a [KernelValue], QueryExecError> {
    match value {
        Some(KernelValue::Array(items)) => Ok(items.as_slice()),
        Some(other) => Err(QueryExecError::TypeMismatch {
            expected: format!("{label}: Array"),
            found: format!("{other:?}"),
        }),
        None => Err(QueryExecError::TypeMismatch {
            expected: format!("{label}: Array"),
            found: "missing value".to_string(),
        }),
    }
}

pub(super) fn expect_struct<'a>(
    value: Option<&'a KernelValue>,
    name: &str,
) -> Result<&'a KernelStructValue, QueryExecError> {
    match value {
        Some(KernelValue::Struct(value)) if value.name.as_str() == name => Ok(value),
        Some(other) => Err(QueryExecError::TypeMismatch {
            expected: format!("{name}"),
            found: format!("{other:?}"),
        }),
        None => Err(QueryExecError::TypeMismatch {
            expected: name.to_string(),
            found: "missing value".to_string(),
        }),
    }
}

pub(super) fn expect_struct_ref<'a>(
    value: &'a KernelValue,
    name: &str,
) -> Result<&'a KernelStructValue, QueryExecError> {
    expect_struct(Some(value), name)
}

pub(super) fn expect_vec3(
    value: Option<&KernelValue>,
    label: &str,
) -> Result<[f32; 3], QueryExecError> {
    match value {
        Some(KernelValue::Vec3(value)) => Ok(*value),
        Some(other) => Err(QueryExecError::TypeMismatch {
            expected: format!("{label}: Vec3"),
            found: format!("{other:?}"),
        }),
        None => Err(QueryExecError::TypeMismatch {
            expected: format!("{label}: Vec3"),
            found: "missing value".to_string(),
        }),
    }
}

pub(super) fn expect_f32(value: Option<&KernelValue>, label: &str) -> Result<f32, QueryExecError> {
    match value {
        Some(KernelValue::F32(value)) => Ok(*value),
        Some(KernelValue::I32(value)) => Ok(*value as f32),
        Some(KernelValue::U32(value)) => Ok(*value as f32),
        Some(other) => Err(QueryExecError::TypeMismatch {
            expected: format!("{label}: F32"),
            found: format!("{other:?}"),
        }),
        None => Err(QueryExecError::TypeMismatch {
            expected: format!("{label}: F32"),
            found: "missing value".to_string(),
        }),
    }
}

pub(super) fn expect_i32(value: Option<&KernelValue>, label: &str) -> Result<i32, QueryExecError> {
    match value {
        Some(KernelValue::I32(value)) => Ok(*value),
        Some(KernelValue::U32(value)) => Ok(*value as i32),
        Some(other) => Err(QueryExecError::TypeMismatch {
            expected: format!("{label}: I32"),
            found: format!("{other:?}"),
        }),
        None => Err(QueryExecError::TypeMismatch {
            expected: format!("{label}: I32"),
            found: "missing value".to_string(),
        }),
    }
}

pub(super) fn expect_abs_scalar(value: &KernelValue) -> Result<f32, QueryExecError> {
    match value {
        KernelValue::F32(value) => Ok(value.abs()),
        KernelValue::I32(value) => Ok((*value as f32).abs()),
        KernelValue::U32(value) => Ok(*value as f32),
        other => Err(QueryExecError::TypeMismatch {
            expected: "scalar".to_string(),
            found: format!("{other:?}"),
        }),
    }
}

pub(super) fn expect_struct_bool(
    value: &KernelStructValue,
    field: &str,
) -> Result<bool, QueryExecError> {
    match struct_field(value, field)? {
        KernelValue::Bool(value) => Ok(*value),
        other => Err(QueryExecError::TypeMismatch {
            expected: format!("{field}: Bool"),
            found: format!("{other:?}"),
        }),
    }
}

pub(super) fn expect_struct_f32(
    value: &KernelStructValue,
    field: &str,
) -> Result<f32, QueryExecError> {
    expect_f32(Some(struct_field(value, field)?), field)
}

pub(super) fn expect_struct_i32(
    value: &KernelStructValue,
    field: &str,
) -> Result<i32, QueryExecError> {
    expect_i32(Some(struct_field(value, field)?), field)
}

pub(super) fn expect_struct_u32(
    value: &KernelStructValue,
    field: &str,
) -> Result<u32, QueryExecError> {
    match struct_field(value, field)? {
        KernelValue::U32(value) => Ok(*value),
        KernelValue::I32(value) if *value >= 0 => Ok(*value as u32),
        other => Err(QueryExecError::TypeMismatch {
            expected: format!("{field}: U32"),
            found: format!("{other:?}"),
        }),
    }
}

pub(super) fn expect_struct_vec3(
    value: &KernelStructValue,
    field: &str,
) -> Result<[f32; 3], QueryExecError> {
    match struct_field(value, field)? {
        KernelValue::Vec3(value) => Ok(*value),
        other => Err(QueryExecError::TypeMismatch {
            expected: format!("{field}: Vec3"),
            found: format!("{other:?}"),
        }),
    }
}

pub(super) fn expect_struct_mat4(
    value: &KernelStructValue,
    field: &str,
) -> Result<[f32; 16], QueryExecError> {
    match struct_field(value, field)? {
        KernelValue::Mat4(value) => Ok(*value),
        other => Err(QueryExecError::TypeMismatch {
            expected: format!("{field}: Mat4"),
            found: format!("{other:?}"),
        }),
    }
}

pub(super) fn struct_field<'a>(
    value: &'a KernelStructValue,
    field: &str,
) -> Result<&'a KernelValue, QueryExecError> {
    value
        .fields
        .iter()
        .find(|(name, _)| name.as_str() == field)
        .map(|(_, value)| value)
        .ok_or_else(|| QueryExecError::Unsupported {
            message: format!("missing struct field '{field}' on '{}'", value.name),
        })
}

pub(super) fn expect_scalar_as_i32(
    args: &[KernelValue],
    name: &str,
) -> Result<i32, QueryExecError> {
    expect_i32(args.first(), name)
}

pub(super) fn expect_scalar_as_u32(
    args: &[KernelValue],
    name: &str,
) -> Result<u32, QueryExecError> {
    match args.first() {
        Some(KernelValue::U32(value)) => Ok(*value),
        Some(KernelValue::I32(value)) if *value >= 0 => Ok(*value as u32),
        Some(other) => Err(QueryExecError::TypeMismatch {
            expected: format!("{name}: U32"),
            found: format!("{other:?}"),
        }),
        None => Err(QueryExecError::TypeMismatch {
            expected: format!("{name}: U32"),
            found: "missing value".to_string(),
        }),
    }
}

pub(super) fn expect_scalar_as_f32(
    args: &[KernelValue],
    name: &str,
) -> Result<f32, QueryExecError> {
    expect_f32(args.first(), name)
}

pub(super) fn expect_scalar_as_f32_arg(
    args: &[KernelValue],
    index: usize,
    name: &str,
) -> Result<f32, QueryExecError> {
    expect_f32(args.get(index), name)
}
