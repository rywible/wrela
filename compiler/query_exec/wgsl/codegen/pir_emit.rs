//! Owns emission of portable IR functions/statements into WGSL source text.
//! Does not own portable IR construction or runtime buffer bindings.
//!
//! Key invariants:
//! - emitted temporaries and scratch slots preserve PIR evaluation order.
//! - PIR type rendering must stay aligned with the ABI/codegen type helpers used
//!   elsewhere in the WGSL backend.
//! - statement emission fails loudly on unsupported PIR constructs instead of
//!   inventing shader-side behavior.
//!
//! Primary entrypoints:
//! - `emit_pir_function`
//! - `emit_pir_stmt`
//! - `emit_pir_expr`
//!
//! Failure modes / common pitfalls:
//! - reordering side-effectful PIR expressions here can change query semantics.
//! - letting one emitter path bypass shared type naming makes generated shaders
//!   inconsistent and difficult to debug.

use super::*;

pub(super) fn emit_pir_function(
    function: &pir::ir::PirFunction,
    scratch: &mut usize,
    out: &mut String,
) -> Result<(), QueryExecError> {
    write!(out, "fn {}(", portable_function_name(&function.name)).ok();
    for (index, param) in function.params.iter().enumerate() {
        if index > 0 {
            out.push_str(", ");
        }
        write!(
            out,
            "{}: {}",
            sanitize_ident(&param.name),
            pir_type_name(&param.ty)?
        )
        .ok();
    }
    if matches!(function.ret, pir::ir::PirType::Nothing) {
        writeln!(out, ") {{").ok();
    } else {
        writeln!(out, ") -> {} {{", pir_type_name(&function.ret)?).ok();
    }
    emit_pir_block(&function.body, 1, scratch, out)?;
    writeln!(out, "}}").ok();
    Ok(())
}

pub(super) fn emit_pir_block(
    block: &[pir::ir::PirStmt],
    indent: usize,
    scratch: &mut usize,
    out: &mut String,
) -> Result<(), QueryExecError> {
    for stmt in block {
        emit_pir_stmt(stmt, indent, scratch, out)?;
    }
    Ok(())
}

pub(super) fn emit_pir_stmt(
    stmt: &pir::ir::PirStmt,
    indent: usize,
    scratch: &mut usize,
    out: &mut String,
) -> Result<(), QueryExecError> {
    let pad = "  ".repeat(indent);
    match stmt {
        pir::ir::PirStmt::Let {
            name,
            mutable,
            ty,
            value,
            ..
        } => {
            let keyword = if *mutable { "var" } else { "let" };
            writeln!(
                out,
                "{pad}{keyword} {}: {} = {};",
                sanitize_ident(name),
                pir_type_name(ty)?,
                emit_pir_expr(value)?
            )
            .ok();
        }
        pir::ir::PirStmt::Assign { name, value, .. } => {
            writeln!(
                out,
                "{pad}{} = {};",
                sanitize_ident(name),
                emit_pir_expr(value)?
            )
            .ok();
        }
        pir::ir::PirStmt::Expr { value, .. } => {
            if matches!(value.ty(), pir::ir::PirType::Nothing) {
                writeln!(out, "{pad}{};", emit_pir_expr(value)?).ok();
            } else {
                let temp = *scratch;
                *scratch += 1;
                writeln!(
                    out,
                    "{pad}let _expr_{temp}: {} = {};",
                    pir_type_name(value.ty())?,
                    emit_pir_expr(value)?
                )
                .ok();
            }
        }
        pir::ir::PirStmt::If {
            condition,
            then_block,
            else_block,
            ..
        } => {
            writeln!(out, "{pad}if ({}) {{", emit_pir_expr(condition)?).ok();
            emit_pir_block(then_block, indent + 1, scratch, out)?;
            if else_block.is_empty() {
                writeln!(out, "{pad}}}").ok();
            } else {
                writeln!(out, "{pad}}} else {{").ok();
                emit_pir_block(else_block, indent + 1, scratch, out)?;
                writeln!(out, "{pad}}}").ok();
            }
        }
        pir::ir::PirStmt::Return { value, .. } => match value {
            Some(value) => {
                writeln!(out, "{pad}return {};", emit_pir_expr(value)?).ok();
            }
            None => {
                writeln!(out, "{pad}return;").ok();
            }
        },
    }
    Ok(())
}

