use crate::hir;
use crate::kernel::KernelValue;
use crate::kernel::ir::{KernelBatchQueryPlan, KernelCaptureQueryPlan, KernelWorldQueryPlan};
use crate::mir::ir::TypeTagId;
use crate::mir::lower::portable_value_struct_abi;
use crate::pir;
use crate::portable::{
    PortableAbiType, PortableStructField, all_builtin_records, portable_abi_emit_wgsl_structs,
    portable_any_builtin_record_abi, portable_query_item_abi, portable_query_result_abi,
};
use crate::query_contract::{
    self, QueryCardinality, QueryContractDescriptor, QueryExecutionBinding, QueryResultKind,
};
use crate::query_exec::QueryExecContext;
use crate::query_exec::cpu::{DirectQueryOps, QueryExecError};
use crate::query_plan::{NormalizedQueryBehavior, NormalizedQueryValuePath};
use crate::query_solver::{RaySolverPlan, ray_solver_method_name};
use crate::scene_ir::{
    FieldNodeKindSummary, FieldNodeRecord, RepeatKind, SceneOperatorPayload, SceneProfileExpr,
    SceneValueExpr, ShapeMergeProvenancePolicy, ShapeNodeKindSummary, ShapeNodeProvenancePolicy,
    ShapeSubtractProvenancePolicy, SmoothKind, TransformKind,
};
use smol_str::SmolStr;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt::Write;

const WORKGROUP_SIZE: u32 = 64;

#[derive(Debug, Clone)]
pub(crate) enum ShaderPlan<'a> {
    Capture(&'a KernelCaptureQueryPlan),
    World(&'a KernelWorldQueryPlan),
    Batch(&'a KernelBatchQueryPlan),
}

#[derive(Debug, Clone)]
pub(crate) struct GeneratedShader {
    pub(crate) source: String,
    pub(crate) workgroup_size: u32,
    pub(crate) dispatch_abi: PortableAbiType,
    pub(crate) item_abi: PortableAbiType,
    pub(crate) result_abi: PortableAbiType,
}

#[derive(Debug, Clone)]
struct NormalizedShaderBehavior {
    cardinality: QueryCardinality,
    result_kind: QueryResultKind,
    requires_material: bool,
    requires_radiance: bool,
    requires_volume: bool,
    requires_trace: bool,
    requires_root_shape_lookup: bool,
    value_path: NormalizedQueryValuePath,
    ray_solver: Option<RaySolverPlan>,
}

impl NormalizedShaderBehavior {
    fn from_plan(plan: ShaderPlan<'_>) -> Result<Self, QueryExecError> {
        let (normalized_behavior, ray_solver) = match plan {
            ShaderPlan::Capture(plan) => (plan.normalized_behavior.clone(), None),
            ShaderPlan::World(plan) => (plan.normalized_behavior.clone(), plan.ray_solver.clone()),
            ShaderPlan::Batch(plan) => (plan.normalized_behavior.clone(), plan.ray_solver.clone()),
        };
        let NormalizedQueryBehavior {
            cardinality,
            result_kind,
            requires_material,
            requires_radiance,
            requires_volume,
            requires_trace,
            requires_root_shape_lookup,
            value_path,
            ..
        } = normalized_behavior;
        Ok(Self {
            cardinality,
            result_kind,
            requires_material,
            requires_radiance,
            requires_volume,
            requires_trace,
            requires_root_shape_lookup,
            value_path,
            ray_solver,
        })
    }

    fn requires_trace(&self) -> bool {
        self.requires_trace
    }

    fn requires_root_shape_lookup(&self) -> bool {
        self.requires_root_shape_lookup
    }

    fn scalar_eval_expr(&self, item_expr: &str) -> String {
        match self.value_path {
            NormalizedQueryValuePath::SupportSummary => {
                panic!(
                    "WGSL portable lowering does not support support.summary normalized behavior"
                )
            }
            NormalizedQueryValuePath::CaptureDistance => {
                format!("capture_distance_point({item_expr})")
            }
            NormalizedQueryValuePath::CaptureNormal => {
                format!("capture_normal_point({item_expr})")
            }
            NormalizedQueryValuePath::CaptureTrace => format!("capture_trace_ray({item_expr})"),
            NormalizedQueryValuePath::CaptureOcclusion => {
                format!("wr_occlusion_result_from_hit(capture_trace_ray({item_expr}))")
            }
            NormalizedQueryValuePath::CaptureSurface => format!("capture_surface_hit({item_expr})"),
            NormalizedQueryValuePath::CaptureRadiance => {
                format!("capture_radiance_query({item_expr})")
            }
            NormalizedQueryValuePath::CaptureMedium => format!("capture_medium_point({item_expr})"),
            NormalizedQueryValuePath::WorldDistance => {
                format!("world_distance_point({item_expr}.point)")
            }
            NormalizedQueryValuePath::WorldNormal => {
                format!("world_normal_point({item_expr}.point)")
            }
            NormalizedQueryValuePath::WorldTrace => format!("world_trace_ray({item_expr})"),
            NormalizedQueryValuePath::WorldOcclusion => {
                format!("wr_occlusion_result_from_hit(world_trace_ray({item_expr}))")
            }
            NormalizedQueryValuePath::WorldSurface => format!("world_surface_hit({item_expr})"),
            NormalizedQueryValuePath::WorldRadiance => format!("world_radiance_query({item_expr})"),
            NormalizedQueryValuePath::WorldMedium => format!("world_medium_point({item_expr})"),
        }
    }

    fn batch_eval_expr(&self, item_expr: &str) -> String {
        match self.result_kind {
            QueryResultKind::DistanceResult => {
                format!("DistanceResult({})", self.scalar_eval_expr(item_expr))
            }
            QueryResultKind::NormalResult => {
                format!("NormalResult({})", self.scalar_eval_expr(item_expr))
            }
            _ => self.scalar_eval_expr(item_expr),
        }
    }
}

#[derive(Debug, Clone)]
struct ShaderSceneIndex {
    field_indices: BTreeMap<SmolStr, u32>,
    shape_indices: BTreeMap<SmolStr, u32>,
}

impl ShaderSceneIndex {
    fn new(ctx: &QueryExecContext) -> Self {
        let field_indices = ctx
            .scene
            .fields
            .keys()
            .enumerate()
            .map(|(index, name)| (name.clone(), index as u32))
            .collect();
        let shape_indices = ctx
            .scene
            .shapes
            .keys()
            .enumerate()
            .map(|(index, name)| (name.clone(), index as u32))
            .collect();
        Self {
            field_indices,
            shape_indices,
        }
    }

    fn field(&self, name: &SmolStr) -> Result<u32, QueryExecError> {
        self.field_indices
            .get(name)
            .copied()
            .ok_or_else(|| QueryExecError::MissingField { name: name.clone() })
    }

    fn shape(&self, name: &SmolStr) -> Result<u32, QueryExecError> {
        self.shape_indices
            .get(name)
            .copied()
            .ok_or_else(|| QueryExecError::MissingShape { name: name.clone() })
    }
}

pub(crate) fn generate_shader(
    ctx: &QueryExecContext,
    plan: ShaderPlan<'_>,
) -> Result<GeneratedShader, QueryExecError> {
    let (descriptor, _binding) = shader_contract(plan.clone())?;
    let behavior = NormalizedShaderBehavior::from_plan(plan)?;

    let type_tags = build_type_tags(ctx);
    let dispatch_abi = wgsl_dispatch_config_abi();
    let item_abi = wgsl_item_abi_for_descriptor(descriptor)?;
    let result_abi = wgsl_result_abi_for_descriptor(descriptor)?;
    let scene_index = ShaderSceneIndex::new(ctx);
    let mut rendered = String::new();

    rendered.push_str("// Generated by wr query_exec::wgsl\n");
    if matches!(
        behavior.value_path,
        NormalizedQueryValuePath::WorldTrace | NormalizedQueryValuePath::WorldOcclusion
    ) {
        rendered.push_str("// ray_solver: generated_dense_fallback\n");
        rendered.push_str("const WR_RAY_SOLVER_GENERATED_DENSE_FALLBACK: u32 = 1u;\n");
        if let Some(solver) = &behavior.ray_solver {
            let methods = solver
                .diagnostic_summary()
                .methods
                .iter()
                .map(|method| ray_solver_method_name(*method))
                .collect::<Vec<_>>()
                .join("|");
            rendered.push_str(&format!("// ray_solver_methods={methods}\n"));
        }
    }
    rendered.push_str("override WG_SIZE: u32 = 64u;\n\n");
    rendered.push_str(&emit_value_and_abi_structs(
        ctx,
        &type_tags,
        &[dispatch_abi.clone(), item_abi.clone(), result_abi.clone()],
    )?);
    rendered.push('\n');
    rendered.push_str(WGSL_PRELUDE);
    rendered.push('\n');
    rendered.push_str(&emit_normal_sample_support()?);
    rendered.push('\n');
    rendered.push_str(&emit_struct_conversions(
        ctx,
        &type_tags,
        &[dispatch_abi.clone(), item_abi.clone(), result_abi.clone()],
    )?);
    rendered.push('\n');
    rendered.push_str(&emit_scene_functions(ctx, &scene_index, &behavior)?);
    rendered.push('\n');
    rendered.push_str(&emit_portable_functions(ctx, &behavior)?);
    rendered.push('\n');
    rendered.push_str(&emit_bindings(&dispatch_abi, &item_abi, &result_abi)?);
    rendered.push('\n');
    rendered.push_str(&emit_query_helpers(ctx, &scene_index, &behavior)?);
    rendered.push('\n');
    rendered.push_str(&emit_main(&behavior, &item_abi, &result_abi)?);

    Ok(GeneratedShader {
        source: rendered,
        workgroup_size: WORKGROUP_SIZE,
        dispatch_abi,
        item_abi,
        result_abi,
    })
}

fn shader_contract(
    plan: ShaderPlan<'_>,
) -> Result<
    (
        &'static QueryContractDescriptor,
        &'static QueryExecutionBinding,
    ),
    QueryExecError,
> {
    let contract_id = match plan {
        ShaderPlan::Capture(plan) => plan.contract_id,
        ShaderPlan::World(plan) => plan.contract_id,
        ShaderPlan::Batch(plan) => plan.contract_id,
    };
    query_contract::query_contract_bundle(contract_id).ok_or_else(|| QueryExecError::Unsupported {
        message: format!("missing WGSL query contract '{}'", contract_id.as_str()),
    })
}

fn build_type_tags(ctx: &QueryExecContext) -> HashMap<SmolStr, TypeTagId> {
    let mut next = 1usize;
    let mut tags = HashMap::new();
    for record in all_builtin_records() {
        tags.insert(SmolStr::new(record.name), TypeTagId(next));
        next += 1;
    }
    for (_idx, class) in ctx.module.classes.iter() {
        if class.role == hir::ClassRole::Value {
            tags.insert(class.name.clone(), TypeTagId(next));
            next += 1;
        }
    }
    tags
}

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
        ],
    }
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

