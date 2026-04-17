//! Owns literal and constant-folded scene value emission for WGSL direct-query
//! code generation.
//! Does not own statement emission or runtime binding layout.
//!
//! Key invariants:
//! - literal emission preserves the kernel value semantics computed by the CPU
//!   oracle and scene-constant evaluator.
//! - bounds/material helpers fail loudly on mismatched value shapes instead of
//!   inventing shader defaults.
//! - generated literal syntax stays WGSL-valid for every scalar/vector path.
//!
//! Primary entrypoints:
//! - `scene_constant_literal`
//! - `kernel_value_literal`
//! - `bounds_center_half`
//!
//! Failure modes / common pitfalls:
//! - silently coercing mismatched kernel values here would hide authoring bugs
//!   behind shader generation.
//! - duplicating constant-folding logic outside this file makes CPU/WGSL literal
//!   semantics drift.

use super::*;

pub(super) fn scene_constant_literal(
    ops: &DirectQueryOps<'_>,
    expr: &SceneValueExpr,
) -> Result<String, QueryExecError> {
    let value = ops.eval_scene_constant(expr)?;
    kernel_value_literal(&value)
}

pub(super) fn bounds_center_half(
    value: &KernelValue,
) -> Result<([f32; 3], [f32; 3]), QueryExecError> {
    let KernelValue::Struct(bounds) = value else {
        return Err(QueryExecError::TypeMismatch {
            expected: "Bounds3".to_string(),
            found: format!("{value:?}"),
        });
    };
    let min = bounds
        .fields
        .iter()
        .find(|(name, _)| name.as_str() == "min")
        .and_then(|(_, value)| match value {
            KernelValue::Vec3(value) => Some(*value),
            _ => None,
        })
        .ok_or_else(|| QueryExecError::Unsupported {
            message: "Bounds3.min is missing".to_string(),
        })?;
    let max = bounds
        .fields
        .iter()
        .find(|(name, _)| name.as_str() == "max")
        .and_then(|(_, value)| match value {
            KernelValue::Vec3(value) => Some(*value),
            _ => None,
        })
        .ok_or_else(|| QueryExecError::Unsupported {
            message: "Bounds3.max is missing".to_string(),
        })?;
    Ok((
        [
            (min[0] + max[0]) * 0.5,
            (min[1] + max[1]) * 0.5,
            (min[2] + max[2]) * 0.5,
        ],
        [
            (max[0] - min[0]) * 0.5,
            (max[1] - min[1]) * 0.5,
            (max[2] - min[2]) * 0.5,
        ],
    ))
}