pub(super) fn emit_pir_expr(expr: &pir::ir::PirExpr) -> Result<String, QueryExecError> {
    match expr {
        pir::ir::PirExpr::Literal(value) => kernel_value_literal(&pir_value_to_kernel(value)?),
        pir::ir::PirExpr::Var { name, .. } => Ok(sanitize_ident(name)),
        pir::ir::PirExpr::Unary { op, expr, .. } => Ok(match op {
            hir::UnaryOp::Neg => format!("(-{})", emit_pir_expr(expr)?),
            hir::UnaryOp::Not => format!("(!{})", emit_pir_expr(expr)?),
            hir::UnaryOp::BitNot => format!("(~{})", emit_pir_expr(expr)?),
            other => {
                return Err(QueryExecError::Unsupported {
                    message: format!("WGSL portable lowering does not support unary op {other:?}"),
                });
            }
        }),
        pir::ir::PirExpr::Binary { op, lhs, rhs, .. } => {
            let symbol = pir_binary_op(*op);
            if symbol.is_empty() {
                return Err(QueryExecError::Unsupported {
                    message: format!("WGSL portable lowering does not support binary op {op:?}"),
                });
            }
            Ok(format!(
                "({} {} {})",
                emit_pir_expr(lhs)?,
                symbol,
                emit_pir_expr(rhs)?
            ))
        }
        pir::ir::PirExpr::Call { target, args, .. } => {
            let rendered_args = args
                .iter()
                .map(emit_pir_expr)
                .collect::<Result<Vec<_>, _>>()?
                .join(", ");
            Ok(match target {
                pir::ir::PirCallTarget::Function(name) => {
                    format!("{}({rendered_args})", portable_function_name(name))
                }
                pir::ir::PirCallTarget::Intrinsic(intrinsic) => {
                    emit_pir_intrinsic_call(*intrinsic, args)?
                }
            })
        }
        pir::ir::PirExpr::Member { base, member, .. } => Ok(format!(
            "{}.{}",
            emit_pir_expr(base)?,
            sanitize_ident(member)
        )),
        pir::ir::PirExpr::Index { base, index, .. } => Ok(format!(
            "{}[{}]",
            emit_pir_expr(base)?,
            emit_pir_expr(index)?
        )),
        pir::ir::PirExpr::ArrayLiteral { items, ty } => Ok(format!(
            "array<{}, {}>({})",
            pir_type_name(match ty {
                pir::ir::PirType::Array(inner, _) => inner,
                other => other,
            })?,
            items.len(),
            items
                .iter()
                .map(emit_pir_expr)
                .collect::<Result<Vec<_>, _>>()?
                .join(", ")
        )),
        pir::ir::PirExpr::StructLiteral { name, fields, .. } => Ok(format!(
            "{}({})",
            sanitize_ident(name),
            fields
                .iter()
                .map(|(_, value)| emit_pir_expr(value))
                .collect::<Result<Vec<_>, _>>()?
                .join(", ")
        )),
    }
}

