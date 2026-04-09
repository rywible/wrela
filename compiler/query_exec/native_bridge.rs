use crate::kernel::{KernelStructValue, KernelValue};
use crate::portable::{PortableBuiltinAtom, PortableBuiltinType, builtin_record};
use crate::query_exec::cpu::{QueryExecError, kernel_to_runtime, runtime_to_kernel_value};
use crate::query_exec::wgsl::codegen::{
    QueryFlavor, wgsl_dispatch_config_abi, wgsl_item_abi_for_flavor, wgsl_result_abi_for_flavor,
};
use crate::query_exec::wgsl::{
    GeneratedShaderModule, GpuDispatchRequest, dispatch_compiled_shader, dispatch_config,
};
use smol_str::SmolStr;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use wrela_runtime::{
    TypeId, Value as RuntimeValue, wr_bytes_from_string, wr_bytes_len, wr_bytes_to_list,
    wr_class_get, wr_crash, wr_list_get, wr_list_len, wr_str_from_utf8, wr_type_id,
};

type BridgeResult = Result<RuntimeValue, QueryExecError>;

#[unsafe(no_mangle)]
pub extern "C" fn wr_wgsl_world_distance_capture(
    source: RuntimeValue,
    workgroup_size: RuntimeValue,
    world_shape_indices: RuntimeValue,
    point: RuntimeValue,
) -> RuntimeValue {
    bridge_result(world_query(
        &cached_world_module(source, workgroup_size, WorldBridgeKind::Distance),
        WorldBridgeKind::Distance,
        world_shape_indices,
        &[point],
    ))
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_wgsl_world_normal_capture(
    source: RuntimeValue,
    workgroup_size: RuntimeValue,
    world_shape_indices: RuntimeValue,
    point: RuntimeValue,
) -> RuntimeValue {
    bridge_result(world_query(
        &cached_world_module(source, workgroup_size, WorldBridgeKind::Normal),
        WorldBridgeKind::Normal,
        world_shape_indices,
        &[point],
    ))
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_wgsl_world_trace_capture(
    source: RuntimeValue,
    workgroup_size: RuntimeValue,
    world_shape_indices: RuntimeValue,
    ray: RuntimeValue,
) -> RuntimeValue {
    bridge_result(world_query(
        &cached_world_module(source, workgroup_size, WorldBridgeKind::Trace),
        WorldBridgeKind::Trace,
        world_shape_indices,
        &[ray],
    ))
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_wgsl_world_surface_capture(
    source: RuntimeValue,
    workgroup_size: RuntimeValue,
    world_shape_indices: RuntimeValue,
    hit: RuntimeValue,
) -> RuntimeValue {
    bridge_result(world_query(
        &cached_world_module(source, workgroup_size, WorldBridgeKind::Surface),
        WorldBridgeKind::Surface,
        world_shape_indices,
        &[hit],
    ))
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_wgsl_world_radiance_capture(
    source: RuntimeValue,
    workgroup_size: RuntimeValue,
    world_shape_indices: RuntimeValue,
    sample: RuntimeValue,
) -> RuntimeValue {
    bridge_result(world_query(
        &cached_world_module(source, workgroup_size, WorldBridgeKind::Radiance),
        WorldBridgeKind::Radiance,
        world_shape_indices,
        &[sample],
    ))
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_wgsl_world_medium_capture(
    source: RuntimeValue,
    workgroup_size: RuntimeValue,
    world_shape_indices: RuntimeValue,
    point: RuntimeValue,
) -> RuntimeValue {
    bridge_result(world_query(
        &cached_world_module(source, workgroup_size, WorldBridgeKind::Medium),
        WorldBridgeKind::Medium,
        world_shape_indices,
        &[point],
    ))
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_wgsl_field_distance_batch_queries(
    source: RuntimeValue,
    workgroup_size: RuntimeValue,
    capture_index: RuntimeValue,
    points: RuntimeValue,
) -> RuntimeValue {
    bridge_result(batch_query(
        &cached_batch_module(source, workgroup_size, BatchBridgeKind::FieldDistance),
        0,
        capture_index,
        BatchBridgeKind::FieldDistance,
        points,
    ))
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_wgsl_shape_distance_batch_queries(
    source: RuntimeValue,
    workgroup_size: RuntimeValue,
    capture_index: RuntimeValue,
    points: RuntimeValue,
) -> RuntimeValue {
    bridge_result(batch_query(
        &cached_batch_module(source, workgroup_size, BatchBridgeKind::ShapeDistance),
        1,
        capture_index,
        BatchBridgeKind::ShapeDistance,
        points,
    ))
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_wgsl_field_normal_batch_queries(
    source: RuntimeValue,
    workgroup_size: RuntimeValue,
    capture_index: RuntimeValue,
    points: RuntimeValue,
) -> RuntimeValue {
    bridge_result(batch_query(
        &cached_batch_module(source, workgroup_size, BatchBridgeKind::FieldNormal),
        0,
        capture_index,
        BatchBridgeKind::FieldNormal,
        points,
    ))
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_wgsl_shape_normal_batch_queries(
    source: RuntimeValue,
    workgroup_size: RuntimeValue,
    capture_index: RuntimeValue,
    points: RuntimeValue,
) -> RuntimeValue {
    bridge_result(batch_query(
        &cached_batch_module(source, workgroup_size, BatchBridgeKind::ShapeNormal),
        1,
        capture_index,
        BatchBridgeKind::ShapeNormal,
        points,
    ))
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_wgsl_shape_trace_batch_queries(
    source: RuntimeValue,
    workgroup_size: RuntimeValue,
    capture_index: RuntimeValue,
    rays: RuntimeValue,
) -> RuntimeValue {
    bridge_result(batch_query(
        &cached_batch_module(source, workgroup_size, BatchBridgeKind::ShapeTrace),
        1,
        capture_index,
        BatchBridgeKind::ShapeTrace,
        rays,
    ))
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_wgsl_shape_surface_batch_queries(
    source: RuntimeValue,
    workgroup_size: RuntimeValue,
    capture_index: RuntimeValue,
    hits: RuntimeValue,
) -> RuntimeValue {
    bridge_result(batch_query(
        &cached_batch_module(source, workgroup_size, BatchBridgeKind::ShapeSurface),
        1,
        capture_index,
        BatchBridgeKind::ShapeSurface,
        hits,
    ))
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_wgsl_shape_occluded_batch_queries(
    source: RuntimeValue,
    workgroup_size: RuntimeValue,
    capture_index: RuntimeValue,
    rays: RuntimeValue,
) -> RuntimeValue {
    bridge_result(batch_query(
        &cached_batch_module(source, workgroup_size, BatchBridgeKind::ShapeOccluded),
        1,
        capture_index,
        BatchBridgeKind::ShapeOccluded,
        rays,
    ))
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum WorldBridgeKind {
    Distance,
    Normal,
    Trace,
    Surface,
    Radiance,
    Medium,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum BatchBridgeKind {
    FieldDistance,
    ShapeDistance,
    FieldNormal,
    ShapeNormal,
    ShapeTrace,
    ShapeSurface,
    ShapeOccluded,
}

fn bridge_result(result: BridgeResult) -> RuntimeValue {
    match result {
        Ok(value) => value,
        Err(err) => {
            let message = err.to_string();
            wr_crash(wr_str_from_utf8(message.as_ptr(), message.len()))
        }
    }
}

fn cached_world_module(
    source: RuntimeValue,
    workgroup_size: RuntimeValue,
    kind: WorldBridgeKind,
) -> Result<GeneratedShaderModule, QueryExecError> {
    static MODULES: OnceLock<
        Mutex<
            HashMap<(WorldBridgeKind, String, u32), Result<GeneratedShaderModule, QueryExecError>>,
        >,
    > = OnceLock::new();
    let cache = MODULES.get_or_init(|| Mutex::new(HashMap::new()));
    let source = runtime_string(source)?;
    let workgroup_size = runtime_workgroup_size(workgroup_size)?;
    let key = (kind, source.clone(), workgroup_size);
    let mut guard = cache.lock().unwrap_or_else(|poison| poison.into_inner());
    let entry = guard
        .entry(key)
        .or_insert_with(|| world_module_from_parts(source, workgroup_size, kind));
    entry.clone()
}

fn cached_batch_module(
    source: RuntimeValue,
    workgroup_size: RuntimeValue,
    kind: BatchBridgeKind,
) -> Result<GeneratedShaderModule, QueryExecError> {
    static MODULES: OnceLock<
        Mutex<
            HashMap<(BatchBridgeKind, String, u32), Result<GeneratedShaderModule, QueryExecError>>,
        >,
    > = OnceLock::new();
    let cache = MODULES.get_or_init(|| Mutex::new(HashMap::new()));
    let source = runtime_string(source)?;
    let workgroup_size = runtime_workgroup_size(workgroup_size)?;
    let key = (kind, source.clone(), workgroup_size);
    let mut guard = cache.lock().unwrap_or_else(|poison| poison.into_inner());
    let entry = guard
        .entry(key)
        .or_insert_with(|| batch_module_from_parts(source, workgroup_size, kind));
    entry.clone()
}

fn make_world_module(
    source: RuntimeValue,
    workgroup_size: RuntimeValue,
    kind: WorldBridgeKind,
) -> Result<GeneratedShaderModule, QueryExecError> {
    world_module_from_parts(
        runtime_string(source)?,
        runtime_workgroup_size(workgroup_size)?,
        kind,
    )
}

fn make_batch_module(
    source: RuntimeValue,
    workgroup_size: RuntimeValue,
    kind: BatchBridgeKind,
) -> Result<GeneratedShaderModule, QueryExecError> {
    batch_module_from_parts(
        runtime_string(source)?,
        runtime_workgroup_size(workgroup_size)?,
        kind,
    )
}

fn world_module_from_parts(
    source: String,
    workgroup_size: u32,
    kind: WorldBridgeKind,
) -> Result<GeneratedShaderModule, QueryExecError> {
    let flavor = kind.flavor();
    Ok(GeneratedShaderModule {
        source,
        workgroup_size,
        dispatch_abi: wgsl_dispatch_config_abi(),
        item_abi: wgsl_item_abi_for_flavor(flavor)?,
        result_abi: wgsl_result_abi_for_flavor(flavor)?,
    })
}

fn batch_module_from_parts(
    source: String,
    workgroup_size: u32,
    kind: BatchBridgeKind,
) -> Result<GeneratedShaderModule, QueryExecError> {
    let flavor = kind.flavor();
    Ok(GeneratedShaderModule {
        source,
        workgroup_size,
        dispatch_abi: wgsl_dispatch_config_abi(),
        item_abi: wgsl_item_abi_for_flavor(flavor)?,
        result_abi: wgsl_result_abi_for_flavor(flavor)?,
    })
}

fn world_query(
    module: &Result<GeneratedShaderModule, QueryExecError>,
    kind: WorldBridgeKind,
    world_shape_indices: RuntimeValue,
    args: &[RuntimeValue],
) -> BridgeResult {
    let module = module.clone()?;
    let shape_indices = runtime_u32_list(world_shape_indices)?;
    let item = kind.item_from_runtime(args)?;
    let result = dispatch_compiled_shader(
        &module,
        GpuDispatchRequest {
            dispatch: dispatch_config(
                2,
                0,
                1,
                shape_indices.len() as u32,
                matches!(kind, WorldBridgeKind::Surface),
                matches!(kind, WorldBridgeKind::Radiance),
                matches!(kind, WorldBridgeKind::Medium),
            ),
            items: vec![item],
            world_shape_indices: shape_indices,
        },
    )?
    .into_iter()
    .next()
    .ok_or_else(|| QueryExecError::Unsupported {
        message: "native WGSL bridge produced no world result".to_string(),
    })?;
    kernel_to_runtime(&result)
}

fn batch_query(
    module: &Result<GeneratedShaderModule, QueryExecError>,
    capture_kind: u32,
    capture_index: RuntimeValue,
    kind: BatchBridgeKind,
    items: RuntimeValue,
) -> BridgeResult {
    let module = module.clone()?;
    let capture_index = runtime_int(capture_index, "capture_index")?;
    let capture_index = u32::try_from(capture_index).map_err(|_| QueryExecError::Unsupported {
        message: format!("invalid capture index {capture_index}"),
    })?;
    let items = kind.items_from_runtime(items)?;
    let values = dispatch_compiled_shader(
        &module,
        GpuDispatchRequest {
            dispatch: dispatch_config(
                capture_kind,
                capture_index,
                items.len() as u32,
                0,
                false,
                false,
                false,
            ),
            items,
            world_shape_indices: Vec::new(),
        },
    )?;
    kernel_array_to_runtime(&values)
}

fn kernel_array_to_runtime(values: &[KernelValue]) -> Result<RuntimeValue, QueryExecError> {
    let list = wrela_runtime::wr_list_new(0);
    for value in values {
        wrela_runtime::wr_list_push(list, kernel_to_runtime(value)?);
    }
    Ok(list)
}

fn runtime_string(value: RuntimeValue) -> Result<String, QueryExecError> {
    if wr_type_id(value) as u32 != TypeId::String as u32 {
        return Err(QueryExecError::TypeMismatch {
            expected: "String".to_string(),
            found: format!("type id {}", wr_type_id(value)),
        });
    }
    let bytes = wr_bytes_from_string(value);
    let len = runtime_int(wr_bytes_len(bytes), "bytes length")?;
    let list = wr_bytes_to_list(bytes);
    let mut out = Vec::with_capacity(len as usize);
    for index in 0..len as usize {
        let value = wr_list_get(list, index);
        let byte = runtime_int(value, "byte")?;
        out.push(u8::try_from(byte).map_err(|_| QueryExecError::Unsupported {
            message: format!("invalid UTF-8 byte value {byte}"),
        })?);
    }
    String::from_utf8(out).map_err(|err| QueryExecError::Unsupported {
        message: format!("invalid shader UTF-8: {err}"),
    })
}

fn runtime_u32_list(value: RuntimeValue) -> Result<Vec<u32>, QueryExecError> {
    let len = runtime_list_len(value)?;
    let mut out = Vec::with_capacity(len);
    for index in 0..len {
        let item = wr_list_get(value, index);
        let raw = runtime_int(item, "u32 list item")?;
        out.push(u32::try_from(raw).map_err(|_| QueryExecError::Unsupported {
            message: format!("invalid u32 list item {raw}"),
        })?);
    }
    Ok(out)
}

fn runtime_list_len(value: RuntimeValue) -> Result<usize, QueryExecError> {
    if wr_type_id(value) as u32 != TypeId::List as u32 {
        return Err(QueryExecError::TypeMismatch {
            expected: "List".to_string(),
            found: format!("type id {}", wr_type_id(value)),
        });
    }
    let len = runtime_int(wr_list_len(value), "list length")?;
    usize::try_from(len).map_err(|_| QueryExecError::Unsupported {
        message: format!("invalid list length {len}"),
    })
}

fn runtime_int(value: RuntimeValue, label: &str) -> Result<i64, QueryExecError> {
    if value.is_int() {
        Ok(value.as_int())
    } else if value.is_float() {
        Ok(value.as_float() as i64)
    } else {
        Err(QueryExecError::TypeMismatch {
            expected: format!("Integer for {label}"),
            found: format!("type id {}", wr_type_id(value)),
        })
    }
}

fn runtime_workgroup_size(value: RuntimeValue) -> Result<u32, QueryExecError> {
    let workgroup_size = runtime_int(value, "workgroup_size")?;
    u32::try_from(workgroup_size).map_err(|_| QueryExecError::Unsupported {
        message: format!("invalid WGSL workgroup size {workgroup_size}"),
    })
}

fn runtime_bool(value: RuntimeValue, label: &str) -> Result<bool, QueryExecError> {
    if value.is_bool() {
        Ok(value.as_bool())
    } else {
        Err(QueryExecError::TypeMismatch {
            expected: format!("Bool for {label}"),
            found: format!("type id {}", wr_type_id(value)),
        })
    }
}

fn runtime_f32(value: RuntimeValue, label: &str) -> Result<f32, QueryExecError> {
    if value.is_float() {
        Ok(value.as_float() as f32)
    } else if value.is_int() {
        Ok(value.as_int() as f32)
    } else {
        Err(QueryExecError::TypeMismatch {
            expected: format!("F32 for {label}"),
            found: format!("type id {}", wr_type_id(value)),
        })
    }
}

fn runtime_to_builtin_record_value(
    value: RuntimeValue,
    name: &str,
) -> Result<KernelValue, QueryExecError> {
    let record = builtin_record(name).ok_or_else(|| QueryExecError::Unsupported {
        message: format!("unknown builtin record '{name}'"),
    })?;
    let mut fields = Vec::with_capacity(record.fields.len());
    for field in record.fields {
        let field_value = wr_class_get(value, field.name.as_ptr(), field.name.len());
        fields.push((
            SmolStr::new(field.name),
            runtime_to_builtin_type(field_value, field.ty, field.name)?,
        ));
    }
    Ok(KernelValue::Struct(KernelStructValue {
        name: SmolStr::new(name),
        fields,
    }))
}

fn runtime_to_builtin_type(
    value: RuntimeValue,
    ty: PortableBuiltinType,
    label: &str,
) -> Result<KernelValue, QueryExecError> {
    match ty {
        PortableBuiltinType::Atom(atom) => match atom {
            PortableBuiltinAtom::Bool => Ok(KernelValue::Bool(runtime_bool(value, label)?)),
            PortableBuiltinAtom::I32 => Ok(KernelValue::I32(
                i32::try_from(runtime_int(value, label)?).map_err(|_| {
                    QueryExecError::Unsupported {
                        message: format!("invalid i32 for {label}"),
                    }
                })?,
            )),
            PortableBuiltinAtom::U32 => Ok(KernelValue::U32(
                u32::try_from(runtime_int(value, label)?).map_err(|_| {
                    QueryExecError::Unsupported {
                        message: format!("invalid u32 for {label}"),
                    }
                })?,
            )),
            PortableBuiltinAtom::F32 => Ok(KernelValue::F32(runtime_f32(value, label)?)),
            PortableBuiltinAtom::Vec2
            | PortableBuiltinAtom::Vec3
            | PortableBuiltinAtom::Vec4
            | PortableBuiltinAtom::Mat3
            | PortableBuiltinAtom::Mat4
            | PortableBuiltinAtom::Quat => runtime_to_kernel_value(value),
        },
        PortableBuiltinType::Named(name) => runtime_to_builtin_record_value(value, name),
    }
}

impl WorldBridgeKind {
    fn flavor(self) -> QueryFlavor {
        match self {
            Self::Distance => QueryFlavor::WorldDistance,
            Self::Normal => QueryFlavor::WorldNormal,
            Self::Trace => QueryFlavor::WorldTrace,
            Self::Surface => QueryFlavor::WorldSurface,
            Self::Radiance => QueryFlavor::WorldRadiance,
            Self::Medium => QueryFlavor::WorldMedium,
        }
    }

    fn item_from_runtime(self, args: &[RuntimeValue]) -> Result<KernelValue, QueryExecError> {
        match self {
            Self::Distance | Self::Normal | Self::Medium => {
                let [point] = args else {
                    return Err(arity_error("point world query"));
                };
                Ok(KernelValue::Struct(KernelStructValue {
                    name: SmolStr::new("PointQuery"),
                    fields: vec![(
                        SmolStr::new("point"),
                        runtime_to_builtin_type(
                            *point,
                            PortableBuiltinType::Atom(PortableBuiltinAtom::Vec3),
                            "point",
                        )?,
                    )],
                }))
            }
            Self::Trace => {
                let [ray] = args else {
                    return Err(arity_error("trace world query"));
                };
                runtime_to_builtin_record_value(*ray, "RayQuery")
            }
            Self::Surface => {
                let [hit] = args else {
                    return Err(arity_error("surface world query"));
                };
                runtime_to_builtin_record_value(*hit, "Hit3")
            }
            Self::Radiance => {
                let [sample] = args else {
                    return Err(arity_error("radiance world query"));
                };
                runtime_to_builtin_record_value(*sample, "PointDirectionQuery")
            }
        }
    }
}

impl BatchBridgeKind {
    fn flavor(self) -> QueryFlavor {
        match self {
            Self::FieldDistance | Self::ShapeDistance => QueryFlavor::BatchDistance,
            Self::FieldNormal | Self::ShapeNormal => QueryFlavor::BatchNormal,
            Self::ShapeTrace => QueryFlavor::BatchTrace,
            Self::ShapeSurface => QueryFlavor::BatchSurface,
            Self::ShapeOccluded => QueryFlavor::BatchOccluded,
        }
    }

    fn items_from_runtime(self, value: RuntimeValue) -> Result<Vec<KernelValue>, QueryExecError> {
        let len = runtime_list_len(value)?;
        let mut items = Vec::with_capacity(len);
        for index in 0..len {
            let item = wr_list_get(value, index);
            items.push(match self {
                Self::FieldDistance
                | Self::ShapeDistance
                | Self::FieldNormal
                | Self::ShapeNormal => runtime_to_builtin_record_value(item, "PointQuery")?,
                Self::ShapeTrace | Self::ShapeOccluded => {
                    runtime_to_builtin_record_value(item, "RayQuery")?
                }
                Self::ShapeSurface => runtime_to_builtin_record_value(item, "Hit3")?,
            });
        }
        Ok(items)
    }
}

fn arity_error(label: &str) -> QueryExecError {
    QueryExecError::Unsupported {
        message: format!("invalid argument list for {label}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hir;
    use crate::hir::lower as hir_lower;
    use crate::hir::project::load_project;
    use crate::kernel::{KernelValue, lower_batch_query_plan, lower_world_query_plan};
    use crate::parser::ast;
    use crate::parser::ast::AstNode;
    use crate::parser::parse;
    use crate::query_exec::{
        QueryExecContext, execute_batch_query_with_trace_on, execute_world_query_on,
        stable_region_scene_capture_id,
    };
    use crate::query_plan::{
        BatchQueryKind, BatchQueryPlan, DispatchBackend, WorldQueryKind, WorldQueryPlan,
    };
    use std::path::PathBuf;

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("compiler crate should have repo parent")
            .to_path_buf()
    }

    fn preview_context() -> QueryExecContext {
        let entry_path = repo_root().join("language/preview/src/main.wr");
        let project = load_project(&entry_path).expect("load preview project");
        let module = project.module;
        let semantic = hir::semantic::check_module(&module);
        assert!(
            semantic.errors.is_empty(),
            "semantic errors: {:?}",
            semantic.errors
        );
        let (type_errors, type_info) = hir::typeck::check_module_with_info(&module);
        assert!(type_errors.is_empty(), "type errors: {type_errors:?}");
        QueryExecContext::compile(&module, &type_info)
    }

    fn inline_context(source: &str) -> QueryExecContext {
        let node = parse(source);
        let root = ast::Root::cast(node).expect("root");
        let module = hir_lower::lower(root);
        let semantic = hir::semantic::check_module(&module);
        assert!(
            semantic.errors.is_empty(),
            "semantic errors: {:?}",
            semantic.errors
        );
        let (type_errors, type_info) = hir::typeck::check_module_with_info(&module);
        assert!(type_errors.is_empty(), "type errors: {type_errors:?}");
        QueryExecContext::compile(&module, &type_info)
    }

    fn ray_query(origin: [f32; 3], direction: [f32; 3]) -> KernelValue {
        KernelValue::Struct(KernelStructValue {
            name: SmolStr::new("RayQuery"),
            fields: vec![
                (SmolStr::new("origin"), KernelValue::Vec3(origin)),
                (SmolStr::new("direction"), KernelValue::Vec3(direction)),
                (SmolStr::new("max_distance"), KernelValue::F32(6.0)),
                (SmolStr::new("min_step"), KernelValue::F32(0.05)),
                (SmolStr::new("hit_epsilon"), KernelValue::F32(0.001)),
                (SmolStr::new("max_steps"), KernelValue::I32(96)),
            ],
        })
    }

    fn point_query(point: [f32; 3]) -> KernelValue {
        KernelValue::Struct(KernelStructValue {
            name: SmolStr::new("PointQuery"),
            fields: vec![(SmolStr::new("point"), KernelValue::Vec3(point))],
        })
    }

    fn preview_domain() -> KernelValue {
        KernelValue::Struct(KernelStructValue {
            name: SmolStr::new("SceneDomain"),
            fields: vec![
                (
                    SmolStr::new("scene_id"),
                    KernelValue::U32(stable_region_scene_capture_id(&SmolStr::new(
                        "scene_region",
                    ))),
                ),
                (
                    SmolStr::new("spatial"),
                    KernelValue::Struct(KernelStructValue {
                        name: SmolStr::new("SpatialDomainContract"),
                        fields: vec![
                            (SmolStr::new("geometry_detail"), KernelValue::I32(1)),
                            (SmolStr::new("guarantee"), KernelValue::U32(0)),
                        ],
                    }),
                ),
                (
                    SmolStr::new("surface"),
                    KernelValue::Struct(KernelStructValue {
                        name: SmolStr::new("SurfaceDomainContract"),
                        fields: vec![(SmolStr::new("material"), KernelValue::Bool(true))],
                    }),
                ),
                (
                    SmolStr::new("participants"),
                    KernelValue::Struct(KernelStructValue {
                        name: SmolStr::new("ParticipantDomainContract"),
                        fields: vec![
                            (SmolStr::new("radiance"), KernelValue::Bool(true)),
                            (SmolStr::new("media"), KernelValue::Bool(true)),
                        ],
                    }),
                ),
            ],
        })
    }

    #[test]
    fn world_trace_bridge_matches_direct_wgsl_for_preview_probe() {
        let ctx = preview_context();
        let plan = lower_world_query_plan(&WorldQueryPlan::for_query_with_backend(
            WorldQueryKind::Trace,
            DispatchBackend::Wgsl,
        ));
        let shader = crate::query_exec::wgsl::compile_world_shader(&ctx, &plan)
            .expect("compile preview world trace shader");

        let shape_lookup = ctx
            .scene
            .shapes
            .keys()
            .enumerate()
            .map(|(index, name)| (name.clone(), index as u32))
            .collect::<std::collections::HashMap<_, _>>();
        let shape_indices = ctx
            .region_cases
            .iter()
            .find(|case| case.region_name.as_str() == "scene_region")
            .expect("scene region case")
            .shapes_for_detail(1)
            .expect("scene region fine shapes")
            .iter()
            .map(|shape| {
                *shape_lookup
                    .get(shape)
                    .unwrap_or_else(|| panic!("missing shape index for {shape}"))
            })
            .collect::<Vec<_>>();
        let shape_indices_runtime = wrela_runtime::wr_list_new(0);
        for index in shape_indices {
            wrela_runtime::wr_list_push(
                shape_indices_runtime,
                RuntimeValue::from_int(i64::from(index)),
            );
        }

        let ray = ray_query([0.0, 0.1, 2.7], [-0.405183, -0.375170, -0.833711]);
        let bridge_hit = wr_wgsl_world_trace_capture(
            wr_str_from_utf8(shader.source.as_ptr(), shader.source.len()),
            RuntimeValue::from_int(i64::from(shader.workgroup_size)),
            shape_indices_runtime,
            kernel_to_runtime(&ray).expect("ray runtime"),
        );
        let bridge_hit = runtime_to_builtin_record_value(bridge_hit, "Hit3").expect("bridge hit");

        let direct_hit = execute_world_query_on(
            &ctx,
            DispatchBackend::Wgsl,
            &plan,
            &[
                KernelValue::Capture(SmolStr::new("scene_region")),
                preview_domain(),
                ray,
            ],
        )
        .expect("direct world trace");

        assert_eq!(bridge_hit, direct_hit);
    }

    #[test]
    fn field_distance_batch_bridge_matches_direct_wgsl_distance_records() {
        let ctx = inline_context(
            r#"
field exact distance scene_field(p: Vec3) -> F32 {
    sphere(radius = 1.0)
}
"#,
        );
        let plan = lower_batch_query_plan(&BatchQueryPlan::for_field_query(
            BatchQueryKind::Distance,
            crate::query_plan::CaptureKind::Field,
            DispatchBackend::Wgsl,
            None,
        ));
        let shader = crate::query_exec::wgsl::compile_batch_shader(&ctx, &plan)
            .expect("compile field distance batch shader");
        let field_index = ctx
            .scene
            .fields
            .keys()
            .enumerate()
            .find_map(|(index, name)| (name.as_str() == "scene_field").then_some(index as i64))
            .expect("scene_field index");

        let points = vec![point_query([0.0, 0.0, 2.0]), point_query([0.0, 0.0, 3.0])];
        let runtime_points = wrela_runtime::wr_list_new(0);
        for point in &points {
            wrela_runtime::wr_list_push(
                runtime_points,
                kernel_to_runtime(point).expect("runtime point"),
            );
        }

        let bridge_values = wr_wgsl_field_distance_batch_queries(
            wr_str_from_utf8(shader.source.as_ptr(), shader.source.len()),
            RuntimeValue::from_int(i64::from(shader.workgroup_size)),
            RuntimeValue::from_int(field_index),
            runtime_points,
        );
        assert_eq!(runtime_list_len(bridge_values).expect("distance len"), 2);

        let mut bridge_results = Vec::new();
        for index in 0..2 {
            bridge_results.push(
                runtime_to_builtin_record_value(
                    wr_list_get(bridge_values, index),
                    "DistanceResult",
                )
                .unwrap_or_else(|err| panic!("bridge distance {index} decode failed: {err:?}")),
            );
        }

        let (direct_values, _) = execute_batch_query_with_trace_on(
            &ctx,
            DispatchBackend::Wgsl,
            &plan,
            &[
                KernelValue::Capture(SmolStr::new("scene_field")),
                KernelValue::Array(points),
            ],
        )
        .expect("direct distance batch");
        let KernelValue::Array(direct_values) = direct_values else {
            panic!("expected direct WGSL distance result array");
        };

        assert_eq!(bridge_results, direct_values);
    }

    #[test]
    fn shape_trace_batch_bridge_matches_direct_wgsl_and_returns_dense_hits() {
        let ctx = inline_context(
            r#"
field exact distance scene_field(p: Vec3) -> F32 {
    sphere(radius = 1.0)
}

material scene_surface(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(0.25, 0.35, 0.45),
        roughness=0.5,
        metalness=0.1,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

shape scene_shape {
    field = scene_field
    material = scene_surface
    payload = Payload(
        entity_id=u32(11),
        material_id=u32(22),
        actor=ActorHandle(id=u32(33), generation=u32(0))
    )
}
"#,
        );
        let plan = lower_batch_query_plan(&BatchQueryPlan::for_shape_query(
            BatchQueryKind::Trace,
            DispatchBackend::Wgsl,
            None,
        ));
        let shader = crate::query_exec::wgsl::compile_batch_shader(&ctx, &plan)
            .expect("compile trace batch shader");
        let shape_index = ctx
            .scene
            .shapes
            .keys()
            .enumerate()
            .find_map(|(index, name)| (name.as_str() == "scene_shape").then_some(index as i64))
            .expect("scene_shape index");

        let rays = vec![
            ray_query([0.0, 0.0, 3.0], [0.0, 0.0, -1.0]),
            ray_query([0.0, 0.0, 3.0], [0.0, 1.0, 0.0]),
        ];
        let runtime_rays = wrela_runtime::wr_list_new(0);
        for ray in &rays {
            wrela_runtime::wr_list_push(runtime_rays, kernel_to_runtime(ray).expect("runtime ray"));
        }

        let bridge_hits = wr_wgsl_shape_trace_batch_queries(
            wr_str_from_utf8(shader.source.as_ptr(), shader.source.len()),
            RuntimeValue::from_int(i64::from(shader.workgroup_size)),
            RuntimeValue::from_int(shape_index),
            runtime_rays,
        );
        assert_eq!(runtime_list_len(bridge_hits).expect("bridge hit len"), 2);

        let mut bridge_values = Vec::new();
        for index in 0..2 {
            bridge_values.push(
                runtime_to_builtin_record_value(wr_list_get(bridge_hits, index), "Hit3")
                    .unwrap_or_else(|err| panic!("bridge hit {index} decode failed: {err:?}")),
            );
        }

        let (direct_hits, _) = execute_batch_query_with_trace_on(
            &ctx,
            DispatchBackend::Wgsl,
            &plan,
            &[
                KernelValue::Capture(SmolStr::new("scene_shape")),
                KernelValue::Array(rays),
            ],
        )
        .expect("direct trace batch");
        let KernelValue::Array(direct_hits) = direct_hits else {
            panic!("expected direct WGSL trace hit array");
        };

        assert_eq!(bridge_values, direct_hits);
    }
}