fn emit_value_and_abi_structs(
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

fn emit_value_structs(roots: &[PortableAbiType]) -> Result<String, QueryExecError> {
    let mut out = String::new();
    let mut seen = BTreeSet::new();
    for root in roots {
        emit_value_struct_recursive(root, &mut seen, &mut out)?;
    }
    Ok(out)
}

fn emit_value_struct_recursive(
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

fn value_type_name(abi: &PortableAbiType) -> Result<String, QueryExecError> {
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

fn prefix_abi_names(abi: &PortableAbiType, prefix: &str) -> PortableAbiType {
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

fn emit_struct_conversions(
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

fn emit_conversion_recursive(
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

fn abi_supports_wgsl_value(abi: &PortableAbiType) -> bool {
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

fn to_abi_expr(abi: &PortableAbiType, expr: &str) -> Result<String, QueryExecError> {
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

fn from_abi_expr(abi: &PortableAbiType, expr: &str) -> Result<String, QueryExecError> {
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

fn abi_type_name(abi: &PortableAbiType, prefix: &str) -> Result<String, QueryExecError> {
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

fn emit_bindings(
    dispatch_abi: &PortableAbiType,
    item_abi: &PortableAbiType,
    result_abi: &PortableAbiType,
) -> Result<String, QueryExecError> {
    let mut out = String::new();
    writeln!(out, "@group(0) @binding(0)").ok();
    writeln!(
        out,
        "var<storage, read> dispatch_config: {};",
        abi_type_name(dispatch_abi, "Abi_")?
    )
    .ok();
    writeln!(out, "struct InputBuffer {{").ok();
    writeln!(
        out,
        "  values: array<{}>,",
        abi_type_name(item_abi, "Abi_")?
    )
    .ok();
    writeln!(out, "}}\n").ok();
    writeln!(out, "struct ResultBuffer {{").ok();
    writeln!(
        out,
        "  values: array<{}>,",
        abi_type_name(result_abi, "Abi_")?
    )
    .ok();
    writeln!(out, "}}\n").ok();
    writeln!(out, "struct ShapeIndexBuffer {{").ok();
    writeln!(out, "  values: array<u32>,").ok();
    writeln!(out, "}}\n").ok();
    writeln!(out, "@group(0) @binding(1)").ok();
    writeln!(out, "var<storage, read> input_items: InputBuffer;").ok();
    writeln!(out, "@group(0) @binding(2)").ok();
    writeln!(out, "var<storage, read_write> output_items: ResultBuffer;").ok();
    writeln!(out, "@group(0) @binding(3)").ok();
    writeln!(out, "var<storage, read> world_shapes: ShapeIndexBuffer;").ok();
    Ok(out)
}

fn emit_scene_functions(
    ctx: &QueryExecContext,
    scene_index: &ShaderSceneIndex,
    behavior: &NormalizedShaderBehavior,
) -> Result<String, QueryExecError> {
    let ops = DirectQueryOps::new(ctx);
    let mut out = String::new();
    emit_profile_helper_functions(ctx, &mut out)?;
    emit_field_scene_functions(ctx, scene_index, &ops, &mut out)?;
    emit_shape_scene_functions(ctx, scene_index, &ops, behavior, &mut out)?;
    emit_scene_dispatch_functions(ctx, scene_index, behavior, &mut out)?;
    Ok(out)
}

fn emit_profile_helper_functions(
    ctx: &QueryExecContext,
    out: &mut String,
) -> Result<(), QueryExecError> {
    let mut polygon_arities = BTreeSet::new();
    let mut polyline_arities = BTreeSet::new();
    for scene in ctx.scene.fields.values() {
        collect_profile_helper_arities(&scene.root, &mut polygon_arities, &mut polyline_arities)?;
    }
    for arity in polygon_arities {
        emit_polygon_helper(out, arity)?;
        out.push('\n');
    }
    for arity in polyline_arities {
        emit_polyline_helper(out, arity)?;
        out.push('\n');
    }
    Ok(())
}

fn collect_profile_helper_arities(
    node: &crate::scene_ir::FieldNode,
    polygon_arities: &mut BTreeSet<usize>,
    polyline_arities: &mut BTreeSet<usize>,
) -> Result<(), QueryExecError> {
    match node {
        crate::scene_ir::FieldNode::Union { items }
        | crate::scene_ir::FieldNode::Intersection { items }
        | crate::scene_ir::FieldNode::Smooth { items, .. } => {
            for item in items {
                collect_profile_helper_arities(item, polygon_arities, polyline_arities)?;
            }
        }
        crate::scene_ir::FieldNode::Subtract { left, right } => {
            collect_profile_helper_arities(left, polygon_arities, polyline_arities)?;
            collect_profile_helper_arities(right, polygon_arities, polyline_arities)?;
        }
        crate::scene_ir::FieldNode::Transform { inner, .. }
        | crate::scene_ir::FieldNode::Repeat { inner, .. } => {
            collect_profile_helper_arities(inner, polygon_arities, polyline_arities)?;
        }
        crate::scene_ir::FieldNode::Extrude { profile, .. }
        | crate::scene_ir::FieldNode::Revolve { profile }
        | crate::scene_ir::FieldNode::Sweep { profile, .. } => {
            if let Some(profile) = profile {
                collect_profile_arity(profile, polygon_arities, polyline_arities)?;
            }
        }
        crate::scene_ir::FieldNode::Loft { from, to, .. } => {
            if let Some(profile) = from {
                collect_profile_arity(profile, polygon_arities, polyline_arities)?;
            }
            if let Some(profile) = to {
                collect_profile_arity(profile, polygon_arities, polyline_arities)?;
            }
        }
        crate::scene_ir::FieldNode::Use { .. }
        | crate::scene_ir::FieldNode::Primitive { .. }
        | crate::scene_ir::FieldNode::OpaqueLeaf => {}
    }
    Ok(())
}

fn collect_profile_arity(
    profile: &SceneProfileExpr,
    polygon_arities: &mut BTreeSet<usize>,
    polyline_arities: &mut BTreeSet<usize>,
) -> Result<(), QueryExecError> {
    let SceneProfileExpr::Primitive { primitive, args } = profile;
    match primitive {
        hir::ProfilePrimitive::Polygon2 => {
            let arity = scene_value_list_len(scene_named_arg_value(args, "vertices")?)?;
            if arity < 3 {
                return Err(QueryExecError::Unsupported {
                    message: format!("polygon2 requires at least 3 vertices, got {arity}"),
                });
            }
            polygon_arities.insert(arity);
        }
        hir::ProfilePrimitive::Polyline2 => {
            let arity = scene_value_list_len(scene_named_arg_value(args, "vertices")?)?;
            if arity < 2 {
                return Err(QueryExecError::Unsupported {
                    message: format!("polyline2 requires at least 2 vertices, got {arity}"),
                });
            }
            polyline_arities.insert(arity);
        }
        _ => {}
    }
    Ok(())
}

fn emit_polygon_helper(out: &mut String, arity: usize) -> Result<(), QueryExecError> {
    writeln!(
        out,
        "fn wr_polygon2_n{arity}(point: vec2<f32>, vertices: array<vec2<f32>, {arity}>) -> f32 {{"
    )
    .ok();
    writeln!(out, "  var inside = false;").ok();
    writeln!(out, "  var best = 3.4028235e38;").ok();
    writeln!(
        out,
        "  for (var index: u32 = 0u; index < {arity}u; index = index + 1u) {{"
    )
    .ok();
    writeln!(out, "    let a = vertices[index];").ok();
    writeln!(out, "    let b = vertices[(index + 1u) % {arity}u];").ok();
    writeln!(
        out,
        "    best = min(best, wr_polygon2_edge_distance(point, a, b));"
    )
    .ok();
    writeln!(out, "    if (wr_polygon2_edge_crosses(point, a, b)) {{").ok();
    writeln!(out, "      inside = !inside;").ok();
    writeln!(out, "    }}").ok();
    writeln!(out, "  }}").ok();
    writeln!(out, "  return wr_polygon2_finalize(best, inside);").ok();
    writeln!(out, "}}").ok();
    Ok(())
}

fn emit_polyline_helper(out: &mut String, arity: usize) -> Result<(), QueryExecError> {
    writeln!(
        out,
        "fn wr_polyline2_n{arity}(point: vec2<f32>, vertices: array<vec2<f32>, {arity}>) -> f32 {{"
    )
    .ok();
    writeln!(out, "  var best = 3.4028235e38;").ok();
    writeln!(
        out,
        "  for (var index: u32 = 0u; index + 1u < {arity}u; index = index + 1u) {{"
    )
    .ok();
    writeln!(out, "    let a = vertices[index];").ok();
    writeln!(out, "    let b = vertices[index + 1u];").ok();
    writeln!(
        out,
        "    best = min(best, wr_polyline2_edge_distance(point, a, b));"
    )
    .ok();
    writeln!(out, "  }}").ok();
    writeln!(out, "  return best;").ok();
    writeln!(out, "}}").ok();
    Ok(())
}

fn emit_portable_functions(
    ctx: &QueryExecContext,
    behavior: &NormalizedShaderBehavior,
) -> Result<String, QueryExecError> {
    let mut lowered = BTreeMap::<SmolStr, pir::ir::PirFunction>::new();
    let mut roots = BTreeSet::new();
    for scene in ctx.scene.shapes.values() {
        for leaf in scene.leaves.values() {
            if behavior.requires_material {
                roots.insert(leaf.material.clone());
            }
            if behavior.requires_radiance
                && let Some(radiance) = &leaf.radiance
            {
                roots.insert(radiance.clone());
            }
            if behavior.requires_volume
                && let Some(volume) = &leaf.volume
            {
                roots.insert(volume.clone());
            }
        }
    }

    for root in roots {
        let module = pir::lower_portable_entry_by_name(&ctx.module, &ctx.type_info, root.as_str())
            .map_err(|errors| QueryExecError::Unsupported {
                message: format!(
                    "failed to lower portable WGSL function '{}': {errors:?}",
                    root
                ),
            })?;
        for function in module.functions {
            lowered.entry(function.name.clone()).or_insert(function);
        }
    }

    let mut out = String::new();
    let mut scratch = 0usize;
    for function in lowered.values() {
        emit_pir_function(function, &mut scratch, &mut out)?;
        out.push('\n');
    }
    Ok(out)
}

fn emit_query_helpers(
    ctx: &QueryExecContext,
    scene_index: &ShaderSceneIndex,
    behavior: &NormalizedShaderBehavior,
) -> Result<String, QueryExecError> {
    let ops = DirectQueryOps::new(ctx);
    let mut out = String::new();

    if behavior.requires_trace() {
        emit_payload_lookup_function(ctx, scene_index, &ops, &mut out)?;

        writeln!(
            out,
            "fn trace_shape_for_index(shape_index: u32, origin: vec3<f32>, direction: vec3<f32>, max_distance: f32, min_step: f32, hit_epsilon: f32, max_steps: i32) -> Hit3 {{"
        )
        .ok();
        writeln!(out, "  var travel: f32 = 0.0;").ok();
        writeln!(out, "  var steps: i32 = 0;").ok();
        writeln!(out, "  loop {{").ok();
        writeln!(
            out,
            "    if (!(steps < max_steps && travel <= max_distance)) {{ break; }}"
        )
        .ok();
        writeln!(out, "    let point = origin + direction * travel;").ok();
        writeln!(
            out,
            "    let distance = shape_distance_dispatch(shape_index, point);"
        )
        .ok();
        writeln!(out, "    if (distance <= hit_epsilon) {{").ok();
        writeln!(
            out,
            "      let normal = shape_normal_dispatch(shape_index, point);"
        )
        .ok();
        writeln!(
            out,
            "      let winner = shape_winner_dispatch(shape_index, point);"
        )
        .ok();
        writeln!(out, "      if (winner.has_leaf != 0u) {{").ok();
        writeln!(
            out,
            "        let frame = field_local_frame_dispatch(winner.field_index, point);"
        )
        .ok();
        writeln!(
            out,
            "        let local_normal = field_local_normal_dispatch(winner.field_index, frame);"
        )
        .ok();
        writeln!(
            out,
            "        let payload = payload_for_shape_leaf(winner.leaf_scene_index, winner.leaf_id);"
        )
        .ok();
        writeln!(
            out,
            "        return wr_hit_value(true, travel, point, normal, frame.point, local_normal, steps, winner.feature_id, frame.instance_id, frame.repeat_id, root_shape_id_for_shape(shape_index), payload);"
        )
        .ok();
        writeln!(
            out,
            "      }} else {{ return wr_hit_value(true, travel, point, normal, point, normal, steps, 0u, 0u, 0u, root_shape_id_for_shape(shape_index), wr_default_payload()); }}"
        )
        .ok();
        writeln!(out, "    }}").ok();
        writeln!(out, "    travel = travel + max(distance, min_step);").ok();
        writeln!(out, "    steps = steps + 1;").ok();
        writeln!(out, "  }}").ok();
        writeln!(out, "  return wr_default_hit(origin);").ok();
        writeln!(out, "}}\n").ok();
    }

    writeln!(out, "fn world_distance_point(point: vec3<f32>) -> f32 {{").ok();
    writeln!(out, "  var current: f32 = 1000000.0;").ok();
    writeln!(
        out,
        "  for (var index: u32 = 0u; index < dispatch_config.shape_count; index = index + 1u) {{"
    )
    .ok();
    writeln!(
        out,
        "    current = min(current, shape_distance_dispatch(world_shapes.values[index], point));"
    )
    .ok();
    writeln!(out, "  }}").ok();
    writeln!(out, "  return current;").ok();
    writeln!(out, "}}\n").ok();

    writeln!(
        out,
        "fn world_normal_point(point: vec3<f32>) -> vec3<f32> {{"
    )
    .ok();
    out.push_str("  if (dispatch_config.shape_count == 1u) {\n");
    out.push_str("    let sample = shape_normal_dispatch_sample(world_shapes.values[0], point);\n");
    out.push_str("    if (sample.available != 0u) { return wr_normalize3(sample.normal); }\n");
    out.push_str("  }\n");
    writeln!(out, "  let eps: f32 = 0.001;").ok();
    writeln!(
        out,
        "  let dx = world_distance_point(point + vec3<f32>(eps, 0.0, 0.0)) - world_distance_point(point - vec3<f32>(eps, 0.0, 0.0));"
    )
    .ok();
    writeln!(
        out,
        "  let dy = world_distance_point(point + vec3<f32>(0.0, eps, 0.0)) - world_distance_point(point - vec3<f32>(0.0, eps, 0.0));"
    )
    .ok();
    writeln!(
        out,
        "  let dz = world_distance_point(point + vec3<f32>(0.0, 0.0, eps)) - world_distance_point(point - vec3<f32>(0.0, 0.0, eps));"
    )
    .ok();
    writeln!(out, "  return wr_normalize3(vec3<f32>(dx, dy, dz));").ok();
    writeln!(out, "}}\n").ok();

    if matches!(
        behavior.value_path,
        NormalizedQueryValuePath::WorldTrace | NormalizedQueryValuePath::WorldOcclusion
    ) {
        writeln!(out, "fn world_trace_ray(ray: RayQuery) -> Hit3 {{").ok();
        writeln!(out, "  var best = wr_default_hit(ray.origin);").ok();
        writeln!(out, "  var best_distance: f32 = 1e30;").ok();
        writeln!(out, "  for (var index: u32 = 0u; index < dispatch_config.shape_count; index = index + 1u) {{").ok();
        writeln!(
            out,
            "    let hit = trace_shape_for_index(world_shapes.values[index], ray.origin, ray.direction, ray.max_distance, ray.min_step, ray.hit_epsilon, ray.max_steps);"
        )
        .ok();
        writeln!(
            out,
            "    if (hit.hit && hit.distance < best_distance) {{ best_distance = hit.distance; best = hit; }}"
        )
        .ok();
        writeln!(out, "  }}").ok();
        writeln!(out, "  return best;").ok();
        writeln!(out, "}}\n").ok();
    }

    if matches!(behavior.value_path, NormalizedQueryValuePath::WorldSurface) {
        writeln!(out, "fn world_surface_hit(hit: Hit3) -> Surface {{").ok();
        out.push_str(
            "  if (dispatch_config.material_enabled == 0u) { return wr_default_surface(); }\n",
        );
        writeln!(
            out,
            "  let shape_index = shape_index_from_root_shape_id(hit.root_shape_id);"
        )
        .ok();
        out.push_str("  if (shape_index == 0xffffffffu) { return wr_default_surface(); }\n");
        writeln!(out, "  return surface_at_shape_dispatch(shape_index, hit);").ok();
        writeln!(out, "}}\n").ok();
    }

    if matches!(behavior.value_path, NormalizedQueryValuePath::WorldRadiance) {
        writeln!(
            out,
            "fn world_radiance_query(query: PointDirectionQuery) -> vec3<f32> {{"
        )
        .ok();
        out.push_str(
            "  if (dispatch_config.radiance_enabled == 0u) { return vec3<f32>(0.0, 0.0, 0.0); }\n",
        );
        writeln!(out, "  var total = vec3<f32>(0.0, 0.0, 0.0);").ok();
        writeln!(out, "  for (var index: u32 = 0u; index < dispatch_config.shape_count; index = index + 1u) {{").ok();
        writeln!(
            out,
            "    total = total + radiance_at_shape_dispatch(world_shapes.values[index], query.point, query.direction);"
        )
        .ok();
        writeln!(out, "  }}").ok();
        writeln!(out, "  return total;").ok();
        writeln!(out, "}}\n").ok();
    }

    if matches!(behavior.value_path, NormalizedQueryValuePath::WorldMedium) {
        writeln!(out, "fn world_medium_point(point: PointQuery) -> Medium {{").ok();
        out.push_str(
            "  if (dispatch_config.media_enabled == 0u) { return wr_default_medium(); }\n",
        );
        writeln!(out, "  var total = wr_default_medium();").ok();
        writeln!(out, "  for (var index: u32 = 0u; index < dispatch_config.shape_count; index = index + 1u) {{").ok();
        writeln!(
            out,
            "    total = wr_combine_medium_values(total, medium_at_shape_dispatch(world_shapes.values[index], point.point));"
        )
        .ok();
        writeln!(out, "  }}").ok();
        writeln!(out, "  return total;").ok();
        writeln!(out, "}}\n").ok();
    }

    writeln!(
        out,
        "fn capture_distance_point(point: PointQuery) -> f32 {{"
    )
    .ok();
    out.push_str("  if (dispatch_config.capture_kind == 0u) { return field_distance_dispatch(dispatch_config.capture_index, point.point); }\n");
    writeln!(
        out,
        "  return shape_distance_dispatch(dispatch_config.capture_index, point.point);"
    )
    .ok();
    writeln!(out, "}}\n").ok();

    writeln!(
        out,
        "fn capture_normal_point(point: PointQuery) -> vec3<f32> {{"
    )
    .ok();
    out.push_str("  if (dispatch_config.capture_kind == 0u) { return field_normal_dispatch(dispatch_config.capture_index, point.point); }\n");
    writeln!(
        out,
        "  return shape_normal_dispatch(dispatch_config.capture_index, point.point);"
    )
    .ok();
    writeln!(out, "}}\n").ok();

    if matches!(
        behavior.value_path,
        NormalizedQueryValuePath::CaptureTrace | NormalizedQueryValuePath::CaptureOcclusion
    ) {
        out.push_str("fn capture_trace_ray(ray: RayQuery) -> Hit3 { return trace_shape_for_index(dispatch_config.capture_index, ray.origin, ray.direction, ray.max_distance, ray.min_step, ray.hit_epsilon, ray.max_steps); }\n\n");
    }

    if matches!(
        behavior.value_path,
        NormalizedQueryValuePath::CaptureSurface
    ) {
        out.push_str("fn capture_surface_hit(hit: Hit3) -> Surface { return surface_at_shape_dispatch(dispatch_config.capture_index, hit); }\n\n");
    }

    if matches!(
        behavior.value_path,
        NormalizedQueryValuePath::CaptureRadiance
    ) {
        out.push_str("fn capture_radiance_query(query: PointDirectionQuery) -> vec3<f32> { return radiance_at_shape_dispatch(dispatch_config.capture_index, query.point, query.direction); }\n\n");
    }

    if matches!(behavior.value_path, NormalizedQueryValuePath::CaptureMedium) {
        out.push_str("fn capture_medium_point(point: PointQuery) -> Medium { return medium_at_shape_dispatch(dispatch_config.capture_index, point.point); }\n\n");
    }

    Ok(out)
}

fn emit_main(
    behavior: &NormalizedShaderBehavior,
    item_abi: &PortableAbiType,
    result_abi: &PortableAbiType,
) -> Result<String, QueryExecError> {
    let item_expr = from_abi_expr(item_abi, "input_items.values[index]")?;
    let eval_expr = match behavior.cardinality {
        QueryCardinality::Scalar => behavior.scalar_eval_expr(&item_expr),
        QueryCardinality::Batch => behavior.batch_eval_expr(&item_expr),
    };
    let store_expr = to_abi_expr(result_abi, "result")?;
    let mut out = String::new();
    writeln!(
        out,
        "@compute @workgroup_size(WG_SIZE)\nfn main(@builtin(global_invocation_id) global_id: vec3<u32>) {{"
    )
    .ok();
    writeln!(out, "  let index = global_id.x;").ok();
    writeln!(
        out,
        "  if (index >= dispatch_config.item_count) {{ return; }}"
    )
    .ok();
    writeln!(out, "  let result = {eval_expr};").ok();
    writeln!(out, "  output_items.values[index] = {store_expr};").ok();
    writeln!(out, "}}").ok();
    Ok(out)
}

fn emit_payload_lookup_function(
    ctx: &QueryExecContext,
    scene_index: &ShaderSceneIndex,
    ops: &DirectQueryOps<'_>,
    out: &mut String,
) -> Result<(), QueryExecError> {
    writeln!(
        out,
        "fn payload_for_shape_leaf(leaf_scene_index: u32, leaf_id: u32) -> Payload {{"
    )
    .ok();
    writeln!(out, "  switch leaf_scene_index {{").ok();
    for (shape_name, scene) in &ctx.scene.shapes {
        let shape_index = scene_index.shape(shape_name)?;
        writeln!(out, "    case {shape_index}u: {{").ok();
        writeln!(out, "      switch leaf_id {{").ok();
        for (leaf_id, leaf) in &scene.leaves {
            let payload = ops.eval_payload_body(&leaf.payload)?;
            writeln!(
                out,
                "        case {}u: {{ return {}; }}",
                leaf_id.0,
                kernel_value_literal(&payload)?
            )
            .ok();
        }
        writeln!(out, "        default: {{ return wr_default_payload(); }}").ok();
        writeln!(out, "      }}").ok();
        writeln!(out, "    }}").ok();
    }
    writeln!(out, "    default: {{ return wr_default_payload(); }}").ok();
    writeln!(out, "  }}").ok();
    writeln!(out, "}}\n").ok();
    Ok(())
}

const WGSL_PRELUDE: &str = include_str!("prelude.wgsl");

fn emit_normal_sample_support() -> Result<String, QueryExecError> {
    let mut out = String::new();
    out.push_str("const WR_NORMAL_ROLE_UNKNOWN: u32 = 0u;\n");
    out.push_str("const WR_NORMAL_ROLE_CERTIFIED_FIELD_GRADIENT: u32 = 1u;\n");
    out.push_str("const WR_NORMAL_ROLE_FEATURE_NORMAL: u32 = 2u;\n");
    out.push_str("const WR_NORMAL_ROLE_HEURISTIC_SHADING_NORMAL: u32 = 3u;\n\n");
    out.push_str("struct CertifiedNormalSample {\n");
    out.push_str("  normal: vec3<f32>,\n");
    out.push_str("  available: u32,\n");
    out.push_str("  role: u32,\n");
    out.push_str("}\n\n");
    out.push_str(
        "fn wr_unavailable_normal_sample() -> CertifiedNormalSample { return CertifiedNormalSample(vec3<f32>(0.0, 0.0, 0.0), 0u, WR_NORMAL_ROLE_UNKNOWN); }\n",
    );
    out.push_str(
        "fn wr_certified_field_gradient_sample(normal: vec3<f32>) -> CertifiedNormalSample { return CertifiedNormalSample(wr_safe_normalize3(normal), 1u, WR_NORMAL_ROLE_CERTIFIED_FIELD_GRADIENT); }\n",
    );
    out.push_str(
        "fn wr_feature_normal_sample(normal: vec3<f32>) -> CertifiedNormalSample { return CertifiedNormalSample(wr_safe_normalize3(normal), 1u, WR_NORMAL_ROLE_FEATURE_NORMAL); }\n",
    );
    out.push_str(
        "fn wr_smooth_blend_weight(left_distance: f32, right_distance: f32, smoothing: f32) -> f32 {\n",
    );
    out.push_str("  if (smoothing <= 0.0) { return 1.0; }\n");
    out.push_str(
        "  return clamp(0.5 + 0.5 * (right_distance - left_distance) / smoothing, 0.0, 1.0);\n",
    );
    out.push_str("}\n\n");
    Ok(out)
}

fn emit_field_scene_functions(
    ctx: &QueryExecContext,
    scene_index: &ShaderSceneIndex,
    ops: &DirectQueryOps<'_>,
    out: &mut String,
) -> Result<(), QueryExecError> {
    for (field_name, scene) in &ctx.scene.fields {
        let field_index = scene_index.field(field_name)?;
        for record in &scene.node_records {
            emit_field_node_function(ctx, scene_index, ops, field_name, field_index, record, out)?;
            emit_field_normal_function(
                ctx,
                scene_index,
                ops,
                field_name,
                field_index,
                record,
                out,
            )?;
        }
        emit_field_local_frame_functions(ctx, scene_index, ops, field_name, field_index, out)?;
    }
    Ok(())
}

fn emit_field_node_function(
    ctx: &QueryExecContext,
    scene_index: &ShaderSceneIndex,
    ops: &DirectQueryOps<'_>,
    field_name: &SmolStr,
    field_index: u32,
    record: &FieldNodeRecord,
    out: &mut String,
) -> Result<(), QueryExecError> {
    let fn_name = field_node_function_name(field_index, record.id.0);
    writeln!(out, "fn {fn_name}(point: vec3<f32>) -> f32 {{").ok();
    match record.kind {
        FieldNodeKindSummary::Use => {
            let target = record.target.as_ref().expect("use target");
            let target_scene = ctx.scene.fields.get(target).expect("field use scene");
            writeln!(
                out,
                "  return {}(point);",
                field_node_function_name(scene_index.field(target)?, target_scene.root_node_id.0)
            )
            .ok();
        }
        FieldNodeKindSummary::Primitive(kind) => {
            let payload = match record.payload.as_ref() {
                Some(SceneOperatorPayload::Primitive { args }) => args.as_deref().unwrap_or(&[]),
                _ => &[],
            };
            writeln!(
                out,
                "  return {};",
                emit_field_primitive_call(ops, kind, payload, "point")?
            )
            .ok();
        }
        FieldNodeKindSummary::Union => {
            writeln!(out, "  var current: f32 = 1000000.0;").ok();
            for child in &record.children {
                writeln!(
                    out,
                    "  current = wr_field_union(current, {}(point));",
                    field_node_function_name(field_index, child.0)
                )
                .ok();
            }
            writeln!(out, "  return current;").ok();
        }
        FieldNodeKindSummary::Intersection => {
            if let Some(first) = record.children.first() {
                writeln!(
                    out,
                    "  var current: f32 = {}(point);",
                    field_node_function_name(field_index, first.0)
                )
                .ok();
                for child in record.children.iter().skip(1) {
                    writeln!(
                        out,
                        "  current = wr_field_intersection(current, {}(point));",
                        field_node_function_name(field_index, child.0)
                    )
                    .ok();
                }
                writeln!(out, "  return current;").ok();
            } else {
                writeln!(out, "  return 1000000.0;").ok();
            }
        }
        FieldNodeKindSummary::Subtract => {
            let left = record.children.first().copied();
            let right = record.children.get(1).copied();
            if let (Some(left), Some(right)) = (left, right) {
                writeln!(
                    out,
                    "  return wr_field_subtract({}(point), {}(point));",
                    field_node_function_name(field_index, left.0),
                    field_node_function_name(field_index, right.0)
                )
                .ok();
            } else {
                writeln!(out, "  return 1000000.0;").ok();
            }
        }
        FieldNodeKindSummary::Transform(kind) => {
            let inner = record.children.first().copied();
            let param = match record.payload.as_ref() {
                Some(SceneOperatorPayload::Transform { param }) => param.as_ref(),
                _ => None,
            };
            if let Some(inner) = inner {
                if let Some(param) = param {
                    let value = ops.eval_scene_constant(param)?;
                    let rendered = kernel_value_literal(&value)?;
                    writeln!(
                        out,
                        "  let local_point = {}({}, point);",
                        transform_helper_name_for_value(kind, &value)?,
                        rendered
                    )
                    .ok();
                    writeln!(
                        out,
                        "  let inner_distance = {}(local_point);",
                        field_node_function_name(field_index, inner.0)
                    )
                    .ok();
                    if matches!(kind, TransformKind::UniformScale) {
                        writeln!(out, "  return inner_distance * wr_abs_scalar({rendered});").ok();
                    } else {
                        writeln!(out, "  return inner_distance;").ok();
                    }
                } else {
                    writeln!(
                        out,
                        "  return {}(point);",
                        field_node_function_name(field_index, inner.0)
                    )
                    .ok();
                }
            } else {
                writeln!(out, "  return 1000000.0;").ok();
            }
        }
        FieldNodeKindSummary::Repeat(kind) => {
            let inner = record.children.first().copied();
            let param = match record.payload.as_ref() {
                Some(SceneOperatorPayload::Repeat { param }) => param.as_ref(),
                _ => None,
            };
            if let Some(inner) = inner {
                if let Some(param) = param {
                    let value = ops.eval_scene_constant(param)?;
                    let rendered = kernel_value_literal(&value)?;
                    writeln!(
                        out,
                        "  let local_point = {}({}, point);",
                        repeat_helper_name_for_value(kind, &value)?,
                        rendered
                    )
                    .ok();
                    writeln!(
                        out,
                        "  return {}(local_point);",
                        field_node_function_name(field_index, inner.0)
                    )
                    .ok();
                } else {
                    writeln!(
                        out,
                        "  return {}(point);",
                        field_node_function_name(field_index, inner.0)
                    )
                    .ok();
                }
            } else {
                writeln!(out, "  return 1000000.0;").ok();
            }
        }
        FieldNodeKindSummary::Smooth(kind) => {
            let smoothing = match record.payload.as_ref() {
                Some(SceneOperatorPayload::Smooth { smoothing }) => smoothing.as_ref(),
                _ => None,
            };
            if let Some(first) = record.children.first() {
                writeln!(
                    out,
                    "  var current: f32 = {}(point);",
                    field_node_function_name(field_index, first.0)
                )
                .ok();
                let smoothing = smoothing
                    .map(|value| scene_constant_literal(ops, value))
                    .transpose()?
                    .unwrap_or_else(|| "0.0".to_string());
                match kind {
                    SmoothKind::Union => {
                        for child in record.children.iter().skip(1) {
                            writeln!(
                                out,
                                "  current = wr_smooth_union(current, {}(point), {});",
                                field_node_function_name(field_index, child.0),
                                smoothing
                            )
                            .ok();
                        }
                    }
                    SmoothKind::Intersection => {
                        for child in record.children.iter().skip(1) {
                            writeln!(
                                out,
                                "  current = wr_smooth_intersection(current, {}(point), {});",
                                field_node_function_name(field_index, child.0),
                                smoothing
                            )
                            .ok();
                        }
                    }
                    SmoothKind::Subtract => {
                        if let Some(second) = record.children.get(1) {
                            writeln!(
                                out,
                                "  current = wr_smooth_subtract(current, {}(point), {});",
                                field_node_function_name(field_index, second.0),
                                smoothing
                            )
                            .ok();
                        }
                    }
                }
                writeln!(out, "  return current;").ok();
            } else {
                writeln!(out, "  return 1000000.0;").ok();
            }
        }
        FieldNodeKindSummary::Extrude => {
            let (height, profile) = match record.payload.as_ref() {
                Some(SceneOperatorPayload::Extrude { height, profile }) => {
                    (height.as_ref(), profile.as_ref())
                }
                _ => (None, None),
            };
            if let (Some(height), Some(profile)) = (height, profile) {
                let height_value = ops.eval_scene_constant(height)?;
                let abs_height = abs_scalar_kernel_value(&height_value)?;
                let half_height = abs_height * 0.5;
                let profile_distance =
                    emit_profile_expr(ops, profile, "vec2<f32>(point.x, point.z)")?;
                writeln!(out, "  let profile_distance: f32 = {profile_distance};").ok();
                writeln!(
                    out,
                    "  let axial: f32 = abs(point.y) - {};",
                    format_f32(half_height)
                )
                .ok();
                writeln!(
                    out,
                    "  return wr_profile_cap_distance(profile_distance, axial);"
                )
                .ok();
            } else {
                writeln!(out, "  return 1000000.0;").ok();
            }
        }
        FieldNodeKindSummary::Revolve => {
            let profile = match record.payload.as_ref() {
                Some(SceneOperatorPayload::Revolve { profile }) => profile.as_ref(),
                _ => None,
            };
            if let Some(profile) = profile {
                let radial = "vec2<f32>(length(vec2<f32>(point.x, point.z)), point.y)";
                writeln!(
                    out,
                    "  return {};",
                    emit_profile_expr(ops, profile, radial)?
                )
                .ok();
            } else {
                writeln!(out, "  return 1000000.0;").ok();
            }
        }
        FieldNodeKindSummary::Sweep => {
            let (path, profile) = match record.payload.as_ref() {
                Some(SceneOperatorPayload::Sweep { path, profile }) => {
                    (path.as_ref(), profile.as_ref())
                }
                _ => (None, None),
            };
            if let (Some(path), Some(profile)) = (path, profile) {
                let path_value = ops.eval_scene_constant(path)?;
                let path_length = kernel_value_length(&path_value)?;
                let path_expr = kernel_value_literal(&path_value)?;
                writeln!(
                    out,
                    "  let coords = wr_field_sweep_coords({}, point);",
                    path_expr
                )
                .ok();
                writeln!(
                    out,
                    "  let profile_distance: f32 = {};",
                    emit_profile_expr(ops, profile, "vec2<f32>(coords.x, coords.y)")?
                )
                .ok();
                writeln!(
                    out,
                    "  let axial: f32 = abs(coords.z) - {};",
                    format_f32(path_length * 0.5)
                )
                .ok();
                writeln!(
                    out,
                    "  return wr_profile_cap_distance(profile_distance, axial);"
                )
                .ok();
            } else {
                writeln!(out, "  return 1000000.0;").ok();
            }
        }
        FieldNodeKindSummary::Loft => {
            let (height, from, to) = match record.payload.as_ref() {
                Some(SceneOperatorPayload::Loft { height, from, to }) => {
                    (height.as_ref(), from.as_ref(), to.as_ref())
                }
                _ => (None, None, None),
            };
            if let (Some(height), Some(from), Some(to)) = (height, from, to) {
                let height_value = ops.eval_scene_constant(height)?;
                let abs_height = abs_scalar_kernel_value(&height_value)?;
                let half_height = abs_height * 0.5;
                let safe_height = abs_height.max(0.0001);
                writeln!(out, "  let profile_point = vec2<f32>(point.x, point.z);").ok();
                writeln!(
                    out,
                    "  let from_distance: f32 = {};",
                    emit_profile_expr(ops, from, "profile_point")?
                )
                .ok();
                writeln!(
                    out,
                    "  let to_distance: f32 = {};",
                    emit_profile_expr(ops, to, "profile_point")?
                )
                .ok();
                writeln!(
                    out,
                    "  let t: f32 = clamp((point.y + {}) / {}, 0.0, 1.0);",
                    format_f32(half_height),
                    format_f32(safe_height)
                )
                .ok();
                writeln!(
                    out,
                    "  let mixed: f32 = from_distance + (to_distance - from_distance) * t;"
                )
                .ok();
                writeln!(
                    out,
                    "  let axial: f32 = abs(point.y) - {};",
                    format_f32(half_height)
                )
                .ok();
                writeln!(out, "  return wr_profile_cap_distance(mixed, axial);").ok();
            } else {
                writeln!(out, "  return 1000000.0;").ok();
            }
        }
        FieldNodeKindSummary::OpaqueLeaf => {
            let _ = (ctx, field_name);
            writeln!(out, "  return 1000000.0;").ok();
        }
    }
    writeln!(out, "}}\n").ok();
    Ok(())
}

fn emit_field_normal_function(
    ctx: &QueryExecContext,
    scene_index: &ShaderSceneIndex,
    ops: &DirectQueryOps<'_>,
    field_name: &SmolStr,
    field_index: u32,
    record: &FieldNodeRecord,
    out: &mut String,
) -> Result<(), QueryExecError> {
    let fn_name = field_normal_function_name(field_index, record.id.0);
    writeln!(
        out,
        "fn {fn_name}(point: vec3<f32>) -> CertifiedNormalSample {{"
    )
    .ok();
    match record.kind {
        FieldNodeKindSummary::Use => {
            let target = record.target.as_ref().expect("use target");
            let target_scene = ctx.scene.fields.get(target).expect("field use scene");
            writeln!(
                out,
                "  return {}(point);",
                field_normal_function_name(scene_index.field(target)?, target_scene.root_node_id.0)
            )
            .ok();
        }
        FieldNodeKindSummary::Primitive(kind) => {
            let payload = match record.payload.as_ref() {
                Some(SceneOperatorPayload::Primitive { args }) => args.as_deref().unwrap_or(&[]),
                _ => &[],
            };
            match kind {
                hir::FieldPrimitive::Sphere => {
                    writeln!(out, "  return wr_certified_field_gradient_sample(point);").ok();
                }
                hir::FieldPrimitive::Plane => {
                    writeln!(
                        out,
                        "  return wr_certified_field_gradient_sample({});",
                        scene_named_arg_literal(ops, payload, "normal")?
                    )
                    .ok();
                }
                _ => {
                    writeln!(out, "  return wr_unavailable_normal_sample();").ok();
                }
            }
        }
        FieldNodeKindSummary::Transform(kind) => {
            let inner = record.children.first().copied();
            let param = match record.payload.as_ref() {
                Some(SceneOperatorPayload::Transform { param }) => param.as_ref(),
                _ => None,
            };
            if let (Some(inner), Some(param)) = (inner, param) {
                let value = ops.eval_scene_constant(param)?;
                let rendered = kernel_value_literal(&value)?;
                if matches!(
                    kind,
                    TransformKind::Translate | TransformKind::Rotate | TransformKind::UniformScale
                ) {
                    writeln!(
                        out,
                        "  let local_point = {}({}, point);",
                        transform_helper_name_for_value(kind, &value)?,
                        rendered
                    )
                    .ok();
                    writeln!(
                        out,
                        "  let inner = {}(local_point);",
                        field_normal_function_name(field_index, inner.0)
                    )
                    .ok();
                    writeln!(out, "  if (inner.available == 0u) {{ return inner; }}").ok();
                    writeln!(
                        out,
                        "  return CertifiedNormalSample(wr_safe_normalize3({}), 1u, inner.role);",
                        transform_normal_expr_for_value(kind, &value, &rendered, "inner.normal")?
                    )
                    .ok();
                } else {
                    writeln!(out, "  return wr_unavailable_normal_sample();").ok();
                }
            } else if let Some(inner) = inner {
                writeln!(
                    out,
                    "  return {}(point);",
                    field_normal_function_name(field_index, inner.0)
                )
                .ok();
            } else {
                writeln!(out, "  return wr_unavailable_normal_sample();").ok();
            }
        }
        FieldNodeKindSummary::Smooth(kind) => {
            let smoothing = match record.payload.as_ref() {
                Some(SceneOperatorPayload::Smooth { smoothing }) => smoothing.as_ref(),
                _ => None,
            };
            if let Some(first) = record.children.first() {
                let smoothing = smoothing
                    .map(|value| scene_constant_literal(ops, value))
                    .transpose()?
                    .unwrap_or_else(|| "0.0".to_string());
                writeln!(
                    out,
                    "  if ({smoothing} <= 0.0) {{ return wr_unavailable_normal_sample(); }}"
                )
                .ok();
                writeln!(
                    out,
                    "  let first_sample = {}(point);",
                    field_normal_function_name(field_index, first.0)
                )
                .ok();
                writeln!(
                    out,
                    "  if (first_sample.available == 0u) {{ return first_sample; }}"
                )
                .ok();
                writeln!(
                    out,
                    "  var current_distance: f32 = {}(point);",
                    field_node_function_name(field_index, first.0)
                )
                .ok();
                writeln!(out, "  var current_normal = first_sample.normal;").ok();
                match kind {
                    SmoothKind::Union | SmoothKind::Intersection => {
                        for child in record.children.iter().skip(1) {
                            writeln!(
                                out,
                                "  let rhs_sample = {}(point);",
                                field_normal_function_name(field_index, child.0)
                            )
                            .ok();
                            writeln!(
                                out,
                                "  if (rhs_sample.available == 0u) {{ return rhs_sample; }}"
                            )
                            .ok();
                            writeln!(
                                out,
                                "  let rhs_distance: f32 = {}(point);",
                                field_node_function_name(field_index, child.0)
                            )
                            .ok();
                            writeln!(
                                out,
                                "  let h = wr_smooth_blend_weight(current_distance, rhs_distance, {smoothing});"
                            )
                            .ok();
                            writeln!(
                                out,
                                "  current_normal = wr_safe_normalize3(current_normal * h + rhs_sample.normal * (1.0 - h));"
                            )
                            .ok();
                            writeln!(
                                out,
                                "  current_distance = {}(current_distance, rhs_distance, {smoothing});",
                                match kind {
                                    SmoothKind::Union => "wr_smooth_union",
                                    SmoothKind::Intersection => "wr_smooth_intersection",
                                    SmoothKind::Subtract => unreachable!(),
                                }
                            )
                            .ok();
                        }
                    }
                    SmoothKind::Subtract => {
                        if let Some(second) = record.children.get(1) {
                            writeln!(
                                out,
                                "  let rhs_sample = {}(point);",
                                field_normal_function_name(field_index, second.0)
                            )
                            .ok();
                            writeln!(
                                out,
                                "  if (rhs_sample.available == 0u) {{ return rhs_sample; }}"
                            )
                            .ok();
                            writeln!(
                                out,
                                "  let rhs_distance: f32 = {}(point);",
                                field_node_function_name(field_index, second.0)
                            )
                            .ok();
                            writeln!(
                                out,
                                "  let h = wr_smooth_blend_weight(current_distance, rhs_distance, {smoothing});"
                            )
                            .ok();
                            writeln!(
                                out,
                                "  current_normal = wr_safe_normalize3(current_normal * h + (-rhs_sample.normal) * (1.0 - h));"
                            )
                            .ok();
                        } else {
                            writeln!(out, "  return wr_unavailable_normal_sample();").ok();
                        }
                    }
                }
                writeln!(
                    out,
                    "  return wr_certified_field_gradient_sample(current_normal);"
                )
                .ok();
            } else {
                writeln!(out, "  return wr_unavailable_normal_sample();").ok();
            }
        }
        FieldNodeKindSummary::Repeat(_)
        | FieldNodeKindSummary::Union
        | FieldNodeKindSummary::Intersection
        | FieldNodeKindSummary::Subtract
        | FieldNodeKindSummary::Extrude
        | FieldNodeKindSummary::Revolve
        | FieldNodeKindSummary::Sweep
        | FieldNodeKindSummary::Loft
        | FieldNodeKindSummary::OpaqueLeaf => {
            let _ = field_name;
            writeln!(out, "  return wr_unavailable_normal_sample();").ok();
        }
    }
    writeln!(out, "}}\n").ok();
    Ok(())
}

fn emit_field_local_frame_functions(
    ctx: &QueryExecContext,
    scene_index: &ShaderSceneIndex,
    ops: &DirectQueryOps<'_>,
    field_name: &SmolStr,
    field_index: u32,
    out: &mut String,
) -> Result<(), QueryExecError> {
    let scene = ctx.scene.fields.get(field_name).expect("field scene");
    for record in &scene.node_records {
        let fn_name = field_local_frame_function_name(field_index, record.id.0);
        writeln!(
            out,
            "fn {fn_name}(point: vec3<f32>, instance_id: u32, repeat_id: u32) -> FieldLocalFrame {{"
        )
        .ok();
        match record.kind {
            FieldNodeKindSummary::Use => {
                let target = record.target.as_ref().expect("field use target");
                let target_scene = ctx.scene.fields.get(target).expect("field use scene");
                writeln!(
                    out,
                    "  return {}(point, instance_id, repeat_id);",
                    field_local_frame_function_name(
                        scene_index.field(target)?,
                        target_scene.root_node_id.0,
                    )
                )
                .ok();
            }
            FieldNodeKindSummary::Transform(kind) => {
                let inner = record.children.first().copied();
                let param = match record.payload.as_ref() {
                    Some(SceneOperatorPayload::Transform { param }) => param.as_ref(),
                    _ => None,
                };
                if let (Some(inner), Some(param)) = (inner, param) {
                    let value = ops.eval_scene_constant(param)?;
                    let rendered = kernel_value_literal(&value)?;
                    writeln!(
                        out,
                        "  let local_point = {}({}, point);",
                        transform_helper_name_for_value(kind, &value)?,
                        rendered
                    )
                    .ok();
                    writeln!(
                        out,
                        "  return {}(local_point, instance_id, repeat_id);",
                        field_local_frame_function_name(field_index, inner.0)
                    )
                    .ok();
                } else if let Some(inner) = inner {
                    writeln!(
                        out,
                        "  return {}(point, instance_id, repeat_id);",
                        field_local_frame_function_name(field_index, inner.0)
                    )
                    .ok();
                } else {
                    writeln!(
                        out,
                        "  return FieldLocalFrame(point, instance_id, repeat_id, {}u);",
                        record.id.0
                    )
                    .ok();
                }
            }
            FieldNodeKindSummary::Repeat(kind) => {
                let inner = record.children.first().copied();
                let param = match record.payload.as_ref() {
                    Some(SceneOperatorPayload::Repeat { param }) => param.as_ref(),
                    _ => None,
                };
                if let (Some(inner), Some(param)) = (inner, param) {
                    let value = ops.eval_scene_constant(param)?;
                    let rendered = kernel_value_literal(&value)?;
                    let identity_fn = repeat_identity_helper_name_for_value(kind, &value)?;
                    match kind {
                        RepeatKind::InstanceArray => {
                            writeln!(out, "  let component = {}({});", identity_fn, rendered).ok();
                        }
                        _ => {
                            writeln!(
                                out,
                                "  let component = {}({}, point);",
                                identity_fn, rendered
                            )
                            .ok();
                        }
                    }
                    writeln!(
                        out,
                        "  let local_point = {}({}, point);",
                        repeat_helper_name_for_value(kind, &value)?,
                        rendered
                    )
                    .ok();
                    match kind {
                        RepeatKind::InstanceArray => {
                            writeln!(
                                out,
                                "  let next_instance_id = wr_chain_identity_component(instance_id, component);"
                            )
                            .ok();
                            writeln!(
                                out,
                                "  return {}(local_point, next_instance_id, repeat_id);",
                                field_local_frame_function_name(field_index, inner.0)
                            )
                            .ok();
                        }
                        _ => {
                            writeln!(
                                out,
                                "  let next_repeat_id = wr_chain_identity_component(repeat_id, component);"
                            )
                            .ok();
                            writeln!(
                                out,
                                "  return {}(local_point, instance_id, next_repeat_id);",
                                field_local_frame_function_name(field_index, inner.0)
                            )
                            .ok();
                        }
                    }
                } else if let Some(inner) = inner {
                    writeln!(
                        out,
                        "  return {}(point, instance_id, repeat_id);",
                        field_local_frame_function_name(field_index, inner.0)
                    )
                    .ok();
                } else {
                    writeln!(
                        out,
                        "  return FieldLocalFrame(point, instance_id, repeat_id, {}u);",
                        record.id.0
                    )
                    .ok();
                }
            }
            _ => {
                writeln!(
                    out,
                    "  return FieldLocalFrame(point, instance_id, repeat_id, {}u);",
                    record.id.0
                )
                .ok();
            }
        }
        writeln!(out, "}}\n").ok();
    }

    let opaque_distance = if scene.opaque_boundary {
        let bounds = scene
            .authored_bounds
            .as_ref()
            .ok_or_else(|| QueryExecError::Unsupported {
                message: format!("opaque field '{}' is missing authored bounds", field_name),
            })?;
        let bounds_value = ops.eval_scene_constant(bounds)?;
        let (center, half) = bounds_center_half(&bounds_value)?;
        format!(
            "wr_box(point - {}, {})",
            kernel_value_literal(&KernelValue::Vec3(center))?,
            kernel_value_literal(&KernelValue::Vec3(half))?
        )
    } else {
        "1000000.0".to_string()
    };
    writeln!(
        out,
        "fn {}(point: vec3<f32>) -> f32 {{ return {}; }}\n",
        field_opaque_distance_function_name(field_index),
        opaque_distance
    )
    .ok();

    writeln!(
        out,
        "fn {}(terminal_node_id: u32, point: vec3<f32>) -> f32 {{",
        field_terminal_distance_function_name(field_index)
    )
    .ok();
    writeln!(out, "  switch terminal_node_id {{").ok();
    for record in &scene.node_records {
        writeln!(out, "    case {}u: {{", record.id.0).ok();
        if matches!(record.kind, FieldNodeKindSummary::OpaqueLeaf) {
            writeln!(
                out,
                "      return {}(point);",
                field_opaque_distance_function_name(field_index)
            )
            .ok();
        } else {
            writeln!(
                out,
                "      return {}(point);",
                field_node_function_name(field_index, record.id.0)
            )
            .ok();
        }
        writeln!(out, "    }}").ok();
    }
    writeln!(out, "    default: {{ return 1000000.0; }}").ok();
    writeln!(out, "  }}").ok();
    writeln!(out, "}}\n").ok();

    writeln!(
        out,
        "fn {}(terminal_node_id: u32, point: vec3<f32>) -> CertifiedNormalSample {{",
        field_terminal_normal_function_name(field_index)
    )
    .ok();
    writeln!(out, "  switch terminal_node_id {{").ok();
    for record in &scene.node_records {
        writeln!(out, "    case {}u: {{", record.id.0).ok();
        if matches!(record.kind, FieldNodeKindSummary::OpaqueLeaf) {
            writeln!(out, "      return wr_unavailable_normal_sample();").ok();
        } else {
            writeln!(
                out,
                "      return {}(point);",
                field_normal_function_name(field_index, record.id.0)
            )
            .ok();
        }
        writeln!(out, "    }}").ok();
    }
    writeln!(
        out,
        "    default: {{ return wr_unavailable_normal_sample(); }}"
    )
    .ok();
    writeln!(out, "  }}").ok();
    writeln!(out, "}}\n").ok();

    Ok(())
}

fn emit_shape_scene_functions(
    ctx: &QueryExecContext,
    scene_index: &ShaderSceneIndex,
    ops: &DirectQueryOps<'_>,
    behavior: &NormalizedShaderBehavior,
    out: &mut String,
) -> Result<(), QueryExecError> {
    for (shape_name, scene) in &ctx.scene.shapes {
        let shape_index = scene_index.shape(shape_name)?;
        for record in &scene.node_records {
            emit_shape_distance_function(ctx, scene_index, shape_name, shape_index, record, out)?;
            emit_shape_normal_function(ctx, scene_index, shape_name, shape_index, record, out)?;
            if behavior.requires_trace() {
                emit_shape_winner_function(ctx, scene_index, shape_name, shape_index, record, out)?;
            }
            if behavior.requires_radiance {
                emit_shape_radiance_function(
                    ctx,
                    scene_index,
                    shape_name,
                    shape_index,
                    record,
                    out,
                )?;
            }
            if behavior.requires_volume {
                emit_shape_medium_function(
                    ctx,
                    scene_index,
                    ops,
                    shape_name,
                    shape_index,
                    record,
                    out,
                )?;
            }
        }
        if behavior.requires_material {
            emit_shape_surface_function(ctx, scene_index, shape_name, shape_index, out)?;
        }
    }
    Ok(())
}

fn emit_scene_dispatch_functions(
    ctx: &QueryExecContext,
    scene_index: &ShaderSceneIndex,
    behavior: &NormalizedShaderBehavior,
    out: &mut String,
) -> Result<(), QueryExecError> {
    emit_field_dispatch_functions(ctx, scene_index, out)?;
    emit_shape_dispatch_functions(ctx, scene_index, behavior, out)?;
    Ok(())
}

fn emit_field_dispatch_functions(
    ctx: &QueryExecContext,
    scene_index: &ShaderSceneIndex,
    out: &mut String,
) -> Result<(), QueryExecError> {
    writeln!(
        out,
        "fn field_distance_dispatch(field_index: u32, point: vec3<f32>) -> f32 {{"
    )
    .ok();
    writeln!(out, "  switch field_index {{").ok();
    for (field_name, scene) in &ctx.scene.fields {
        let field_index = scene_index.field(field_name)?;
        if scene.opaque_boundary {
            writeln!(
                out,
                "    case {field_index}u: {{ return {}(point); }}",
                field_opaque_distance_function_name(field_index)
            )
            .ok();
        } else {
            writeln!(
                out,
                "    case {field_index}u: {{ return {}(point); }}",
                field_node_function_name(field_index, scene.root_node_id.0)
            )
            .ok();
        }
    }
    writeln!(out, "    default: {{ return 1000000.0; }}").ok();
    writeln!(out, "  }}").ok();
    writeln!(out, "}}\n").ok();

    writeln!(
        out,
        "fn field_local_frame_dispatch_with_ids(field_index: u32, point: vec3<f32>, instance_id: u32, repeat_id: u32) -> FieldLocalFrame {{"
    )
    .ok();
    writeln!(out, "  switch field_index {{").ok();
    for (field_name, scene) in &ctx.scene.fields {
        let field_index = scene_index.field(field_name)?;
        writeln!(
            out,
            "    case {field_index}u: {{ return {}(point, instance_id, repeat_id); }}",
            field_local_frame_function_name(field_index, scene.root_node_id.0)
        )
        .ok();
    }
    writeln!(
        out,
        "    default: {{ return FieldLocalFrame(point, instance_id, repeat_id, 0u); }}"
    )
    .ok();
    writeln!(out, "  }}").ok();
    writeln!(out, "}}\n").ok();

    writeln!(
        out,
        "fn field_local_frame_dispatch(field_index: u32, point: vec3<f32>) -> FieldLocalFrame {{"
    )
    .ok();
    writeln!(
        out,
        "  return field_local_frame_dispatch_with_ids(field_index, point, 0u, 0u);"
    )
    .ok();
    writeln!(out, "}}\n").ok();

    writeln!(
        out,
        "fn field_terminal_distance_dispatch(field_index: u32, terminal_node_id: u32, point: vec3<f32>) -> f32 {{"
    )
    .ok();
    writeln!(out, "  switch field_index {{").ok();
    for (field_name, _scene) in &ctx.scene.fields {
        let field_index = scene_index.field(field_name)?;
        writeln!(
            out,
            "    case {field_index}u: {{ return {}(terminal_node_id, point); }}",
            field_terminal_distance_function_name(field_index)
        )
        .ok();
    }
    writeln!(out, "    default: {{ return 1000000.0; }}").ok();
    writeln!(out, "  }}").ok();
    writeln!(out, "}}\n").ok();

    writeln!(
        out,
        "fn field_terminal_normal_dispatch_sample(field_index: u32, terminal_node_id: u32, point: vec3<f32>) -> CertifiedNormalSample {{"
    )
    .ok();
    writeln!(out, "  switch field_index {{").ok();
    for (field_name, _scene) in &ctx.scene.fields {
        let field_index = scene_index.field(field_name)?;
        writeln!(
            out,
            "    case {field_index}u: {{ return {}(terminal_node_id, point); }}",
            field_terminal_normal_function_name(field_index)
        )
        .ok();
    }
    writeln!(
        out,
        "    default: {{ return wr_unavailable_normal_sample(); }}"
    )
    .ok();
    writeln!(out, "  }}").ok();
    writeln!(out, "}}\n").ok();

    writeln!(
        out,
        "fn field_normal_dispatch_sample(field_index: u32, point: vec3<f32>) -> CertifiedNormalSample {{"
    )
    .ok();
    writeln!(out, "  switch field_index {{").ok();
    for (field_name, scene) in &ctx.scene.fields {
        let field_index = scene_index.field(field_name)?;
        writeln!(
            out,
            "    case {field_index}u: {{ return {}(point); }}",
            field_normal_function_name(field_index, scene.root_node_id.0)
        )
        .ok();
    }
    writeln!(
        out,
        "    default: {{ return wr_unavailable_normal_sample(); }}"
    )
    .ok();
    writeln!(out, "  }}").ok();
    writeln!(out, "}}\n").ok();

    writeln!(
        out,
        "fn field_normal_dispatch(field_index: u32, point: vec3<f32>) -> vec3<f32> {{"
    )
    .ok();
    writeln!(
        out,
        "  let sample = field_normal_dispatch_sample(field_index, point);"
    )
    .ok();
    writeln!(
        out,
        "  if (sample.available != 0u) {{ return wr_normalize3(sample.normal); }}"
    )
    .ok();
    writeln!(out, "  let eps: f32 = 0.001;").ok();
    writeln!(
        out,
        "  let dx = field_distance_dispatch(field_index, point + vec3<f32>(eps, 0.0, 0.0)) - field_distance_dispatch(field_index, point - vec3<f32>(eps, 0.0, 0.0));"
    )
    .ok();
    writeln!(
        out,
        "  let dy = field_distance_dispatch(field_index, point + vec3<f32>(0.0, eps, 0.0)) - field_distance_dispatch(field_index, point - vec3<f32>(0.0, eps, 0.0));"
    )
    .ok();
    writeln!(
        out,
        "  let dz = field_distance_dispatch(field_index, point + vec3<f32>(0.0, 0.0, eps)) - field_distance_dispatch(field_index, point - vec3<f32>(0.0, 0.0, eps));"
    )
    .ok();
    writeln!(out, "  return wr_normalize3(vec3<f32>(dx, dy, dz));").ok();
    writeln!(out, "}}\n").ok();

    writeln!(
        out,
        "fn field_local_normal_dispatch(field_index: u32, frame: FieldLocalFrame) -> vec3<f32> {{"
    )
    .ok();
    writeln!(
        out,
        "  let sample = field_terminal_normal_dispatch_sample(field_index, frame.terminal_node_id, frame.point);"
    )
    .ok();
    writeln!(
        out,
        "  if (sample.available != 0u) {{ return wr_normalize3(sample.normal); }}"
    )
    .ok();
    writeln!(out, "  let eps: f32 = 0.001;").ok();
    writeln!(
        out,
        "  let dx = field_terminal_distance_dispatch(field_index, frame.terminal_node_id, frame.point + vec3<f32>(eps, 0.0, 0.0)) - field_terminal_distance_dispatch(field_index, frame.terminal_node_id, frame.point - vec3<f32>(eps, 0.0, 0.0));"
    )
    .ok();
    writeln!(
        out,
        "  let dy = field_terminal_distance_dispatch(field_index, frame.terminal_node_id, frame.point + vec3<f32>(0.0, eps, 0.0)) - field_terminal_distance_dispatch(field_index, frame.terminal_node_id, frame.point - vec3<f32>(0.0, eps, 0.0));"
    )
    .ok();
    writeln!(
        out,
        "  let dz = field_terminal_distance_dispatch(field_index, frame.terminal_node_id, frame.point + vec3<f32>(0.0, 0.0, eps)) - field_terminal_distance_dispatch(field_index, frame.terminal_node_id, frame.point - vec3<f32>(0.0, 0.0, eps));"
    )
    .ok();
    writeln!(out, "  return wr_normalize3(vec3<f32>(dx, dy, dz));").ok();
    writeln!(out, "}}\n").ok();

    Ok(())
}

fn emit_shape_dispatch_functions(
    ctx: &QueryExecContext,
    scene_index: &ShaderSceneIndex,
    behavior: &NormalizedShaderBehavior,
    out: &mut String,
) -> Result<(), QueryExecError> {
    writeln!(
        out,
        "fn shape_distance_dispatch(shape_index: u32, point: vec3<f32>) -> f32 {{"
    )
    .ok();
    writeln!(out, "  switch shape_index {{").ok();
    for (shape_name, scene) in &ctx.scene.shapes {
        let shape_index = scene_index.shape(shape_name)?;
        writeln!(
            out,
            "    case {shape_index}u: {{ return {}(point); }}",
            shape_distance_function_name(shape_index, scene.root_node_id.0)
        )
        .ok();
    }
    writeln!(out, "    default: {{ return 1000000.0; }}").ok();
    writeln!(out, "  }}").ok();
    writeln!(out, "}}\n").ok();

    if behavior.requires_trace() {
        writeln!(
            out,
            "fn shape_winner_dispatch(shape_index: u32, point: vec3<f32>) -> ShapeWinner {{"
        )
        .ok();
        writeln!(out, "  switch shape_index {{").ok();
        for (shape_name, scene) in &ctx.scene.shapes {
            let shape_index = scene_index.shape(shape_name)?;
            writeln!(
                out,
                "    case {shape_index}u: {{ return {}(point); }}",
                shape_winner_function_name(shape_index, scene.root_node_id.0)
            )
            .ok();
        }
        writeln!(out, "    default: {{ return wr_default_shape_winner(); }}").ok();
        writeln!(out, "  }}").ok();
        writeln!(out, "}}\n").ok();
    }

    writeln!(
        out,
        "fn shape_normal_dispatch_sample(shape_index: u32, point: vec3<f32>) -> CertifiedNormalSample {{"
    )
    .ok();
    writeln!(out, "  switch shape_index {{").ok();
    for (shape_name, scene) in &ctx.scene.shapes {
        let shape_index = scene_index.shape(shape_name)?;
        writeln!(
            out,
            "    case {shape_index}u: {{ return {}(point); }}",
            shape_normal_function_name(shape_index, scene.root_node_id.0)
        )
        .ok();
    }
    writeln!(
        out,
        "    default: {{ return wr_unavailable_normal_sample(); }}"
    )
    .ok();
    writeln!(out, "  }}").ok();
    writeln!(out, "}}\n").ok();

    writeln!(
        out,
        "fn shape_normal_dispatch(shape_index: u32, point: vec3<f32>) -> vec3<f32> {{"
    )
    .ok();
    writeln!(
        out,
        "  let sample = shape_normal_dispatch_sample(shape_index, point);"
    )
    .ok();
    writeln!(
        out,
        "  if (sample.available != 0u) {{ return wr_normalize3(sample.normal); }}"
    )
    .ok();
    writeln!(out, "  let eps: f32 = 0.001;").ok();
    writeln!(
        out,
        "  let dx = shape_distance_dispatch(shape_index, point + vec3<f32>(eps, 0.0, 0.0)) - shape_distance_dispatch(shape_index, point - vec3<f32>(eps, 0.0, 0.0));"
    )
    .ok();
    writeln!(
        out,
        "  let dy = shape_distance_dispatch(shape_index, point + vec3<f32>(0.0, eps, 0.0)) - shape_distance_dispatch(shape_index, point - vec3<f32>(0.0, eps, 0.0));"
    )
    .ok();
    writeln!(
        out,
        "  let dz = shape_distance_dispatch(shape_index, point + vec3<f32>(0.0, 0.0, eps)) - shape_distance_dispatch(shape_index, point - vec3<f32>(0.0, 0.0, eps));"
    )
    .ok();
    writeln!(out, "  return wr_normalize3(vec3<f32>(dx, dy, dz));").ok();
    writeln!(out, "}}\n").ok();

    if behavior.requires_material {
        writeln!(
            out,
            "fn surface_at_shape_dispatch(shape_index: u32, hit: Hit3) -> Surface {{"
        )
        .ok();
        writeln!(out, "  switch shape_index {{").ok();
        for (shape_name, _scene) in &ctx.scene.shapes {
            let shape_index = scene_index.shape(shape_name)?;
            writeln!(
                out,
                "    case {shape_index}u: {{ return {}(hit); }}",
                shape_surface_function_name(shape_index)
            )
            .ok();
        }
        writeln!(out, "    default: {{ return wr_default_surface(); }}").ok();
        writeln!(out, "  }}").ok();
        writeln!(out, "}}\n").ok();
    }

    if behavior.requires_radiance {
        writeln!(out, "fn radiance_at_shape_dispatch(shape_index: u32, point: vec3<f32>, direction: vec3<f32>) -> vec3<f32> {{").ok();
        writeln!(out, "  switch shape_index {{").ok();
        for (shape_name, scene) in &ctx.scene.shapes {
            let shape_index = scene_index.shape(shape_name)?;
            writeln!(
                out,
                "    case {shape_index}u: {{ return {}(point, direction); }}",
                shape_radiance_function_name(shape_index, scene.root_node_id.0)
            )
            .ok();
        }
        writeln!(out, "    default: {{ return vec3<f32>(0.0, 0.0, 0.0); }}").ok();
        writeln!(out, "  }}").ok();
        writeln!(out, "}}\n").ok();
    }

    if behavior.requires_volume {
        writeln!(
            out,
            "fn medium_at_shape_dispatch(shape_index: u32, point: vec3<f32>) -> Medium {{"
        )
        .ok();
        writeln!(out, "  switch shape_index {{").ok();
        for (shape_name, scene) in &ctx.scene.shapes {
            let shape_index = scene_index.shape(shape_name)?;
            writeln!(
                out,
                "    case {shape_index}u: {{ return {}(point); }}",
                shape_medium_function_name(shape_index, scene.root_node_id.0)
            )
            .ok();
        }
        writeln!(out, "    default: {{ return wr_default_medium(); }}").ok();
        writeln!(out, "  }}").ok();
        writeln!(out, "}}\n").ok();
    }

    if behavior.requires_trace() {
        writeln!(
            out,
            "fn root_shape_id_for_shape(shape_index: u32) -> u32 {{"
        )
        .ok();
        writeln!(out, "  switch shape_index {{").ok();
        for (shape_name, _scene) in &ctx.scene.shapes {
            let shape_index = scene_index.shape(shape_name)?;
            writeln!(
                out,
                "    case {shape_index}u: {{ return {}u; }}",
                crate::query_exec::stable_shape_capture_id(shape_name)
            )
            .ok();
        }
        writeln!(out, "    default: {{ return 0u; }}").ok();
        writeln!(out, "  }}").ok();
        writeln!(out, "}}\n").ok();
    }

    if behavior.requires_root_shape_lookup() {
        writeln!(
            out,
            "fn shape_index_from_root_shape_id(root_shape_id: u32) -> u32 {{"
        )
        .ok();
        writeln!(out, "  switch root_shape_id {{").ok();
        for (shape_name, _scene) in &ctx.scene.shapes {
            let shape_index = scene_index.shape(shape_name)?;
            writeln!(
                out,
                "    case {}u: {{ return {shape_index}u; }}",
                crate::query_exec::stable_shape_capture_id(shape_name)
            )
            .ok();
        }
        writeln!(out, "    default: {{ return 0xffffffffu; }}").ok();
        writeln!(out, "  }}").ok();
        writeln!(out, "}}\n").ok();
    }

    Ok(())
}

fn field_node_function_name(field_index: u32, node_id: u32) -> String {
    format!("wr_field_{field_index}_node_{node_id}")
}

fn field_local_frame_function_name(field_index: u32, node_id: u32) -> String {
    format!("wr_field_{field_index}_local_frame_{node_id}")
}

fn field_terminal_distance_function_name(field_index: u32) -> String {
    format!("wr_field_{field_index}_terminal_distance")
}

fn field_terminal_normal_function_name(field_index: u32) -> String {
    format!("wr_field_{field_index}_terminal_normal")
}

fn field_normal_function_name(field_index: u32, node_id: u32) -> String {
    format!("wr_field_{field_index}_normal_{node_id}")
}

fn field_opaque_distance_function_name(field_index: u32) -> String {
    format!("wr_field_{field_index}_opaque_distance")
}

fn shape_distance_function_name(shape_index: u32, node_id: u32) -> String {
    format!("wr_shape_{shape_index}_distance_{node_id}")
}

fn shape_normal_function_name(shape_index: u32, node_id: u32) -> String {
    format!("wr_shape_{shape_index}_normal_{node_id}")
}

fn shape_winner_function_name(shape_index: u32, node_id: u32) -> String {
    format!("wr_shape_{shape_index}_winner_{node_id}")
}

fn shape_radiance_function_name(shape_index: u32, node_id: u32) -> String {
    format!("wr_shape_{shape_index}_radiance_{node_id}")
}

fn shape_medium_function_name(shape_index: u32, node_id: u32) -> String {
    format!("wr_shape_{shape_index}_medium_{node_id}")
}

fn shape_surface_function_name(shape_index: u32) -> String {
    format!("wr_shape_{shape_index}_surface")
}

fn scene_constant_literal(
    ops: &DirectQueryOps<'_>,
    expr: &SceneValueExpr,
) -> Result<String, QueryExecError> {
    let value = ops.eval_scene_constant(expr)?;
    kernel_value_literal(&value)
}

fn bounds_center_half(value: &KernelValue) -> Result<([f32; 3], [f32; 3]), QueryExecError> {
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

fn abs_scalar_kernel_value(value: &KernelValue) -> Result<f32, QueryExecError> {
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

fn kernel_value_length(value: &KernelValue) -> Result<f32, QueryExecError> {
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

fn kernel_value_literal(value: &KernelValue) -> Result<String, QueryExecError> {
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

fn kernel_value_type_name(value: &KernelValue) -> Result<String, QueryExecError> {
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

fn format_f32(value: f32) -> String {
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

fn transform_helper_name_for_value(
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

fn repeat_helper_name_for_value(
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

fn repeat_identity_helper_name_for_value(
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

fn rotate_helper_name(value: &KernelValue) -> Result<&'static str, QueryExecError> {
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

fn transform_normal_expr_for_value(
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

fn emit_profile_expr(
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

fn scene_named_arg_value<'a>(
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

fn scene_value_list_len(value: &SceneValueExpr) -> Result<usize, QueryExecError> {
    match value {
        SceneValueExpr::List(items) => Ok(items.len()),
        other => Err(QueryExecError::Unsupported {
            message: format!("expected scene list constant, found {other:?}"),
        }),
    }
}

fn scene_named_arg_literal(
    ops: &DirectQueryOps<'_>,
    args: &[crate::scene_ir::SceneArgExpr],
    name: &str,
) -> Result<String, QueryExecError> {
    scene_constant_literal(ops, scene_named_arg_value(args, name)?)
}

fn emit_field_primitive_call(
    ops: &DirectQueryOps<'_>,
    primitive: hir::FieldPrimitive,
    args: &[crate::scene_ir::SceneArgExpr],
    point_expr: &str,
) -> Result<String, QueryExecError> {
    Ok(match primitive {
        hir::FieldPrimitive::Sphere => format!(
            "wr_sphere({}, {})",
            point_expr,
            scene_named_arg_literal(ops, args, "radius")?
        ),
        hir::FieldPrimitive::Box => format!(
            "wr_box({}, {})",
            point_expr,
            scene_named_arg_literal(ops, args, "half").or_else(|_| scene_named_arg_literal(
                ops,
                args,
                "half_size"
            ))?
        ),
        hir::FieldPrimitive::Capsule => format!(
            "wr_capsule({}, {}, {}, {})",
            point_expr,
            scene_named_arg_literal(ops, args, "a")?,
            scene_named_arg_literal(ops, args, "b")?,
            scene_named_arg_literal(ops, args, "radius")?
        ),
        hir::FieldPrimitive::Cylinder => format!(
            "wr_cylinder({}, {}, {})",
            point_expr,
            scene_named_arg_literal(ops, args, "radius")?,
            scene_named_arg_literal(ops, args, "half_height")?
        ),
        hir::FieldPrimitive::Plane => format!(
            "wr_plane({}, {}, {})",
            point_expr,
            scene_named_arg_literal(ops, args, "normal")?,
            scene_named_arg_literal(ops, args, "offset")?
        ),
        hir::FieldPrimitive::Torus => format!(
            "wr_torus({}, {}, {})",
            point_expr,
            scene_named_arg_literal(ops, args, "major_radius")?,
            scene_named_arg_literal(ops, args, "minor_radius")?
        ),
        hir::FieldPrimitive::RoundedBox => format!(
            "wr_rounded_box({}, {}, {})",
            point_expr,
            scene_named_arg_literal(ops, args, "half")?,
            scene_named_arg_literal(ops, args, "radius")?
        ),
        hir::FieldPrimitive::Ellipsoid => format!(
            "wr_ellipsoid({}, {})",
            point_expr,
            scene_named_arg_literal(ops, args, "radii")?
        ),
        hir::FieldPrimitive::Cone => format!(
            "wr_cone({}, {}, {})",
            point_expr,
            scene_named_arg_literal(ops, args, "radius")?,
            scene_named_arg_literal(ops, args, "half_height")?
        ),
        hir::FieldPrimitive::CappedCone => format!(
            "wr_capped_cone({}, {}, {}, {})",
            point_expr,
            scene_named_arg_literal(ops, args, "radius_bottom")?,
            scene_named_arg_literal(ops, args, "radius_top")?,
            scene_named_arg_literal(ops, args, "half_height")?
        ),
        hir::FieldPrimitive::BoxFrame => format!(
            "wr_box_frame({}, {}, {})",
            point_expr,
            scene_named_arg_literal(ops, args, "half")?,
            scene_named_arg_literal(ops, args, "thickness")?
        ),
        hir::FieldPrimitive::Slab => format!(
            "wr_slab({}, {})",
            point_expr,
            scene_named_arg_literal(ops, args, "thickness")?
        ),
        hir::FieldPrimitive::TrianglePrism => format!(
            "wr_triangle_prism({}, {}, {})",
            point_expr,
            scene_named_arg_literal(ops, args, "half")?,
            scene_named_arg_literal(ops, args, "half_height")?
        ),
        hir::FieldPrimitive::HexPrism => format!(
            "wr_hex_prism({}, {}, {})",
            point_expr,
            scene_named_arg_literal(ops, args, "half")?,
            scene_named_arg_literal(ops, args, "half_height")?
        ),
    })
}

fn emit_shape_distance_function(
    ctx: &QueryExecContext,
    scene_index: &ShaderSceneIndex,
    shape_name: &SmolStr,
    shape_index: u32,
    record: &crate::scene_ir::ShapeNodeRecord,
    out: &mut String,
) -> Result<(), QueryExecError> {
    let fn_name = shape_distance_function_name(shape_index, record.id.0);
    writeln!(out, "fn {fn_name}(point: vec3<f32>) -> f32 {{").ok();
    match record.kind {
        ShapeNodeKindSummary::Use => {
            let target = record.target.as_ref().expect("shape use target");
            let target_index = scene_index.shape(target)?;
            let target_scene =
                ctx.scene
                    .shapes
                    .get(target)
                    .ok_or_else(|| QueryExecError::Unsupported {
                        message: format!(
                            "shape '{}' is missing target '{}' during WGSL emission",
                            shape_name, target
                        ),
                    })?;
            writeln!(
                out,
                "  return {}(point);",
                shape_distance_function_name(target_index, target_scene.root_node_id.0)
            )
            .ok();
        }
        ShapeNodeKindSummary::Leaf => {
            let leaf_id = record.leaf.expect("shape leaf id");
            let leaf =
                ctx.shape_leaf(shape_name, leaf_id)
                    .ok_or_else(|| QueryExecError::Unsupported {
                        message: format!(
                            "shape '{}' is missing leaf {} during WGSL emission",
                            shape_name, leaf_id.0
                        ),
                    })?;
            writeln!(
                out,
                "  return field_distance_dispatch({}, point);",
                scene_index.field(&leaf.field)?
            )
            .ok();
        }
        ShapeNodeKindSummary::Union => {
            writeln!(out, "  var current: f32 = 1000000.0;").ok();
            for child in &record.children {
                writeln!(
                    out,
                    "  current = wr_field_union(current, {}(point));",
                    shape_distance_function_name(shape_index, child.0)
                )
                .ok();
            }
            writeln!(out, "  return current;").ok();
        }
        ShapeNodeKindSummary::Intersection => {
            if let Some(first) = record.children.first() {
                writeln!(
                    out,
                    "  var current: f32 = {}(point);",
                    shape_distance_function_name(shape_index, first.0)
                )
                .ok();
                for child in record.children.iter().skip(1) {
                    writeln!(
                        out,
                        "  current = wr_field_intersection(current, {}(point));",
                        shape_distance_function_name(shape_index, child.0)
                    )
                    .ok();
                }
                writeln!(out, "  return current;").ok();
            } else {
                writeln!(out, "  return 1000000.0;").ok();
            }
        }
        ShapeNodeKindSummary::Subtract => {
            let left = record.children.first().copied();
            let right = record.children.get(1).copied();
            if let (Some(left), Some(right)) = (left, right) {
                writeln!(
                    out,
                    "  return wr_field_subtract({}(point), {}(point));",
                    shape_distance_function_name(shape_index, left.0),
                    shape_distance_function_name(shape_index, right.0)
                )
                .ok();
            } else {
                writeln!(out, "  return 1000000.0;").ok();
            }
        }
    }
    writeln!(out, "}}\n").ok();
    Ok(())
}

fn emit_shape_normal_function(
    ctx: &QueryExecContext,
    scene_index: &ShaderSceneIndex,
    shape_name: &SmolStr,
    shape_index: u32,
    record: &crate::scene_ir::ShapeNodeRecord,
    out: &mut String,
) -> Result<(), QueryExecError> {
    let fn_name = shape_normal_function_name(shape_index, record.id.0);
    writeln!(
        out,
        "fn {fn_name}(point: vec3<f32>) -> CertifiedNormalSample {{"
    )
    .ok();
    match record.kind {
        ShapeNodeKindSummary::Use => {
            let target = record.target.as_ref().expect("shape use target");
            let target_index = scene_index.shape(target)?;
            let target_scene =
                ctx.scene
                    .shapes
                    .get(target)
                    .ok_or_else(|| QueryExecError::Unsupported {
                        message: format!(
                            "shape '{}' is missing target '{}' during WGSL emission",
                            shape_name, target
                        ),
                    })?;
            writeln!(
                out,
                "  return {}(point);",
                shape_normal_function_name(target_index, target_scene.root_node_id.0)
            )
            .ok();
        }
        ShapeNodeKindSummary::Leaf => {
            let leaf_id = record.leaf.expect("shape leaf id");
            let leaf =
                ctx.shape_leaf(shape_name, leaf_id)
                    .ok_or_else(|| QueryExecError::Unsupported {
                        message: format!(
                            "shape '{}' is missing leaf {} during WGSL emission",
                            shape_name, leaf_id.0
                        ),
                    })?;
            writeln!(
                out,
                "  let field_sample = field_normal_dispatch_sample({}, point);",
                scene_index.field(&leaf.field)?
            )
            .ok();
            writeln!(
                out,
                "  if (field_sample.available == 0u) {{ return field_sample; }}"
            )
            .ok();
            writeln!(
                out,
                "  return wr_feature_normal_sample(field_sample.normal);"
            )
            .ok();
        }
        ShapeNodeKindSummary::Union
        | ShapeNodeKindSummary::Intersection
        | ShapeNodeKindSummary::Subtract => {
            writeln!(out, "  return wr_unavailable_normal_sample();").ok();
        }
    }
    writeln!(out, "}}\n").ok();
    Ok(())
}

fn emit_shape_winner_function(
    ctx: &QueryExecContext,
    scene_index: &ShaderSceneIndex,
    shape_name: &SmolStr,
    shape_index: u32,
    record: &crate::scene_ir::ShapeNodeRecord,
    out: &mut String,
) -> Result<(), QueryExecError> {
    let fn_name = shape_winner_function_name(shape_index, record.id.0);
    writeln!(out, "fn {fn_name}(point: vec3<f32>) -> ShapeWinner {{").ok();
    let scene = ctx.scene.shapes.get(shape_name).expect("shape scene");
    match record.kind {
        ShapeNodeKindSummary::Use => {
            let target = record.target.as_ref().expect("shape use target");
            let target_index = scene_index.shape(target)?;
            let target_scene =
                ctx.scene
                    .shapes
                    .get(target)
                    .ok_or_else(|| QueryExecError::Unsupported {
                        message: format!(
                            "shape '{}' is missing target '{}' during WGSL emission",
                            shape_name, target
                        ),
                    })?;
            writeln!(
                out,
                "  return {}(point);",
                shape_winner_function_name(target_index, target_scene.root_node_id.0)
            )
            .ok();
        }
        ShapeNodeKindSummary::Leaf => {
            let leaf_id = record.leaf.expect("shape leaf id");
            let leaf =
                ctx.shape_leaf(shape_name, leaf_id)
                    .ok_or_else(|| QueryExecError::Unsupported {
                        message: format!(
                            "shape '{}' is missing leaf {} during WGSL emission",
                            shape_name, leaf_id.0
                        ),
                    })?;
            let leaf_scene_index = scene_index.shape(shape_name)?;
            let field_index = scene_index.field(&leaf.field)?;
            writeln!(
                out,
                "  return ShapeWinner(field_distance_dispatch({field_index}u, point), {}u, 1u, {}u, {}u, {field_index}u);",
                leaf.feature_id,
                leaf_scene_index,
                leaf_id.0
            )
            .ok();
        }
        ShapeNodeKindSummary::Union => {
            emit_shape_merge_winner(
                record,
                out,
                shape_index,
                scene
                    .provenance_record(record.id)
                    .and_then(|record| match record.policy {
                        ShapeNodeProvenancePolicy::Union(policy) => Some(policy),
                        _ => None,
                    })
                    .unwrap_or(ShapeMergeProvenancePolicy::Nearest),
                true,
            )?;
        }
        ShapeNodeKindSummary::Intersection => {
            emit_shape_merge_winner(
                record,
                out,
                shape_index,
                scene
                    .provenance_record(record.id)
                    .and_then(|record| match record.policy {
                        ShapeNodeProvenancePolicy::Intersection(policy) => Some(policy),
                        _ => None,
                    })
                    .unwrap_or(ShapeMergeProvenancePolicy::Nearest),
                false,
            )?;
        }
        ShapeNodeKindSummary::Subtract => {
            let left = record.children.first().copied();
            let right = record.children.get(1).copied();
            let policy = scene
                .provenance_record(record.id)
                .and_then(|record| match record.policy {
                    ShapeNodeProvenancePolicy::Subtract(policy) => Some(policy),
                    _ => None,
                })
                .unwrap_or(ShapeSubtractProvenancePolicy::Left);
            if let (Some(left), Some(right)) = (left, right) {
                writeln!(
                    out,
                    "  let left = {}(point);",
                    shape_winner_function_name(shape_index, left.0)
                )
                .ok();
                writeln!(
                    out,
                    "  let right = {}(point);",
                    shape_winner_function_name(shape_index, right.0)
                )
                .ok();
                writeln!(out, "  let neg_right = -right.distance;").ok();
                writeln!(out, "  if (left.distance >= neg_right) {{ return left; }}").ok();
                let chooser = match policy {
                    ShapeSubtractProvenancePolicy::Left => "left",
                    ShapeSubtractProvenancePolicy::Right => "right",
                };
                writeln!(
                    out,
                    "  return ShapeWinner(neg_right, {chooser}.feature_id, {chooser}.has_leaf, {chooser}.leaf_scene_index, {chooser}.leaf_id, {chooser}.field_index);"
                )
                .ok();
            } else {
                writeln!(out, "  return wr_default_shape_winner();").ok();
            }
        }
    }
    writeln!(out, "}}\n").ok();
    Ok(())
}

fn emit_shape_merge_winner(
    record: &crate::scene_ir::ShapeNodeRecord,
    out: &mut String,
    shape_index: u32,
    policy: ShapeMergeProvenancePolicy,
    is_union: bool,
) -> Result<(), QueryExecError> {
    if let Some(first) = record.children.first() {
        writeln!(
            out,
            "  var current = {}(point);",
            shape_winner_function_name(shape_index, first.0)
        )
        .ok();
        for (index, child) in record.children.iter().skip(1).enumerate() {
            let next_name = format!("next_{index}");
            writeln!(
                out,
                "  let {next_name} = {}(point);",
                shape_winner_function_name(shape_index, child.0),
            )
            .ok();
            match policy {
                ShapeMergeProvenancePolicy::Ordered => {
                    writeln!(
                        out,
                        "  current.distance = {}(current.distance, {next_name}.distance);",
                        if is_union {
                            "wr_field_union"
                        } else {
                            "wr_field_intersection"
                        }
                    )
                    .ok();
                }
                ShapeMergeProvenancePolicy::Nearest => {
                    writeln!(
                        out,
                        "  if ({next_name}.distance {} current.distance) {{ current = {next_name}; }}",
                        if is_union { "<" } else { ">" }
                    )
                    .ok();
                }
            }
        }
        writeln!(out, "  return current;").ok();
    } else {
        writeln!(out, "  return wr_default_shape_winner();").ok();
    }
    Ok(())
}

fn emit_shape_radiance_function(
    ctx: &QueryExecContext,
    scene_index: &ShaderSceneIndex,
    shape_name: &SmolStr,
    shape_index: u32,
    record: &crate::scene_ir::ShapeNodeRecord,
    out: &mut String,
) -> Result<(), QueryExecError> {
    let fn_name = shape_radiance_function_name(shape_index, record.id.0);
    writeln!(
        out,
        "fn {fn_name}(point: vec3<f32>, direction: vec3<f32>) -> vec3<f32> {{"
    )
    .ok();
    match record.kind {
        ShapeNodeKindSummary::Use => {
            let target = record.target.as_ref().expect("shape use target");
            let target_index = scene_index.shape(target)?;
            let target_scene =
                ctx.scene
                    .shapes
                    .get(target)
                    .ok_or_else(|| QueryExecError::Unsupported {
                        message: format!(
                            "shape '{}' is missing target '{}' during WGSL emission",
                            shape_name, target
                        ),
                    })?;
            writeln!(
                out,
                "  return {}(point, direction);",
                shape_radiance_function_name(target_index, target_scene.root_node_id.0)
            )
            .ok();
        }
        ShapeNodeKindSummary::Leaf => {
            let leaf_id = record.leaf.expect("shape leaf id");
            let leaf =
                ctx.shape_leaf(shape_name, leaf_id)
                    .ok_or_else(|| QueryExecError::Unsupported {
                        message: format!(
                            "shape '{}' is missing leaf {} during WGSL emission",
                            shape_name, leaf_id.0
                        ),
                    })?;
            if let Some(radiance) = &leaf.radiance {
                let field_index = scene_index.field(&leaf.field)?;
                writeln!(
                    out,
                    "  let frame = field_local_frame_dispatch({field_index}u, point);"
                )
                .ok();
                writeln!(
                    out,
                    "  return {}(frame.point, direction, {}u);",
                    portable_function_name(radiance),
                    leaf.feature_id
                )
                .ok();
            } else {
                writeln!(out, "  return vec3<f32>(0.0, 0.0, 0.0);").ok();
            }
        }
        ShapeNodeKindSummary::Union | ShapeNodeKindSummary::Intersection => {
            writeln!(out, "  var total = vec3<f32>(0.0, 0.0, 0.0);").ok();
            for child in &record.children {
                writeln!(
                    out,
                    "  total = total + {}(point, direction);",
                    shape_radiance_function_name(shape_index, child.0)
                )
                .ok();
            }
            writeln!(out, "  return total;").ok();
        }
        ShapeNodeKindSummary::Subtract => {
            let left = record.children.first().copied();
            let right = record.children.get(1).copied();
            if let (Some(left), Some(right)) = (left, right) {
                writeln!(
                    out,
                    "  return {}(point, direction) + {}(point, direction);",
                    shape_radiance_function_name(shape_index, left.0),
                    shape_radiance_function_name(shape_index, right.0)
                )
                .ok();
            } else {
                writeln!(out, "  return vec3<f32>(0.0, 0.0, 0.0);").ok();
            }
        }
    }
    writeln!(out, "}}\n").ok();
    Ok(())
}

fn emit_shape_medium_function(
    ctx: &QueryExecContext,
    scene_index: &ShaderSceneIndex,
    _ops: &DirectQueryOps<'_>,
    shape_name: &SmolStr,
    shape_index: u32,
    record: &crate::scene_ir::ShapeNodeRecord,
    out: &mut String,
) -> Result<(), QueryExecError> {
    let fn_name = shape_medium_function_name(shape_index, record.id.0);
    writeln!(out, "fn {fn_name}(point: vec3<f32>) -> Medium {{").ok();
    match record.kind {
        ShapeNodeKindSummary::Use => {
            let target = record.target.as_ref().expect("shape use target");
            let target_index = scene_index.shape(target)?;
            let target_scene =
                ctx.scene
                    .shapes
                    .get(target)
                    .ok_or_else(|| QueryExecError::Unsupported {
                        message: format!(
                            "shape '{}' is missing target '{}' during WGSL emission",
                            shape_name, target
                        ),
                    })?;
            writeln!(
                out,
                "  return {}(point);",
                shape_medium_function_name(target_index, target_scene.root_node_id.0)
            )
            .ok();
        }
        ShapeNodeKindSummary::Leaf => {
            let leaf_id = record.leaf.expect("shape leaf id");
            let leaf =
                ctx.shape_leaf(shape_name, leaf_id)
                    .ok_or_else(|| QueryExecError::Unsupported {
                        message: format!(
                            "shape '{}' is missing leaf {} during WGSL emission",
                            shape_name, leaf_id.0
                        ),
                    })?;
            if let Some(volume) = &leaf.volume {
                let field_index = scene_index.field(&leaf.field)?;
                writeln!(
                    out,
                    "  let frame = field_local_frame_dispatch({field_index}u, point);"
                )
                .ok();
                writeln!(
                    out,
                    "  let surface_distance = field_terminal_distance_dispatch({field_index}u, frame.terminal_node_id, frame.point);"
                )
                .ok();
                writeln!(
                    out,
                    "  return {}(frame.point, surface_distance);",
                    portable_function_name(volume)
                )
                .ok();
            } else {
                writeln!(out, "  return wr_default_medium();").ok();
            }
        }
        ShapeNodeKindSummary::Union | ShapeNodeKindSummary::Intersection => {
            writeln!(out, "  var total = wr_default_medium();").ok();
            for child in &record.children {
                writeln!(
                    out,
                    "  total = wr_combine_medium_values(total, {}(point));",
                    shape_medium_function_name(shape_index, child.0)
                )
                .ok();
            }
            writeln!(out, "  return total;").ok();
        }
        ShapeNodeKindSummary::Subtract => {
            let left = record.children.first().copied();
            let right = record.children.get(1).copied();
            if let (Some(left), Some(right)) = (left, right) {
                writeln!(
                    out,
                    "  return wr_combine_medium_values({}(point), {}(point));",
                    shape_medium_function_name(shape_index, left.0),
                    shape_medium_function_name(shape_index, right.0)
                )
                .ok();
            } else {
                writeln!(out, "  return wr_default_medium();").ok();
            }
        }
    }
    writeln!(out, "}}\n").ok();
    Ok(())
}

fn emit_shape_surface_function(
    ctx: &QueryExecContext,
    _scene_index: &ShaderSceneIndex,
    shape_name: &SmolStr,
    shape_index: u32,
    out: &mut String,
) -> Result<(), QueryExecError> {
    let scene = ctx.scene.shapes.get(shape_name).expect("shape scene");
    writeln!(
        out,
        "fn {}(hit: Hit3) -> Surface {{",
        shape_surface_function_name(shape_index)
    )
    .ok();
    writeln!(out, "  switch hit.feature_id {{").ok();
    for (feature_id, leaf_ref) in &scene.feature_leaves {
        let leaf = ctx
            .shape_leaf(&leaf_ref.scene, leaf_ref.leaf)
            .ok_or_else(|| QueryExecError::Unsupported {
                message: format!(
                    "shape '{}' is missing leaf {} during surface WGSL emission",
                    leaf_ref.scene, leaf_ref.leaf.0
                ),
            })?;
        writeln!(
            out,
            "    case {}u: {{ return {}(hit); }}",
            feature_id,
            portable_function_name(&leaf.material)
        )
        .ok();
    }
    writeln!(out, "    default: {{ return wr_default_surface(); }}").ok();
    writeln!(out, "  }}").ok();
    writeln!(out, "}}\n").ok();
    Ok(())
}

fn emit_pir_function(
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

fn emit_pir_block(
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

fn emit_pir_stmt(
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

fn emit_pir_expr(expr: &pir::ir::PirExpr) -> Result<String, QueryExecError> {
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

fn emit_pir_intrinsic_call(
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

fn render_bounds_expr(
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

fn render_rotate_call(
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

fn render_affine_transform_call(
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

fn render_repeat_call(
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

fn render_instance_array_call(
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

fn render_field_repeat_call(
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

fn portable_function_name(name: &SmolStr) -> String {
    format!("wr_portable_{}", sanitize_ident(name))
}

fn sanitize_ident(name: &SmolStr) -> String {
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

fn pir_type_name(ty: &pir::ir::PirType) -> Result<String, QueryExecError> {
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

fn pir_binary_op(op: hir::BinaryOp) -> &'static str {
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

fn pir_value_to_kernel(value: &pir::ir::PirValue) -> Result<KernelValue, QueryExecError> {
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