pub(super) fn emit_pir_intrinsic_call(
    intrinsic: pir::ir::PirIntrinsic,
    args: &[pir::ir::PirExpr],
) -> Result<String, QueryExecError> {
    let rendered_args = args
        .iter()
        .map(emit_pir_expr)
        .collect::<Result<Vec<_>, _>>()?;
    let call = match intrinsic {
        pir::ir::PirIntrinsic::CastI32 => format!("i32({})", rendered_args[0]),
        pir::ir::PirIntrinsic::CastU32 => format!("u32({})", rendered_args[0]),
        pir::ir::PirIntrinsic::CastI64 | pir::ir::PirIntrinsic::CastU64 => {
            return Err(QueryExecError::Unsupported {
                message: format!("WGSL portable lowering does not support {intrinsic:?}"),
            });
        }
        pir::ir::PirIntrinsic::CastF32 => format!("f32({})", rendered_args[0]),
        pir::ir::PirIntrinsic::Vec2 => format!("vec2<f32>({})", rendered_args.join(", ")),
        pir::ir::PirIntrinsic::Vec3 => format!("vec3<f32>({})", rendered_args.join(", ")),
        pir::ir::PirIntrinsic::Vec4 | pir::ir::PirIntrinsic::Quat => {
            format!("vec4<f32>({})", rendered_args.join(", "))
        }
        pir::ir::PirIntrinsic::Mat3Identity => {
            "mat3x3<f32>(vec3<f32>(1.0, 0.0, 0.0), vec3<f32>(0.0, 1.0, 0.0), vec3<f32>(0.0, 0.0, 1.0))".to_string()
        }
        pir::ir::PirIntrinsic::Mat3Cols => format!("mat3x3<f32>({})", rendered_args.join(", ")),
        pir::ir::PirIntrinsic::Mat4Identity => {
            "mat4x4<f32>(vec4<f32>(1.0, 0.0, 0.0, 0.0), vec4<f32>(0.0, 1.0, 0.0, 0.0), vec4<f32>(0.0, 0.0, 1.0, 0.0), vec4<f32>(0.0, 0.0, 0.0, 1.0))".to_string()
        }
        pir::ir::PirIntrinsic::Mat4Cols => format!("mat4x4<f32>({})", rendered_args.join(", ")),
        pir::ir::PirIntrinsic::Bounds2Center => {
            render_bounds_expr(&rendered_args[0], args[0].ty(), true)?
        }
        pir::ir::PirIntrinsic::Bounds2Size => {
            render_bounds_expr(&rendered_args[0], args[0].ty(), false)?
        }
        pir::ir::PirIntrinsic::Bounds3Center => {
            render_bounds_expr(&rendered_args[0], args[0].ty(), true)?
        }
        pir::ir::PirIntrinsic::Bounds3Size => {
            render_bounds_expr(&rendered_args[0], args[0].ty(), false)?
        }
        pir::ir::PirIntrinsic::Transform3Identity => "wr_transform3_identity()".to_string(),
        pir::ir::PirIntrinsic::TransformPoint => format!("wr_transform_point({})", rendered_args.join(", ")),
        pir::ir::PirIntrinsic::TransformVector => format!("wr_transform_vector({})", rendered_args.join(", ")),
        pir::ir::PirIntrinsic::TransformNormal => format!("wr_transform_normal({})", rendered_args.join(", ")),
        pir::ir::PirIntrinsic::ComposeTransform3 => format!("wr_compose_transform3({})", rendered_args.join(", ")),
        pir::ir::PirIntrinsic::InverseTransform3 => format!("wr_inverse_transform3({})", rendered_args.join(", ")),
        pir::ir::PirIntrinsic::Translate => format!("wr_translate({})", rendered_args.join(", ")),
        pir::ir::PirIntrinsic::Rotate => render_rotate_call(args, &rendered_args)?,
        pir::ir::PirIntrinsic::UniformScale => format!("wr_uniform_scale({})", rendered_args.join(", ")),
        pir::ir::PirIntrinsic::AffineTransform => render_affine_transform_call(args, &rendered_args)?,
        pir::ir::PirIntrinsic::Warp => render_affine_transform_call(args, &rendered_args)?,
        pir::ir::PirIntrinsic::RepeatLinear => render_repeat_call("wr_repeat_linear", "wr_repeat_linear_scalar", args, &rendered_args)?,
        pir::ir::PirIntrinsic::RepeatGrid => render_repeat_call("wr_repeat_grid", "wr_repeat_grid_scalar", args, &rendered_args)?,
        pir::ir::PirIntrinsic::RadialRepeat => render_repeat_call("wr_radial_repeat", "wr_radial_repeat_scalar", args, &rendered_args)?,
        pir::ir::PirIntrinsic::MirrorArray => format!("wr_mirror_array({})", rendered_args.join(", ")),
        pir::ir::PirIntrinsic::InstanceArray => render_instance_array_call(args, &rendered_args)?,
        pir::ir::PirIntrinsic::FieldRotatePoint => render_rotate_call(args, &rendered_args)?,
        pir::ir::PirIntrinsic::FieldTransformPoint => render_affine_transform_call(args, &rendered_args)?,
        pir::ir::PirIntrinsic::FieldInstancePoint => render_instance_array_call(args, &rendered_args)?,
        pir::ir::PirIntrinsic::FieldMirrorPoint => format!("wr_field_mirror_point({})", rendered_args.join(", ")),
        pir::ir::PirIntrinsic::FieldRepeatPoint => render_field_repeat_call(args, &rendered_args)?,
        pir::ir::PirIntrinsic::FieldSweepCoords => format!("wr_field_sweep_coords({})", rendered_args.join(", ")),
        pir::ir::PirIntrinsic::RoundedBox => format!("wr_rounded_box({})", rendered_args.join(", ")),
        pir::ir::PirIntrinsic::Circle2 => format!("wr_circle2({})", rendered_args.join(", ")),
        pir::ir::PirIntrinsic::Rect2 => format!("wr_rect2({})", rendered_args.join(", ")),
        pir::ir::PirIntrinsic::RoundedRect2 => format!("wr_rounded_rect2({})", rendered_args.join(", ")),
        pir::ir::PirIntrinsic::Capsule2 => format!("wr_capsule2({})", rendered_args.join(", ")),
        pir::ir::PirIntrinsic::Segment2 => format!("wr_segment2({})", rendered_args.join(", ")),
        pir::ir::PirIntrinsic::Polygon2 | pir::ir::PirIntrinsic::Polyline2 => {
            return Err(QueryExecError::Unsupported {
                message: format!(
                    "WGSL portable lowering does not yet support variable-vertex {:?}",
                    intrinsic
                ),
            });
        }
        pir::ir::PirIntrinsic::Ellipsoid => format!("wr_ellipsoid({})", rendered_args.join(", ")),
        pir::ir::PirIntrinsic::Cone => format!("wr_cone({})", rendered_args.join(", ")),
        pir::ir::PirIntrinsic::CappedCone => format!("wr_capped_cone({})", rendered_args.join(", ")),
        pir::ir::PirIntrinsic::BoxFrame => format!("wr_box_frame({})", rendered_args.join(", ")),
        pir::ir::PirIntrinsic::Slab => format!("wr_slab({})", rendered_args.join(", ")),
        pir::ir::PirIntrinsic::TrianglePrism => format!("wr_triangle_prism({})", rendered_args.join(", ")),
        pir::ir::PirIntrinsic::HexPrism => format!("wr_hex_prism({})", rendered_args.join(", ")),
        pir::ir::PirIntrinsic::Sphere => format!("wr_sphere({})", rendered_args.join(", ")),
        pir::ir::PirIntrinsic::Box => format!("wr_box({})", rendered_args.join(", ")),
        pir::ir::PirIntrinsic::Capsule => format!("wr_capsule({})", rendered_args.join(", ")),
        pir::ir::PirIntrinsic::Cylinder => format!("wr_cylinder({})", rendered_args.join(", ")),
        pir::ir::PirIntrinsic::Plane => format!("wr_plane({})", rendered_args.join(", ")),
        pir::ir::PirIntrinsic::Torus => format!("wr_torus({})", rendered_args.join(", ")),
        pir::ir::PirIntrinsic::SmoothUnion => format!("wr_smooth_union({})", rendered_args.join(", ")),
        pir::ir::PirIntrinsic::SmoothIntersection => format!("wr_smooth_intersection({})", rendered_args.join(", ")),
        pir::ir::PirIntrinsic::SmoothSubtract => format!("wr_smooth_subtract({})", rendered_args.join(", ")),
        pir::ir::PirIntrinsic::FieldUnion => format!("wr_field_union({})", rendered_args.join(", ")),
        pir::ir::PirIntrinsic::FieldIntersection => format!("wr_field_intersection({})", rendered_args.join(", ")),
        pir::ir::PirIntrinsic::FieldSubtract => format!("wr_field_subtract({})", rendered_args.join(", ")),
        pir::ir::PirIntrinsic::Bend => format!("wr_bend({})", rendered_args.join(", ")),
        pir::ir::PirIntrinsic::Twist => format!("wr_twist({})", rendered_args.join(", ")),
        pir::ir::PirIntrinsic::Taper => format!("wr_taper({})", rendered_args.join(", ")),
        pir::ir::PirIntrinsic::Displace => format!("wr_displace({})", rendered_args.join(", ")),
        pir::ir::PirIntrinsic::Dot => format!("dot({})", rendered_args.join(", ")),
        pir::ir::PirIntrinsic::Length => format!("length({})", rendered_args.join(", ")),
        pir::ir::PirIntrinsic::Normalize => format!("normalize({})", rendered_args.join(", ")),
        pir::ir::PirIntrinsic::Cross => format!("cross({})", rendered_args.join(", ")),
        pir::ir::PirIntrinsic::Min => format!("min({})", rendered_args.join(", ")),
        pir::ir::PirIntrinsic::Max => format!("max({})", rendered_args.join(", ")),
        pir::ir::PirIntrinsic::Clamp => format!("clamp({})", rendered_args.join(", ")),
        pir::ir::PirIntrinsic::Mix => format!("mix({})", rendered_args.join(", ")),
        pir::ir::PirIntrinsic::Abs => format!("abs({})", rendered_args.join(", ")),
        pir::ir::PirIntrinsic::Sign => format!("sign({})", rendered_args.join(", ")),
        pir::ir::PirIntrinsic::Floor => format!("floor({})", rendered_args.join(", ")),
        pir::ir::PirIntrinsic::Ceil => format!("ceil({})", rendered_args.join(", ")),
        pir::ir::PirIntrinsic::Fract => format!("fract({})", rendered_args.join(", ")),
        pir::ir::PirIntrinsic::Sin => format!("sin({})", rendered_args.join(", ")),
        pir::ir::PirIntrinsic::Cos => format!("cos({})", rendered_args.join(", ")),
        pir::ir::PirIntrinsic::Sqrt => format!("sqrt({})", rendered_args.join(", ")),
        pir::ir::PirIntrinsic::Pow => format!("pow({})", rendered_args.join(", ")),
        pir::ir::PirIntrinsic::Distance => format!("distance({})", rendered_args.join(", ")),
        pir::ir::PirIntrinsic::Reflect => format!("reflect({})", rendered_args.join(", ")),
    };
    Ok(call)
}

