//! Owns lowering authored HIR modules into MIR plus the helper bridges that keep
//! query, kernel, and portable ABI lowering aligned.
//! Does not own HIR parsing/typechecking, MIR consumers, or runtime execution.
//!
//! Key invariants:
//! - lowering must preserve authored meaning unless a deliberate, tested loss is
//!   surfaced as an explicit diagnostic.
//! - helper lowering for queries, kernels, and interfaces must agree on symbol
//!   identity and ABI layout.
//! - module-local rewrites may simplify MIR shape, but they must not invent new
//!   public semantics.
//!
//! Primary entrypoints:
//! - `lower_module`
//! - `lower_module_with_types`
//! - `lower_module_with_types_and_backend`
//!
//! Failure modes / common pitfalls:
//! - drifting helper naming or ABI layout between lowering paths can break
//!   backend equivalence far from the original authored site.
//! - lowering order matters when later passes expect declarations, captures, and
//!   helper synthesis to be present in a specific sequence.

use crate::hir::{
    self, AssignOp, BinaryOp, Expr, FunctionRole, FunctionTypeInfo, Literal, Module,
    Stmt as HirStmt, Type, TypeInfo, UnaryOp,
};
use crate::kernel::{
    KernelExpr, KernelStmt, ParsedKernelDispatch, lower_batch_query_plan, lower_kernel_function,
    lower_world_query_plan, parse_dispatch_compute as parse_kernel_dispatch_compute,
    validate_module as validate_kernel_module,
};
use crate::mir::ir::Stmt as MirStmt;
use crate::mir::ir::*;
use crate::portable::{
    PortableBuiltinType, all_builtin_records, any_builtin_record, builtin_record_by_function,
    portable_builtin_type_abi,
};
use crate::query_contract::{self, QueryItemKind};
use crate::query_exec::mir::{
    lower_field_batch_queries_helper, lower_scene_distance_capture_helper,
    lower_scene_medium_capture_helper, lower_scene_normal_capture_helper,
    lower_scene_occluded_capture_helper, lower_scene_radiance_capture_helper,
    lower_scene_support_summary_capture_helper, lower_scene_surface_capture_helper,
    lower_scene_surface_queries_helper, lower_scene_trace_capture_helper,
    lower_scene_trace_queries_helper, lower_shape_batch_queries_helper,
    lower_shape_distance_helper, lower_shape_surface_helper, lower_shape_trace_helper,
    lower_world_batch_queries_helper, lower_world_distance_capture_helper,
    lower_world_medium_capture_helper, lower_world_normal_capture_helper,
    lower_world_occluded_capture_helper, lower_world_radiance_capture_helper,
    lower_world_support_summary_capture_helper, lower_world_surface_capture_helper,
    lower_world_trace_capture_helper,
};
use crate::query_exec::{QueryExecContext, wgsl};
use crate::query_plan::{
    BatchQueryKind, BatchQueryPlan, CaptureKind, DispatchBackend, FieldBatchPlanKind,
    ShapeBatchPlanKind, WorldQueryKind, WorldQueryPlan,
};
use crate::scene_ir;
use rowan::TextRange;
use smol_str::SmolStr;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::env;

mod builders;
mod core;
mod expressions;
mod function_entry;
mod interface_dispatch;
mod kernel;
mod module_lower;
mod render_helpers;
mod statements;
#[cfg(test)]
mod tests;

pub(crate) use core::{FunctionLowerer, LoopTarget, ShapeExecutionMode};
pub use module_lower::{
    lower_module, lower_module_with_types, lower_module_with_types_and_backend,
};

fn compile_time_auto_pool_size(objective: i64, min: i64, max: i64, weight: i64) -> i64 {
    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1) as i64;
    let base = match objective {
        0 => cores,
        1 => cores.saturating_mul(2),
        2 => (cores / 2).max(1),
        _ => cores,
    };
    let min = if min > 0 { min } else { 1 };
    let max = if max > 0 { max } else { cores.max(1) };
    let weight = if weight > 0 { weight } else { 1 };
    base.saturating_mul(weight).clamp(min, max.max(min))
}

fn builtin_type_tag(name: &SmolStr) -> Option<TypeTagId> {
    match name.as_str() {
        "Integer" => Some(TypeTagId(1)),
        "Boolean" => Some(TypeTagId(2)),
        "Nothing" | "Nil" => Some(TypeTagId(3)),
        "Float" => Some(TypeTagId(4)),
        "String" => Some(TypeTagId(5)),
        "List" => Some(TypeTagId(6)),
        "Map" => Some(TypeTagId(7)),
        "Actor" => Some(TypeTagId(8)),
        "Pending" => Some(TypeTagId(9)),
        "Iterator" => Some(TypeTagId(10)),
        "Result" => Some(TypeTagId(11)),
        "Pool" => Some(TypeTagId(12)),
        "Bytes" => Some(TypeTagId(13)),
        "Mat3" => Some(TypeTagId(14)),
        "Quat" => Some(TypeTagId(15)),
        _ => None,
    }
}

