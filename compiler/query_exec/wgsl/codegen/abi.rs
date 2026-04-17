//! Owns the portable ABI fragments used by WGSL direct-query code generation.
//! Does not own shader text emission or query-plan lowering.
//!
//! Key invariants:
//! - ABI layouts stay stable across emitters so storage-buffer bindings agree on
//!   shape, item, result, and dispatch records.
//! - descriptor-driven item/result ABIs must preserve the contract schema chosen
//!   before code generation.
//! - helper names exposed here are the canonical ones reused by sibling WGSL
//!   emission modules.
//!
//! Primary entrypoints:
//! - `wgsl_dispatch_config_abi`
//! - `wgsl_item_abi_for_descriptor`
//! - `wgsl_result_abi_for_descriptor`
//!
//! Failure modes / common pitfalls:
//! - changing field ordering here without updating bindings/emission helpers can
//!   corrupt every generated shader.
//! - hard-coding one contract's result layout would break the typed query model.

use super::*;

pub(crate) fn wgsl_dispatch_config_abi() -> PortableAbiType {
    PortableAbiType::Struct {
        name: SmolStr::new("WgslDispatchConfig"),
        class_id: 0,
        fields: vec![
            PortableStructField {
                name: SmolStr::new("capture_kind"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("capture_index"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("item_count"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("shape_count"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("accel_root_index"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("accel_node_count"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("cache_brick_count"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("material_enabled"),
                ty: PortableAbiType::Bool,
            },
            PortableStructField {
                name: SmolStr::new("radiance_enabled"),
                ty: PortableAbiType::Bool,
            },
            PortableStructField {
                name: SmolStr::new("media_enabled"),
                ty: PortableAbiType::Bool,
            },
            PortableStructField {
                name: SmolStr::new("candidate_spans_enabled"),
                ty: PortableAbiType::Bool,
            },
        ],
    }
}

pub(crate) fn wgsl_cache_brick_abi() -> PortableAbiType {
    PortableAbiType::Struct {
        name: SmolStr::new("WgslCacheBrick"),
        class_id: 0,
        fields: vec![
            PortableStructField {
                name: SmolStr::new("min"),
                ty: PortableAbiType::Vec3,
            },
            PortableStructField {
                name: SmolStr::new("max"),
                ty: PortableAbiType::Vec3,
            },
        ],
    }
}

pub(crate) fn wgsl_accel_node_abi() -> PortableAbiType {
    PortableAbiType::Struct {
        name: SmolStr::new("WgslAccelNode"),
        class_id: 0,
        fields: vec![
            PortableStructField {
                name: SmolStr::new("min"),
                ty: PortableAbiType::Vec3,
            },
            PortableStructField {
                name: SmolStr::new("max"),
                ty: PortableAbiType::Vec3,
            },
            PortableStructField {
                name: SmolStr::new("child_start"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("child_len"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("leaf_shape_index"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("flags"),
                ty: PortableAbiType::U32,
            },
        ],
    }
}

pub(crate) fn wgsl_shape_meta_abi() -> PortableAbiType {
    PortableAbiType::Struct {
        name: SmolStr::new("WgslShapeMeta"),
        class_id: 0,
        fields: vec![
            PortableStructField {
                name: SmolStr::new("root_shape_id"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("analytic_kind"),
                ty: PortableAbiType::U32,
            },
        ],
    }
}

pub(super) fn build_shape_meta_values(
    ctx: &QueryExecContext,
    _behavior: &NormalizedShaderBehavior,
    scene_index: &ShaderSceneIndex,
) -> Result<Vec<KernelValue>, QueryExecError> {
    let mut values = Vec::new();
    for shape_name in ctx.scene.shapes.keys() {
        let _ = scene_index.shape(shape_name)?;
        values.push(KernelValue::Struct(KernelStructValue {
            name: SmolStr::new("WgslShapeMeta"),
            fields: vec![
                (
                    SmolStr::new("root_shape_id"),
                    KernelValue::U32(ctx.shape_root_feature_id(shape_name)),
                ),
                (
                    SmolStr::new("analytic_kind"),
                    KernelValue::U32(analytic_shape_kind(ctx, shape_name)?),
                ),
            ],
        }));
    }
    if values.is_empty() {
        values.push(KernelValue::Struct(KernelStructValue {
            name: SmolStr::new("WgslShapeMeta"),
            fields: vec![
                (SmolStr::new("root_shape_id"), KernelValue::U32(0)),
                (SmolStr::new("analytic_kind"), KernelValue::U32(0)),
            ],
        }));
    }
    Ok(values)
}

pub(crate) fn wgsl_item_abi_for_descriptor(
    descriptor: &QueryContractDescriptor,
) -> Result<PortableAbiType, QueryExecError> {
    portable_query_item_abi(descriptor.item_kind).ok_or_else(|| QueryExecError::Unsupported {
        message: format!(
            "missing WGSL item ABI for query contract '{}'",
            descriptor.id.as_str()
        ),
    })
}

pub(crate) fn wgsl_result_abi_for_descriptor(
    descriptor: &QueryContractDescriptor,
) -> Result<PortableAbiType, QueryExecError> {
    portable_query_result_abi(descriptor.surface, descriptor.result_kind).ok_or_else(|| {
        QueryExecError::Unsupported {
            message: format!(
                "missing WGSL result ABI for query contract '{}'",
                descriptor.id.as_str()
            ),
        }
    })
}

pub(super) fn emit_value_and_abi_structs(
    ctx: &QueryExecContext,
    type_tags: &HashMap<SmolStr, TypeTagId>,
    extra_roots: &[PortableAbiType],
) -> Result<String, QueryExecError> {
    let mut value_roots = Vec::new();
    for record in all_builtin_records() {
        if let Some(abi) = portable_any_builtin_record_abi(record.name) {
            value_roots.push(abi);
        }
    }
    for (_idx, class) in ctx.module.classes.iter() {
        if class.role == hir::ClassRole::Value
            && let Some(abi) = portable_value_struct_abi(
                class.name.as_str(),
                &ctx.module,
                type_tags,
                &mut HashSet::new(),
            )
            && abi_supports_wgsl_value(&abi)
        {
            value_roots.push(abi);
        }
    }
    value_roots.extend(
        extra_roots
            .iter()
            .filter(|abi| abi_supports_wgsl_value(abi))
            .cloned(),
    );
    let value_structs = emit_value_structs(&value_roots)?;
    let mut abi_roots = value_roots
        .into_iter()
        .map(|abi| prefix_abi_names(&abi, "Abi_"))
        .collect::<Vec<_>>();
    abi_roots.push(prefix_abi_names(&PortableAbiType::U32, "Abi_"));
    let abi_structs =
        portable_abi_emit_wgsl_structs(&abi_roots).map_err(|err| QueryExecError::Unsupported {
            message: format!("failed to emit WGSL ABI structs: {err}"),
        })?;
    Ok(format!("{value_structs}\n\n{abi_structs}"))
}

pub(super) fn emit_value_structs(roots: &[PortableAbiType]) -> Result<String, QueryExecError> {
    let mut out = String::new();
    let mut seen = BTreeSet::new();
    for root in roots {
        emit_value_struct_recursive(root, &mut seen, &mut out)?;
    }
    Ok(out)
}

pub(super) fn emit_value_struct_recursive(
    abi: &PortableAbiType,
    seen: &mut BTreeSet<SmolStr>,
    out: &mut String,
) -> Result<(), QueryExecError> {
    match abi {
        PortableAbiType::Array(inner, _) => emit_value_struct_recursive(inner, seen, out),
        PortableAbiType::Struct { name, fields, .. } => {
            if seen.contains(name) {
                return Ok(());
            }
            for field in fields {
                emit_value_struct_recursive(&field.ty, seen, out)?;
            }
            seen.insert(name.clone());
            writeln!(out, "struct {name} {{").ok();
            if fields.is_empty() {
                writeln!(out, "  _unit: u32,").ok();
            } else {
                for field in fields {
                    writeln!(out, "  {}: {},", field.name, value_type_name(&field.ty)?).ok();
                }
            }
            writeln!(out, "}}\n").ok();
            Ok(())
        }
        PortableAbiType::Value => Err(QueryExecError::Unsupported {
            message: "WGSL value structs cannot transport runtime Value".to_string(),
        }),
        _ => Ok(()),
    }
}

pub(super) fn value_type_name(abi: &PortableAbiType) -> Result<String, QueryExecError> {
    match abi {
        PortableAbiType::Value => Err(QueryExecError::Unsupported {
            message: "WGSL value type does not support runtime Value".to_string(),
        }),
        PortableAbiType::Bool => Ok("bool".to_string()),
        PortableAbiType::I32 => Ok("i32".to_string()),
        PortableAbiType::U32 => Ok("u32".to_string()),
        PortableAbiType::F32 => Ok("f32".to_string()),
        PortableAbiType::Vec2 => Ok("vec2<f32>".to_string()),
        PortableAbiType::Vec3 => Ok("vec3<f32>".to_string()),
        PortableAbiType::Vec4 | PortableAbiType::Quat => Ok("vec4<f32>".to_string()),
        PortableAbiType::Mat3 => Ok("mat3x3<f32>".to_string()),
        PortableAbiType::Mat4 => Ok("mat4x4<f32>".to_string()),
        PortableAbiType::Array(inner, len) => {
            Ok(format!("array<{}, {}>", value_type_name(inner)?, len))
        }
        PortableAbiType::Struct { name, .. } => Ok(name.to_string()),
    }
}

pub(super) fn prefix_abi_names(abi: &PortableAbiType, prefix: &str) -> PortableAbiType {
    match abi {
        PortableAbiType::Array(inner, len) => {
            PortableAbiType::Array(Box::new(prefix_abi_names(inner, prefix)), *len)
        }
        PortableAbiType::Struct {
            name,
            class_id,
            fields,
        } => PortableAbiType::Struct {
            name: SmolStr::new(&format!("{prefix}{name}")),
            class_id: *class_id,
            fields: fields
                .iter()
                .map(|field| PortableStructField {
                    name: field.name.clone(),
                    ty: prefix_abi_names(&field.ty, prefix),
                })
                .collect(),
        },
        other => other.clone(),
    }
}

pub(super) fn emit_struct_conversions(
    ctx: &QueryExecContext,
    type_tags: &HashMap<SmolStr, TypeTagId>,
    extra_roots: &[PortableAbiType],
) -> Result<String, QueryExecError> {
    let mut roots = Vec::new();
    for record in all_builtin_records() {
        if let Some(abi) = portable_any_builtin_record_abi(record.name) {
            roots.push(abi);
        }
    }
    for (_idx, class) in ctx.module.classes.iter() {
        if class.role == hir::ClassRole::Value
            && let Some(abi) = portable_value_struct_abi(
                class.name.as_str(),
                &ctx.module,
                type_tags,
                &mut HashSet::new(),
            )
            && abi_supports_wgsl_value(&abi)
        {
            roots.push(abi);
        }
    }
    roots.extend(
        extra_roots
            .iter()
            .filter(|abi| abi_supports_wgsl_value(abi))
            .cloned(),
    );

    let mut out = String::new();
    let mut seen = BTreeSet::new();
    for root in &roots {
        emit_conversion_recursive(root, &mut seen, &mut out)?;
    }
    Ok(out)
}

pub(super) fn emit_conversion_recursive(
    abi: &PortableAbiType,
    seen: &mut BTreeSet<SmolStr>,
    out: &mut String,
) -> Result<(), QueryExecError> {
    match abi {
        PortableAbiType::Array(inner, _) => emit_conversion_recursive(inner, seen, out),
        PortableAbiType::Struct { name, fields, .. } => {
            if seen.contains(name) {
                return Ok(());
            }
            for field in fields {
                emit_conversion_recursive(&field.ty, seen, out)?;
            }
            seen.insert(name.clone());
            let abi_name = format!("Abi_{name}");
            writeln!(out, "fn to_{abi_name}(value: {name}) -> {abi_name} {{").ok();
            if fields.is_empty() {
                writeln!(out, "  _ = value;").ok();
                writeln!(out, "  return {abi_name}(0u);").ok();
            } else {
                write!(out, "  return {abi_name}(").ok();
                for (index, field) in fields.iter().enumerate() {
                    if index > 0 {
                        out.push_str(", ");
                    }
                    out.push_str(&to_abi_expr(&field.ty, &format!("value.{}", field.name))?);
                }
                out.push_str(");\n");
            }
            out.push_str("}\n\n");

            writeln!(out, "fn from_{abi_name}(value: {abi_name}) -> {name} {{").ok();
            if fields.is_empty() {
                writeln!(out, "  _ = value;").ok();
                writeln!(out, "  return {name}(0u);").ok();
            } else {
                write!(out, "  return {name}(").ok();
                for (index, field) in fields.iter().enumerate() {
                    if index > 0 {
                        out.push_str(", ");
                    }
                    out.push_str(&from_abi_expr(&field.ty, &format!("value.{}", field.name))?);
                }
                out.push_str(");\n");
            }
            out.push_str("}\n\n");
            Ok(())
        }
        PortableAbiType::Value => Err(QueryExecError::Unsupported {
            message: "WGSL conversion does not support runtime Value".to_string(),
        }),
        _ => Ok(()),
    }
}

pub(super) fn abi_supports_wgsl_value(abi: &PortableAbiType) -> bool {
    match abi {
        PortableAbiType::Value => false,
        PortableAbiType::Array(inner, _) => abi_supports_wgsl_value(inner),
        PortableAbiType::Struct { fields, .. } => fields
            .iter()
            .all(|field| abi_supports_wgsl_value(&field.ty)),
        PortableAbiType::Bool
        | PortableAbiType::I32
        | PortableAbiType::U32
        | PortableAbiType::F32
        | PortableAbiType::Vec2
        | PortableAbiType::Vec3
        | PortableAbiType::Vec4
        | PortableAbiType::Quat
        | PortableAbiType::Mat3
        | PortableAbiType::Mat4 => true,
    }
}

pub(super) fn to_abi_expr(abi: &PortableAbiType, expr: &str) -> Result<String, QueryExecError> {
    match abi {
        PortableAbiType::Bool => Ok(format!("select(0u, 1u, {expr})")),
        PortableAbiType::Array(inner, len) => Ok(format!(
            "array<{}, {}>({})",
            abi_type_name(inner, "Abi_")?,
            len,
            (0..*len)
                .map(|index| to_abi_expr(inner, &format!("{expr}[{index}]")))
                .collect::<Result<Vec<_>, _>>()?
                .join(", ")
        )),
        PortableAbiType::Struct { name, .. } => Ok(format!("to_Abi_{name}({expr})")),
        _ => Ok(expr.to_string()),
    }
}

pub(super) fn from_abi_expr(abi: &PortableAbiType, expr: &str) -> Result<String, QueryExecError> {
    match abi {
        PortableAbiType::Bool => Ok(format!("({expr} != 0u)")),
        PortableAbiType::Array(inner, len) => Ok(format!(
            "array<{}, {}>({})",
            value_type_name(inner)?,
            len,
            (0..*len)
                .map(|index| from_abi_expr(inner, &format!("{expr}[{index}]")))
                .collect::<Result<Vec<_>, _>>()?
                .join(", ")
        )),
        PortableAbiType::Struct { name, .. } => Ok(format!("from_Abi_{name}({expr})")),
        _ => Ok(expr.to_string()),
    }
}

pub(super) fn abi_type_name(abi: &PortableAbiType, prefix: &str) -> Result<String, QueryExecError> {
    match abi {
        PortableAbiType::Value => Err(QueryExecError::Unsupported {
            message: "WGSL ABI type does not support runtime Value".to_string(),
        }),
        PortableAbiType::Bool => Ok("u32".to_string()),
        PortableAbiType::I32 => Ok("i32".to_string()),
        PortableAbiType::U32 => Ok("u32".to_string()),
        PortableAbiType::F32 => Ok("f32".to_string()),
        PortableAbiType::Vec2 => Ok("vec2<f32>".to_string()),
        PortableAbiType::Vec3 => Ok("vec3<f32>".to_string()),
        PortableAbiType::Vec4 | PortableAbiType::Quat => Ok("vec4<f32>".to_string()),
        PortableAbiType::Mat3 => Ok("mat3x3<f32>".to_string()),
        PortableAbiType::Mat4 => Ok("mat4x4<f32>".to_string()),
        PortableAbiType::Array(inner, len) => {
            Ok(format!("array<{}, {}>", abi_type_name(inner, prefix)?, len))
        }
        PortableAbiType::Struct { name, .. } => Ok(format!("{prefix}{name}")),
    }
}