pub(super) fn render_bounds_expr(
    arg: &str,
    ty: &pir::ir::PirType,
    center: bool,
) -> Result<String, QueryExecError> {
    let pir::ir::PirType::Struct(layout) = ty else {
        return Err(QueryExecError::Unsupported {
            message: format!("WGSL bounds lowering expected struct, found {ty:?}"),
        });
    };
    match layout.name.as_str() {
        "Bounds2" => Ok(if center {
            format!("wr_bounds2_center({arg}.min, {arg}.max)")
        } else {
            format!("wr_bounds2_size({arg}.min, {arg}.max)")
        }),
        "Bounds3" => Ok(if center {
            format!("wr_bounds3_center({arg}.min, {arg}.max)")
        } else {
            format!("wr_bounds3_size({arg}.min, {arg}.max)")
        }),
        other => Err(QueryExecError::Unsupported {
            message: format!("WGSL bounds lowering does not support {other}"),
        }),
    }
}

pub(super) fn render_rotate_call(
    args: &[pir::ir::PirExpr],
    rendered_args: &[String],
) -> Result<String, QueryExecError> {
    let helper = match args.first().map(pir::ir::PirExpr::ty) {
        Some(pir::ir::PirType::F32) => "wr_rotate_angle",
        Some(pir::ir::PirType::Vec3) => "wr_rotate_euler",
        Some(pir::ir::PirType::Quat | pir::ir::PirType::Vec4) => "wr_rotate_quat",
        Some(pir::ir::PirType::Mat3) => "wr_rotate_mat3",
        Some(pir::ir::PirType::Struct(layout)) if layout.name.as_str() == "Transform3" => {
            "wr_rotate_transform3"
        }
        other => {
            return Err(QueryExecError::Unsupported {
                message: format!("WGSL rotate lowering does not support {other:?}"),
            });
        }
    };
    Ok(format!("{helper}({})", rendered_args.join(", ")))
}