pub(super) fn abs_scalar_kernel_value(value: &KernelValue) -> Result<f32, QueryExecError> {
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

pub(super) fn kernel_value_length(value: &KernelValue) -> Result<f32, QueryExecError> {
    match value {
        KernelValue::Vec2(value) => Ok((value[0] * value[0] + value[1] * value[1]).sqrt()),
        KernelValue::Vec3(value) => {
            Ok((value[0] * value[0] + value[1] * value[1] + value[2] * value[2]).sqrt())
        }
        KernelValue::Vec4(value) | KernelValue::Quat(value) => Ok((value[0] * value[0]
            + value[1] * value[1]
            + value[2] * value[2]
            + value[3] * value[3])
            .sqrt()),
        other => Err(QueryExecError::TypeMismatch {
            expected: "vector".to_string(),
            found: format!("{other:?}"),
        }),
    }
}

pub(super) fn kernel_value_literal(value: &KernelValue) -> Result<String, QueryExecError> {
    Ok(match value {
        KernelValue::Nothing => "0.0".to_string(),
        KernelValue::Bool(value) => {
            if *value {
                "true".to_string()
            } else {
                "false".to_string()
            }
        }
        KernelValue::I32(value) => format!("{value}i"),
        KernelValue::U32(value) => format!("{value}u"),
        KernelValue::F32(value) => format_f32(*value),
        KernelValue::Vec2(value) => format!(
            "vec2<f32>({}, {})",
            format_f32(value[0]),
            format_f32(value[1])
        ),
        KernelValue::Vec3(value) => format!(
            "vec3<f32>({}, {}, {})",
            format_f32(value[0]),
            format_f32(value[1]),
            format_f32(value[2])
        ),
        KernelValue::Vec4(value) | KernelValue::Quat(value) => format!(
            "vec4<f32>({}, {}, {}, {})",
            format_f32(value[0]),
            format_f32(value[1]),
            format_f32(value[2]),
            format_f32(value[3])
        ),
        KernelValue::Mat3(value) => format!(
            "mat3x3<f32>(vec3<f32>({}, {}, {}), vec3<f32>({}, {}, {}), vec3<f32>({}, {}, {}))",
            format_f32(value[0]),
            format_f32(value[1]),
            format_f32(value[2]),
            format_f32(value[3]),
            format_f32(value[4]),
            format_f32(value[5]),
            format_f32(value[6]),
            format_f32(value[7]),
            format_f32(value[8]),
        ),
        KernelValue::Mat4(value) => format!(
            "mat4x4<f32>(vec4<f32>({}, {}, {}, {}), vec4<f32>({}, {}, {}, {}), vec4<f32>({}, {}, {}, {}), vec4<f32>({}, {}, {}, {}))",
            format_f32(value[0]),
            format_f32(value[1]),
            format_f32(value[2]),
            format_f32(value[3]),
            format_f32(value[4]),
            format_f32(value[5]),
            format_f32(value[6]),
            format_f32(value[7]),
            format_f32(value[8]),
            format_f32(value[9]),
            format_f32(value[10]),
            format_f32(value[11]),
            format_f32(value[12]),
            format_f32(value[13]),
            format_f32(value[14]),
            format_f32(value[15]),
        ),
        KernelValue::Array(items) => {
            let ty = items.first().ok_or_else(|| QueryExecError::Unsupported {
                message: "WGSL scene constants do not support empty arrays".to_string(),
            })?;
            format!(
                "array<{}, {}>({})",
                kernel_value_type_name(ty)?,
                items.len(),
                items
                    .iter()
                    .map(kernel_value_literal)
                    .collect::<Result<Vec<_>, _>>()?
                    .join(", ")
            )
        }
        KernelValue::Struct(value) => format!(
            "{}({})",
            sanitize_ident(&value.name),
            value
                .fields
                .iter()
                .map(|(_, value)| kernel_value_literal(value))
                .collect::<Result<Vec<_>, _>>()?
                .join(", ")
        ),
        KernelValue::Capture(_)
        | KernelValue::DispatchBackend(_)
        | KernelValue::GpuBuffer(_)
        | KernelValue::GpuAtomicI32(_)
        | KernelValue::GpuAtomicU32(_) => {
            return Err(QueryExecError::Unsupported {
                message: format!("WGSL scene constants do not support {value:?}"),
            });
        }
    })
}

pub(super) fn kernel_value_type_name(value: &KernelValue) -> Result<String, QueryExecError> {
    Ok(match value {
        KernelValue::Nothing => "f32".to_string(),
        KernelValue::Bool(_) => "bool".to_string(),
        KernelValue::I32(_) => "i32".to_string(),
        KernelValue::U32(_) => "u32".to_string(),
        KernelValue::F32(_) => "f32".to_string(),
        KernelValue::Vec2(_) => "vec2<f32>".to_string(),
        KernelValue::Vec3(_) => "vec3<f32>".to_string(),
        KernelValue::Vec4(_) | KernelValue::Quat(_) => "vec4<f32>".to_string(),
        KernelValue::Mat3(_) => "mat3x3<f32>".to_string(),
        KernelValue::Mat4(_) => "mat4x4<f32>".to_string(),
        KernelValue::Array(items) => {
            let first = items.first().ok_or_else(|| QueryExecError::Unsupported {
                message: "WGSL does not support inferring empty array element types".to_string(),
            })?;
            format!("array<{}, {}>", kernel_value_type_name(first)?, items.len())
        }
        KernelValue::Struct(value) => sanitize_ident(&value.name),
        KernelValue::Capture(_)
        | KernelValue::DispatchBackend(_)
        | KernelValue::GpuBuffer(_)
        | KernelValue::GpuAtomicI32(_)
        | KernelValue::GpuAtomicU32(_) => {
            return Err(QueryExecError::Unsupported {
                message: format!("WGSL scene constants do not support {value:?}"),
            });
        }
    })
}

pub(super) fn format_f32(value: f32) -> String {
    if value.is_nan() {
        "0.0".to_string()
    } else if value.is_infinite() {
        if value.is_sign_positive() {
            "1e30".to_string()
        } else {
            "-1e30".to_string()
        }
    } else {
        let mut rendered = format!("{value:?}");
        if !rendered.contains('.') && !rendered.contains('e') && !rendered.contains('E') {
            rendered.push_str(".0");
        }
        rendered
    }
}

pub(super) fn transform_helper_name_for_value(
    kind: TransformKind,
    value: &KernelValue,
) -> Result<&'static str, QueryExecError> {
    match kind {
        TransformKind::Translate => Ok("wr_translate"),
        TransformKind::Rotate => rotate_helper_name(value),
        TransformKind::UniformScale => Ok("wr_uniform_scale"),
        TransformKind::AffineTransform | TransformKind::Warp => match value {
            KernelValue::Vec3(_) => Ok("wr_translate"),
            KernelValue::Struct(value) if value.name.as_str() == "Transform3" => {
                Ok("wr_affine_transform")
            }
            other => Err(QueryExecError::Unsupported {
                message: format!(
                    "WGSL transform lowering does not support {:?} with parameter {other:?}",
                    kind
                ),
            }),
        },
        TransformKind::Bend => Ok("wr_bend"),
        TransformKind::Twist => Ok("wr_twist"),
        TransformKind::Taper => Ok("wr_taper"),
        TransformKind::Displace => Ok("wr_displace"),
    }
}