pub(crate) struct PoolOfSpec {
    class_expr: hir::Idx<Expr>,
    size: Option<hir::PoolSize>,
    objective: Option<hir::Objective>,
    config: SpawnConfig,
    min_size: Option<i64>,
    max_size: Option<i64>,
    weight: Option<i64>,
    queue_cap: Option<i64>,
}

pub(crate) struct ClassTargetInfo {
    name: SmolStr,
    class_id: TypeTagId,
    fields: Vec<SmolStr>,
    field_defaults: Vec<Option<hir::FieldDefault>>,
    field_values: Vec<Option<Value>>,
}

fn pool_size_from_expr(body: &hir::Body, expr_id: hir::Idx<Expr>) -> Option<hir::PoolSize> {
    match &body.exprs[expr_id] {
        Expr::Literal(hir::Literal::Integer(value)) => Some(hir::PoolSize::Fixed(*value)),
        Expr::Variable(name) if name.as_str() == "n" => Some(hir::PoolSize::Auto),
        _ => None,
    }
}

fn objective_from_expr(body: &hir::Body, expr_id: hir::Idx<Expr>) -> Option<hir::Objective> {
    match &body.exprs[expr_id] {
        Expr::Variable(name) => hir::Objective::from_str(name.as_str()),
        _ => None,
    }
}

fn int_literal_from_expr(body: &hir::Body, expr_id: hir::Idx<Expr>) -> Option<i64> {
    match &body.exprs[expr_id] {
        Expr::Literal(hir::Literal::Integer(value)) => Some(*value),
        _ => None,
    }
}

struct BackpressureSpec {
    mailbox_cap: Option<i64>,
    enqueue_timeout_ms: Option<i64>,
    queue_cap: Option<i64>,
}

fn batch_limit_from_expr(body: &hir::Body, expr_id: hir::Idx<Expr>) -> Option<i64> {
    int_literal_from_expr(body, expr_id)
}

fn backpressure_from_expr(body: &hir::Body, expr_id: hir::Idx<Expr>) -> Option<BackpressureSpec> {
    match &body.exprs[expr_id] {
        Expr::Variable(name) if name.as_str() == "drop" => Some(BackpressureSpec {
            mailbox_cap: None,
            enqueue_timeout_ms: Some(0),
            queue_cap: Some(0),
        }),
        Expr::Call { callee, args, .. } => {
            let Expr::Variable(name) = &body.exprs[*callee] else {
                return None;
            };
            if name.as_str() != "queue" || args.len() != 1 {
                return None;
            }
            let arg = match &args[0] {
                hir::Arg::Positional { value, .. } => *value,
                hir::Arg::Named { value, .. } => *value,
            };
            match &body.exprs[arg] {
                Expr::Literal(hir::Literal::Integer(value)) => Some(BackpressureSpec {
                    mailbox_cap: Some(*value),
                    enqueue_timeout_ms: None,
                    queue_cap: Some(*value),
                }),
                _ => None,
            }
        }
        _ => None,
    }
}

fn objective_code(objective: hir::Objective) -> i64 {
    match objective {
        hir::Objective::Latency => 0,
        hir::Objective::Throughput => 1,
        hir::Objective::Conservation => 2,
        hir::Objective::Balance => 3,
    }
}

fn mir_type_from_type(ty: &Type) -> MirType {
    match ty {
        Type::Unknown => MirType::Unknown,
        Type::Integer | Type::I32 | Type::U32 | Type::I64 | Type::U64 => MirType::Integer,
        Type::Float | Type::F32 => MirType::Float,
        Type::Boolean => MirType::Boolean,
        Type::String => MirType::String,
        Type::Nil => MirType::Nil,
        Type::List(_) | Type::Array(_, _) => MirType::Named(SmolStr::new("List")),
        Type::Map(_, _) => MirType::Named(SmolStr::new("Map")),
        Type::Named(name, _) => MirType::Named(name.clone()),
        Type::Param(_) => MirType::Unknown,
        Type::Result(ok, err) => MirType::Result(
            Box::new(mir_type_from_type(ok)),
            Box::new(mir_type_from_type(err)),
        ),
        Type::Actor(inner) => MirType::Actor(Box::new(mir_type_from_type(inner))),
        Type::Pending(inner) => MirType::Pending(Box::new(mir_type_from_type(inner))),
        Type::Vec2 => MirType::Vec2,
        Type::Vec3 => MirType::Vec3,
        Type::Vec4 => MirType::Vec4,
        Type::Mat3 => MirType::Mat3,
        Type::Mat4 => MirType::Mat4,
        Type::Quat => MirType::Quat,
        Type::GpuBuffer(_) => MirType::Named(SmolStr::new("Buffer")),
        Type::GpuAtomicI32 => MirType::Named(SmolStr::new("GpuAtomicI32")),
        Type::GpuAtomicU32 => MirType::Named(SmolStr::new("GpuAtomicU32")),
        Type::GpuDispatchSchedule => MirType::Named(SmolStr::new("GpuDispatchSchedule")),
        Type::Texture2D => MirType::Named(SmolStr::new("Texture2D")),
        Type::Sampler => MirType::Named(SmolStr::new("Sampler")),
        _ => MirType::Unknown,
    }
}