pub(super) fn render_affine_transform_call(
    args: &[pir::ir::PirExpr],
    rendered_args: &[String],
) -> Result<String, QueryExecError> {
    let helper = match args.first().map(pir::ir::PirExpr::ty) {
        Some(pir::ir::PirType::Vec3) => "wr_translate",
        Some(pir::ir::PirType::Struct(layout)) if layout.name.as_str() == "Transform3" => {
            "wr_affine_transform"
        }
        other => {
            return Err(QueryExecError::Unsupported {
                message: format!("WGSL affine transform lowering does not support {other:?}"),
            });
        }
    };
    Ok(format!("{helper}({})", rendered_args.join(", ")))
}

pub(super) fn render_repeat_call(
    vec_helper: &str,
    scalar_helper: &str,
    args: &[pir::ir::PirExpr],
    rendered_args: &[String],
) -> Result<String, QueryExecError> {
    let helper = match args.first().map(pir::ir::PirExpr::ty) {
        Some(pir::ir::PirType::F32) => scalar_helper,
        Some(pir::ir::PirType::Vec3) => vec_helper,
        other => {
            return Err(QueryExecError::Unsupported {
                message: format!("WGSL repeat lowering does not support {other:?}"),
            });
        }
    };
    Ok(format!("{helper}({})", rendered_args.join(", ")))
}