pub(super) fn repeat_helper_name_for_value(
    kind: RepeatKind,
    value: &KernelValue,
) -> Result<&'static str, QueryExecError> {
    match kind {
        RepeatKind::RepeatLinear => match value {
            KernelValue::F32(_) => Ok("wr_repeat_linear_scalar"),
            KernelValue::Vec3(_) => Ok("wr_repeat_linear"),
            other => Err(QueryExecError::Unsupported {
                message: format!("WGSL repeat_linear does not support parameter {other:?}"),
            }),
        },
        RepeatKind::RepeatGrid => match value {
            KernelValue::F32(_) => Ok("wr_repeat_grid_scalar"),
            KernelValue::Vec3(_) => Ok("wr_repeat_grid"),
            other => Err(QueryExecError::Unsupported {
                message: format!("WGSL repeat_grid does not support parameter {other:?}"),
            }),
        },
        RepeatKind::RadialRepeat => match value {
            KernelValue::F32(_) => Ok("wr_radial_repeat_scalar"),
            KernelValue::Vec3(_) => Ok("wr_radial_repeat"),
            other => Err(QueryExecError::Unsupported {
                message: format!("WGSL radial_repeat does not support parameter {other:?}"),
            }),
        },
        RepeatKind::MirrorArray => Ok("wr_mirror_array"),
        RepeatKind::InstanceArray => match value {
            KernelValue::Vec3(_) => Ok("wr_instance_array_translation"),
            KernelValue::Struct(value) if value.name.as_str() == "Transform3" => {
                Ok("wr_instance_array")
            }
            other => Err(QueryExecError::Unsupported {
                message: format!("WGSL instance_array does not support parameter {other:?}"),
            }),
        },
    }
}

pub(super) fn repeat_identity_helper_name_for_value(
    kind: RepeatKind,
    value: &KernelValue,
) -> Result<&'static str, QueryExecError> {
    match kind {
        RepeatKind::RepeatLinear => match value {
            KernelValue::F32(_) => Ok("wr_repeat_linear_identity_scalar"),
            KernelValue::Vec3(_) => Ok("wr_repeat_linear_identity"),
            other => Err(QueryExecError::Unsupported {
                message: format!("WGSL repeat_linear identity does not support {other:?}"),
            }),
        },
        RepeatKind::RepeatGrid => match value {
            KernelValue::F32(_) => Ok("wr_repeat_grid_identity_scalar"),
            KernelValue::Vec3(_) => Ok("wr_repeat_grid_identity"),
            other => Err(QueryExecError::Unsupported {
                message: format!("WGSL repeat_grid identity does not support {other:?}"),
            }),
        },
        RepeatKind::RadialRepeat => match value {
            KernelValue::F32(_) => Ok("wr_radial_repeat_identity_scalar"),
            KernelValue::Vec3(_) => Ok("wr_radial_repeat_identity"),
            other => Err(QueryExecError::Unsupported {
                message: format!("WGSL radial_repeat identity does not support {other:?}"),
            }),
        },
        RepeatKind::MirrorArray => Ok("wr_mirror_array_identity"),
        RepeatKind::InstanceArray => match value {
            KernelValue::Vec3(_) => Ok("wr_instance_array_identity_translation"),
            KernelValue::Struct(value) if value.name.as_str() == "Transform3" => {
                Ok("wr_instance_array_identity")
            }
            other => Err(QueryExecError::Unsupported {
                message: format!("WGSL instance_array identity does not support {other:?}"),
            }),
        },
    }
}

pub(super) fn rotate_helper_name(value: &KernelValue) -> Result<&'static str, QueryExecError> {
    match value {
        KernelValue::F32(_) => Ok("wr_rotate_angle"),
        KernelValue::Vec3(_) => Ok("wr_rotate_euler"),
        KernelValue::Quat(_) | KernelValue::Vec4(_) => Ok("wr_rotate_quat"),
        KernelValue::Mat3(_) => Ok("wr_rotate_mat3"),
        KernelValue::Struct(value) if value.name.as_str() == "Transform3" => {
            Ok("wr_rotate_transform3")
        }
        other => Err(QueryExecError::Unsupported {
            message: format!("WGSL rotate does not support parameter {other:?}"),
        }),
    }
}