pub(crate) fn portable_abi_from_type_ref(
    ty: Option<&crate::hir::TypeRef>,
    module: &hir::Module,
    type_tags: &HashMap<SmolStr, TypeTagId>,
    visiting: &mut HashSet<SmolStr>,
) -> PortableAbiType {
    let Some(ty) = ty else {
        return PortableAbiType::Value;
    };

    match ty.name.as_str() {
        "Bool" => PortableAbiType::Bool,
        "I32" => PortableAbiType::I32,
        "U32" => PortableAbiType::U32,
        "F32" => PortableAbiType::F32,
        "Vec2" => PortableAbiType::Vec2,
        "Vec3" => PortableAbiType::Vec3,
        "Vec4" => PortableAbiType::Vec4,
        "Mat3" => PortableAbiType::Mat3,
        "Mat4" => PortableAbiType::Mat4,
        "Quat" => PortableAbiType::Quat,
        "Array" => match ty.args.as_slice() {
            [inner, len] => len
                .name
                .parse::<usize>()
                .ok()
                .map(|len| {
                    PortableAbiType::Array(
                        Box::new(portable_abi_from_type_ref(
                            Some(inner),
                            module,
                            type_tags,
                            visiting,
                        )),
                        len,
                    )
                })
                .unwrap_or(PortableAbiType::Value),
            _ => PortableAbiType::Value,
        },
        name => portable_value_struct_abi(name, module, type_tags, visiting)
            .unwrap_or(PortableAbiType::Value),
    }
}

pub(crate) fn portable_value_struct_abi(
    name: &str,
    module: &hir::Module,
    type_tags: &HashMap<SmolStr, TypeTagId>,
    visiting: &mut HashSet<SmolStr>,
) -> Option<PortableAbiType> {
    let name = SmolStr::new(name);
    if !visiting.insert(name.clone()) {
        return None;
    }
    let Some(class_id) = type_tags.get(&name).map(|id| id.0 as u32) else {
        visiting.remove(&name);
        return None;
    };
    let fields = if let Some(record) = any_builtin_record(name.as_str()) {
        record
            .fields
            .iter()
            .map(|field| PortableStructField {
                name: SmolStr::new(field.name),
                ty: portable_builtin_abi_from_type(field.ty, module, type_tags, visiting),
            })
            .collect::<Vec<_>>()
    } else {
        let Some(class) = module.classes.iter().find_map(|(_, class)| {
            (class.name == name && matches!(class.role, hir::ClassRole::Value)).then_some(class)
        }) else {
            visiting.remove(&name);
            return None;
        };
        class
            .fields
            .iter()
            .map(|field| PortableStructField {
                name: field.name.clone(),
                ty: portable_abi_from_type_ref(field.ty.as_ref(), module, type_tags, visiting),
            })
            .collect::<Vec<_>>()
    };
    visiting.remove(&name);
    Some(PortableAbiType::Struct {
        name,
        class_id,
        fields,
    })
}

fn portable_builtin_abi_from_type(
    ty: PortableBuiltinType,
    module: &hir::Module,
    type_tags: &HashMap<SmolStr, TypeTagId>,
    visiting: &mut HashSet<SmolStr>,
) -> PortableAbiType {
    match ty {
        PortableBuiltinType::Named(name) => {
            portable_value_struct_abi(name, module, type_tags, visiting)
                .unwrap_or(PortableAbiType::Value)
        }
        _ => portable_builtin_type_abi(ty).unwrap_or(PortableAbiType::Value),
    }
}

fn scene_domain_compat_member(
    member: &str,
) -> Option<(&'static str, &'static str, &'static str, MirType)> {
    match member {
        "geometry_detail" => Some((
            "SpatialDomainContract",
            "spatial",
            "geometry_detail",
            MirType::Integer,
        )),
        "material" => Some((
            "SurfaceDomainContract",
            "surface",
            "material",
            MirType::Boolean,
        )),
        "radiance" => Some((
            "ParticipantDomainContract",
            "participants",
            "radiance",
            MirType::Boolean,
        )),
        "media" => Some((
            "ParticipantDomainContract",
            "participants",
            "media",
            MirType::Boolean,
        )),
        _ => None,
    }
}

pub(crate) fn vector_component_index(ty: MirType, member: &SmolStr) -> Option<usize> {
    match (ty, member.as_str()) {
        (MirType::Vec2, "x") => Some(0),
        (MirType::Vec2, "y") => Some(1),
        (MirType::Vec3, "x") => Some(0),
        (MirType::Vec3, "y") => Some(1),
        (MirType::Vec3, "z") => Some(2),
        (MirType::Vec4, "x") => Some(0),
        (MirType::Vec4, "y") => Some(1),
        (MirType::Vec4, "z") => Some(2),
        (MirType::Vec4, "w") => Some(3),
        (MirType::Quat, "x") => Some(0),
        (MirType::Quat, "y") => Some(1),
        (MirType::Quat, "z") => Some(2),
        (MirType::Quat, "w") => Some(3),
        _ => None,
    }
}