pub(super) fn render_instance_array_call(
    args: &[pir::ir::PirExpr],
    rendered_args: &[String],
) -> Result<String, QueryExecError> {
    let helper = match args.first().map(pir::ir::PirExpr::ty) {
        Some(pir::ir::PirType::Vec3) => "wr_instance_array_translation",
        Some(pir::ir::PirType::Struct(layout)) if layout.name.as_str() == "Transform3" => {
            "wr_instance_array"
        }
        other => {
            return Err(QueryExecError::Unsupported {
                message: format!("WGSL instance array lowering does not support {other:?}"),
            });
        }
    };
    Ok(format!("{helper}({})", rendered_args.join(", ")))
}

pub(super) fn render_field_repeat_call(
    args: &[pir::ir::PirExpr],
    rendered_args: &[String],
) -> Result<String, QueryExecError> {
    match args.first().map(pir::ir::PirExpr::ty) {
        Some(pir::ir::PirType::F32) => Ok(format!(
            "wr_repeat_point({}, wr_splat_period({}))",
            rendered_args[1], rendered_args[0]
        )),
        Some(pir::ir::PirType::Vec3) => Ok(format!(
            "wr_repeat_point({}, {})",
            rendered_args[1], rendered_args[0]
        )),
        other => Err(QueryExecError::Unsupported {
            message: format!("WGSL field repeat lowering does not support {other:?}"),
        }),
    }
}

pub(super) fn portable_function_name(name: &SmolStr) -> String {
    format!("wr_portable_{}", sanitize_ident(name))
}

pub(super) fn sanitize_ident(name: &SmolStr) -> String {
    let mut rendered = String::new();
    for (index, ch) in name.chars().enumerate() {
        if (index == 0 && (ch.is_ascii_alphabetic() || ch == '_'))
            || (index > 0 && (ch.is_ascii_alphanumeric() || ch == '_'))
        {
            rendered.push(ch);
        } else {
            rendered.push('_');
        }
    }
    if rendered.is_empty() {
        "_".to_string()
    } else {
        rendered
    }
}