pub(super) fn transform_normal_expr_for_value(
    kind: TransformKind,
    value: &KernelValue,
    rendered_value: &str,
    normal_expr: &str,
) -> Result<String, QueryExecError> {
    match kind {
        TransformKind::Translate | TransformKind::UniformScale => Ok(normal_expr.to_string()),
        TransformKind::Rotate => Ok(match value {
            KernelValue::F32(_) => format!("wr_rotate_angle(-({rendered_value}), {normal_expr})"),
            KernelValue::Vec3(_) => format!("wr_rotate_euler(-({rendered_value}), {normal_expr})"),
            KernelValue::Quat(_) | KernelValue::Vec4(_) => {
                format!("wr_rotate_vec3_by_quat({normal_expr}, {rendered_value})")
            }
            KernelValue::Mat3(_) => format!("({rendered_value} * {normal_expr})"),
            KernelValue::Struct(value) if value.name.as_str() == "Transform3" => {
                format!("wr_transform_vector({rendered_value}, {normal_expr})")
            }
            other => {
                return Err(QueryExecError::Unsupported {
                    message: format!("WGSL rotate normal lowering does not support {other:?}"),
                });
            }
        }),
        _ => Err(QueryExecError::Unsupported {
            message: format!("WGSL normal transform lowering does not support {:?}", kind),
        }),
    }
}

pub(super) fn emit_profile_expr(
    ops: &DirectQueryOps<'_>,
    profile: &SceneProfileExpr,
    point_expr: &str,
) -> Result<String, QueryExecError> {
    match profile {
        SceneProfileExpr::Primitive { primitive, args } => Ok(match primitive {
            hir::ProfilePrimitive::Circle2 => format!(
                "wr_circle2({}, {})",
                point_expr,
                scene_named_arg_literal(ops, args, "radius")?
            ),
            hir::ProfilePrimitive::Rect2 => format!(
                "wr_rect2({}, {})",
                point_expr,
                scene_named_arg_literal(ops, args, "half")?
            ),
            hir::ProfilePrimitive::RoundedRect2 => format!(
                "wr_rounded_rect2({}, {}, {})",
                point_expr,
                scene_named_arg_literal(ops, args, "half")?,
                scene_named_arg_literal(ops, args, "radius")?
            ),
            hir::ProfilePrimitive::Capsule2 => format!(
                "wr_capsule2({}, {}, {}, {})",
                point_expr,
                scene_named_arg_literal(ops, args, "a")?,
                scene_named_arg_literal(ops, args, "b")?,
                scene_named_arg_literal(ops, args, "radius")?
            ),
            hir::ProfilePrimitive::Segment2 => format!(
                "wr_segment2({}, {}, {})",
                point_expr,
                scene_named_arg_literal(ops, args, "a")?,
                scene_named_arg_literal(ops, args, "b")?
            ),
            hir::ProfilePrimitive::Polygon2 => format!(
                "wr_polygon2_n{}({}, {})",
                scene_value_list_len(scene_named_arg_value(args, "vertices")?)?,
                point_expr,
                scene_named_arg_literal(ops, args, "vertices")?
            ),
            hir::ProfilePrimitive::Polyline2 => format!(
                "wr_polyline2_n{}({}, {})",
                scene_value_list_len(scene_named_arg_value(args, "vertices")?)?,
                point_expr,
                scene_named_arg_literal(ops, args, "vertices")?
            ),
        }),
    }
}

pub(super) fn scene_named_arg_value<'a>(
    args: &'a [crate::scene_ir::SceneArgExpr],
    name: &str,
) -> Result<&'a SceneValueExpr, QueryExecError> {
    for arg in args {
        if let crate::scene_ir::SceneArgExpr::Named {
            name: arg_name,
            value,
        } = arg
            && arg_name.as_str() == name
        {
            return Ok(value);
        }
    }
    Err(QueryExecError::Unsupported {
        message: format!("missing scene argument '{name}'"),
    })
}

pub(super) fn scene_value_list_len(value: &SceneValueExpr) -> Result<usize, QueryExecError> {
    match value {
        SceneValueExpr::List(items) => Ok(items.len()),
        other => Err(QueryExecError::Unsupported {
            message: format!("expected scene list constant, found {other:?}"),
        }),
    }
}

pub(super) fn scene_named_arg_literal(
    ops: &DirectQueryOps<'_>,
    args: &[crate::scene_ir::SceneArgExpr],
    name: &str,
) -> Result<String, QueryExecError> {
    scene_constant_literal(ops, scene_named_arg_value(args, name)?)
}