pub(super) fn pir_type_name(ty: &pir::ir::PirType) -> Result<String, QueryExecError> {
    match ty {
        pir::ir::PirType::Nothing => Ok("void".to_string()),
        pir::ir::PirType::Bool => Ok("bool".to_string()),
        pir::ir::PirType::I32 => Ok("i32".to_string()),
        pir::ir::PirType::U32 => Ok("u32".to_string()),
        pir::ir::PirType::I64 | pir::ir::PirType::U64 => Err(QueryExecError::Unsupported {
            message: format!("WGSL portable lowering does not support {ty:?}"),
        }),
        pir::ir::PirType::F32 => Ok("f32".to_string()),
        pir::ir::PirType::Vec2 => Ok("vec2<f32>".to_string()),
        pir::ir::PirType::Vec3 => Ok("vec3<f32>".to_string()),
        pir::ir::PirType::Vec4 | pir::ir::PirType::Quat => Ok("vec4<f32>".to_string()),
        pir::ir::PirType::Mat3 => Ok("mat3x3<f32>".to_string()),
        pir::ir::PirType::Mat4 => Ok("mat4x4<f32>".to_string()),
        pir::ir::PirType::Array(inner, len) => {
            Ok(format!("array<{}, {}>", pir_type_name(inner)?, len))
        }
        pir::ir::PirType::Struct(layout) => Ok(sanitize_ident(&layout.name)),
    }
}

pub(super) fn pir_binary_op(op: hir::BinaryOp) -> &'static str {
    match op {
        hir::BinaryOp::Add => "+",
        hir::BinaryOp::Sub => "-",
        hir::BinaryOp::Mul => "*",
        hir::BinaryOp::Div => "/",
        hir::BinaryOp::Mod => "%",
        hir::BinaryOp::Eq => "==",
        hir::BinaryOp::Ne => "!=",
        hir::BinaryOp::Lt => "<",
        hir::BinaryOp::Gt => ">",
        hir::BinaryOp::Le => "<=",
        hir::BinaryOp::Ge => ">=",
        hir::BinaryOp::And => "&&",
        hir::BinaryOp::Or => "||",
        other => panic!("WGSL portable lowering does not support binary op {other:?}"),
    }
}

pub(super) fn pir_value_to_kernel(
    value: &pir::ir::PirValue,
) -> Result<KernelValue, QueryExecError> {
    Ok(match value {
        pir::ir::PirValue::Nothing => KernelValue::Nothing,
        pir::ir::PirValue::Bool(value) => KernelValue::Bool(*value),
        pir::ir::PirValue::I32(value) => KernelValue::I32(*value),
        pir::ir::PirValue::U32(value) => KernelValue::U32(*value),
        pir::ir::PirValue::I64(_) | pir::ir::PirValue::U64(_) => {
            return Err(QueryExecError::Unsupported {
                message: format!("WGSL portable lowering does not support {value:?}"),
            });
        }
        pir::ir::PirValue::F32(value) => KernelValue::F32(*value),
        pir::ir::PirValue::Vec2(value) => KernelValue::Vec2(*value),
        pir::ir::PirValue::Vec3(value) => KernelValue::Vec3(*value),
        pir::ir::PirValue::Vec4(value) => KernelValue::Vec4(*value),
        pir::ir::PirValue::Mat3(value) => KernelValue::Mat3(*value),
        pir::ir::PirValue::Mat4(value) => KernelValue::Mat4(*value),
        pir::ir::PirValue::Quat(value) => KernelValue::Quat(*value),
        pir::ir::PirValue::Array(values) => KernelValue::Array(
            values
                .iter()
                .map(pir_value_to_kernel)
                .collect::<Result<Vec<_>, _>>()?,
        ),
        pir::ir::PirValue::Struct(value) => KernelValue::Struct(crate::kernel::KernelStructValue {
            name: match &value.ty {
                pir::ir::PirType::Struct(layout) => layout.name.clone(),
                _ => SmolStr::new("Anonymous"),
            },
            fields: value
                .fields
                .iter()
                .map(|(name, value)| Ok((name.clone(), pir_value_to_kernel(value)?)))
                .collect::<Result<Vec<_>, QueryExecError>>()?,
        }),
    })
}
