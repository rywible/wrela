use crate::hir::{
    self, AssignOp, BinaryOp, Expr, FieldBounds, FieldClass, FieldSupport, FunctionRole,
    FunctionTypeInfo, Literal, Module, Stmt as HirStmt, Type, TypeInfo, UnaryOp,
};
use crate::mir::ir::Stmt as MirStmt;
use crate::mir::ir::*;
use crate::portable::{
    PortableBuiltinAtom, PortableBuiltinType, builtin_record, builtin_record_by_function,
    builtin_records,
};
use rowan::TextRange;
use smol_str::SmolStr;
use std::collections::{HashMap, HashSet};
use std::env;

fn is_syntactic_stringish(body: &hir::Body, expr: hir::Idx<hir::Expr>) -> bool {
    match &body.exprs[expr] {
        Expr::Literal(Literal::String(_)) => true,
        Expr::StringInterp(_) => true,
        _ => false,
    }
}

fn stable_shape_capture_id(shape_name: &SmolStr) -> i64 {
    const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;
    let mut hash = FNV_OFFSET_BASIS;
    for byte in shape_name.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash as i64
}

fn stable_shape_scene_capture_id(shape_name: &SmolStr) -> i64 {
    const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;
    let mut hash = FNV_OFFSET_BASIS;
    for byte in b"scene::shape::" {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    for byte in shape_name.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash as i64
}

fn stable_field_scene_capture_id(field_name: &SmolStr) -> i64 {
    const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;
    let mut hash = FNV_OFFSET_BASIS;
    for byte in b"scene::field::" {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    for byte in field_name.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash as i64
}

fn stable_region_scene_capture_id(region_name: &SmolStr) -> i64 {
    const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;
    let mut hash = FNV_OFFSET_BASIS;
    for byte in b"scene::region::" {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    for byte in region_name.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash as i64
}

fn region_item_matches_detail(
    domain_detail: hir::DomainGeometryDetail,
    item_detail: Option<hir::RegionDetailLevel>,
) -> bool {
    match item_detail {
        None => true,
        Some(hir::RegionDetailLevel::Coarse) => true,
        Some(hir::RegionDetailLevel::Fine) => matches!(domain_detail, hir::DomainGeometryDetail::Fine),
    }
}

fn resolve_region_shapes_for_detail(
    metadata: &hir::RegionMetadata,
    domain_detail: hir::DomainGeometryDetail,
) -> Result<Vec<SmolStr>, &'static str> {
    fn walk(
        items: &[hir::RegionItemMetadata],
        domain_detail: hir::DomainGeometryDetail,
        named: &mut HashMap<SmolStr, SmolStr>,
        ordered: &mut Vec<SmolStr>,
    ) -> Result<(), &'static str> {
        for item in items {
            match item {
                hir::RegionItemMetadata::Compose {
                    kind,
                    name,
                    shape,
                    detail,
                    ..
                } => {
                    if !region_item_matches_detail(domain_detail, *detail) {
                        continue;
                    }
                    match kind {
                        hir::RegionComposeKind::Place => {
                            if !named.contains_key(name) {
                                named.insert(name.clone(), shape.clone());
                                ordered.push(name.clone());
                            }
                        }
                        hir::RegionComposeKind::Replace => {
                            if !named.contains_key(name) {
                                ordered.push(name.clone());
                            }
                            named.insert(name.clone(), shape.clone());
                        }
                        hir::RegionComposeKind::Overlay => {
                            ordered.push(SmolStr::new(format!("__overlay_{}_{}", name, ordered.len())));
                            named.insert(
                                ordered.last().cloned().unwrap_or_default(),
                                shape.clone(),
                            );
                        }
                    }
                }
                hir::RegionItemMetadata::Scatter { .. } => {
                    return Err("scatter regions are not executable yet");
                }
                hir::RegionItemMetadata::Conditional { .. } => {
                    return Err("conditional regions are not executable yet");
                }
            }
        }
        Ok(())
    }

    let mut named = HashMap::new();
    let mut ordered = Vec::new();
    walk(&metadata.items, domain_detail, &mut named, &mut ordered)?;
    Ok(ordered
        .into_iter()
        .filter_map(|name| named.remove(&name))
        .collect())
}

fn executable_region_shape_lists(
    func: &hir::Function,
) -> Result<(Vec<SmolStr>, Vec<SmolStr>), &'static str> {
    let metadata = func.region.as_ref().ok_or("region metadata missing")?;
    let coarse = resolve_region_shapes_for_detail(metadata, hir::DomainGeometryDetail::Coarse)?;
    let fine = resolve_region_shapes_for_detail(metadata, hir::DomainGeometryDetail::Fine)?;
    Ok((coarse, fine))
}

pub fn lower_module(module: &Module) -> MirModule {
    let (_type_errors, type_info) = crate::hir::typeck::check_module_with_info(module);
    lower_module_with_types(module, &type_info)
}

pub fn lower_module_with_types(module: &Module, type_info: &TypeInfo) -> MirModule {
    const CLASS_ID_BASE: usize = 100;
    let mut type_tags = Vec::new();
    let mut tag_map = HashMap::new();
    let mut class_fields = HashMap::new();
    let mut class_field_defaults = HashMap::new();
    let mut classes = Vec::new();
    let mut class_method_ids = HashMap::new();
    let mut interface_methods: HashMap<SmolStr, HashSet<SmolStr>> = HashMap::new();
    let mut interface_impls: HashMap<SmolStr, Vec<SmolStr>> = HashMap::new();
    let mut method_ids = HashSet::new();
    let mut method_qnames: HashMap<hir::Idx<hir::Function>, SmolStr> = HashMap::new();
    for record in builtin_records() {
        let id = TypeTagId(type_tags.len() + CLASS_ID_BASE);
        let name = SmolStr::new(record.name);
        let fields: Vec<SmolStr> = record
            .fields
            .iter()
            .map(|field| SmolStr::new(field.name))
            .collect();
        type_tags.push(name.clone());
        tag_map.insert(name.clone(), id);
        class_fields.insert(name.clone(), fields.clone());
        class_field_defaults.insert(name.clone(), vec![None; fields.len()]);
        classes.push(MirClassInfo {
            name,
            id,
            fields,
            methods: Vec::new(),
        });
    }
    for (_idx, class) in module.classes.iter() {
        let id = TypeTagId(type_tags.len() + CLASS_ID_BASE);
        type_tags.push(class.name.clone());
        tag_map.insert(class.name.clone(), id);
        let fields: Vec<SmolStr> = class
            .fields
            .iter()
            .map(|field| field.name.clone())
            .collect();
        let defaults: Vec<Option<hir::FieldDefault>> = class
            .fields
            .iter()
            .map(|field| field.default.clone())
            .collect();
        class_fields.insert(class.name.clone(), fields);
        class_field_defaults.insert(class.name.clone(), defaults);
        let mut methods = Vec::new();
        let mut method_map = HashMap::new();
        for (idx, method_id) in class.methods.iter().enumerate() {
            let method = &module.functions[*method_id];
            method_ids.insert(method_id.into_raw());
            method_map.insert(method.name.clone(), idx as u32);
            let qname = SmolStr::new(format!("{}.{}", class.name, method.name));
            method_qnames.insert(*method_id, qname.clone());
            methods.push(MirMethodInfo {
                name: method.name.clone(),
                func: qname,
                arity: method.params.len() + 1,
                id: idx as u32,
            });
        }
        class_method_ids.insert(class.name.clone(), method_map);
        classes.push(MirClassInfo {
            name: class.name.clone(),
            id,
            fields: class_fields.get(&class.name).cloned().unwrap_or_default(),
            methods,
        });
    }

    for (_idx, interface) in module.interfaces.iter() {
        let method_set = interface_methods.entry(interface.name.clone()).or_default();
        for method in &interface.methods {
            method_set.insert(method.name.clone());
        }
    }
    for (_idx, class) in module.classes.iter() {
        for iface in &class.implements {
            interface_impls
                .entry(iface.clone())
                .or_default()
                .push(class.name.clone());
        }
    }

    for (_idx, en) in module.enums.iter() {
        for variant in &en.variants {
            let name = SmolStr::new(format!("{}.{}", en.name, variant.name));
            let id = TypeTagId(type_tags.len() + CLASS_ID_BASE);
            type_tags.push(name.clone());
            tag_map.insert(name.clone(), id);
            let fields: Vec<SmolStr> = variant
                .params
                .iter()
                .map(|param| param.name.clone())
                .collect();
            class_fields.insert(name.clone(), fields.clone());
            class_field_defaults.insert(name.clone(), vec![None; fields.len()]);
            classes.push(MirClassInfo {
                name: name.clone(),
                id,
                fields,
                methods: Vec::new(),
            });
        }
    }

    let mut functions = Vec::new();
    let mut function_names: HashSet<SmolStr> = module
        .functions
        .iter()
        .filter_map(|(idx, func)| {
            if method_ids.contains(&idx.into_raw()) {
                None
            } else {
                Some(func.name.clone())
            }
        })
        .collect();
    for qname in method_qnames.values() {
        function_names.insert(qname.clone());
    }
    for name in builtin_function_names() {
        function_names.insert(name);
    }
    let field_names: HashSet<SmolStr> = module
        .functions
        .iter()
        .filter_map(|(idx, func)| {
            if method_ids.contains(&idx.into_raw()) || !matches!(func.role, FunctionRole::Field) {
                None
            } else {
                Some(func.name.clone())
            }
        })
        .collect();
    let shape_names: HashSet<SmolStr> = module
        .shapes
        .iter()
        .map(|(_, shape)| shape.name.clone())
        .collect();
    let result_functions: HashSet<SmolStr> = module
        .functions
        .iter()
        .filter_map(|(_, func)| {
            let ret = func.ret_type.as_ref()?;
            if ret.name == "Result" {
                Some(func.name.clone())
            } else {
                None
            }
        })
        .collect();
    let shape_graphs: HashMap<SmolStr, hir::ShapeGraph> = module
        .shapes
        .iter()
        .filter_map(|(_, shape)| {
            shape
                .graph
                .as_ref()
                .map(|graph| (shape.name.clone(), graph.clone()))
        })
        .collect();
    let field_graphs: HashMap<SmolStr, hir::FieldGraph> = module
        .functions
        .iter()
        .filter_map(|(idx, func)| {
            if method_ids.contains(&idx.into_raw()) || !matches!(func.role, FunctionRole::Field) {
                None
            } else {
                func.field_graph
                    .as_ref()
                    .map(|graph| (func.name.clone(), graph.clone()))
            }
        })
        .collect();
    let field_bodies: HashMap<SmolStr, hir::Body> = module
        .functions
        .iter()
        .filter_map(|(idx, func)| {
            if method_ids.contains(&idx.into_raw()) || !matches!(func.role, FunctionRole::Field) {
                None
            } else {
                func.body
                    .as_ref()
                    .map(|body| (func.name.clone(), body.clone()))
            }
        })
        .collect();
    let field_metadata: HashMap<SmolStr, hir::FieldMetadata> = module
        .functions
        .iter()
        .filter_map(|(idx, func)| {
            if method_ids.contains(&idx.into_raw()) || !matches!(func.role, FunctionRole::Field) {
                None
            } else {
                func.field
                    .as_ref()
                    .map(|metadata| (func.name.clone(), metadata.clone()))
            }
        })
        .collect();
    let radiance_param_counts: HashMap<SmolStr, usize> = module
        .functions
        .iter()
        .filter_map(|(idx, func)| {
            if method_ids.contains(&idx.into_raw()) || !matches!(func.role, FunctionRole::Radiance)
            {
                None
            } else {
                Some((func.name.clone(), func.params.len()))
            }
        })
        .collect();
    let volume_param_counts: HashMap<SmolStr, usize> = module
        .functions
        .iter()
        .filter_map(|(idx, func)| {
            if method_ids.contains(&idx.into_raw()) || !matches!(func.role, FunctionRole::Volume) {
                None
            } else {
                Some((func.name.clone(), func.params.len()))
            }
        })
        .collect();
    for (_idx, func) in module.functions.iter() {
        let Some(body) = &func.body else {
            continue;
        };
        let is_method = method_ids.contains(&_idx.into_raw());
        let fn_types = type_info
            .function(_idx)
            .expect("missing type info for function during MIR lowering");
        let name = if is_method {
            method_qnames
                .get(&_idx)
                .cloned()
                .unwrap_or_else(|| func.name.clone())
        } else {
            func.name.clone()
        };
        functions.push(lower_function(
            module,
            func,
            name,
            body,
            &tag_map,
            &class_fields,
            &class_field_defaults,
            &function_names,
            &field_names,
            &shape_names,
            &shape_graphs,
            &field_graphs,
            &field_bodies,
            &field_metadata,
            &radiance_param_counts,
            &volume_param_counts,
            &result_functions,
            &class_method_ids,
            &interface_methods,
            is_method,
            fn_types,
        ));
    }

    for (_, shape) in module.shapes.iter() {
        if shape.graph.is_none() {
            continue;
        }
        functions.push(lower_shape_distance_helper(
            shape,
            &tag_map,
            &class_fields,
            &class_field_defaults,
            &function_names,
            &field_names,
            &shape_names,
            &shape_graphs,
            &field_graphs,
            &field_bodies,
            &field_metadata,
            &radiance_param_counts,
            &volume_param_counts,
            &result_functions,
            &class_method_ids,
            &interface_methods,
        ));
        functions.push(lower_shape_trace_helper(
            shape,
            module,
            &tag_map,
            &class_fields,
            &class_field_defaults,
            &function_names,
            &field_names,
            &shape_names,
            &shape_graphs,
            &field_graphs,
            &field_bodies,
            &field_metadata,
            &radiance_param_counts,
            &volume_param_counts,
            &result_functions,
            &class_method_ids,
            &interface_methods,
        ));
        functions.push(lower_shape_surface_helper(
            shape,
            module,
            &tag_map,
            &class_fields,
            &class_field_defaults,
            &function_names,
            &field_names,
            &shape_names,
            &shape_graphs,
            &field_graphs,
            &field_bodies,
            &field_metadata,
            &radiance_param_counts,
            &volume_param_counts,
            &result_functions,
            &class_method_ids,
            &interface_methods,
        ));
    }

    functions.push(lower_scene_distance_capture_helper(
        module,
        &tag_map,
        &class_fields,
        &class_field_defaults,
        &function_names,
        &field_names,
        &shape_names,
        &shape_graphs,
        &field_graphs,
        &field_bodies,
        &field_metadata,
        &radiance_param_counts,
        &volume_param_counts,
        &result_functions,
        &class_method_ids,
        &interface_methods,
        "FieldCapture",
        "__wr_field_distance_capture",
    ));
    functions.push(lower_scene_normal_capture_helper(
        module,
        &tag_map,
        &class_fields,
        &class_field_defaults,
        &function_names,
        &field_names,
        &shape_names,
        &shape_graphs,
        &field_graphs,
        &field_bodies,
        &field_metadata,
        &radiance_param_counts,
        &volume_param_counts,
        &result_functions,
        &class_method_ids,
        &interface_methods,
        "FieldCapture",
        "__wr_field_normal_capture",
    ));
    functions.push(lower_scene_distance_capture_helper(
        module,
        &tag_map,
        &class_fields,
        &class_field_defaults,
        &function_names,
        &field_names,
        &shape_names,
        &shape_graphs,
        &field_graphs,
        &field_bodies,
        &field_metadata,
        &radiance_param_counts,
        &volume_param_counts,
        &result_functions,
        &class_method_ids,
        &interface_methods,
        "ShapeCapture",
        "__wr_shape_distance_capture",
    ));
    functions.push(lower_scene_normal_capture_helper(
        module,
        &tag_map,
        &class_fields,
        &class_field_defaults,
        &function_names,
        &field_names,
        &shape_names,
        &shape_graphs,
        &field_graphs,
        &field_bodies,
        &field_metadata,
        &radiance_param_counts,
        &volume_param_counts,
        &result_functions,
        &class_method_ids,
        &interface_methods,
        "ShapeCapture",
        "__wr_shape_normal_capture",
    ));
    functions.push(lower_scene_trace_capture_helper(
        module,
        &tag_map,
        &class_fields,
        &class_field_defaults,
        &function_names,
        &field_names,
        &shape_names,
        &shape_graphs,
        &field_graphs,
        &field_bodies,
        &field_metadata,
        &radiance_param_counts,
        &volume_param_counts,
        &result_functions,
        &class_method_ids,
        &interface_methods,
    ));
    functions.push(lower_scene_surface_capture_helper(
        module,
        &tag_map,
        &class_fields,
        &class_field_defaults,
        &function_names,
        &field_names,
        &shape_names,
        &shape_graphs,
        &field_graphs,
        &field_bodies,
        &field_metadata,
        &radiance_param_counts,
        &volume_param_counts,
        &result_functions,
        &class_method_ids,
        &interface_methods,
    ));
    functions.push(lower_scene_radiance_capture_helper(
        module,
        &tag_map,
        &class_fields,
        &class_field_defaults,
        &function_names,
        &field_names,
        &shape_names,
        &shape_graphs,
        &field_graphs,
        &field_bodies,
        &field_metadata,
        &radiance_param_counts,
        &volume_param_counts,
        &result_functions,
        &class_method_ids,
        &interface_methods,
    ));
    functions.push(lower_scene_medium_capture_helper(
        module,
        &tag_map,
        &class_fields,
        &class_field_defaults,
        &function_names,
        &field_names,
        &shape_names,
        &shape_graphs,
        &field_graphs,
        &field_bodies,
        &field_metadata,
        &radiance_param_counts,
        &volume_param_counts,
        &result_functions,
        &class_method_ids,
        &interface_methods,
    ));
    functions.push(lower_world_distance_capture_helper(
        module,
        &tag_map,
        &class_fields,
        &class_field_defaults,
        &function_names,
        &field_names,
        &shape_names,
        &shape_graphs,
        &field_graphs,
        &field_bodies,
        &field_metadata,
        &radiance_param_counts,
        &volume_param_counts,
        &result_functions,
        &class_method_ids,
        &interface_methods,
    ));
    functions.push(lower_world_normal_capture_helper(
        module,
        &tag_map,
        &class_fields,
        &class_field_defaults,
        &function_names,
        &field_names,
        &shape_names,
        &shape_graphs,
        &field_graphs,
        &field_bodies,
        &field_metadata,
        &radiance_param_counts,
        &volume_param_counts,
        &result_functions,
        &class_method_ids,
        &interface_methods,
    ));
    functions.push(lower_world_trace_capture_helper(
        module,
        &tag_map,
        &class_fields,
        &class_field_defaults,
        &function_names,
        &field_names,
        &shape_names,
        &shape_graphs,
        &field_graphs,
        &field_bodies,
        &field_metadata,
        &radiance_param_counts,
        &volume_param_counts,
        &result_functions,
        &class_method_ids,
        &interface_methods,
    ));
    functions.push(lower_world_surface_capture_helper(
        module,
        &tag_map,
        &class_fields,
        &class_field_defaults,
        &function_names,
        &field_names,
        &shape_names,
        &shape_graphs,
        &field_graphs,
        &field_bodies,
        &field_metadata,
        &radiance_param_counts,
        &volume_param_counts,
        &result_functions,
        &class_method_ids,
        &interface_methods,
    ));
    functions.push(lower_world_radiance_capture_helper(
        module,
        &tag_map,
        &class_fields,
        &class_field_defaults,
        &function_names,
        &field_names,
        &shape_names,
        &shape_graphs,
        &field_graphs,
        &field_bodies,
        &field_metadata,
        &radiance_param_counts,
        &volume_param_counts,
        &result_functions,
        &class_method_ids,
        &interface_methods,
    ));
    functions.push(lower_world_medium_capture_helper(
        module,
        &tag_map,
        &class_fields,
        &class_field_defaults,
        &function_names,
        &field_names,
        &shape_names,
        &shape_graphs,
        &field_graphs,
        &field_bodies,
        &field_metadata,
        &radiance_param_counts,
        &volume_param_counts,
        &result_functions,
        &class_method_ids,
        &interface_methods,
    ));
    functions.push(lower_render_shadow_visibility_helper(
        module,
        &tag_map,
        &class_fields,
        &class_field_defaults,
        &function_names,
        &field_names,
        &shape_names,
        &shape_graphs,
        &field_graphs,
        &field_bodies,
        &field_metadata,
        &radiance_param_counts,
        &volume_param_counts,
        &result_functions,
        &class_method_ids,
        &interface_methods,
    ));
    functions.push(lower_render_ambient_occlusion_helper(
        module,
        &tag_map,
        &class_fields,
        &class_field_defaults,
        &function_names,
        &field_names,
        &shape_names,
        &shape_graphs,
        &field_graphs,
        &field_bodies,
        &field_metadata,
        &radiance_param_counts,
        &volume_param_counts,
        &result_functions,
        &class_method_ids,
        &interface_methods,
    ));
    functions.push(lower_render_scene_color_helper(
        module,
        &tag_map,
        &class_fields,
        &class_field_defaults,
        &function_names,
        &field_names,
        &shape_names,
        &shape_graphs,
        &field_graphs,
        &field_bodies,
        &field_metadata,
        &radiance_param_counts,
        &volume_param_counts,
        &result_functions,
        &class_method_ids,
        &interface_methods,
    ));
    functions.push(lower_render_capture_to_ppm_helper(
        module,
        &tag_map,
        &class_fields,
        &class_field_defaults,
        &function_names,
        &field_names,
        &shape_names,
        &shape_graphs,
        &field_graphs,
        &field_bodies,
        &field_metadata,
        &radiance_param_counts,
        &volume_param_counts,
        &result_functions,
        &class_method_ids,
        &interface_methods,
    ));
    functions.push(lower_scene_trace_queries_helper(
        module,
        &tag_map,
        &class_fields,
        &class_field_defaults,
        &function_names,
        &field_names,
        &shape_names,
        &shape_graphs,
        &field_graphs,
        &field_bodies,
        &field_metadata,
        &radiance_param_counts,
        &volume_param_counts,
        &result_functions,
        &class_method_ids,
        &interface_methods,
    ));
    functions.push(lower_scene_surface_queries_helper(
        module,
        &tag_map,
        &class_fields,
        &class_field_defaults,
        &function_names,
        &field_names,
        &shape_names,
        &shape_graphs,
        &field_graphs,
        &field_bodies,
        &field_metadata,
        &radiance_param_counts,
        &volume_param_counts,
        &result_functions,
        &class_method_ids,
        &interface_methods,
    ));

    let dispatch_functions = build_interface_dispatch_functions(module, &interface_impls, &tag_map);
    for func in dispatch_functions {
        functions.push(func);
    }
    MirModule {
        functions,
        type_tags,
        classes,
    }
}

fn lower_function(
    module: &hir::Module,
    func: &hir::Function,
    name: SmolStr,
    body: &hir::Body,
    type_tags: &HashMap<SmolStr, TypeTagId>,
    class_fields: &HashMap<SmolStr, Vec<SmolStr>>,
    class_field_defaults: &HashMap<SmolStr, Vec<Option<hir::FieldDefault>>>,
    function_names: &HashSet<SmolStr>,
    field_names: &HashSet<SmolStr>,
    shape_names: &HashSet<SmolStr>,
    shape_graphs: &HashMap<SmolStr, hir::ShapeGraph>,
    field_graphs: &HashMap<SmolStr, hir::FieldGraph>,
    field_bodies: &HashMap<SmolStr, hir::Body>,
    field_metadata: &HashMap<SmolStr, hir::FieldMetadata>,
    radiance_param_counts: &HashMap<SmolStr, usize>,
    volume_param_counts: &HashMap<SmolStr, usize>,
    result_functions: &HashSet<SmolStr>,
    class_method_ids: &HashMap<SmolStr, HashMap<SmolStr, u32>>,
    interface_methods: &HashMap<SmolStr, HashSet<SmolStr>>,
    is_method: bool,
    type_info: &FunctionTypeInfo,
) -> MirFunction {
    if matches!(func.role, FunctionRole::Region) {
        return lower_region_function(
            module,
            func,
            name,
            type_tags,
            class_fields,
            class_field_defaults,
            function_names,
            field_names,
            shape_names,
            shape_graphs,
            field_graphs,
            field_bodies,
            field_metadata,
            radiance_param_counts,
            volume_param_counts,
            result_functions,
            class_method_ids,
            interface_methods,
            type_info,
        );
    }
    if matches!(func.role, FunctionRole::Domain) {
        return lower_domain_function(
            module,
            func,
            name,
            type_tags,
            class_fields,
            class_field_defaults,
            function_names,
            field_names,
            shape_names,
            shape_graphs,
            field_graphs,
            field_bodies,
            field_metadata,
            radiance_param_counts,
            volume_param_counts,
            result_functions,
            class_method_ids,
            interface_methods,
            type_info,
        );
    }
    if matches!(func.role, FunctionRole::Render) {
        return lower_render_function(
            module,
            func,
            name,
            type_tags,
            class_fields,
            class_field_defaults,
            function_names,
            field_names,
            shape_names,
            shape_graphs,
            field_graphs,
            field_bodies,
            field_metadata,
            radiance_param_counts,
            volume_param_counts,
            result_functions,
            class_method_ids,
            interface_methods,
            type_info,
        );
    }
    let mut lowerer = FunctionLowerer::new(
        name,
        type_tags,
        class_fields,
        class_field_defaults,
        function_names,
        field_names,
        shape_names,
        shape_graphs,
        field_graphs,
        field_bodies,
        field_metadata,
        radiance_param_counts,
        volume_param_counts,
        result_functions,
        class_method_ids,
        interface_methods,
        matches!(
            func.ret_type.as_ref().map(|t| t.name.as_str()),
            Some("Result")
        ),
        Some(type_info),
    );

    if is_method {
        let local = lowerer.new_local(
            SmolStr::new("self"),
            false,
            lowerer.local_type_for_name(&SmolStr::new("self")),
        );
        lowerer.declare_local(SmolStr::new("self"), local);
        lowerer.params.push(local);
    }
    for param in &func.params {
        let local = lowerer.new_local(
            param.name.clone(),
            false,
            lowerer.local_type_for_name(&param.name),
        );
        lowerer.declare_local(param.name.clone(), local);
        let is_result = matches!(param.ty.as_ref().map(|t| t.name.as_str()), Some("Result"));
        lowerer.declare_resultness(param.name.clone(), is_result);
        lowerer.params.push(local);
    }

    let entry = lowerer.new_block();
    lowerer.current_block = entry;
    lowerer.lower_stmt_block(body, &body.root_stmts);
    if lowerer.block_is_open(lowerer.current_block) {
        lowerer.set_terminator(Terminator::Return {
            value: None,
            span: TextRange::empty(0.into()),
        });
    }

    MirFunction {
        name: lowerer.name,
        params: lowerer.params,
        abi_params: {
            let mut abi_params = Vec::with_capacity(func.params.len() + usize::from(is_method));
            if is_method {
                abi_params.push(PortableAbiType::Value);
            }
            abi_params.extend(func.params.iter().map(|param| {
                portable_abi_from_type_ref(
                    param.ty.as_ref(),
                    module,
                    type_tags,
                    &mut HashSet::new(),
                )
            }));
            abi_params
        },
        abi_return: portable_abi_from_type_ref(
            func.ret_type.as_ref(),
            module,
            type_tags,
            &mut HashSet::new(),
        ),
        locals: lowerer.locals,
        temps: lowerer.temps,
        blocks: lowerer.blocks,
        entry,
        suspendable: lowerer.suspendable,
    }
}

fn lower_region_function(
    module: &hir::Module,
    func: &hir::Function,
    name: SmolStr,
    type_tags: &HashMap<SmolStr, TypeTagId>,
    class_fields: &HashMap<SmolStr, Vec<SmolStr>>,
    class_field_defaults: &HashMap<SmolStr, Vec<Option<hir::FieldDefault>>>,
    function_names: &HashSet<SmolStr>,
    field_names: &HashSet<SmolStr>,
    shape_names: &HashSet<SmolStr>,
    shape_graphs: &HashMap<SmolStr, hir::ShapeGraph>,
    field_graphs: &HashMap<SmolStr, hir::FieldGraph>,
    field_bodies: &HashMap<SmolStr, hir::Body>,
    field_metadata: &HashMap<SmolStr, hir::FieldMetadata>,
    radiance_param_counts: &HashMap<SmolStr, usize>,
    volume_param_counts: &HashMap<SmolStr, usize>,
    result_functions: &HashSet<SmolStr>,
    class_method_ids: &HashMap<SmolStr, HashMap<SmolStr, u32>>,
    interface_methods: &HashMap<SmolStr, HashSet<SmolStr>>,
    type_info: &FunctionTypeInfo,
) -> MirFunction {
    let span = TextRange::empty(0.into());
    let mut lowerer = FunctionLowerer::new(
        name,
        type_tags,
        class_fields,
        class_field_defaults,
        function_names,
        field_names,
        shape_names,
        shape_graphs,
        field_graphs,
        field_bodies,
        field_metadata,
        radiance_param_counts,
        volume_param_counts,
        result_functions,
        class_method_ids,
        interface_methods,
        false,
        Some(type_info),
    );

    for param in &func.params {
        let local = lowerer.new_local(
            param.name.clone(),
            false,
            lowerer.local_type_for_name(&param.name),
        );
        lowerer.declare_local(param.name.clone(), local);
        lowerer.params.push(local);
    }

    let entry = lowerer.new_block();
    lowerer.current_block = entry;
    let result = if func.params.is_empty() {
        lowerer.build_scene_capture_value(&func.name, span)
    } else {
        let crash_temp = lowerer.new_temp(MirType::Unknown);
        lowerer.push_stmt(MirStmt::Assign {
            place: Place::Temp(crash_temp),
            value: Rvalue::Crash {
                value: Value::Const(Literal::String(SmolStr::new(
                    "parameterized regions are not executable yet; capture a zero-argument region",
                ))),
            },
            span,
        });
        Value::Temp(crash_temp)
    };
    lowerer.set_terminator(Terminator::Return {
        value: Some(result),
        span,
    });

    MirFunction {
        name: lowerer.name,
        params: lowerer.params,
        abi_params: func
            .params
            .iter()
            .map(|param| {
                portable_abi_from_type_ref(
                    param.ty.as_ref(),
                    module,
                    type_tags,
                    &mut HashSet::new(),
                )
            })
            .collect(),
        abi_return: portable_abi_from_type_ref(
            func.ret_type.as_ref(),
            module,
            type_tags,
            &mut HashSet::new(),
        ),
        locals: lowerer.locals,
        temps: lowerer.temps,
        blocks: lowerer.blocks,
        entry,
        suspendable: false,
    }
}

fn lower_domain_function(
    module: &hir::Module,
    func: &hir::Function,
    name: SmolStr,
    type_tags: &HashMap<SmolStr, TypeTagId>,
    class_fields: &HashMap<SmolStr, Vec<SmolStr>>,
    class_field_defaults: &HashMap<SmolStr, Vec<Option<hir::FieldDefault>>>,
    function_names: &HashSet<SmolStr>,
    field_names: &HashSet<SmolStr>,
    shape_names: &HashSet<SmolStr>,
    shape_graphs: &HashMap<SmolStr, hir::ShapeGraph>,
    field_graphs: &HashMap<SmolStr, hir::FieldGraph>,
    field_bodies: &HashMap<SmolStr, hir::Body>,
    field_metadata: &HashMap<SmolStr, hir::FieldMetadata>,
    radiance_param_counts: &HashMap<SmolStr, usize>,
    volume_param_counts: &HashMap<SmolStr, usize>,
    result_functions: &HashSet<SmolStr>,
    class_method_ids: &HashMap<SmolStr, HashMap<SmolStr, u32>>,
    interface_methods: &HashMap<SmolStr, HashSet<SmolStr>>,
    type_info: &FunctionTypeInfo,
) -> MirFunction {
    let span = TextRange::empty(0.into());
    let mut lowerer = FunctionLowerer::new(
        name,
        type_tags,
        class_fields,
        class_field_defaults,
        function_names,
        field_names,
        shape_names,
        shape_graphs,
        field_graphs,
        field_bodies,
        field_metadata,
        radiance_param_counts,
        volume_param_counts,
        result_functions,
        class_method_ids,
        interface_methods,
        false,
        Some(type_info),
    );

    for param in &func.params {
        let local = lowerer.new_local(
            param.name.clone(),
            false,
            lowerer.local_type_for_name(&param.name),
        );
        lowerer.declare_local(param.name.clone(), local);
        lowerer.params.push(local);
    }

    let entry = lowerer.new_block();
    lowerer.current_block = entry;
    let metadata = func.domain.as_ref().expect("domain metadata");
    let world_local = func
        .params
        .first()
        .and_then(|param| lowerer.resolve_local(&param.name));
    let world_scene_id = world_local.map(|world| {
        lowerer.lower_get_named_field(
            Value::Local(world),
            "RegionCapture",
            "scene_id",
            MirType::Integer,
            span,
        )
    });

    let mut class = lowerer.synthetic_class_target_info("SceneDomain");
    FunctionLowerer::set_class_field_value(
        &mut class,
        "scene_id",
        world_scene_id.unwrap_or(Value::Const(Literal::Integer(0))),
    );
    FunctionLowerer::set_class_field_value(
        &mut class,
        "geometry_detail",
        Value::Const(Literal::Integer(match metadata.geometry_detail {
            hir::DomainGeometryDetail::Coarse => 0,
            hir::DomainGeometryDetail::Fine => 1,
        })),
    );
    FunctionLowerer::set_class_field_value(
        &mut class,
        "material",
        Value::Const(Literal::Boolean(metadata.material)),
    );
    FunctionLowerer::set_class_field_value(
        &mut class,
        "radiance",
        Value::Const(Literal::Boolean(metadata.radiance)),
    );
    FunctionLowerer::set_class_field_value(
        &mut class,
        "media",
        Value::Const(Literal::Boolean(metadata.media)),
    );
    FunctionLowerer::set_class_field_value(
        &mut class,
        "max_distance",
        metadata
            .max_distance
            .as_ref()
            .map(|body| lowerer.lower_wrapped_body_value(body, span))
            .unwrap_or(Value::Const(Literal::Float(12.0))),
    );
    FunctionLowerer::set_class_field_value(
        &mut class,
        "min_step",
        metadata
            .min_step
            .as_ref()
            .map(|body| lowerer.lower_wrapped_body_value(body, span))
            .unwrap_or(Value::Const(Literal::Float(0.02))),
    );
    FunctionLowerer::set_class_field_value(
        &mut class,
        "hit_epsilon",
        metadata
            .hit_epsilon
            .as_ref()
            .map(|body| lowerer.lower_wrapped_body_value(body, span))
            .unwrap_or(Value::Const(Literal::Float(0.001))),
    );
    FunctionLowerer::set_class_field_value(
        &mut class,
        "max_steps",
        metadata
            .max_steps
            .as_ref()
            .map(|body| lowerer.lower_wrapped_body_value(body, span))
            .unwrap_or(Value::Const(Literal::Integer(96))),
    );
    let result = lowerer.build_class_instance(&class, span);
    lowerer.set_terminator(Terminator::Return {
        value: Some(result),
        span,
    });

    MirFunction {
        name: lowerer.name,
        params: lowerer.params,
        abi_params: func
            .params
            .iter()
            .map(|param| {
                portable_abi_from_type_ref(
                    param.ty.as_ref(),
                    module,
                    type_tags,
                    &mut HashSet::new(),
                )
            })
            .collect(),
        abi_return: portable_abi_from_type_ref(
            func.ret_type.as_ref(),
            module,
            type_tags,
            &mut HashSet::new(),
        ),
        locals: lowerer.locals,
        temps: lowerer.temps,
        blocks: lowerer.blocks,
        entry,
        suspendable: false,
    }
}

fn portable_abi_named_type(
    name: &str,
    module: &hir::Module,
    type_tags: &HashMap<SmolStr, TypeTagId>,
) -> PortableAbiType {
    let ty = hir::TypeRef {
        name: SmolStr::new(name),
        name_span: None,
        args: Vec::new(),
    };
    portable_abi_from_type_ref(Some(&ty), module, type_tags, &mut HashSet::new())
}

fn lower_render_function(
    module: &hir::Module,
    func: &hir::Function,
    name: SmolStr,
    type_tags: &HashMap<SmolStr, TypeTagId>,
    class_fields: &HashMap<SmolStr, Vec<SmolStr>>,
    class_field_defaults: &HashMap<SmolStr, Vec<Option<hir::FieldDefault>>>,
    function_names: &HashSet<SmolStr>,
    field_names: &HashSet<SmolStr>,
    shape_names: &HashSet<SmolStr>,
    shape_graphs: &HashMap<SmolStr, hir::ShapeGraph>,
    field_graphs: &HashMap<SmolStr, hir::FieldGraph>,
    field_bodies: &HashMap<SmolStr, hir::Body>,
    field_metadata: &HashMap<SmolStr, hir::FieldMetadata>,
    radiance_param_counts: &HashMap<SmolStr, usize>,
    volume_param_counts: &HashMap<SmolStr, usize>,
    result_functions: &HashSet<SmolStr>,
    class_method_ids: &HashMap<SmolStr, HashMap<SmolStr, u32>>,
    interface_methods: &HashMap<SmolStr, HashSet<SmolStr>>,
    type_info: &FunctionTypeInfo,
) -> MirFunction {
    let span = TextRange::empty(0.into());
    let mut lowerer = FunctionLowerer::new(
        name,
        type_tags,
        class_fields,
        class_field_defaults,
        function_names,
        field_names,
        shape_names,
        shape_graphs,
        field_graphs,
        field_bodies,
        field_metadata,
        radiance_param_counts,
        volume_param_counts,
        result_functions,
        class_method_ids,
        interface_methods,
        false,
        Some(type_info),
    );

    for param in &func.params {
        let local = lowerer.new_local(
            param.name.clone(),
            false,
            lowerer.local_type_for_name(&param.name),
        );
        lowerer.declare_local(param.name.clone(), local);
        lowerer.params.push(local);
    }

    let entry = lowerer.new_block();
    lowerer.current_block = entry;

    let metadata = func.render.as_ref().expect("render metadata");
    let world_local = func
        .params
        .first()
        .and_then(|param| lowerer.resolve_local(&param.name))
        .expect("render world param");
    let camera_local = func
        .params
        .get(1)
        .and_then(|param| lowerer.resolve_local(&param.name))
        .expect("render camera param");

    let domain = metadata
        .domain
        .as_ref()
        .map(|body| lowerer.lower_wrapped_body_value(body, span))
        .unwrap_or_else(|| build_default_scene_domain_value(&mut lowerer, world_local, span));
    let light = metadata
        .light
        .as_ref()
        .map(|body| lowerer.lower_wrapped_body_value(body, span))
        .unwrap_or_else(|| build_default_render_light_value(&mut lowerer, span));
    let width = metadata
        .width
        .as_ref()
        .map(|body| lowerer.lower_wrapped_body_value(body, span))
        .unwrap_or(Value::Const(Literal::Integer(40)));
    let height = metadata
        .height
        .as_ref()
        .map(|body| lowerer.lower_wrapped_body_value(body, span))
        .unwrap_or(Value::Const(Literal::Integer(40)));
    let world_up = metadata
        .world_up
        .as_ref()
        .map(|body| lowerer.lower_wrapped_body_value(body, span))
        .unwrap_or_else(|| {
            lowerer.lower_get_named_field(
                Value::Local(camera_local),
                "Camera",
                "up",
                MirType::Vec3,
                span,
            )
        });
    let view_scale = metadata
        .view_scale
        .as_ref()
        .map(|body| lowerer.lower_wrapped_body_value(body, span))
        .unwrap_or(Value::Const(Literal::Float(0.72)));
    let fill_dir = metadata
        .fill_dir
        .as_ref()
        .map(|body| lowerer.lower_wrapped_body_value(body, span))
        .unwrap_or_else(|| build_default_render_fill_dir_value(&mut lowerer, span));

    let result = lowerer.lower_call_temp(
        MirType::String,
        SmolStr::new("__wr_render_capture_to_ppm"),
        vec![
            Value::Local(world_local),
            domain,
            Value::Local(camera_local),
            light,
            width,
            height,
            world_up,
            view_scale,
            fill_dir,
        ],
        span,
    );
    lowerer.set_terminator(Terminator::Return {
        value: Some(result),
        span,
    });

    MirFunction {
        name: lowerer.name,
        params: lowerer.params,
        abi_params: func
            .params
            .iter()
            .map(|param| {
                portable_abi_from_type_ref(
                    param.ty.as_ref(),
                    module,
                    type_tags,
                    &mut HashSet::new(),
                )
            })
            .collect(),
        abi_return: portable_abi_from_type_ref(
            func.ret_type.as_ref(),
            module,
            type_tags,
            &mut HashSet::new(),
        ),
        locals: lowerer.locals,
        temps: lowerer.temps,
        blocks: lowerer.blocks,
        entry,
        suspendable: false,
    }
}

fn build_vec3_value(lowerer: &mut FunctionLowerer, values: [f64; 3], span: TextRange) -> Value {
    lowerer.lower_call_temp(
        MirType::Vec3,
        SmolStr::new("vec3"),
        vec![
            Value::Const(Literal::Float(values[0])),
            Value::Const(Literal::Float(values[1])),
            Value::Const(Literal::Float(values[2])),
        ],
        span,
    )
}

fn build_default_render_fill_dir_value(lowerer: &mut FunctionLowerer, span: TextRange) -> Value {
    let base = build_vec3_value(lowerer, [-0.9, 0.45, 0.2], span);
    lowerer.lower_call_temp(MirType::Vec3, SmolStr::new("normalize"), vec![base], span)
}

fn build_default_render_light_value(lowerer: &mut FunctionLowerer, span: TextRange) -> Value {
    let mut class = lowerer.synthetic_class_target_info("Light");
    FunctionLowerer::set_class_field_value(
        &mut class,
        "position",
        build_vec3_value(lowerer, [2.4, 2.8, 2.4], span),
    );
    let light_dir = build_vec3_value(lowerer, [-0.8, -0.9, -0.9], span);
    FunctionLowerer::set_class_field_value(
        &mut class,
        "direction",
        lowerer.lower_call_temp(MirType::Vec3, SmolStr::new("normalize"), vec![light_dir], span),
    );
    FunctionLowerer::set_class_field_value(
        &mut class,
        "intensity",
        build_vec3_value(lowerer, [1.0, 0.98, 0.95], span),
    );
    FunctionLowerer::set_class_field_value(
        &mut class,
        "range",
        Value::Const(Literal::Float(12.0)),
    );
    lowerer.build_class_instance(&class, span)
}

fn build_default_scene_domain_value(
    lowerer: &mut FunctionLowerer,
    world_local: LocalId,
    span: TextRange,
) -> Value {
    let scene_id = lowerer.lower_get_named_field(
        Value::Local(world_local),
        "RegionCapture",
        "scene_id",
        MirType::Integer,
        span,
    );
    let mut class = lowerer.synthetic_class_target_info("SceneDomain");
    FunctionLowerer::set_class_field_value(&mut class, "scene_id", scene_id);
    FunctionLowerer::set_class_field_value(
        &mut class,
        "geometry_detail",
        Value::Const(Literal::Integer(1)),
    );
    FunctionLowerer::set_class_field_value(
        &mut class,
        "material",
        Value::Const(Literal::Boolean(true)),
    );
    FunctionLowerer::set_class_field_value(
        &mut class,
        "radiance",
        Value::Const(Literal::Boolean(true)),
    );
    FunctionLowerer::set_class_field_value(
        &mut class,
        "media",
        Value::Const(Literal::Boolean(true)),
    );
    FunctionLowerer::set_class_field_value(
        &mut class,
        "max_distance",
        Value::Const(Literal::Float(12.0)),
    );
    FunctionLowerer::set_class_field_value(
        &mut class,
        "min_step",
        Value::Const(Literal::Float(0.02)),
    );
    FunctionLowerer::set_class_field_value(
        &mut class,
        "hit_epsilon",
        Value::Const(Literal::Float(0.001)),
    );
    FunctionLowerer::set_class_field_value(
        &mut class,
        "max_steps",
        Value::Const(Literal::Integer(96)),
    );
    lowerer.build_class_instance(&class, span)
}

fn declare_internal_param(lowerer: &mut FunctionLowerer, name: &str, ty: MirType) -> LocalId {
    let local = lowerer.new_local(SmolStr::new(name), false, ty);
    lowerer.declare_local(SmolStr::new(name), local);
    lowerer.params.push(local);
    local
}

fn lower_render_world_distance_call(
    lowerer: &mut FunctionLowerer,
    world: Value,
    domain: Value,
    point: Value,
    span: TextRange,
) -> Value {
    lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("__wr_world_distance_capture"),
        vec![world, domain, point],
        span,
    )
}

fn lower_render_world_trace_call(
    lowerer: &mut FunctionLowerer,
    world: Value,
    domain: Value,
    origin: Value,
    direction: Value,
    max_distance: Value,
    min_step: Value,
    hit_epsilon: Value,
    max_steps: Value,
    span: TextRange,
) -> Value {
    lowerer.lower_call_temp(
        MirType::Named(SmolStr::new("Hit3")),
        SmolStr::new("__wr_world_trace_capture"),
        vec![
            world,
            domain,
            origin,
            direction,
            max_distance,
            min_step,
            hit_epsilon,
            max_steps,
        ],
        span,
    )
}

fn lower_render_world_surface_call(
    lowerer: &mut FunctionLowerer,
    world: Value,
    domain: Value,
    hit: Value,
    span: TextRange,
) -> Value {
    lowerer.lower_call_temp(
        MirType::Named(SmolStr::new("Surface")),
        SmolStr::new("__wr_world_surface_capture"),
        vec![world, domain, hit],
        span,
    )
}

fn lower_render_world_radiance_call(
    lowerer: &mut FunctionLowerer,
    world: Value,
    domain: Value,
    point: Value,
    direction: Value,
    span: TextRange,
) -> Value {
    lowerer.lower_call_temp(
        MirType::Vec3,
        SmolStr::new("__wr_world_radiance_capture"),
        vec![world, domain, point, direction],
        span,
    )
}

fn lower_render_world_medium_call(
    lowerer: &mut FunctionLowerer,
    world: Value,
    domain: Value,
    point: Value,
    span: TextRange,
) -> Value {
    lowerer.lower_call_temp(
        MirType::Named(SmolStr::new("Medium")),
        SmolStr::new("__wr_world_medium_capture"),
        vec![world, domain, point],
        span,
    )
}

fn lower_render_shadow_visibility_helper(
    module: &hir::Module,
    type_tags: &HashMap<SmolStr, TypeTagId>,
    class_fields: &HashMap<SmolStr, Vec<SmolStr>>,
    class_field_defaults: &HashMap<SmolStr, Vec<Option<hir::FieldDefault>>>,
    function_names: &HashSet<SmolStr>,
    field_names: &HashSet<SmolStr>,
    shape_names: &HashSet<SmolStr>,
    shape_graphs: &HashMap<SmolStr, hir::ShapeGraph>,
    field_graphs: &HashMap<SmolStr, hir::FieldGraph>,
    field_bodies: &HashMap<SmolStr, hir::Body>,
    field_metadata: &HashMap<SmolStr, hir::FieldMetadata>,
    radiance_param_counts: &HashMap<SmolStr, usize>,
    volume_param_counts: &HashMap<SmolStr, usize>,
    result_functions: &HashSet<SmolStr>,
    class_method_ids: &HashMap<SmolStr, HashMap<SmolStr, u32>>,
    interface_methods: &HashMap<SmolStr, HashSet<SmolStr>>,
) -> MirFunction {
    let span = TextRange::empty(0.into());
    let mut lowerer = FunctionLowerer::new(
        SmolStr::new("__wr_render_shadow_visibility_capture"),
        type_tags,
        class_fields,
        class_field_defaults,
        function_names,
        field_names,
        shape_names,
        shape_graphs,
        field_graphs,
        field_bodies,
        field_metadata,
        radiance_param_counts,
        volume_param_counts,
        result_functions,
        class_method_ids,
        interface_methods,
        false,
        None,
    );

    let world = declare_internal_param(&mut lowerer, "world", MirType::Named(SmolStr::new("RegionCapture")));
    let domain = declare_internal_param(&mut lowerer, "domain", MirType::Named(SmolStr::new("SceneDomain")));
    let hit_position = declare_internal_param(&mut lowerer, "hit_position", MirType::Vec3);
    let hit_normal = declare_internal_param(&mut lowerer, "hit_normal", MirType::Vec3);
    let light = declare_internal_param(&mut lowerer, "light", MirType::Named(SmolStr::new("Light")));

    let entry = lowerer.new_block();
    let hit_block = lowerer.new_block();
    let miss_block = lowerer.new_block();
    let join_block = lowerer.new_block();
    lowerer.current_block = entry;

    let normal_bias = lowerer.lower_binary_temp(
        MirType::Vec3,
        BinaryOp::Mul,
        Value::Local(hit_normal),
        Value::Const(Literal::Float(0.01)),
        span,
    );
    let shadow_origin = lowerer.lower_binary_temp(
        MirType::Vec3,
        BinaryOp::Add,
        Value::Local(hit_position),
        normal_bias,
        span,
    );
    let light_position = lowerer.lower_get_named_field(
        Value::Local(light),
        "Light",
        "position",
        MirType::Vec3,
        span,
    );
    let light_range = lowerer.lower_get_named_field(
        Value::Local(light),
        "Light",
        "range",
        MirType::Float,
        span,
    );
    let light_delta = lowerer.lower_binary_temp(
        MirType::Vec3,
        BinaryOp::Sub,
        light_position,
        shadow_origin.clone(),
        span,
    );
    let shadow_direction = lowerer.lower_call_temp(
        MirType::Vec3,
        SmolStr::new("normalize"),
        vec![light_delta.clone()],
        span,
    );
    let light_distance = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("length"),
        vec![light_delta],
        span,
    );
    let shadow_limit = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("min"),
        vec![light_distance, light_range],
        span,
    );
    let min_step = lowerer.lower_get_named_field(
        Value::Local(domain),
        "SceneDomain",
        "min_step",
        MirType::Float,
        span,
    );
    let hit_epsilon = lowerer.lower_get_named_field(
        Value::Local(domain),
        "SceneDomain",
        "hit_epsilon",
        MirType::Float,
        span,
    );
    let max_steps = lowerer.lower_get_named_field(
        Value::Local(domain),
        "SceneDomain",
        "max_steps",
        MirType::Integer,
        span,
    );
    let shadow_hit = lower_render_world_trace_call(
        &mut lowerer,
        Value::Local(world),
        Value::Local(domain),
        shadow_origin,
        shadow_direction,
        shadow_limit,
        min_step,
        hit_epsilon,
        max_steps,
        span,
    );
    let shadow_hit_flag = lowerer.lower_get_named_field(
        shadow_hit,
        "Hit3",
        "hit",
        MirType::Boolean,
        span,
    );
    let result_local = lowerer.new_local(SmolStr::new("$shadow_visibility"), true, MirType::Float);
    lowerer.assign_use(
        Place::Local(result_local),
        Value::Const(Literal::Float(1.0)),
        span,
    );
    lowerer.set_terminator(Terminator::Branch {
        cond: shadow_hit_flag,
        then_target: hit_block,
        else_target: miss_block,
        span,
    });

    lowerer.current_block = hit_block;
    lowerer.assign_use(
        Place::Local(result_local),
        Value::Const(Literal::Float(0.0)),
        span,
    );
    lowerer.set_terminator(Terminator::Jump {
        target: join_block,
        span,
    });

    lowerer.current_block = miss_block;
    lowerer.assign_use(
        Place::Local(result_local),
        Value::Const(Literal::Float(1.0)),
        span,
    );
    lowerer.set_terminator(Terminator::Jump {
        target: join_block,
        span,
    });

    lowerer.current_block = join_block;
    lowerer.set_terminator(Terminator::Return {
        value: Some(Value::Local(result_local)),
        span,
    });

    MirFunction {
        name: lowerer.name,
        params: lowerer.params,
        abi_params: vec![
            portable_abi_named_type("RegionCapture", module, type_tags),
            portable_abi_named_type("SceneDomain", module, type_tags),
            PortableAbiType::Vec3,
            PortableAbiType::Vec3,
            portable_abi_named_type("Light", module, type_tags),
        ],
        abi_return: PortableAbiType::F32,
        locals: lowerer.locals,
        temps: lowerer.temps,
        blocks: lowerer.blocks,
        entry,
        suspendable: false,
    }
}

fn lower_render_ambient_occlusion_helper(
    module: &hir::Module,
    type_tags: &HashMap<SmolStr, TypeTagId>,
    class_fields: &HashMap<SmolStr, Vec<SmolStr>>,
    class_field_defaults: &HashMap<SmolStr, Vec<Option<hir::FieldDefault>>>,
    function_names: &HashSet<SmolStr>,
    field_names: &HashSet<SmolStr>,
    shape_names: &HashSet<SmolStr>,
    shape_graphs: &HashMap<SmolStr, hir::ShapeGraph>,
    field_graphs: &HashMap<SmolStr, hir::FieldGraph>,
    field_bodies: &HashMap<SmolStr, hir::Body>,
    field_metadata: &HashMap<SmolStr, hir::FieldMetadata>,
    radiance_param_counts: &HashMap<SmolStr, usize>,
    volume_param_counts: &HashMap<SmolStr, usize>,
    result_functions: &HashSet<SmolStr>,
    class_method_ids: &HashMap<SmolStr, HashMap<SmolStr, u32>>,
    interface_methods: &HashMap<SmolStr, HashSet<SmolStr>>,
) -> MirFunction {
    let span = TextRange::empty(0.into());
    let mut lowerer = FunctionLowerer::new(
        SmolStr::new("__wr_render_ambient_occlusion_capture"),
        type_tags,
        class_fields,
        class_field_defaults,
        function_names,
        field_names,
        shape_names,
        shape_graphs,
        field_graphs,
        field_bodies,
        field_metadata,
        radiance_param_counts,
        volume_param_counts,
        result_functions,
        class_method_ids,
        interface_methods,
        false,
        None,
    );

    let world = declare_internal_param(&mut lowerer, "world", MirType::Named(SmolStr::new("RegionCapture")));
    let domain = declare_internal_param(&mut lowerer, "domain", MirType::Named(SmolStr::new("SceneDomain")));
    let hit_position = declare_internal_param(&mut lowerer, "hit_position", MirType::Vec3);
    let hit_normal = declare_internal_param(&mut lowerer, "hit_normal", MirType::Vec3);

    let entry = lowerer.new_block();
    lowerer.current_block = entry;
    let sample_a_offset = lowerer.lower_binary_temp(
        MirType::Vec3,
        BinaryOp::Mul,
        Value::Local(hit_normal),
        Value::Const(Literal::Float(0.06)),
        span,
    );
    let sample_a_point = lowerer.lower_binary_temp(
        MirType::Vec3,
        BinaryOp::Add,
        Value::Local(hit_position),
        sample_a_offset,
        span,
    );
    let sample_a = lower_render_world_distance_call(
        &mut lowerer,
        Value::Local(world),
        Value::Local(domain),
        sample_a_point,
        span,
    );
    let sample_b_offset = lowerer.lower_binary_temp(
        MirType::Vec3,
        BinaryOp::Mul,
        Value::Local(hit_normal),
        Value::Const(Literal::Float(0.14)),
        span,
    );
    let sample_b_point = lowerer.lower_binary_temp(
        MirType::Vec3,
        BinaryOp::Add,
        Value::Local(hit_position),
        sample_b_offset,
        span,
    );
    let sample_b = lower_render_world_distance_call(
        &mut lowerer,
        Value::Local(world),
        Value::Local(domain),
        sample_b_point,
        span,
    );
    let sample_c_offset = lowerer.lower_binary_temp(
        MirType::Vec3,
        BinaryOp::Mul,
        Value::Local(hit_normal),
        Value::Const(Literal::Float(0.28)),
        span,
    );
    let sample_c_point = lowerer.lower_binary_temp(
        MirType::Vec3,
        BinaryOp::Add,
        Value::Local(hit_position),
        sample_c_offset,
        span,
    );
    let sample_c = lower_render_world_distance_call(
        &mut lowerer,
        Value::Local(world),
        Value::Local(domain),
        sample_c_point,
        span,
    );

    let sample_a_gap = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Sub,
        Value::Const(Literal::Float(0.06)),
        sample_a,
        span,
    );
    let sample_b_gap = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Sub,
        Value::Const(Literal::Float(0.14)),
        sample_b,
        span,
    );
    let sample_c_gap = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Sub,
        Value::Const(Literal::Float(0.28)),
        sample_c,
        span,
    );
    let term_a = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Mul,
        sample_a_gap,
        Value::Const(Literal::Float(1.6)),
        span,
    );
    let term_b = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Mul,
        sample_b_gap,
        Value::Const(Literal::Float(1.1)),
        span,
    );
    let term_c = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Mul,
        sample_c_gap,
        Value::Const(Literal::Float(0.8)),
        span,
    );
    let term_ab = lowerer.lower_binary_temp(MirType::Float, BinaryOp::Add, term_a, term_b, span);
    let occlusion_sum = lowerer.lower_binary_temp(MirType::Float, BinaryOp::Add, term_ab, term_c, span);
    let occlusion = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("clamp"),
        vec![
            occlusion_sum,
            Value::Const(Literal::Float(0.0)),
            Value::Const(Literal::Float(1.0)),
        ],
        span,
    );
    let occlusion_scaled = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Mul,
        occlusion,
        Value::Const(Literal::Float(0.85)),
        span,
    );
    let ao = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Sub,
        Value::Const(Literal::Float(1.0)),
        occlusion_scaled,
        span,
    );
    lowerer.set_terminator(Terminator::Return {
        value: Some(ao),
        span,
    });

    MirFunction {
        name: lowerer.name,
        params: lowerer.params,
        abi_params: vec![
            portable_abi_named_type("RegionCapture", module, type_tags),
            portable_abi_named_type("SceneDomain", module, type_tags),
            PortableAbiType::Vec3,
            PortableAbiType::Vec3,
        ],
        abi_return: PortableAbiType::F32,
        locals: lowerer.locals,
        temps: lowerer.temps,
        blocks: lowerer.blocks,
        entry,
        suspendable: false,
    }
}

fn lower_render_scene_color_helper(
    module: &hir::Module,
    type_tags: &HashMap<SmolStr, TypeTagId>,
    class_fields: &HashMap<SmolStr, Vec<SmolStr>>,
    class_field_defaults: &HashMap<SmolStr, Vec<Option<hir::FieldDefault>>>,
    function_names: &HashSet<SmolStr>,
    field_names: &HashSet<SmolStr>,
    shape_names: &HashSet<SmolStr>,
    shape_graphs: &HashMap<SmolStr, hir::ShapeGraph>,
    field_graphs: &HashMap<SmolStr, hir::FieldGraph>,
    field_bodies: &HashMap<SmolStr, hir::Body>,
    field_metadata: &HashMap<SmolStr, hir::FieldMetadata>,
    radiance_param_counts: &HashMap<SmolStr, usize>,
    volume_param_counts: &HashMap<SmolStr, usize>,
    result_functions: &HashSet<SmolStr>,
    class_method_ids: &HashMap<SmolStr, HashMap<SmolStr, u32>>,
    interface_methods: &HashMap<SmolStr, HashSet<SmolStr>>,
) -> MirFunction {
    let span = TextRange::empty(0.into());
    let mut lowerer = FunctionLowerer::new(
        SmolStr::new("__wr_render_scene_color_capture"),
        type_tags,
        class_fields,
        class_field_defaults,
        function_names,
        field_names,
        shape_names,
        shape_graphs,
        field_graphs,
        field_bodies,
        field_metadata,
        radiance_param_counts,
        volume_param_counts,
        result_functions,
        class_method_ids,
        interface_methods,
        false,
        None,
    );

    let world = declare_internal_param(&mut lowerer, "world", MirType::Named(SmolStr::new("RegionCapture")));
    let domain = declare_internal_param(&mut lowerer, "domain", MirType::Named(SmolStr::new("SceneDomain")));
    let camera_position = declare_internal_param(&mut lowerer, "camera_position", MirType::Vec3);
    let light = declare_internal_param(&mut lowerer, "light", MirType::Named(SmolStr::new("Light")));
    let ray_direction = declare_internal_param(&mut lowerer, "ray_direction", MirType::Vec3);
    let fill_dir = declare_internal_param(&mut lowerer, "fill_dir", MirType::Vec3);

    let entry = lowerer.new_block();
    let hit_block = lowerer.new_block();
    let miss_block = lowerer.new_block();
    let join_block = lowerer.new_block();
    lowerer.current_block = entry;

    let max_distance = lowerer.lower_get_named_field(
        Value::Local(domain),
        "SceneDomain",
        "max_distance",
        MirType::Float,
        span,
    );
    let min_step = lowerer.lower_get_named_field(
        Value::Local(domain),
        "SceneDomain",
        "min_step",
        MirType::Float,
        span,
    );
    let hit_epsilon = lowerer.lower_get_named_field(
        Value::Local(domain),
        "SceneDomain",
        "hit_epsilon",
        MirType::Float,
        span,
    );
    let max_steps = lowerer.lower_get_named_field(
        Value::Local(domain),
        "SceneDomain",
        "max_steps",
        MirType::Integer,
        span,
    );
    let hit = lower_render_world_trace_call(
        &mut lowerer,
        Value::Local(world),
        Value::Local(domain),
        Value::Local(camera_position),
        Value::Local(ray_direction),
        max_distance,
        min_step.clone(),
        hit_epsilon.clone(),
        max_steps.clone(),
        span,
    );
    let hit_flag = lowerer.lower_get_named_field(hit.clone(), "Hit3", "hit", MirType::Boolean, span);
    let result_local = lowerer.new_local(SmolStr::new("$scene_color"), true, MirType::Vec3);
    let black = build_vec3_value(&mut lowerer, [0.0, 0.0, 0.0], span);
    lowerer.assign_use(Place::Local(result_local), black, span);
    lowerer.set_terminator(Terminator::Branch {
        cond: hit_flag,
        then_target: hit_block,
        else_target: miss_block,
        span,
    });

    lowerer.current_block = hit_block;
    let hit_position = lowerer.lower_get_named_field(hit.clone(), "Hit3", "position", MirType::Vec3, span);
    let hit_normal = lowerer.lower_get_named_field(hit.clone(), "Hit3", "normal", MirType::Vec3, span);
    let surface = lower_render_world_surface_call(
        &mut lowerer,
        Value::Local(world),
        Value::Local(domain),
        hit.clone(),
        span,
    );
    let captured_radiance = lower_render_world_radiance_call(
        &mut lowerer,
        Value::Local(world),
        Value::Local(domain),
        hit_position.clone(),
        Value::Local(ray_direction),
        span,
    );
    let medium = lower_render_world_medium_call(
        &mut lowerer,
        Value::Local(world),
        Value::Local(domain),
        hit_position.clone(),
        span,
    );
    let light_position = lowerer.lower_get_named_field(Value::Local(light), "Light", "position", MirType::Vec3, span);
    let light_intensity = lowerer.lower_get_named_field(Value::Local(light), "Light", "intensity", MirType::Vec3, span);
    let light_range = lowerer.lower_get_named_field(Value::Local(light), "Light", "range", MirType::Float, span);
    let key_delta = lowerer.lower_binary_temp(MirType::Vec3, BinaryOp::Sub, light_position, hit_position.clone(), span);
    let key_dir = lowerer.lower_call_temp(MirType::Vec3, SmolStr::new("normalize"), vec![key_delta.clone()], span);
    let view_delta = lowerer.lower_binary_temp(MirType::Vec3, BinaryOp::Sub, Value::Local(camera_position), hit_position.clone(), span);
    let view_dir = lowerer.lower_call_temp(MirType::Vec3, SmolStr::new("normalize"), vec![view_delta], span);
    let half_sum = lowerer.lower_binary_temp(MirType::Vec3, BinaryOp::Add, key_dir.clone(), view_dir.clone(), span);
    let half_dir = lowerer.lower_call_temp(
        MirType::Vec3,
        SmolStr::new("normalize"),
        vec![half_sum],
        span,
    );
    let distance_to_light = lowerer.lower_call_temp(MirType::Float, SmolStr::new("length"), vec![key_delta], span);
    let light_distance_ratio = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Div,
        distance_to_light.clone(),
        light_range,
        span,
    );
    let attenuation_base = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Sub,
        Value::Const(Literal::Float(1.0)),
        light_distance_ratio,
        span,
    );
    let attenuation = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("clamp"),
        vec![
            attenuation_base,
            Value::Const(Literal::Float(0.0)),
            Value::Const(Literal::Float(1.0)),
        ],
        span,
    );
    let ao = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("__wr_render_ambient_occlusion_capture"),
        vec![Value::Local(world), Value::Local(domain), hit_position.clone(), hit_normal.clone()],
        span,
    );
    let shadow = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("__wr_render_shadow_visibility_capture"),
        vec![Value::Local(world), Value::Local(domain), hit_position.clone(), hit_normal.clone(), Value::Local(light)],
        span,
    );
    let ndotl_raw = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("dot"),
        vec![hit_normal.clone(), key_dir.clone()],
        span,
    );
    let ndotl = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("max"),
        vec![ndotl_raw, Value::Const(Literal::Float(0.0))],
        span,
    );
    let ndotv_raw = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("dot"),
        vec![hit_normal.clone(), view_dir.clone()],
        span,
    );
    let _ndotv = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("max"),
        vec![ndotv_raw, Value::Const(Literal::Float(0.0))],
        span,
    );
    let ndoth_raw = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("dot"),
        vec![hit_normal.clone(), half_dir.clone()],
        span,
    );
    let ndoth = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("max"),
        vec![ndoth_raw, Value::Const(Literal::Float(0.0))],
        span,
    );
    let diffuse_base = lowerer.lower_binary_temp(MirType::Float, BinaryOp::Mul, ndotl, attenuation, span);
    let diffuse = lowerer.lower_binary_temp(MirType::Float, BinaryOp::Mul, diffuse_base, shadow, span);
    let fill_dot_raw = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("dot"),
        vec![hit_normal.clone(), Value::Local(fill_dir)],
        span,
    );
    let fill_dot = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("max"),
        vec![fill_dot_raw, Value::Const(Literal::Float(0.0))],
        span,
    );
    let fill = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Mul,
        fill_dot,
        Value::Const(Literal::Float(0.22)),
        span,
    );
    let roughness = lowerer.lower_get_named_field(surface.clone(), "Surface", "roughness", MirType::Float, span);
    let roughness_clamped = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("clamp"),
        vec![
            roughness,
            Value::Const(Literal::Float(0.0)),
            Value::Const(Literal::Float(1.0)),
        ],
        span,
    );
    let spec_power = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("mix"),
        vec![
            Value::Const(Literal::Float(48.0)),
            Value::Const(Literal::Float(8.0)),
            roughness_clamped,
        ],
        span,
    );
    let metalness = lowerer.lower_get_named_field(surface.clone(), "Surface", "metalness", MirType::Float, span);
    let clearcoat = lowerer.lower_get_named_field(surface.clone(), "Surface", "clearcoat", MirType::Float, span);
    let spec_raw = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("pow"),
        vec![ndoth, spec_power],
        span,
    );
    let metalness_term = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Mul,
        metalness.clone(),
        Value::Const(Literal::Float(0.25)),
        span,
    );
    let specular_strength_a = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Add,
        Value::Const(Literal::Float(0.10)),
        metalness_term,
        span,
    );
    let clearcoat_term = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Mul,
        clearcoat,
        Value::Const(Literal::Float(0.20)),
        span,
    );
    let specular_strength = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Add,
        specular_strength_a,
        clearcoat_term,
        span,
    );
    let highlight = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Mul,
        spec_raw,
        specular_strength.clone(),
        span,
    );
    let lighting_a = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Add,
        Value::Const(Literal::Float(0.12)),
        diffuse,
        span,
    );
    let lighting_b = lowerer.lower_binary_temp(MirType::Float, BinaryOp::Add, lighting_a, fill, span);
    let lighting = lowerer.lower_binary_temp(MirType::Float, BinaryOp::Mul, lighting_b, ao.clone(), span);
    let albedo = lowerer.lower_get_named_field(surface.clone(), "Surface", "albedo", MirType::Vec3, span);
    let intensity_x = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("__wr_vec_component"),
        vec![light_intensity.clone(), Value::Const(Literal::Integer(0))],
        span,
    );
    let intensity_y = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("__wr_vec_component"),
        vec![light_intensity.clone(), Value::Const(Literal::Integer(1))],
        span,
    );
    let intensity_z = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("__wr_vec_component"),
        vec![light_intensity, Value::Const(Literal::Integer(2))],
        span,
    );
    let albedo_x = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("__wr_vec_component"),
        vec![albedo.clone(), Value::Const(Literal::Integer(0))],
        span,
    );
    let albedo_y = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("__wr_vec_component"),
        vec![albedo.clone(), Value::Const(Literal::Integer(1))],
        span,
    );
    let albedo_z = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("__wr_vec_component"),
        vec![albedo, Value::Const(Literal::Integer(2))],
        span,
    );
    let direct_x_base = lowerer.lower_binary_temp(MirType::Float, BinaryOp::Mul, albedo_x, lighting.clone(), span);
    let direct_x_lit = lowerer.lower_binary_temp(MirType::Float, BinaryOp::Mul, direct_x_base, intensity_x, span);
    let direct_x_highlight = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Mul,
        highlight.clone(),
        Value::Const(Literal::Float(220.0)),
        span,
    );
    let direct_x_sum = lowerer.lower_binary_temp(MirType::Float, BinaryOp::Add, direct_x_lit, direct_x_highlight, span);
    let direct_x = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("clamp"),
        vec![direct_x_sum, Value::Const(Literal::Float(0.0)), Value::Const(Literal::Float(255.0))],
        span,
    );
    let direct_y_base = lowerer.lower_binary_temp(MirType::Float, BinaryOp::Mul, albedo_y, lighting.clone(), span);
    let direct_y_lit = lowerer.lower_binary_temp(MirType::Float, BinaryOp::Mul, direct_y_base, intensity_y, span);
    let direct_y_highlight = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Mul,
        highlight.clone(),
        Value::Const(Literal::Float(208.0)),
        span,
    );
    let direct_y_sum = lowerer.lower_binary_temp(MirType::Float, BinaryOp::Add, direct_y_lit, direct_y_highlight, span);
    let direct_y = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("clamp"),
        vec![direct_y_sum, Value::Const(Literal::Float(0.0)), Value::Const(Literal::Float(255.0))],
        span,
    );
    let direct_z_base = lowerer.lower_binary_temp(MirType::Float, BinaryOp::Mul, albedo_z, lighting, span);
    let direct_z_lit = lowerer.lower_binary_temp(MirType::Float, BinaryOp::Mul, direct_z_base, intensity_z, span);
    let direct_z_highlight = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Mul,
        highlight.clone(),
        Value::Const(Literal::Float(196.0)),
        span,
    );
    let direct_z_sum = lowerer.lower_binary_temp(MirType::Float, BinaryOp::Add, direct_z_lit, direct_z_highlight, span);
    let direct_z = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("clamp"),
        vec![direct_z_sum, Value::Const(Literal::Float(0.0)), Value::Const(Literal::Float(255.0))],
        span,
    );
    let direct = lowerer.lower_call_temp(
        MirType::Vec3,
        SmolStr::new("vec3"),
        vec![direct_x, direct_y, direct_z],
        span,
    );
    let medium_density = lowerer.lower_get_named_field(medium.clone(), "Medium", "density", MirType::Float, span);
    let fog_distance = lowerer.lower_binary_temp(MirType::Float, BinaryOp::Mul, medium_density, distance_to_light.clone(), span);
    let fog_distance_scaled = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Mul,
        fog_distance,
        Value::Const(Literal::Float(0.18)),
        span,
    );
    let one_minus_ao = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Sub,
        Value::Const(Literal::Float(1.0)),
        ao,
        span,
    );
    let fog_occlusion = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Mul,
        one_minus_ao,
        Value::Const(Literal::Float(0.08)),
        span,
    );
    let fog_sum = lowerer.lower_binary_temp(MirType::Float, BinaryOp::Add, fog_distance_scaled, fog_occlusion, span);
    let fog_strength = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("clamp"),
        vec![fog_sum, Value::Const(Literal::Float(0.0)), Value::Const(Literal::Float(0.55))],
        span,
    );
    let radiance_fog = lowerer.lower_binary_temp(
        MirType::Vec3,
        BinaryOp::Mul,
        captured_radiance.clone(),
        Value::Const(Literal::Float(0.22)),
        span,
    );
    let medium_emission = lowerer.lower_get_named_field(medium.clone(), "Medium", "emission", MirType::Vec3, span);
    let fog_color = lowerer.lower_binary_temp(MirType::Vec3, BinaryOp::Add, medium_emission.clone(), radiance_fog, span);
    let highlight_radiance = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Mul,
        highlight,
        Value::Const(Literal::Float(0.15)),
        span,
    );
    let radiance_scale = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Add,
        Value::Const(Literal::Float(0.25)),
        highlight_radiance,
        span,
    );
    let radiance_lit = lowerer.lower_binary_temp(
        MirType::Vec3,
        BinaryOp::Mul,
        captured_radiance,
        radiance_scale,
        span,
    );
    let surface_emissive = lowerer.lower_get_named_field(surface.clone(), "Surface", "emissive", MirType::Vec3, span);
    let lit_base = lowerer.lower_binary_temp(MirType::Vec3, BinaryOp::Add, direct, surface_emissive, span);
    let lit = lowerer.lower_binary_temp(MirType::Vec3, BinaryOp::Add, lit_base, radiance_lit, span);
    let hit_color = lowerer.lower_call_temp(
        MirType::Vec3,
        SmolStr::new("mix"),
        vec![lit, fog_color, fog_strength],
        span,
    );
    lowerer.assign_use(Place::Local(result_local), hit_color, span);
    lowerer.set_terminator(Terminator::Jump {
        target: join_block,
        span,
    });

    lowerer.current_block = miss_block;
    let miss_offset = lowerer.lower_binary_temp(
        MirType::Vec3,
        BinaryOp::Mul,
        Value::Local(ray_direction),
        Value::Const(Literal::Float(4.0)),
        span,
    );
    let miss_point = lowerer.lower_binary_temp(
        MirType::Vec3,
        BinaryOp::Add,
        Value::Local(camera_position),
        miss_offset,
        span,
    );
    let miss_radiance = lower_render_world_radiance_call(
        &mut lowerer,
        Value::Local(world),
        Value::Local(domain),
        miss_point.clone(),
        Value::Local(ray_direction),
        span,
    );
    let miss_medium = lower_render_world_medium_call(
        &mut lowerer,
        Value::Local(world),
        Value::Local(domain),
        miss_point,
        span,
    );
    let miss_density = lowerer.lower_get_named_field(miss_medium.clone(), "Medium", "density", MirType::Float, span);
    let miss_fog_raw = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Mul,
        miss_density,
        Value::Const(Literal::Float(3.0)),
        span,
    );
    let miss_fog = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("clamp"),
        vec![miss_fog_raw, Value::Const(Literal::Float(0.0)), Value::Const(Literal::Float(0.45))],
        span,
    );
    let miss_emission = lowerer.lower_get_named_field(miss_medium, "Medium", "emission", MirType::Vec3, span);
    let miss_radiance_scaled = lowerer.lower_binary_temp(
        MirType::Vec3,
        BinaryOp::Mul,
        miss_radiance.clone(),
        Value::Const(Literal::Float(0.28)),
        span,
    );
    let miss_mix_color = lowerer.lower_binary_temp(
        MirType::Vec3,
        BinaryOp::Add,
        miss_emission,
        miss_radiance_scaled,
        span,
    );
    let miss_color = lowerer.lower_call_temp(
        MirType::Vec3,
        SmolStr::new("mix"),
        vec![miss_radiance, miss_mix_color, miss_fog],
        span,
    );
    lowerer.assign_use(Place::Local(result_local), miss_color, span);
    lowerer.set_terminator(Terminator::Jump {
        target: join_block,
        span,
    });

    lowerer.current_block = join_block;
    lowerer.set_terminator(Terminator::Return {
        value: Some(Value::Local(result_local)),
        span,
    });

    MirFunction {
        name: lowerer.name,
        params: lowerer.params,
        abi_params: vec![
            portable_abi_named_type("RegionCapture", module, type_tags),
            portable_abi_named_type("SceneDomain", module, type_tags),
            PortableAbiType::Vec3,
            portable_abi_named_type("Light", module, type_tags),
            PortableAbiType::Vec3,
            PortableAbiType::Vec3,
        ],
        abi_return: PortableAbiType::Vec3,
        locals: lowerer.locals,
        temps: lowerer.temps,
        blocks: lowerer.blocks,
        entry,
        suspendable: false,
    }
}

fn lower_render_capture_to_ppm_helper(
    module: &hir::Module,
    type_tags: &HashMap<SmolStr, TypeTagId>,
    class_fields: &HashMap<SmolStr, Vec<SmolStr>>,
    class_field_defaults: &HashMap<SmolStr, Vec<Option<hir::FieldDefault>>>,
    function_names: &HashSet<SmolStr>,
    field_names: &HashSet<SmolStr>,
    shape_names: &HashSet<SmolStr>,
    shape_graphs: &HashMap<SmolStr, hir::ShapeGraph>,
    field_graphs: &HashMap<SmolStr, hir::FieldGraph>,
    field_bodies: &HashMap<SmolStr, hir::Body>,
    field_metadata: &HashMap<SmolStr, hir::FieldMetadata>,
    radiance_param_counts: &HashMap<SmolStr, usize>,
    volume_param_counts: &HashMap<SmolStr, usize>,
    result_functions: &HashSet<SmolStr>,
    class_method_ids: &HashMap<SmolStr, HashMap<SmolStr, u32>>,
    interface_methods: &HashMap<SmolStr, HashSet<SmolStr>>,
) -> MirFunction {
    let span = TextRange::empty(0.into());
    let mut lowerer = FunctionLowerer::new(
        SmolStr::new("__wr_render_capture_to_ppm"),
        type_tags,
        class_fields,
        class_field_defaults,
        function_names,
        field_names,
        shape_names,
        shape_graphs,
        field_graphs,
        field_bodies,
        field_metadata,
        radiance_param_counts,
        volume_param_counts,
        result_functions,
        class_method_ids,
        interface_methods,
        false,
        None,
    );

    let world = declare_internal_param(&mut lowerer, "world", MirType::Named(SmolStr::new("RegionCapture")));
    let domain = declare_internal_param(&mut lowerer, "domain", MirType::Named(SmolStr::new("SceneDomain")));
    let camera = declare_internal_param(&mut lowerer, "camera", MirType::Named(SmolStr::new("Camera")));
    let light = declare_internal_param(&mut lowerer, "light", MirType::Named(SmolStr::new("Light")));
    let width = declare_internal_param(&mut lowerer, "width", MirType::Integer);
    let height = declare_internal_param(&mut lowerer, "height", MirType::Integer);
    let world_up = declare_internal_param(&mut lowerer, "world_up", MirType::Vec3);
    let view_scale = declare_internal_param(&mut lowerer, "view_scale", MirType::Float);
    let fill_dir = declare_internal_param(&mut lowerer, "fill_dir", MirType::Vec3);

    let entry = lowerer.new_block();
    let y_head = lowerer.new_block();
    let y_body = lowerer.new_block();
    let x_head = lowerer.new_block();
    let x_body = lowerer.new_block();
    let row_done = lowerer.new_block();
    let exit = lowerer.new_block();
    lowerer.current_block = entry;

    let camera_position = lowerer.lower_get_named_field(Value::Local(camera), "Camera", "position", MirType::Vec3, span);
    let camera_forward = lowerer.lower_get_named_field(Value::Local(camera), "Camera", "forward", MirType::Vec3, span);
    let width_float = lowerer.lower_call_temp(MirType::Float, SmolStr::new("f32"), vec![Value::Local(width)], span);
    let height_float = lowerer.lower_call_temp(MirType::Float, SmolStr::new("f32"), vec![Value::Local(height)], span);
    let aspect = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Div,
        width_float.clone(),
        height_float.clone(),
        span,
    );
    let right_cross = lowerer.lower_call_temp(
        MirType::Vec3,
        SmolStr::new("cross"),
        vec![camera_forward.clone(), Value::Local(world_up)],
        span,
    );
    let right = lowerer.lower_call_temp(
        MirType::Vec3,
        SmolStr::new("normalize"),
        vec![right_cross],
        span,
    );
    let up_cross = lowerer.lower_call_temp(
        MirType::Vec3,
        SmolStr::new("cross"),
        vec![right.clone(), camera_forward.clone()],
        span,
    );
    let up = lowerer.lower_call_temp(
        MirType::Vec3,
        SmolStr::new("normalize"),
        vec![up_cross],
        span,
    );
    let ppm_local = lowerer.new_local(SmolStr::new("$ppm"), true, MirType::String);
    let header = lowerer.lower_string_interp_temp(
        vec![
            StringPartValue::Literal(SmolStr::new("P3\n")),
            StringPartValue::Value(Value::Local(width)),
            StringPartValue::Literal(SmolStr::new(" ")),
            StringPartValue::Value(Value::Local(height)),
            StringPartValue::Literal(SmolStr::new("\n255\n")),
        ],
        span,
    );
    lowerer.assign_use(Place::Local(ppm_local), header, span);
    let y_local = lowerer.new_local(SmolStr::new("$y"), true, MirType::Integer);
    lowerer.assign_use(Place::Local(y_local), Value::Const(Literal::Integer(0)), span);
    let x_local = lowerer.new_local(SmolStr::new("$x"), true, MirType::Integer);
    lowerer.assign_use(Place::Local(x_local), Value::Const(Literal::Integer(0)), span);
    lowerer.set_terminator(Terminator::Jump {
        target: y_head,
        span,
    });

    lowerer.current_block = y_head;
    let y_cond = lowerer.lower_binary_temp(
        MirType::Boolean,
        BinaryOp::Lt,
        Value::Local(y_local),
        Value::Local(height),
        span,
    );
    lowerer.set_terminator(Terminator::Branch {
        cond: y_cond,
        then_target: y_body,
        else_target: exit,
        span,
    });

    lowerer.current_block = y_body;
    lowerer.assign_use(Place::Local(x_local), Value::Const(Literal::Integer(0)), span);
    lowerer.set_terminator(Terminator::Jump {
        target: x_head,
        span,
    });

    lowerer.current_block = x_head;
    let x_cond = lowerer.lower_binary_temp(
        MirType::Boolean,
        BinaryOp::Lt,
        Value::Local(x_local),
        Value::Local(width),
        span,
    );
    lowerer.set_terminator(Terminator::Branch {
        cond: x_cond,
        then_target: x_body,
        else_target: row_done,
        span,
    });

    lowerer.current_block = x_body;
    let x_float = lowerer.lower_call_temp(MirType::Float, SmolStr::new("f32"), vec![Value::Local(x_local)], span);
    let y_float = lowerer.lower_call_temp(MirType::Float, SmolStr::new("f32"), vec![Value::Local(y_local)], span);
    let sample_u_num = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Add,
        x_float,
        Value::Const(Literal::Float(0.5)),
        span,
    );
    let sample_u = lowerer.lower_binary_temp(MirType::Float, BinaryOp::Div, sample_u_num, width_float.clone(), span);
    let sample_v_num = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Add,
        y_float,
        Value::Const(Literal::Float(0.5)),
        span,
    );
    let sample_v = lowerer.lower_binary_temp(MirType::Float, BinaryOp::Div, sample_v_num, height_float.clone(), span);
    let centered_u = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Sub,
        sample_u,
        Value::Const(Literal::Float(0.5)),
        span,
    );
    let doubled_u = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Mul,
        centered_u,
        Value::Const(Literal::Float(2.0)),
        span,
    );
    let aspect_u = lowerer.lower_binary_temp(MirType::Float, BinaryOp::Mul, doubled_u, aspect.clone(), span);
    let screen_x = lowerer.lower_binary_temp(MirType::Float, BinaryOp::Mul, aspect_u, Value::Local(view_scale), span);
    let centered_v = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Sub,
        Value::Const(Literal::Float(0.5)),
        sample_v,
        span,
    );
    let doubled_v = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Mul,
        centered_v,
        Value::Const(Literal::Float(2.0)),
        span,
    );
    let screen_y = lowerer.lower_binary_temp(MirType::Float, BinaryOp::Mul, doubled_v, Value::Local(view_scale), span);
    let ray_x = lowerer.lower_binary_temp(MirType::Vec3, BinaryOp::Mul, right.clone(), screen_x, span);
    let ray_xy = lowerer.lower_binary_temp(MirType::Vec3, BinaryOp::Add, camera_forward.clone(), ray_x, span);
    let ray_y = lowerer.lower_binary_temp(MirType::Vec3, BinaryOp::Mul, up.clone(), screen_y, span);
    let ray_base = lowerer.lower_binary_temp(MirType::Vec3, BinaryOp::Add, ray_xy, ray_y, span);
    let ray = lowerer.lower_call_temp(
        MirType::Vec3,
        SmolStr::new("normalize"),
        vec![ray_base],
        span,
    );
    let shaded = lowerer.lower_call_temp(
        MirType::Vec3,
        SmolStr::new("__wr_render_scene_color_capture"),
        vec![
            Value::Local(world),
            Value::Local(domain),
            camera_position.clone(),
            Value::Local(light),
            ray,
            Value::Local(fill_dir),
        ],
        span,
    );
    let shaded_x = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("__wr_vec_component"),
        vec![shaded.clone(), Value::Const(Literal::Integer(0))],
        span,
    );
    let shaded_y = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("__wr_vec_component"),
        vec![shaded.clone(), Value::Const(Literal::Integer(1))],
        span,
    );
    let shaded_z = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("__wr_vec_component"),
        vec![shaded, Value::Const(Literal::Integer(2))],
        span,
    );
    let shaded_x_clamped = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("clamp"),
        vec![shaded_x, Value::Const(Literal::Float(0.0)), Value::Const(Literal::Float(255.0))],
        span,
    );
    let r = lowerer.lower_call_temp(MirType::Integer, SmolStr::new("i32"), vec![shaded_x_clamped], span);
    let shaded_y_clamped = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("clamp"),
        vec![shaded_y, Value::Const(Literal::Float(0.0)), Value::Const(Literal::Float(255.0))],
        span,
    );
    let g = lowerer.lower_call_temp(MirType::Integer, SmolStr::new("i32"), vec![shaded_y_clamped], span);
    let shaded_z_clamped = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("clamp"),
        vec![shaded_z, Value::Const(Literal::Float(0.0)), Value::Const(Literal::Float(255.0))],
        span,
    );
    let b = lowerer.lower_call_temp(MirType::Integer, SmolStr::new("i32"), vec![shaded_z_clamped], span);
    let line = lowerer.lower_string_interp_temp(
        vec![
            StringPartValue::Value(r),
            StringPartValue::Literal(SmolStr::new(" ")),
            StringPartValue::Value(g),
            StringPartValue::Literal(SmolStr::new(" ")),
            StringPartValue::Value(b),
            StringPartValue::Literal(SmolStr::new("\n")),
        ],
        span,
    );
    let ppm_next = lowerer.lower_string_concat_temp(Value::Local(ppm_local), line, span);
    lowerer.assign_use(Place::Local(ppm_local), ppm_next, span);
    let x_next = lowerer.lower_binary_temp(
        MirType::Integer,
        BinaryOp::Add,
        Value::Local(x_local),
        Value::Const(Literal::Integer(1)),
        span,
    );
    lowerer.assign_use(Place::Local(x_local), x_next, span);
    lowerer.set_terminator(Terminator::Jump {
        target: x_head,
        span,
    });

    lowerer.current_block = row_done;
    let y_next = lowerer.lower_binary_temp(
        MirType::Integer,
        BinaryOp::Add,
        Value::Local(y_local),
        Value::Const(Literal::Integer(1)),
        span,
    );
    lowerer.assign_use(Place::Local(y_local), y_next, span);
    lowerer.set_terminator(Terminator::Jump {
        target: y_head,
        span,
    });

    lowerer.current_block = exit;
    lowerer.set_terminator(Terminator::Return {
        value: Some(Value::Local(ppm_local)),
        span,
    });

    MirFunction {
        name: lowerer.name,
        params: lowerer.params,
        abi_params: vec![
            portable_abi_named_type("RegionCapture", module, type_tags),
            portable_abi_named_type("SceneDomain", module, type_tags),
            portable_abi_named_type("Camera", module, type_tags),
            portable_abi_named_type("Light", module, type_tags),
            PortableAbiType::I64,
            PortableAbiType::I64,
            PortableAbiType::Vec3,
            PortableAbiType::F32,
            PortableAbiType::Vec3,
        ],
        abi_return: PortableAbiType::Value,
        locals: lowerer.locals,
        temps: lowerer.temps,
        blocks: lowerer.blocks,
        entry,
        suspendable: false,
    }
}

fn lower_shape_distance_helper(
    shape: &hir::Shape,
    type_tags: &HashMap<SmolStr, TypeTagId>,
    class_fields: &HashMap<SmolStr, Vec<SmolStr>>,
    class_field_defaults: &HashMap<SmolStr, Vec<Option<hir::FieldDefault>>>,
    function_names: &HashSet<SmolStr>,
    field_names: &HashSet<SmolStr>,
    shape_names: &HashSet<SmolStr>,
    shape_graphs: &HashMap<SmolStr, hir::ShapeGraph>,
    field_graphs: &HashMap<SmolStr, hir::FieldGraph>,
    field_bodies: &HashMap<SmolStr, hir::Body>,
    field_metadata: &HashMap<SmolStr, hir::FieldMetadata>,
    radiance_param_counts: &HashMap<SmolStr, usize>,
    volume_param_counts: &HashMap<SmolStr, usize>,
    result_functions: &HashSet<SmolStr>,
    class_method_ids: &HashMap<SmolStr, HashMap<SmolStr, u32>>,
    interface_methods: &HashMap<SmolStr, HashSet<SmolStr>>,
) -> MirFunction {
    let helper_name = SmolStr::new(format!("__wr_shape_distance_{}", shape.name));
    let span = TextRange::empty(0.into());
    let mut lowerer = FunctionLowerer::new(
        helper_name.clone(),
        type_tags,
        class_fields,
        class_field_defaults,
        function_names,
        field_names,
        shape_names,
        shape_graphs,
        field_graphs,
        field_bodies,
        field_metadata,
        radiance_param_counts,
        volume_param_counts,
        result_functions,
        class_method_ids,
        interface_methods,
        false,
        None,
    );

    let point = lowerer.new_local(SmolStr::new("p"), false, MirType::Vec3);
    lowerer.declare_local(SmolStr::new("p"), point);
    lowerer.params.push(point);

    let entry = lowerer.new_block();
    lowerer.current_block = entry;
    let distance = lowerer.lower_shape_distance_expr(
        &shape.graph.as_ref().expect("shape graph").root,
        Value::Local(point),
        span,
    );
    lowerer.set_terminator(Terminator::Return {
        value: Some(distance),
        span,
    });

    MirFunction {
        name: helper_name,
        params: lowerer.params,
        abi_params: vec![PortableAbiType::Vec3],
        abi_return: PortableAbiType::F32,
        locals: lowerer.locals,
        temps: lowerer.temps,
        blocks: lowerer.blocks,
        entry,
        suspendable: false,
    }
}

fn lower_shape_trace_helper(
    shape: &hir::Shape,
    _module: &hir::Module,
    type_tags: &HashMap<SmolStr, TypeTagId>,
    class_fields: &HashMap<SmolStr, Vec<SmolStr>>,
    class_field_defaults: &HashMap<SmolStr, Vec<Option<hir::FieldDefault>>>,
    function_names: &HashSet<SmolStr>,
    field_names: &HashSet<SmolStr>,
    shape_names: &HashSet<SmolStr>,
    shape_graphs: &HashMap<SmolStr, hir::ShapeGraph>,
    field_graphs: &HashMap<SmolStr, hir::FieldGraph>,
    field_bodies: &HashMap<SmolStr, hir::Body>,
    field_metadata: &HashMap<SmolStr, hir::FieldMetadata>,
    radiance_param_counts: &HashMap<SmolStr, usize>,
    volume_param_counts: &HashMap<SmolStr, usize>,
    result_functions: &HashSet<SmolStr>,
    class_method_ids: &HashMap<SmolStr, HashMap<SmolStr, u32>>,
    interface_methods: &HashMap<SmolStr, HashSet<SmolStr>>,
) -> MirFunction {
    let helper_name = SmolStr::new(format!("__wr_shape_trace_{}", shape.name));
    let span = TextRange::empty(0.into());
    let mut lowerer = FunctionLowerer::new(
        helper_name.clone(),
        type_tags,
        class_fields,
        class_field_defaults,
        function_names,
        field_names,
        shape_names,
        shape_graphs,
        field_graphs,
        field_bodies,
        field_metadata,
        radiance_param_counts,
        volume_param_counts,
        result_functions,
        class_method_ids,
        interface_methods,
        false,
        None,
    );

    let origin = lowerer.new_local(SmolStr::new("origin"), false, MirType::Vec3);
    let direction = lowerer.new_local(SmolStr::new("direction"), false, MirType::Vec3);
    let max_distance = lowerer.new_local(SmolStr::new("max_distance"), false, MirType::Float);
    let min_step = lowerer.new_local(SmolStr::new("min_step"), false, MirType::Float);
    let hit_epsilon = lowerer.new_local(SmolStr::new("hit_epsilon"), false, MirType::Float);
    let max_steps = lowerer.new_local(SmolStr::new("max_steps"), false, MirType::Integer);
    for (name, local) in [
        (SmolStr::new("origin"), origin),
        (SmolStr::new("direction"), direction),
        (SmolStr::new("max_distance"), max_distance),
        (SmolStr::new("min_step"), min_step),
        (SmolStr::new("hit_epsilon"), hit_epsilon),
        (SmolStr::new("max_steps"), max_steps),
    ] {
        lowerer.declare_local(name, local);
        lowerer.params.push(local);
    }

    let entry = lowerer.new_block();
    lowerer.current_block = entry;

    let _ = lowerer.lower_call_temp(
        MirType::Nil,
        SmolStr::new("__wr_metrics_scene_trace"),
        vec![],
        span,
    );
    let trace = shape.graph.as_ref().map(|graph| graph.trace);
    match trace.map(|trace| trace.class) {
        Some(FieldClass::Exact) => {
            let _ = lowerer.lower_call_temp(
                MirType::Nil,
                SmolStr::new("__wr_metrics_scene_trace_exact_path"),
                vec![],
                span,
            );
        }
        _ => {
            let _ = lowerer.lower_call_temp(
                MirType::Nil,
                SmolStr::new("__wr_metrics_scene_trace_conservative_path"),
                vec![],
                span,
            );
        }
    }
    let field_sample_metric_id = lowerer.lower_call_temp(
        MirType::Integer,
        SmolStr::new("__wr_metrics_field_sample_id"),
        vec![],
        span,
    );
    let field_samples_before = lowerer.new_local(
        SmolStr::new("$shape_field_samples_before"),
        true,
        MirType::Integer,
    );
    let field_samples_start = lowerer.lower_call_temp(
        MirType::Integer,
        SmolStr::new("__wr_metrics_get"),
        vec![field_sample_metric_id.clone()],
        span,
    );
    lowerer.assign_use(
        Place::Local(field_samples_before),
        field_samples_start,
        span,
    );

    let total = lowerer.new_local(SmolStr::new("$shape_total"), true, MirType::Float);
    lowerer.assign_use(Place::Local(total), Value::Const(Literal::Float(0.0)), span);

    let has_hit = lowerer.new_local(SmolStr::new("$shape_hit"), true, MirType::Boolean);
    lowerer.assign_use(
        Place::Local(has_hit),
        Value::Const(Literal::Boolean(false)),
        span,
    );

    let position = lowerer.new_local(SmolStr::new("$shape_position"), true, MirType::Vec3);
    lowerer.assign_use(Place::Local(position), Value::Local(origin), span);

    let normal_default = lowerer.lower_call_temp(
        MirType::Vec3,
        SmolStr::new("vec3"),
        vec![
            Value::Const(Literal::Float(0.0)),
            Value::Const(Literal::Float(0.0)),
            Value::Const(Literal::Float(1.0)),
        ],
        span,
    );
    let normal = lowerer.new_local(SmolStr::new("$shape_normal"), true, MirType::Vec3);
    lowerer.assign_use(Place::Local(normal), normal_default, span);

    let payload = lowerer.new_local(
        SmolStr::new("$shape_payload"),
        true,
        MirType::Named(SmolStr::new("Payload")),
    );
    let default_payload = lowerer.build_default_payload(span);
    lowerer.assign_use(Place::Local(payload), default_payload, span);

    let feature_id = lowerer.new_local(SmolStr::new("$shape_feature_id"), true, MirType::Integer);
    lowerer.assign_use(
        Place::Local(feature_id),
        Value::Const(Literal::Integer(0)),
        span,
    );

    let step_count = lowerer.new_local(SmolStr::new("$shape_steps"), true, MirType::Integer);
    lowerer.assign_use(
        Place::Local(step_count),
        Value::Const(Literal::Integer(0)),
        span,
    );

    let loop_check = lowerer.new_block();
    let loop_body = lowerer.new_block();
    let hit_block = lowerer.new_block();
    let advance_block = lowerer.new_block();
    let continue_block = lowerer.new_block();
    let end_block = lowerer.new_block();
    lowerer.set_terminator(Terminator::Jump {
        target: loop_check,
        span,
    });

    lowerer.current_block = loop_check;
    let within_distance = lowerer.lower_binary_temp(
        MirType::Boolean,
        BinaryOp::Lt,
        Value::Local(total),
        Value::Local(max_distance),
        span,
    );
    let not_hit = lowerer.lower_unary_temp(
        MirType::Boolean,
        hir::UnaryOp::Not,
        Value::Local(has_hit),
        span,
    );
    let cond_a = lowerer.lower_binary_temp(
        MirType::Boolean,
        BinaryOp::And,
        within_distance,
        not_hit,
        span,
    );
    let within_steps = lowerer.lower_binary_temp(
        MirType::Boolean,
        BinaryOp::Lt,
        Value::Local(step_count),
        Value::Local(max_steps),
        span,
    );
    let cond =
        lowerer.lower_binary_temp(MirType::Boolean, BinaryOp::And, cond_a, within_steps, span);
    lowerer.set_terminator(Terminator::Branch {
        cond,
        then_target: loop_body,
        else_target: end_block,
        span,
    });

    lowerer.current_block = loop_body;
    let scaled_direction = lowerer.lower_binary_temp(
        MirType::Vec3,
        BinaryOp::Mul,
        Value::Local(direction),
        Value::Local(total),
        span,
    );
    let next_position = lowerer.lower_binary_temp(
        MirType::Vec3,
        BinaryOp::Add,
        Value::Local(origin),
        scaled_direction,
        span,
    );
    lowerer.assign_use(Place::Local(position), next_position, span);
    let sampled_distance =
        lowerer.lower_shape_distance_call(&shape.name, Value::Local(position), span);
    let is_hit = lowerer.lower_binary_temp(
        MirType::Boolean,
        BinaryOp::Lt,
        sampled_distance.clone(),
        Value::Local(hit_epsilon),
        span,
    );
    lowerer.set_terminator(Terminator::Branch {
        cond: is_hit,
        then_target: hit_block,
        else_target: advance_block,
        span,
    });

    lowerer.current_block = hit_block;
    lowerer.assign_use(
        Place::Local(has_hit),
        Value::Const(Literal::Boolean(true)),
        span,
    );
    let shape_graph = shape.graph.as_ref().expect("shape graph");
    let (_, payload_value, feature_id_value) = lowerer.lower_shape_payload_selection(
        &shape_graph.root,
        shape_graph.provenance.as_ref(),
        Value::Local(position),
        Value::Local(hit_epsilon),
        &mut vec![shape.name.clone()],
        span,
    );
    lowerer.assign_use(Place::Local(payload), payload_value, span);
    lowerer.assign_use(Place::Local(feature_id), feature_id_value, span);
    let normal_value = lowerer.lower_shape_normal_call(&shape.name, Value::Local(position), span);
    lowerer.assign_use(Place::Local(normal), normal_value, span);
    lowerer.set_terminator(Terminator::Jump {
        target: continue_block,
        span,
    });

    lowerer.current_block = advance_block;
    let step_scale = match trace.map(|trace| trace.class) {
        Some(FieldClass::Exact) => Value::Const(Literal::Float(1.0)),
        _ => Value::Const(Literal::Float(0.75)),
    };
    let scaled_step = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Mul,
        sampled_distance,
        step_scale,
        span,
    );
    let step = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("max"),
        vec![scaled_step, Value::Local(min_step)],
        span,
    );
    let next_total = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Add,
        Value::Local(total),
        step,
        span,
    );
    lowerer.assign_use(Place::Local(total), next_total, span);
    lowerer.set_terminator(Terminator::Jump {
        target: continue_block,
        span,
    });

    lowerer.current_block = continue_block;
    let next_steps = lowerer.lower_binary_temp(
        MirType::Integer,
        BinaryOp::Add,
        Value::Local(step_count),
        Value::Const(Literal::Integer(1)),
        span,
    );
    lowerer.assign_use(Place::Local(step_count), next_steps, span);
    lowerer.set_terminator(Terminator::Jump {
        target: loop_check,
        span,
    });

    lowerer.current_block = end_block;
    let (_, local_position_value) = lowerer.lower_shape_local_point_selection(
        &shape_graph.root,
        Value::Local(feature_id),
        Value::Local(position),
        &mut vec![shape.name.clone()],
        span,
    );
    let mut hit_class = lowerer.synthetic_class_target_info("Hit3");
    FunctionLowerer::set_class_field_value(&mut hit_class, "hit", Value::Local(has_hit));
    FunctionLowerer::set_class_field_value(&mut hit_class, "distance", Value::Local(total));
    FunctionLowerer::set_class_field_value(&mut hit_class, "position", Value::Local(position));
    FunctionLowerer::set_class_field_value(&mut hit_class, "normal", Value::Local(normal));
    FunctionLowerer::set_class_field_value(&mut hit_class, "local_position", local_position_value);
    FunctionLowerer::set_class_field_value(&mut hit_class, "local_normal", Value::Local(normal));
    let shading_frame =
        lowerer.lower_stable_surface_frame(Value::Local(position), Value::Local(normal), span);
    FunctionLowerer::set_class_field_value(&mut hit_class, "shading_frame", shading_frame);
    FunctionLowerer::set_class_field_value(&mut hit_class, "steps", Value::Local(step_count));
    FunctionLowerer::set_class_field_value(&mut hit_class, "feature_id", Value::Local(feature_id));
    FunctionLowerer::set_class_field_value(
        &mut hit_class,
        "root_shape_id",
        Value::Const(Literal::Integer(stable_shape_capture_id(&shape.name))),
    );
    FunctionLowerer::set_class_field_value(&mut hit_class, "payload", Value::Local(payload));
    let hit_value = lowerer.build_class_instance(&hit_class, span);
    let field_samples_after = lowerer.lower_call_temp(
        MirType::Integer,
        SmolStr::new("__wr_metrics_get"),
        vec![field_sample_metric_id],
        span,
    );
    let field_sample_delta = lowerer.lower_binary_temp(
        MirType::Integer,
        BinaryOp::Sub,
        field_samples_after,
        Value::Local(field_samples_before),
        span,
    );
    let record_hit_block = lowerer.new_block();
    let skip_hit_block = lowerer.new_block();
    let return_block = lowerer.new_block();
    lowerer.set_terminator(Terminator::Branch {
        cond: Value::Local(has_hit),
        then_target: record_hit_block,
        else_target: skip_hit_block,
        span,
    });
    lowerer.current_block = record_hit_block;
    let _ = lowerer.lower_call_temp(
        MirType::Nil,
        SmolStr::new("__wr_metrics_scene_trace_hit"),
        vec![Value::Local(step_count), field_sample_delta],
        span,
    );
    lowerer.set_terminator(Terminator::Jump {
        target: return_block,
        span,
    });
    lowerer.current_block = skip_hit_block;
    lowerer.set_terminator(Terminator::Jump {
        target: return_block,
        span,
    });
    lowerer.current_block = return_block;
    lowerer.set_terminator(Terminator::Return {
        value: Some(hit_value),
        span,
    });

    MirFunction {
        name: helper_name,
        params: lowerer.params,
        abi_params: vec![
            PortableAbiType::Vec3,
            PortableAbiType::Vec3,
            PortableAbiType::F32,
            PortableAbiType::F32,
            PortableAbiType::F32,
            PortableAbiType::I64,
        ],
        abi_return: PortableAbiType::Struct {
            name: SmolStr::new("Hit3"),
            class_id: type_tags
                .get(&SmolStr::new("Hit3"))
                .map(|id| id.0 as u32)
                .unwrap_or_default(),
            fields: portable_value_struct_abi("Hit3", _module, type_tags, &mut HashSet::new())
                .and_then(|abi| match abi {
                    PortableAbiType::Struct { fields, .. } => Some(fields),
                    _ => None,
                })
                .unwrap_or_default(),
        },
        locals: lowerer.locals,
        temps: lowerer.temps,
        blocks: lowerer.blocks,
        entry,
        suspendable: false,
    }
}

fn lower_shape_surface_helper(
    shape: &hir::Shape,
    module: &hir::Module,
    type_tags: &HashMap<SmolStr, TypeTagId>,
    class_fields: &HashMap<SmolStr, Vec<SmolStr>>,
    class_field_defaults: &HashMap<SmolStr, Vec<Option<hir::FieldDefault>>>,
    function_names: &HashSet<SmolStr>,
    field_names: &HashSet<SmolStr>,
    shape_names: &HashSet<SmolStr>,
    shape_graphs: &HashMap<SmolStr, hir::ShapeGraph>,
    field_graphs: &HashMap<SmolStr, hir::FieldGraph>,
    field_bodies: &HashMap<SmolStr, hir::Body>,
    field_metadata: &HashMap<SmolStr, hir::FieldMetadata>,
    radiance_param_counts: &HashMap<SmolStr, usize>,
    volume_param_counts: &HashMap<SmolStr, usize>,
    result_functions: &HashSet<SmolStr>,
    class_method_ids: &HashMap<SmolStr, HashMap<SmolStr, u32>>,
    interface_methods: &HashMap<SmolStr, HashSet<SmolStr>>,
) -> MirFunction {
    let helper_name = SmolStr::new(format!("__wr_shape_surface_{}", shape.name));
    let span = TextRange::empty(0.into());
    let mut lowerer = FunctionLowerer::new(
        helper_name.clone(),
        type_tags,
        class_fields,
        class_field_defaults,
        function_names,
        field_names,
        shape_names,
        shape_graphs,
        field_graphs,
        field_bodies,
        field_metadata,
        radiance_param_counts,
        volume_param_counts,
        result_functions,
        class_method_ids,
        interface_methods,
        false,
        None,
    );

    let hit = lowerer.new_local(
        SmolStr::new("hit"),
        false,
        MirType::Named(SmolStr::new("Hit3")),
    );
    lowerer.declare_local(SmolStr::new("hit"), hit);
    lowerer.params.push(hit);

    let entry = lowerer.new_block();
    lowerer.current_block = entry;
    let feature_id_temp = lowerer.new_temp(MirType::Integer);
    lowerer.push_stmt(MirStmt::Assign {
        place: Place::Temp(feature_id_temp),
        value: Rvalue::GetField {
            base: Value::Local(hit),
            field: SmolStr::new("feature_id"),
            slot: lowerer.field_slot("Hit3", "feature_id"),
        },
        span,
    });
    let (_, surface) = lowerer.lower_shape_surface_selection(
        &shape.graph.as_ref().expect("shape graph").root,
        Value::Temp(feature_id_temp),
        Value::Local(hit),
        &mut vec![shape.name.clone()],
        span,
    );
    lowerer.set_terminator(Terminator::Return {
        value: Some(surface),
        span,
    });

    MirFunction {
        name: helper_name,
        params: lowerer.params,
        abi_params: vec![portable_abi_from_type_ref(
            Some(&hir::TypeRef {
                name: SmolStr::new("Hit3"),
                name_span: None,
                args: Vec::new(),
            }),
            module,
            type_tags,
            &mut HashSet::new(),
        )],
        abi_return: portable_abi_from_type_ref(
            Some(&hir::TypeRef {
                name: SmolStr::new("Surface"),
                name_span: None,
                args: Vec::new(),
            }),
            module,
            type_tags,
            &mut HashSet::new(),
        ),
        locals: lowerer.locals,
        temps: lowerer.temps,
        blocks: lowerer.blocks,
        entry,
        suspendable: false,
    }
}

impl FunctionLowerer {
    fn lower_stable_surface_frame(
        &mut self,
        position: Value,
        normal: Value,
        span: TextRange,
    ) -> Value {
        let zero = Value::Const(Literal::Float(0.0));
        let one = Value::Const(Literal::Float(1.0));

        let unit_normal =
            self.lower_call_temp(MirType::Vec3, SmolStr::new("normalize"), vec![normal], span);
        let world_up = self.lower_call_temp(
            MirType::Vec3,
            SmolStr::new("vec3"),
            vec![zero.clone(), one.clone(), zero.clone()],
            span,
        );
        let world_right = self.lower_call_temp(
            MirType::Vec3,
            SmolStr::new("vec3"),
            vec![one.clone(), zero.clone(), zero.clone()],
            span,
        );
        let tangent_seed = self.lower_call_temp(
            MirType::Vec3,
            SmolStr::new("cross"),
            vec![world_up, unit_normal.clone()],
            span,
        );
        let tangent_seed_len = self.lower_call_temp(
            MirType::Float,
            SmolStr::new("length"),
            vec![tangent_seed.clone()],
            span,
        );
        let tangent = self.new_local(SmolStr::new("$surface_frame_tangent"), true, MirType::Vec3);
        self.assign_use(Place::Local(tangent), tangent_seed, span);
        let tangent_fallback_block = self.new_block();
        let tangent_normalize_block = self.new_block();
        let tangent_merge_block = self.new_block();
        let needs_fallback = self.lower_binary_temp(
            MirType::Boolean,
            BinaryOp::Eq,
            tangent_seed_len,
            zero.clone(),
            span,
        );
        self.set_terminator(Terminator::Branch {
            cond: needs_fallback,
            then_target: tangent_fallback_block,
            else_target: tangent_normalize_block,
            span,
        });
        self.current_block = tangent_fallback_block;
        let tangent_fallback = self.lower_call_temp(
            MirType::Vec3,
            SmolStr::new("cross"),
            vec![world_right, unit_normal.clone()],
            span,
        );
        self.assign_use(Place::Local(tangent), tangent_fallback, span);
        self.set_terminator(Terminator::Jump {
            target: tangent_normalize_block,
            span,
        });
        self.current_block = tangent_normalize_block;
        let tangent_normalized = self.lower_call_temp(
            MirType::Vec3,
            SmolStr::new("normalize"),
            vec![Value::Local(tangent)],
            span,
        );
        self.assign_use(Place::Local(tangent), tangent_normalized, span);
        self.set_terminator(Terminator::Jump {
            target: tangent_merge_block,
            span,
        });
        self.current_block = tangent_merge_block;

        let bitangent = self.lower_call_temp(
            MirType::Vec3,
            SmolStr::new("cross"),
            vec![unit_normal.clone(), Value::Local(tangent)],
            span,
        );

        let tangent_x = self.lower_call_temp(
            MirType::Float,
            SmolStr::new("__wr_vec_component"),
            vec![Value::Local(tangent), Value::Const(Literal::Integer(0))],
            span,
        );
        let tangent_y = self.lower_call_temp(
            MirType::Float,
            SmolStr::new("__wr_vec_component"),
            vec![Value::Local(tangent), Value::Const(Literal::Integer(1))],
            span,
        );
        let tangent_z = self.lower_call_temp(
            MirType::Float,
            SmolStr::new("__wr_vec_component"),
            vec![Value::Local(tangent), Value::Const(Literal::Integer(2))],
            span,
        );
        let bitangent_x = self.lower_call_temp(
            MirType::Float,
            SmolStr::new("__wr_vec_component"),
            vec![bitangent.clone(), Value::Const(Literal::Integer(0))],
            span,
        );
        let bitangent_y = self.lower_call_temp(
            MirType::Float,
            SmolStr::new("__wr_vec_component"),
            vec![bitangent.clone(), Value::Const(Literal::Integer(1))],
            span,
        );
        let bitangent_z = self.lower_call_temp(
            MirType::Float,
            SmolStr::new("__wr_vec_component"),
            vec![bitangent.clone(), Value::Const(Literal::Integer(2))],
            span,
        );
        let normal_x = self.lower_call_temp(
            MirType::Float,
            SmolStr::new("__wr_vec_component"),
            vec![unit_normal.clone(), Value::Const(Literal::Integer(0))],
            span,
        );
        let normal_y = self.lower_call_temp(
            MirType::Float,
            SmolStr::new("__wr_vec_component"),
            vec![unit_normal.clone(), Value::Const(Literal::Integer(1))],
            span,
        );
        let normal_z = self.lower_call_temp(
            MirType::Float,
            SmolStr::new("__wr_vec_component"),
            vec![unit_normal.clone(), Value::Const(Literal::Integer(2))],
            span,
        );
        let position_x = self.lower_call_temp(
            MirType::Float,
            SmolStr::new("__wr_vec_component"),
            vec![position.clone(), Value::Const(Literal::Integer(0))],
            span,
        );
        let position_y = self.lower_call_temp(
            MirType::Float,
            SmolStr::new("__wr_vec_component"),
            vec![position.clone(), Value::Const(Literal::Integer(1))],
            span,
        );
        let position_z = self.lower_call_temp(
            MirType::Float,
            SmolStr::new("__wr_vec_component"),
            vec![position, Value::Const(Literal::Integer(2))],
            span,
        );
        let position_vec = self.lower_call_temp(
            MirType::Vec3,
            SmolStr::new("vec3"),
            vec![position_x.clone(), position_y.clone(), position_z.clone()],
            span,
        );

        let column_0 = self.lower_call_temp(
            MirType::Vec4,
            SmolStr::new("vec4"),
            vec![
                tangent_x.clone(),
                tangent_y.clone(),
                tangent_z.clone(),
                zero.clone(),
            ],
            span,
        );
        let column_1 = self.lower_call_temp(
            MirType::Vec4,
            SmolStr::new("vec4"),
            vec![
                bitangent_x.clone(),
                bitangent_y.clone(),
                bitangent_z.clone(),
                zero.clone(),
            ],
            span,
        );
        let column_2 = self.lower_call_temp(
            MirType::Vec4,
            SmolStr::new("vec4"),
            vec![
                normal_x.clone(),
                normal_y.clone(),
                normal_z.clone(),
                zero.clone(),
            ],
            span,
        );
        let dot_tangent = self.lower_call_temp(
            MirType::Float,
            SmolStr::new("dot"),
            vec![Value::Local(tangent), position_vec.clone()],
            span,
        );
        let dot_bitangent = self.lower_call_temp(
            MirType::Float,
            SmolStr::new("dot"),
            vec![bitangent, position_vec.clone()],
            span,
        );
        let dot_normal = self.lower_call_temp(
            MirType::Float,
            SmolStr::new("dot"),
            vec![unit_normal.clone(), position_vec],
            span,
        );
        let neg_dot_tangent =
            self.lower_unary_temp(MirType::Float, UnaryOp::Neg, dot_tangent, span);
        let neg_dot_bitangent =
            self.lower_unary_temp(MirType::Float, UnaryOp::Neg, dot_bitangent, span);
        let neg_dot_normal = self.lower_unary_temp(MirType::Float, UnaryOp::Neg, dot_normal, span);
        let column_3 = self.lower_call_temp(
            MirType::Vec4,
            SmolStr::new("vec4"),
            vec![
                neg_dot_tangent.clone(),
                neg_dot_bitangent.clone(),
                neg_dot_normal.clone(),
                one.clone(),
            ],
            span,
        );
        let matrix = self.lower_call_temp(
            MirType::Mat4,
            SmolStr::new("mat4_cols"),
            vec![column_0, column_1, column_2, column_3],
            span,
        );
        let inverse_col_0 = self.lower_call_temp(
            MirType::Vec4,
            SmolStr::new("vec4"),
            vec![tangent_x, bitangent_x, normal_x, zero.clone()],
            span,
        );
        let inverse_col_1 = self.lower_call_temp(
            MirType::Vec4,
            SmolStr::new("vec4"),
            vec![tangent_y, bitangent_y, normal_y, zero.clone()],
            span,
        );
        let inverse_col_2 = self.lower_call_temp(
            MirType::Vec4,
            SmolStr::new("vec4"),
            vec![tangent_z, bitangent_z, normal_z, zero.clone()],
            span,
        );
        let inverse_col_3 = self.lower_call_temp(
            MirType::Vec4,
            SmolStr::new("vec4"),
            vec![neg_dot_tangent, neg_dot_bitangent, neg_dot_normal, one],
            span,
        );
        let inverse = self.lower_call_temp(
            MirType::Mat4,
            SmolStr::new("mat4_cols"),
            vec![inverse_col_0, inverse_col_1, inverse_col_2, inverse_col_3],
            span,
        );

        let mut class = self.synthetic_class_target_info("Transform3");
        Self::set_class_field_value(&mut class, "matrix", matrix);
        Self::set_class_field_value(&mut class, "inverse", inverse);
        self.build_class_instance(&class, span)
    }
}

fn lower_scene_distance_capture_helper(
    module: &hir::Module,
    type_tags: &HashMap<SmolStr, TypeTagId>,
    class_fields: &HashMap<SmolStr, Vec<SmolStr>>,
    class_field_defaults: &HashMap<SmolStr, Vec<Option<hir::FieldDefault>>>,
    function_names: &HashSet<SmolStr>,
    field_names: &HashSet<SmolStr>,
    shape_names: &HashSet<SmolStr>,
    shape_graphs: &HashMap<SmolStr, hir::ShapeGraph>,
    field_graphs: &HashMap<SmolStr, hir::FieldGraph>,
    field_bodies: &HashMap<SmolStr, hir::Body>,
    field_metadata: &HashMap<SmolStr, hir::FieldMetadata>,
    radiance_param_counts: &HashMap<SmolStr, usize>,
    volume_param_counts: &HashMap<SmolStr, usize>,
    result_functions: &HashSet<SmolStr>,
    class_method_ids: &HashMap<SmolStr, HashMap<SmolStr, u32>>,
    interface_methods: &HashMap<SmolStr, HashSet<SmolStr>>,
    capture_type_name: &'static str,
    helper_name: &'static str,
) -> MirFunction {
    let helper_name = SmolStr::new(helper_name);
    let span = TextRange::empty(0.into());
    let mut lowerer = FunctionLowerer::new(
        helper_name.clone(),
        type_tags,
        class_fields,
        class_field_defaults,
        function_names,
        field_names,
        shape_names,
        shape_graphs,
        field_graphs,
        field_bodies,
        field_metadata,
        radiance_param_counts,
        volume_param_counts,
        result_functions,
        class_method_ids,
        interface_methods,
        false,
        None,
    );

    let capture = lowerer.new_local(
        SmolStr::new("capture"),
        false,
        MirType::Named(SmolStr::new(capture_type_name)),
    );
    let point = lowerer.new_local(SmolStr::new("point"), false, MirType::Vec3);
    lowerer.declare_local(SmolStr::new("capture"), capture);
    lowerer.declare_local(SmolStr::new("point"), point);
    lowerer.params.push(capture);
    lowerer.params.push(point);

    let entry = lowerer.new_block();
    lowerer.current_block = entry;
    let scene_id = lowerer.new_temp(MirType::Integer);
    lowerer.push_stmt(MirStmt::Assign {
        place: Place::Temp(scene_id),
        value: Rvalue::GetField {
            base: Value::Local(capture),
            field: SmolStr::new("scene_id"),
            slot: lowerer.field_slot(capture_type_name, "scene_id"),
        },
        span,
    });

    let result = lowerer.new_local(SmolStr::new("$scene_distance"), true, MirType::Float);
    lowerer.assign_use(
        Place::Local(result),
        Value::Const(Literal::Float(0.0)),
        span,
    );
    let return_block = lowerer.new_block();
    let mut dispatch_block = lowerer.new_block();
    lowerer.set_terminator(Terminator::Jump {
        target: dispatch_block,
        span,
    });

    for (_, field) in module
        .functions
        .iter()
        .filter(|(_, func)| matches!(func.role, FunctionRole::Field))
    {
        let match_block = lowerer.new_block();
        let next_block = lowerer.new_block();
        lowerer.current_block = dispatch_block;
        let matched = lowerer.lower_binary_temp(
            MirType::Boolean,
            BinaryOp::Eq,
            Value::Temp(scene_id),
            Value::Const(Literal::Integer(stable_field_scene_capture_id(&field.name))),
            span,
        );
        lowerer.set_terminator(Terminator::Branch {
            cond: matched,
            then_target: match_block,
            else_target: next_block,
            span,
        });
        lowerer.current_block = match_block;
        let distance = lowerer.lower_field_distance_call(&field.name, Value::Local(point), span);
        lowerer.assign_use(Place::Local(result), distance, span);
        lowerer.set_terminator(Terminator::Jump {
            target: return_block,
            span,
        });
        dispatch_block = next_block;
    }

    for (_, shape) in module
        .shapes
        .iter()
        .filter(|(_, shape)| shape.graph.is_some())
    {
        let match_block = lowerer.new_block();
        let next_block = lowerer.new_block();
        lowerer.current_block = dispatch_block;
        let matched = lowerer.lower_binary_temp(
            MirType::Boolean,
            BinaryOp::Eq,
            Value::Temp(scene_id),
            Value::Const(Literal::Integer(stable_shape_scene_capture_id(&shape.name))),
            span,
        );
        lowerer.set_terminator(Terminator::Branch {
            cond: matched,
            then_target: match_block,
            else_target: next_block,
            span,
        });
        lowerer.current_block = match_block;
        let distance = lowerer.lower_shape_distance_call(&shape.name, Value::Local(point), span);
        lowerer.assign_use(Place::Local(result), distance, span);
        lowerer.set_terminator(Terminator::Jump {
            target: return_block,
            span,
        });
        dispatch_block = next_block;
    }

    lowerer.current_block = dispatch_block;
    let invalid_scene_block = lowerer.new_block();
    lowerer.set_terminator(Terminator::Jump {
        target: invalid_scene_block,
        span,
    });
    lowerer.current_block = invalid_scene_block;
    let crash_temp = lowerer.new_temp(MirType::Unknown);
    lowerer.push_stmt(MirStmt::Assign {
        place: Place::Temp(crash_temp),
        value: Rvalue::Crash {
            value: Value::Const(Literal::String(SmolStr::new(
                "distance_at requires a capture created by `capture`",
            ))),
        },
        span,
    });
    lowerer.set_terminator(Terminator::Return {
        value: Some(Value::Temp(crash_temp)),
        span,
    });

    lowerer.current_block = return_block;
    lowerer.set_terminator(Terminator::Return {
        value: Some(Value::Local(result)),
        span,
    });

    MirFunction {
        name: helper_name,
        params: lowerer.params,
        abi_params: vec![
            portable_abi_from_type_ref(
                Some(&hir::TypeRef {
                    name: SmolStr::new(capture_type_name),
                    name_span: None,
                    args: Vec::new(),
                }),
                module,
                type_tags,
                &mut HashSet::new(),
            ),
            PortableAbiType::Vec3,
        ],
        abi_return: PortableAbiType::F32,
        locals: lowerer.locals,
        temps: lowerer.temps,
        blocks: lowerer.blocks,
        entry,
        suspendable: false,
    }
}

fn lower_scene_normal_capture_helper(
    module: &hir::Module,
    type_tags: &HashMap<SmolStr, TypeTagId>,
    class_fields: &HashMap<SmolStr, Vec<SmolStr>>,
    class_field_defaults: &HashMap<SmolStr, Vec<Option<hir::FieldDefault>>>,
    function_names: &HashSet<SmolStr>,
    field_names: &HashSet<SmolStr>,
    shape_names: &HashSet<SmolStr>,
    shape_graphs: &HashMap<SmolStr, hir::ShapeGraph>,
    field_graphs: &HashMap<SmolStr, hir::FieldGraph>,
    field_bodies: &HashMap<SmolStr, hir::Body>,
    field_metadata: &HashMap<SmolStr, hir::FieldMetadata>,
    radiance_param_counts: &HashMap<SmolStr, usize>,
    volume_param_counts: &HashMap<SmolStr, usize>,
    result_functions: &HashSet<SmolStr>,
    class_method_ids: &HashMap<SmolStr, HashMap<SmolStr, u32>>,
    interface_methods: &HashMap<SmolStr, HashSet<SmolStr>>,
    capture_type_name: &'static str,
    helper_name: &'static str,
) -> MirFunction {
    let helper_name = SmolStr::new(helper_name);
    let span = TextRange::empty(0.into());
    let mut lowerer = FunctionLowerer::new(
        helper_name.clone(),
        type_tags,
        class_fields,
        class_field_defaults,
        function_names,
        field_names,
        shape_names,
        shape_graphs,
        field_graphs,
        field_bodies,
        field_metadata,
        radiance_param_counts,
        volume_param_counts,
        result_functions,
        class_method_ids,
        interface_methods,
        false,
        None,
    );

    let capture = lowerer.new_local(
        SmolStr::new("capture"),
        false,
        MirType::Named(SmolStr::new(capture_type_name)),
    );
    let point = lowerer.new_local(SmolStr::new("point"), false, MirType::Vec3);
    lowerer.declare_local(SmolStr::new("capture"), capture);
    lowerer.declare_local(SmolStr::new("point"), point);
    lowerer.params.push(capture);
    lowerer.params.push(point);

    let entry = lowerer.new_block();
    lowerer.current_block = entry;
    let scene_id = lowerer.new_temp(MirType::Integer);
    lowerer.push_stmt(MirStmt::Assign {
        place: Place::Temp(scene_id),
        value: Rvalue::GetField {
            base: Value::Local(capture),
            field: SmolStr::new("scene_id"),
            slot: lowerer.field_slot(capture_type_name, "scene_id"),
        },
        span,
    });
    let result = lowerer.new_local(SmolStr::new("$scene_normal"), true, MirType::Vec3);
    let default_normal = lowerer.lower_call_temp(
        MirType::Vec3,
        SmolStr::new("vec3"),
        vec![
            Value::Const(Literal::Float(0.0)),
            Value::Const(Literal::Float(0.0)),
            Value::Const(Literal::Float(1.0)),
        ],
        span,
    );
    lowerer.assign_use(Place::Local(result), default_normal, span);
    let return_block = lowerer.new_block();
    let mut dispatch_block = lowerer.new_block();
    lowerer.set_terminator(Terminator::Jump {
        target: dispatch_block,
        span,
    });

    for (_, field) in module
        .functions
        .iter()
        .filter(|(_, func)| matches!(func.role, FunctionRole::Field))
    {
        let match_block = lowerer.new_block();
        let next_block = lowerer.new_block();
        lowerer.current_block = dispatch_block;
        let matched = lowerer.lower_binary_temp(
            MirType::Boolean,
            BinaryOp::Eq,
            Value::Temp(scene_id),
            Value::Const(Literal::Integer(stable_field_scene_capture_id(&field.name))),
            span,
        );
        lowerer.set_terminator(Terminator::Branch {
            cond: matched,
            then_target: match_block,
            else_target: next_block,
            span,
        });
        lowerer.current_block = match_block;
        let normal = lowerer.lower_field_normal_call(&field.name, Value::Local(point), span);
        lowerer.assign_use(Place::Local(result), normal, span);
        lowerer.set_terminator(Terminator::Jump {
            target: return_block,
            span,
        });
        dispatch_block = next_block;
    }

    for (_, shape) in module
        .shapes
        .iter()
        .filter(|(_, shape)| shape.graph.is_some())
    {
        let match_block = lowerer.new_block();
        let next_block = lowerer.new_block();
        lowerer.current_block = dispatch_block;
        let matched = lowerer.lower_binary_temp(
            MirType::Boolean,
            BinaryOp::Eq,
            Value::Temp(scene_id),
            Value::Const(Literal::Integer(stable_shape_scene_capture_id(&shape.name))),
            span,
        );
        lowerer.set_terminator(Terminator::Branch {
            cond: matched,
            then_target: match_block,
            else_target: next_block,
            span,
        });
        lowerer.current_block = match_block;
        let normal = lowerer.lower_shape_normal_call(&shape.name, Value::Local(point), span);
        lowerer.assign_use(Place::Local(result), normal, span);
        lowerer.set_terminator(Terminator::Jump {
            target: return_block,
            span,
        });
        dispatch_block = next_block;
    }

    lowerer.current_block = dispatch_block;
    let invalid_scene_block = lowerer.new_block();
    lowerer.set_terminator(Terminator::Jump {
        target: invalid_scene_block,
        span,
    });
    lowerer.current_block = invalid_scene_block;
    let crash_temp = lowerer.new_temp(MirType::Unknown);
    lowerer.push_stmt(MirStmt::Assign {
        place: Place::Temp(crash_temp),
        value: Rvalue::Crash {
            value: Value::Const(Literal::String(SmolStr::new(
                "normal_at requires a capture created by `capture`",
            ))),
        },
        span,
    });
    lowerer.set_terminator(Terminator::Return {
        value: Some(Value::Temp(crash_temp)),
        span,
    });

    lowerer.current_block = return_block;
    lowerer.set_terminator(Terminator::Return {
        value: Some(Value::Local(result)),
        span,
    });

    MirFunction {
        name: helper_name,
        params: lowerer.params,
        abi_params: vec![
            portable_abi_from_type_ref(
                Some(&hir::TypeRef {
                    name: SmolStr::new(capture_type_name),
                    name_span: None,
                    args: Vec::new(),
                }),
                module,
                type_tags,
                &mut HashSet::new(),
            ),
            PortableAbiType::Vec3,
        ],
        abi_return: PortableAbiType::Vec3,
        locals: lowerer.locals,
        temps: lowerer.temps,
        blocks: lowerer.blocks,
        entry,
        suspendable: false,
    }
}

fn lower_scene_trace_capture_helper(
    module: &hir::Module,
    type_tags: &HashMap<SmolStr, TypeTagId>,
    class_fields: &HashMap<SmolStr, Vec<SmolStr>>,
    class_field_defaults: &HashMap<SmolStr, Vec<Option<hir::FieldDefault>>>,
    function_names: &HashSet<SmolStr>,
    field_names: &HashSet<SmolStr>,
    shape_names: &HashSet<SmolStr>,
    shape_graphs: &HashMap<SmolStr, hir::ShapeGraph>,
    field_graphs: &HashMap<SmolStr, hir::FieldGraph>,
    field_bodies: &HashMap<SmolStr, hir::Body>,
    field_metadata: &HashMap<SmolStr, hir::FieldMetadata>,
    radiance_param_counts: &HashMap<SmolStr, usize>,
    volume_param_counts: &HashMap<SmolStr, usize>,
    result_functions: &HashSet<SmolStr>,
    class_method_ids: &HashMap<SmolStr, HashMap<SmolStr, u32>>,
    interface_methods: &HashMap<SmolStr, HashSet<SmolStr>>,
) -> MirFunction {
    let helper_name = SmolStr::new("__wr_scene_trace_capture");
    let span = TextRange::empty(0.into());
    let mut lowerer = FunctionLowerer::new(
        helper_name.clone(),
        type_tags,
        class_fields,
        class_field_defaults,
        function_names,
        field_names,
        shape_names,
        shape_graphs,
        field_graphs,
        field_bodies,
        field_metadata,
        radiance_param_counts,
        volume_param_counts,
        result_functions,
        class_method_ids,
        interface_methods,
        false,
        None,
    );

    let capture = lowerer.new_local(
        SmolStr::new("capture"),
        false,
        MirType::Named(SmolStr::new("ShapeCapture")),
    );
    let origin = lowerer.new_local(SmolStr::new("origin"), false, MirType::Vec3);
    let direction = lowerer.new_local(SmolStr::new("direction"), false, MirType::Vec3);
    let max_distance = lowerer.new_local(SmolStr::new("max_distance"), false, MirType::Float);
    let min_step = lowerer.new_local(SmolStr::new("min_step"), false, MirType::Float);
    let hit_epsilon = lowerer.new_local(SmolStr::new("hit_epsilon"), false, MirType::Float);
    let max_steps = lowerer.new_local(SmolStr::new("max_steps"), false, MirType::Integer);
    for (name, local) in [
        (SmolStr::new("capture"), capture),
        (SmolStr::new("origin"), origin),
        (SmolStr::new("direction"), direction),
        (SmolStr::new("max_distance"), max_distance),
        (SmolStr::new("min_step"), min_step),
        (SmolStr::new("hit_epsilon"), hit_epsilon),
        (SmolStr::new("max_steps"), max_steps),
    ] {
        lowerer.declare_local(name, local);
        lowerer.params.push(local);
    }

    let entry = lowerer.new_block();
    lowerer.current_block = entry;
    let root_feature_id = lowerer.new_temp(MirType::Integer);
    lowerer.push_stmt(MirStmt::Assign {
        place: Place::Temp(root_feature_id),
        value: Rvalue::GetField {
            base: Value::Local(capture),
            field: SmolStr::new("root_feature_id"),
            slot: lowerer.field_slot("ShapeCapture", "root_feature_id"),
        },
        span,
    });
    let invalid_capture_block = lowerer.new_block();
    let shape_capture_block = lowerer.new_block();
    let field_capture = lowerer.lower_binary_temp(
        MirType::Boolean,
        BinaryOp::Eq,
        Value::Temp(root_feature_id),
        Value::Const(Literal::Integer(0)),
        span,
    );
    lowerer.set_terminator(Terminator::Branch {
        cond: field_capture,
        then_target: invalid_capture_block,
        else_target: shape_capture_block,
        span,
    });

    lowerer.current_block = invalid_capture_block;
    let crash_temp = lowerer.new_temp(MirType::Unknown);
    lowerer.push_stmt(MirStmt::Assign {
        place: Place::Temp(crash_temp),
        value: Rvalue::Crash {
            value: Value::Const(Literal::String(SmolStr::new(
                "trace_shape requires a shape capture",
            ))),
        },
        span,
    });
    lowerer.set_terminator(Terminator::Return {
        value: Some(Value::Temp(crash_temp)),
        span,
    });

    lowerer.current_block = shape_capture_block;
    let result = lowerer.new_local(
        SmolStr::new("$scene_trace_result"),
        true,
        MirType::Named(SmolStr::new("Hit3")),
    );
    let default_hit = lowerer.build_default_hit(Value::Local(origin), span);
    lowerer.assign_use(Place::Local(result), default_hit, span);
    let return_block = lowerer.new_block();

    let shapes: Vec<&hir::Shape> = module.shapes.iter().map(|(_, shape)| shape).collect();
    let mut dispatch_block = lowerer.new_block();
    lowerer.set_terminator(Terminator::Jump {
        target: dispatch_block,
        span,
    });
    for shape in shapes.iter().copied().filter(|shape| shape.graph.is_some()) {
        let match_block = lowerer.new_block();
        let next_block = lowerer.new_block();
        lowerer.current_block = dispatch_block;
        let matched = lowerer.lower_binary_temp(
            MirType::Boolean,
            BinaryOp::Eq,
            Value::Temp(root_feature_id),
            Value::Const(Literal::Integer(stable_shape_capture_id(&shape.name))),
            span,
        );
        lowerer.set_terminator(Terminator::Branch {
            cond: matched,
            then_target: match_block,
            else_target: next_block,
            span,
        });
        lowerer.current_block = match_block;
        let hit = lowerer.lower_call_temp(
            MirType::Named(SmolStr::new("Hit3")),
            SmolStr::new(format!("__wr_shape_trace_{}", shape.name)),
            vec![
                Value::Local(origin),
                Value::Local(direction),
                Value::Local(max_distance),
                Value::Local(min_step),
                Value::Local(hit_epsilon),
                Value::Local(max_steps),
            ],
            span,
        );
        lowerer.assign_use(Place::Local(result), hit, span);
        lowerer.set_terminator(Terminator::Jump {
            target: return_block,
            span,
        });
        dispatch_block = next_block;
    }
    lowerer.current_block = dispatch_block;
    let invalid_scene_block = lowerer.new_block();
    lowerer.set_terminator(Terminator::Jump {
        target: invalid_scene_block,
        span,
    });
    lowerer.current_block = invalid_scene_block;
    let crash_temp = lowerer.new_temp(MirType::Unknown);
    lowerer.push_stmt(MirStmt::Assign {
        place: Place::Temp(crash_temp),
        value: Rvalue::Crash {
            value: Value::Const(Literal::String(SmolStr::new(
                "trace_shape requires a capture created by `capture`",
            ))),
        },
        span,
    });
    lowerer.set_terminator(Terminator::Return {
        value: Some(Value::Temp(crash_temp)),
        span,
    });

    lowerer.current_block = return_block;
    lowerer.set_terminator(Terminator::Return {
        value: Some(Value::Local(result)),
        span,
    });

    MirFunction {
        name: helper_name,
        params: lowerer.params,
        abi_params: vec![
            portable_abi_from_type_ref(
                Some(&hir::TypeRef {
                    name: SmolStr::new("ShapeCapture"),
                    name_span: None,
                    args: Vec::new(),
                }),
                module,
                type_tags,
                &mut HashSet::new(),
            ),
            PortableAbiType::Vec3,
            PortableAbiType::Vec3,
            PortableAbiType::F32,
            PortableAbiType::F32,
            PortableAbiType::F32,
            PortableAbiType::I64,
        ],
        abi_return: portable_abi_from_type_ref(
            Some(&hir::TypeRef {
                name: SmolStr::new("Hit3"),
                name_span: None,
                args: Vec::new(),
            }),
            module,
            type_tags,
            &mut HashSet::new(),
        ),
        locals: lowerer.locals,
        temps: lowerer.temps,
        blocks: lowerer.blocks,
        entry,
        suspendable: false,
    }
}

fn lower_scene_surface_capture_helper(
    module: &hir::Module,
    type_tags: &HashMap<SmolStr, TypeTagId>,
    class_fields: &HashMap<SmolStr, Vec<SmolStr>>,
    class_field_defaults: &HashMap<SmolStr, Vec<Option<hir::FieldDefault>>>,
    function_names: &HashSet<SmolStr>,
    field_names: &HashSet<SmolStr>,
    shape_names: &HashSet<SmolStr>,
    shape_graphs: &HashMap<SmolStr, hir::ShapeGraph>,
    field_graphs: &HashMap<SmolStr, hir::FieldGraph>,
    field_bodies: &HashMap<SmolStr, hir::Body>,
    field_metadata: &HashMap<SmolStr, hir::FieldMetadata>,
    radiance_param_counts: &HashMap<SmolStr, usize>,
    volume_param_counts: &HashMap<SmolStr, usize>,
    result_functions: &HashSet<SmolStr>,
    class_method_ids: &HashMap<SmolStr, HashMap<SmolStr, u32>>,
    interface_methods: &HashMap<SmolStr, HashSet<SmolStr>>,
) -> MirFunction {
    let helper_name = SmolStr::new("__wr_scene_surface_capture");
    let span = TextRange::empty(0.into());
    let mut lowerer = FunctionLowerer::new(
        helper_name.clone(),
        type_tags,
        class_fields,
        class_field_defaults,
        function_names,
        field_names,
        shape_names,
        shape_graphs,
        field_graphs,
        field_bodies,
        field_metadata,
        radiance_param_counts,
        volume_param_counts,
        result_functions,
        class_method_ids,
        interface_methods,
        false,
        None,
    );

    let capture = lowerer.new_local(
        SmolStr::new("capture"),
        false,
        MirType::Named(SmolStr::new("ShapeCapture")),
    );
    let hit = lowerer.new_local(
        SmolStr::new("hit"),
        false,
        MirType::Named(SmolStr::new("Hit3")),
    );
    lowerer.declare_local(SmolStr::new("capture"), capture);
    lowerer.declare_local(SmolStr::new("hit"), hit);
    lowerer.params.push(capture);
    lowerer.params.push(hit);

    let entry = lowerer.new_block();
    lowerer.current_block = entry;
    let root_feature_id = lowerer.new_temp(MirType::Integer);
    lowerer.push_stmt(MirStmt::Assign {
        place: Place::Temp(root_feature_id),
        value: Rvalue::GetField {
            base: Value::Local(capture),
            field: SmolStr::new("root_feature_id"),
            slot: lowerer.field_slot("ShapeCapture", "root_feature_id"),
        },
        span,
    });
    let invalid_capture_block = lowerer.new_block();
    let shape_capture_block = lowerer.new_block();
    let field_capture = lowerer.lower_binary_temp(
        MirType::Boolean,
        BinaryOp::Eq,
        Value::Temp(root_feature_id),
        Value::Const(Literal::Integer(0)),
        span,
    );
    lowerer.set_terminator(Terminator::Branch {
        cond: field_capture,
        then_target: invalid_capture_block,
        else_target: shape_capture_block,
        span,
    });

    lowerer.current_block = invalid_capture_block;
    let crash_temp = lowerer.new_temp(MirType::Unknown);
    lowerer.push_stmt(MirStmt::Assign {
        place: Place::Temp(crash_temp),
        value: Rvalue::Crash {
            value: Value::Const(Literal::String(SmolStr::new(
                "surface_at requires a shape capture",
            ))),
        },
        span,
    });
    lowerer.set_terminator(Terminator::Return {
        value: Some(Value::Temp(crash_temp)),
        span,
    });

    lowerer.current_block = shape_capture_block;
    let result = lowerer.new_local(
        SmolStr::new("$scene_surface_result"),
        true,
        MirType::Named(SmolStr::new("Surface")),
    );
    let default_surface = lowerer.build_default_surface(span);
    lowerer.assign_use(Place::Local(result), default_surface, span);
    let return_block = lowerer.new_block();

    let shapes: Vec<&hir::Shape> = module.shapes.iter().map(|(_, shape)| shape).collect();
    let mut dispatch_block = lowerer.new_block();
    lowerer.set_terminator(Terminator::Jump {
        target: dispatch_block,
        span,
    });
    for shape in shapes.iter().copied().filter(|shape| shape.graph.is_some()) {
        let match_block = lowerer.new_block();
        let next_block = lowerer.new_block();
        lowerer.current_block = dispatch_block;
        let matched = lowerer.lower_binary_temp(
            MirType::Boolean,
            BinaryOp::Eq,
            Value::Temp(root_feature_id),
            Value::Const(Literal::Integer(stable_shape_capture_id(&shape.name))),
            span,
        );
        lowerer.set_terminator(Terminator::Branch {
            cond: matched,
            then_target: match_block,
            else_target: next_block,
            span,
        });
        lowerer.current_block = match_block;
        let surface = lowerer.lower_call_temp(
            MirType::Named(SmolStr::new("Surface")),
            SmolStr::new(format!("__wr_shape_surface_{}", shape.name)),
            vec![Value::Local(hit)],
            span,
        );
        lowerer.assign_use(Place::Local(result), surface, span);
        lowerer.set_terminator(Terminator::Jump {
            target: return_block,
            span,
        });
        dispatch_block = next_block;
    }
    lowerer.current_block = dispatch_block;
    let invalid_scene_block = lowerer.new_block();
    lowerer.set_terminator(Terminator::Jump {
        target: invalid_scene_block,
        span,
    });
    lowerer.current_block = invalid_scene_block;
    let crash_temp = lowerer.new_temp(MirType::Unknown);
    lowerer.push_stmt(MirStmt::Assign {
        place: Place::Temp(crash_temp),
        value: Rvalue::Crash {
            value: Value::Const(Literal::String(SmolStr::new(
                "surface_at requires a capture created by `capture`",
            ))),
        },
        span,
    });
    lowerer.set_terminator(Terminator::Return {
        value: Some(Value::Temp(crash_temp)),
        span,
    });

    lowerer.current_block = return_block;
    lowerer.set_terminator(Terminator::Return {
        value: Some(Value::Local(result)),
        span,
    });

    MirFunction {
        name: helper_name,
        params: lowerer.params,
        abi_params: vec![
            portable_abi_from_type_ref(
                Some(&hir::TypeRef {
                    name: SmolStr::new("ShapeCapture"),
                    name_span: None,
                    args: Vec::new(),
                }),
                module,
                type_tags,
                &mut HashSet::new(),
            ),
            portable_abi_from_type_ref(
                Some(&hir::TypeRef {
                    name: SmolStr::new("Hit3"),
                    name_span: None,
                    args: Vec::new(),
                }),
                module,
                type_tags,
                &mut HashSet::new(),
            ),
        ],
        abi_return: portable_abi_from_type_ref(
            Some(&hir::TypeRef {
                name: SmolStr::new("Surface"),
                name_span: None,
                args: Vec::new(),
            }),
            module,
            type_tags,
            &mut HashSet::new(),
        ),
        locals: lowerer.locals,
        temps: lowerer.temps,
        blocks: lowerer.blocks,
        entry,
        suspendable: false,
    }
}

fn lower_scene_radiance_capture_helper(
    module: &hir::Module,
    type_tags: &HashMap<SmolStr, TypeTagId>,
    class_fields: &HashMap<SmolStr, Vec<SmolStr>>,
    class_field_defaults: &HashMap<SmolStr, Vec<Option<hir::FieldDefault>>>,
    function_names: &HashSet<SmolStr>,
    field_names: &HashSet<SmolStr>,
    shape_names: &HashSet<SmolStr>,
    shape_graphs: &HashMap<SmolStr, hir::ShapeGraph>,
    field_graphs: &HashMap<SmolStr, hir::FieldGraph>,
    field_bodies: &HashMap<SmolStr, hir::Body>,
    field_metadata: &HashMap<SmolStr, hir::FieldMetadata>,
    radiance_param_counts: &HashMap<SmolStr, usize>,
    volume_param_counts: &HashMap<SmolStr, usize>,
    result_functions: &HashSet<SmolStr>,
    class_method_ids: &HashMap<SmolStr, HashMap<SmolStr, u32>>,
    interface_methods: &HashMap<SmolStr, HashSet<SmolStr>>,
) -> MirFunction {
    let helper_name = SmolStr::new("__wr_scene_radiance_capture");
    let span = TextRange::empty(0.into());
    let mut lowerer = FunctionLowerer::new(
        helper_name.clone(),
        type_tags,
        class_fields,
        class_field_defaults,
        function_names,
        field_names,
        shape_names,
        shape_graphs,
        field_graphs,
        field_bodies,
        field_metadata,
        radiance_param_counts,
        volume_param_counts,
        result_functions,
        class_method_ids,
        interface_methods,
        false,
        None,
    );

    let capture = lowerer.new_local(
        SmolStr::new("capture"),
        false,
        MirType::Named(SmolStr::new("ShapeCapture")),
    );
    let point = lowerer.new_local(SmolStr::new("point"), false, MirType::Vec3);
    let direction = lowerer.new_local(SmolStr::new("direction"), false, MirType::Vec3);
    for (name, local) in [
        (SmolStr::new("capture"), capture),
        (SmolStr::new("point"), point),
        (SmolStr::new("direction"), direction),
    ] {
        lowerer.declare_local(name, local);
        lowerer.params.push(local);
    }

    let entry = lowerer.new_block();
    lowerer.current_block = entry;
    let root_feature_id = lowerer.new_temp(MirType::Integer);
    lowerer.push_stmt(MirStmt::Assign {
        place: Place::Temp(root_feature_id),
        value: Rvalue::GetField {
            base: Value::Local(capture),
            field: SmolStr::new("root_feature_id"),
            slot: lowerer.field_slot("ShapeCapture", "root_feature_id"),
        },
        span,
    });
    let invalid_capture_block = lowerer.new_block();
    let shape_capture_block = lowerer.new_block();
    let field_capture = lowerer.lower_binary_temp(
        MirType::Boolean,
        BinaryOp::Eq,
        Value::Temp(root_feature_id),
        Value::Const(Literal::Integer(0)),
        span,
    );
    lowerer.set_terminator(Terminator::Branch {
        cond: field_capture,
        then_target: invalid_capture_block,
        else_target: shape_capture_block,
        span,
    });

    lowerer.current_block = invalid_capture_block;
    let crash_temp = lowerer.new_temp(MirType::Unknown);
    lowerer.push_stmt(MirStmt::Assign {
        place: Place::Temp(crash_temp),
        value: Rvalue::Crash {
            value: Value::Const(Literal::String(SmolStr::new(
                "radiance_at requires a shape capture",
            ))),
        },
        span,
    });
    lowerer.set_terminator(Terminator::Return {
        value: Some(Value::Temp(crash_temp)),
        span,
    });

    lowerer.current_block = shape_capture_block;
    let result = lowerer.new_local(SmolStr::new("$scene_radiance_result"), true, MirType::Vec3);
    let default_radiance = lowerer.lower_call_temp(
        MirType::Vec3,
        SmolStr::new("vec3"),
        vec![
            Value::Const(Literal::Float(0.0)),
            Value::Const(Literal::Float(0.0)),
            Value::Const(Literal::Float(0.0)),
        ],
        span,
    );
    lowerer.assign_use(Place::Local(result), default_radiance, span);
    let return_block = lowerer.new_block();

    let shapes: Vec<&hir::Shape> = module.shapes.iter().map(|(_, shape)| shape).collect();
    let mut dispatch_block = lowerer.new_block();
    lowerer.set_terminator(Terminator::Jump {
        target: dispatch_block,
        span,
    });
    for shape in shapes.iter().copied().filter(|shape| shape.graph.is_some()) {
        let match_block = lowerer.new_block();
        let next_block = lowerer.new_block();
        lowerer.current_block = dispatch_block;
        let matched = lowerer.lower_binary_temp(
            MirType::Boolean,
            BinaryOp::Eq,
            Value::Temp(root_feature_id),
            Value::Const(Literal::Integer(stable_shape_capture_id(&shape.name))),
            span,
        );
        lowerer.set_terminator(Terminator::Branch {
            cond: matched,
            then_target: match_block,
            else_target: next_block,
            span,
        });
        lowerer.current_block = match_block;
        let shape_graph = shape.graph.as_ref().expect("shape graph");
        let (_, _, feature_id) = lowerer.lower_shape_payload_selection(
            &shape_graph.root,
            shape_graph.provenance.as_ref(),
            Value::Local(point),
            Value::Const(Literal::Float(0.001)),
            &mut vec![shape.name.clone()],
            span,
        );
        let (_, radiance) = lowerer.lower_shape_radiance_selection(
            &shape_graph.root,
            feature_id,
            Value::Local(point),
            Value::Local(direction),
            &mut vec![shape.name.clone()],
            span,
        );
        lowerer.assign_use(Place::Local(result), radiance, span);
        lowerer.set_terminator(Terminator::Jump {
            target: return_block,
            span,
        });
        dispatch_block = next_block;
    }

    lowerer.current_block = dispatch_block;
    let invalid_scene_block = lowerer.new_block();
    lowerer.set_terminator(Terminator::Jump {
        target: invalid_scene_block,
        span,
    });
    lowerer.current_block = invalid_scene_block;
    let crash_temp = lowerer.new_temp(MirType::Unknown);
    lowerer.push_stmt(MirStmt::Assign {
        place: Place::Temp(crash_temp),
        value: Rvalue::Crash {
            value: Value::Const(Literal::String(SmolStr::new(
                "radiance_at requires a capture created by `capture`",
            ))),
        },
        span,
    });
    lowerer.set_terminator(Terminator::Return {
        value: Some(Value::Temp(crash_temp)),
        span,
    });

    lowerer.current_block = return_block;
    lowerer.set_terminator(Terminator::Return {
        value: Some(Value::Local(result)),
        span,
    });

    MirFunction {
        name: helper_name,
        params: lowerer.params,
        abi_params: vec![
            portable_abi_from_type_ref(
                Some(&hir::TypeRef {
                    name: SmolStr::new("ShapeCapture"),
                    name_span: None,
                    args: Vec::new(),
                }),
                module,
                type_tags,
                &mut HashSet::new(),
            ),
            PortableAbiType::Vec3,
            PortableAbiType::Vec3,
        ],
        abi_return: PortableAbiType::Vec3,
        locals: lowerer.locals,
        temps: lowerer.temps,
        blocks: lowerer.blocks,
        entry,
        suspendable: false,
    }
}

fn lower_scene_medium_capture_helper(
    module: &hir::Module,
    type_tags: &HashMap<SmolStr, TypeTagId>,
    class_fields: &HashMap<SmolStr, Vec<SmolStr>>,
    class_field_defaults: &HashMap<SmolStr, Vec<Option<hir::FieldDefault>>>,
    function_names: &HashSet<SmolStr>,
    field_names: &HashSet<SmolStr>,
    shape_names: &HashSet<SmolStr>,
    shape_graphs: &HashMap<SmolStr, hir::ShapeGraph>,
    field_graphs: &HashMap<SmolStr, hir::FieldGraph>,
    field_bodies: &HashMap<SmolStr, hir::Body>,
    field_metadata: &HashMap<SmolStr, hir::FieldMetadata>,
    radiance_param_counts: &HashMap<SmolStr, usize>,
    volume_param_counts: &HashMap<SmolStr, usize>,
    result_functions: &HashSet<SmolStr>,
    class_method_ids: &HashMap<SmolStr, HashMap<SmolStr, u32>>,
    interface_methods: &HashMap<SmolStr, HashSet<SmolStr>>,
) -> MirFunction {
    let helper_name = SmolStr::new("__wr_scene_medium_capture");
    let span = TextRange::empty(0.into());
    let mut lowerer = FunctionLowerer::new(
        helper_name.clone(),
        type_tags,
        class_fields,
        class_field_defaults,
        function_names,
        field_names,
        shape_names,
        shape_graphs,
        field_graphs,
        field_bodies,
        field_metadata,
        radiance_param_counts,
        volume_param_counts,
        result_functions,
        class_method_ids,
        interface_methods,
        false,
        None,
    );

    let capture = lowerer.new_local(
        SmolStr::new("capture"),
        false,
        MirType::Named(SmolStr::new("ShapeCapture")),
    );
    let point = lowerer.new_local(SmolStr::new("point"), false, MirType::Vec3);
    lowerer.declare_local(SmolStr::new("capture"), capture);
    lowerer.declare_local(SmolStr::new("point"), point);
    lowerer.params.push(capture);
    lowerer.params.push(point);

    let entry = lowerer.new_block();
    lowerer.current_block = entry;
    let root_feature_id = lowerer.new_temp(MirType::Integer);
    lowerer.push_stmt(MirStmt::Assign {
        place: Place::Temp(root_feature_id),
        value: Rvalue::GetField {
            base: Value::Local(capture),
            field: SmolStr::new("root_feature_id"),
            slot: lowerer.field_slot("ShapeCapture", "root_feature_id"),
        },
        span,
    });
    let invalid_capture_block = lowerer.new_block();
    let shape_capture_block = lowerer.new_block();
    let field_capture = lowerer.lower_binary_temp(
        MirType::Boolean,
        BinaryOp::Eq,
        Value::Temp(root_feature_id),
        Value::Const(Literal::Integer(0)),
        span,
    );
    lowerer.set_terminator(Terminator::Branch {
        cond: field_capture,
        then_target: invalid_capture_block,
        else_target: shape_capture_block,
        span,
    });

    lowerer.current_block = invalid_capture_block;
    let crash_temp = lowerer.new_temp(MirType::Unknown);
    lowerer.push_stmt(MirStmt::Assign {
        place: Place::Temp(crash_temp),
        value: Rvalue::Crash {
            value: Value::Const(Literal::String(SmolStr::new(
                "medium_at requires a shape capture",
            ))),
        },
        span,
    });
    lowerer.set_terminator(Terminator::Return {
        value: Some(Value::Temp(crash_temp)),
        span,
    });

    lowerer.current_block = shape_capture_block;
    let result = lowerer.new_local(
        SmolStr::new("$scene_medium_result"),
        true,
        MirType::Named(SmolStr::new("Medium")),
    );
    let default_medium = lowerer.build_default_medium(span);
    lowerer.assign_use(Place::Local(result), default_medium, span);
    let return_block = lowerer.new_block();

    let shapes: Vec<&hir::Shape> = module.shapes.iter().map(|(_, shape)| shape).collect();
    let mut dispatch_block = lowerer.new_block();
    lowerer.set_terminator(Terminator::Jump {
        target: dispatch_block,
        span,
    });
    for shape in shapes.iter().copied().filter(|shape| shape.graph.is_some()) {
        let match_block = lowerer.new_block();
        let next_block = lowerer.new_block();
        lowerer.current_block = dispatch_block;
        let matched = lowerer.lower_binary_temp(
            MirType::Boolean,
            BinaryOp::Eq,
            Value::Temp(root_feature_id),
            Value::Const(Literal::Integer(stable_shape_capture_id(&shape.name))),
            span,
        );
        lowerer.set_terminator(Terminator::Branch {
            cond: matched,
            then_target: match_block,
            else_target: next_block,
            span,
        });
        lowerer.current_block = match_block;
        let shape_graph = shape.graph.as_ref().expect("shape graph");
        let surface_distance =
            lowerer.lower_shape_distance_call(&shape.name, Value::Local(point), span);
        let (_, _, feature_id) = lowerer.lower_shape_payload_selection(
            &shape_graph.root,
            shape_graph.provenance.as_ref(),
            Value::Local(point),
            Value::Const(Literal::Float(0.001)),
            &mut vec![shape.name.clone()],
            span,
        );
        let (_, medium) = lowerer.lower_shape_medium_selection(
            &shape_graph.root,
            feature_id,
            Value::Local(point),
            surface_distance,
            &mut vec![shape.name.clone()],
            span,
        );
        lowerer.assign_use(Place::Local(result), medium, span);
        lowerer.set_terminator(Terminator::Jump {
            target: return_block,
            span,
        });
        dispatch_block = next_block;
    }

    lowerer.current_block = dispatch_block;
    let invalid_scene_block = lowerer.new_block();
    lowerer.set_terminator(Terminator::Jump {
        target: invalid_scene_block,
        span,
    });
    lowerer.current_block = invalid_scene_block;
    let crash_temp = lowerer.new_temp(MirType::Unknown);
    lowerer.push_stmt(MirStmt::Assign {
        place: Place::Temp(crash_temp),
        value: Rvalue::Crash {
            value: Value::Const(Literal::String(SmolStr::new(
                "medium_at requires a capture created by `capture`",
            ))),
        },
        span,
    });
    lowerer.set_terminator(Terminator::Return {
        value: Some(Value::Temp(crash_temp)),
        span,
    });

    lowerer.current_block = return_block;
    lowerer.set_terminator(Terminator::Return {
        value: Some(Value::Local(result)),
        span,
    });

    MirFunction {
        name: helper_name,
        params: lowerer.params,
        abi_params: vec![
            portable_abi_from_type_ref(
                Some(&hir::TypeRef {
                    name: SmolStr::new("ShapeCapture"),
                    name_span: None,
                    args: Vec::new(),
                }),
                module,
                type_tags,
                &mut HashSet::new(),
            ),
            PortableAbiType::Vec3,
        ],
        abi_return: portable_abi_from_type_ref(
            Some(&hir::TypeRef {
                name: SmolStr::new("Medium"),
                name_span: None,
                args: Vec::new(),
            }),
            module,
            type_tags,
            &mut HashSet::new(),
        ),
        locals: lowerer.locals,
        temps: lowerer.temps,
        blocks: lowerer.blocks,
        entry,
        suspendable: false,
    }
}

fn lower_world_distance_capture_helper(
    module: &hir::Module,
    type_tags: &HashMap<SmolStr, TypeTagId>,
    class_fields: &HashMap<SmolStr, Vec<SmolStr>>,
    class_field_defaults: &HashMap<SmolStr, Vec<Option<hir::FieldDefault>>>,
    function_names: &HashSet<SmolStr>,
    field_names: &HashSet<SmolStr>,
    shape_names: &HashSet<SmolStr>,
    shape_graphs: &HashMap<SmolStr, hir::ShapeGraph>,
    field_graphs: &HashMap<SmolStr, hir::FieldGraph>,
    field_bodies: &HashMap<SmolStr, hir::Body>,
    field_metadata: &HashMap<SmolStr, hir::FieldMetadata>,
    radiance_param_counts: &HashMap<SmolStr, usize>,
    volume_param_counts: &HashMap<SmolStr, usize>,
    result_functions: &HashSet<SmolStr>,
    class_method_ids: &HashMap<SmolStr, HashMap<SmolStr, u32>>,
    interface_methods: &HashMap<SmolStr, HashSet<SmolStr>>,
) -> MirFunction {
    let helper_name = SmolStr::new("__wr_world_distance_capture");
    let span = TextRange::empty(0.into());
    let mut lowerer = FunctionLowerer::new(
        helper_name.clone(),
        type_tags,
        class_fields,
        class_field_defaults,
        function_names,
        field_names,
        shape_names,
        shape_graphs,
        field_graphs,
        field_bodies,
        field_metadata,
        radiance_param_counts,
        volume_param_counts,
        result_functions,
        class_method_ids,
        interface_methods,
        false,
        None,
    );

    let capture = lowerer.new_local(
        SmolStr::new("capture"),
        false,
        MirType::Named(SmolStr::new("RegionCapture")),
    );
    let domain = lowerer.new_local(
        SmolStr::new("domain"),
        false,
        MirType::Named(SmolStr::new("SceneDomain")),
    );
    let point = lowerer.new_local(SmolStr::new("point"), false, MirType::Vec3);
    for (name, local) in [
        (SmolStr::new("capture"), capture),
        (SmolStr::new("domain"), domain),
        (SmolStr::new("point"), point),
    ] {
        lowerer.declare_local(name, local);
        lowerer.params.push(local);
    }

    let entry = lowerer.new_block();
    lowerer.current_block = entry;
    let capture_scene_id = lowerer.lower_get_named_field(
        Value::Local(capture),
        "RegionCapture",
        "scene_id",
        MirType::Integer,
        span,
    );
    let domain_scene_id = lowerer.lower_get_named_field(
        Value::Local(domain),
        "SceneDomain",
        "scene_id",
        MirType::Integer,
        span,
    );
    let scene_ids_match = lowerer.lower_binary_temp(
        MirType::Boolean,
        BinaryOp::Eq,
        capture_scene_id.clone(),
        domain_scene_id,
        span,
    );
    let matched_block = lowerer.new_block();
    let mismatch_block = lowerer.new_block();
    lowerer.set_terminator(Terminator::Branch {
        cond: scene_ids_match,
        then_target: matched_block,
        else_target: mismatch_block,
        span,
    });

    lowerer.current_block = mismatch_block;
    let crash_temp = lowerer.new_temp(MirType::Unknown);
    lowerer.push_stmt(MirStmt::Assign {
        place: Place::Temp(crash_temp),
        value: Rvalue::Crash {
            value: Value::Const(Literal::String(SmolStr::new(
                "world queries require a domain derived from the same region capture",
            ))),
        },
        span,
    });
    lowerer.set_terminator(Terminator::Return {
        value: Some(Value::Temp(crash_temp)),
        span,
    });

    lowerer.current_block = matched_block;
    let detail = lowerer.lower_get_named_field(
        Value::Local(domain),
        "SceneDomain",
        "geometry_detail",
        MirType::Integer,
        span,
    );
    let result = lowerer.new_local(SmolStr::new("$world_distance_result"), true, MirType::Float);
    lowerer.assign_use(
        Place::Local(result),
        Value::Const(Literal::Float(1_000_000.0)),
        span,
    );
    let return_block = lowerer.new_block();
    let mut dispatch_block = lowerer.new_block();
    lowerer.set_terminator(Terminator::Jump {
        target: dispatch_block,
        span,
    });

    for (_, region) in module
        .functions
        .iter()
        .filter(|(_, func)| matches!(func.role, FunctionRole::Region))
    {
        let match_block = lowerer.new_block();
        let next_block = lowerer.new_block();
        lowerer.current_block = dispatch_block;
        let matched = lowerer.lower_binary_temp(
            MirType::Boolean,
            BinaryOp::Eq,
            capture_scene_id.clone(),
            Value::Const(Literal::Integer(stable_region_scene_capture_id(&region.name))),
            span,
        );
        lowerer.set_terminator(Terminator::Branch {
            cond: matched,
            then_target: match_block,
            else_target: next_block,
            span,
        });
        lowerer.current_block = match_block;
        let Ok((coarse_shapes, fine_shapes)) = executable_region_shape_lists(region) else {
            let crash_temp = lowerer.new_temp(MirType::Unknown);
            lowerer.push_stmt(MirStmt::Assign {
                place: Place::Temp(crash_temp),
                value: Rvalue::Crash {
                    value: Value::Const(Literal::String(SmolStr::new(
                        "world queries only support direct region compose items today",
                    ))),
                },
                span,
            });
            lowerer.set_terminator(Terminator::Return {
                value: Some(Value::Temp(crash_temp)),
                span,
            });
            dispatch_block = next_block;
            continue;
        };
        let coarse_block = lowerer.new_block();
        let fine_block = lowerer.new_block();
        let detail_is_coarse = lowerer.lower_binary_temp(
            MirType::Boolean,
            BinaryOp::Eq,
            detail.clone(),
            Value::Const(Literal::Integer(0)),
            span,
        );
        lowerer.set_terminator(Terminator::Branch {
            cond: detail_is_coarse,
            then_target: coarse_block,
            else_target: fine_block,
            span,
        });
        for (shapes, block) in [(&coarse_shapes, coarse_block), (&fine_shapes, fine_block)] {
            lowerer.current_block = block;
            for shape in shapes {
                let distance =
                    lowerer.lower_shape_distance_call(shape, Value::Local(point), span);
                let next = lowerer.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("min"),
                    vec![Value::Local(result), distance],
                    span,
                );
                lowerer.assign_use(Place::Local(result), next, span);
            }
            lowerer.set_terminator(Terminator::Jump {
                target: return_block,
                span,
            });
        }
        dispatch_block = next_block;
    }

    lowerer.current_block = dispatch_block;
    let invalid_scene_block = lowerer.new_block();
    lowerer.set_terminator(Terminator::Jump {
        target: invalid_scene_block,
        span,
    });
    lowerer.current_block = invalid_scene_block;
    let crash_temp = lowerer.new_temp(MirType::Unknown);
    lowerer.push_stmt(MirStmt::Assign {
        place: Place::Temp(crash_temp),
        value: Rvalue::Crash {
            value: Value::Const(Literal::String(SmolStr::new(
                "distance_world requires a capture created from a region declaration",
            ))),
        },
        span,
    });
    lowerer.set_terminator(Terminator::Return {
        value: Some(Value::Temp(crash_temp)),
        span,
    });

    lowerer.current_block = return_block;
    lowerer.set_terminator(Terminator::Return {
        value: Some(Value::Local(result)),
        span,
    });

    MirFunction {
        name: helper_name,
        params: lowerer.params,
        abi_params: vec![
            portable_abi_from_type_ref(
                Some(&hir::TypeRef {
                    name: SmolStr::new("RegionCapture"),
                    name_span: None,
                    args: Vec::new(),
                }),
                module,
                type_tags,
                &mut HashSet::new(),
            ),
            portable_abi_from_type_ref(
                Some(&hir::TypeRef {
                    name: SmolStr::new("SceneDomain"),
                    name_span: None,
                    args: Vec::new(),
                }),
                module,
                type_tags,
                &mut HashSet::new(),
            ),
            PortableAbiType::Vec3,
        ],
        abi_return: PortableAbiType::F32,
        locals: lowerer.locals,
        temps: lowerer.temps,
        blocks: lowerer.blocks,
        entry,
        suspendable: false,
    }
}

fn lower_world_normal_capture_helper(
    module: &hir::Module,
    type_tags: &HashMap<SmolStr, TypeTagId>,
    class_fields: &HashMap<SmolStr, Vec<SmolStr>>,
    class_field_defaults: &HashMap<SmolStr, Vec<Option<hir::FieldDefault>>>,
    function_names: &HashSet<SmolStr>,
    field_names: &HashSet<SmolStr>,
    shape_names: &HashSet<SmolStr>,
    shape_graphs: &HashMap<SmolStr, hir::ShapeGraph>,
    field_graphs: &HashMap<SmolStr, hir::FieldGraph>,
    field_bodies: &HashMap<SmolStr, hir::Body>,
    field_metadata: &HashMap<SmolStr, hir::FieldMetadata>,
    radiance_param_counts: &HashMap<SmolStr, usize>,
    volume_param_counts: &HashMap<SmolStr, usize>,
    result_functions: &HashSet<SmolStr>,
    class_method_ids: &HashMap<SmolStr, HashMap<SmolStr, u32>>,
    interface_methods: &HashMap<SmolStr, HashSet<SmolStr>>,
) -> MirFunction {
    let helper_name = SmolStr::new("__wr_world_normal_capture");
    let span = TextRange::empty(0.into());
    let mut lowerer = FunctionLowerer::new(
        helper_name.clone(),
        type_tags,
        class_fields,
        class_field_defaults,
        function_names,
        field_names,
        shape_names,
        shape_graphs,
        field_graphs,
        field_bodies,
        field_metadata,
        radiance_param_counts,
        volume_param_counts,
        result_functions,
        class_method_ids,
        interface_methods,
        false,
        None,
    );

    let capture = lowerer.new_local(
        SmolStr::new("capture"),
        false,
        MirType::Named(SmolStr::new("RegionCapture")),
    );
    let domain = lowerer.new_local(
        SmolStr::new("domain"),
        false,
        MirType::Named(SmolStr::new("SceneDomain")),
    );
    let point = lowerer.new_local(SmolStr::new("point"), false, MirType::Vec3);
    for (name, local) in [
        (SmolStr::new("capture"), capture),
        (SmolStr::new("domain"), domain),
        (SmolStr::new("point"), point),
    ] {
        lowerer.declare_local(name, local);
        lowerer.params.push(local);
    }

    let entry = lowerer.new_block();
    lowerer.current_block = entry;
    let epsilon = Value::Const(Literal::Float(0.001));
    let offset_x = lowerer.lower_call_temp(
        MirType::Vec3,
        SmolStr::new("vec3"),
        vec![epsilon.clone(), Value::Const(Literal::Float(0.0)), Value::Const(Literal::Float(0.0))],
        span,
    );
    let offset_y = lowerer.lower_call_temp(
        MirType::Vec3,
        SmolStr::new("vec3"),
        vec![Value::Const(Literal::Float(0.0)), epsilon.clone(), Value::Const(Literal::Float(0.0))],
        span,
    );
    let offset_z = lowerer.lower_call_temp(
        MirType::Vec3,
        SmolStr::new("vec3"),
        vec![Value::Const(Literal::Float(0.0)), Value::Const(Literal::Float(0.0)), epsilon],
        span,
    );
    let px = lowerer.lower_binary_temp(MirType::Vec3, BinaryOp::Add, Value::Local(point), offset_x.clone(), span);
    let nx = lowerer.lower_binary_temp(MirType::Vec3, BinaryOp::Sub, Value::Local(point), offset_x, span);
    let py = lowerer.lower_binary_temp(MirType::Vec3, BinaryOp::Add, Value::Local(point), offset_y.clone(), span);
    let ny = lowerer.lower_binary_temp(MirType::Vec3, BinaryOp::Sub, Value::Local(point), offset_y, span);
    let pz = lowerer.lower_binary_temp(MirType::Vec3, BinaryOp::Add, Value::Local(point), offset_z.clone(), span);
    let nz = lowerer.lower_binary_temp(MirType::Vec3, BinaryOp::Sub, Value::Local(point), offset_z, span);
    let dxp = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("__wr_world_distance_capture"),
        vec![Value::Local(capture), Value::Local(domain), px],
        span,
    );
    let dxn = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("__wr_world_distance_capture"),
        vec![Value::Local(capture), Value::Local(domain), nx],
        span,
    );
    let dyp = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("__wr_world_distance_capture"),
        vec![Value::Local(capture), Value::Local(domain), py],
        span,
    );
    let dyn_ = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("__wr_world_distance_capture"),
        vec![Value::Local(capture), Value::Local(domain), ny],
        span,
    );
    let dzp = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("__wr_world_distance_capture"),
        vec![Value::Local(capture), Value::Local(domain), pz],
        span,
    );
    let dzn = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("__wr_world_distance_capture"),
        vec![Value::Local(capture), Value::Local(domain), nz],
        span,
    );
    let nx_comp = lowerer.lower_binary_temp(MirType::Float, BinaryOp::Sub, dxp, dxn, span);
    let ny_comp = lowerer.lower_binary_temp(MirType::Float, BinaryOp::Sub, dyp, dyn_, span);
    let nz_comp = lowerer.lower_binary_temp(MirType::Float, BinaryOp::Sub, dzp, dzn, span);
    let gradient = lowerer.lower_call_temp(
        MirType::Vec3,
        SmolStr::new("vec3"),
        vec![nx_comp, ny_comp, nz_comp],
        span,
    );
    let normal = lowerer.lower_call_temp(
        MirType::Vec3,
        SmolStr::new("normalize"),
        vec![gradient],
        span,
    );
    lowerer.set_terminator(Terminator::Return {
        value: Some(normal),
        span,
    });

    MirFunction {
        name: helper_name,
        params: lowerer.params,
        abi_params: vec![
            portable_abi_from_type_ref(
                Some(&hir::TypeRef {
                    name: SmolStr::new("RegionCapture"),
                    name_span: None,
                    args: Vec::new(),
                }),
                module,
                type_tags,
                &mut HashSet::new(),
            ),
            portable_abi_from_type_ref(
                Some(&hir::TypeRef {
                    name: SmolStr::new("SceneDomain"),
                    name_span: None,
                    args: Vec::new(),
                }),
                module,
                type_tags,
                &mut HashSet::new(),
            ),
            PortableAbiType::Vec3,
        ],
        abi_return: PortableAbiType::Vec3,
        locals: lowerer.locals,
        temps: lowerer.temps,
        blocks: lowerer.blocks,
        entry,
        suspendable: false,
    }
}

fn lower_world_trace_capture_helper(
    module: &hir::Module,
    type_tags: &HashMap<SmolStr, TypeTagId>,
    class_fields: &HashMap<SmolStr, Vec<SmolStr>>,
    class_field_defaults: &HashMap<SmolStr, Vec<Option<hir::FieldDefault>>>,
    function_names: &HashSet<SmolStr>,
    field_names: &HashSet<SmolStr>,
    shape_names: &HashSet<SmolStr>,
    shape_graphs: &HashMap<SmolStr, hir::ShapeGraph>,
    field_graphs: &HashMap<SmolStr, hir::FieldGraph>,
    field_bodies: &HashMap<SmolStr, hir::Body>,
    field_metadata: &HashMap<SmolStr, hir::FieldMetadata>,
    radiance_param_counts: &HashMap<SmolStr, usize>,
    volume_param_counts: &HashMap<SmolStr, usize>,
    result_functions: &HashSet<SmolStr>,
    class_method_ids: &HashMap<SmolStr, HashMap<SmolStr, u32>>,
    interface_methods: &HashMap<SmolStr, HashSet<SmolStr>>,
) -> MirFunction {
    let helper_name = SmolStr::new("__wr_world_trace_capture");
    let span = TextRange::empty(0.into());
    let mut lowerer = FunctionLowerer::new(
        helper_name.clone(),
        type_tags,
        class_fields,
        class_field_defaults,
        function_names,
        field_names,
        shape_names,
        shape_graphs,
        field_graphs,
        field_bodies,
        field_metadata,
        radiance_param_counts,
        volume_param_counts,
        result_functions,
        class_method_ids,
        interface_methods,
        false,
        None,
    );

    let capture = lowerer.new_local(
        SmolStr::new("capture"),
        false,
        MirType::Named(SmolStr::new("RegionCapture")),
    );
    let domain = lowerer.new_local(
        SmolStr::new("domain"),
        false,
        MirType::Named(SmolStr::new("SceneDomain")),
    );
    let origin = lowerer.new_local(SmolStr::new("origin"), false, MirType::Vec3);
    let direction = lowerer.new_local(SmolStr::new("direction"), false, MirType::Vec3);
    let max_distance = lowerer.new_local(SmolStr::new("max_distance"), false, MirType::Float);
    let min_step = lowerer.new_local(SmolStr::new("min_step"), false, MirType::Float);
    let hit_epsilon = lowerer.new_local(SmolStr::new("hit_epsilon"), false, MirType::Float);
    let max_steps = lowerer.new_local(SmolStr::new("max_steps"), false, MirType::Integer);
    for (name, local) in [
        (SmolStr::new("capture"), capture),
        (SmolStr::new("domain"), domain),
        (SmolStr::new("origin"), origin),
        (SmolStr::new("direction"), direction),
        (SmolStr::new("max_distance"), max_distance),
        (SmolStr::new("min_step"), min_step),
        (SmolStr::new("hit_epsilon"), hit_epsilon),
        (SmolStr::new("max_steps"), max_steps),
    ] {
        lowerer.declare_local(name, local);
        lowerer.params.push(local);
    }

    let entry = lowerer.new_block();
    lowerer.current_block = entry;
    let capture_scene_id = lowerer.lower_get_named_field(
        Value::Local(capture),
        "RegionCapture",
        "scene_id",
        MirType::Integer,
        span,
    );
    let domain_scene_id = lowerer.lower_get_named_field(
        Value::Local(domain),
        "SceneDomain",
        "scene_id",
        MirType::Integer,
        span,
    );
    let scene_ids_match = lowerer.lower_binary_temp(
        MirType::Boolean,
        BinaryOp::Eq,
        capture_scene_id.clone(),
        domain_scene_id,
        span,
    );
    let matched_block = lowerer.new_block();
    let mismatch_block = lowerer.new_block();
    lowerer.set_terminator(Terminator::Branch {
        cond: scene_ids_match,
        then_target: matched_block,
        else_target: mismatch_block,
        span,
    });

    lowerer.current_block = mismatch_block;
    let crash_temp = lowerer.new_temp(MirType::Unknown);
    lowerer.push_stmt(MirStmt::Assign {
        place: Place::Temp(crash_temp),
        value: Rvalue::Crash {
            value: Value::Const(Literal::String(SmolStr::new(
                "trace_world requires a domain derived from the same region capture",
            ))),
        },
        span,
    });
    lowerer.set_terminator(Terminator::Return {
        value: Some(Value::Temp(crash_temp)),
        span,
    });

    lowerer.current_block = matched_block;
    let detail = lowerer.lower_get_named_field(
        Value::Local(domain),
        "SceneDomain",
        "geometry_detail",
        MirType::Integer,
        span,
    );
    let result = lowerer.new_local(
        SmolStr::new("$world_trace_result"),
        true,
        MirType::Named(SmolStr::new("Hit3")),
    );
    let default_hit = lowerer.build_default_hit(Value::Local(origin), span);
    lowerer.assign_use(Place::Local(result), default_hit, span);
    let return_block = lowerer.new_block();
    let mut dispatch_block = lowerer.new_block();
    lowerer.set_terminator(Terminator::Jump {
        target: dispatch_block,
        span,
    });

    for (_, region) in module
        .functions
        .iter()
        .filter(|(_, func)| matches!(func.role, FunctionRole::Region))
    {
        let match_block = lowerer.new_block();
        let next_block = lowerer.new_block();
        lowerer.current_block = dispatch_block;
        let matched = lowerer.lower_binary_temp(
            MirType::Boolean,
            BinaryOp::Eq,
            capture_scene_id.clone(),
            Value::Const(Literal::Integer(stable_region_scene_capture_id(&region.name))),
            span,
        );
        lowerer.set_terminator(Terminator::Branch {
            cond: matched,
            then_target: match_block,
            else_target: next_block,
            span,
        });
        lowerer.current_block = match_block;
        let Ok((coarse_shapes, fine_shapes)) = executable_region_shape_lists(region) else {
            let crash_temp = lowerer.new_temp(MirType::Unknown);
            lowerer.push_stmt(MirStmt::Assign {
                place: Place::Temp(crash_temp),
                value: Rvalue::Crash {
                    value: Value::Const(Literal::String(SmolStr::new(
                        "world queries only support direct region compose items today",
                    ))),
                },
                span,
            });
            lowerer.set_terminator(Terminator::Return {
                value: Some(Value::Temp(crash_temp)),
                span,
            });
            dispatch_block = next_block;
            continue;
        };
        let coarse_block = lowerer.new_block();
        let fine_block = lowerer.new_block();
        let detail_is_coarse = lowerer.lower_binary_temp(
            MirType::Boolean,
            BinaryOp::Eq,
            detail.clone(),
            Value::Const(Literal::Integer(0)),
            span,
        );
        lowerer.set_terminator(Terminator::Branch {
            cond: detail_is_coarse,
            then_target: coarse_block,
            else_target: fine_block,
            span,
        });

        for (shapes, block) in [(&coarse_shapes, coarse_block), (&fine_shapes, fine_block)] {
            lowerer.current_block = block;
            for shape in shapes {
                let candidate = lowerer.lower_call_temp(
                    MirType::Named(SmolStr::new("Hit3")),
                    SmolStr::new(format!("__wr_shape_trace_{}", shape)),
                    vec![
                        Value::Local(origin),
                        Value::Local(direction),
                        Value::Local(max_distance),
                        Value::Local(min_step),
                        Value::Local(hit_epsilon),
                        Value::Local(max_steps),
                    ],
                    span,
                );
                let candidate_hit = lowerer.lower_get_named_field(
                    candidate.clone(),
                    "Hit3",
                    "hit",
                    MirType::Boolean,
                    span,
                );
                let current_hit = lowerer.lower_get_named_field(
                    Value::Local(result),
                    "Hit3",
                    "hit",
                    MirType::Boolean,
                    span,
                );
                let candidate_distance = lowerer.lower_get_named_field(
                    candidate.clone(),
                    "Hit3",
                    "distance",
                    MirType::Float,
                    span,
                );
                let current_distance = lowerer.lower_get_named_field(
                    Value::Local(result),
                    "Hit3",
                    "distance",
                    MirType::Float,
                    span,
                );
                let current_miss =
                    lowerer.lower_unary_temp(MirType::Boolean, UnaryOp::Not, current_hit, span);
                let candidate_nearer = lowerer.lower_binary_temp(
                    MirType::Boolean,
                    BinaryOp::Lt,
                    candidate_distance,
                    current_distance,
                    span,
                );
                let replace = lowerer.lower_binary_temp(
                    MirType::Boolean,
                    BinaryOp::Or,
                    current_miss,
                    candidate_nearer,
                    span,
                );
                let should_take = lowerer.lower_binary_temp(
                    MirType::Boolean,
                    BinaryOp::And,
                    candidate_hit,
                    replace,
                    span,
                );
                let take_block = lowerer.new_block();
                let skip_block = lowerer.new_block();
                let merge_block = lowerer.new_block();
                lowerer.set_terminator(Terminator::Branch {
                    cond: should_take,
                    then_target: take_block,
                    else_target: skip_block,
                    span,
                });
                lowerer.current_block = take_block;
                lowerer.assign_use(Place::Local(result), candidate, span);
                lowerer.set_terminator(Terminator::Jump {
                    target: merge_block,
                    span,
                });
                lowerer.current_block = skip_block;
                lowerer.set_terminator(Terminator::Jump {
                    target: merge_block,
                    span,
                });
                lowerer.current_block = merge_block;
            }
            lowerer.set_terminator(Terminator::Jump {
                target: return_block,
                span,
            });
        }
        dispatch_block = next_block;
    }

    lowerer.current_block = dispatch_block;
    let invalid_scene_block = lowerer.new_block();
    lowerer.set_terminator(Terminator::Jump {
        target: invalid_scene_block,
        span,
    });
    lowerer.current_block = invalid_scene_block;
    let crash_temp = lowerer.new_temp(MirType::Unknown);
    lowerer.push_stmt(MirStmt::Assign {
        place: Place::Temp(crash_temp),
        value: Rvalue::Crash {
            value: Value::Const(Literal::String(SmolStr::new(
                "trace_world requires a capture created from a region declaration",
            ))),
        },
        span,
    });
    lowerer.set_terminator(Terminator::Return {
        value: Some(Value::Temp(crash_temp)),
        span,
    });

    lowerer.current_block = return_block;
    lowerer.set_terminator(Terminator::Return {
        value: Some(Value::Local(result)),
        span,
    });

    MirFunction {
        name: helper_name,
        params: lowerer.params,
        abi_params: vec![
            portable_abi_from_type_ref(
                Some(&hir::TypeRef {
                    name: SmolStr::new("RegionCapture"),
                    name_span: None,
                    args: Vec::new(),
                }),
                module,
                type_tags,
                &mut HashSet::new(),
            ),
            portable_abi_from_type_ref(
                Some(&hir::TypeRef {
                    name: SmolStr::new("SceneDomain"),
                    name_span: None,
                    args: Vec::new(),
                }),
                module,
                type_tags,
                &mut HashSet::new(),
            ),
            PortableAbiType::Vec3,
            PortableAbiType::Vec3,
            PortableAbiType::F32,
            PortableAbiType::F32,
            PortableAbiType::F32,
            PortableAbiType::I64,
        ],
        abi_return: portable_abi_from_type_ref(
            Some(&hir::TypeRef {
                name: SmolStr::new("Hit3"),
                name_span: None,
                args: Vec::new(),
            }),
            module,
            type_tags,
            &mut HashSet::new(),
        ),
        locals: lowerer.locals,
        temps: lowerer.temps,
        blocks: lowerer.blocks,
        entry,
        suspendable: false,
    }
}

fn lower_world_surface_capture_helper(
    module: &hir::Module,
    type_tags: &HashMap<SmolStr, TypeTagId>,
    class_fields: &HashMap<SmolStr, Vec<SmolStr>>,
    class_field_defaults: &HashMap<SmolStr, Vec<Option<hir::FieldDefault>>>,
    function_names: &HashSet<SmolStr>,
    field_names: &HashSet<SmolStr>,
    shape_names: &HashSet<SmolStr>,
    shape_graphs: &HashMap<SmolStr, hir::ShapeGraph>,
    field_graphs: &HashMap<SmolStr, hir::FieldGraph>,
    field_bodies: &HashMap<SmolStr, hir::Body>,
    field_metadata: &HashMap<SmolStr, hir::FieldMetadata>,
    radiance_param_counts: &HashMap<SmolStr, usize>,
    volume_param_counts: &HashMap<SmolStr, usize>,
    result_functions: &HashSet<SmolStr>,
    class_method_ids: &HashMap<SmolStr, HashMap<SmolStr, u32>>,
    interface_methods: &HashMap<SmolStr, HashSet<SmolStr>>,
) -> MirFunction {
    let helper_name = SmolStr::new("__wr_world_surface_capture");
    let span = TextRange::empty(0.into());
    let mut lowerer = FunctionLowerer::new(
        helper_name.clone(),
        type_tags,
        class_fields,
        class_field_defaults,
        function_names,
        field_names,
        shape_names,
        shape_graphs,
        field_graphs,
        field_bodies,
        field_metadata,
        radiance_param_counts,
        volume_param_counts,
        result_functions,
        class_method_ids,
        interface_methods,
        false,
        None,
    );

    let capture = lowerer.new_local(
        SmolStr::new("capture"),
        false,
        MirType::Named(SmolStr::new("RegionCapture")),
    );
    let domain = lowerer.new_local(
        SmolStr::new("domain"),
        false,
        MirType::Named(SmolStr::new("SceneDomain")),
    );
    let hit = lowerer.new_local(
        SmolStr::new("hit"),
        false,
        MirType::Named(SmolStr::new("Hit3")),
    );
    for (name, local) in [
        (SmolStr::new("capture"), capture),
        (SmolStr::new("domain"), domain),
        (SmolStr::new("hit"), hit),
    ] {
        lowerer.declare_local(name, local);
        lowerer.params.push(local);
    }

    let entry = lowerer.new_block();
    lowerer.current_block = entry;
    let capture_scene_id = lowerer.lower_get_named_field(
        Value::Local(capture),
        "RegionCapture",
        "scene_id",
        MirType::Integer,
        span,
    );
    let domain_scene_id = lowerer.lower_get_named_field(
        Value::Local(domain),
        "SceneDomain",
        "scene_id",
        MirType::Integer,
        span,
    );
    let scene_ids_match = lowerer.lower_binary_temp(
        MirType::Boolean,
        BinaryOp::Eq,
        capture_scene_id,
        domain_scene_id,
        span,
    );
    let matched_block = lowerer.new_block();
    let mismatch_block = lowerer.new_block();
    lowerer.set_terminator(Terminator::Branch {
        cond: scene_ids_match,
        then_target: matched_block,
        else_target: mismatch_block,
        span,
    });

    lowerer.current_block = mismatch_block;
    let crash_temp = lowerer.new_temp(MirType::Unknown);
    lowerer.push_stmt(MirStmt::Assign {
        place: Place::Temp(crash_temp),
        value: Rvalue::Crash {
            value: Value::Const(Literal::String(SmolStr::new(
                "surface_world requires a domain derived from the same region capture",
            ))),
        },
        span,
    });
    lowerer.set_terminator(Terminator::Return {
        value: Some(Value::Temp(crash_temp)),
        span,
    });

    lowerer.current_block = matched_block;
    let material_enabled = lowerer.lower_get_named_field(
        Value::Local(domain),
        "SceneDomain",
        "material",
        MirType::Boolean,
        span,
    );
    let material_block = lowerer.new_block();
    let disabled_block = lowerer.new_block();
    lowerer.set_terminator(Terminator::Branch {
        cond: material_enabled,
        then_target: material_block,
        else_target: disabled_block,
        span,
    });

    lowerer.current_block = disabled_block;
    let default_surface = lowerer.build_default_surface(span);
    lowerer.set_terminator(Terminator::Return {
        value: Some(default_surface),
        span,
    });

    lowerer.current_block = material_block;
    let root_shape_id = lowerer.lower_get_named_field(
        Value::Local(hit),
        "Hit3",
        "root_shape_id",
        MirType::Integer,
        span,
    );
    let result = lowerer.new_local(
        SmolStr::new("$world_surface_result"),
        true,
        MirType::Named(SmolStr::new("Surface")),
    );
    let default_surface = lowerer.build_default_surface(span);
    lowerer.assign_use(Place::Local(result), default_surface, span);
    let return_block = lowerer.new_block();
    let mut dispatch_block = lowerer.new_block();
    lowerer.set_terminator(Terminator::Jump {
        target: dispatch_block,
        span,
    });
    for (_, shape) in module.shapes.iter().filter(|(_, shape)| shape.graph.is_some()) {
        let match_block = lowerer.new_block();
        let next_block = lowerer.new_block();
        lowerer.current_block = dispatch_block;
        let matched = lowerer.lower_binary_temp(
            MirType::Boolean,
            BinaryOp::Eq,
            root_shape_id.clone(),
            Value::Const(Literal::Integer(stable_shape_capture_id(&shape.name))),
            span,
        );
        lowerer.set_terminator(Terminator::Branch {
            cond: matched,
            then_target: match_block,
            else_target: next_block,
            span,
        });
        lowerer.current_block = match_block;
        let surface = lowerer.lower_call_temp(
            MirType::Named(SmolStr::new("Surface")),
            SmolStr::new(format!("__wr_shape_surface_{}", shape.name)),
            vec![Value::Local(hit)],
            span,
        );
        lowerer.assign_use(Place::Local(result), surface, span);
        lowerer.set_terminator(Terminator::Jump {
            target: return_block,
            span,
        });
        dispatch_block = next_block;
    }
    lowerer.current_block = dispatch_block;
    lowerer.set_terminator(Terminator::Jump {
        target: return_block,
        span,
    });
    lowerer.current_block = return_block;
    lowerer.set_terminator(Terminator::Return {
        value: Some(Value::Local(result)),
        span,
    });

    MirFunction {
        name: helper_name,
        params: lowerer.params,
        abi_params: vec![
            portable_abi_from_type_ref(
                Some(&hir::TypeRef {
                    name: SmolStr::new("RegionCapture"),
                    name_span: None,
                    args: Vec::new(),
                }),
                module,
                type_tags,
                &mut HashSet::new(),
            ),
            portable_abi_from_type_ref(
                Some(&hir::TypeRef {
                    name: SmolStr::new("SceneDomain"),
                    name_span: None,
                    args: Vec::new(),
                }),
                module,
                type_tags,
                &mut HashSet::new(),
            ),
            portable_abi_from_type_ref(
                Some(&hir::TypeRef {
                    name: SmolStr::new("Hit3"),
                    name_span: None,
                    args: Vec::new(),
                }),
                module,
                type_tags,
                &mut HashSet::new(),
            ),
        ],
        abi_return: portable_abi_from_type_ref(
            Some(&hir::TypeRef {
                name: SmolStr::new("Surface"),
                name_span: None,
                args: Vec::new(),
            }),
            module,
            type_tags,
            &mut HashSet::new(),
        ),
        locals: lowerer.locals,
        temps: lowerer.temps,
        blocks: lowerer.blocks,
        entry,
        suspendable: false,
    }
}

fn lower_world_radiance_capture_helper(
    module: &hir::Module,
    type_tags: &HashMap<SmolStr, TypeTagId>,
    class_fields: &HashMap<SmolStr, Vec<SmolStr>>,
    class_field_defaults: &HashMap<SmolStr, Vec<Option<hir::FieldDefault>>>,
    function_names: &HashSet<SmolStr>,
    field_names: &HashSet<SmolStr>,
    shape_names: &HashSet<SmolStr>,
    shape_graphs: &HashMap<SmolStr, hir::ShapeGraph>,
    field_graphs: &HashMap<SmolStr, hir::FieldGraph>,
    field_bodies: &HashMap<SmolStr, hir::Body>,
    field_metadata: &HashMap<SmolStr, hir::FieldMetadata>,
    radiance_param_counts: &HashMap<SmolStr, usize>,
    volume_param_counts: &HashMap<SmolStr, usize>,
    result_functions: &HashSet<SmolStr>,
    class_method_ids: &HashMap<SmolStr, HashMap<SmolStr, u32>>,
    interface_methods: &HashMap<SmolStr, HashSet<SmolStr>>,
) -> MirFunction {
    let helper_name = SmolStr::new("__wr_world_radiance_capture");
    let span = TextRange::empty(0.into());
    let mut lowerer = FunctionLowerer::new(
        helper_name.clone(),
        type_tags,
        class_fields,
        class_field_defaults,
        function_names,
        field_names,
        shape_names,
        shape_graphs,
        field_graphs,
        field_bodies,
        field_metadata,
        radiance_param_counts,
        volume_param_counts,
        result_functions,
        class_method_ids,
        interface_methods,
        false,
        None,
    );

    let capture = lowerer.new_local(
        SmolStr::new("capture"),
        false,
        MirType::Named(SmolStr::new("RegionCapture")),
    );
    let domain = lowerer.new_local(
        SmolStr::new("domain"),
        false,
        MirType::Named(SmolStr::new("SceneDomain")),
    );
    let point = lowerer.new_local(SmolStr::new("point"), false, MirType::Vec3);
    let direction = lowerer.new_local(SmolStr::new("direction"), false, MirType::Vec3);
    for (name, local) in [
        (SmolStr::new("capture"), capture),
        (SmolStr::new("domain"), domain),
        (SmolStr::new("point"), point),
        (SmolStr::new("direction"), direction),
    ] {
        lowerer.declare_local(name, local);
        lowerer.params.push(local);
    }

    let entry = lowerer.new_block();
    lowerer.current_block = entry;
    let capture_scene_id = lowerer.lower_get_named_field(
        Value::Local(capture),
        "RegionCapture",
        "scene_id",
        MirType::Integer,
        span,
    );
    let domain_scene_id = lowerer.lower_get_named_field(
        Value::Local(domain),
        "SceneDomain",
        "scene_id",
        MirType::Integer,
        span,
    );
    let scene_ids_match = lowerer.lower_binary_temp(
        MirType::Boolean,
        BinaryOp::Eq,
        capture_scene_id.clone(),
        domain_scene_id,
        span,
    );
    let matched_block = lowerer.new_block();
    let mismatch_block = lowerer.new_block();
    lowerer.set_terminator(Terminator::Branch {
        cond: scene_ids_match,
        then_target: matched_block,
        else_target: mismatch_block,
        span,
    });

    lowerer.current_block = mismatch_block;
    let crash_temp = lowerer.new_temp(MirType::Unknown);
    lowerer.push_stmt(MirStmt::Assign {
        place: Place::Temp(crash_temp),
        value: Rvalue::Crash {
            value: Value::Const(Literal::String(SmolStr::new(
                "radiance_world requires a domain derived from the same region capture",
            ))),
        },
        span,
    });
    lowerer.set_terminator(Terminator::Return {
        value: Some(Value::Temp(crash_temp)),
        span,
    });

    lowerer.current_block = matched_block;
    let radiance_enabled = lowerer.lower_get_named_field(
        Value::Local(domain),
        "SceneDomain",
        "radiance",
        MirType::Boolean,
        span,
    );
    let enabled_block = lowerer.new_block();
    let disabled_block = lowerer.new_block();
    lowerer.set_terminator(Terminator::Branch {
        cond: radiance_enabled,
        then_target: enabled_block,
        else_target: disabled_block,
        span,
    });
    lowerer.current_block = disabled_block;
    let black = lowerer.lower_call_temp(
        MirType::Vec3,
        SmolStr::new("vec3"),
        vec![
            Value::Const(Literal::Float(0.0)),
            Value::Const(Literal::Float(0.0)),
            Value::Const(Literal::Float(0.0)),
        ],
        span,
    );
    lowerer.set_terminator(Terminator::Return {
        value: Some(black),
        span,
    });

    lowerer.current_block = enabled_block;
    let detail = lowerer.lower_get_named_field(
        Value::Local(domain),
        "SceneDomain",
        "geometry_detail",
        MirType::Integer,
        span,
    );
    let result = lowerer.new_local(SmolStr::new("$world_radiance_result"), true, MirType::Vec3);
    let zero = lowerer.lower_call_temp(
        MirType::Vec3,
        SmolStr::new("vec3"),
        vec![
            Value::Const(Literal::Float(0.0)),
            Value::Const(Literal::Float(0.0)),
            Value::Const(Literal::Float(0.0)),
        ],
        span,
    );
    lowerer.assign_use(Place::Local(result), zero, span);
    let best_distance =
        lowerer.new_local(SmolStr::new("$world_radiance_best"), true, MirType::Float);
    lowerer.assign_use(
        Place::Local(best_distance),
        Value::Const(Literal::Float(1_000_000.0)),
        span,
    );
    let return_block = lowerer.new_block();
    let mut dispatch_block = lowerer.new_block();
    lowerer.set_terminator(Terminator::Jump {
        target: dispatch_block,
        span,
    });

    for (_, region) in module
        .functions
        .iter()
        .filter(|(_, func)| matches!(func.role, FunctionRole::Region))
    {
        let match_block = lowerer.new_block();
        let next_block = lowerer.new_block();
        lowerer.current_block = dispatch_block;
        let matched = lowerer.lower_binary_temp(
            MirType::Boolean,
            BinaryOp::Eq,
            capture_scene_id.clone(),
            Value::Const(Literal::Integer(stable_region_scene_capture_id(&region.name))),
            span,
        );
        lowerer.set_terminator(Terminator::Branch {
            cond: matched,
            then_target: match_block,
            else_target: next_block,
            span,
        });
        lowerer.current_block = match_block;
        let Ok((coarse_shapes, fine_shapes)) = executable_region_shape_lists(region) else {
            let crash_temp = lowerer.new_temp(MirType::Unknown);
            lowerer.push_stmt(MirStmt::Assign {
                place: Place::Temp(crash_temp),
                value: Rvalue::Crash {
                    value: Value::Const(Literal::String(SmolStr::new(
                        "world queries only support direct region compose items today",
                    ))),
                },
                span,
            });
            lowerer.set_terminator(Terminator::Return {
                value: Some(Value::Temp(crash_temp)),
                span,
            });
            dispatch_block = next_block;
            continue;
        };
        let coarse_block = lowerer.new_block();
        let fine_block = lowerer.new_block();
        let detail_is_coarse = lowerer.lower_binary_temp(
            MirType::Boolean,
            BinaryOp::Eq,
            detail.clone(),
            Value::Const(Literal::Integer(0)),
            span,
        );
        lowerer.set_terminator(Terminator::Branch {
            cond: detail_is_coarse,
            then_target: coarse_block,
            else_target: fine_block,
            span,
        });
        for (shapes, block) in [(&coarse_shapes, coarse_block), (&fine_shapes, fine_block)] {
            lowerer.current_block = block;
            for shape_name in shapes {
                let distance =
                    lowerer.lower_shape_distance_call(shape_name, Value::Local(point), span);
                let better = lowerer.lower_binary_temp(
                    MirType::Boolean,
                    BinaryOp::Lt,
                    distance.clone(),
                    Value::Local(best_distance),
                    span,
                );
                let take_block = lowerer.new_block();
                let skip_block = lowerer.new_block();
                let merge_block = lowerer.new_block();
                lowerer.set_terminator(Terminator::Branch {
                    cond: better,
                    then_target: take_block,
                    else_target: skip_block,
                    span,
                });
                lowerer.current_block = take_block;
                if let Some(shape) = module.shapes.iter().find_map(|(_, shape)| {
                    (shape.name == *shape_name).then_some(shape)
                }) {
                    if let Some(graph) = shape.graph.as_ref() {
                        let (_, _, feature_id) = lowerer.lower_shape_payload_selection(
                            &graph.root,
                            graph.provenance.as_ref(),
                            Value::Local(point),
                            Value::Const(Literal::Float(0.001)),
                            &mut vec![shape.name.clone()],
                            span,
                        );
                        let (_, radiance) = lowerer.lower_shape_radiance_selection(
                            &graph.root,
                            feature_id,
                            Value::Local(point),
                            Value::Local(direction),
                            &mut vec![shape.name.clone()],
                            span,
                        );
                        lowerer.assign_use(Place::Local(result), radiance, span);
                        lowerer.assign_use(Place::Local(best_distance), distance, span);
                    }
                }
                lowerer.set_terminator(Terminator::Jump {
                    target: merge_block,
                    span,
                });
                lowerer.current_block = skip_block;
                lowerer.set_terminator(Terminator::Jump {
                    target: merge_block,
                    span,
                });
                lowerer.current_block = merge_block;
            }
            lowerer.set_terminator(Terminator::Jump {
                target: return_block,
                span,
            });
        }
        dispatch_block = next_block;
    }

    lowerer.current_block = dispatch_block;
    lowerer.set_terminator(Terminator::Jump {
        target: return_block,
        span,
    });
    lowerer.current_block = return_block;
    lowerer.set_terminator(Terminator::Return {
        value: Some(Value::Local(result)),
        span,
    });

    MirFunction {
        name: helper_name,
        params: lowerer.params,
        abi_params: vec![
            portable_abi_from_type_ref(
                Some(&hir::TypeRef {
                    name: SmolStr::new("RegionCapture"),
                    name_span: None,
                    args: Vec::new(),
                }),
                module,
                type_tags,
                &mut HashSet::new(),
            ),
            portable_abi_from_type_ref(
                Some(&hir::TypeRef {
                    name: SmolStr::new("SceneDomain"),
                    name_span: None,
                    args: Vec::new(),
                }),
                module,
                type_tags,
                &mut HashSet::new(),
            ),
            PortableAbiType::Vec3,
            PortableAbiType::Vec3,
        ],
        abi_return: PortableAbiType::Vec3,
        locals: lowerer.locals,
        temps: lowerer.temps,
        blocks: lowerer.blocks,
        entry,
        suspendable: false,
    }
}

fn lower_world_medium_capture_helper(
    module: &hir::Module,
    type_tags: &HashMap<SmolStr, TypeTagId>,
    class_fields: &HashMap<SmolStr, Vec<SmolStr>>,
    class_field_defaults: &HashMap<SmolStr, Vec<Option<hir::FieldDefault>>>,
    function_names: &HashSet<SmolStr>,
    field_names: &HashSet<SmolStr>,
    shape_names: &HashSet<SmolStr>,
    shape_graphs: &HashMap<SmolStr, hir::ShapeGraph>,
    field_graphs: &HashMap<SmolStr, hir::FieldGraph>,
    field_bodies: &HashMap<SmolStr, hir::Body>,
    field_metadata: &HashMap<SmolStr, hir::FieldMetadata>,
    radiance_param_counts: &HashMap<SmolStr, usize>,
    volume_param_counts: &HashMap<SmolStr, usize>,
    result_functions: &HashSet<SmolStr>,
    class_method_ids: &HashMap<SmolStr, HashMap<SmolStr, u32>>,
    interface_methods: &HashMap<SmolStr, HashSet<SmolStr>>,
) -> MirFunction {
    let helper_name = SmolStr::new("__wr_world_medium_capture");
    let span = TextRange::empty(0.into());
    let mut lowerer = FunctionLowerer::new(
        helper_name.clone(),
        type_tags,
        class_fields,
        class_field_defaults,
        function_names,
        field_names,
        shape_names,
        shape_graphs,
        field_graphs,
        field_bodies,
        field_metadata,
        radiance_param_counts,
        volume_param_counts,
        result_functions,
        class_method_ids,
        interface_methods,
        false,
        None,
    );

    let capture = lowerer.new_local(
        SmolStr::new("capture"),
        false,
        MirType::Named(SmolStr::new("RegionCapture")),
    );
    let domain = lowerer.new_local(
        SmolStr::new("domain"),
        false,
        MirType::Named(SmolStr::new("SceneDomain")),
    );
    let point = lowerer.new_local(SmolStr::new("point"), false, MirType::Vec3);
    for (name, local) in [
        (SmolStr::new("capture"), capture),
        (SmolStr::new("domain"), domain),
        (SmolStr::new("point"), point),
    ] {
        lowerer.declare_local(name, local);
        lowerer.params.push(local);
    }

    let entry = lowerer.new_block();
    lowerer.current_block = entry;
    let capture_scene_id = lowerer.lower_get_named_field(
        Value::Local(capture),
        "RegionCapture",
        "scene_id",
        MirType::Integer,
        span,
    );
    let domain_scene_id = lowerer.lower_get_named_field(
        Value::Local(domain),
        "SceneDomain",
        "scene_id",
        MirType::Integer,
        span,
    );
    let scene_ids_match = lowerer.lower_binary_temp(
        MirType::Boolean,
        BinaryOp::Eq,
        capture_scene_id.clone(),
        domain_scene_id,
        span,
    );
    let matched_block = lowerer.new_block();
    let mismatch_block = lowerer.new_block();
    lowerer.set_terminator(Terminator::Branch {
        cond: scene_ids_match,
        then_target: matched_block,
        else_target: mismatch_block,
        span,
    });

    lowerer.current_block = mismatch_block;
    let crash_temp = lowerer.new_temp(MirType::Unknown);
    lowerer.push_stmt(MirStmt::Assign {
        place: Place::Temp(crash_temp),
        value: Rvalue::Crash {
            value: Value::Const(Literal::String(SmolStr::new(
                "medium_world requires a domain derived from the same region capture",
            ))),
        },
        span,
    });
    lowerer.set_terminator(Terminator::Return {
        value: Some(Value::Temp(crash_temp)),
        span,
    });

    lowerer.current_block = matched_block;
    let media_enabled = lowerer.lower_get_named_field(
        Value::Local(domain),
        "SceneDomain",
        "media",
        MirType::Boolean,
        span,
    );
    let enabled_block = lowerer.new_block();
    let disabled_block = lowerer.new_block();
    lowerer.set_terminator(Terminator::Branch {
        cond: media_enabled,
        then_target: enabled_block,
        else_target: disabled_block,
        span,
    });
    lowerer.current_block = disabled_block;
    let default_medium = lowerer.build_default_medium(span);
    lowerer.set_terminator(Terminator::Return {
        value: Some(default_medium),
        span,
    });

    lowerer.current_block = enabled_block;
    let detail = lowerer.lower_get_named_field(
        Value::Local(domain),
        "SceneDomain",
        "geometry_detail",
        MirType::Integer,
        span,
    );
    let result = lowerer.new_local(
        SmolStr::new("$world_medium_result"),
        true,
        MirType::Named(SmolStr::new("Medium")),
    );
    let default_medium = lowerer.build_default_medium(span);
    lowerer.assign_use(Place::Local(result), default_medium, span);
    let best_distance =
        lowerer.new_local(SmolStr::new("$world_medium_best"), true, MirType::Float);
    lowerer.assign_use(
        Place::Local(best_distance),
        Value::Const(Literal::Float(1_000_000.0)),
        span,
    );
    let return_block = lowerer.new_block();
    let mut dispatch_block = lowerer.new_block();
    lowerer.set_terminator(Terminator::Jump {
        target: dispatch_block,
        span,
    });

    for (_, region) in module
        .functions
        .iter()
        .filter(|(_, func)| matches!(func.role, FunctionRole::Region))
    {
        let match_block = lowerer.new_block();
        let next_block = lowerer.new_block();
        lowerer.current_block = dispatch_block;
        let matched = lowerer.lower_binary_temp(
            MirType::Boolean,
            BinaryOp::Eq,
            capture_scene_id.clone(),
            Value::Const(Literal::Integer(stable_region_scene_capture_id(&region.name))),
            span,
        );
        lowerer.set_terminator(Terminator::Branch {
            cond: matched,
            then_target: match_block,
            else_target: next_block,
            span,
        });
        lowerer.current_block = match_block;
        let Ok((coarse_shapes, fine_shapes)) = executable_region_shape_lists(region) else {
            let crash_temp = lowerer.new_temp(MirType::Unknown);
            lowerer.push_stmt(MirStmt::Assign {
                place: Place::Temp(crash_temp),
                value: Rvalue::Crash {
                    value: Value::Const(Literal::String(SmolStr::new(
                        "world queries only support direct region compose items today",
                    ))),
                },
                span,
            });
            lowerer.set_terminator(Terminator::Return {
                value: Some(Value::Temp(crash_temp)),
                span,
            });
            dispatch_block = next_block;
            continue;
        };
        let coarse_block = lowerer.new_block();
        let fine_block = lowerer.new_block();
        let detail_is_coarse = lowerer.lower_binary_temp(
            MirType::Boolean,
            BinaryOp::Eq,
            detail.clone(),
            Value::Const(Literal::Integer(0)),
            span,
        );
        lowerer.set_terminator(Terminator::Branch {
            cond: detail_is_coarse,
            then_target: coarse_block,
            else_target: fine_block,
            span,
        });
        for (shapes, block) in [(&coarse_shapes, coarse_block), (&fine_shapes, fine_block)] {
            lowerer.current_block = block;
            for shape_name in shapes {
                let distance =
                    lowerer.lower_shape_distance_call(shape_name, Value::Local(point), span);
                let better = lowerer.lower_binary_temp(
                    MirType::Boolean,
                    BinaryOp::Lt,
                    distance.clone(),
                    Value::Local(best_distance),
                    span,
                );
                let take_block = lowerer.new_block();
                let skip_block = lowerer.new_block();
                let merge_block = lowerer.new_block();
                lowerer.set_terminator(Terminator::Branch {
                    cond: better,
                    then_target: take_block,
                    else_target: skip_block,
                    span,
                });
                lowerer.current_block = take_block;
                if let Some(shape) = module.shapes.iter().find_map(|(_, shape)| {
                    (shape.name == *shape_name).then_some(shape)
                }) {
                    if let Some(graph) = shape.graph.as_ref() {
                        let (_, _, feature_id) = lowerer.lower_shape_payload_selection(
                            &graph.root,
                            graph.provenance.as_ref(),
                            Value::Local(point),
                            Value::Const(Literal::Float(0.001)),
                            &mut vec![shape.name.clone()],
                            span,
                        );
                        let (_, medium) = lowerer.lower_shape_medium_selection(
                            &graph.root,
                            feature_id,
                            Value::Local(point),
                            distance.clone(),
                            &mut vec![shape.name.clone()],
                            span,
                        );
                        lowerer.assign_use(Place::Local(result), medium, span);
                        lowerer.assign_use(Place::Local(best_distance), distance, span);
                    }
                }
                lowerer.set_terminator(Terminator::Jump {
                    target: merge_block,
                    span,
                });
                lowerer.current_block = skip_block;
                lowerer.set_terminator(Terminator::Jump {
                    target: merge_block,
                    span,
                });
                lowerer.current_block = merge_block;
            }
            lowerer.set_terminator(Terminator::Jump {
                target: return_block,
                span,
            });
        }
        dispatch_block = next_block;
    }

    lowerer.current_block = dispatch_block;
    lowerer.set_terminator(Terminator::Jump {
        target: return_block,
        span,
    });
    lowerer.current_block = return_block;
    lowerer.set_terminator(Terminator::Return {
        value: Some(Value::Local(result)),
        span,
    });

    MirFunction {
        name: helper_name,
        params: lowerer.params,
        abi_params: vec![
            portable_abi_from_type_ref(
                Some(&hir::TypeRef {
                    name: SmolStr::new("RegionCapture"),
                    name_span: None,
                    args: Vec::new(),
                }),
                module,
                type_tags,
                &mut HashSet::new(),
            ),
            portable_abi_from_type_ref(
                Some(&hir::TypeRef {
                    name: SmolStr::new("SceneDomain"),
                    name_span: None,
                    args: Vec::new(),
                }),
                module,
                type_tags,
                &mut HashSet::new(),
            ),
            PortableAbiType::Vec3,
        ],
        abi_return: portable_abi_from_type_ref(
            Some(&hir::TypeRef {
                name: SmolStr::new("Medium"),
                name_span: None,
                args: Vec::new(),
            }),
            module,
            type_tags,
            &mut HashSet::new(),
        ),
        locals: lowerer.locals,
        temps: lowerer.temps,
        blocks: lowerer.blocks,
        entry,
        suspendable: false,
    }
}

fn lower_scene_trace_queries_helper(
    _module: &hir::Module,
    type_tags: &HashMap<SmolStr, TypeTagId>,
    class_fields: &HashMap<SmolStr, Vec<SmolStr>>,
    class_field_defaults: &HashMap<SmolStr, Vec<Option<hir::FieldDefault>>>,
    function_names: &HashSet<SmolStr>,
    field_names: &HashSet<SmolStr>,
    shape_names: &HashSet<SmolStr>,
    shape_graphs: &HashMap<SmolStr, hir::ShapeGraph>,
    field_graphs: &HashMap<SmolStr, hir::FieldGraph>,
    field_bodies: &HashMap<SmolStr, hir::Body>,
    field_metadata: &HashMap<SmolStr, hir::FieldMetadata>,
    radiance_param_counts: &HashMap<SmolStr, usize>,
    volume_param_counts: &HashMap<SmolStr, usize>,
    result_functions: &HashSet<SmolStr>,
    class_method_ids: &HashMap<SmolStr, HashMap<SmolStr, u32>>,
    interface_methods: &HashMap<SmolStr, HashSet<SmolStr>>,
) -> MirFunction {
    let helper_name = SmolStr::new("__wr_scene_trace_queries");
    let span = TextRange::empty(0.into());
    let mut lowerer = FunctionLowerer::new(
        helper_name.clone(),
        type_tags,
        class_fields,
        class_field_defaults,
        function_names,
        field_names,
        shape_names,
        shape_graphs,
        field_graphs,
        field_bodies,
        field_metadata,
        radiance_param_counts,
        volume_param_counts,
        result_functions,
        class_method_ids,
        interface_methods,
        false,
        None,
    );

    let queries = lowerer.new_local(
        SmolStr::new("queries"),
        false,
        MirType::Named(SmolStr::new("List")),
    );
    lowerer.declare_local(SmolStr::new("queries"), queries);
    lowerer.params.push(queries);

    let entry = lowerer.new_block();
    lowerer.current_block = entry;
    let len = lowerer.lower_call_temp(
        MirType::Integer,
        SmolStr::new("__wr_list_len"),
        vec![Value::Local(queries)],
        span,
    );
    let result = lowerer.lower_call_temp(
        MirType::Named(SmolStr::new("List")),
        SmolStr::new("__wr_list_new"),
        vec![len.clone()],
        span,
    );
    let index = lowerer.new_local(SmolStr::new("$query_index"), true, MirType::Integer);
    lowerer.assign_use(Place::Local(index), Value::Const(Literal::Integer(0)), span);
    let head = lowerer.new_block();
    let body_block = lowerer.new_block();
    let exit = lowerer.new_block();
    lowerer.set_terminator(Terminator::Jump { target: head, span });

    lowerer.current_block = head;
    let within_bounds = lowerer.lower_binary_temp(
        MirType::Boolean,
        BinaryOp::Lt,
        Value::Local(index),
        len,
        span,
    );
    lowerer.set_terminator(Terminator::Branch {
        cond: within_bounds,
        then_target: body_block,
        else_target: exit,
        span,
    });

    lowerer.current_block = body_block;
    let query = lowerer.lower_call_temp(
        MirType::Named(SmolStr::new("TraceQuery")),
        SmolStr::new("__wr_list_get"),
        vec![Value::Local(queries), Value::Local(index)],
        span,
    );
    let capture = lowerer.new_temp(MirType::Named(SmolStr::new("ShapeCapture")));
    lowerer.push_stmt(MirStmt::Assign {
        place: Place::Temp(capture),
        value: Rvalue::GetField {
            base: query.clone(),
            field: SmolStr::new("capture"),
            slot: lowerer.field_slot("TraceQuery", "capture"),
        },
        span,
    });
    let origin = lowerer.new_temp(MirType::Vec3);
    lowerer.push_stmt(MirStmt::Assign {
        place: Place::Temp(origin),
        value: Rvalue::GetField {
            base: query.clone(),
            field: SmolStr::new("origin"),
            slot: lowerer.field_slot("TraceQuery", "origin"),
        },
        span,
    });
    let direction = lowerer.new_temp(MirType::Vec3);
    lowerer.push_stmt(MirStmt::Assign {
        place: Place::Temp(direction),
        value: Rvalue::GetField {
            base: query.clone(),
            field: SmolStr::new("direction"),
            slot: lowerer.field_slot("TraceQuery", "direction"),
        },
        span,
    });
    let max_distance = lowerer.new_temp(MirType::Float);
    lowerer.push_stmt(MirStmt::Assign {
        place: Place::Temp(max_distance),
        value: Rvalue::GetField {
            base: query.clone(),
            field: SmolStr::new("max_distance"),
            slot: lowerer.field_slot("TraceQuery", "max_distance"),
        },
        span,
    });
    let min_step = lowerer.new_temp(MirType::Float);
    lowerer.push_stmt(MirStmt::Assign {
        place: Place::Temp(min_step),
        value: Rvalue::GetField {
            base: query.clone(),
            field: SmolStr::new("min_step"),
            slot: lowerer.field_slot("TraceQuery", "min_step"),
        },
        span,
    });
    let hit_epsilon = lowerer.new_temp(MirType::Float);
    lowerer.push_stmt(MirStmt::Assign {
        place: Place::Temp(hit_epsilon),
        value: Rvalue::GetField {
            base: query.clone(),
            field: SmolStr::new("hit_epsilon"),
            slot: lowerer.field_slot("TraceQuery", "hit_epsilon"),
        },
        span,
    });
    let max_steps = lowerer.new_temp(MirType::Integer);
    lowerer.push_stmt(MirStmt::Assign {
        place: Place::Temp(max_steps),
        value: Rvalue::GetField {
            base: query,
            field: SmolStr::new("max_steps"),
            slot: lowerer.field_slot("TraceQuery", "max_steps"),
        },
        span,
    });
    let hit = lowerer.lower_call_temp(
        MirType::Named(SmolStr::new("Hit3")),
        SmolStr::new("__wr_scene_trace_capture"),
        vec![
            Value::Temp(capture),
            Value::Temp(origin),
            Value::Temp(direction),
            Value::Temp(max_distance),
            Value::Temp(min_step),
            Value::Temp(hit_epsilon),
            Value::Temp(max_steps),
        ],
        span,
    );
    let _ = lowerer.lower_call_temp(
        MirType::Nil,
        SmolStr::new("__wr_list_set"),
        vec![result.clone(), Value::Local(index), hit],
        span,
    );
    let next = lowerer.lower_binary_temp(
        MirType::Integer,
        BinaryOp::Add,
        Value::Local(index),
        Value::Const(Literal::Integer(1)),
        span,
    );
    lowerer.assign_use(Place::Local(index), next, span);
    lowerer.set_terminator(Terminator::Jump { target: head, span });

    lowerer.current_block = exit;
    lowerer.set_terminator(Terminator::Return {
        value: Some(result),
        span,
    });

    MirFunction {
        name: helper_name,
        params: lowerer.params,
        abi_params: vec![PortableAbiType::Value],
        abi_return: PortableAbiType::Value,
        locals: lowerer.locals,
        temps: lowerer.temps,
        blocks: lowerer.blocks,
        entry,
        suspendable: false,
    }
}

fn lower_scene_surface_queries_helper(
    _module: &hir::Module,
    type_tags: &HashMap<SmolStr, TypeTagId>,
    class_fields: &HashMap<SmolStr, Vec<SmolStr>>,
    class_field_defaults: &HashMap<SmolStr, Vec<Option<hir::FieldDefault>>>,
    function_names: &HashSet<SmolStr>,
    field_names: &HashSet<SmolStr>,
    shape_names: &HashSet<SmolStr>,
    shape_graphs: &HashMap<SmolStr, hir::ShapeGraph>,
    field_graphs: &HashMap<SmolStr, hir::FieldGraph>,
    field_bodies: &HashMap<SmolStr, hir::Body>,
    field_metadata: &HashMap<SmolStr, hir::FieldMetadata>,
    radiance_param_counts: &HashMap<SmolStr, usize>,
    volume_param_counts: &HashMap<SmolStr, usize>,
    result_functions: &HashSet<SmolStr>,
    class_method_ids: &HashMap<SmolStr, HashMap<SmolStr, u32>>,
    interface_methods: &HashMap<SmolStr, HashSet<SmolStr>>,
) -> MirFunction {
    let helper_name = SmolStr::new("__wr_scene_surface_queries");
    let span = TextRange::empty(0.into());
    let mut lowerer = FunctionLowerer::new(
        helper_name.clone(),
        type_tags,
        class_fields,
        class_field_defaults,
        function_names,
        field_names,
        shape_names,
        shape_graphs,
        field_graphs,
        field_bodies,
        field_metadata,
        radiance_param_counts,
        volume_param_counts,
        result_functions,
        class_method_ids,
        interface_methods,
        false,
        None,
    );

    let queries = lowerer.new_local(
        SmolStr::new("queries"),
        false,
        MirType::Named(SmolStr::new("List")),
    );
    lowerer.declare_local(SmolStr::new("queries"), queries);
    lowerer.params.push(queries);

    let entry = lowerer.new_block();
    lowerer.current_block = entry;
    let len = lowerer.lower_call_temp(
        MirType::Integer,
        SmolStr::new("__wr_list_len"),
        vec![Value::Local(queries)],
        span,
    );
    let result = lowerer.lower_call_temp(
        MirType::Named(SmolStr::new("List")),
        SmolStr::new("__wr_list_new"),
        vec![len.clone()],
        span,
    );
    let index = lowerer.new_local(SmolStr::new("$query_index"), true, MirType::Integer);
    lowerer.assign_use(Place::Local(index), Value::Const(Literal::Integer(0)), span);
    let head = lowerer.new_block();
    let body_block = lowerer.new_block();
    let exit = lowerer.new_block();
    lowerer.set_terminator(Terminator::Jump { target: head, span });

    lowerer.current_block = head;
    let within_bounds = lowerer.lower_binary_temp(
        MirType::Boolean,
        BinaryOp::Lt,
        Value::Local(index),
        len,
        span,
    );
    lowerer.set_terminator(Terminator::Branch {
        cond: within_bounds,
        then_target: body_block,
        else_target: exit,
        span,
    });

    lowerer.current_block = body_block;
    let query = lowerer.lower_call_temp(
        MirType::Named(SmolStr::new("SurfaceQuery")),
        SmolStr::new("__wr_list_get"),
        vec![Value::Local(queries), Value::Local(index)],
        span,
    );
    let capture = lowerer.new_temp(MirType::Named(SmolStr::new("ShapeCapture")));
    lowerer.push_stmt(MirStmt::Assign {
        place: Place::Temp(capture),
        value: Rvalue::GetField {
            base: query.clone(),
            field: SmolStr::new("capture"),
            slot: lowerer.field_slot("SurfaceQuery", "capture"),
        },
        span,
    });
    let hit = lowerer.new_temp(MirType::Named(SmolStr::new("Hit3")));
    lowerer.push_stmt(MirStmt::Assign {
        place: Place::Temp(hit),
        value: Rvalue::GetField {
            base: query,
            field: SmolStr::new("hit"),
            slot: lowerer.field_slot("SurfaceQuery", "hit"),
        },
        span,
    });
    let surface = lowerer.lower_call_temp(
        MirType::Named(SmolStr::new("Surface")),
        SmolStr::new("__wr_scene_surface_capture"),
        vec![Value::Temp(capture), Value::Temp(hit)],
        span,
    );
    let _ = lowerer.lower_call_temp(
        MirType::Nil,
        SmolStr::new("__wr_list_set"),
        vec![result.clone(), Value::Local(index), surface],
        span,
    );
    let next = lowerer.lower_binary_temp(
        MirType::Integer,
        BinaryOp::Add,
        Value::Local(index),
        Value::Const(Literal::Integer(1)),
        span,
    );
    lowerer.assign_use(Place::Local(index), next, span);
    lowerer.set_terminator(Terminator::Jump { target: head, span });

    lowerer.current_block = exit;
    lowerer.set_terminator(Terminator::Return {
        value: Some(result),
        span,
    });

    MirFunction {
        name: helper_name,
        params: lowerer.params,
        abi_params: vec![PortableAbiType::Value],
        abi_return: PortableAbiType::Value,
        locals: lowerer.locals,
        temps: lowerer.temps,
        blocks: lowerer.blocks,
        entry,
        suspendable: false,
    }
}

struct LoopTarget {
    break_target: BlockId,
    continue_target: BlockId,
}

struct FunctionLowerer {
    name: SmolStr,
    params: Vec<LocalId>,
    locals: Vec<Local>,
    temps: Vec<Temp>,
    blocks: Vec<BasicBlock>,
    current_block: BlockId,
    suspendable: bool,
    scopes: Vec<HashMap<SmolStr, LocalId>>,
    result_scopes: Vec<HashMap<SmolStr, bool>>,
    loop_stack: Vec<LoopTarget>,
    type_tags: HashMap<SmolStr, TypeTagId>,
    class_fields: HashMap<SmolStr, Vec<SmolStr>>,
    class_field_defaults: HashMap<SmolStr, Vec<Option<hir::FieldDefault>>>,
    class_method_ids: HashMap<SmolStr, HashMap<SmolStr, u32>>,
    interface_methods: HashMap<SmolStr, HashSet<SmolStr>>,
    function_names: HashSet<SmolStr>,
    field_names: HashSet<SmolStr>,
    shape_names: HashSet<SmolStr>,
    shape_graphs: HashMap<SmolStr, hir::ShapeGraph>,
    field_graphs: HashMap<SmolStr, hir::FieldGraph>,
    field_bodies: HashMap<SmolStr, hir::Body>,
    field_metadata: HashMap<SmolStr, hir::FieldMetadata>,
    radiance_param_counts: HashMap<SmolStr, usize>,
    volume_param_counts: HashMap<SmolStr, usize>,
    result_functions: HashSet<SmolStr>,
    returns_result: bool,
    type_info: Option<FunctionTypeInfo>,
    defers: Vec<hir::Idx<hir::Expr>>,
    objective_stack: Vec<hir::Objective>,
}

impl FunctionLowerer {
    fn new(
        name: SmolStr,
        type_tags: &HashMap<SmolStr, TypeTagId>,
        class_fields: &HashMap<SmolStr, Vec<SmolStr>>,
        class_field_defaults: &HashMap<SmolStr, Vec<Option<hir::FieldDefault>>>,
        function_names: &HashSet<SmolStr>,
        field_names: &HashSet<SmolStr>,
        shape_names: &HashSet<SmolStr>,
        shape_graphs: &HashMap<SmolStr, hir::ShapeGraph>,
        field_graphs: &HashMap<SmolStr, hir::FieldGraph>,
        field_bodies: &HashMap<SmolStr, hir::Body>,
        field_metadata: &HashMap<SmolStr, hir::FieldMetadata>,
        radiance_param_counts: &HashMap<SmolStr, usize>,
        volume_param_counts: &HashMap<SmolStr, usize>,
        result_functions: &HashSet<SmolStr>,
        class_method_ids: &HashMap<SmolStr, HashMap<SmolStr, u32>>,
        interface_methods: &HashMap<SmolStr, HashSet<SmolStr>>,
        returns_result: bool,
        type_info: Option<&FunctionTypeInfo>,
    ) -> Self {
        Self {
            name,
            params: Vec::new(),
            locals: Vec::new(),
            temps: Vec::new(),
            blocks: Vec::new(),
            current_block: BlockId(0),
            suspendable: false,
            scopes: vec![HashMap::new()],
            result_scopes: vec![HashMap::new()],
            loop_stack: Vec::new(),
            type_tags: type_tags.clone(),
            class_fields: class_fields.clone(),
            class_field_defaults: class_field_defaults.clone(),
            class_method_ids: class_method_ids.clone(),
            interface_methods: interface_methods.clone(),
            function_names: function_names.clone(),
            field_names: field_names.clone(),
            shape_names: shape_names.clone(),
            shape_graphs: shape_graphs.clone(),
            field_graphs: field_graphs.clone(),
            field_bodies: field_bodies.clone(),
            field_metadata: field_metadata.clone(),
            radiance_param_counts: radiance_param_counts.clone(),
            volume_param_counts: volume_param_counts.clone(),
            result_functions: result_functions.clone(),
            returns_result,
            type_info: type_info.cloned(),
            defers: Vec::new(),
            objective_stack: Vec::new(),
        }
    }

    fn current_objective(&self) -> Option<hir::Objective> {
        self.objective_stack.last().copied()
    }

    fn new_block(&mut self) -> BlockId {
        let id = BlockId(self.blocks.len());
        self.blocks.push(BasicBlock {
            stmts: Vec::new(),
            terminator: Terminator::Unreachable {
                span: TextRange::empty(0.into()),
            },
        });
        id
    }

    fn block_is_open(&self, block: BlockId) -> bool {
        matches!(
            self.blocks[block.0].terminator,
            Terminator::Unreachable { .. }
        )
    }

    fn set_terminator(&mut self, term: Terminator) {
        self.blocks[self.current_block.0].terminator = term;
    }

    fn push_stmt(&mut self, stmt: Stmt) {
        self.blocks[self.current_block.0].stmts.push(stmt);
    }

    fn local_type_for_name(&self, name: &SmolStr) -> MirType {
        self.type_info
            .as_ref()
            .and_then(|info| info.local_types.get(name))
            .map(mir_type_from_type)
            .unwrap_or(MirType::Unknown)
    }

    fn expr_type(&self, expr_id: hir::Idx<Expr>) -> MirType {
        self.type_info
            .as_ref()
            .and_then(|info| info.expr_types.get(&expr_id.into_raw()))
            .map(mir_type_from_type)
            .unwrap_or(MirType::Unknown)
    }

    fn proven_range_induction_type(lhs_ty: &MirType, rhs_ty: &MirType) -> Option<MirType> {
        match (lhs_ty, rhs_ty) {
            (MirType::Integer, MirType::Integer) => Some(MirType::Integer),
            (MirType::Float, MirType::Float) => Some(MirType::Float),
            _ => None,
        }
    }

    fn new_temp_for_expr(&mut self, expr_id: hir::Idx<Expr>) -> TempId {
        let ty = self.expr_type(expr_id);
        self.new_temp(ty)
    }

    fn new_temp(&mut self, ty: MirType) -> TempId {
        let id = TempId(self.temps.len());
        self.temps.push(Temp { ty });
        id
    }

    fn new_local(&mut self, name: SmolStr, mutable: bool, ty: MirType) -> LocalId {
        let id = LocalId(self.locals.len());
        self.locals.push(Local { name, mutable, ty });
        id
    }

    fn new_temp_local(&mut self) -> LocalId {
        let name = SmolStr::new(format!("$tmp{}", self.locals.len()));
        self.new_local(name, true, MirType::Unknown)
    }

    fn declare_local(&mut self, name: SmolStr, local: LocalId) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name, local);
        }
    }

    fn declare_resultness(&mut self, name: SmolStr, is_result: bool) {
        if let Some(scope) = self.result_scopes.last_mut() {
            scope.insert(name, is_result);
        }
    }

    fn set_resultness(&mut self, name: &SmolStr, is_result: bool) {
        for scope in self.result_scopes.iter_mut().rev() {
            if let Some(entry) = scope.get_mut(name) {
                *entry = is_result;
                return;
            }
        }
    }

    fn resolve_resultness(&self, name: &SmolStr) -> Option<bool> {
        for scope in self.result_scopes.iter().rev() {
            if let Some(result) = scope.get(name) {
                return Some(*result);
            }
        }
        None
    }

    fn resolve_local(&self, name: &SmolStr) -> Option<LocalId> {
        for scope in self.scopes.iter().rev() {
            if let Some(local) = scope.get(name) {
                return Some(*local);
            }
        }
        None
    }

    fn expr_is_result(&self, body: &hir::Body, expr_id: hir::Idx<Expr>) -> bool {
        match &body.exprs[expr_id] {
            Expr::Unary { op, .. } => matches!(op, UnaryOp::Await | UnaryOp::Err),
            Expr::Binary { .. } => false,
            Expr::Crash { .. } => false,
            Expr::Call { callee, .. } => {
                if let Expr::Variable(name) = &body.exprs[*callee] {
                    return self.result_functions.contains(name);
                }
                false
            }
            Expr::Variable(name) => self.resolve_resultness(name).unwrap_or(false),
            _ => false,
        }
    }

    fn enter_scope(&mut self) {
        self.scopes.push(HashMap::new());
        self.result_scopes.push(HashMap::new());
    }

    fn exit_scope(&mut self) {
        self.scopes.pop();
        self.result_scopes.pop();
    }

    fn lower_stmt_block(&mut self, body: &hir::Body, stmts: &[hir::Idx<HirStmt>]) {
        for stmt in stmts {
            if !self.block_is_open(self.current_block) {
                break;
            }
            self.lower_stmt(body, *stmt);
        }
    }

    fn lower_stmt(&mut self, body: &hir::Body, stmt_id: hir::Idx<HirStmt>) {
        let span = body.stmt_span(stmt_id);
        match &body.stmts[stmt_id] {
            HirStmt::Expr(expr) => {
                let _ = self.lower_expr(body, *expr);
            }
            HirStmt::Assert {
                kind,
                expr,
                rhs,
                tolerance,
            } => {
                let cond = self.lower_assert_expr(body, *expr, *rhs, *kind, *tolerance);
                let func = SmolStr::new("assert");
                let args = vec![cond, Value::Const(Literal::Nil)];
                let temp = self.new_temp(MirType::Nil);
                self.push_stmt(MirStmt::Assign {
                    place: Place::Temp(temp),
                    value: Rvalue::Call {
                        kind: CallKind::Sync,
                        target: CallTarget::Function(func),
                        args,
                    },
                    span,
                });
            }
            HirStmt::Require { condition, message } => {
                let cond = self.lower_expr(body, *condition);
                let msg = self.lower_expr(body, *message);
                let func = SmolStr::new("assert");
                let args = vec![cond, msg];
                let temp = self.new_temp(MirType::Nil);
                self.push_stmt(MirStmt::Assign {
                    place: Place::Temp(temp),
                    value: Rvalue::Call {
                        kind: CallKind::Sync,
                        target: CallTarget::Function(func),
                        args,
                    },
                    span,
                });
            }
            HirStmt::Let {
                name,
                value,
                mutable,
                ..
            } => {
                let is_result = self.expr_is_result(body, *value);
                let value = self.lower_expr(body, *value);
                let local = self.new_local(name.clone(), *mutable, self.local_type_for_name(name));
                self.declare_local(name.clone(), local);
                self.declare_resultness(name.clone(), is_result);
                self.push_stmt(MirStmt::Assign {
                    place: Place::Local(local),
                    value: Rvalue::Use(value),
                    span,
                });
            }
            HirStmt::Capture { name, value } => {
                let value = self.lower_expr(body, *value);
                let local = self.new_local(name.clone(), false, self.local_type_for_name(name));
                self.declare_local(name.clone(), local);
                self.declare_resultness(name.clone(), true);
                self.push_stmt(MirStmt::Assign {
                    place: Place::Local(local),
                    value: Rvalue::Use(value),
                    span,
                });
            }
            HirStmt::Assign {
                name, op, value, ..
            } => {
                let Some(local) = self.resolve_local(name) else {
                    return;
                };
                let is_result = self.expr_is_result(body, *value);
                let rhs = self.lower_expr(body, *value);
                self.set_resultness(name, is_result);
                match op {
                    AssignOp::Assign => {
                        self.push_stmt(MirStmt::Assign {
                            place: Place::Local(local),
                            value: Rvalue::Use(rhs),
                            span,
                        });
                    }
                    AssignOp::AddAssign
                    | AssignOp::SubAssign
                    | AssignOp::MulAssign
                    | AssignOp::DivAssign => {
                        let op = match op {
                            AssignOp::AddAssign => BinaryOp::Add,
                            AssignOp::SubAssign => BinaryOp::Sub,
                            AssignOp::MulAssign => BinaryOp::Mul,
                            AssignOp::DivAssign => BinaryOp::Div,
                            AssignOp::Assign => BinaryOp::Assign,
                        };
                        let temp = self.new_temp(MirType::Unknown);
                        self.push_stmt(MirStmt::Assign {
                            place: Place::Temp(temp),
                            value: Rvalue::Binary {
                                op,
                                lhs: Value::Local(local),
                                rhs,
                            },
                            span,
                        });
                        self.push_stmt(MirStmt::Assign {
                            place: Place::Local(local),
                            value: Rvalue::Use(Value::Temp(temp)),
                            span,
                        });
                    }
                }
            }
            HirStmt::IgnoreResult { expr } => {
                let _ = self.lower_expr(body, *expr);
            }
            HirStmt::Optimize {
                objective,
                body: optimize_body,
                ..
            } => {
                self.objective_stack.push(*objective);
                self.enter_scope();
                self.lower_stmt_block(body, optimize_body);
                self.exit_scope();
                let popped = self.objective_stack.pop();
                debug_assert_eq!(popped, Some(*objective));
            }
            HirStmt::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let cond = self.lower_expr(body, *condition);
                let then_block = self.new_block();
                let else_block = self.new_block();
                let join_block = self.new_block();
                self.set_terminator(Terminator::Branch {
                    cond,
                    then_target: then_block,
                    else_target: else_block,
                    span,
                });

                self.current_block = then_block;
                self.enter_scope();
                self.lower_stmt_block(body, then_branch);
                self.exit_scope();
                if self.block_is_open(self.current_block) {
                    self.set_terminator(Terminator::Jump {
                        target: join_block,
                        span,
                    });
                }

                self.current_block = else_block;
                if let Some(branch) = else_branch {
                    self.enter_scope();
                    self.lower_stmt_block(body, branch);
                    self.exit_scope();
                }
                if self.block_is_open(self.current_block) {
                    self.set_terminator(Terminator::Jump {
                        target: join_block,
                        span,
                    });
                }

                self.current_block = join_block;
            }
            HirStmt::While {
                condition,
                body: loop_body,
            } => {
                let head_block = self.new_block();
                let body_block = self.new_block();
                let exit_block = self.new_block();

                self.set_terminator(Terminator::Jump {
                    target: head_block,
                    span,
                });

                self.current_block = head_block;
                let cond = self.lower_expr(body, *condition);
                self.set_terminator(Terminator::Branch {
                    cond,
                    then_target: body_block,
                    else_target: exit_block,
                    span,
                });

                self.current_block = body_block;
                self.loop_stack.push(LoopTarget {
                    break_target: exit_block,
                    continue_target: head_block,
                });
                self.enter_scope();
                self.lower_stmt_block(body, loop_body);
                self.exit_scope();
                self.loop_stack.pop();
                if self.block_is_open(self.current_block) {
                    self.set_terminator(Terminator::Jump {
                        target: head_block,
                        span,
                    });
                }

                self.current_block = exit_block;
            }
            HirStmt::For {
                value_name,
                key_name,
                index_name,
                iterable,
                body: loop_body,
            } => {
                if let Expr::Binary {
                    lhs,
                    op: BinaryOp::Range,
                    rhs,
                    ..
                } = &body.exprs[*iterable]
                    && key_name.is_none()
                    && self
                        .lower_range_for(body, value_name, index_name, *lhs, *rhs, loop_body, span)
                {
                    return;
                }

                let iterable_value = self.lower_expr(body, *iterable);
                let iter_temp = self.new_temp(MirType::Unknown);
                self.push_stmt(MirStmt::IterInit {
                    dst: Place::Temp(iter_temp),
                    iterable: iterable_value.clone(),
                    span,
                });
                let iter_count_local = index_name.as_ref().map(|_| {
                    self.new_local(
                        SmolStr::new(format!("$iter_count{}", self.locals.len())),
                        true,
                        MirType::Integer,
                    )
                });
                if let Some(iter_count_local) = iter_count_local {
                    self.push_stmt(MirStmt::Assign {
                        place: Place::Local(iter_count_local),
                        value: Rvalue::Use(Value::Const(Literal::Integer(0))),
                        span,
                    });
                }

                let head_block = self.new_block();
                let body_block = self.new_block();
                let exit_block = self.new_block();

                self.set_terminator(Terminator::Jump {
                    target: head_block,
                    span,
                });

                self.current_block = head_block;
                let value_temp = self.new_temp(MirType::Unknown);
                let done_temp = self.new_temp(MirType::Boolean);
                self.push_stmt(MirStmt::IterNext {
                    iter: Value::Temp(iter_temp),
                    dst_value: Place::Temp(value_temp),
                    dst_done: Place::Temp(done_temp),
                    span,
                });
                self.set_terminator(Terminator::Branch {
                    cond: Value::Temp(done_temp),
                    then_target: exit_block,
                    else_target: body_block,
                    span,
                });

                self.current_block = body_block;
                self.enter_scope();
                if let Some(key_name) = key_name {
                    let key_local = self.new_local(key_name.clone(), false, MirType::Unknown);
                    self.declare_local(key_name.clone(), key_local);
                    self.push_stmt(MirStmt::Assign {
                        place: Place::Local(key_local),
                        value: Rvalue::Use(Value::Temp(value_temp)),
                        span,
                    });
                    let map_get_temp = self.new_temp(MirType::Unknown);
                    self.push_stmt(MirStmt::Assign {
                        place: Place::Temp(map_get_temp),
                        value: Rvalue::Call {
                            kind: CallKind::Sync,
                            target: CallTarget::Function(SmolStr::new("__wr_map_get")),
                            args: vec![iterable_value.clone(), Value::Local(key_local)],
                        },
                        span,
                    });
                    let value_local = self.new_local(value_name.clone(), false, MirType::Unknown);
                    self.declare_local(value_name.clone(), value_local);
                    self.push_stmt(MirStmt::Assign {
                        place: Place::Local(value_local),
                        value: Rvalue::Use(Value::Temp(map_get_temp)),
                        span,
                    });
                } else {
                    let local = self.new_local(value_name.clone(), false, MirType::Unknown);
                    self.declare_local(value_name.clone(), local);
                    self.push_stmt(MirStmt::Assign {
                        place: Place::Local(local),
                        value: Rvalue::Use(Value::Temp(value_temp)),
                        span,
                    });
                }
                if let Some(index_name) = index_name {
                    let index_local = self.new_local(index_name.clone(), false, MirType::Integer);
                    self.declare_local(index_name.clone(), index_local);
                    if let Some(iter_count_local) = iter_count_local {
                        self.push_stmt(MirStmt::Assign {
                            place: Place::Local(index_local),
                            value: Rvalue::Use(Value::Local(iter_count_local)),
                            span,
                        });
                    }
                }
                self.loop_stack.push(LoopTarget {
                    break_target: exit_block,
                    continue_target: head_block,
                });
                self.lower_stmt_block(body, loop_body);
                self.loop_stack.pop();
                self.exit_scope();
                if self.block_is_open(self.current_block) {
                    if let Some(iter_count_local) = iter_count_local {
                        let next_count = self.new_temp(MirType::Integer);
                        self.push_stmt(MirStmt::Assign {
                            place: Place::Temp(next_count),
                            value: Rvalue::Binary {
                                op: BinaryOp::Add,
                                lhs: Value::Local(iter_count_local),
                                rhs: Value::Const(Literal::Integer(1)),
                            },
                            span,
                        });
                        self.push_stmt(MirStmt::Assign {
                            place: Place::Local(iter_count_local),
                            value: Rvalue::Use(Value::Temp(next_count)),
                            span,
                        });
                    }
                    self.set_terminator(Terminator::Jump {
                        target: head_block,
                        span,
                    });
                }

                self.current_block = exit_block;
            }
            HirStmt::Match {
                subject,
                cases,
                otherwise,
            } => {
                let scrutinee = self.lower_expr(body, *subject);
                if self.match_has_result_patterns(cases) {
                    self.lower_result_match(body, span, scrutinee, cases, otherwise);
                } else {
                    let switch_block = self.current_block;
                    let join_block = self.new_block();
                    let default_block = self.new_block();
                    let default_case_idx = cases.iter().position(|case| {
                        case.labels
                            .iter()
                            .any(|label| self.is_default_match_pattern(label))
                    });
                    let mut switch_cases = Vec::new();

                    for (idx, case) in cases.iter().enumerate() {
                        if Some(idx) == default_case_idx {
                            continue;
                        }
                        let case_block = self.new_block();
                        for label in &case.labels {
                            if let Some(case_label) = self.lower_case_label(label) {
                                switch_cases.push((case_label, case_block));
                            }
                        }
                        self.current_block = case_block;
                        self.enter_scope();
                        if let Some(label) = case.labels.first() {
                            self.bind_pattern(body, label, scrutinee.clone(), span);
                        }
                        self.lower_stmt_block(body, &case.body);
                        self.exit_scope();
                        if self.block_is_open(self.current_block) {
                            self.set_terminator(Terminator::Jump {
                                target: join_block,
                                span,
                            });
                        }
                    }

                    self.current_block = default_block;
                    if let Some(idx) = default_case_idx {
                        let case = &cases[idx];
                        self.enter_scope();
                        if let Some(label) = case.labels.first() {
                            self.bind_pattern(body, label, scrutinee.clone(), span);
                        }
                        self.lower_stmt_block(body, &case.body);
                        self.exit_scope();
                    } else if let Some(branch) = otherwise {
                        self.enter_scope();
                        self.lower_stmt_block(body, branch);
                        self.exit_scope();
                    }
                    if self.block_is_open(self.current_block) {
                        self.set_terminator(Terminator::Jump {
                            target: join_block,
                            span,
                        });
                    }

                    self.current_block = switch_block;
                    self.set_terminator(Terminator::Switch {
                        scrutinee,
                        cases: switch_cases,
                        default: default_block,
                        span,
                    });

                    self.current_block = join_block;
                }
            }
            HirStmt::Use { .. } => {}
            HirStmt::Defer { expr } => {
                self.defers.push(*expr);
            }
            HirStmt::Return(expr) => {
                let value = match expr {
                    Some(expr_id) => {
                        let raw_value = self.lower_expr(body, *expr_id);
                        if self.returns_result && !self.expr_is_result(body, *expr_id) {
                            let temp = self.new_temp(MirType::Unknown);
                            self.push_stmt(MirStmt::Assign {
                                place: Place::Temp(temp),
                                value: Rvalue::ResultOk { value: raw_value },
                                span,
                            });
                            Some(Value::Temp(temp))
                        } else {
                            Some(raw_value)
                        }
                    }
                    None => {
                        if self.returns_result {
                            let temp = self.new_temp(MirType::Unknown);
                            self.push_stmt(MirStmt::Assign {
                                place: Place::Temp(temp),
                                value: Rvalue::ResultOk {
                                    value: Value::Const(Literal::Nil),
                                },
                                span,
                            });
                            Some(Value::Temp(temp))
                        } else {
                            None
                        }
                    }
                };
                self.emit_defers(body, span);
                self.set_terminator(Terminator::Return { value, span });
            }
            HirStmt::Break => {
                if let Some(target) = self.loop_stack.last() {
                    self.set_terminator(Terminator::Jump {
                        target: target.break_target,
                        span,
                    });
                }
            }
            HirStmt::Continue => {
                if let Some(target) = self.loop_stack.last() {
                    self.set_terminator(Terminator::Jump {
                        target: target.continue_target,
                        span,
                    });
                }
            }
        }
    }

    fn lower_range_for(
        &mut self,
        body: &hir::Body,
        value_name: &SmolStr,
        index_name: &Option<SmolStr>,
        lhs: hir::Idx<Expr>,
        rhs: hir::Idx<Expr>,
        loop_body: &[hir::Idx<hir::Stmt>],
        span: TextRange,
    ) -> bool {
        if env::var_os("WRELA_DISABLE_TYPED_RANGE_FASTPATH").is_some() {
            return false;
        }
        let lhs_ty = self.expr_type(lhs);
        let rhs_ty = self.expr_type(rhs);
        let Some(induction_ty) = Self::proven_range_induction_type(&lhs_ty, &rhs_ty) else {
            return false;
        };

        let start_val = self.lower_expr(body, lhs);
        let end_val = self.lower_expr(body, rhs);
        let constant_int_bounds = match (&body.exprs[lhs], &body.exprs[rhs]) {
            (
                Expr::Literal(hir::Literal::Integer(start)),
                Expr::Literal(hir::Literal::Integer(end)),
            ) if matches!(induction_ty, MirType::Integer) => Some((*start, *end)),
            _ => None,
        };

        if let Some((start, end)) = constant_int_bounds {
            let idx_local = self.new_local(
                SmolStr::new(format!("$range_idx{}", self.locals.len())),
                true,
                induction_ty.clone(),
            );
            let loop_var = self.new_local(value_name.clone(), false, induction_ty.clone());
            let loop_index = index_name
                .as_ref()
                .map(|name| self.new_local(name.clone(), false, MirType::Integer));
            let iter_count = self.new_local(
                SmolStr::new(format!("$range_count{}", self.locals.len())),
                true,
                MirType::Integer,
            );
            let head_block = self.new_block();
            let body_block = self.new_block();
            let exit_block = self.new_block();
            self.push_stmt(MirStmt::Assign {
                place: Place::Local(idx_local),
                value: Rvalue::Use(start_val.clone()),
                span,
            });
            self.push_stmt(MirStmt::Assign {
                place: Place::Local(iter_count),
                value: Rvalue::Use(Value::Const(Literal::Integer(0))),
                span,
            });
            self.set_terminator(Terminator::Jump {
                target: head_block,
                span,
            });

            self.current_block = head_block;
            let cond_temp = self.new_temp(MirType::Boolean);
            let cond_op = if start <= end {
                BinaryOp::Le
            } else {
                BinaryOp::Ge
            };
            self.push_stmt(MirStmt::Assign {
                place: Place::Temp(cond_temp),
                value: Rvalue::Binary {
                    op: cond_op,
                    lhs: Value::Local(idx_local),
                    rhs: end_val.clone(),
                },
                span,
            });
            self.set_terminator(Terminator::Branch {
                cond: Value::Temp(cond_temp),
                then_target: body_block,
                else_target: exit_block,
                span,
            });

            self.current_block = body_block;
            self.enter_scope();
            self.declare_local(value_name.clone(), loop_var);
            self.push_stmt(MirStmt::Assign {
                place: Place::Local(loop_var),
                value: Rvalue::Use(Value::Local(idx_local)),
                span,
            });
            if let Some(loop_index) = loop_index {
                if let Some(index_name) = index_name {
                    self.declare_local(index_name.clone(), loop_index);
                }
                self.push_stmt(MirStmt::Assign {
                    place: Place::Local(loop_index),
                    value: Rvalue::Use(Value::Local(iter_count)),
                    span,
                });
            }
            self.loop_stack.push(LoopTarget {
                break_target: exit_block,
                continue_target: head_block,
            });
            self.lower_stmt_block(body, loop_body);
            self.loop_stack.pop();
            self.exit_scope();
            if self.block_is_open(self.current_block) {
                let step_temp = self.new_temp(induction_ty);
                self.push_stmt(MirStmt::Assign {
                    place: Place::Temp(step_temp),
                    value: Rvalue::Binary {
                        op: BinaryOp::Add,
                        lhs: Value::Local(idx_local),
                        rhs: Value::Const(Literal::Integer(if start <= end { 1 } else { -1 })),
                    },
                    span,
                });
                self.push_stmt(MirStmt::Assign {
                    place: Place::Local(idx_local),
                    value: Rvalue::Use(Value::Temp(step_temp)),
                    span,
                });
                let count_temp = self.new_temp(MirType::Integer);
                self.push_stmt(MirStmt::Assign {
                    place: Place::Temp(count_temp),
                    value: Rvalue::Binary {
                        op: BinaryOp::Add,
                        lhs: Value::Local(iter_count),
                        rhs: Value::Const(Literal::Integer(1)),
                    },
                    span,
                });
                self.push_stmt(MirStmt::Assign {
                    place: Place::Local(iter_count),
                    value: Rvalue::Use(Value::Temp(count_temp)),
                    span,
                });
                self.set_terminator(Terminator::Jump {
                    target: head_block,
                    span,
                });
            }

            self.current_block = exit_block;
            return true;
        }

        let idx_local = self.new_local(
            SmolStr::new(format!("$range_idx{}", self.locals.len())),
            true,
            induction_ty.clone(),
        );
        let step_local = self.new_local(
            SmolStr::new(format!("$range_step{}", self.locals.len())),
            true,
            induction_ty.clone(),
        );
        let step_is_pos_local = self.new_local(
            SmolStr::new(format!("$range_pos{}", self.locals.len())),
            true,
            MirType::Boolean,
        );

        let loop_var = self.new_local(value_name.clone(), false, induction_ty.clone());
        let loop_index = index_name
            .as_ref()
            .map(|name| self.new_local(name.clone(), false, MirType::Integer));
        let iter_count = self.new_local(
            SmolStr::new(format!("$range_count{}", self.locals.len())),
            true,
            MirType::Integer,
        );

        let is_pos_temp = self.new_temp(MirType::Boolean);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(is_pos_temp),
            value: Rvalue::Binary {
                op: BinaryOp::Le,
                lhs: start_val.clone(),
                rhs: end_val.clone(),
            },
            span,
        });

        let asc_init = self.new_block();
        let desc_init = self.new_block();
        let head_block = self.new_block();
        let check_pos = self.new_block();
        let check_neg = self.new_block();
        let body_block = self.new_block();
        let exit_block = self.new_block();

        self.set_terminator(Terminator::Branch {
            cond: Value::Temp(is_pos_temp),
            then_target: asc_init,
            else_target: desc_init,
            span,
        });

        let step_value = if matches!(induction_ty, MirType::Float) {
            Value::Const(Literal::Float(1.0))
        } else {
            Value::Const(Literal::Integer(1))
        };
        let neg_step_value = if matches!(induction_ty, MirType::Float) {
            Value::Const(Literal::Float(-1.0))
        } else {
            Value::Const(Literal::Integer(-1))
        };

        self.current_block = asc_init;
        self.push_stmt(MirStmt::Assign {
            place: Place::Local(step_local),
            value: Rvalue::Use(step_value.clone()),
            span,
        });
        self.push_stmt(MirStmt::Assign {
            place: Place::Local(step_is_pos_local),
            value: Rvalue::Use(Value::Const(Literal::Boolean(true))),
            span,
        });
        self.push_stmt(MirStmt::Assign {
            place: Place::Local(idx_local),
            value: Rvalue::Use(start_val.clone()),
            span,
        });
        self.push_stmt(MirStmt::Assign {
            place: Place::Local(iter_count),
            value: Rvalue::Use(Value::Const(Literal::Integer(0))),
            span,
        });
        self.set_terminator(Terminator::Jump {
            target: head_block,
            span,
        });

        self.current_block = desc_init;
        self.push_stmt(MirStmt::Assign {
            place: Place::Local(step_local),
            value: Rvalue::Use(neg_step_value.clone()),
            span,
        });
        self.push_stmt(MirStmt::Assign {
            place: Place::Local(step_is_pos_local),
            value: Rvalue::Use(Value::Const(Literal::Boolean(false))),
            span,
        });
        self.push_stmt(MirStmt::Assign {
            place: Place::Local(idx_local),
            value: Rvalue::Use(start_val.clone()),
            span,
        });
        self.push_stmt(MirStmt::Assign {
            place: Place::Local(iter_count),
            value: Rvalue::Use(Value::Const(Literal::Integer(0))),
            span,
        });
        self.set_terminator(Terminator::Jump {
            target: head_block,
            span,
        });

        self.current_block = head_block;
        self.set_terminator(Terminator::Branch {
            cond: Value::Local(step_is_pos_local),
            then_target: check_pos,
            else_target: check_neg,
            span,
        });

        self.current_block = check_pos;
        let pos_cond = self.new_temp(MirType::Boolean);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(pos_cond),
            value: Rvalue::Binary {
                op: BinaryOp::Le,
                lhs: Value::Local(idx_local),
                rhs: end_val.clone(),
            },
            span,
        });
        self.set_terminator(Terminator::Branch {
            cond: Value::Temp(pos_cond),
            then_target: body_block,
            else_target: exit_block,
            span,
        });

        self.current_block = check_neg;
        let neg_cond = self.new_temp(MirType::Boolean);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(neg_cond),
            value: Rvalue::Binary {
                op: BinaryOp::Ge,
                lhs: Value::Local(idx_local),
                rhs: end_val.clone(),
            },
            span,
        });
        self.set_terminator(Terminator::Branch {
            cond: Value::Temp(neg_cond),
            then_target: body_block,
            else_target: exit_block,
            span,
        });

        self.current_block = body_block;
        self.enter_scope();
        self.declare_local(value_name.clone(), loop_var);
        self.push_stmt(MirStmt::Assign {
            place: Place::Local(loop_var),
            value: Rvalue::Use(Value::Local(idx_local)),
            span,
        });
        if let Some(loop_index) = loop_index {
            if let Some(index_name) = index_name {
                self.declare_local(index_name.clone(), loop_index);
            }
            self.push_stmt(MirStmt::Assign {
                place: Place::Local(loop_index),
                value: Rvalue::Use(Value::Local(iter_count)),
                span,
            });
        }
        self.loop_stack.push(LoopTarget {
            break_target: exit_block,
            continue_target: head_block,
        });
        self.lower_stmt_block(body, loop_body);
        self.loop_stack.pop();
        self.exit_scope();
        if self.block_is_open(self.current_block) {
            let step_temp = self.new_temp(induction_ty);
            self.push_stmt(MirStmt::Assign {
                place: Place::Temp(step_temp),
                value: Rvalue::Binary {
                    op: BinaryOp::Add,
                    lhs: Value::Local(idx_local),
                    rhs: Value::Local(step_local),
                },
                span,
            });
            self.push_stmt(MirStmt::Assign {
                place: Place::Local(idx_local),
                value: Rvalue::Use(Value::Temp(step_temp)),
                span,
            });
            let count_temp = self.new_temp(MirType::Integer);
            self.push_stmt(MirStmt::Assign {
                place: Place::Temp(count_temp),
                value: Rvalue::Binary {
                    op: BinaryOp::Add,
                    lhs: Value::Local(iter_count),
                    rhs: Value::Const(Literal::Integer(1)),
                },
                span,
            });
            self.push_stmt(MirStmt::Assign {
                place: Place::Local(iter_count),
                value: Rvalue::Use(Value::Temp(count_temp)),
                span,
            });
            self.set_terminator(Terminator::Jump {
                target: head_block,
                span,
            });
        }

        self.current_block = exit_block;
        true
    }

    fn lower_assert_expr(
        &mut self,
        body: &hir::Body,
        expr_id: hir::Idx<Expr>,
        rhs_id: Option<hir::Idx<Expr>>,
        kind: hir::AssertKind,
        tolerance: Option<hir::Idx<Expr>>,
    ) -> Value {
        if matches!(kind, hir::AssertKind::Approx) {
            let span = body.expr_span(expr_id);
            let left_id = match &body.exprs[expr_id] {
                Expr::Binary { lhs, .. } => *lhs,
                _ => expr_id,
            };
            let left = self.lower_expr(body, left_id);
            let right = rhs_id
                .map(|rhs| self.lower_expr(body, rhs))
                .unwrap_or(Value::Const(Literal::Nil));
            let tol = tolerance
                .map(|tol| self.lower_expr(body, tol))
                .unwrap_or(Value::Const(Literal::Nil));
            let temp = self.new_temp(MirType::Boolean);
            self.push_stmt(MirStmt::Assign {
                place: Place::Temp(temp),
                value: Rvalue::Call {
                    kind: CallKind::Sync,
                    target: CallTarget::Function(SmolStr::new("approx_eq")),
                    args: vec![left, right, tol],
                },
                span,
            });
            return Value::Temp(temp);
        }
        let span = body.expr_span(expr_id);
        if let Expr::Binary { lhs, op, rhs, .. } = &body.exprs[expr_id]
            && matches!(op, BinaryOp::Eq | BinaryOp::Ne)
        {
            let left = self.lower_expr(body, *lhs);
            let right = self.lower_expr(body, *rhs);
            let func = match kind {
                hir::AssertKind::Value => SmolStr::new("value_deep_eq"),
                hir::AssertKind::Identity => SmolStr::new("identity_eq"),
                hir::AssertKind::Approx => unreachable!(),
            };
            let temp = self.new_temp(MirType::Boolean);
            self.push_stmt(MirStmt::Assign {
                place: Place::Temp(temp),
                value: Rvalue::Call {
                    kind: CallKind::Sync,
                    target: CallTarget::Function(func),
                    args: vec![left, right],
                },
                span,
            });
            let mut result = Value::Temp(temp);
            if matches!(op, BinaryOp::Ne) {
                let not_temp = self.new_temp(MirType::Boolean);
                self.push_stmt(MirStmt::Assign {
                    place: Place::Temp(not_temp),
                    value: Rvalue::Unary {
                        op: UnaryOp::Not,
                        operand: result,
                    },
                    span,
                });
                result = Value::Temp(not_temp);
            }
            return result;
        }
        self.lower_expr(body, expr_id)
    }

    fn lower_case_label(&mut self, pattern: &hir::Pattern) -> Option<SwitchCase> {
        match pattern {
            hir::Pattern::Literal(lit) => Some(SwitchCase::Literal(lit.clone())),
            hir::Pattern::Binding(name) => {
                if let Some(tag) = self.type_tags.get(name).copied() {
                    return Some(SwitchCase::Type(tag));
                }
                if let Some(tag) = builtin_type_tag(name) {
                    return Some(SwitchCase::Type(tag));
                }
                None
            }
            hir::Pattern::Path { parts, args: _ } => {
                if parts.len() == 1 {
                    if let Some(tag) = self.type_tags.get(&parts[0]).copied() {
                        return Some(SwitchCase::Type(tag));
                    }
                    if let Some(tag) = builtin_type_tag(&parts[0]) {
                        return Some(SwitchCase::Type(tag));
                    }
                }
                if parts.len() == 2 {
                    let name = SmolStr::new(format!("{}.{}", parts[0], parts[1]));
                    return self.type_tags.get(&name).copied().map(SwitchCase::Type);
                }
                None
            }
            _ => None,
        }
    }

    fn match_has_result_patterns(&self, cases: &[hir::MatchCase]) -> bool {
        cases.iter().any(|case| {
            case.labels
                .iter()
                .any(|label| self.result_pattern_kind(label).is_some())
        })
    }

    fn result_pattern_kind(&self, pattern: &hir::Pattern) -> Option<bool> {
        if let hir::Pattern::Path { parts, .. } = pattern {
            if parts.len() == 1 && parts[0].as_str() == "Ok" {
                return Some(true);
            }
            if parts.len() == 1 && parts[0].as_str() == "Err" {
                return Some(false);
            }
        }
        None
    }

    fn lower_result_match(
        &mut self,
        body: &hir::Body,
        span: TextRange,
        scrutinee: Value,
        cases: &[hir::MatchCase],
        otherwise: &Option<Vec<hir::Idx<hir::Stmt>>>,
    ) {
        let join_block = self.new_block();
        let mut default_block = None;

        for case in cases {
            let case_block = self.new_block();
            let fallthrough_block = self.new_block();
            let label = case.labels.first();
            if let Some(label) = label {
                if let Some(is_ok) = self.result_pattern_kind(label) {
                    let is_ok_temp = self.new_temp(MirType::Boolean);
                    self.push_stmt(MirStmt::Assign {
                        place: Place::Temp(is_ok_temp),
                        value: Rvalue::ResultIsOk {
                            value: scrutinee.clone(),
                        },
                        span,
                    });
                    let mut cond_val = Value::Temp(is_ok_temp);
                    if !is_ok {
                        let not_temp = self.new_temp(MirType::Boolean);
                        self.push_stmt(MirStmt::Assign {
                            place: Place::Temp(not_temp),
                            value: Rvalue::Unary {
                                op: UnaryOp::Not,
                                operand: cond_val,
                            },
                            span,
                        });
                        cond_val = Value::Temp(not_temp);
                    }
                    self.set_terminator(Terminator::Branch {
                        cond: cond_val,
                        then_target: case_block,
                        else_target: fallthrough_block,
                        span,
                    });
                } else {
                    default_block = Some(case_block);
                    self.set_terminator(Terminator::Jump {
                        target: case_block,
                        span,
                    });
                }
            }

            self.current_block = case_block;
            self.enter_scope();
            if let Some(label) = label {
                self.bind_result_pattern(body, label, scrutinee.clone(), span);
            }
            self.lower_stmt_block(body, &case.body);
            self.exit_scope();
            if self.block_is_open(self.current_block) {
                self.set_terminator(Terminator::Jump {
                    target: join_block,
                    span,
                });
            }

            self.current_block = fallthrough_block;
        }

        if let Some(branch) = otherwise {
            let otherwise_block = self.new_block();
            self.set_terminator(Terminator::Jump {
                target: otherwise_block,
                span,
            });
            self.current_block = otherwise_block;
            self.enter_scope();
            self.lower_stmt_block(body, branch);
            self.exit_scope();
            if self.block_is_open(self.current_block) {
                self.set_terminator(Terminator::Jump {
                    target: join_block,
                    span,
                });
            }
        } else if let Some(default_block) = default_block {
            self.set_terminator(Terminator::Jump {
                target: default_block,
                span,
            });
        }

        self.current_block = join_block;
    }

    fn bind_pattern(
        &mut self,
        body: &hir::Body,
        pattern: &hir::Pattern,
        value: Value,
        span: TextRange,
    ) {
        match pattern {
            hir::Pattern::Wildcard | hir::Pattern::Literal(_) => {}
            hir::Pattern::Binding(name) => {
                let local = self.new_local(name.clone(), false, MirType::Unknown);
                self.declare_local(name.clone(), local);
                self.push_stmt(MirStmt::Assign {
                    place: Place::Local(local),
                    value: Rvalue::Use(value),
                    span,
                });
            }
            hir::Pattern::Path { parts, args } => {
                if parts.len() == 1 {
                    let class_name = parts[0].clone();
                    if let Some(fields) = self.class_fields.get(&class_name).cloned() {
                        for (idx, arg) in args.iter().enumerate() {
                            if let Some(field) = fields.get(idx) {
                                let temp = self.new_temp(MirType::Unknown);
                                self.push_stmt(MirStmt::Assign {
                                    place: Place::Temp(temp),
                                    value: Rvalue::GetField {
                                        base: value.clone(),
                                        field: field.clone(),
                                        slot: Some(idx as u32),
                                    },
                                    span,
                                });
                                self.bind_pattern(body, arg, Value::Temp(temp), span);
                            }
                        }
                        return;
                    }
                    if let Some(arg) = args.first() {
                        self.bind_pattern(body, arg, value, span);
                    }
                    return;
                }
                if parts.len() == 2 {
                    let class_name = SmolStr::new(format!("{}.{}", parts[0], parts[1]));
                    let Some(fields) = self.class_fields.get(&class_name).cloned() else {
                        return;
                    };
                    for (idx, arg) in args.iter().enumerate() {
                        if let Some(field) = fields.get(idx) {
                            let temp = self.new_temp(MirType::Unknown);
                            self.push_stmt(MirStmt::Assign {
                                place: Place::Temp(temp),
                                value: Rvalue::GetField {
                                    base: value.clone(),
                                    field: field.clone(),
                                    slot: Some(idx as u32),
                                },
                                span,
                            });
                            self.bind_pattern(body, arg, Value::Temp(temp), span);
                        }
                    }
                }
            }
            hir::Pattern::Struct {
                parts,
                fields: pattern_fields,
            } => {
                let class_name = if parts.len() == 1 {
                    Some(parts[0].clone())
                } else if parts.len() == 2 {
                    Some(SmolStr::new(format!("{}.{}", parts[0], parts[1])))
                } else {
                    None
                };
                let Some(class_name) = class_name else {
                    return;
                };
                let Some(fields) = self.class_fields.get(&class_name).cloned() else {
                    return;
                };
                for (field_name, field_pattern) in pattern_fields {
                    let Some(idx) = fields.iter().position(|f| f == field_name) else {
                        continue;
                    };
                    let temp = self.new_temp(MirType::Unknown);
                    self.push_stmt(MirStmt::Assign {
                        place: Place::Temp(temp),
                        value: Rvalue::GetField {
                            base: value.clone(),
                            field: field_name.clone(),
                            slot: Some(idx as u32),
                        },
                        span,
                    });
                    self.bind_pattern(body, field_pattern, Value::Temp(temp), span);
                }
            }
        }
    }

    fn bind_result_pattern(
        &mut self,
        body: &hir::Body,
        pattern: &hir::Pattern,
        value: Value,
        span: TextRange,
    ) {
        let Some(kind) = self.result_pattern_kind(pattern) else {
            return;
        };
        if let hir::Pattern::Path { args, .. } = pattern {
            if args.is_empty() {
                return;
            }
            let temp = self.new_temp(MirType::Unknown);
            let rvalue = if kind {
                Rvalue::ResultUnwrap { value }
            } else {
                Rvalue::ResultErrUnwrap { value }
            };
            self.push_stmt(MirStmt::Assign {
                place: Place::Temp(temp),
                value: rvalue,
                span,
            });
            self.bind_pattern(body, &args[0], Value::Temp(temp), span);
        }
    }

    fn lower_expr(&mut self, body: &hir::Body, expr_id: hir::Idx<Expr>) -> Value {
        let span = body.expr_span(expr_id);
        match &body.exprs[expr_id] {
            Expr::Literal(lit) => Value::Const(lit.clone()),
            Expr::Variable(name) => self
                .resolve_local(name)
                .map(Value::Local)
                .unwrap_or_else(|| Value::Const(Literal::Nil)),
            Expr::Detach {
                target,
                size,
                objective,
            } => self.lower_detach_expr(body, *target, *size, *objective, expr_id, span),
            Expr::Unary { op, expr, .. } => {
                if matches!(op, UnaryOp::Await) {
                    self.suspendable = true;
                }
                match op {
                    UnaryOp::Await => self.lower_await(body, *expr, span),
                    UnaryOp::Try => {
                        let result_val = self.lower_expr(body, *expr);
                        let ok_flag = self.new_temp(MirType::Boolean);
                        self.push_stmt(MirStmt::Assign {
                            place: Place::Temp(ok_flag),
                            value: Rvalue::ResultIsOk {
                                value: result_val.clone(),
                            },
                            span,
                        });

                        let then_block = self.new_block();
                        let else_block = self.new_block();
                        let join_block = self.new_block();
                        let result_local = self.new_temp_local();
                        self.push_stmt(MirStmt::Assign {
                            place: Place::Local(result_local),
                            value: Rvalue::Use(Value::Const(Literal::Nil)),
                            span,
                        });
                        self.set_terminator(Terminator::Branch {
                            cond: Value::Temp(ok_flag),
                            then_target: then_block,
                            else_target: else_block,
                            span,
                        });

                        self.current_block = then_block;
                        let ok_value = self.new_temp(MirType::Unknown);
                        self.push_stmt(MirStmt::Assign {
                            place: Place::Temp(ok_value),
                            value: Rvalue::ResultUnwrap {
                                value: result_val.clone(),
                            },
                            span,
                        });
                        self.push_stmt(MirStmt::Assign {
                            place: Place::Local(result_local),
                            value: Rvalue::Use(Value::Temp(ok_value)),
                            span,
                        });
                        if self.block_is_open(self.current_block) {
                            self.set_terminator(Terminator::Jump {
                                target: join_block,
                                span,
                            });
                        }

                        self.current_block = else_block;
                        self.emit_defers(body, span);
                        self.set_terminator(Terminator::Return {
                            value: Some(result_val),
                            span,
                        });

                        self.current_block = join_block;
                        Value::Local(result_local)
                    }
                    UnaryOp::Fire => {
                        if let Expr::Call { callee, args, .. } = &body.exprs[*expr] {
                            if self.is_actor_call(body, *callee) {
                                let (target, arg_values) =
                                    self.lower_call_target(body, *callee, args);
                                self.push_stmt(MirStmt::ActorFire {
                                    target,
                                    args: arg_values,
                                    span,
                                });
                                return Value::Const(Literal::Nil);
                            }
                        }
                        let pending = self.lower_pending_call_or_value(body, *expr, span);
                        self.push_stmt(MirStmt::Fire { pending, span });
                        Value::Const(Literal::Nil)
                    }
                    UnaryOp::Spawn => self.lower_detach_expr(
                        body,
                        *expr,
                        hir::PoolSize::Fixed(1),
                        None,
                        expr_id,
                        span,
                    ),
                    UnaryOp::Err => {
                        let operand = self.lower_expr(body, *expr);
                        let temp = self.new_temp_for_expr(expr_id);
                        self.push_stmt(MirStmt::Assign {
                            place: Place::Temp(temp),
                            value: Rvalue::ResultErr { value: operand },
                            span,
                        });
                        Value::Temp(temp)
                    }
                    _ => {
                        let operand = self.lower_expr(body, *expr);
                        let temp = self.new_temp_for_expr(expr_id);
                        self.push_stmt(MirStmt::Assign {
                            place: Place::Temp(temp),
                            value: Rvalue::Unary { op: *op, operand },
                            span,
                        });
                        Value::Temp(temp)
                    }
                }
            }
            Expr::TypeApply { callee, .. } => self.lower_expr(body, *callee),
            Expr::Binary { lhs, op, rhs, .. } => {
                if matches!(
                    op,
                    BinaryOp::Assign
                        | BinaryOp::AddAssign
                        | BinaryOp::SubAssign
                        | BinaryOp::MulAssign
                        | BinaryOp::DivAssign
                ) && let Expr::Index { object, index, .. } = &body.exprs[*lhs]
                {
                    let object_value = self.lower_expr(body, *object);
                    let index_value = self.lower_expr(body, *index);
                    let rhs_val = self.lower_expr(body, *rhs);
                    let set_name = match self.expr_type(*object) {
                        MirType::Named(name) if name.as_str() == "Map" => "__wr_map_set",
                        _ => "__wr_list_set",
                    };
                    let (new_val, args) = if *op == BinaryOp::Assign {
                        (
                            rhs_val.clone(),
                            vec![object_value, index_value, rhs_val.clone()],
                        )
                    } else {
                        let get_name = match self.expr_type(*object) {
                            MirType::Named(name) if name.as_str() == "Map" => "__wr_map_get",
                            _ => "__wr_list_get",
                        };
                        let current = self.new_temp(MirType::Unknown);
                        self.push_stmt(MirStmt::Assign {
                            place: Place::Temp(current),
                            value: Rvalue::Call {
                                kind: CallKind::Sync,
                                target: CallTarget::Function(SmolStr::new(get_name)),
                                args: vec![object_value.clone(), index_value.clone()],
                            },
                            span,
                        });
                        let bin_op = match op {
                            BinaryOp::AddAssign => BinaryOp::Add,
                            BinaryOp::SubAssign => BinaryOp::Sub,
                            BinaryOp::MulAssign => BinaryOp::Mul,
                            BinaryOp::DivAssign => BinaryOp::Div,
                            _ => BinaryOp::Assign,
                        };
                        let temp = self.new_temp(MirType::Unknown);
                        self.push_stmt(MirStmt::Assign {
                            place: Place::Temp(temp),
                            value: Rvalue::Binary {
                                op: bin_op,
                                lhs: Value::Temp(current),
                                rhs: rhs_val,
                            },
                            span,
                        });
                        let new_val = Value::Temp(temp);
                        (
                            new_val.clone(),
                            vec![object_value, index_value, new_val.clone()],
                        )
                    };
                    let ignored = self.new_temp(MirType::Unknown);
                    self.push_stmt(MirStmt::Assign {
                        place: Place::Temp(ignored),
                        value: Rvalue::Call {
                            kind: CallKind::Sync,
                            target: CallTarget::Function(SmolStr::new(set_name)),
                            args,
                        },
                        span,
                    });
                    return new_val;
                } else if matches!(
                    op,
                    BinaryOp::Assign
                        | BinaryOp::AddAssign
                        | BinaryOp::SubAssign
                        | BinaryOp::MulAssign
                        | BinaryOp::DivAssign
                ) && let Expr::Member { object, member, .. } = &body.exprs[*lhs]
                {
                    let slot = self.member_slot_hint(*object, member);
                    let base = self.lower_expr(body, *object);
                    let rhs_val = self.lower_expr(body, *rhs);
                    let new_val = if *op == BinaryOp::Assign {
                        rhs_val.clone()
                    } else {
                        let current = self.new_temp(MirType::Unknown);
                        self.push_stmt(MirStmt::Assign {
                            place: Place::Temp(current),
                            value: Rvalue::GetField {
                                base: base.clone(),
                                field: member.clone(),
                                slot,
                            },
                            span,
                        });
                        let bin_op = match op {
                            BinaryOp::AddAssign => BinaryOp::Add,
                            BinaryOp::SubAssign => BinaryOp::Sub,
                            BinaryOp::MulAssign => BinaryOp::Mul,
                            BinaryOp::DivAssign => BinaryOp::Div,
                            _ => BinaryOp::Assign,
                        };
                        let temp = self.new_temp(MirType::Unknown);
                        self.push_stmt(MirStmt::Assign {
                            place: Place::Temp(temp),
                            value: Rvalue::Binary {
                                op: bin_op,
                                lhs: Value::Temp(current),
                                rhs: rhs_val,
                            },
                            span,
                        });
                        Value::Temp(temp)
                    };
                    self.push_stmt(MirStmt::SetField {
                        base,
                        field: member.clone(),
                        slot,
                        value: new_val.clone(),
                        span,
                    });
                    return new_val;
                }
                if matches!(op, BinaryOp::Otherwise) {
                    let result_val = self.lower_expr(body, *lhs);
                    let ok_flag = self.new_temp(MirType::Boolean);
                    self.push_stmt(MirStmt::Assign {
                        place: Place::Temp(ok_flag),
                        value: Rvalue::ResultIsOk {
                            value: result_val.clone(),
                        },
                        span,
                    });

                    let then_block = self.new_block();
                    let else_block = self.new_block();
                    let join_block = self.new_block();
                    let result_local = self.new_temp_local();
                    self.push_stmt(MirStmt::Assign {
                        place: Place::Local(result_local),
                        value: Rvalue::Use(Value::Const(Literal::Nil)),
                        span,
                    });
                    self.set_terminator(Terminator::Branch {
                        cond: Value::Temp(ok_flag),
                        then_target: then_block,
                        else_target: else_block,
                        span,
                    });

                    self.current_block = then_block;
                    let ok_value = self.new_temp(MirType::Unknown);
                    self.push_stmt(MirStmt::Assign {
                        place: Place::Temp(ok_value),
                        value: Rvalue::ResultUnwrap { value: result_val },
                        span,
                    });
                    self.push_stmt(MirStmt::Assign {
                        place: Place::Local(result_local),
                        value: Rvalue::Use(Value::Temp(ok_value)),
                        span,
                    });
                    if self.block_is_open(self.current_block) {
                        self.set_terminator(Terminator::Jump {
                            target: join_block,
                            span,
                        });
                    }

                    self.current_block = else_block;
                    let handler_value = self.lower_expr(body, *rhs);
                    self.push_stmt(MirStmt::Assign {
                        place: Place::Local(result_local),
                        value: Rvalue::Use(handler_value),
                        span,
                    });
                    if self.block_is_open(self.current_block) {
                        self.set_terminator(Terminator::Jump {
                            target: join_block,
                            span,
                        });
                    }

                    self.current_block = join_block;
                    Value::Local(result_local)
                } else if matches!(op, BinaryOp::And | BinaryOp::Or) {
                    let lhs_val = self.lower_expr(body, *lhs);
                    let eval_block = self.new_block();
                    let short_block = self.new_block();
                    let join_block = self.new_block();

                    let (then_target, else_target) = if matches!(op, BinaryOp::And) {
                        (eval_block, short_block)
                    } else {
                        (short_block, eval_block)
                    };
                    let result_local = self.new_temp_local();
                    self.push_stmt(MirStmt::Assign {
                        place: Place::Local(result_local),
                        value: Rvalue::Use(Value::Const(Literal::Nil)),
                        span,
                    });

                    self.set_terminator(Terminator::Branch {
                        cond: lhs_val.clone(),
                        then_target,
                        else_target,
                        span,
                    });

                    self.current_block = short_block;
                    self.push_stmt(MirStmt::Assign {
                        place: Place::Local(result_local),
                        value: Rvalue::Use(lhs_val),
                        span,
                    });
                    if self.block_is_open(self.current_block) {
                        self.set_terminator(Terminator::Jump {
                            target: join_block,
                            span,
                        });
                    }

                    self.current_block = eval_block;
                    let rhs_val = self.lower_expr(body, *rhs);
                    self.push_stmt(MirStmt::Assign {
                        place: Place::Local(result_local),
                        value: Rvalue::Use(rhs_val),
                        span,
                    });
                    if self.block_is_open(self.current_block) {
                        self.set_terminator(Terminator::Jump {
                            target: join_block,
                            span,
                        });
                    }

                    self.current_block = join_block;
                    Value::Local(result_local)
                } else {
                    if matches!(op, BinaryOp::Add)
                        && (is_syntactic_stringish(body, *lhs)
                            || is_syntactic_stringish(body, *rhs))
                    {
                        let lhs = self.lower_expr(body, *lhs);
                        let rhs = self.lower_expr(body, *rhs);
                        let temp = self.new_temp_for_expr(expr_id);
                        self.push_stmt(MirStmt::Assign {
                            place: Place::Temp(temp),
                            value: Rvalue::StrConcat {
                                parts: vec![lhs, rhs],
                                // Escape analysis will refine this to LocalTemp when possible.
                                alloc: AllocKind::Escaping,
                            },
                            span,
                        });
                        return Value::Temp(temp);
                    }
                    let lhs = self.lower_expr(body, *lhs);
                    let rhs = self.lower_expr(body, *rhs);
                    let temp = self.new_temp_for_expr(expr_id);
                    self.push_stmt(MirStmt::Assign {
                        place: Place::Temp(temp),
                        value: Rvalue::Binary { op: *op, lhs, rhs },
                        span,
                    });
                    Value::Temp(temp)
                }
            }
            Expr::Crash { expr } => {
                let value = self.lower_expr(body, *expr);
                let temp = self.new_temp_for_expr(expr_id);
                self.push_stmt(MirStmt::Assign {
                    place: Place::Temp(temp),
                    value: Rvalue::Crash { value },
                    span,
                });
                Value::Temp(temp)
            }
            Expr::Member { object, member, .. } => {
                if let Some((class_name, class_id)) = self.resolve_class_init_target(body, expr_id)
                {
                    let fields = self
                        .class_fields
                        .get(&class_name)
                        .cloned()
                        .unwrap_or_default();
                    if fields.is_empty() {
                        let temp = self.new_temp_for_expr(expr_id);
                        self.push_stmt(MirStmt::Assign {
                            place: Place::Temp(temp),
                            value: Rvalue::ClassInit {
                                class_id: class_id.0 as u32,
                                fields,
                            },
                            span,
                        });
                        self.maybe_call_configure(&class_name, Value::Temp(temp), span);
                        return Value::Temp(temp);
                    }
                }
                if let Some(component_index) =
                    vector_component_index(self.expr_type(*object), member)
                {
                    let base = self.lower_expr(body, *object);
                    let temp = self.new_temp_for_expr(expr_id);
                    self.push_stmt(MirStmt::Assign {
                        place: Place::Temp(temp),
                        value: Rvalue::Call {
                            kind: CallKind::Sync,
                            target: CallTarget::Function(SmolStr::new("__wr_vec_component")),
                            args: vec![
                                base,
                                Value::Const(Literal::Integer(component_index as i64)),
                            ],
                        },
                        span,
                    });
                    return Value::Temp(temp);
                }
                let base = self.lower_expr(body, *object);
                let slot = self.member_slot_hint(*object, member);
                let temp = self.new_temp_for_expr(expr_id);
                self.push_stmt(MirStmt::Assign {
                    place: Place::Temp(temp),
                    value: Rvalue::GetField {
                        base,
                        field: member.clone(),
                        slot,
                    },
                    span,
                });
                Value::Temp(temp)
            }
            Expr::Index { object, index, .. } => {
                let object_value = self.lower_expr(body, *object);
                let index_value = self.lower_expr(body, *index);
                let target_name = match self.expr_type(*object) {
                    MirType::Named(name) if name.as_str() == "Map" => "__wr_map_get",
                    _ => "__wr_list_get",
                };
                let temp = self.new_temp_for_expr(expr_id);
                self.push_stmt(MirStmt::Assign {
                    place: Place::Temp(temp),
                    value: Rvalue::Call {
                        kind: CallKind::Sync,
                        target: CallTarget::Function(SmolStr::new(target_name)),
                        args: vec![object_value, index_value],
                    },
                    span,
                });
                Value::Temp(temp)
            }
            Expr::Call { callee, args, .. } => {
                if let Some(target) = self.parse_capture_builtin(body, expr_id) {
                    return self.build_scene_capture_value(&target, span);
                }
                if let Some(mode) = self.parse_dispatch_backend_builtin(body, expr_id) {
                    return self.build_dispatch_backend_value(mode, span);
                }
                if let Some(spec) = self.parse_field_query(body, expr_id) {
                    return self.lower_field_query_call(body, span, &spec);
                }
                if let Some(spec) = self.parse_shape_query(body, expr_id) {
                    return self.lower_shape_query_call(body, span, &spec);
                }
                if let Some(spec) = self.parse_world_point_query(body, expr_id) {
                    return self.lower_world_point_query_call(body, span, &spec);
                }
                if let Some(spec) = self.parse_world_shape_query(body, expr_id) {
                    return self.lower_world_shape_query_call(body, span, &spec);
                }
                if let Some(spec) = self.parse_field_batch_query(body, expr_id) {
                    return self.lower_field_batch_query_call(body, span, &spec);
                }
                if let Some(spec) = self.parse_shape_batch_query(body, expr_id) {
                    return self.lower_shape_batch_query_call(body, span, &spec);
                }
                if let Some(spec) = self.parse_dispatch_compute(body, expr_id) {
                    return self.lower_dispatch_compute_call(body, span, &spec);
                }
                if let Some((class_name, class_id)) = self.resolve_class_init_target(body, *callee)
                {
                    let fields = self
                        .class_fields
                        .get(&class_name)
                        .cloned()
                        .unwrap_or_default();
                    let field_defaults = self
                        .class_field_defaults
                        .get(&class_name)
                        .cloned()
                        .unwrap_or_else(|| vec![None; fields.len()]);
                    let mut field_values: Vec<Option<Value>> = vec![None; fields.len()];
                    let mut positional_index = 0usize;
                    for arg in args {
                        match arg {
                            hir::Arg::Positional { value, .. } => {
                                let lowered = self.lower_expr(body, *value);
                                if positional_index < field_values.len() {
                                    field_values[positional_index] = Some(lowered);
                                }
                                positional_index += 1;
                            }
                            hir::Arg::Named { name, value, .. } => {
                                let lowered = self.lower_expr(body, *value);
                                if let Some(idx) = fields.iter().position(|f| f == name) {
                                    field_values[idx] = Some(lowered);
                                }
                            }
                        }
                    }
                    let temp = self.new_temp_for_expr(expr_id);
                    self.push_stmt(MirStmt::Assign {
                        place: Place::Temp(temp),
                        value: Rvalue::ClassInit {
                            class_id: class_id.0 as u32,
                            fields,
                        },
                        span,
                    });
                    for idx in 0..field_values.len() {
                        if let Some(value) = field_values[idx].clone() {
                            self.push_stmt(MirStmt::SetField {
                                base: Value::Temp(temp),
                                field: self
                                    .class_fields
                                    .get(&class_name)
                                    .and_then(|fields| fields.get(idx).cloned())
                                    .unwrap_or_default(),
                                slot: Some(idx as u32),
                                value,
                                span,
                            });
                        }
                    }
                    for (idx, default) in field_defaults.iter().enumerate() {
                        if field_values.get(idx).and_then(|val| val.as_ref()).is_none()
                            && let Some(default) = default
                        {
                            let value = self.lower_field_default(default, span);
                            self.push_stmt(MirStmt::SetField {
                                base: Value::Temp(temp),
                                field: self
                                    .class_fields
                                    .get(&class_name)
                                    .and_then(|fields| fields.get(idx).cloned())
                                    .unwrap_or_default(),
                                slot: Some(idx as u32),
                                value,
                                span,
                            });
                        }
                    }
                    self.maybe_call_configure(&class_name, Value::Temp(temp), span);
                    return Value::Temp(temp);
                }
                let (target, args) = self.lower_call_target(body, *callee, args);
                let temp = self.new_temp_for_expr(expr_id);
                self.push_stmt(MirStmt::Assign {
                    place: Place::Temp(temp),
                    value: Rvalue::Call {
                        kind: CallKind::Sync,
                        target,
                        args,
                    },
                    span,
                });
                Value::Temp(temp)
            }
            Expr::List(items) => {
                let mut values = Vec::with_capacity(items.len());
                for item in items {
                    values.push(self.lower_expr(body, *item));
                }
                let temp = self.new_temp_for_expr(expr_id);
                self.push_stmt(MirStmt::Assign {
                    place: Place::Temp(temp),
                    value: Rvalue::BuildList {
                        items: values,
                        alloc: crate::mir::ir::AllocKind::LocalTemp,
                    },
                    span,
                });
                Value::Temp(temp)
            }
            Expr::Map(items) => {
                let mut values = Vec::with_capacity(items.len());
                for (key, value) in items {
                    let key_val = self.lower_expr(body, *key);
                    let value_val = self.lower_expr(body, *value);
                    values.push((key_val, value_val));
                }
                let temp = self.new_temp_for_expr(expr_id);
                self.push_stmt(MirStmt::Assign {
                    place: Place::Temp(temp),
                    value: Rvalue::BuildMap {
                        items: values,
                        alloc: crate::mir::ir::AllocKind::LocalTemp,
                    },
                    span,
                });
                Value::Temp(temp)
            }
            Expr::StringInterp(parts) => {
                let mut values = Vec::with_capacity(parts.len());
                for part in parts {
                    match part {
                        hir::StringPart::Literal(value) => {
                            values.push(StringPartValue::Literal(value.clone()));
                        }
                        hir::StringPart::Expr(expr) => {
                            let value = self.lower_expr(body, *expr);
                            values.push(StringPartValue::Value(value));
                        }
                    }
                }
                let temp = self.new_temp_for_expr(expr_id);
                self.push_stmt(MirStmt::Assign {
                    place: Place::Temp(temp),
                    value: Rvalue::StringInterp {
                        parts: values,
                        alloc: crate::mir::ir::AllocKind::LocalTemp,
                    },
                    span,
                });
                Value::Temp(temp)
            }
            Expr::Closure {
                body: closure_body, ..
            } => {
                // Lower the closure body expression; closures are not yet first-class
                // in the MIR, so we simply lower the body expression inline.
                self.lower_expr(body, *closure_body)
            }
        }
    }

    fn emit_defers(&mut self, body: &hir::Body, _span: TextRange) {
        let defers = self.defers.clone();
        for expr_id in defers.iter().rev() {
            let _ = self.lower_expr(body, *expr_id);
        }
    }

    fn lower_detach_expr(
        &mut self,
        body: &hir::Body,
        target_expr: hir::Idx<Expr>,
        size: hir::PoolSize,
        objective: Option<hir::Objective>,
        result_expr: hir::Idx<Expr>,
        span: TextRange,
    ) -> Value {
        let mut target_expr = target_expr;
        let mut size = size;
        // If the detach site didn't specify an objective, inherit it from the nearest
        // surrounding `optimize <objective>:` block.
        let mut objective = objective.or_else(|| self.current_objective());
        let mut config = SpawnConfig::default();
        let mut min_size = None;
        let mut max_size = None;
        let mut weight = None;
        let mut queue_cap: Option<i64> = None;
        if let Some(spec) = self.parse_pool_of(body, target_expr) {
            target_expr = spec.class_expr;
            if let Some(__wr_pool_size) = spec.size {
                size = __wr_pool_size;
            }
            if let Some(pool_objective) = spec.objective {
                objective = Some(pool_objective);
            }
            config = spec.config;
            min_size = spec.min_size;
            max_size = spec.max_size;
            weight = spec.weight;
            queue_cap = spec.queue_cap;
        }
        match size {
            hir::PoolSize::Fixed(count) => {
                if count > 1
                    && let Some(value) = self.lower_detach_pool_fixed(
                        body,
                        target_expr,
                        count as usize,
                        objective,
                        config,
                        min_size,
                        max_size,
                        weight,
                        queue_cap,
                        result_expr,
                        span,
                    )
                {
                    return value;
                }
            }
            hir::PoolSize::Auto => {
                if let Some(value) = self.lower_detach_pool_auto(
                    body,
                    target_expr,
                    objective,
                    config,
                    min_size,
                    max_size,
                    weight,
                    queue_cap,
                    result_expr,
                    span,
                ) {
                    return value;
                }
            }
        }
        let mut target = None;
        let mut instance = None;
        let mut lowered = None;
        match &body.exprs[target_expr] {
            Expr::Variable(name) => {
                if let Some(id) = self.type_tags.get(name).copied() {
                    target = Some(Value::Const(Literal::Integer(id.0 as i64)));
                    let fields = self.class_fields.get(name).cloned().unwrap_or_default();
                    let field_defaults = self
                        .class_field_defaults
                        .get(name)
                        .cloned()
                        .unwrap_or_else(|| vec![None; fields.len()]);
                    let temp = self.new_temp_for_expr(target_expr);
                    self.push_stmt(MirStmt::Assign {
                        place: Place::Temp(temp),
                        value: Rvalue::ClassInit {
                            class_id: id.0 as u32,
                            fields,
                        },
                        span,
                    });
                    for (idx, default) in field_defaults.iter().enumerate() {
                        if let Some(default) = default {
                            let value = self.lower_field_default(default, span);
                            self.push_stmt(MirStmt::SetField {
                                base: Value::Temp(temp),
                                field: self
                                    .class_fields
                                    .get(name)
                                    .and_then(|fields| fields.get(idx).cloned())
                                    .unwrap_or_default(),
                                slot: Some(idx as u32),
                                value,
                                span,
                            });
                        }
                    }
                    self.maybe_call_configure(name, Value::Temp(temp), span);
                    instance = Some(Value::Temp(temp));
                }
            }
            Expr::Call { callee, .. } => {
                let mut handled = false;
                if let Expr::Variable(name) = &body.exprs[*callee]
                    && let Some(id) = self.type_tags.get(name).copied()
                {
                    target = Some(Value::Const(Literal::Integer(id.0 as i64)));
                    // `detach` on actor classes should always have a concrete instance.
                    // Some actor-class "constructor" call shapes don't lower to a normal
                    // `ClassInit` expression here, so build the instance explicitly from
                    // class metadata (same strategy as Pool.of fast paths).
                    if let Some(class) = self.class_target_info(body, *callee) {
                        let value = self.build_class_instance(&class, span);
                        instance = Some(value.clone());
                        lowered = Some(value);
                        handled = true;
                    }
                }
                if !handled {
                    let value = self.lower_expr(body, target_expr);
                    instance = Some(value.clone());
                    lowered = Some(value);
                }
            }
            _ => {
                let value = self.lower_expr(body, target_expr);
                lowered = Some(value.clone());
                instance = Some(value);
            }
        }
        let target = target.unwrap_or_else(|| {
            lowered
                .clone()
                .unwrap_or_else(|| self.lower_expr(body, target_expr))
        });
        let instance = instance.unwrap_or(Value::Const(Literal::Nil));
        let objective = objective.unwrap_or(hir::Objective::Balance);
        let temp = self.new_temp_for_expr(result_expr);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(temp),
            value: Rvalue::Spawn {
                target,
                instance,
                size,
                objective,
                config,
            },
            span,
        });
        Value::Temp(temp)
    }

    fn lower_detach_pool_fixed(
        &mut self,
        body: &hir::Body,
        target_expr: hir::Idx<Expr>,
        count: usize,
        objective: Option<hir::Objective>,
        config: SpawnConfig,
        min_size: Option<i64>,
        max_size: Option<i64>,
        weight: Option<i64>,
        queue_cap: Option<i64>,
        result_expr: hir::Idx<Expr>,
        span: TextRange,
    ) -> Option<Value> {
        let class = self.class_target_info(body, target_expr)?;
        let objective = objective
            .or_else(|| self.current_objective())
            .unwrap_or(hir::Objective::Balance);
        let mut handles = Vec::with_capacity(count);
        for _ in 0..count {
            let instance = self.build_class_instance(&class, span);
            let target = Value::Const(Literal::Integer(class.class_id.0 as i64));
            let temp = self.new_temp_for_expr(result_expr);
            self.push_stmt(MirStmt::Assign {
                place: Place::Temp(temp),
                value: Rvalue::Spawn {
                    target,
                    instance,
                    size: hir::PoolSize::Fixed(1),
                    objective,
                    config,
                },
                span,
            });
            handles.push(Value::Temp(temp));
        }
        let list_temp = self.new_temp_for_expr(result_expr);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(list_temp),
            value: Rvalue::BuildList {
                items: handles,
                alloc: crate::mir::ir::AllocKind::LocalTemp,
            },
            span,
        });
        let pool_temp = self.new_temp_for_expr(result_expr);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(pool_temp),
            value: Rvalue::PoolNew {
                handles: Value::Temp(list_temp),
                objective,
                min_size: min_size.unwrap_or(0),
                max_size: max_size.unwrap_or(0),
                weight: weight.unwrap_or(0),
                queue_cap: queue_cap.unwrap_or(0),
            },
            span,
        });
        Some(Value::Temp(pool_temp))
    }

    fn lower_detach_pool_auto(
        &mut self,
        body: &hir::Body,
        target_expr: hir::Idx<Expr>,
        objective: Option<hir::Objective>,
        config: SpawnConfig,
        min_size: Option<i64>,
        max_size: Option<i64>,
        weight: Option<i64>,
        queue_cap: Option<i64>,
        result_expr: hir::Idx<Expr>,
        span: TextRange,
    ) -> Option<Value> {
        let class = self.class_target_info(body, target_expr)?;
        let objective = objective
            .or_else(|| self.current_objective())
            .unwrap_or(hir::Objective::Balance);
        let obj_code = objective_code(objective);
        let resolved_size = compile_time_auto_pool_size(
            obj_code,
            min_size.unwrap_or(0),
            max_size.unwrap_or(0),
            weight.unwrap_or(0),
        );

        let size_temp = self.new_temp(MirType::Integer);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(size_temp),
            value: Rvalue::Use(Value::Const(Literal::Integer(resolved_size))),
            span,
        });

        let list_temp = self.new_temp(MirType::Unknown);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(list_temp),
            value: Rvalue::BuildList {
                items: Vec::new(),
                alloc: crate::mir::ir::AllocKind::LocalTemp,
            },
            span,
        });

        let idx_local = self.new_temp_local();
        self.push_stmt(MirStmt::Assign {
            place: Place::Local(idx_local),
            value: Rvalue::Use(Value::Const(Literal::Integer(0))),
            span,
        });

        let head_block = self.new_block();
        let body_block = self.new_block();
        let exit_block = self.new_block();

        self.set_terminator(Terminator::Jump {
            target: head_block,
            span,
        });

        self.current_block = head_block;
        let cond_temp = self.new_temp(MirType::Boolean);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(cond_temp),
            value: Rvalue::Binary {
                op: BinaryOp::Lt,
                lhs: Value::Local(idx_local),
                rhs: Value::Temp(size_temp),
            },
            span,
        });
        self.set_terminator(Terminator::Branch {
            cond: Value::Temp(cond_temp),
            then_target: body_block,
            else_target: exit_block,
            span,
        });

        self.current_block = body_block;
        let instance = self.build_class_instance(&class, span);
        let target = Value::Const(Literal::Integer(class.class_id.0 as i64));
        let handle_temp = self.new_temp_for_expr(result_expr);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(handle_temp),
            value: Rvalue::Spawn {
                target,
                instance,
                size: hir::PoolSize::Fixed(1),
                objective,
                config,
            },
            span,
        });
        let push_temp = self.new_temp(MirType::Unknown);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(push_temp),
            value: Rvalue::Call {
                kind: CallKind::Sync,
                target: CallTarget::Function(SmolStr::new("__wr_list_push")),
                args: vec![Value::Temp(list_temp), Value::Temp(handle_temp)],
            },
            span,
        });

        let next_temp = self.new_temp(MirType::Integer);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(next_temp),
            value: Rvalue::Binary {
                op: BinaryOp::Add,
                lhs: Value::Local(idx_local),
                rhs: Value::Const(Literal::Integer(1)),
            },
            span,
        });
        self.push_stmt(MirStmt::Assign {
            place: Place::Local(idx_local),
            value: Rvalue::Use(Value::Temp(next_temp)),
            span,
        });
        if self.block_is_open(self.current_block) {
            self.set_terminator(Terminator::Jump {
                target: head_block,
                span,
            });
        }

        self.current_block = exit_block;
        let pool_temp = self.new_temp_for_expr(result_expr);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(pool_temp),
            value: Rvalue::PoolNew {
                handles: Value::Temp(list_temp),
                objective,
                min_size: min_size.unwrap_or(0),
                max_size: max_size.unwrap_or(0),
                weight: weight.unwrap_or(0),
                queue_cap: queue_cap.unwrap_or(0),
            },
            span,
        });
        Some(Value::Temp(pool_temp))
    }

    fn class_target_info(
        &mut self,
        body: &hir::Body,
        target_expr: hir::Idx<Expr>,
    ) -> Option<ClassTargetInfo> {
        match &body.exprs[target_expr] {
            Expr::Variable(name) => {
                let class_id = self.type_tags.get(name).copied()?;
                let fields = self.class_fields.get(name).cloned().unwrap_or_default();
                let field_defaults = self
                    .class_field_defaults
                    .get(name)
                    .cloned()
                    .unwrap_or_else(|| vec![None; fields.len()]);
                Some(ClassTargetInfo {
                    name: name.clone(),
                    class_id,
                    fields,
                    field_defaults,
                    field_values: Vec::new(),
                })
            }
            Expr::Call { callee, args, .. } => {
                let Expr::Variable(name) = &body.exprs[*callee] else {
                    return None;
                };
                let class_name = builtin_record_by_function(name.as_str())
                    .map(|record| SmolStr::new(record.name))
                    .unwrap_or_else(|| name.clone());
                let class_id = self.type_tags.get(&class_name).copied()?;
                let fields = self
                    .class_fields
                    .get(&class_name)
                    .cloned()
                    .unwrap_or_default();
                let field_defaults = self
                    .class_field_defaults
                    .get(&class_name)
                    .cloned()
                    .unwrap_or_else(|| vec![None; fields.len()]);
                let mut field_values: Vec<Option<Value>> = vec![None; fields.len()];
                let mut positional_index = 0usize;
                for arg in args {
                    match arg {
                        hir::Arg::Positional { value, .. } => {
                            let lowered = self.lower_expr(body, *value);
                            if positional_index < field_values.len() {
                                field_values[positional_index] = Some(lowered);
                            }
                            positional_index += 1;
                        }
                        hir::Arg::Named { name, value, .. } => {
                            let lowered = self.lower_expr(body, *value);
                            if let Some(idx) = fields.iter().position(|f| f == name) {
                                field_values[idx] = Some(lowered);
                            }
                        }
                    }
                }
                Some(ClassTargetInfo {
                    name: class_name,
                    class_id,
                    fields,
                    field_defaults,
                    field_values,
                })
            }
            _ => None,
        }
    }

    fn build_class_instance(&mut self, class: &ClassTargetInfo, span: TextRange) -> Value {
        let temp = self.new_temp(MirType::Unknown);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(temp),
            value: Rvalue::ClassInit {
                class_id: class.class_id.0 as u32,
                fields: class.fields.clone(),
            },
            span,
        });
        let has_explicit_fields = class.field_values.iter().any(|val| val.is_some());
        for (idx, value) in class.field_values.iter().enumerate() {
            if let Some(value) = value {
                self.push_stmt(MirStmt::SetField {
                    base: Value::Temp(temp),
                    field: class.fields.get(idx).cloned().unwrap_or_default(),
                    slot: Some(idx as u32),
                    value: value.clone(),
                    span,
                });
            }
        }
        for (idx, default) in class.field_defaults.iter().enumerate() {
            if class
                .field_values
                .get(idx)
                .and_then(|val| val.as_ref())
                .is_none()
                && let Some(default) = default
            {
                let value = self.lower_field_default(default, span);
                self.push_stmt(MirStmt::SetField {
                    base: Value::Temp(temp),
                    field: class.fields.get(idx).cloned().unwrap_or_default(),
                    slot: Some(idx as u32),
                    value,
                    span,
                });
            }
        }
        if has_explicit_fields {
            self.maybe_call_configure(&class.name, Value::Temp(temp), span);
        }
        Value::Temp(temp)
    }

    fn lower_field_default(&mut self, default: &hir::FieldDefault, span: TextRange) -> Value {
        match default {
            hir::FieldDefault::Literal(lit) => Value::Const(lit.clone()),
            hir::FieldDefault::List(items) => {
                let values = items
                    .iter()
                    .map(|item| self.lower_field_default(item, span))
                    .collect();
                let temp = self.new_temp(MirType::Unknown);
                self.push_stmt(MirStmt::Assign {
                    place: Place::Temp(temp),
                    value: Rvalue::BuildList {
                        items: values,
                        alloc: crate::mir::ir::AllocKind::LocalTemp,
                    },
                    span,
                });
                Value::Temp(temp)
            }
            hir::FieldDefault::Map(items) => {
                let values = items
                    .iter()
                    .map(|(key, value)| {
                        let key = self.lower_field_default(key, span);
                        let value = self.lower_field_default(value, span);
                        (key, value)
                    })
                    .collect();
                let temp = self.new_temp(MirType::Unknown);
                self.push_stmt(MirStmt::Assign {
                    place: Place::Temp(temp),
                    value: Rvalue::BuildMap {
                        items: values,
                        alloc: crate::mir::ir::AllocKind::LocalTemp,
                    },
                    span,
                });
                Value::Temp(temp)
            }
        }
    }

    fn maybe_call_configure(&mut self, class_name: &SmolStr, receiver: Value, span: TextRange) {
        if self.name == SmolStr::new(format!("{}.{}", class_name, "__configure__")) {
            return;
        }
        let method_id = match self
            .class_method_ids
            .get(class_name)
            .and_then(|methods| methods.get(&SmolStr::new("__configure__")))
        {
            Some(method_id) => *method_id,
            None => return,
        };
        let temp = self.new_temp(MirType::Unknown);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(temp),
            value: Rvalue::Call {
                kind: CallKind::Sync,
                target: CallTarget::Method {
                    receiver,
                    method: SmolStr::new(format!("{}.{}", class_name, "__configure__")),
                    method_id: Some(method_id),
                },
                args: Vec::new(),
            },
            span,
        });
    }

    fn parse_pool_of(&self, body: &hir::Body, expr_id: hir::Idx<Expr>) -> Option<PoolOfSpec> {
        let (callee, args) = match &body.exprs[expr_id] {
            Expr::Call { callee, args, .. } => (callee, args),
            _ => return None,
        };
        let Expr::Member { object, member, .. } = &body.exprs[*callee] else {
            return None;
        };
        if member.as_str() != "of" {
            return None;
        }
        if !matches!(&body.exprs[*object], Expr::Variable(name) if name.as_str() == "Pool") {
            return None;
        }
        let mut class_expr = None;
        let mut size = None;
        let mut objective = None;
        let mut config = SpawnConfig::default();
        let mut min_size = None;
        let mut max_size = None;
        let mut weight = None;
        let mut queue_cap: Option<i64> = None;
        for arg in args {
            match arg {
                hir::Arg::Positional { value, .. } => {
                    if class_expr.is_none() {
                        class_expr = Some(*value);
                    }
                }
                hir::Arg::Named { name, value, .. } => match name.as_str() {
                    "size" => {
                        if let Some(__wr_pool_size) = pool_size_from_expr(body, *value) {
                            size = Some(__wr_pool_size);
                        }
                    }
                    "objective" => {
                        if let Some(obj) = objective_from_expr(body, *value) {
                            objective = Some(obj);
                        }
                    }
                    "min" => {
                        min_size = int_literal_from_expr(body, *value);
                    }
                    "max" => {
                        max_size = int_literal_from_expr(body, *value);
                    }
                    "weight" => {
                        weight = int_literal_from_expr(body, *value);
                    }
                    "batch" => {
                        if let Some(limit) = batch_limit_from_expr(body, *value) {
                            config.batch_limit = Some(limit);
                        }
                    }
                    "backpressure" => {
                        if let Some(bp) = backpressure_from_expr(body, *value) {
                            config.mailbox_cap = bp.mailbox_cap;
                            config.enqueue_timeout_ms = bp.enqueue_timeout_ms;
                            queue_cap = bp.queue_cap;
                        }
                    }
                    _ => {}
                },
            }
        }
        class_expr.map(|expr| PoolOfSpec {
            class_expr: expr,
            size,
            objective,
            config,
            min_size,
            max_size,
            weight,
            queue_cap,
        })
    }

    fn parse_field_query(
        &self,
        body: &hir::Body,
        expr_id: hir::Idx<Expr>,
    ) -> Option<FieldQuerySpec> {
        let (callee, args) = match &body.exprs[expr_id] {
            Expr::Call { callee, args, .. } => (callee, args),
            _ => return None,
        };
        let Expr::Variable(name) = &body.exprs[*callee] else {
            return None;
        };
        let kind = match name.as_str() {
            "distance_at" => FieldQueryKind::Distance,
            "normal_at" => FieldQueryKind::Normal,
            "radiance_at" => FieldQueryKind::Radiance,
            "medium_at" => FieldQueryKind::Medium,
            _ => return None,
        };

        let mut capture = None;
        let mut point = None;
        let mut direction = None;
        for arg in args {
            let hir::Arg::Named { name, value, .. } = arg else {
                return None;
            };
            match name.as_str() {
                "capture" => capture = Some(*value),
                "point" => point = Some(*value),
                "direction" => direction = Some(*value),
                _ => return None,
            }
        }

        Some(FieldQuerySpec {
            kind,
            capture: capture?,
            point: point?,
            direction,
        })
    }

    fn parse_capture_builtin(&self, body: &hir::Body, expr_id: hir::Idx<Expr>) -> Option<SmolStr> {
        let (callee, args) = match &body.exprs[expr_id] {
            Expr::Call { callee, args, .. } => (callee, args),
            _ => return None,
        };
        let Expr::Variable(name) = &body.exprs[*callee] else {
            return None;
        };
        if name.as_str() != "capture" {
            return None;
        }
        let mut positional_target = None;
        for arg in args {
            match arg {
                hir::Arg::Named { name, value, .. } => {
                    if name.as_str() != "scene" {
                        continue;
                    }
                    let Expr::Variable(target) = &body.exprs[*value] else {
                        return None;
                    };
                    if self.shape_names.contains(target)
                        || self.field_names.contains(target)
                        || matches!(
                            self.expr_type(expr_id),
                            MirType::Named(ref name) if name.as_str() == "RegionCapture"
                        )
                    {
                        return Some(target.clone());
                    }
                    return None;
                }
                hir::Arg::Positional { value, .. } => {
                    positional_target = Some(*value);
                }
            };
        }
        if let Some(value) = positional_target {
            let Expr::Variable(target) = &body.exprs[value] else {
                return None;
            };
            if self.shape_names.contains(target)
                || self.field_names.contains(target)
                || matches!(
                    self.expr_type(expr_id),
                    MirType::Named(ref name) if name.as_str() == "RegionCapture"
                )
            {
                return Some(target.clone());
            }
        }
        None
    }

    fn parse_dispatch_backend_builtin(
        &self,
        body: &hir::Body,
        expr_id: hir::Idx<Expr>,
    ) -> Option<i64> {
        let (callee, args) = match &body.exprs[expr_id] {
            Expr::Call { callee, args, .. } => (callee, args),
            _ => return None,
        };
        if !args.is_empty() {
            return None;
        }
        let Expr::Variable(name) = &body.exprs[*callee] else {
            return None;
        };
        match name.as_str() {
            "dispatch_backend_cpu" => Some(0),
            "dispatch_backend_virtual_gpu" => Some(1),
            "dispatch_backend_auto" => Some(2),
            _ => None,
        }
    }

    fn parse_shape_query(
        &self,
        body: &hir::Body,
        expr_id: hir::Idx<Expr>,
    ) -> Option<ShapeQuerySpec> {
        let (callee, args) = match &body.exprs[expr_id] {
            Expr::Call { callee, args, .. } => (callee, args),
            _ => return None,
        };
        let Expr::Variable(name) = &body.exprs[*callee] else {
            return None;
        };
        let kind = match name.as_str() {
            "trace_shape" => ShapeQueryKind::Trace,
            "surface_at" => ShapeQueryKind::Surface,
            _ => return None,
        };

        let mut capture = None;
        let mut origin = None;
        let mut direction = None;
        let mut max_distance = None;
        let mut min_step = None;
        let mut hit_epsilon = None;
        let mut max_steps = None;
        let mut hit = None;

        for arg in args {
            let hir::Arg::Named { name, value, .. } = arg else {
                return None;
            };
            match name.as_str() {
                "capture" => capture = Some(*value),
                "origin" => origin = Some(*value),
                "direction" => direction = Some(*value),
                "max_distance" => max_distance = Some(*value),
                "min_step" => min_step = Some(*value),
                "hit_epsilon" => hit_epsilon = Some(*value),
                "max_steps" => max_steps = Some(*value),
                "hit" => hit = Some(*value),
                _ => return None,
            }
        }

        Some(ShapeQuerySpec {
            kind,
            capture: capture?,
            origin,
            direction,
            max_distance,
            min_step,
            hit_epsilon,
            max_steps,
            hit,
        })
    }

    fn parse_world_point_query(
        &self,
        body: &hir::Body,
        expr_id: hir::Idx<Expr>,
    ) -> Option<WorldPointQuerySpec> {
        let (callee, args) = match &body.exprs[expr_id] {
            Expr::Call { callee, args, .. } => (callee, args),
            _ => return None,
        };
        let Expr::Variable(name) = &body.exprs[*callee] else {
            return None;
        };
        let kind = match name.as_str() {
            "distance_world" => WorldPointQueryKind::Distance,
            "normal_world" => WorldPointQueryKind::Normal,
            "radiance_world" => WorldPointQueryKind::Radiance,
            "medium_world" => WorldPointQueryKind::Medium,
            _ => return None,
        };
        let mut capture = None;
        let mut domain = None;
        let mut point = None;
        let mut direction = None;
        for arg in args {
            let hir::Arg::Named { name, value, .. } = arg else {
                return None;
            };
            match name.as_str() {
                "capture" => capture = Some(*value),
                "domain" => domain = Some(*value),
                "point" => point = Some(*value),
                "direction" if matches!(kind, WorldPointQueryKind::Radiance) => {
                    direction = Some(*value)
                }
                _ => return None,
            }
        }
        Some(WorldPointQuerySpec {
            kind,
            capture: capture?,
            domain: domain?,
            point: point?,
            direction,
        })
    }

    fn parse_world_shape_query(
        &self,
        body: &hir::Body,
        expr_id: hir::Idx<Expr>,
    ) -> Option<WorldShapeQuerySpec> {
        let (callee, args) = match &body.exprs[expr_id] {
            Expr::Call { callee, args, .. } => (callee, args),
            _ => return None,
        };
        let Expr::Variable(name) = &body.exprs[*callee] else {
            return None;
        };
        let kind = match name.as_str() {
            "trace_world" => WorldShapeQueryKind::Trace,
            "surface_world" => WorldShapeQueryKind::Surface,
            _ => return None,
        };
        let mut capture = None;
        let mut domain = None;
        let mut origin = None;
        let mut direction = None;
        let mut max_distance = None;
        let mut min_step = None;
        let mut hit_epsilon = None;
        let mut max_steps = None;
        let mut hit = None;
        for arg in args {
            let hir::Arg::Named { name, value, .. } = arg else {
                return None;
            };
            match name.as_str() {
                "capture" => capture = Some(*value),
                "domain" => domain = Some(*value),
                "origin" => origin = Some(*value),
                "direction" => direction = Some(*value),
                "max_distance" => max_distance = Some(*value),
                "min_step" => min_step = Some(*value),
                "hit_epsilon" => hit_epsilon = Some(*value),
                "max_steps" => max_steps = Some(*value),
                "hit" => hit = Some(*value),
                _ => return None,
            }
        }
        Some(WorldShapeQuerySpec {
            kind,
            capture: capture?,
            domain: domain?,
            origin,
            direction,
            max_distance,
            min_step,
            hit_epsilon,
            max_steps,
            hit,
        })
    }

    fn parse_shape_batch_query(
        &self,
        body: &hir::Body,
        expr_id: hir::Idx<Expr>,
    ) -> Option<ShapeBatchQuerySpec> {
        let (callee, args) = match &body.exprs[expr_id] {
            Expr::Call { callee, args, .. } => (callee, args),
            _ => return None,
        };
        let Expr::Variable(name) = &body.exprs[*callee] else {
            return None;
        };
        let kind = match name.as_str() {
            "trace_shape_batch" => ShapeBatchQueryKind::Trace,
            "surface_at_batch" => ShapeBatchQueryKind::Surface,
            "occluded_batch" => ShapeBatchQueryKind::Occluded,
            _ => return None,
        };
        let mut capture = None;
        let mut items = None;
        let mut backend = None;
        for arg in args {
            let hir::Arg::Named { name, value, .. } = arg else {
                return None;
            };
            match name.as_str() {
                "capture" => capture = Some(*value),
                "rays"
                    if matches!(
                        kind,
                        ShapeBatchQueryKind::Trace | ShapeBatchQueryKind::Occluded
                    ) =>
                {
                    items = Some(*value)
                }
                "hits" if matches!(kind, ShapeBatchQueryKind::Surface) => items = Some(*value),
                "backend" => backend = Some(*value),
                _ => return None,
            }
        }
        Some(ShapeBatchQuerySpec {
            kind,
            capture: capture?,
            items: items?,
            backend: backend?,
        })
    }

    fn parse_field_batch_query(
        &self,
        body: &hir::Body,
        expr_id: hir::Idx<Expr>,
    ) -> Option<FieldBatchQuerySpec> {
        let (callee, args) = match &body.exprs[expr_id] {
            Expr::Call { callee, args, .. } => (callee, args),
            _ => return None,
        };
        let Expr::Variable(name) = &body.exprs[*callee] else {
            return None;
        };
        let kind = match name.as_str() {
            "distance_at_batch" => FieldBatchQueryKind::Distance,
            "normal_at_batch" => FieldBatchQueryKind::Normal,
            _ => return None,
        };
        let mut capture = None;
        let mut items = None;
        let mut backend = None;
        for arg in args {
            let hir::Arg::Named { name, value, .. } = arg else {
                return None;
            };
            match name.as_str() {
                "capture" => capture = Some(*value),
                "points" => items = Some(*value),
                "backend" => backend = Some(*value),
                _ => return None,
            }
        }
        Some(FieldBatchQuerySpec {
            kind,
            capture: capture?,
            items: items?,
            backend: backend?,
        })
    }

    fn lower_field_query_call(
        &mut self,
        body: &hir::Body,
        span: TextRange,
        spec: &FieldQuerySpec,
    ) -> Value {
        let capture = self.lower_expr(body, spec.capture);
        let point = self.lower_expr(body, spec.point);
        let kind = spec.kind;
        if matches!(kind, FieldQueryKind::Distance) {
            let helper_name = match self.expr_type(spec.capture) {
                MirType::Named(name) if name.as_str() == "ShapeCapture" => {
                    SmolStr::new("__wr_shape_distance_capture")
                }
                _ => SmolStr::new("__wr_field_distance_capture"),
            };
            self.lower_call_temp(MirType::Float, helper_name, vec![capture, point], span)
        } else if matches!(kind, FieldQueryKind::Normal) {
            let helper_name = match self.expr_type(spec.capture) {
                MirType::Named(name) if name.as_str() == "ShapeCapture" => {
                    SmolStr::new("__wr_shape_normal_capture")
                }
                _ => SmolStr::new("__wr_field_normal_capture"),
            };
            self.lower_call_temp(MirType::Vec3, helper_name, vec![capture, point], span)
        } else if matches!(kind, FieldQueryKind::Radiance) {
            let direction =
                self.lower_expr(body, spec.direction.expect("radiance_at missing direction"));
            self.lower_call_temp(
                MirType::Vec3,
                SmolStr::new("__wr_scene_radiance_capture"),
                vec![capture, point, direction],
                span,
            )
        } else {
            self.lower_call_temp(
                MirType::Named(SmolStr::new("Medium")),
                SmolStr::new("__wr_scene_medium_capture"),
                vec![capture, point],
                span,
            )
        }
    }

    fn lower_shape_query_call(
        &mut self,
        body: &hir::Body,
        span: TextRange,
        spec: &ShapeQuerySpec,
    ) -> Value {
        let capture = self.lower_expr(body, spec.capture);
        match spec.kind {
            ShapeQueryKind::Trace => {
                let origin =
                    self.lower_expr(body, spec.origin.expect("trace_shape missing origin"));
                let direction =
                    self.lower_expr(body, spec.direction.expect("trace_shape missing direction"));
                let max_distance = self.lower_expr(
                    body,
                    spec.max_distance.expect("trace_shape missing max_distance"),
                );
                let min_step =
                    self.lower_expr(body, spec.min_step.expect("trace_shape missing min_step"));
                let hit_epsilon = self.lower_expr(
                    body,
                    spec.hit_epsilon.expect("trace_shape missing hit_epsilon"),
                );
                let max_steps =
                    self.lower_expr(body, spec.max_steps.expect("trace_shape missing max_steps"));
                self.lower_call_temp(
                    MirType::Named(SmolStr::new("Hit3")),
                    SmolStr::new("__wr_scene_trace_capture"),
                    vec![
                        capture,
                        origin,
                        direction,
                        max_distance,
                        min_step,
                        hit_epsilon,
                        max_steps,
                    ],
                    span,
                )
            }
            ShapeQueryKind::Surface => {
                let hit = self.lower_expr(body, spec.hit.expect("surface_at missing hit"));
                self.lower_call_temp(
                    MirType::Named(SmolStr::new("Surface")),
                    SmolStr::new("__wr_scene_surface_capture"),
                    vec![capture, hit],
                    span,
                )
            }
        }
    }

    fn lower_world_point_query_call(
        &mut self,
        body: &hir::Body,
        span: TextRange,
        spec: &WorldPointQuerySpec,
    ) -> Value {
        let capture = self.lower_expr(body, spec.capture);
        let domain = self.lower_expr(body, spec.domain);
        let point = self.lower_expr(body, spec.point);
        match spec.kind {
            WorldPointQueryKind::Distance => self.lower_call_temp(
                MirType::Float,
                SmolStr::new("__wr_world_distance_capture"),
                vec![capture, domain, point],
                span,
            ),
            WorldPointQueryKind::Normal => self.lower_call_temp(
                MirType::Vec3,
                SmolStr::new("__wr_world_normal_capture"),
                vec![capture, domain, point],
                span,
            ),
            WorldPointQueryKind::Radiance => {
                let direction = self
                    .lower_expr(body, spec.direction.expect("radiance_world missing direction"));
                self.lower_call_temp(
                    MirType::Vec3,
                    SmolStr::new("__wr_world_radiance_capture"),
                    vec![capture, domain, point, direction],
                    span,
                )
            }
            WorldPointQueryKind::Medium => self.lower_call_temp(
                MirType::Named(SmolStr::new("Medium")),
                SmolStr::new("__wr_world_medium_capture"),
                vec![capture, domain, point],
                span,
            ),
        }
    }

    fn lower_world_shape_query_call(
        &mut self,
        body: &hir::Body,
        span: TextRange,
        spec: &WorldShapeQuerySpec,
    ) -> Value {
        let capture = self.lower_expr(body, spec.capture);
        let domain = self.lower_expr(body, spec.domain);
        match spec.kind {
            WorldShapeQueryKind::Trace => {
                let origin =
                    self.lower_expr(body, spec.origin.expect("trace_world missing origin"));
                let direction = self
                    .lower_expr(body, spec.direction.expect("trace_world missing direction"));
                let max_distance = self.lower_expr(
                    body,
                    spec.max_distance.expect("trace_world missing max_distance"),
                );
                let min_step =
                    self.lower_expr(body, spec.min_step.expect("trace_world missing min_step"));
                let hit_epsilon = self.lower_expr(
                    body,
                    spec.hit_epsilon.expect("trace_world missing hit_epsilon"),
                );
                let max_steps =
                    self.lower_expr(body, spec.max_steps.expect("trace_world missing max_steps"));
                self.lower_call_temp(
                    MirType::Named(SmolStr::new("Hit3")),
                    SmolStr::new("__wr_world_trace_capture"),
                    vec![
                        capture,
                        domain,
                        origin,
                        direction,
                        max_distance,
                        min_step,
                        hit_epsilon,
                        max_steps,
                    ],
                    span,
                )
            }
            WorldShapeQueryKind::Surface => {
                let hit = self.lower_expr(body, spec.hit.expect("surface_world missing hit"));
                self.lower_call_temp(
                    MirType::Named(SmolStr::new("Surface")),
                    SmolStr::new("__wr_world_surface_capture"),
                    vec![capture, domain, hit],
                    span,
                )
            }
        }
    }

    fn lower_shape_batch_query_call(
        &mut self,
        body: &hir::Body,
        span: TextRange,
        spec: &ShapeBatchQuerySpec,
    ) -> Value {
        let capture = self.lower_expr(body, spec.capture);
        let items = self.lower_expr(body, spec.items);
        let backend_value = self.lower_expr(body, spec.backend);
        let backend = self.lower_dispatch_backend_id(backend_value, span);
        let result = self.new_local(
            SmolStr::new(format!("$scene_batch_result{}", self.locals.len())),
            true,
            MirType::Named(SmolStr::new("List")),
        );
        self.push_stmt(MirStmt::Assign {
            place: Place::Local(result),
            value: Rvalue::BuildList {
                items: Vec::new(),
                alloc: AllocKind::Escaping,
            },
            span,
        });

        let cpu_block = self.new_block();
        let vgpu_block = self.new_block();
        let backend_check_block = self.new_block();
        let auto_check_block = self.new_block();
        let invalid_backend_block = self.new_block();
        let merge_block = self.new_block();
        let is_vgpu = self.lower_binary_temp(
            MirType::Boolean,
            BinaryOp::Eq,
            backend.clone(),
            Value::Const(Literal::Integer(1)),
            span,
        );
        self.set_terminator(Terminator::Branch {
            cond: is_vgpu,
            then_target: vgpu_block,
            else_target: backend_check_block,
            span,
        });

        self.current_block = backend_check_block;
        let is_cpu = self.lower_binary_temp(
            MirType::Boolean,
            BinaryOp::Eq,
            backend.clone(),
            Value::Const(Literal::Integer(0)),
            span,
        );
        let is_auto = self.lower_binary_temp(
            MirType::Boolean,
            BinaryOp::Eq,
            backend.clone(),
            Value::Const(Literal::Integer(2)),
            span,
        );
        let cpu_or_auto =
            self.lower_binary_temp(MirType::Boolean, BinaryOp::Or, is_cpu, is_auto, span);
        self.set_terminator(Terminator::Branch {
            cond: cpu_or_auto,
            then_target: auto_check_block,
            else_target: invalid_backend_block,
            span,
        });

        self.current_block = auto_check_block;
        let is_cpu = self.lower_binary_temp(
            MirType::Boolean,
            BinaryOp::Eq,
            backend.clone(),
            Value::Const(Literal::Integer(0)),
            span,
        );
        self.set_terminator(Terminator::Branch {
            cond: is_cpu,
            then_target: cpu_block,
            else_target: vgpu_block,
            span,
        });

        self.current_block = invalid_backend_block;
        let crash_temp = self.new_temp(MirType::Unknown);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(crash_temp),
            value: Rvalue::Crash {
                value: Value::Const(Literal::String(SmolStr::new(
                    "scene batch dispatch backend must be cpu, virtual_gpu, or auto",
                ))),
            },
            span,
        });
        self.set_terminator(Terminator::Return {
            value: Some(Value::Temp(crash_temp)),
            span,
        });

        self.current_block = cpu_block;
        match spec.kind {
            ShapeBatchQueryKind::Trace => self.lower_trace_shape_batch_loop(
                items.clone(),
                capture.clone(),
                result,
                span,
                false,
                merge_block,
            ),
            ShapeBatchQueryKind::Surface => self.lower_surface_batch_loop(
                items.clone(),
                capture.clone(),
                result,
                span,
                false,
                merge_block,
            ),
            ShapeBatchQueryKind::Occluded => self.lower_occluded_batch_loop(
                items.clone(),
                capture.clone(),
                result,
                span,
                false,
                merge_block,
            ),
        }

        self.current_block = vgpu_block;
        match spec.kind {
            ShapeBatchQueryKind::Trace => self.lower_trace_shape_batch_loop(
                items.clone(),
                capture.clone(),
                result,
                span,
                true,
                merge_block,
            ),
            ShapeBatchQueryKind::Surface => self.lower_surface_batch_loop(
                items.clone(),
                capture.clone(),
                result,
                span,
                true,
                merge_block,
            ),
            ShapeBatchQueryKind::Occluded => {
                self.lower_occluded_batch_loop(items, capture, result, span, true, merge_block)
            }
        }

        self.current_block = merge_block;
        Value::Local(result)
    }

    fn lower_field_batch_query_call(
        &mut self,
        body: &hir::Body,
        span: TextRange,
        spec: &FieldBatchQuerySpec,
    ) -> Value {
        let capture = self.lower_expr(body, spec.capture);
        let items = self.lower_expr(body, spec.items);
        let backend_value = self.lower_expr(body, spec.backend);
        let backend = self.lower_dispatch_backend_id(backend_value, span);
        let result = self.new_local(
            SmolStr::new(format!("$field_batch_result{}", self.locals.len())),
            true,
            MirType::Named(SmolStr::new("List")),
        );
        self.push_stmt(MirStmt::Assign {
            place: Place::Local(result),
            value: Rvalue::BuildList {
                items: Vec::new(),
                alloc: AllocKind::Escaping,
            },
            span,
        });

        let cpu_block = self.new_block();
        let vgpu_block = self.new_block();
        let backend_check_block = self.new_block();
        let auto_check_block = self.new_block();
        let invalid_backend_block = self.new_block();
        let merge_block = self.new_block();
        let is_vgpu = self.lower_binary_temp(
            MirType::Boolean,
            BinaryOp::Eq,
            backend.clone(),
            Value::Const(Literal::Integer(1)),
            span,
        );
        self.set_terminator(Terminator::Branch {
            cond: is_vgpu,
            then_target: vgpu_block,
            else_target: backend_check_block,
            span,
        });

        self.current_block = backend_check_block;
        let is_cpu = self.lower_binary_temp(
            MirType::Boolean,
            BinaryOp::Eq,
            backend.clone(),
            Value::Const(Literal::Integer(0)),
            span,
        );
        let is_auto = self.lower_binary_temp(
            MirType::Boolean,
            BinaryOp::Eq,
            backend.clone(),
            Value::Const(Literal::Integer(2)),
            span,
        );
        let cpu_or_auto =
            self.lower_binary_temp(MirType::Boolean, BinaryOp::Or, is_cpu, is_auto, span);
        self.set_terminator(Terminator::Branch {
            cond: cpu_or_auto,
            then_target: auto_check_block,
            else_target: invalid_backend_block,
            span,
        });

        self.current_block = auto_check_block;
        let is_cpu = self.lower_binary_temp(
            MirType::Boolean,
            BinaryOp::Eq,
            backend.clone(),
            Value::Const(Literal::Integer(0)),
            span,
        );
        self.set_terminator(Terminator::Branch {
            cond: is_cpu,
            then_target: cpu_block,
            else_target: vgpu_block,
            span,
        });

        self.current_block = invalid_backend_block;
        let crash_temp = self.new_temp(MirType::Unknown);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(crash_temp),
            value: Rvalue::Crash {
                value: Value::Const(Literal::String(SmolStr::new(
                    "scene batch dispatch backend must be cpu, virtual_gpu, or auto",
                ))),
            },
            span,
        });
        self.set_terminator(Terminator::Return {
            value: Some(Value::Temp(crash_temp)),
            span,
        });

        self.current_block = cpu_block;
        self.lower_field_sample_batch_loop(
            items.clone(),
            capture.clone(),
            result,
            span,
            false,
            merge_block,
            spec.kind,
            matches!(self.expr_type(spec.capture), MirType::Named(name) if name.as_str() == "ShapeCapture"),
        );

        self.current_block = vgpu_block;
        self.lower_field_sample_batch_loop(
            items,
            capture,
            result,
            span,
            true,
            merge_block,
            spec.kind,
            matches!(self.expr_type(spec.capture), MirType::Named(name) if name.as_str() == "ShapeCapture"),
        );

        self.current_block = merge_block;
        Value::Local(result)
    }

    fn lower_trace_shape_batch_loop(
        &mut self,
        items: Value,
        capture: Value,
        result_local: LocalId,
        span: TextRange,
        use_virtual_gpu: bool,
        merge_block: BlockId,
    ) {
        let len = self.lower_call_temp(
            MirType::Integer,
            SmolStr::new("__wr_list_len"),
            vec![items.clone()],
            span,
        );
        if use_virtual_gpu {
            let _ = self.lower_call_temp(
                MirType::Nil,
                SmolStr::new("__wr_gpu_dispatch_begin"),
                vec![
                    len.clone(),
                    Value::Const(Literal::Integer(1)),
                    Value::Const(Literal::Integer(1)),
                    Value::Const(Literal::Integer(1)),
                    Value::Const(Literal::Integer(1)),
                    Value::Const(Literal::Integer(1)),
                    Value::Const(Literal::Nil),
                ],
                span,
            );
        }
        let index = self.new_local(
            SmolStr::new(format!("$trace_batch_index{}", self.locals.len())),
            true,
            MirType::Integer,
        );
        self.assign_use(Place::Local(index), Value::Const(Literal::Integer(0)), span);
        let head = self.new_block();
        let body_block = self.new_block();
        let exit = self.new_block();
        self.set_terminator(Terminator::Jump { target: head, span });
        self.current_block = head;
        let cond = self.lower_binary_temp(
            MirType::Boolean,
            BinaryOp::Lt,
            Value::Local(index),
            len,
            span,
        );
        self.set_terminator(Terminator::Branch {
            cond,
            then_target: body_block,
            else_target: exit,
            span,
        });
        self.current_block = body_block;
        if use_virtual_gpu {
            let _ = self.lower_call_temp(
                MirType::Nil,
                SmolStr::new("__wr_gpu_dispatch_select_invocation"),
                vec![Value::Local(index)],
                span,
            );
        }
        let ray = self.lower_call_temp(
            MirType::Named(SmolStr::new("RayQuery")),
            SmolStr::new("__wr_list_get"),
            vec![items.clone(), Value::Local(index)],
            span,
        );
        let origin = self.new_temp(MirType::Vec3);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(origin),
            value: Rvalue::GetField {
                base: ray.clone(),
                field: SmolStr::new("origin"),
                slot: self.field_slot("RayQuery", "origin"),
            },
            span,
        });
        let direction = self.new_temp(MirType::Vec3);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(direction),
            value: Rvalue::GetField {
                base: ray.clone(),
                field: SmolStr::new("direction"),
                slot: self.field_slot("RayQuery", "direction"),
            },
            span,
        });
        let max_distance = self.new_temp(MirType::Float);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(max_distance),
            value: Rvalue::GetField {
                base: ray.clone(),
                field: SmolStr::new("max_distance"),
                slot: self.field_slot("RayQuery", "max_distance"),
            },
            span,
        });
        let min_step = self.new_temp(MirType::Float);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(min_step),
            value: Rvalue::GetField {
                base: ray.clone(),
                field: SmolStr::new("min_step"),
                slot: self.field_slot("RayQuery", "min_step"),
            },
            span,
        });
        let hit_epsilon = self.new_temp(MirType::Float);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(hit_epsilon),
            value: Rvalue::GetField {
                base: ray.clone(),
                field: SmolStr::new("hit_epsilon"),
                slot: self.field_slot("RayQuery", "hit_epsilon"),
            },
            span,
        });
        let max_steps = self.new_temp(MirType::Integer);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(max_steps),
            value: Rvalue::GetField {
                base: ray,
                field: SmolStr::new("max_steps"),
                slot: self.field_slot("RayQuery", "max_steps"),
            },
            span,
        });
        let hit = self.lower_call_temp(
            MirType::Named(SmolStr::new("Hit3")),
            SmolStr::new("__wr_scene_trace_capture"),
            vec![
                capture.clone(),
                Value::Temp(origin),
                Value::Temp(direction),
                Value::Temp(max_distance),
                Value::Temp(min_step),
                Value::Temp(hit_epsilon),
                Value::Temp(max_steps),
            ],
            span,
        );
        let _ = self.lower_call_temp(
            MirType::Nil,
            SmolStr::new("__wr_list_push"),
            vec![Value::Local(result_local), hit],
            span,
        );
        let next = self.lower_binary_temp(
            MirType::Integer,
            BinaryOp::Add,
            Value::Local(index),
            Value::Const(Literal::Integer(1)),
            span,
        );
        self.assign_use(Place::Local(index), next, span);
        self.set_terminator(Terminator::Jump { target: head, span });
        self.current_block = exit;
        if use_virtual_gpu {
            let _ = self.lower_call_temp(
                MirType::Nil,
                SmolStr::new("__wr_gpu_dispatch_end"),
                Vec::new(),
                span,
            );
        }
        self.set_terminator(Terminator::Jump {
            target: merge_block,
            span,
        });
    }

    fn lower_surface_batch_loop(
        &mut self,
        items: Value,
        capture: Value,
        result_local: LocalId,
        span: TextRange,
        use_virtual_gpu: bool,
        merge_block: BlockId,
    ) {
        let len = self.lower_call_temp(
            MirType::Integer,
            SmolStr::new("__wr_list_len"),
            vec![items.clone()],
            span,
        );
        if use_virtual_gpu {
            let _ = self.lower_call_temp(
                MirType::Nil,
                SmolStr::new("__wr_gpu_dispatch_begin"),
                vec![
                    len.clone(),
                    Value::Const(Literal::Integer(1)),
                    Value::Const(Literal::Integer(1)),
                    Value::Const(Literal::Integer(1)),
                    Value::Const(Literal::Integer(1)),
                    Value::Const(Literal::Integer(1)),
                    Value::Const(Literal::Nil),
                ],
                span,
            );
        }
        let index = self.new_local(
            SmolStr::new(format!("$surface_batch_index{}", self.locals.len())),
            true,
            MirType::Integer,
        );
        self.assign_use(Place::Local(index), Value::Const(Literal::Integer(0)), span);
        let head = self.new_block();
        let body_block = self.new_block();
        let exit = self.new_block();
        self.set_terminator(Terminator::Jump { target: head, span });
        self.current_block = head;
        let cond = self.lower_binary_temp(
            MirType::Boolean,
            BinaryOp::Lt,
            Value::Local(index),
            len,
            span,
        );
        self.set_terminator(Terminator::Branch {
            cond,
            then_target: body_block,
            else_target: exit,
            span,
        });
        self.current_block = body_block;
        if use_virtual_gpu {
            let _ = self.lower_call_temp(
                MirType::Nil,
                SmolStr::new("__wr_gpu_dispatch_select_invocation"),
                vec![Value::Local(index)],
                span,
            );
        }
        let hit = self.lower_call_temp(
            MirType::Named(SmolStr::new("Hit3")),
            SmolStr::new("__wr_list_get"),
            vec![items.clone(), Value::Local(index)],
            span,
        );
        let surface = self.lower_call_temp(
            MirType::Named(SmolStr::new("Surface")),
            SmolStr::new("__wr_scene_surface_capture"),
            vec![capture.clone(), hit],
            span,
        );
        let _ = self.lower_call_temp(
            MirType::Nil,
            SmolStr::new("__wr_list_push"),
            vec![Value::Local(result_local), surface],
            span,
        );
        let next = self.lower_binary_temp(
            MirType::Integer,
            BinaryOp::Add,
            Value::Local(index),
            Value::Const(Literal::Integer(1)),
            span,
        );
        self.assign_use(Place::Local(index), next, span);
        self.set_terminator(Terminator::Jump { target: head, span });
        self.current_block = exit;
        if use_virtual_gpu {
            let _ = self.lower_call_temp(
                MirType::Nil,
                SmolStr::new("__wr_gpu_dispatch_end"),
                Vec::new(),
                span,
            );
        }
        self.set_terminator(Terminator::Jump {
            target: merge_block,
            span,
        });
    }

    fn lower_field_sample_batch_loop(
        &mut self,
        items: Value,
        capture: Value,
        result_local: LocalId,
        span: TextRange,
        use_virtual_gpu: bool,
        merge_block: BlockId,
        kind: FieldBatchQueryKind,
        capture_is_shape: bool,
    ) {
        let len = self.lower_call_temp(
            MirType::Integer,
            SmolStr::new("__wr_list_len"),
            vec![items.clone()],
            span,
        );
        if use_virtual_gpu {
            let _ = self.lower_call_temp(
                MirType::Nil,
                SmolStr::new("__wr_gpu_dispatch_begin"),
                vec![
                    len.clone(),
                    Value::Const(Literal::Integer(1)),
                    Value::Const(Literal::Integer(1)),
                    Value::Const(Literal::Integer(1)),
                    Value::Const(Literal::Integer(1)),
                    Value::Const(Literal::Integer(1)),
                    Value::Const(Literal::Nil),
                ],
                span,
            );
        }
        let index = self.new_local(
            SmolStr::new(format!("$field_batch_index{}", self.locals.len())),
            true,
            MirType::Integer,
        );
        self.assign_use(Place::Local(index), Value::Const(Literal::Integer(0)), span);
        let head = self.new_block();
        let body_block = self.new_block();
        let exit = self.new_block();
        self.set_terminator(Terminator::Jump { target: head, span });
        self.current_block = head;
        let cond = self.lower_binary_temp(
            MirType::Boolean,
            BinaryOp::Lt,
            Value::Local(index),
            len,
            span,
        );
        self.set_terminator(Terminator::Branch {
            cond,
            then_target: body_block,
            else_target: exit,
            span,
        });
        self.current_block = body_block;
        if use_virtual_gpu {
            let _ = self.lower_call_temp(
                MirType::Nil,
                SmolStr::new("__wr_gpu_dispatch_select_invocation"),
                vec![Value::Local(index)],
                span,
            );
        }
        let point_query = self.lower_call_temp(
            MirType::Named(SmolStr::new("PointQuery")),
            SmolStr::new("__wr_list_get"),
            vec![items.clone(), Value::Local(index)],
            span,
        );
        let point = self.new_temp(MirType::Vec3);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(point),
            value: Rvalue::GetField {
                base: point_query,
                field: SmolStr::new("point"),
                slot: self.field_slot("PointQuery", "point"),
            },
            span,
        });
        let result_value = match kind {
            FieldBatchQueryKind::Distance => {
                let distance = self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new(if capture_is_shape {
                        "__wr_shape_distance_capture"
                    } else {
                        "__wr_field_distance_capture"
                    }),
                    vec![capture.clone(), Value::Temp(point)],
                    span,
                );
                self.build_distance_result_value(distance, span)
            }
            FieldBatchQueryKind::Normal => {
                let normal = self.lower_call_temp(
                    MirType::Vec3,
                    SmolStr::new(if capture_is_shape {
                        "__wr_shape_normal_capture"
                    } else {
                        "__wr_field_normal_capture"
                    }),
                    vec![capture.clone(), Value::Temp(point)],
                    span,
                );
                self.build_normal_result_value(normal, span)
            }
        };
        let _ = self.lower_call_temp(
            MirType::Nil,
            SmolStr::new("__wr_list_push"),
            vec![Value::Local(result_local), result_value],
            span,
        );
        let next = self.lower_binary_temp(
            MirType::Integer,
            BinaryOp::Add,
            Value::Local(index),
            Value::Const(Literal::Integer(1)),
            span,
        );
        self.assign_use(Place::Local(index), next, span);
        self.set_terminator(Terminator::Jump { target: head, span });
        self.current_block = exit;
        if use_virtual_gpu {
            let _ = self.lower_call_temp(
                MirType::Nil,
                SmolStr::new("__wr_gpu_dispatch_end"),
                Vec::new(),
                span,
            );
        }
        self.set_terminator(Terminator::Jump {
            target: merge_block,
            span,
        });
    }

    fn lower_occluded_batch_loop(
        &mut self,
        items: Value,
        capture: Value,
        result_local: LocalId,
        span: TextRange,
        use_virtual_gpu: bool,
        merge_block: BlockId,
    ) {
        let len = self.lower_call_temp(
            MirType::Integer,
            SmolStr::new("__wr_list_len"),
            vec![items.clone()],
            span,
        );
        if use_virtual_gpu {
            let _ = self.lower_call_temp(
                MirType::Nil,
                SmolStr::new("__wr_gpu_dispatch_begin"),
                vec![
                    len.clone(),
                    Value::Const(Literal::Integer(1)),
                    Value::Const(Literal::Integer(1)),
                    Value::Const(Literal::Integer(1)),
                    Value::Const(Literal::Integer(1)),
                    Value::Const(Literal::Integer(1)),
                    Value::Const(Literal::Nil),
                ],
                span,
            );
        }
        let index = self.new_local(
            SmolStr::new(format!("$occluded_batch_index{}", self.locals.len())),
            true,
            MirType::Integer,
        );
        self.assign_use(Place::Local(index), Value::Const(Literal::Integer(0)), span);
        let head = self.new_block();
        let body_block = self.new_block();
        let exit = self.new_block();
        self.set_terminator(Terminator::Jump { target: head, span });
        self.current_block = head;
        let cond = self.lower_binary_temp(
            MirType::Boolean,
            BinaryOp::Lt,
            Value::Local(index),
            len,
            span,
        );
        self.set_terminator(Terminator::Branch {
            cond,
            then_target: body_block,
            else_target: exit,
            span,
        });
        self.current_block = body_block;
        if use_virtual_gpu {
            let _ = self.lower_call_temp(
                MirType::Nil,
                SmolStr::new("__wr_gpu_dispatch_select_invocation"),
                vec![Value::Local(index)],
                span,
            );
        }
        let ray = self.lower_call_temp(
            MirType::Named(SmolStr::new("RayQuery")),
            SmolStr::new("__wr_list_get"),
            vec![items.clone(), Value::Local(index)],
            span,
        );
        let origin = self.new_temp(MirType::Vec3);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(origin),
            value: Rvalue::GetField {
                base: ray.clone(),
                field: SmolStr::new("origin"),
                slot: self.field_slot("RayQuery", "origin"),
            },
            span,
        });
        let direction = self.new_temp(MirType::Vec3);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(direction),
            value: Rvalue::GetField {
                base: ray.clone(),
                field: SmolStr::new("direction"),
                slot: self.field_slot("RayQuery", "direction"),
            },
            span,
        });
        let max_distance = self.new_temp(MirType::Float);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(max_distance),
            value: Rvalue::GetField {
                base: ray.clone(),
                field: SmolStr::new("max_distance"),
                slot: self.field_slot("RayQuery", "max_distance"),
            },
            span,
        });
        let min_step = self.new_temp(MirType::Float);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(min_step),
            value: Rvalue::GetField {
                base: ray.clone(),
                field: SmolStr::new("min_step"),
                slot: self.field_slot("RayQuery", "min_step"),
            },
            span,
        });
        let hit_epsilon = self.new_temp(MirType::Float);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(hit_epsilon),
            value: Rvalue::GetField {
                base: ray.clone(),
                field: SmolStr::new("hit_epsilon"),
                slot: self.field_slot("RayQuery", "hit_epsilon"),
            },
            span,
        });
        let max_steps = self.new_temp(MirType::Integer);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(max_steps),
            value: Rvalue::GetField {
                base: ray,
                field: SmolStr::new("max_steps"),
                slot: self.field_slot("RayQuery", "max_steps"),
            },
            span,
        });
        let hit = self.lower_call_temp(
            MirType::Named(SmolStr::new("Hit3")),
            SmolStr::new("__wr_scene_trace_capture"),
            vec![
                capture.clone(),
                Value::Temp(origin),
                Value::Temp(direction),
                Value::Temp(max_distance),
                Value::Temp(min_step),
                Value::Temp(hit_epsilon),
                Value::Temp(max_steps),
            ],
            span,
        );
        let occlusion = self.build_occlusion_result_value(hit, span);
        let _ = self.lower_call_temp(
            MirType::Nil,
            SmolStr::new("__wr_list_push"),
            vec![Value::Local(result_local), occlusion],
            span,
        );
        let next = self.lower_binary_temp(
            MirType::Integer,
            BinaryOp::Add,
            Value::Local(index),
            Value::Const(Literal::Integer(1)),
            span,
        );
        self.assign_use(Place::Local(index), next, span);
        self.set_terminator(Terminator::Jump { target: head, span });
        self.current_block = exit;
        if use_virtual_gpu {
            let _ = self.lower_call_temp(
                MirType::Nil,
                SmolStr::new("__wr_gpu_dispatch_end"),
                Vec::new(),
                span,
            );
        }
        self.set_terminator(Terminator::Jump {
            target: merge_block,
            span,
        });
    }

    fn build_distance_result_value(&mut self, distance: Value, span: TextRange) -> Value {
        let mut class = self.synthetic_class_target_info("DistanceResult");
        Self::set_class_field_value(&mut class, "distance", distance);
        self.build_class_instance(&class, span)
    }

    fn build_normal_result_value(&mut self, normal: Value, span: TextRange) -> Value {
        let mut class = self.synthetic_class_target_info("NormalResult");
        Self::set_class_field_value(&mut class, "normal", normal);
        self.build_class_instance(&class, span)
    }

    fn build_occlusion_result_value(&mut self, hit: Value, span: TextRange) -> Value {
        let hit_flag = self.new_temp(MirType::Boolean);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(hit_flag),
            value: Rvalue::GetField {
                base: hit.clone(),
                field: SmolStr::new("hit"),
                slot: self.field_slot("Hit3", "hit"),
            },
            span,
        });
        let distance = self.new_temp(MirType::Float);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(distance),
            value: Rvalue::GetField {
                base: hit.clone(),
                field: SmolStr::new("distance"),
                slot: self.field_slot("Hit3", "distance"),
            },
            span,
        });
        let steps = self.new_temp(MirType::Integer);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(steps),
            value: Rvalue::GetField {
                base: hit,
                field: SmolStr::new("steps"),
                slot: self.field_slot("Hit3", "steps"),
            },
            span,
        });
        let mut class = self.synthetic_class_target_info("OcclusionResult");
        Self::set_class_field_value(&mut class, "occluded", Value::Temp(hit_flag));
        Self::set_class_field_value(&mut class, "distance", Value::Temp(distance));
        Self::set_class_field_value(&mut class, "steps", Value::Temp(steps));
        self.build_class_instance(&class, span)
    }

    fn field_slot(&self, class_name: &str, field_name: &str) -> Option<u32> {
        self.class_fields
            .get(&SmolStr::new(class_name))
            .and_then(|fields| fields.iter().position(|field| field.as_str() == field_name))
            .map(|idx| idx as u32)
    }

    fn assign_use(&mut self, place: Place, value: Value, span: TextRange) {
        self.push_stmt(MirStmt::Assign {
            place,
            value: Rvalue::Use(value),
            span,
        });
    }

    fn lower_binary_temp(
        &mut self,
        ty: MirType,
        op: BinaryOp,
        lhs: Value,
        rhs: Value,
        span: TextRange,
    ) -> Value {
        let temp = self.new_temp(ty);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(temp),
            value: Rvalue::Binary { op, lhs, rhs },
            span,
        });
        Value::Temp(temp)
    }

    fn lower_unary_temp(
        &mut self,
        ty: MirType,
        op: hir::UnaryOp,
        operand: Value,
        span: TextRange,
    ) -> Value {
        let temp = self.new_temp(ty);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(temp),
            value: Rvalue::Unary { op, operand },
            span,
        });
        Value::Temp(temp)
    }

    fn lower_call_temp(
        &mut self,
        ty: MirType,
        target: SmolStr,
        args: Vec<Value>,
        span: TextRange,
    ) -> Value {
        let temp = self.new_temp(ty);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(temp),
            value: Rvalue::Call {
                kind: CallKind::Sync,
                target: CallTarget::Function(target),
                args,
            },
            span,
        });
        Value::Temp(temp)
    }

    fn lower_string_interp_temp(&mut self, parts: Vec<StringPartValue>, span: TextRange) -> Value {
        let temp = self.new_temp(MirType::String);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(temp),
            value: Rvalue::StringInterp {
                parts,
                alloc: AllocKind::LocalTemp,
            },
            span,
        });
        Value::Temp(temp)
    }

    fn lower_string_concat_temp(&mut self, lhs: Value, rhs: Value, span: TextRange) -> Value {
        let temp = self.new_temp(MirType::String);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(temp),
            value: Rvalue::StrConcat {
                parts: vec![lhs, rhs],
                alloc: AllocKind::LocalTemp,
            },
            span,
        });
        Value::Temp(temp)
    }

    fn synthetic_class_target_info(&self, class_name: &str) -> ClassTargetInfo {
        let name = SmolStr::new(class_name);
        let fields = self.class_fields.get(&name).cloned().unwrap_or_default();
        let field_defaults = self
            .class_field_defaults
            .get(&name)
            .cloned()
            .unwrap_or_else(|| vec![None; fields.len()]);
        let field_values = vec![None; fields.len()];
        let class_id = self.type_tags.get(&name).copied().unwrap_or(TypeTagId(0));
        ClassTargetInfo {
            name,
            class_id,
            fields,
            field_defaults,
            field_values,
        }
    }

    fn set_class_field_value(class: &mut ClassTargetInfo, field_name: &str, value: Value) {
        if let Some(idx) = class
            .fields
            .iter()
            .position(|field| field.as_str() == field_name)
        {
            class.field_values[idx] = Some(value);
        }
    }

    fn build_default_actor_handle(&mut self, span: TextRange) -> Value {
        let mut class = self.synthetic_class_target_info("ActorHandle");
        Self::set_class_field_value(&mut class, "id", Value::Const(Literal::Integer(0)));
        Self::set_class_field_value(&mut class, "generation", Value::Const(Literal::Integer(0)));
        self.build_class_instance(&class, span)
    }

    fn build_default_payload(&mut self, span: TextRange) -> Value {
        let mut class = self.synthetic_class_target_info("Payload");
        Self::set_class_field_value(&mut class, "entity_id", Value::Const(Literal::Integer(0)));
        Self::set_class_field_value(&mut class, "material_id", Value::Const(Literal::Integer(0)));
        let actor = self.build_default_actor_handle(span);
        Self::set_class_field_value(&mut class, "actor", actor);
        self.build_class_instance(&class, span)
    }

    fn build_default_surface(&mut self, span: TextRange) -> Value {
        let zero = Value::Const(Literal::Float(0.0));
        let black = self.lower_call_temp(
            MirType::Vec3,
            SmolStr::new("vec3"),
            vec![zero.clone(), zero.clone(), zero.clone()],
            span,
        );
        let mut class = self.synthetic_class_target_info("Surface");
        for field in [
            "roughness",
            "metalness",
            "clearcoat",
            "clearcoat_roughness",
            "sheen",
        ] {
            Self::set_class_field_value(&mut class, field, zero.clone());
        }
        Self::set_class_field_value(&mut class, "albedo", black.clone());
        Self::set_class_field_value(&mut class, "emissive", black);
        self.build_class_instance(&class, span)
    }

    fn build_default_medium(&mut self, span: TextRange) -> Value {
        let zero = Value::Const(Literal::Float(0.0));
        let black = self.lower_call_temp(
            MirType::Vec3,
            SmolStr::new("vec3"),
            vec![zero.clone(), zero.clone(), zero.clone()],
            span,
        );
        let mut class = self.synthetic_class_target_info("Medium");
        Self::set_class_field_value(&mut class, "density", zero.clone());
        Self::set_class_field_value(&mut class, "emission", black);
        Self::set_class_field_value(&mut class, "anisotropy", zero);
        self.build_class_instance(&class, span)
    }

    fn build_default_hit(&mut self, origin: Value, span: TextRange) -> Value {
        let zero = Value::Const(Literal::Float(0.0));
        let mut class = self.synthetic_class_target_info("Hit3");
        Self::set_class_field_value(&mut class, "hit", Value::Const(Literal::Boolean(false)));
        Self::set_class_field_value(&mut class, "distance", zero.clone());
        Self::set_class_field_value(&mut class, "position", origin.clone());
        Self::set_class_field_value(&mut class, "local_position", origin.clone());
        let normal = self.lower_call_temp(
            MirType::Vec3,
            SmolStr::new("vec3"),
            vec![
                zero.clone(),
                zero.clone(),
                Value::Const(Literal::Float(1.0)),
            ],
            span,
        );
        Self::set_class_field_value(&mut class, "normal", normal.clone());
        Self::set_class_field_value(&mut class, "local_normal", normal.clone());
        let shading_frame = self.lower_stable_surface_frame(origin.clone(), normal, span);
        Self::set_class_field_value(&mut class, "shading_frame", shading_frame);
        Self::set_class_field_value(&mut class, "steps", Value::Const(Literal::Integer(0)));
        Self::set_class_field_value(&mut class, "feature_id", Value::Const(Literal::Integer(0)));
        Self::set_class_field_value(&mut class, "root_shape_id", Value::Const(Literal::Integer(0)));
        let payload = self.build_default_payload(span);
        Self::set_class_field_value(&mut class, "payload", payload);
        self.build_class_instance(&class, span)
    }

    fn build_scene_capture_value(&mut self, shape_name: &SmolStr, span: TextRange) -> Value {
        let is_field = self.field_names.contains(shape_name);
        let is_shape = self.shape_names.contains(shape_name);
        let mut class = self.synthetic_class_target_info(if is_field {
            "FieldCapture"
        } else if is_shape {
            "ShapeCapture"
        } else {
            "RegionCapture"
        });
        Self::set_class_field_value(
            &mut class,
            "scene_id",
            Value::Const(Literal::Integer(if is_field {
                stable_field_scene_capture_id(shape_name)
            } else if is_shape {
                stable_shape_scene_capture_id(shape_name)
            } else {
                stable_region_scene_capture_id(shape_name)
            })),
        );
        Self::set_class_field_value(&mut class, "epoch", Value::Const(Literal::Integer(0)));
        Self::set_class_field_value(
            &mut class,
            "root_feature_id",
            Value::Const(Literal::Integer(if is_field {
                0
            } else if is_shape {
                stable_shape_capture_id(shape_name)
            } else {
                0
            })),
        );
        self.build_class_instance(&class, span)
    }

    fn build_dispatch_backend_value(&mut self, mode: i64, span: TextRange) -> Value {
        let mut class = self.synthetic_class_target_info("DispatchBackend");
        Self::set_class_field_value(&mut class, "id", Value::Const(Literal::Integer(mode)));
        self.build_class_instance(&class, span)
    }

    fn lower_dispatch_backend_id(&mut self, backend: Value, span: TextRange) -> Value {
        let temp = self.new_temp(MirType::Integer);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(temp),
            value: Rvalue::GetField {
                base: backend,
                field: SmolStr::new("id"),
                slot: self.field_slot("DispatchBackend", "id"),
            },
            span,
        });
        Value::Temp(temp)
    }

    fn lower_shape_payload_body_value(&mut self, payload: &hir::Body, span: TextRange) -> Value {
        if payload.root_stmts.is_empty() {
            return self.build_default_payload(span);
        }
        if payload.root_stmts.len() > 1 {
            self.lower_stmt_block(payload, &payload.root_stmts[..payload.root_stmts.len() - 1]);
        }
        let last = *payload.root_stmts.last().expect("shape payload stmt");
        match &payload.stmts[last] {
            HirStmt::Expr(expr) => self.lower_expr(payload, *expr),
            HirStmt::Return(Some(expr)) => self.lower_expr(payload, *expr),
            _ => {
                self.lower_stmt(payload, last);
                self.build_default_payload(span)
            }
        }
    }

    fn shape_root_expr(&self, shape_name: &SmolStr) -> Option<hir::ShapeExpr> {
        self.shape_graphs
            .get(shape_name)
            .map(|graph| graph.root.clone())
    }

    fn shape_root_provenance_expr(&self, shape_name: &SmolStr) -> Option<hir::ShapeProvenanceExpr> {
        self.shape_graphs
            .get(shape_name)
            .and_then(|graph| graph.provenance.clone())
    }

    fn field_root_expr(&self, field_name: &SmolStr) -> Option<hir::FieldExpr> {
        self.field_graphs
            .get(field_name)
            .map(|graph| graph.root.clone())
    }

    fn field_body(&self, field_name: &SmolStr) -> Option<&hir::Body> {
        self.field_bodies.get(field_name)
    }

    fn unprunable_support_lower_bound(&self) -> Value {
        Value::Const(Literal::Float(-1_000_000.0))
    }

    fn lower_shape_support_lower_bound_expr(
        &mut self,
        expr: &hir::ShapeExpr,
        point: Value,
        span: TextRange,
    ) -> Value {
        match expr {
            hir::ShapeExpr::Use { target } => {
                let Some(root) = self.shape_root_expr(target) else {
                    return self.unprunable_support_lower_bound();
                };
                self.lower_shape_support_lower_bound_expr(&root, point, span)
            }
            hir::ShapeExpr::Leaf(leaf) => {
                self.lower_field_support_lower_bound_call(&leaf.field, point, span)
            }
            hir::ShapeExpr::Union { items, .. } => {
                let mut iter = items.iter();
                let Some(first) = iter.next() else {
                    return self.unprunable_support_lower_bound();
                };
                let mut current =
                    self.lower_shape_support_lower_bound_expr(first, point.clone(), span);
                for item in iter {
                    let rhs = self.lower_shape_support_lower_bound_expr(item, point.clone(), span);
                    current = self.lower_call_temp(
                        MirType::Float,
                        SmolStr::new("field_union"),
                        vec![current, rhs],
                        span,
                    );
                }
                current
            }
            hir::ShapeExpr::Intersection { items, .. } => {
                let mut iter = items.iter();
                let Some(first) = iter.next() else {
                    return self.unprunable_support_lower_bound();
                };
                let mut current =
                    self.lower_shape_support_lower_bound_expr(first, point.clone(), span);
                for item in iter {
                    let rhs = self.lower_shape_support_lower_bound_expr(item, point.clone(), span);
                    current = self.lower_call_temp(
                        MirType::Float,
                        SmolStr::new("field_intersection"),
                        vec![current, rhs],
                        span,
                    );
                }
                current
            }
            hir::ShapeExpr::Subtract { left, .. } => {
                self.lower_shape_support_lower_bound_expr(left, point, span)
            }
        }
    }

    fn lower_field_support_lower_bound_call(
        &mut self,
        field: &SmolStr,
        point: Value,
        span: TextRange,
    ) -> Value {
        let Some(metadata) = self.field_metadata.get(field).cloned() else {
            return self.unprunable_support_lower_bound();
        };
        if let Some(bounds) = self.lower_field_authored_bounds(&metadata, span) {
            return self.lower_bounds_support_lower_bound_value(point, bounds, span);
        }
        if !Self::field_metadata_can_coarse_support_prune(&metadata) {
            return self.unprunable_support_lower_bound();
        }
        let Some(root) = self.field_root_expr(field) else {
            return self.unprunable_support_lower_bound();
        };
        let Some(body) = self.field_body(field).cloned() else {
            return self.unprunable_support_lower_bound();
        };
        self.lower_field_support_lower_bound_expr(&root, &body, point, span)
    }

    fn field_metadata_can_coarse_support_prune(metadata: &hir::FieldMetadata) -> bool {
        metadata.trace.can_coarse_support_pruning
    }

    fn lower_field_authored_bounds(
        &mut self,
        metadata: &hir::FieldMetadata,
        span: TextRange,
    ) -> Option<Value> {
        if !matches!(metadata.trace.support, FieldSupport::Bounded)
            || !matches!(metadata.trace.bounds, FieldBounds::Bounded)
        {
            return None;
        }
        if let Some(bounds) = metadata.authored_bounds.as_ref() {
            return Some(self.lower_wrapped_body_value(bounds, span));
        }
        metadata.authored_support.as_ref().map(|support| {
            let support_value = self.lower_wrapped_body_value(support, span);
            self.lower_get_named_field(
                support_value,
                "Support3",
                "bounds",
                MirType::Named(SmolStr::new("Bounds3")),
                span,
            )
        })
    }

    fn lower_bounds_support_lower_bound_value(
        &mut self,
        point: Value,
        bounds: Value,
        span: TextRange,
    ) -> Value {
        let min = self.lower_get_named_field(bounds.clone(), "Bounds3", "min", MirType::Vec3, span);
        let max = self.lower_get_named_field(bounds, "Bounds3", "max", MirType::Vec3, span);
        self.lower_bounds_box_support_lower_bound(point, min, max, span)
    }

    fn lower_field_support_lower_bound_expr(
        &mut self,
        expr: &hir::FieldExpr,
        body: &hir::Body,
        point: Value,
        span: TextRange,
    ) -> Value {
        match expr {
            hir::FieldExpr::Use { target } => {
                self.lower_field_support_lower_bound_call(target, point, span)
            }
            hir::FieldExpr::Primitive { primitive, args } => {
                self.lower_field_primitive_support_lower_bound(*primitive, args, body, point, span)
            }
            hir::FieldExpr::Union { items } => {
                let mut iter = items.iter();
                let Some(first) = iter.next() else {
                    return self.unprunable_support_lower_bound();
                };
                let mut current =
                    self.lower_field_support_lower_bound_expr(first, body, point.clone(), span);
                for item in iter {
                    let rhs =
                        self.lower_field_support_lower_bound_expr(item, body, point.clone(), span);
                    current = self.lower_call_temp(
                        MirType::Float,
                        SmolStr::new("field_union"),
                        vec![current, rhs],
                        span,
                    );
                }
                current
            }
            hir::FieldExpr::Intersection { items } => {
                let mut iter = items.iter();
                let Some(first) = iter.next() else {
                    return self.unprunable_support_lower_bound();
                };
                let mut current =
                    self.lower_field_support_lower_bound_expr(first, body, point.clone(), span);
                for item in iter {
                    let rhs =
                        self.lower_field_support_lower_bound_expr(item, body, point.clone(), span);
                    current = self.lower_call_temp(
                        MirType::Float,
                        SmolStr::new("field_intersection"),
                        vec![current, rhs],
                        span,
                    );
                }
                current
            }
            hir::FieldExpr::Subtract { left, .. } => {
                self.lower_field_support_lower_bound_expr(left, body, point, span)
            }
            hir::FieldExpr::Translate {
                translate,
                body: inner,
            } => {
                if !self.field_wrapper_body_returns_named_call(translate, "vec3") {
                    return self.unprunable_support_lower_bound();
                }
                let local_point =
                    self.lower_wrapped_support_point("translate", "offset", translate, point, span);
                self.lower_field_support_lower_bound_expr(inner, body, local_point, span)
            }
            hir::FieldExpr::Rotate {
                rotate,
                body: inner,
            } => {
                let local_point =
                    self.lower_wrapped_support_point("rotate", "rotation", rotate, point, span);
                self.lower_field_support_lower_bound_expr(inner, body, local_point, span)
            }
            hir::FieldExpr::UniformScale { scale, body: inner } => {
                let wrapper_value = self.lower_wrapped_body_value(scale, span);
                let local_point = self.lower_call_temp(
                    MirType::Vec3,
                    SmolStr::new("uniform_scale"),
                    vec![wrapper_value.clone(), point],
                    span,
                );
                let child =
                    self.lower_field_support_lower_bound_expr(inner, body, local_point, span);
                let abs_scale = self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("abs"),
                    vec![wrapper_value],
                    span,
                );
                self.lower_binary_temp(MirType::Float, BinaryOp::Mul, child, abs_scale, span)
            }
            hir::FieldExpr::AffineTransform { .. }
            | hir::FieldExpr::Warp { .. }
            | hir::FieldExpr::RadialRepeat { .. }
            | hir::FieldExpr::InstanceArray { .. }
            | hir::FieldExpr::SmoothUnion { .. }
            | hir::FieldExpr::SmoothIntersection { .. }
            | hir::FieldExpr::SmoothSubtract { .. }
            | hir::FieldExpr::Bend { .. }
            | hir::FieldExpr::Twist { .. }
            | hir::FieldExpr::Taper { .. }
            | hir::FieldExpr::Displace { .. } => self.unprunable_support_lower_bound(),
            hir::FieldExpr::RepeatLinear {
                repeat,
                body: inner,
            } => {
                let local_point = self.lower_wrapped_support_point(
                    "repeat_linear",
                    "period",
                    repeat,
                    point,
                    span,
                );
                self.lower_field_support_lower_bound_expr(inner, body, local_point, span)
            }
            hir::FieldExpr::RepeatGrid {
                repeat,
                body: inner,
            } => {
                let local_point =
                    self.lower_wrapped_support_point("repeat_grid", "period", repeat, point, span);
                self.lower_field_support_lower_bound_expr(inner, body, local_point, span)
            }
            hir::FieldExpr::MirrorArray {
                mirror,
                body: inner,
            } => {
                let local_point =
                    self.lower_wrapped_support_point("mirror_array", "mirror", mirror, point, span);
                self.lower_field_support_lower_bound_expr(inner, body, local_point, span)
            }
            hir::FieldExpr::Extrude { height, profile } => {
                let Some(bounds4) = self.lower_profile_bounds4(profile, body, span) else {
                    return self.unprunable_support_lower_bound();
                };
                let height_value = self.lower_wrapped_body_value(height, span);
                let abs_height = self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("abs"),
                    vec![height_value],
                    span,
                );
                let half_height = self.lower_binary_temp(
                    MirType::Float,
                    BinaryOp::Mul,
                    abs_height,
                    Value::Const(Literal::Float(0.5)),
                    span,
                );
                let min_x = self.lower_vec_component_value(bounds4.clone(), 0, span);
                let min_z = self.lower_vec_component_value(bounds4.clone(), 1, span);
                let max_x = self.lower_vec_component_value(bounds4.clone(), 2, span);
                let max_z = self.lower_vec_component_value(bounds4, 3, span);
                let min_y = self.lower_binary_temp(
                    MirType::Float,
                    BinaryOp::Sub,
                    Value::Const(Literal::Float(0.0)),
                    half_height.clone(),
                    span,
                );
                let min = self.lower_vec3_value(min_x, min_y, min_z, span);
                let max = self.lower_vec3_value(max_x, half_height, max_z, span);
                self.lower_bounds_box_support_lower_bound(point, min, max, span)
            }
            hir::FieldExpr::Revolve { profile } => {
                let Some(bounds4) = self.lower_profile_bounds4(profile, body, span) else {
                    return self.unprunable_support_lower_bound();
                };
                let min_x = self.lower_vec_component_value(bounds4.clone(), 0, span);
                let min_y = self.lower_vec_component_value(bounds4.clone(), 1, span);
                let max_x = self.lower_vec_component_value(bounds4.clone(), 2, span);
                let max_y = self.lower_vec_component_value(bounds4, 3, span);
                let abs_min_x =
                    self.lower_call_temp(MirType::Float, SmolStr::new("abs"), vec![min_x], span);
                let abs_max_x =
                    self.lower_call_temp(MirType::Float, SmolStr::new("abs"), vec![max_x], span);
                let radial = self.lower_scalar_max(abs_min_x, abs_max_x, span);
                let neg_radial = self.lower_binary_temp(
                    MirType::Float,
                    BinaryOp::Sub,
                    Value::Const(Literal::Float(0.0)),
                    radial.clone(),
                    span,
                );
                let min = self.lower_vec3_value(neg_radial.clone(), min_y, neg_radial, span);
                let max = self.lower_vec3_value(radial.clone(), max_y, radial, span);
                self.lower_bounds_box_support_lower_bound(point, min, max, span)
            }
            hir::FieldExpr::Sweep { path, profile } => {
                let Some(bounds4) = self.lower_profile_bounds4(profile, body, span) else {
                    return self.unprunable_support_lower_bound();
                };
                let path_value = self.lower_wrapped_body_value(path, span);
                let abs_path = self.lower_call_temp(
                    MirType::Vec3,
                    SmolStr::new("abs"),
                    vec![path_value],
                    span,
                );
                let half_path = self.lower_binary_temp(
                    MirType::Vec3,
                    BinaryOp::Mul,
                    abs_path,
                    Value::Const(Literal::Float(0.5)),
                    span,
                );
                let radius = self.lower_profile_radius_from_bounds4(bounds4, span);
                let radius_vec = self.lower_vec3_splat(radius, span);
                let zero_vec = self.lower_vec3_value(
                    Value::Const(Literal::Float(0.0)),
                    Value::Const(Literal::Float(0.0)),
                    Value::Const(Literal::Float(0.0)),
                    span,
                );
                let neg_half_path = self.lower_binary_temp(
                    MirType::Vec3,
                    BinaryOp::Sub,
                    zero_vec,
                    half_path.clone(),
                    span,
                );
                let min = self.lower_binary_temp(
                    MirType::Vec3,
                    BinaryOp::Sub,
                    neg_half_path,
                    radius_vec.clone(),
                    span,
                );
                let max = self.lower_binary_temp(
                    MirType::Vec3,
                    BinaryOp::Add,
                    half_path,
                    radius_vec,
                    span,
                );
                self.lower_bounds_box_support_lower_bound(point, min, max, span)
            }
            hir::FieldExpr::Loft { height, from, to } => {
                let (Some(from_bounds4), Some(to_bounds4)) = (
                    self.lower_profile_bounds4(from, body, span),
                    self.lower_profile_bounds4(to, body, span),
                ) else {
                    return self.unprunable_support_lower_bound();
                };
                let from_min_x = self.lower_vec_component_value(from_bounds4.clone(), 0, span);
                let from_min_z = self.lower_vec_component_value(from_bounds4.clone(), 1, span);
                let from_max_x = self.lower_vec_component_value(from_bounds4.clone(), 2, span);
                let from_max_z = self.lower_vec_component_value(from_bounds4, 3, span);
                let to_min_x = self.lower_vec_component_value(to_bounds4.clone(), 0, span);
                let to_min_z = self.lower_vec_component_value(to_bounds4.clone(), 1, span);
                let to_max_x = self.lower_vec_component_value(to_bounds4.clone(), 2, span);
                let to_max_z = self.lower_vec_component_value(to_bounds4, 3, span);
                let min_x = self.lower_scalar_min(from_min_x, to_min_x, span);
                let min_z = self.lower_scalar_min(from_min_z, to_min_z, span);
                let max_x = self.lower_scalar_max(from_max_x, to_max_x, span);
                let max_z = self.lower_scalar_max(from_max_z, to_max_z, span);
                let height_value = self.lower_wrapped_body_value(height, span);
                let abs_height = self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("abs"),
                    vec![height_value],
                    span,
                );
                let half_height = self.lower_binary_temp(
                    MirType::Float,
                    BinaryOp::Mul,
                    abs_height,
                    Value::Const(Literal::Float(0.5)),
                    span,
                );
                let min_y = self.lower_binary_temp(
                    MirType::Float,
                    BinaryOp::Sub,
                    Value::Const(Literal::Float(0.0)),
                    half_height.clone(),
                    span,
                );
                let min = self.lower_vec3_value(min_x, min_y, min_z, span);
                let max = self.lower_vec3_value(max_x, half_height, max_z, span);
                self.lower_bounds_box_support_lower_bound(point, min, max, span)
            }
            hir::FieldExpr::Custom { .. } => self.unprunable_support_lower_bound(),
        }
    }

    fn lower_profile_bounds4(
        &mut self,
        profile: &hir::ProfileExpr,
        body: &hir::Body,
        span: TextRange,
    ) -> Option<Value> {
        match profile {
            hir::ProfileExpr::Primitive { primitive, args } => match primitive {
                hir::ProfilePrimitive::Circle2 => {
                    let radius = self.lower_field_named_arg_value(args, body, "radius")?;
                    let neg_radius = self.lower_binary_temp(
                        MirType::Float,
                        BinaryOp::Sub,
                        Value::Const(Literal::Float(0.0)),
                        radius.clone(),
                        span,
                    );
                    Some(self.lower_vec4_value(
                        neg_radius.clone(),
                        neg_radius,
                        radius.clone(),
                        radius,
                        span,
                    ))
                }
                hir::ProfilePrimitive::Rect2 => {
                    let half = self.lower_field_named_arg_value(args, body, "half")?;
                    let half_x = self.lower_vec_component_value(half.clone(), 0, span);
                    let half_y = self.lower_vec_component_value(half, 1, span);
                    let min_x = self.lower_binary_temp(
                        MirType::Float,
                        BinaryOp::Sub,
                        Value::Const(Literal::Float(0.0)),
                        half_x.clone(),
                        span,
                    );
                    let min_y = self.lower_binary_temp(
                        MirType::Float,
                        BinaryOp::Sub,
                        Value::Const(Literal::Float(0.0)),
                        half_y.clone(),
                        span,
                    );
                    Some(self.lower_vec4_value(min_x, min_y, half_x, half_y, span))
                }
                hir::ProfilePrimitive::RoundedRect2 => {
                    let half = self.lower_field_named_arg_value(args, body, "half")?;
                    let radius = self.lower_field_named_arg_value(args, body, "radius")?;
                    let half_x = self.lower_vec_component_value(half.clone(), 0, span);
                    let half_y = self.lower_vec_component_value(half, 1, span);
                    let outer_x = self.lower_binary_temp(
                        MirType::Float,
                        BinaryOp::Add,
                        half_x.clone(),
                        radius.clone(),
                        span,
                    );
                    let outer_y = self.lower_binary_temp(
                        MirType::Float,
                        BinaryOp::Add,
                        half_y.clone(),
                        radius,
                        span,
                    );
                    let min_x = self.lower_binary_temp(
                        MirType::Float,
                        BinaryOp::Sub,
                        Value::Const(Literal::Float(0.0)),
                        outer_x.clone(),
                        span,
                    );
                    let min_y = self.lower_binary_temp(
                        MirType::Float,
                        BinaryOp::Sub,
                        Value::Const(Literal::Float(0.0)),
                        outer_y.clone(),
                        span,
                    );
                    Some(self.lower_vec4_value(min_x, min_y, outer_x, outer_y, span))
                }
                hir::ProfilePrimitive::Capsule2 => {
                    let (a, b, radius) = (
                        self.lower_field_named_arg_value(args, body, "a")?,
                        self.lower_field_named_arg_value(args, body, "b")?,
                        self.lower_field_named_arg_value(args, body, "radius")?,
                    );
                    let a_x = self.lower_vec_component_value(a.clone(), 0, span);
                    let a_y = self.lower_vec_component_value(a, 1, span);
                    let b_x = self.lower_vec_component_value(b.clone(), 0, span);
                    let b_y = self.lower_vec_component_value(b, 1, span);
                    let min_x = self.lower_scalar_min(a_x.clone(), b_x.clone(), span);
                    let min_y = self.lower_scalar_min(a_y.clone(), b_y.clone(), span);
                    let max_x = self.lower_scalar_max(a_x, b_x, span);
                    let max_y = self.lower_scalar_max(a_y, b_y, span);
                    let min_x = self.lower_binary_temp(
                        MirType::Float,
                        BinaryOp::Sub,
                        min_x,
                        radius.clone(),
                        span,
                    );
                    let min_y = self.lower_binary_temp(
                        MirType::Float,
                        BinaryOp::Sub,
                        min_y,
                        radius.clone(),
                        span,
                    );
                    let max_x = self.lower_binary_temp(
                        MirType::Float,
                        BinaryOp::Add,
                        max_x,
                        radius.clone(),
                        span,
                    );
                    let max_y =
                        self.lower_binary_temp(MirType::Float, BinaryOp::Add, max_y, radius, span);
                    Some(self.lower_vec4_value(min_x, min_y, max_x, max_y, span))
                }
                hir::ProfilePrimitive::Segment2 => {
                    let (a, b) = (
                        self.lower_field_named_arg_value(args, body, "a")?,
                        self.lower_field_named_arg_value(args, body, "b")?,
                    );
                    let a_x = self.lower_vec_component_value(a.clone(), 0, span);
                    let a_y = self.lower_vec_component_value(a, 1, span);
                    let b_x = self.lower_vec_component_value(b.clone(), 0, span);
                    let b_y = self.lower_vec_component_value(b, 1, span);
                    let min_x = self.lower_scalar_min(a_x.clone(), b_x.clone(), span);
                    let min_y = self.lower_scalar_min(a_y.clone(), b_y.clone(), span);
                    let max_x = self.lower_scalar_max(a_x, b_x, span);
                    let max_y = self.lower_scalar_max(a_y, b_y, span);
                    Some(self.lower_vec4_value(min_x, min_y, max_x, max_y, span))
                }
                hir::ProfilePrimitive::Polygon2 | hir::ProfilePrimitive::Polyline2 => {
                    let vertices = self.lower_field_named_arg_value(args, body, "vertices")?;
                    Some(self.lower_call_temp(
                        MirType::Vec4,
                        SmolStr::new("field_profile_vertices_bounds4"),
                        vec![vertices],
                        span,
                    ))
                }
            },
        }
    }

    fn lower_profile_radius_from_bounds4(&mut self, bounds4: Value, span: TextRange) -> Value {
        let min_x = self.lower_vec_component_value(bounds4.clone(), 0, span);
        let min_y = self.lower_vec_component_value(bounds4.clone(), 1, span);
        let max_x = self.lower_vec_component_value(bounds4.clone(), 2, span);
        let max_y = self.lower_vec_component_value(bounds4, 3, span);
        let abs_min_x =
            self.lower_call_temp(MirType::Float, SmolStr::new("abs"), vec![min_x], span);
        let abs_min_y =
            self.lower_call_temp(MirType::Float, SmolStr::new("abs"), vec![min_y], span);
        let abs_max_x =
            self.lower_call_temp(MirType::Float, SmolStr::new("abs"), vec![max_x], span);
        let abs_max_y =
            self.lower_call_temp(MirType::Float, SmolStr::new("abs"), vec![max_y], span);
        let radius_x = self.lower_scalar_max(abs_min_x, abs_max_x, span);
        let radius_y = self.lower_scalar_max(abs_min_y, abs_max_y, span);
        self.lower_scalar_max(radius_x, radius_y, span)
    }

    fn lower_wrapped_support_point(
        &mut self,
        callee_name: &str,
        _arg_name: &str,
        wrapped: &hir::Body,
        point: Value,
        span: TextRange,
    ) -> Value {
        let wrapper_value = self.lower_wrapped_body_value(wrapped, span);
        self.lower_call_temp(
            MirType::Vec3,
            SmolStr::new(callee_name),
            vec![wrapper_value, point],
            span,
        )
    }

    fn lower_wrapped_body_value(&mut self, body: &hir::Body, _span: TextRange) -> Value {
        if body.root_stmts.is_empty() {
            return Value::Const(Literal::Nil);
        }
        if body.root_stmts.len() > 1 {
            self.lower_stmt_block(body, &body.root_stmts[..body.root_stmts.len() - 1]);
        }
        let last = *body.root_stmts.last().expect("wrapped body stmt");
        match &body.stmts[last] {
            HirStmt::Expr(expr) => self.lower_expr(body, *expr),
            HirStmt::Return(Some(expr)) => self.lower_expr(body, *expr),
            _ => {
                self.lower_stmt(body, last);
                Value::Const(Literal::Nil)
            }
        }
    }

    fn field_wrapper_body_returns_named_call(&self, body: &hir::Body, name: &str) -> bool {
        self.field_wrapper_body_terminal_callee_name(body)
            .is_some_and(|callee_name| callee_name == name)
    }

    fn field_wrapper_body_terminal_callee_name(&self, body: &hir::Body) -> Option<SmolStr> {
        let Some(expr) = self.field_wrapper_body_terminal_expr(body) else {
            return None;
        };
        let Expr::Call { callee, .. } = &body.exprs[expr] else {
            return None;
        };
        match &body.exprs[*callee] {
            Expr::Variable(callee_name) => Some(callee_name.clone()),
            _ => None,
        }
    }

    fn field_wrapper_body_terminal_expr(&self, body: &hir::Body) -> Option<hir::Idx<Expr>> {
        let stmt = *body.root_stmts.last()?;
        match &body.stmts[stmt] {
            HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) => Some(*expr),
            _ => None,
        }
    }

    fn lower_field_primitive_support_lower_bound(
        &mut self,
        primitive: hir::FieldPrimitive,
        args: &[hir::Arg],
        body: &hir::Body,
        point: Value,
        span: TextRange,
    ) -> Value {
        match primitive {
            hir::FieldPrimitive::Sphere => {
                let Some(radius) = self.lower_field_named_arg_value(args, body, "radius") else {
                    return self.unprunable_support_lower_bound();
                };
                self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("sphere"),
                    vec![point, radius],
                    span,
                )
            }
            hir::FieldPrimitive::Box => {
                let Some(half) = self.lower_field_named_arg_value(args, body, "half") else {
                    return self.unprunable_support_lower_bound();
                };
                self.lower_call_temp(MirType::Float, SmolStr::new("box"), vec![point, half], span)
            }
            hir::FieldPrimitive::Capsule => {
                let (Some(a), Some(b), Some(radius)) = (
                    self.lower_field_named_arg_value(args, body, "a"),
                    self.lower_field_named_arg_value(args, body, "b"),
                    self.lower_field_named_arg_value(args, body, "radius"),
                ) else {
                    return self.unprunable_support_lower_bound();
                };
                let radius_vec = self.lower_vec3_splat(radius, span);
                let min_ab = self.lower_call_temp(
                    MirType::Vec3,
                    SmolStr::new("min"),
                    vec![a.clone(), b.clone()],
                    span,
                );
                let max_ab =
                    self.lower_call_temp(MirType::Vec3, SmolStr::new("max"), vec![a, b], span);
                let min = self.lower_binary_temp(
                    MirType::Vec3,
                    BinaryOp::Sub,
                    min_ab,
                    radius_vec.clone(),
                    span,
                );
                let max =
                    self.lower_binary_temp(MirType::Vec3, BinaryOp::Add, max_ab, radius_vec, span);
                self.lower_bounds_box_support_lower_bound(point, min, max, span)
            }
            hir::FieldPrimitive::Cylinder => {
                let (Some(radius), Some(half_height)) = (
                    self.lower_field_named_arg_value(args, body, "radius"),
                    self.lower_field_named_arg_value(args, body, "half_height"),
                ) else {
                    return self.unprunable_support_lower_bound();
                };
                let min_radius = self.lower_binary_temp(
                    MirType::Float,
                    BinaryOp::Sub,
                    Value::Const(Literal::Float(0.0)),
                    radius.clone(),
                    span,
                );
                let min_half_height = self.lower_binary_temp(
                    MirType::Float,
                    BinaryOp::Sub,
                    Value::Const(Literal::Float(0.0)),
                    half_height.clone(),
                    span,
                );
                let min_radius_z = min_radius.clone();
                let min = self.lower_call_temp(
                    MirType::Vec3,
                    SmolStr::new("vec3"),
                    vec![min_radius, min_half_height, min_radius_z],
                    span,
                );
                let radius_max = radius.clone();
                let half_height_max = half_height.clone();
                let radius_z = radius;
                let max = self.lower_call_temp(
                    MirType::Vec3,
                    SmolStr::new("vec3"),
                    vec![radius_max, half_height_max, radius_z],
                    span,
                );
                self.lower_bounds_box_support_lower_bound(point, min, max, span)
            }
            hir::FieldPrimitive::Plane => self.unprunable_support_lower_bound(),
            hir::FieldPrimitive::Torus => {
                let (Some(major_radius), Some(minor_radius)) = (
                    self.lower_field_named_arg_value(args, body, "major_radius"),
                    self.lower_field_named_arg_value(args, body, "minor_radius"),
                ) else {
                    return self.unprunable_support_lower_bound();
                };
                let outer = self.lower_binary_temp(
                    MirType::Float,
                    BinaryOp::Add,
                    major_radius.clone(),
                    minor_radius.clone(),
                    span,
                );
                let min_outer = self.lower_binary_temp(
                    MirType::Float,
                    BinaryOp::Sub,
                    Value::Const(Literal::Float(0.0)),
                    outer.clone(),
                    span,
                );
                let min_minor = self.lower_binary_temp(
                    MirType::Float,
                    BinaryOp::Sub,
                    Value::Const(Literal::Float(0.0)),
                    minor_radius.clone(),
                    span,
                );
                let min_outer_z = min_outer.clone();
                let min = self.lower_call_temp(
                    MirType::Vec3,
                    SmolStr::new("vec3"),
                    vec![min_outer, min_minor, min_outer_z],
                    span,
                );
                let max_outer_z = outer.clone();
                let max = self.lower_call_temp(
                    MirType::Vec3,
                    SmolStr::new("vec3"),
                    vec![outer, minor_radius, max_outer_z],
                    span,
                );
                self.lower_bounds_box_support_lower_bound(point, min, max, span)
            }
            hir::FieldPrimitive::RoundedBox => {
                let (Some(half), Some(radius)) = (
                    self.lower_field_named_arg_value(args, body, "half"),
                    self.lower_field_named_arg_value(args, body, "radius"),
                ) else {
                    return self.unprunable_support_lower_bound();
                };
                self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("rounded_box"),
                    vec![point, half, radius],
                    span,
                )
            }
            hir::FieldPrimitive::Ellipsoid => {
                let Some(radii) = self.lower_field_named_arg_value(args, body, "radii") else {
                    return self.unprunable_support_lower_bound();
                };
                self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("ellipsoid"),
                    vec![point, radii],
                    span,
                )
            }
            hir::FieldPrimitive::Cone => {
                let (Some(radius), Some(half_height)) = (
                    self.lower_field_named_arg_value(args, body, "radius"),
                    self.lower_field_named_arg_value(args, body, "half_height"),
                ) else {
                    return self.unprunable_support_lower_bound();
                };
                self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("cone"),
                    vec![point, radius, half_height],
                    span,
                )
            }
            hir::FieldPrimitive::CappedCone => {
                let (Some(radius_bottom), Some(radius_top), Some(half_height)) = (
                    self.lower_field_named_arg_value(args, body, "radius_bottom"),
                    self.lower_field_named_arg_value(args, body, "radius_top"),
                    self.lower_field_named_arg_value(args, body, "half_height"),
                ) else {
                    return self.unprunable_support_lower_bound();
                };
                self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("capped_cone"),
                    vec![point, radius_bottom, radius_top, half_height],
                    span,
                )
            }
            hir::FieldPrimitive::BoxFrame => {
                let (Some(half), Some(thickness)) = (
                    self.lower_field_named_arg_value(args, body, "half"),
                    self.lower_field_named_arg_value(args, body, "thickness"),
                ) else {
                    return self.unprunable_support_lower_bound();
                };
                self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("box_frame"),
                    vec![point, half, thickness],
                    span,
                )
            }
            hir::FieldPrimitive::Slab => self.unprunable_support_lower_bound(),
            hir::FieldPrimitive::TrianglePrism => {
                let (Some(half), Some(half_height)) = (
                    self.lower_field_named_arg_value(args, body, "half"),
                    self.lower_field_named_arg_value(args, body, "half_height"),
                ) else {
                    return self.unprunable_support_lower_bound();
                };
                self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("triangle_prism"),
                    vec![point, half, half_height],
                    span,
                )
            }
            hir::FieldPrimitive::HexPrism => {
                let (Some(half), Some(half_height)) = (
                    self.lower_field_named_arg_value(args, body, "half"),
                    self.lower_field_named_arg_value(args, body, "half_height"),
                ) else {
                    return self.unprunable_support_lower_bound();
                };
                self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("hex_prism"),
                    vec![point, half, half_height],
                    span,
                )
            }
        }
    }

    fn lower_field_named_arg_value(
        &mut self,
        args: &[hir::Arg],
        body: &hir::Body,
        name: &str,
    ) -> Option<Value> {
        args.iter().find_map(|arg| match arg {
            hir::Arg::Named {
                name: arg_name,
                value,
                ..
            } if arg_name.as_str() == name => Some(self.lower_expr(body, *value)),
            _ => None,
        })
    }

    fn lower_vec3_splat(&mut self, value: Value, span: TextRange) -> Value {
        self.lower_call_temp(
            MirType::Vec3,
            SmolStr::new("vec3"),
            vec![value.clone(), value.clone(), value],
            span,
        )
    }

    fn lower_vec3_value(&mut self, x: Value, y: Value, z: Value, span: TextRange) -> Value {
        self.lower_call_temp(MirType::Vec3, SmolStr::new("vec3"), vec![x, y, z], span)
    }

    fn lower_vec4_value(
        &mut self,
        x: Value,
        y: Value,
        z: Value,
        w: Value,
        span: TextRange,
    ) -> Value {
        self.lower_call_temp(MirType::Vec4, SmolStr::new("vec4"), vec![x, y, z, w], span)
    }

    fn lower_vec_component_value(&mut self, value: Value, index: i64, span: TextRange) -> Value {
        self.lower_call_temp(
            MirType::Float,
            SmolStr::new("__wr_vec_component"),
            vec![value, Value::Const(Literal::Integer(index))],
            span,
        )
    }

    fn lower_scalar_min(&mut self, left: Value, right: Value, span: TextRange) -> Value {
        self.lower_call_temp(MirType::Float, SmolStr::new("min"), vec![left, right], span)
    }

    fn lower_scalar_max(&mut self, left: Value, right: Value, span: TextRange) -> Value {
        self.lower_call_temp(MirType::Float, SmolStr::new("max"), vec![left, right], span)
    }

    fn lower_bounds_box_support_lower_bound(
        &mut self,
        point: Value,
        min: Value,
        max: Value,
        span: TextRange,
    ) -> Value {
        let center_sum =
            self.lower_binary_temp(MirType::Vec3, BinaryOp::Add, min.clone(), max.clone(), span);
        let center = self.lower_binary_temp(
            MirType::Vec3,
            BinaryOp::Mul,
            center_sum,
            Value::Const(Literal::Float(0.5)),
            span,
        );
        let half_delta = self.lower_binary_temp(MirType::Vec3, BinaryOp::Sub, max, min, span);
        let half = self.lower_binary_temp(
            MirType::Vec3,
            BinaryOp::Mul,
            half_delta,
            Value::Const(Literal::Float(0.5)),
            span,
        );
        let local_point = self.lower_binary_temp(MirType::Vec3, BinaryOp::Sub, point, center, span);
        self.lower_call_temp(
            MirType::Float,
            SmolStr::new("box"),
            vec![local_point, half],
            span,
        )
    }

    fn lower_shape_distance_call(
        &mut self,
        shape: &SmolStr,
        point: Value,
        span: TextRange,
    ) -> Value {
        if !self.shape_names.contains(shape) {
            return Value::Const(Literal::Float(1_000_000.0));
        }
        self.lower_call_temp(
            MirType::Float,
            SmolStr::new(format!("__wr_shape_distance_{shape}")),
            vec![point],
            span,
        )
    }

    fn lower_shape_distance_expr(
        &mut self,
        expr: &hir::ShapeExpr,
        point: Value,
        span: TextRange,
    ) -> Value {
        match expr {
            hir::ShapeExpr::Use { target } => self.lower_shape_distance_call(target, point, span),
            hir::ShapeExpr::Leaf(leaf) => self.lower_field_distance_call(&leaf.field, point, span),
            hir::ShapeExpr::Union { items, .. } => {
                let mut iter = items.iter();
                let Some(first) = iter.next() else {
                    return Value::Const(Literal::Float(1_000_000.0));
                };
                let _ = self.lower_call_temp(
                    MirType::Nil,
                    SmolStr::new("__wr_metrics_scene_trace_candidate_branch"),
                    vec![],
                    span,
                );
                let mut current = self.lower_shape_distance_expr(first, point.clone(), span);
                for item in iter {
                    let _ = self.lower_call_temp(
                        MirType::Nil,
                        SmolStr::new("__wr_metrics_scene_trace_candidate_branch"),
                        vec![],
                        span,
                    );
                    let support_lower_bound =
                        self.lower_shape_support_lower_bound_expr(item, point.clone(), span);
                    let keep_pruned = self.lower_binary_temp(
                        MirType::Boolean,
                        BinaryOp::Ge,
                        support_lower_bound,
                        current.clone(),
                        span,
                    );
                    let prune_block = self.new_block();
                    let eval_block = self.new_block();
                    let merge_block = self.new_block();
                    let dist_local =
                        self.new_local(SmolStr::new("$shape_union_dist"), true, MirType::Float);
                    self.assign_use(Place::Local(dist_local), current, span);
                    self.set_terminator(Terminator::Branch {
                        cond: keep_pruned,
                        then_target: prune_block,
                        else_target: eval_block,
                        span,
                    });
                    self.current_block = prune_block;
                    let _ = self.lower_call_temp(
                        MirType::Nil,
                        SmolStr::new("__wr_metrics_scene_trace_support_pruned_branch"),
                        vec![],
                        span,
                    );
                    self.set_terminator(Terminator::Jump {
                        target: merge_block,
                        span,
                    });
                    self.current_block = eval_block;
                    let rhs = self.lower_shape_distance_expr(item, point.clone(), span);
                    let next = self.lower_call_temp(
                        MirType::Float,
                        SmolStr::new("field_union"),
                        vec![Value::Local(dist_local), rhs],
                        span,
                    );
                    self.assign_use(Place::Local(dist_local), next, span);
                    self.set_terminator(Terminator::Jump {
                        target: merge_block,
                        span,
                    });
                    self.current_block = merge_block;
                    current = Value::Local(dist_local);
                }
                current
            }
            hir::ShapeExpr::Intersection { items, .. } => {
                let mut iter = items.iter();
                let Some(first) = iter.next() else {
                    return Value::Const(Literal::Float(1_000_000.0));
                };
                let _ = self.lower_call_temp(
                    MirType::Nil,
                    SmolStr::new("__wr_metrics_scene_trace_candidate_branch"),
                    vec![],
                    span,
                );
                let mut current = self.lower_shape_distance_expr(first, point.clone(), span);
                for item in iter {
                    let _ = self.lower_call_temp(
                        MirType::Nil,
                        SmolStr::new("__wr_metrics_scene_trace_candidate_branch"),
                        vec![],
                        span,
                    );
                    let rhs = self.lower_shape_distance_expr(item, point.clone(), span);
                    current = self.lower_call_temp(
                        MirType::Float,
                        SmolStr::new("field_intersection"),
                        vec![current, rhs],
                        span,
                    );
                }
                current
            }
            hir::ShapeExpr::Subtract { left, right, .. } => {
                let _ = self.lower_call_temp(
                    MirType::Nil,
                    SmolStr::new("__wr_metrics_scene_trace_candidate_branch"),
                    vec![],
                    span,
                );
                let lhs = self.lower_shape_distance_expr(left, point.clone(), span);
                let _ = self.lower_call_temp(
                    MirType::Nil,
                    SmolStr::new("__wr_metrics_scene_trace_candidate_branch"),
                    vec![],
                    span,
                );
                let rhs = self.lower_shape_distance_expr(right, point, span);
                self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("field_subtract"),
                    vec![lhs, rhs],
                    span,
                )
            }
        }
    }

    fn lower_shape_normal_call(&mut self, shape: &SmolStr, point: Value, span: TextRange) -> Value {
        let dx = self.lower_shape_axis_difference(shape, point.clone(), [0.001, 0.0, 0.0], span);
        let dy = self.lower_shape_axis_difference(shape, point.clone(), [0.0, 0.001, 0.0], span);
        let dz = self.lower_shape_axis_difference(shape, point, [0.0, 0.0, 0.001], span);
        let gradient =
            self.lower_call_temp(MirType::Vec3, SmolStr::new("vec3"), vec![dx, dy, dz], span);
        self.lower_call_temp(
            MirType::Vec3,
            SmolStr::new("normalize"),
            vec![gradient],
            span,
        )
    }

    fn lower_shape_axis_difference(
        &mut self,
        shape: &SmolStr,
        point: Value,
        offset: [f64; 3],
        span: TextRange,
    ) -> Value {
        let plus_point = self.lower_offset_point(point.clone(), offset, span);
        let plus = self.lower_shape_distance_call(shape, plus_point, span);
        let minus_point =
            self.lower_offset_point(point, [-offset[0], -offset[1], -offset[2]], span);
        let minus = self.lower_shape_distance_call(shape, minus_point, span);
        self.lower_binary_temp(MirType::Float, BinaryOp::Sub, plus, minus, span)
    }

    fn lower_shape_feature_id_value(
        &self,
        _feature_path: &[SmolStr],
        leaf_feature_id: u64,
    ) -> Value {
        Value::Const(Literal::Integer(
            (leaf_feature_id & (i64::MAX as u64)) as i64,
        ))
    }

    fn lower_shape_merge_keep_current(
        &mut self,
        provenance: hir::ShapeMergeProvenancePolicy,
        current_dist: Value,
        next_dist: Value,
        prefer_larger: bool,
        _hit_epsilon: Value,
        span: TextRange,
    ) -> Value {
        match provenance {
            hir::ShapeMergeProvenancePolicy::Nearest => self.lower_binary_temp(
                MirType::Boolean,
                if prefer_larger {
                    BinaryOp::Ge
                } else {
                    BinaryOp::Le
                },
                current_dist,
                next_dist,
                span,
            ),
            hir::ShapeMergeProvenancePolicy::Ordered => Value::Const(Literal::Boolean(true)),
        }
    }

    fn lower_shape_payload_selection(
        &mut self,
        expr: &hir::ShapeExpr,
        provenance: Option<&hir::ShapeProvenanceExpr>,
        point: Value,
        hit_epsilon: Value,
        feature_path: &mut Vec<SmolStr>,
        span: TextRange,
    ) -> (Value, Value, Value) {
        match expr {
            hir::ShapeExpr::Use { target } => {
                let Some(root) = self.shape_root_expr(target) else {
                    return (
                        Value::Const(Literal::Float(1_000_000.0)),
                        self.build_default_payload(span),
                        Value::Const(Literal::Integer(0)),
                    );
                };
                let root_provenance = self.shape_root_provenance_expr(target);
                feature_path.push(SmolStr::new(format!("use[{target}]")));
                let result = self.lower_shape_payload_selection(
                    &root,
                    root_provenance.as_ref(),
                    point,
                    hit_epsilon.clone(),
                    feature_path,
                    span,
                );
                feature_path.pop();
                result
            }
            hir::ShapeExpr::Leaf(leaf) => (
                self.lower_field_distance_call(&leaf.field, point, span),
                self.lower_shape_payload_body_value(&leaf.payload, span),
                self.lower_shape_feature_id_value(feature_path, leaf.feature_id),
            ),
            hir::ShapeExpr::Union { items, .. } => {
                let (merge_policy, provenance_items) = match provenance {
                    Some(hir::ShapeProvenanceExpr::Union { provenance, items }) => {
                        (*provenance, Some(items.as_slice()))
                    }
                    _ => (hir::ShapeMergeProvenancePolicy::Nearest, None),
                };
                let mut iter = items.iter();
                let Some(first) = iter.next() else {
                    return (
                        Value::Const(Literal::Float(1_000_000.0)),
                        self.build_default_payload(span),
                        Value::Const(Literal::Integer(0)),
                    );
                };
                feature_path.push(SmolStr::new("union[0]"));
                let first_provenance = provenance_items.and_then(|items| items.first());
                let (first_dist, first_payload, first_feature_id) = self
                    .lower_shape_payload_selection(
                        first,
                        first_provenance,
                        point.clone(),
                        hit_epsilon.clone(),
                        feature_path,
                        span,
                    );
                feature_path.pop();
                let dist_local = self.new_local(SmolStr::new("$shape_dist"), true, MirType::Float);
                let payload_local = self.new_local(
                    SmolStr::new("$shape_payload"),
                    true,
                    MirType::Named(SmolStr::new("Payload")),
                );
                let feature_id_local =
                    self.new_local(SmolStr::new("$shape_feature_id"), true, MirType::Integer);
                self.assign_use(Place::Local(dist_local), first_dist, span);
                self.assign_use(Place::Local(payload_local), first_payload, span);
                self.assign_use(Place::Local(feature_id_local), first_feature_id, span);
                for (idx, item) in iter.enumerate().map(|(idx, item)| (idx + 1, item)) {
                    feature_path.push(SmolStr::new(format!("union[{idx}]")));
                    let next_provenance = provenance_items.and_then(|items| items.get(idx));
                    let (next_dist, next_payload, next_feature_id) = self
                        .lower_shape_payload_selection(
                            item,
                            next_provenance,
                            point.clone(),
                            hit_epsilon.clone(),
                            feature_path,
                            span,
                        );
                    feature_path.pop();
                    match merge_policy {
                        hir::ShapeMergeProvenancePolicy::Ordered => {
                            let composed_dist = self.lower_call_temp(
                                MirType::Float,
                                SmolStr::new("field_union"),
                                vec![Value::Local(dist_local), next_dist],
                                span,
                            );
                            self.assign_use(Place::Local(dist_local), composed_dist, span);
                        }
                        hir::ShapeMergeProvenancePolicy::Nearest => {
                            let keep_current = self.lower_shape_merge_keep_current(
                                merge_policy,
                                Value::Local(dist_local),
                                next_dist.clone(),
                                false,
                                hit_epsilon.clone(),
                                span,
                            );
                            let keep_block = self.new_block();
                            let replace_block = self.new_block();
                            let merge_block = self.new_block();
                            self.set_terminator(Terminator::Branch {
                                cond: keep_current,
                                then_target: keep_block,
                                else_target: replace_block,
                                span,
                            });
                            self.current_block = keep_block;
                            self.set_terminator(Terminator::Jump {
                                target: merge_block,
                                span,
                            });
                            self.current_block = replace_block;
                            self.assign_use(Place::Local(dist_local), next_dist, span);
                            self.assign_use(Place::Local(payload_local), next_payload, span);
                            self.assign_use(Place::Local(feature_id_local), next_feature_id, span);
                            self.set_terminator(Terminator::Jump {
                                target: merge_block,
                                span,
                            });
                            self.current_block = merge_block;
                        }
                    }
                }
                (
                    Value::Local(dist_local),
                    Value::Local(payload_local),
                    Value::Local(feature_id_local),
                )
            }
            hir::ShapeExpr::Intersection { items, .. } => {
                let (merge_policy, provenance_items) = match provenance {
                    Some(hir::ShapeProvenanceExpr::Intersection { provenance, items }) => {
                        (*provenance, Some(items.as_slice()))
                    }
                    _ => (hir::ShapeMergeProvenancePolicy::Nearest, None),
                };
                let mut iter = items.iter();
                let Some(first) = iter.next() else {
                    return (
                        Value::Const(Literal::Float(1_000_000.0)),
                        self.build_default_payload(span),
                        Value::Const(Literal::Integer(0)),
                    );
                };
                feature_path.push(SmolStr::new("intersection[0]"));
                let first_provenance = provenance_items.and_then(|items| items.first());
                let (first_dist, first_payload, first_feature_id) = self
                    .lower_shape_payload_selection(
                        first,
                        first_provenance,
                        point.clone(),
                        hit_epsilon.clone(),
                        feature_path,
                        span,
                    );
                feature_path.pop();
                let dist_local = self.new_local(SmolStr::new("$shape_dist"), true, MirType::Float);
                let payload_local = self.new_local(
                    SmolStr::new("$shape_payload"),
                    true,
                    MirType::Named(SmolStr::new("Payload")),
                );
                let feature_id_local =
                    self.new_local(SmolStr::new("$shape_feature_id"), true, MirType::Integer);
                self.assign_use(Place::Local(dist_local), first_dist, span);
                self.assign_use(Place::Local(payload_local), first_payload, span);
                self.assign_use(Place::Local(feature_id_local), first_feature_id, span);
                for (idx, item) in iter.enumerate().map(|(idx, item)| (idx + 1, item)) {
                    feature_path.push(SmolStr::new(format!("intersection[{idx}]")));
                    let next_provenance = provenance_items.and_then(|items| items.get(idx));
                    let (next_dist, next_payload, next_feature_id) = self
                        .lower_shape_payload_selection(
                            item,
                            next_provenance,
                            point.clone(),
                            hit_epsilon.clone(),
                            feature_path,
                            span,
                        );
                    feature_path.pop();
                    match merge_policy {
                        hir::ShapeMergeProvenancePolicy::Ordered => {
                            let composed_dist = self.lower_call_temp(
                                MirType::Float,
                                SmolStr::new("field_intersection"),
                                vec![Value::Local(dist_local), next_dist],
                                span,
                            );
                            self.assign_use(Place::Local(dist_local), composed_dist, span);
                        }
                        hir::ShapeMergeProvenancePolicy::Nearest => {
                            let keep_current = self.lower_shape_merge_keep_current(
                                merge_policy,
                                Value::Local(dist_local),
                                next_dist.clone(),
                                true,
                                hit_epsilon.clone(),
                                span,
                            );
                            let keep_block = self.new_block();
                            let replace_block = self.new_block();
                            let merge_block = self.new_block();
                            self.set_terminator(Terminator::Branch {
                                cond: keep_current,
                                then_target: keep_block,
                                else_target: replace_block,
                                span,
                            });
                            self.current_block = keep_block;
                            self.set_terminator(Terminator::Jump {
                                target: merge_block,
                                span,
                            });
                            self.current_block = replace_block;
                            self.assign_use(Place::Local(dist_local), next_dist, span);
                            self.assign_use(Place::Local(payload_local), next_payload, span);
                            self.assign_use(Place::Local(feature_id_local), next_feature_id, span);
                            self.set_terminator(Terminator::Jump {
                                target: merge_block,
                                span,
                            });
                            self.current_block = merge_block;
                        }
                    }
                }
                (
                    Value::Local(dist_local),
                    Value::Local(payload_local),
                    Value::Local(feature_id_local),
                )
            }
            hir::ShapeExpr::Subtract { left, right, .. } => {
                let (subtract_policy, left_provenance, right_provenance) = match provenance {
                    Some(hir::ShapeProvenanceExpr::Subtract {
                        provenance,
                        left,
                        right,
                    }) => (*provenance, Some(left.as_ref()), Some(right.as_ref())),
                    _ => (hir::ShapeSubtractProvenancePolicy::Left, None, None),
                };
                feature_path.push(SmolStr::new("subtract[left]"));
                let (left_dist, left_payload, left_feature_id) = self
                    .lower_shape_payload_selection(
                        left,
                        left_provenance,
                        point.clone(),
                        hit_epsilon.clone(),
                        feature_path,
                        span,
                    );
                feature_path.pop();
                feature_path.push(SmolStr::new("subtract[right]"));
                let (right_dist, right_payload, right_feature_id) = self
                    .lower_shape_payload_selection(
                        right,
                        right_provenance,
                        point,
                        hit_epsilon,
                        feature_path,
                        span,
                    );
                feature_path.pop();
                let neg_right = self.lower_binary_temp(
                    MirType::Float,
                    BinaryOp::Sub,
                    Value::Const(Literal::Float(0.0)),
                    right_dist,
                    span,
                );
                let choose_left = self.lower_binary_temp(
                    MirType::Boolean,
                    BinaryOp::Ge,
                    left_dist.clone(),
                    neg_right.clone(),
                    span,
                );
                let dist_local = self.new_local(SmolStr::new("$shape_dist"), true, MirType::Float);
                let payload_local = self.new_local(
                    SmolStr::new("$shape_payload"),
                    true,
                    MirType::Named(SmolStr::new("Payload")),
                );
                let feature_id_local =
                    self.new_local(SmolStr::new("$shape_feature_id"), true, MirType::Integer);
                let left_block = self.new_block();
                let right_block = self.new_block();
                let merge_block = self.new_block();
                self.set_terminator(Terminator::Branch {
                    cond: choose_left,
                    then_target: left_block,
                    else_target: right_block,
                    span,
                });
                self.current_block = left_block;
                self.assign_use(Place::Local(dist_local), left_dist, span);
                self.assign_use(Place::Local(payload_local), left_payload.clone(), span);
                self.assign_use(
                    Place::Local(feature_id_local),
                    left_feature_id.clone(),
                    span,
                );
                self.set_terminator(Terminator::Jump {
                    target: merge_block,
                    span,
                });
                self.current_block = right_block;
                self.assign_use(Place::Local(dist_local), neg_right, span);
                match subtract_policy {
                    hir::ShapeSubtractProvenancePolicy::Left => {
                        self.assign_use(Place::Local(payload_local), left_payload, span);
                        self.assign_use(Place::Local(feature_id_local), left_feature_id, span);
                    }
                    hir::ShapeSubtractProvenancePolicy::Right => {
                        self.assign_use(Place::Local(payload_local), right_payload, span);
                        self.assign_use(Place::Local(feature_id_local), right_feature_id, span);
                    }
                }
                self.set_terminator(Terminator::Jump {
                    target: merge_block,
                    span,
                });
                self.current_block = merge_block;
                (
                    Value::Local(dist_local),
                    Value::Local(payload_local),
                    Value::Local(feature_id_local),
                )
            }
        }
    }

    fn lower_shape_surface_selection(
        &mut self,
        expr: &hir::ShapeExpr,
        feature_id: Value,
        hit: Value,
        feature_path: &mut Vec<SmolStr>,
        span: TextRange,
    ) -> (Value, Value) {
        match expr {
            hir::ShapeExpr::Use { target } => {
                let Some(root) = self.shape_root_expr(target) else {
                    return (
                        Value::Const(Literal::Boolean(false)),
                        self.build_default_surface(span),
                    );
                };
                feature_path.push(SmolStr::new(format!("use[{target}]")));
                let result =
                    self.lower_shape_surface_selection(&root, feature_id, hit, feature_path, span);
                feature_path.pop();
                result
            }
            hir::ShapeExpr::Leaf(leaf) => {
                let leaf_feature_id =
                    self.lower_shape_feature_id_value(feature_path, leaf.feature_id);
                let matched = self.lower_binary_temp(
                    MirType::Boolean,
                    BinaryOp::Eq,
                    feature_id,
                    leaf_feature_id,
                    span,
                );
                let surface_local = self.new_local(
                    SmolStr::new("$shape_surface_leaf"),
                    true,
                    MirType::Named(SmolStr::new("Surface")),
                );
                let default_surface = self.build_default_surface(span);
                self.assign_use(Place::Local(surface_local), default_surface, span);
                let matched_block = self.new_block();
                let miss_block = self.new_block();
                let merge_block = self.new_block();
                self.set_terminator(Terminator::Branch {
                    cond: matched.clone(),
                    then_target: matched_block,
                    else_target: miss_block,
                    span,
                });
                self.current_block = matched_block;
                let surface = self.lower_call_temp(
                    MirType::Named(SmolStr::new("Surface")),
                    leaf.material.clone(),
                    vec![hit],
                    span,
                );
                self.assign_use(Place::Local(surface_local), surface, span);
                self.set_terminator(Terminator::Jump {
                    target: merge_block,
                    span,
                });
                self.current_block = miss_block;
                self.set_terminator(Terminator::Jump {
                    target: merge_block,
                    span,
                });
                self.current_block = merge_block;
                (matched, Value::Local(surface_local))
            }
            hir::ShapeExpr::Union { items, .. } => {
                let mut iter = items.iter();
                let Some(first) = iter.next() else {
                    return (
                        Value::Const(Literal::Boolean(false)),
                        self.build_default_surface(span),
                    );
                };
                feature_path.push(SmolStr::new("union[0]"));
                let result = self.lower_shape_surface_selection(
                    first,
                    feature_id.clone(),
                    hit.clone(),
                    feature_path,
                    span,
                );
                feature_path.pop();
                let (first_matched, first_surface) = result;
                let matched_local =
                    self.new_local(SmolStr::new("$shape_surface_match"), true, MirType::Boolean);
                let surface_local = self.new_local(
                    SmolStr::new("$shape_surface"),
                    true,
                    MirType::Named(SmolStr::new("Surface")),
                );
                self.assign_use(Place::Local(matched_local), first_matched, span);
                self.assign_use(Place::Local(surface_local), first_surface, span);
                for (idx, item) in iter.enumerate().map(|(idx, item)| (idx + 1, item)) {
                    feature_path.push(SmolStr::new(format!("union[{idx}]")));
                    let (next_matched, next_surface) = self.lower_shape_surface_selection(
                        item,
                        feature_id.clone(),
                        hit.clone(),
                        feature_path,
                        span,
                    );
                    feature_path.pop();
                    let already_matched = Value::Local(matched_local);
                    let keep_current = already_matched.clone();
                    let take_next = self.lower_unary_temp(
                        MirType::Boolean,
                        hir::UnaryOp::Not,
                        already_matched,
                        span,
                    );
                    let keep_block = self.new_block();
                    let replace_block = self.new_block();
                    let merge_block = self.new_block();
                    self.set_terminator(Terminator::Branch {
                        cond: keep_current,
                        then_target: keep_block,
                        else_target: replace_block,
                        span,
                    });
                    self.current_block = keep_block;
                    self.set_terminator(Terminator::Jump {
                        target: merge_block,
                        span,
                    });
                    self.current_block = replace_block;
                    let matched_block = self.new_block();
                    let miss_block = self.new_block();
                    self.set_terminator(Terminator::Branch {
                        cond: take_next,
                        then_target: matched_block,
                        else_target: miss_block,
                        span,
                    });
                    self.current_block = matched_block;
                    self.assign_use(Place::Local(matched_local), next_matched, span);
                    self.assign_use(Place::Local(surface_local), next_surface, span);
                    self.set_terminator(Terminator::Jump {
                        target: merge_block,
                        span,
                    });
                    self.current_block = miss_block;
                    self.set_terminator(Terminator::Jump {
                        target: merge_block,
                        span,
                    });
                    self.current_block = merge_block;
                }
                (Value::Local(matched_local), Value::Local(surface_local))
            }
            hir::ShapeExpr::Intersection { items, .. } => {
                let mut iter = items.iter();
                let Some(first) = iter.next() else {
                    return (
                        Value::Const(Literal::Boolean(false)),
                        self.build_default_surface(span),
                    );
                };
                feature_path.push(SmolStr::new("intersection[0]"));
                let result = self.lower_shape_surface_selection(
                    first,
                    feature_id.clone(),
                    hit.clone(),
                    feature_path,
                    span,
                );
                feature_path.pop();
                let (first_matched, first_surface) = result;
                let matched_local =
                    self.new_local(SmolStr::new("$shape_surface_match"), true, MirType::Boolean);
                let surface_local = self.new_local(
                    SmolStr::new("$shape_surface"),
                    true,
                    MirType::Named(SmolStr::new("Surface")),
                );
                self.assign_use(Place::Local(matched_local), first_matched, span);
                self.assign_use(Place::Local(surface_local), first_surface, span);
                for (idx, item) in iter.enumerate().map(|(idx, item)| (idx + 1, item)) {
                    feature_path.push(SmolStr::new(format!("intersection[{idx}]")));
                    let (next_matched, next_surface) = self.lower_shape_surface_selection(
                        item,
                        feature_id.clone(),
                        hit.clone(),
                        feature_path,
                        span,
                    );
                    feature_path.pop();
                    let already_matched = Value::Local(matched_local);
                    let keep_current = already_matched.clone();
                    let take_next = self.lower_unary_temp(
                        MirType::Boolean,
                        hir::UnaryOp::Not,
                        already_matched,
                        span,
                    );
                    let keep_block = self.new_block();
                    let replace_block = self.new_block();
                    let merge_block = self.new_block();
                    self.set_terminator(Terminator::Branch {
                        cond: keep_current,
                        then_target: keep_block,
                        else_target: replace_block,
                        span,
                    });
                    self.current_block = keep_block;
                    self.set_terminator(Terminator::Jump {
                        target: merge_block,
                        span,
                    });
                    self.current_block = replace_block;
                    let matched_block = self.new_block();
                    let miss_block = self.new_block();
                    self.set_terminator(Terminator::Branch {
                        cond: take_next,
                        then_target: matched_block,
                        else_target: miss_block,
                        span,
                    });
                    self.current_block = matched_block;
                    self.assign_use(Place::Local(matched_local), next_matched, span);
                    self.assign_use(Place::Local(surface_local), next_surface, span);
                    self.set_terminator(Terminator::Jump {
                        target: merge_block,
                        span,
                    });
                    self.current_block = miss_block;
                    self.set_terminator(Terminator::Jump {
                        target: merge_block,
                        span,
                    });
                    self.current_block = merge_block;
                }
                (Value::Local(matched_local), Value::Local(surface_local))
            }
            hir::ShapeExpr::Subtract { left, right, .. } => {
                feature_path.push(SmolStr::new("subtract[left]"));
                let result = self.lower_shape_surface_selection(
                    left,
                    feature_id.clone(),
                    hit.clone(),
                    feature_path,
                    span,
                );
                feature_path.pop();
                let (left_matched, left_surface) = result;
                let matched_local =
                    self.new_local(SmolStr::new("$shape_surface_match"), true, MirType::Boolean);
                let surface_local = self.new_local(
                    SmolStr::new("$shape_surface"),
                    true,
                    MirType::Named(SmolStr::new("Surface")),
                );
                self.assign_use(Place::Local(matched_local), left_matched, span);
                self.assign_use(Place::Local(surface_local), left_surface, span);
                let already_matched = Value::Local(matched_local);
                let keep_block = self.new_block();
                let replace_block = self.new_block();
                let merge_block = self.new_block();
                self.set_terminator(Terminator::Branch {
                    cond: already_matched.clone(),
                    then_target: keep_block,
                    else_target: replace_block,
                    span,
                });
                self.current_block = keep_block;
                self.set_terminator(Terminator::Jump {
                    target: merge_block,
                    span,
                });
                self.current_block = replace_block;
                feature_path.push(SmolStr::new("subtract[right]"));
                let result =
                    self.lower_shape_surface_selection(right, feature_id, hit, feature_path, span);
                feature_path.pop();
                let (right_matched, right_surface) = result;
                self.assign_use(Place::Local(matched_local), right_matched, span);
                self.assign_use(Place::Local(surface_local), right_surface, span);
                self.set_terminator(Terminator::Jump {
                    target: merge_block,
                    span,
                });
                self.current_block = merge_block;
                (Value::Local(matched_local), Value::Local(surface_local))
            }
        }
    }

    fn lower_shape_local_point_selection(
        &mut self,
        expr: &hir::ShapeExpr,
        feature_id: Value,
        point: Value,
        feature_path: &mut Vec<SmolStr>,
        span: TextRange,
    ) -> (Value, Value) {
        match expr {
            hir::ShapeExpr::Use { target } => {
                let Some(root) = self.shape_root_expr(target) else {
                    return (Value::Const(Literal::Boolean(false)), point);
                };
                feature_path.push(SmolStr::new(format!("use[{target}]")));
                let result = self.lower_shape_local_point_selection(
                    &root,
                    feature_id,
                    point,
                    feature_path,
                    span,
                );
                feature_path.pop();
                result
            }
            hir::ShapeExpr::Leaf(leaf) => {
                let leaf_feature_id =
                    self.lower_shape_feature_id_value(feature_path, leaf.feature_id);
                let matched = self.lower_binary_temp(
                    MirType::Boolean,
                    BinaryOp::Eq,
                    feature_id,
                    leaf_feature_id,
                    span,
                );
                let local_point =
                    self.new_local(SmolStr::new("$shape_local_point_leaf"), true, MirType::Vec3);
                let local_point_value = self.lower_field_local_point_call(&leaf.field, point, span);
                self.assign_use(Place::Local(local_point), local_point_value, span);
                (matched, Value::Local(local_point))
            }
            hir::ShapeExpr::Union { items, .. } | hir::ShapeExpr::Intersection { items, .. } => {
                let mut iter = items.iter();
                let Some(first) = iter.next() else {
                    return (Value::Const(Literal::Boolean(false)), point);
                };
                let label = match expr {
                    hir::ShapeExpr::Union { .. } => "union",
                    _ => "intersection",
                };
                feature_path.push(SmolStr::new(format!("{label}[0]")));
                let (first_matched, first_point) = self.lower_shape_local_point_selection(
                    first,
                    feature_id.clone(),
                    point.clone(),
                    feature_path,
                    span,
                );
                feature_path.pop();
                let matched_local = self.new_local(
                    SmolStr::new("$shape_local_point_match"),
                    true,
                    MirType::Boolean,
                );
                let point_local =
                    self.new_local(SmolStr::new("$shape_local_point"), true, MirType::Vec3);
                self.assign_use(Place::Local(matched_local), first_matched, span);
                self.assign_use(Place::Local(point_local), first_point, span);
                for (idx, item) in iter.enumerate().map(|(idx, item)| (idx + 1, item)) {
                    feature_path.push(SmolStr::new(format!("{label}[{idx}]")));
                    let (next_matched, next_point) = self.lower_shape_local_point_selection(
                        item,
                        feature_id.clone(),
                        point.clone(),
                        feature_path,
                        span,
                    );
                    feature_path.pop();
                    let already_matched = Value::Local(matched_local);
                    let keep_block = self.new_block();
                    let replace_block = self.new_block();
                    let merge_block = self.new_block();
                    self.set_terminator(Terminator::Branch {
                        cond: already_matched.clone(),
                        then_target: keep_block,
                        else_target: replace_block,
                        span,
                    });
                    self.current_block = keep_block;
                    self.set_terminator(Terminator::Jump {
                        target: merge_block,
                        span,
                    });
                    self.current_block = replace_block;
                    self.assign_use(Place::Local(matched_local), next_matched, span);
                    self.assign_use(Place::Local(point_local), next_point, span);
                    self.set_terminator(Terminator::Jump {
                        target: merge_block,
                        span,
                    });
                    self.current_block = merge_block;
                }
                (Value::Local(matched_local), Value::Local(point_local))
            }
            hir::ShapeExpr::Subtract { left, right, .. } => {
                feature_path.push(SmolStr::new("subtract[left]"));
                let (left_matched, left_point) = self.lower_shape_local_point_selection(
                    left,
                    feature_id.clone(),
                    point.clone(),
                    feature_path,
                    span,
                );
                feature_path.pop();
                let matched_local = self.new_local(
                    SmolStr::new("$shape_local_point_match"),
                    true,
                    MirType::Boolean,
                );
                let point_local =
                    self.new_local(SmolStr::new("$shape_local_point"), true, MirType::Vec3);
                self.assign_use(Place::Local(matched_local), left_matched, span);
                self.assign_use(Place::Local(point_local), left_point, span);
                let already_matched = Value::Local(matched_local);
                let keep_block = self.new_block();
                let replace_block = self.new_block();
                let merge_block = self.new_block();
                self.set_terminator(Terminator::Branch {
                    cond: already_matched.clone(),
                    then_target: keep_block,
                    else_target: replace_block,
                    span,
                });
                self.current_block = keep_block;
                self.set_terminator(Terminator::Jump {
                    target: merge_block,
                    span,
                });
                self.current_block = replace_block;
                feature_path.push(SmolStr::new("subtract[right]"));
                let (right_matched, right_point) = self.lower_shape_local_point_selection(
                    right,
                    feature_id,
                    point,
                    feature_path,
                    span,
                );
                feature_path.pop();
                self.assign_use(Place::Local(matched_local), right_matched, span);
                self.assign_use(Place::Local(point_local), right_point, span);
                self.set_terminator(Terminator::Jump {
                    target: merge_block,
                    span,
                });
                self.current_block = merge_block;
                (Value::Local(matched_local), Value::Local(point_local))
            }
        }
    }

    fn lower_shape_radiance_selection(
        &mut self,
        expr: &hir::ShapeExpr,
        feature_id: Value,
        point: Value,
        direction: Value,
        feature_path: &mut Vec<SmolStr>,
        span: TextRange,
    ) -> (Value, Value) {
        match expr {
            hir::ShapeExpr::Use { target } => {
                let Some(root) = self.shape_root_expr(target) else {
                    let black = self.lower_call_temp(
                        MirType::Vec3,
                        SmolStr::new("vec3"),
                        vec![
                            Value::Const(Literal::Float(0.0)),
                            Value::Const(Literal::Float(0.0)),
                            Value::Const(Literal::Float(0.0)),
                        ],
                        span,
                    );
                    return (Value::Const(Literal::Boolean(false)), black);
                };
                feature_path.push(SmolStr::new(format!("use[{target}]")));
                let result = self.lower_shape_radiance_selection(
                    &root,
                    feature_id,
                    point,
                    direction,
                    feature_path,
                    span,
                );
                feature_path.pop();
                result
            }
            hir::ShapeExpr::Leaf(leaf) => {
                let leaf_feature_id =
                    self.lower_shape_feature_id_value(feature_path, leaf.feature_id);
                let matched = self.lower_binary_temp(
                    MirType::Boolean,
                    BinaryOp::Eq,
                    feature_id.clone(),
                    leaf_feature_id,
                    span,
                );
                let radiance_local =
                    self.new_local(SmolStr::new("$shape_radiance_leaf"), true, MirType::Vec3);
                let default_radiance = self.lower_call_temp(
                    MirType::Vec3,
                    SmolStr::new("vec3"),
                    vec![
                        Value::Const(Literal::Float(0.0)),
                        Value::Const(Literal::Float(0.0)),
                        Value::Const(Literal::Float(0.0)),
                    ],
                    span,
                );
                self.assign_use(Place::Local(radiance_local), default_radiance, span);
                let matched_block = self.new_block();
                let miss_block = self.new_block();
                let merge_block = self.new_block();
                self.set_terminator(Terminator::Branch {
                    cond: matched.clone(),
                    then_target: matched_block,
                    else_target: miss_block,
                    span,
                });
                self.current_block = matched_block;
                if let Some(radiance) = &leaf.radiance {
                    let radiance_value =
                        self.lower_radiance_call(radiance, point, direction, feature_id, span);
                    self.assign_use(Place::Local(radiance_local), radiance_value, span);
                }
                self.set_terminator(Terminator::Jump {
                    target: merge_block,
                    span,
                });
                self.current_block = miss_block;
                self.set_terminator(Terminator::Jump {
                    target: merge_block,
                    span,
                });
                self.current_block = merge_block;
                (matched, Value::Local(radiance_local))
            }
            hir::ShapeExpr::Union { items, .. } | hir::ShapeExpr::Intersection { items, .. } => {
                let mut iter = items.iter();
                let Some(first) = iter.next() else {
                    let black = self.lower_call_temp(
                        MirType::Vec3,
                        SmolStr::new("vec3"),
                        vec![
                            Value::Const(Literal::Float(0.0)),
                            Value::Const(Literal::Float(0.0)),
                            Value::Const(Literal::Float(0.0)),
                        ],
                        span,
                    );
                    return (Value::Const(Literal::Boolean(false)), black);
                };
                let label = match expr {
                    hir::ShapeExpr::Union { .. } => "union",
                    _ => "intersection",
                };
                feature_path.push(SmolStr::new(format!("{label}[0]")));
                let (first_matched, first_radiance) = self.lower_shape_radiance_selection(
                    first,
                    feature_id.clone(),
                    point.clone(),
                    direction.clone(),
                    feature_path,
                    span,
                );
                feature_path.pop();
                let matched_local = self.new_local(
                    SmolStr::new("$shape_radiance_match"),
                    true,
                    MirType::Boolean,
                );
                let radiance_local =
                    self.new_local(SmolStr::new("$shape_radiance"), true, MirType::Vec3);
                self.assign_use(Place::Local(matched_local), first_matched, span);
                self.assign_use(Place::Local(radiance_local), first_radiance, span);
                for (idx, item) in iter.enumerate().map(|(idx, item)| (idx + 1, item)) {
                    feature_path.push(SmolStr::new(format!("{label}[{idx}]")));
                    let (next_matched, next_radiance) = self.lower_shape_radiance_selection(
                        item,
                        feature_id.clone(),
                        point.clone(),
                        direction.clone(),
                        feature_path,
                        span,
                    );
                    feature_path.pop();
                    let already_matched = Value::Local(matched_local);
                    let keep_block = self.new_block();
                    let replace_block = self.new_block();
                    let merge_block = self.new_block();
                    self.set_terminator(Terminator::Branch {
                        cond: already_matched.clone(),
                        then_target: keep_block,
                        else_target: replace_block,
                        span,
                    });
                    self.current_block = keep_block;
                    self.set_terminator(Terminator::Jump {
                        target: merge_block,
                        span,
                    });
                    self.current_block = replace_block;
                    self.assign_use(Place::Local(matched_local), next_matched, span);
                    self.assign_use(Place::Local(radiance_local), next_radiance, span);
                    self.set_terminator(Terminator::Jump {
                        target: merge_block,
                        span,
                    });
                    self.current_block = merge_block;
                }
                (Value::Local(matched_local), Value::Local(radiance_local))
            }
            hir::ShapeExpr::Subtract { left, right, .. } => {
                feature_path.push(SmolStr::new("subtract[left]"));
                let (left_matched, left_radiance) = self.lower_shape_radiance_selection(
                    left,
                    feature_id.clone(),
                    point.clone(),
                    direction.clone(),
                    feature_path,
                    span,
                );
                feature_path.pop();
                let matched_local = self.new_local(
                    SmolStr::new("$shape_radiance_match"),
                    true,
                    MirType::Boolean,
                );
                let radiance_local =
                    self.new_local(SmolStr::new("$shape_radiance"), true, MirType::Vec3);
                self.assign_use(Place::Local(matched_local), left_matched, span);
                self.assign_use(Place::Local(radiance_local), left_radiance, span);
                let already_matched = Value::Local(matched_local);
                let keep_block = self.new_block();
                let replace_block = self.new_block();
                let merge_block = self.new_block();
                self.set_terminator(Terminator::Branch {
                    cond: already_matched.clone(),
                    then_target: keep_block,
                    else_target: replace_block,
                    span,
                });
                self.current_block = keep_block;
                self.set_terminator(Terminator::Jump {
                    target: merge_block,
                    span,
                });
                self.current_block = replace_block;
                feature_path.push(SmolStr::new("subtract[right]"));
                let (right_matched, right_radiance) = self.lower_shape_radiance_selection(
                    right,
                    feature_id,
                    point,
                    direction,
                    feature_path,
                    span,
                );
                feature_path.pop();
                self.assign_use(Place::Local(matched_local), right_matched, span);
                self.assign_use(Place::Local(radiance_local), right_radiance, span);
                self.set_terminator(Terminator::Jump {
                    target: merge_block,
                    span,
                });
                self.current_block = merge_block;
                (Value::Local(matched_local), Value::Local(radiance_local))
            }
        }
    }

    fn lower_shape_medium_selection(
        &mut self,
        expr: &hir::ShapeExpr,
        feature_id: Value,
        point: Value,
        surface_distance: Value,
        feature_path: &mut Vec<SmolStr>,
        span: TextRange,
    ) -> (Value, Value) {
        match expr {
            hir::ShapeExpr::Use { target } => {
                let Some(root) = self.shape_root_expr(target) else {
                    return (
                        Value::Const(Literal::Boolean(false)),
                        self.build_default_medium(span),
                    );
                };
                feature_path.push(SmolStr::new(format!("use[{target}]")));
                let result = self.lower_shape_medium_selection(
                    &root,
                    feature_id,
                    point,
                    surface_distance,
                    feature_path,
                    span,
                );
                feature_path.pop();
                result
            }
            hir::ShapeExpr::Leaf(leaf) => {
                let leaf_feature_id =
                    self.lower_shape_feature_id_value(feature_path, leaf.feature_id);
                let matched = self.lower_binary_temp(
                    MirType::Boolean,
                    BinaryOp::Eq,
                    feature_id.clone(),
                    leaf_feature_id,
                    span,
                );
                let medium_local = self.new_local(
                    SmolStr::new("$shape_medium_leaf"),
                    true,
                    MirType::Named(SmolStr::new("Medium")),
                );
                let default_medium = self.build_default_medium(span);
                self.assign_use(Place::Local(medium_local), default_medium, span);
                let matched_block = self.new_block();
                let miss_block = self.new_block();
                let merge_block = self.new_block();
                self.set_terminator(Terminator::Branch {
                    cond: matched.clone(),
                    then_target: matched_block,
                    else_target: miss_block,
                    span,
                });
                self.current_block = matched_block;
                if let Some(volume) = &leaf.volume {
                    let medium_value =
                        self.lower_volume_call(volume, point, surface_distance, span);
                    self.assign_use(Place::Local(medium_local), medium_value, span);
                }
                self.set_terminator(Terminator::Jump {
                    target: merge_block,
                    span,
                });
                self.current_block = miss_block;
                self.set_terminator(Terminator::Jump {
                    target: merge_block,
                    span,
                });
                self.current_block = merge_block;
                (matched, Value::Local(medium_local))
            }
            hir::ShapeExpr::Union { items, .. } | hir::ShapeExpr::Intersection { items, .. } => {
                let mut iter = items.iter();
                let Some(first) = iter.next() else {
                    return (
                        Value::Const(Literal::Boolean(false)),
                        self.build_default_medium(span),
                    );
                };
                let label = match expr {
                    hir::ShapeExpr::Union { .. } => "union",
                    _ => "intersection",
                };
                feature_path.push(SmolStr::new(format!("{label}[0]")));
                let (first_matched, first_medium) = self.lower_shape_medium_selection(
                    first,
                    feature_id.clone(),
                    point.clone(),
                    surface_distance.clone(),
                    feature_path,
                    span,
                );
                feature_path.pop();
                let matched_local =
                    self.new_local(SmolStr::new("$shape_medium_match"), true, MirType::Boolean);
                let medium_local = self.new_local(
                    SmolStr::new("$shape_medium"),
                    true,
                    MirType::Named(SmolStr::new("Medium")),
                );
                self.assign_use(Place::Local(matched_local), first_matched, span);
                self.assign_use(Place::Local(medium_local), first_medium, span);
                for (idx, item) in iter.enumerate().map(|(idx, item)| (idx + 1, item)) {
                    feature_path.push(SmolStr::new(format!("{label}[{idx}]")));
                    let (next_matched, next_medium) = self.lower_shape_medium_selection(
                        item,
                        feature_id.clone(),
                        point.clone(),
                        surface_distance.clone(),
                        feature_path,
                        span,
                    );
                    feature_path.pop();
                    let already_matched = Value::Local(matched_local);
                    let keep_block = self.new_block();
                    let replace_block = self.new_block();
                    let merge_block = self.new_block();
                    self.set_terminator(Terminator::Branch {
                        cond: already_matched.clone(),
                        then_target: keep_block,
                        else_target: replace_block,
                        span,
                    });
                    self.current_block = keep_block;
                    self.set_terminator(Terminator::Jump {
                        target: merge_block,
                        span,
                    });
                    self.current_block = replace_block;
                    self.assign_use(Place::Local(matched_local), next_matched, span);
                    self.assign_use(Place::Local(medium_local), next_medium, span);
                    self.set_terminator(Terminator::Jump {
                        target: merge_block,
                        span,
                    });
                    self.current_block = merge_block;
                }
                (Value::Local(matched_local), Value::Local(medium_local))
            }
            hir::ShapeExpr::Subtract { left, right, .. } => {
                feature_path.push(SmolStr::new("subtract[left]"));
                let (left_matched, left_medium) = self.lower_shape_medium_selection(
                    left,
                    feature_id.clone(),
                    point.clone(),
                    surface_distance.clone(),
                    feature_path,
                    span,
                );
                feature_path.pop();
                let matched_local =
                    self.new_local(SmolStr::new("$shape_medium_match"), true, MirType::Boolean);
                let medium_local = self.new_local(
                    SmolStr::new("$shape_medium"),
                    true,
                    MirType::Named(SmolStr::new("Medium")),
                );
                self.assign_use(Place::Local(matched_local), left_matched, span);
                self.assign_use(Place::Local(medium_local), left_medium, span);
                let already_matched = Value::Local(matched_local);
                let keep_block = self.new_block();
                let replace_block = self.new_block();
                let merge_block = self.new_block();
                self.set_terminator(Terminator::Branch {
                    cond: already_matched.clone(),
                    then_target: keep_block,
                    else_target: replace_block,
                    span,
                });
                self.current_block = keep_block;
                self.set_terminator(Terminator::Jump {
                    target: merge_block,
                    span,
                });
                self.current_block = replace_block;
                feature_path.push(SmolStr::new("subtract[right]"));
                let (right_matched, right_medium) = self.lower_shape_medium_selection(
                    right,
                    feature_id,
                    point,
                    surface_distance,
                    feature_path,
                    span,
                );
                feature_path.pop();
                self.assign_use(Place::Local(matched_local), right_matched, span);
                self.assign_use(Place::Local(medium_local), right_medium, span);
                self.set_terminator(Terminator::Jump {
                    target: merge_block,
                    span,
                });
                self.current_block = merge_block;
                (Value::Local(matched_local), Value::Local(medium_local))
            }
        }
    }

    fn lower_field_local_point_call(
        &mut self,
        field: &SmolStr,
        point: Value,
        span: TextRange,
    ) -> Value {
        let Some(root) = self.field_root_expr(field) else {
            return point;
        };
        let Some(body) = self.field_body(field).cloned() else {
            return point;
        };
        self.lower_field_local_point_expr(&root, &body, point, span)
    }

    fn lower_field_local_point_expr(
        &mut self,
        expr: &hir::FieldExpr,
        body: &hir::Body,
        point: Value,
        span: TextRange,
    ) -> Value {
        match expr {
            hir::FieldExpr::Use { target } => {
                self.lower_field_local_point_call(target, point, span)
            }
            hir::FieldExpr::Translate {
                translate,
                body: inner,
            } => {
                let local_point =
                    self.lower_wrapped_support_point("translate", "offset", translate, point, span);
                self.lower_field_local_point_expr(inner, body, local_point, span)
            }
            hir::FieldExpr::Rotate {
                rotate,
                body: inner,
            } => {
                let local_point =
                    self.lower_wrapped_support_point("rotate", "rotation", rotate, point, span);
                self.lower_field_local_point_expr(inner, body, local_point, span)
            }
            hir::FieldExpr::UniformScale { scale, body: inner } => {
                let wrapper_value = self.lower_wrapped_body_value(scale, span);
                let local_point = self.lower_call_temp(
                    MirType::Vec3,
                    SmolStr::new("uniform_scale"),
                    vec![wrapper_value, point],
                    span,
                );
                self.lower_field_local_point_expr(inner, body, local_point, span)
            }
            hir::FieldExpr::AffineTransform {
                transform,
                body: inner,
            } => {
                let local_point = self.lower_wrapped_support_point(
                    "affine_transform",
                    "transform",
                    transform,
                    point,
                    span,
                );
                self.lower_field_local_point_expr(inner, body, local_point, span)
            }
            hir::FieldExpr::Warp { warp, body: inner } => {
                let local_point =
                    self.lower_wrapped_support_point("warp", "warp", warp, point, span);
                self.lower_field_local_point_expr(inner, body, local_point, span)
            }
            hir::FieldExpr::RepeatLinear {
                repeat,
                body: inner,
            } => {
                let local_point = self.lower_wrapped_support_point(
                    "repeat_linear",
                    "period",
                    repeat,
                    point,
                    span,
                );
                self.lower_field_local_point_expr(inner, body, local_point, span)
            }
            hir::FieldExpr::RepeatGrid {
                repeat,
                body: inner,
            } => {
                let local_point =
                    self.lower_wrapped_support_point("repeat_grid", "period", repeat, point, span);
                self.lower_field_local_point_expr(inner, body, local_point, span)
            }
            hir::FieldExpr::RadialRepeat {
                radial,
                body: inner,
            } => {
                let local_point = self.lower_wrapped_support_point(
                    "radial_repeat",
                    "radial",
                    radial,
                    point,
                    span,
                );
                self.lower_field_local_point_expr(inner, body, local_point, span)
            }
            hir::FieldExpr::MirrorArray {
                mirror,
                body: inner,
            } => {
                let local_point =
                    self.lower_wrapped_support_point("mirror_array", "mirror", mirror, point, span);
                self.lower_field_local_point_expr(inner, body, local_point, span)
            }
            hir::FieldExpr::InstanceArray {
                instance,
                body: inner,
            } => {
                let local_point = self.lower_wrapped_support_point(
                    "instance_array",
                    "instance",
                    instance,
                    point,
                    span,
                );
                self.lower_field_local_point_expr(inner, body, local_point, span)
            }
            hir::FieldExpr::Bend { bend, body: inner } => {
                let local_point =
                    self.lower_wrapped_support_point("bend", "bend", bend, point, span);
                self.lower_field_local_point_expr(inner, body, local_point, span)
            }
            hir::FieldExpr::Twist { twist, body: inner } => {
                let local_point =
                    self.lower_wrapped_support_point("twist", "twist", twist, point, span);
                self.lower_field_local_point_expr(inner, body, local_point, span)
            }
            hir::FieldExpr::Taper { taper, body: inner } => {
                let local_point =
                    self.lower_wrapped_support_point("taper", "taper", taper, point, span);
                self.lower_field_local_point_expr(inner, body, local_point, span)
            }
            hir::FieldExpr::Displace {
                displace,
                body: inner,
            } => {
                let local_point =
                    self.lower_wrapped_support_point("displace", "displace", displace, point, span);
                self.lower_field_local_point_expr(inner, body, local_point, span)
            }
            _ => point,
        }
    }

    fn lower_radiance_call(
        &mut self,
        radiance: &SmolStr,
        point: Value,
        direction: Value,
        feature_id: Value,
        span: TextRange,
    ) -> Value {
        match self
            .radiance_param_counts
            .get(radiance)
            .copied()
            .unwrap_or(1)
        {
            1 => self.lower_call_temp(MirType::Vec3, radiance.clone(), vec![point], span),
            2 => self.lower_call_temp(
                MirType::Vec3,
                radiance.clone(),
                vec![point, direction],
                span,
            ),
            _ => self.lower_call_temp(
                MirType::Vec3,
                radiance.clone(),
                vec![point, direction, feature_id],
                span,
            ),
        }
    }

    fn lower_volume_call(
        &mut self,
        volume: &SmolStr,
        point: Value,
        surface_distance: Value,
        span: TextRange,
    ) -> Value {
        match self.volume_param_counts.get(volume).copied().unwrap_or(1) {
            1 => self.lower_call_temp(
                MirType::Named(SmolStr::new("Medium")),
                volume.clone(),
                vec![point],
                span,
            ),
            _ => self.lower_call_temp(
                MirType::Named(SmolStr::new("Medium")),
                volume.clone(),
                vec![point, surface_distance],
                span,
            ),
        }
    }

    fn lower_get_named_field(
        &mut self,
        base: Value,
        type_name: &str,
        field: &str,
        ty: MirType,
        span: TextRange,
    ) -> Value {
        let temp = self.new_temp(ty);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(temp),
            value: Rvalue::GetField {
                base,
                field: SmolStr::new(field),
                slot: self.field_slot(type_name, field),
            },
            span,
        });
        Value::Temp(temp)
    }

    fn lower_field_distance_call(
        &mut self,
        field: &SmolStr,
        point: Value,
        span: TextRange,
    ) -> Value {
        let _ = self.lower_call_temp(
            MirType::Nil,
            SmolStr::new("__wr_metrics_field_sample"),
            vec![],
            span,
        );
        let temp = self.new_temp(MirType::Float);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(temp),
            value: Rvalue::Call {
                kind: CallKind::Sync,
                target: CallTarget::Function(field.clone()),
                args: vec![point],
            },
            span,
        });
        Value::Temp(temp)
    }

    fn lower_field_normal_call(&mut self, field: &SmolStr, point: Value, span: TextRange) -> Value {
        let dx = self.lower_field_axis_difference(field, point.clone(), [0.001, 0.0, 0.0], span);
        let dy = self.lower_field_axis_difference(field, point.clone(), [0.0, 0.001, 0.0], span);
        let dz = self.lower_field_axis_difference(field, point, [0.0, 0.0, 0.001], span);

        let gradient = self.new_temp(MirType::Vec3);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(gradient),
            value: Rvalue::Call {
                kind: CallKind::Sync,
                target: CallTarget::Function(SmolStr::new("vec3")),
                args: vec![dx, dy, dz],
            },
            span,
        });

        let normal = self.new_temp(MirType::Vec3);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(normal),
            value: Rvalue::Call {
                kind: CallKind::Sync,
                target: CallTarget::Function(SmolStr::new("normalize")),
                args: vec![Value::Temp(gradient)],
            },
            span,
        });
        Value::Temp(normal)
    }

    fn lower_field_axis_difference(
        &mut self,
        field: &SmolStr,
        point: Value,
        offset: [f64; 3],
        span: TextRange,
    ) -> Value {
        let plus_point = self.lower_offset_point(point.clone(), offset, span);
        let plus = self.lower_field_distance_call(field, plus_point, span);
        let minus_point =
            self.lower_offset_point(point, [-offset[0], -offset[1], -offset[2]], span);
        let minus = self.lower_field_distance_call(field, minus_point, span);
        let diff = self.new_temp(MirType::Float);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(diff),
            value: Rvalue::Binary {
                op: BinaryOp::Sub,
                lhs: plus,
                rhs: minus,
            },
            span,
        });
        Value::Temp(diff)
    }

    fn lower_offset_point(&mut self, point: Value, offset: [f64; 3], span: TextRange) -> Value {
        let offset_vec = self.new_temp(MirType::Vec3);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(offset_vec),
            value: Rvalue::Call {
                kind: CallKind::Sync,
                target: CallTarget::Function(SmolStr::new("vec3")),
                args: vec![
                    Value::Const(Literal::Float(offset[0])),
                    Value::Const(Literal::Float(offset[1])),
                    Value::Const(Literal::Float(offset[2])),
                ],
            },
            span,
        });
        let shifted = self.new_temp(MirType::Vec3);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(shifted),
            value: Rvalue::Binary {
                op: BinaryOp::Add,
                lhs: point,
                rhs: Value::Temp(offset_vec),
            },
            span,
        });
        Value::Temp(shifted)
    }

    fn parse_dispatch_compute(
        &self,
        body: &hir::Body,
        expr_id: hir::Idx<Expr>,
    ) -> Option<ComputeDispatchSpec> {
        let (callee, args) = match &body.exprs[expr_id] {
            Expr::Call { callee, args, .. } => (callee, args),
            _ => return None,
        };
        let Expr::Variable(name) = &body.exprs[*callee] else {
            return None;
        };
        if name.as_str() != "dispatch_compute" {
            return None;
        }
        let mut kernel = None;
        let mut workgroups_x = None;
        let mut workgroups_y = None;
        let mut workgroups_z = None;
        let mut workgroup_size_x = None;
        let mut workgroup_size_y = None;
        let mut workgroup_size_z = None;
        let mut schedule = None;
        let mut kernel_args = Vec::new();
        for arg in args {
            match arg {
                hir::Arg::Positional { value, .. } => kernel_args.push(*value),
                hir::Arg::Named { name, value, .. } => match name.as_str() {
                    "kernel" => {
                        if let Expr::Variable(func_name) = &body.exprs[*value] {
                            kernel = Some(func_name.clone());
                        } else {
                            return None;
                        }
                    }
                    "workgroups_x" => workgroups_x = Some(*value),
                    "workgroups_y" => workgroups_y = Some(*value),
                    "workgroups_z" => workgroups_z = Some(*value),
                    "workgroup_size_x" => workgroup_size_x = Some(*value),
                    "workgroup_size_y" => workgroup_size_y = Some(*value),
                    "workgroup_size_z" => workgroup_size_z = Some(*value),
                    "schedule" => schedule = Some(*value),
                    _ => kernel_args.push(*value),
                },
            }
        }
        Some(ComputeDispatchSpec {
            kernel: kernel?,
            workgroups_x: workgroups_x?,
            workgroups_y: workgroups_y?,
            workgroups_z: workgroups_z?,
            workgroup_size_x: workgroup_size_x?,
            workgroup_size_y: workgroup_size_y?,
            workgroup_size_z: workgroup_size_z?,
            schedule,
            kernel_args,
        })
    }

    fn lower_dispatch_compute_call(
        &mut self,
        body: &hir::Body,
        span: TextRange,
        spec: &ComputeDispatchSpec,
    ) -> Value {
        let workgroups_x = self.lower_expr(body, spec.workgroups_x);
        let workgroups_y = self.lower_expr(body, spec.workgroups_y);
        let workgroups_z = self.lower_expr(body, spec.workgroups_z);
        let workgroup_size_x = self.lower_expr(body, spec.workgroup_size_x);
        let workgroup_size_y = self.lower_expr(body, spec.workgroup_size_y);
        let workgroup_size_z = self.lower_expr(body, spec.workgroup_size_z);
        let schedule = spec
            .schedule
            .map(|expr| self.lower_expr(body, expr))
            .unwrap_or(Value::Const(Literal::Nil));
        let kernel_args = spec
            .kernel_args
            .iter()
            .map(|expr| self.lower_expr(body, *expr))
            .collect::<Vec<_>>();

        let workgroups_x_local = self.new_local(
            SmolStr::new(format!("$gpu_workgroups_x{}", self.locals.len())),
            false,
            MirType::Integer,
        );
        let workgroups_y_local = self.new_local(
            SmolStr::new(format!("$gpu_workgroups_y{}", self.locals.len())),
            false,
            MirType::Integer,
        );
        let workgroups_z_local = self.new_local(
            SmolStr::new(format!("$gpu_workgroups_z{}", self.locals.len())),
            false,
            MirType::Integer,
        );
        let workgroup_size_x_local = self.new_local(
            SmolStr::new(format!("$gpu_workgroup_size_x{}", self.locals.len())),
            false,
            MirType::Integer,
        );
        let workgroup_size_y_local = self.new_local(
            SmolStr::new(format!("$gpu_workgroup_size_y{}", self.locals.len())),
            false,
            MirType::Integer,
        );
        let workgroup_size_z_local = self.new_local(
            SmolStr::new(format!("$gpu_workgroup_size_z{}", self.locals.len())),
            false,
            MirType::Integer,
        );
        for (local, value) in [
            (workgroups_x_local, workgroups_x),
            (workgroups_y_local, workgroups_y),
            (workgroups_z_local, workgroups_z),
            (workgroup_size_x_local, workgroup_size_x),
            (workgroup_size_y_local, workgroup_size_y),
            (workgroup_size_z_local, workgroup_size_z),
        ] {
            self.push_stmt(MirStmt::Assign {
                place: Place::Local(local),
                value: Rvalue::Use(value),
                span,
            });
        }

        let total_x_local = self.new_local(
            SmolStr::new(format!("$gpu_total_x{}", self.locals.len())),
            false,
            MirType::Integer,
        );
        let total_y_local = self.new_local(
            SmolStr::new(format!("$gpu_total_y{}", self.locals.len())),
            false,
            MirType::Integer,
        );
        let total_z_local = self.new_local(
            SmolStr::new(format!("$gpu_total_z{}", self.locals.len())),
            false,
            MirType::Integer,
        );
        let total_xy_local = self.new_local(
            SmolStr::new(format!("$gpu_total_xy{}", self.locals.len())),
            false,
            MirType::Integer,
        );
        let total_count_local = self.new_local(
            SmolStr::new(format!("$gpu_total_count{}", self.locals.len())),
            false,
            MirType::Integer,
        );
        for (local, lhs, rhs) in [
            (
                total_x_local,
                Value::Local(workgroups_x_local),
                Value::Local(workgroup_size_x_local),
            ),
            (
                total_y_local,
                Value::Local(workgroups_y_local),
                Value::Local(workgroup_size_y_local),
            ),
            (
                total_z_local,
                Value::Local(workgroups_z_local),
                Value::Local(workgroup_size_z_local),
            ),
        ] {
            let temp = self.new_temp(MirType::Integer);
            self.push_stmt(MirStmt::Assign {
                place: Place::Temp(temp),
                value: Rvalue::Binary {
                    op: BinaryOp::Mul,
                    lhs,
                    rhs,
                },
                span,
            });
            self.push_stmt(MirStmt::Assign {
                place: Place::Local(local),
                value: Rvalue::Use(Value::Temp(temp)),
                span,
            });
        }
        let total_xy_temp = self.new_temp(MirType::Integer);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(total_xy_temp),
            value: Rvalue::Binary {
                op: BinaryOp::Mul,
                lhs: Value::Local(total_x_local),
                rhs: Value::Local(total_y_local),
            },
            span,
        });
        self.push_stmt(MirStmt::Assign {
            place: Place::Local(total_xy_local),
            value: Rvalue::Use(Value::Temp(total_xy_temp)),
            span,
        });
        let total_count_temp = self.new_temp(MirType::Integer);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(total_count_temp),
            value: Rvalue::Binary {
                op: BinaryOp::Mul,
                lhs: Value::Local(total_xy_local),
                rhs: Value::Local(total_z_local),
            },
            span,
        });
        self.push_stmt(MirStmt::Assign {
            place: Place::Local(total_count_local),
            value: Rvalue::Use(Value::Temp(total_count_temp)),
            span,
        });

        let dispatch_begin = self.new_temp(MirType::Nil);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(dispatch_begin),
            value: Rvalue::Call {
                kind: CallKind::Sync,
                target: CallTarget::Function(SmolStr::new("__wr_gpu_dispatch_begin")),
                args: vec![
                    Value::Local(workgroups_x_local),
                    Value::Local(workgroups_y_local),
                    Value::Local(workgroups_z_local),
                    Value::Local(workgroup_size_x_local),
                    Value::Local(workgroup_size_y_local),
                    Value::Local(workgroup_size_z_local),
                    schedule,
                ],
            },
            span,
        });

        let loop_index_local = self.new_local(
            SmolStr::new(format!("$gpu_linear_index{}", self.locals.len())),
            true,
            MirType::Integer,
        );
        self.push_stmt(MirStmt::Assign {
            place: Place::Local(loop_index_local),
            value: Rvalue::Use(Value::Const(Literal::Integer(0))),
            span,
        });

        let head_block = self.new_block();
        let body_block = self.new_block();
        let exit_block = self.new_block();
        self.set_terminator(Terminator::Jump {
            target: head_block,
            span,
        });

        self.current_block = head_block;
        let cond_temp = self.new_temp(MirType::Boolean);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(cond_temp),
            value: Rvalue::Binary {
                op: BinaryOp::Lt,
                lhs: Value::Local(loop_index_local),
                rhs: Value::Local(total_count_local),
            },
            span,
        });
        self.set_terminator(Terminator::Branch {
            cond: Value::Temp(cond_temp),
            then_target: body_block,
            else_target: exit_block,
            span,
        });

        self.current_block = body_block;
        let dispatch_select = self.new_temp(MirType::Nil);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(dispatch_select),
            value: Rvalue::Call {
                kind: CallKind::Sync,
                target: CallTarget::Function(SmolStr::new("__wr_gpu_dispatch_select_invocation")),
                args: vec![Value::Local(loop_index_local)],
            },
            span,
        });

        let kernel_result = self.new_temp(MirType::Nil);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(kernel_result),
            value: Rvalue::Call {
                kind: CallKind::Sync,
                target: CallTarget::Function(spec.kernel.clone()),
                args: kernel_args,
            },
            span,
        });

        let next_index = self.new_temp(MirType::Integer);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(next_index),
            value: Rvalue::Binary {
                op: BinaryOp::Add,
                lhs: Value::Local(loop_index_local),
                rhs: Value::Const(Literal::Integer(1)),
            },
            span,
        });
        self.push_stmt(MirStmt::Assign {
            place: Place::Local(loop_index_local),
            value: Rvalue::Use(Value::Temp(next_index)),
            span,
        });
        self.set_terminator(Terminator::Jump {
            target: head_block,
            span,
        });

        self.current_block = exit_block;
        let dispatch_end = self.new_temp(MirType::Nil);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(dispatch_end),
            value: Rvalue::Call {
                kind: CallKind::Sync,
                target: CallTarget::Function(SmolStr::new("__wr_gpu_dispatch_end")),
                args: Vec::new(),
            },
            span,
        });
        Value::Const(Literal::Nil)
    }

    fn lower_await(&mut self, body: &hir::Body, expr_id: hir::Idx<Expr>, span: TextRange) -> Value {
        let pending = self.lower_pending_call_or_value(body, expr_id, span);
        let temp = self.new_temp_for_expr(expr_id);
        self.push_stmt(MirStmt::Await {
            dst: Place::Temp(temp),
            pending,
            span,
        });
        Value::Temp(temp)
    }

    fn lower_pending_call_or_value(
        &mut self,
        body: &hir::Body,
        expr_id: hir::Idx<Expr>,
        span: TextRange,
    ) -> Value {
        if let Expr::Call { callee, args, .. } = &body.exprs[expr_id] {
            let kind = if self.is_actor_call(body, *callee) {
                CallKind::Actor
            } else {
                CallKind::Sync
            };
            let (target, args) = self.lower_call_target(body, *callee, args);
            let temp = self.new_temp_for_expr(expr_id);
            self.push_stmt(MirStmt::Assign {
                place: Place::Temp(temp),
                value: Rvalue::Call { kind, target, args },
                span,
            });
            Value::Temp(temp)
        } else {
            self.lower_expr(body, expr_id)
        }
    }

    fn is_actor_call(&self, body: &hir::Body, callee: hir::Idx<Expr>) -> bool {
        if let Expr::Member { object, .. } = &body.exprs[callee] {
            matches!(self.expr_type(*object), MirType::Actor(_))
        } else {
            false
        }
    }

    fn lower_call_target(
        &mut self,
        body: &hir::Body,
        callee: hir::Idx<Expr>,
        args: &[hir::Arg],
    ) -> (CallTarget, Vec<Value>) {
        let mut values = Vec::with_capacity(args.len());
        for arg in args {
            match arg {
                hir::Arg::Positional { value, .. } => {
                    values.push(self.lower_expr(body, *value));
                }
                hir::Arg::Named { value, .. } => {
                    values.push(self.lower_expr(body, *value));
                }
            }
        }

        match &body.exprs[callee] {
            Expr::Member { object, member, .. } => {
                let receiver = self.lower_expr(body, *object);
                let collection_intrinsic = match self.expr_type(*object) {
                    MirType::Named(name) if name.as_str() == "List" => match member.as_str() {
                        "push" => Some("__wr_list_push"),
                        "len" => Some("__wr_list_len"),
                        _ => None,
                    },
                    MirType::Named(name) if name.as_str() == "Map" => match member.as_str() {
                        "set" => Some("__wr_map_set"),
                        "get" => Some("__wr_map_get"),
                        "len" => Some("__wr_map_len"),
                        _ => None,
                    },
                    _ => None,
                };
                let collection_intrinsic = collection_intrinsic.or_else(|| match member.as_str() {
                    "push" => Some("__wr_list_push"),
                    "set" => Some("__wr_map_set"),
                    "get" => Some("__wr_map_get"),
                    "len" => {
                        if matches!(&body.exprs[*object], Expr::List(_)) {
                            Some("__wr_list_len")
                        } else {
                            Some("__wr_map_len")
                        }
                    }
                    _ => None,
                });
                if let Some(intrinsic) = collection_intrinsic {
                    let mut intrinsic_args = Vec::with_capacity(values.len() + 1);
                    intrinsic_args.push(receiver);
                    intrinsic_args.extend(values);
                    return (
                        CallTarget::Function(SmolStr::new(intrinsic)),
                        intrinsic_args,
                    );
                }
                let class_hint = match &body.exprs[*object] {
                    Expr::Variable(name) if self.type_tags.contains_key(name) => Some(name.clone()),
                    _ => None,
                };
                if let MirType::Named(class_name) = self.expr_type(*object)
                    && let Some(methods) = self.interface_methods.get(&class_name)
                    && methods.contains(member)
                {
                    let mut args_with_recv = Vec::with_capacity(values.len() + 1);
                    args_with_recv.push(receiver.clone());
                    args_with_recv.extend(values);
                    let func_name = SmolStr::new(format!("{}.{}", class_name, member));
                    return (CallTarget::Function(func_name), args_with_recv);
                }
                let (method_id, method_name) = match self.expr_type(*object) {
                    MirType::Actor(inner) => {
                        if let MirType::Named(class_name) = *inner {
                            (
                                self.method_id_for(&class_name, member),
                                SmolStr::new(format!("{}.{}", class_name, member)),
                            )
                        } else {
                            (None, member.clone())
                        }
                    }
                    MirType::Named(class_name) => (
                        self.method_id_for(&class_name, member),
                        SmolStr::new(format!("{}.{}", class_name, member)),
                    ),
                    _ => {
                        if let Some(class_name) = class_hint {
                            (
                                self.method_id_for(&class_name, member),
                                SmolStr::new(format!("{}.{}", class_name, member)),
                            )
                        } else {
                            (None, member.clone())
                        }
                    }
                };
                if method_id.is_none()
                    && !method_name.as_str().contains('.')
                    && let Some(interface_dispatch_target) =
                        self.resolve_unique_interface_dispatch_target(member)
                {
                    let mut args_with_recv = Vec::with_capacity(values.len() + 1);
                    args_with_recv.push(receiver.clone());
                    args_with_recv.extend(values);
                    return (
                        CallTarget::Function(interface_dispatch_target),
                        args_with_recv,
                    );
                }
                (
                    CallTarget::Method {
                        receiver,
                        method: method_name,
                        method_id,
                    },
                    values,
                )
            }
            Expr::Variable(name) if self.function_names.contains(name) => {
                let mut call_args = values;
                if matches!(
                    name.as_str(),
                    "transform3_identity" | "compose_transform3" | "inverse_transform3"
                ) && let Some(class_id) = self.type_tags.get(&SmolStr::new("Transform3"))
                {
                    call_args.insert(0, Value::Const(Literal::Integer(class_id.0 as i64)));
                }
                if name.as_str() == "assert" && call_args.len() == 1 {
                    call_args.push(Value::Const(Literal::Nil));
                }
                (CallTarget::Function(name.clone()), call_args)
            }
            _ => {
                let callee_value = self.lower_expr(body, callee);
                (CallTarget::Indirect(callee_value), values)
            }
        }
    }

    fn method_id_for(&self, class_name: &SmolStr, method: &SmolStr) -> Option<u32> {
        self.class_method_ids
            .get(class_name)
            .and_then(|methods| methods.get(method).copied())
    }

    fn resolve_unique_interface_dispatch_target(&self, method: &SmolStr) -> Option<SmolStr> {
        let mut matched_interface: Option<&SmolStr> = None;
        for (interface_name, methods) in &self.interface_methods {
            if !methods.contains(method) {
                continue;
            }
            if matched_interface.is_some() {
                return None;
            }
            matched_interface = Some(interface_name);
        }
        matched_interface.map(|interface_name| SmolStr::new(format!("{interface_name}.{method}")))
    }

    fn member_slot_hint(&self, object_expr: hir::Idx<Expr>, member: &SmolStr) -> Option<u32> {
        let MirType::Named(class_name) = self.expr_type(object_expr) else {
            return None;
        };
        self.class_fields
            .get(&class_name)
            .and_then(|fields| fields.iter().position(|field| field == member))
            .map(|idx| idx as u32)
    }

    fn resolve_class_init_target(
        &self,
        body: &hir::Body,
        callee: hir::Idx<Expr>,
    ) -> Option<(SmolStr, TypeTagId)> {
        match &body.exprs[callee] {
            Expr::Variable(name) => {
                let class_name = builtin_record_by_function(name.as_str())
                    .map(|record| SmolStr::new(record.name))
                    .unwrap_or_else(|| name.clone());
                self.type_tags
                    .get(&class_name)
                    .copied()
                    .map(|id| (class_name, id))
            }
            Expr::Member { object, member, .. } => {
                let enum_name = match &body.exprs[*object] {
                    Expr::Variable(name) => Some(name.clone()),
                    Expr::TypeApply { callee, .. } => match &body.exprs[*callee] {
                        Expr::Variable(name) => Some(name.clone()),
                        _ => None,
                    },
                    _ => None,
                };
                let enum_name = enum_name?;
                let class_name = SmolStr::new(format!("{}.{}", enum_name, member));
                self.type_tags
                    .get(&class_name)
                    .copied()
                    .map(|id| (class_name, id))
            }
            _ => None,
        }
    }
    fn is_default_match_pattern(&self, pattern: &hir::Pattern) -> bool {
        match pattern {
            hir::Pattern::Wildcard => true,
            hir::Pattern::Binding(name) => {
                !self.type_tags.contains_key(name) && builtin_type_tag(name).is_none()
            }
            _ => false,
        }
    }
}

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

struct PoolOfSpec {
    class_expr: hir::Idx<Expr>,
    size: Option<hir::PoolSize>,
    objective: Option<hir::Objective>,
    config: SpawnConfig,
    min_size: Option<i64>,
    max_size: Option<i64>,
    weight: Option<i64>,
    queue_cap: Option<i64>,
}

struct ClassTargetInfo {
    name: SmolStr,
    class_id: TypeTagId,
    fields: Vec<SmolStr>,
    field_defaults: Vec<Option<hir::FieldDefault>>,
    field_values: Vec<Option<Value>>,
}

struct ComputeDispatchSpec {
    kernel: SmolStr,
    workgroups_x: hir::Idx<Expr>,
    workgroups_y: hir::Idx<Expr>,
    workgroups_z: hir::Idx<Expr>,
    workgroup_size_x: hir::Idx<Expr>,
    workgroup_size_y: hir::Idx<Expr>,
    workgroup_size_z: hir::Idx<Expr>,
    schedule: Option<hir::Idx<Expr>>,
    kernel_args: Vec<hir::Idx<Expr>>,
}

#[derive(Clone, Copy)]
enum FieldQueryKind {
    Distance,
    Normal,
    Radiance,
    Medium,
}

struct FieldQuerySpec {
    kind: FieldQueryKind,
    capture: hir::Idx<Expr>,
    point: hir::Idx<Expr>,
    direction: Option<hir::Idx<Expr>>,
}

#[derive(Clone, Copy)]
enum ShapeQueryKind {
    Trace,
    Surface,
}

struct ShapeQuerySpec {
    kind: ShapeQueryKind,
    capture: hir::Idx<Expr>,
    origin: Option<hir::Idx<Expr>>,
    direction: Option<hir::Idx<Expr>>,
    max_distance: Option<hir::Idx<Expr>>,
    min_step: Option<hir::Idx<Expr>>,
    hit_epsilon: Option<hir::Idx<Expr>>,
    max_steps: Option<hir::Idx<Expr>>,
    hit: Option<hir::Idx<Expr>>,
}

#[derive(Clone, Copy)]
enum WorldPointQueryKind {
    Distance,
    Normal,
    Radiance,
    Medium,
}

struct WorldPointQuerySpec {
    kind: WorldPointQueryKind,
    capture: hir::Idx<Expr>,
    domain: hir::Idx<Expr>,
    point: hir::Idx<Expr>,
    direction: Option<hir::Idx<Expr>>,
}

#[derive(Clone, Copy)]
enum WorldShapeQueryKind {
    Trace,
    Surface,
}

struct WorldShapeQuerySpec {
    kind: WorldShapeQueryKind,
    capture: hir::Idx<Expr>,
    domain: hir::Idx<Expr>,
    origin: Option<hir::Idx<Expr>>,
    direction: Option<hir::Idx<Expr>>,
    max_distance: Option<hir::Idx<Expr>>,
    min_step: Option<hir::Idx<Expr>>,
    hit_epsilon: Option<hir::Idx<Expr>>,
    max_steps: Option<hir::Idx<Expr>>,
    hit: Option<hir::Idx<Expr>>,
}

#[derive(Clone, Copy)]
enum ShapeBatchQueryKind {
    Trace,
    Surface,
    Occluded,
}

struct ShapeBatchQuerySpec {
    kind: ShapeBatchQueryKind,
    capture: hir::Idx<Expr>,
    items: hir::Idx<Expr>,
    backend: hir::Idx<Expr>,
}

#[derive(Clone, Copy)]
enum FieldBatchQueryKind {
    Distance,
    Normal,
}

struct FieldBatchQuerySpec {
    kind: FieldBatchQueryKind,
    capture: hir::Idx<Expr>,
    items: hir::Idx<Expr>,
    backend: hir::Idx<Expr>,
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

fn portable_abi_from_type_ref(
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
        "I64" => PortableAbiType::I64,
        "U64" => PortableAbiType::U64,
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

fn portable_value_struct_abi(
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
    let fields = if let Some(record) = builtin_record(name.as_str()) {
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
        PortableBuiltinType::Atom(atom) => match atom {
            PortableBuiltinAtom::Bool => PortableAbiType::Bool,
            PortableBuiltinAtom::I32 => PortableAbiType::I32,
            PortableBuiltinAtom::U32 => PortableAbiType::U32,
            PortableBuiltinAtom::I64 => PortableAbiType::I64,
            PortableBuiltinAtom::U64 => PortableAbiType::U64,
            PortableBuiltinAtom::F32 => PortableAbiType::F32,
            PortableBuiltinAtom::Vec2 => PortableAbiType::Vec2,
            PortableBuiltinAtom::Vec3 => PortableAbiType::Vec3,
            PortableBuiltinAtom::Vec4 => PortableAbiType::Vec4,
            PortableBuiltinAtom::Mat3 => PortableAbiType::Mat3,
            PortableBuiltinAtom::Mat4 => PortableAbiType::Mat4,
            PortableBuiltinAtom::Quat => PortableAbiType::Quat,
        },
        PortableBuiltinType::Named(name) => {
            portable_value_struct_abi(name, module, type_tags, visiting)
                .unwrap_or(PortableAbiType::Value)
        }
    }
}

fn build_interface_dispatch_functions(
    module: &hir::Module,
    interface_impls: &HashMap<SmolStr, Vec<SmolStr>>,
    type_tags: &HashMap<SmolStr, TypeTagId>,
) -> Vec<MirFunction> {
    let mut functions = Vec::new();
    for (_idx, interface) in module.interfaces.iter() {
        let impls = interface_impls
            .get(&interface.name)
            .cloned()
            .unwrap_or_default();
        for method in &interface.methods {
            let params: Vec<SmolStr> = method.params.iter().map(|p| p.name.clone()).collect();
            functions.push(build_interface_dispatch_function(
                &interface.name,
                &method.name,
                &params,
                &impls,
                type_tags,
            ));
        }
    }
    functions
}

fn build_interface_dispatch_function(
    interface: &SmolStr,
    method: &SmolStr,
    params: &[SmolStr],
    impls: &[SmolStr],
    type_tags: &HashMap<SmolStr, TypeTagId>,
) -> MirFunction {
    let span = TextRange::empty(0.into());
    let mut locals = Vec::new();
    let mut params_ids = Vec::new();
    let mut temps = Vec::new();

    let receiver_id = LocalId(0);
    locals.push(Local {
        name: SmolStr::new("self"),
        mutable: false,
        ty: MirType::Unknown,
    });
    params_ids.push(receiver_id);

    for (idx, name) in params.iter().enumerate() {
        let local_id = LocalId(idx + 1);
        locals.push(Local {
            name: name.clone(),
            mutable: false,
            ty: MirType::Unknown,
        });
        params_ids.push(local_id);
    }

    let mut blocks = Vec::new();
    blocks.push(BasicBlock {
        stmts: Vec::new(),
        terminator: Terminator::Unreachable { span },
    });

    let mut cases = Vec::new();
    let mut impls_with_tags = Vec::new();
    for class in impls {
        let Some(tag) = type_tags.get(class) else {
            continue;
        };
        let block_id = BlockId(blocks.len());
        blocks.push(BasicBlock {
            stmts: Vec::new(),
            terminator: Terminator::Unreachable { span },
        });
        cases.push((SwitchCase::Type(*tag), block_id));
        impls_with_tags.push(class.clone());
    }

    let default_block = BlockId(blocks.len());
    blocks.push(BasicBlock {
        stmts: Vec::new(),
        terminator: Terminator::Unreachable { span },
    });

    blocks[0].terminator = Terminator::Switch {
        scrutinee: Value::Local(receiver_id),
        cases,
        default: default_block,
        span,
    };

    let call_args: Vec<Value> = params_ids.iter().map(|id| Value::Local(*id)).collect();

    for (idx, class) in impls_with_tags.iter().enumerate() {
        let block_id = BlockId(idx + 1);
        if block_id.0 >= blocks.len() {
            continue;
        }
        let temp_id = TempId(temps.len());
        temps.push(Temp {
            ty: MirType::Unknown,
        });
        let func_name = SmolStr::new(format!("{}.{}", class, method));
        blocks[block_id.0].stmts.push(MirStmt::Assign {
            place: Place::Temp(temp_id),
            value: Rvalue::Call {
                kind: CallKind::Sync,
                target: CallTarget::Function(func_name),
                args: call_args.clone(),
            },
            span,
        });
        blocks[block_id.0].terminator = Terminator::Return {
            value: Some(Value::Temp(temp_id)),
            span,
        };
    }

    let crash_temp = TempId(temps.len());
    temps.push(Temp {
        ty: MirType::Unknown,
    });
    blocks[default_block.0].stmts.push(MirStmt::Assign {
        place: Place::Temp(crash_temp),
        value: Rvalue::Crash {
            value: Value::Const(Literal::String(SmolStr::new("interface dispatch failed"))),
        },
        span,
    });
    blocks[default_block.0].terminator = Terminator::Return {
        value: Some(Value::Temp(crash_temp)),
        span,
    };

    MirFunction {
        name: SmolStr::new(format!("{}.{}", interface, method)),
        params: params_ids,
        abi_params: vec![PortableAbiType::Value; params.len() + 1],
        abi_return: PortableAbiType::Value,
        locals,
        temps,
        blocks,
        entry: BlockId(0),
        suspendable: false,
    }
}

fn builtin_function_names() -> Vec<SmolStr> {
    vec![
        SmolStr::new("__wr_assert_err"),
        SmolStr::new("__wr_print"),
        SmolStr::new("__wr_vec_component"),
        SmolStr::new("__wr_bytes_from_string"),
        SmolStr::new("__wr_bytes_from_list"),
        SmolStr::new("__wr_bytes_to_string"),
        SmolStr::new("__wr_bytes_to_list"),
        SmolStr::new("__wr_bytes_len"),
        SmolStr::new("__wr_fs_read_bytes"),
        SmolStr::new("__wr_fs_write_bytes"),
        SmolStr::new("__wr_external_call"),
        SmolStr::new("__wr_http_call"),
        SmolStr::new("__wr_web_parse_json_text"),
        SmolStr::new("__wr_web_render_json_text"),
        SmolStr::new("__wr_auth_hash_password"),
        SmolStr::new("__wr_auth_verify_password_hash"),
        SmolStr::new("__wr_auth_sign_jwt"),
        SmolStr::new("__wr_auth_verify_jwt"),
        SmolStr::new("__wr_auth_generate_secure_token"),
        SmolStr::new("__wr_auth_render_jwks_document"),
        SmolStr::new("vec2"),
        SmolStr::new("vec3"),
        SmolStr::new("vec4"),
        SmolStr::new("quat"),
        SmolStr::new("mat3_identity"),
        SmolStr::new("mat3_cols"),
        SmolStr::new("mat4_identity"),
        SmolStr::new("mat4_cols"),
        SmolStr::new("f32"),
        SmolStr::new("i32"),
        SmolStr::new("i64"),
        SmolStr::new("u32"),
        SmolStr::new("u64"),
        SmolStr::new("dot"),
        SmolStr::new("length"),
        SmolStr::new("normalize"),
        SmolStr::new("cross"),
        SmolStr::new("min"),
        SmolStr::new("max"),
        SmolStr::new("clamp"),
        SmolStr::new("mix"),
        SmolStr::new("abs"),
        SmolStr::new("sign"),
        SmolStr::new("floor"),
        SmolStr::new("ceil"),
        SmolStr::new("fract"),
        SmolStr::new("sin"),
        SmolStr::new("cos"),
        SmolStr::new("sqrt"),
        SmolStr::new("pow"),
        SmolStr::new("distance"),
        SmolStr::new("reflect"),
        SmolStr::new("bounds2_center"),
        SmolStr::new("bounds2_size"),
        SmolStr::new("bounds3_center"),
        SmolStr::new("bounds3_size"),
        SmolStr::new("transform3_identity"),
        SmolStr::new("transform_point"),
        SmolStr::new("transform_vector"),
        SmolStr::new("transform_normal"),
        SmolStr::new("compose_transform3"),
        SmolStr::new("inverse_transform3"),
        SmolStr::new("translate"),
        SmolStr::new("rotate"),
        SmolStr::new("uniform_scale"),
        SmolStr::new("affine_transform"),
        SmolStr::new("warp"),
        SmolStr::new("repeat_linear"),
        SmolStr::new("repeat_grid"),
        SmolStr::new("radial_repeat"),
        SmolStr::new("mirror_array"),
        SmolStr::new("instance_array"),
        SmolStr::new("field_translate_point"),
        SmolStr::new("field_rotate_point"),
        SmolStr::new("field_uniform_scale_point"),
        SmolStr::new("field_affine_transform_point"),
        SmolStr::new("field_warp_point"),
        SmolStr::new("field_repeat_linear_point"),
        SmolStr::new("field_repeat_grid_point"),
        SmolStr::new("field_radial_repeat_point"),
        SmolStr::new("field_mirror_array_point"),
        SmolStr::new("field_instance_array_point"),
        SmolStr::new("field_sweep_coords"),
        SmolStr::new("field_profile_vertices_bounds4"),
        SmolStr::new("field_smooth_union"),
        SmolStr::new("field_smooth_intersection"),
        SmolStr::new("field_smooth_subtract"),
        SmolStr::new("field_bend_point"),
        SmolStr::new("field_twist_point"),
        SmolStr::new("field_taper_point"),
        SmolStr::new("field_displace_point"),
        SmolStr::new("rounded_box"),
        SmolStr::new("ellipsoid"),
        SmolStr::new("cone"),
        SmolStr::new("capped_cone"),
        SmolStr::new("box_frame"),
        SmolStr::new("slab"),
        SmolStr::new("triangle_prism"),
        SmolStr::new("hex_prism"),
        SmolStr::new("sphere"),
        SmolStr::new("box"),
        SmolStr::new("capsule"),
        SmolStr::new("cylinder"),
        SmolStr::new("plane"),
        SmolStr::new("torus"),
        SmolStr::new("circle2"),
        SmolStr::new("rect2"),
        SmolStr::new("rounded_rect2"),
        SmolStr::new("capsule2"),
        SmolStr::new("segment2"),
        SmolStr::new("polygon2"),
        SmolStr::new("polyline2"),
        SmolStr::new("smooth_union"),
        SmolStr::new("smooth_intersection"),
        SmolStr::new("smooth_subtract"),
        SmolStr::new("__wr_primitive_sphere"),
        SmolStr::new("__wr_primitive_box"),
        SmolStr::new("__wr_primitive_capsule"),
        SmolStr::new("__wr_primitive_cylinder"),
        SmolStr::new("__wr_primitive_plane"),
        SmolStr::new("__wr_primitive_torus"),
        SmolStr::new("field_union"),
        SmolStr::new("field_intersection"),
        SmolStr::new("field_subtract"),
        SmolStr::new("bend"),
        SmolStr::new("twist"),
        SmolStr::new("taper"),
        SmolStr::new("displace"),
        SmolStr::new("__wr_field_distance_capture"),
        SmolStr::new("__wr_field_normal_capture"),
        SmolStr::new("__wr_shape_distance_capture"),
        SmolStr::new("__wr_shape_normal_capture"),
        SmolStr::new("__wr_scene_trace_capture"),
        SmolStr::new("__wr_scene_surface_capture"),
        SmolStr::new("__wr_scene_radiance_capture"),
        SmolStr::new("__wr_scene_medium_capture"),
        SmolStr::new("__wr_scene_trace_queries"),
        SmolStr::new("__wr_scene_surface_queries"),
        SmolStr::new("gpu_buffer_new"),
        SmolStr::new("gpu_buffer_len"),
        SmolStr::new("gpu_buffer_get"),
        SmolStr::new("gpu_buffer_set"),
        SmolStr::new("gpu_atomic_i32_new"),
        SmolStr::new("gpu_atomic_i32_drop"),
        SmolStr::new("gpu_atomic_i32_load"),
        SmolStr::new("gpu_atomic_i32_store"),
        SmolStr::new("gpu_atomic_i32_fetch_add"),
        SmolStr::new("gpu_atomic_u32_new"),
        SmolStr::new("gpu_atomic_u32_drop"),
        SmolStr::new("gpu_atomic_u32_load"),
        SmolStr::new("gpu_atomic_u32_store"),
        SmolStr::new("gpu_atomic_u32_fetch_add"),
        SmolStr::new("global_invocation_id"),
        SmolStr::new("local_invocation_id"),
        SmolStr::new("workgroup_id"),
        SmolStr::new("num_workgroups"),
        SmolStr::new("workgroup_size"),
        SmolStr::new("gpu_schedule_deterministic"),
        SmolStr::new("gpu_schedule_reverse"),
        SmolStr::new("gpu_schedule_shuffle"),
        SmolStr::new("gpu_schedule_workgroup_reverse"),
        SmolStr::new("gpu_schedule_workgroup_shuffle"),
        SmolStr::new("gpu_schedule_round_robin_workgroups"),
        SmolStr::new("dispatch_compute"),
        SmolStr::new("__wr_gpu_dispatch_begin"),
        SmolStr::new("__wr_gpu_dispatch_select_invocation"),
        SmolStr::new("__wr_gpu_dispatch_end"),
        SmolStr::new("__wr_map_new"),
        SmolStr::new("__wr_list_push"),
        SmolStr::new("__wr_list_get"),
        SmolStr::new("__wr_list_set"),
        SmolStr::new("__wr_list_len"),
        SmolStr::new("__wr_map_get"),
        SmolStr::new("__wr_map_len"),
        SmolStr::new("__wr_map_set"),
        SmolStr::new("__wr_str_len"),
        SmolStr::new("__wr_log"),
        SmolStr::new("__wr_log_configure"),
        SmolStr::new("__wr_runtime_cpu_count"),
        SmolStr::new("__wr_reactor_new"),
        SmolStr::new("__wr_reactor_drop"),
        SmolStr::new("__wr_reactor_register"),
        SmolStr::new("__wr_reactor_deregister"),
        SmolStr::new("__wr_reactor_arm_timer"),
        SmolStr::new("__wr_task_signal_new"),
        SmolStr::new("__wr_task_signal_drop"),
        SmolStr::new("__wr_task_unpark_one"),
        SmolStr::new("__wr_task_unpark_all"),
        SmolStr::new("__wr_task_epoch"),
        SmolStr::new("__wr_atomic_i64_new"),
        SmolStr::new("__wr_atomic_i64_drop"),
        SmolStr::new("__wr_atomic_i64_load"),
        SmolStr::new("__wr_atomic_i64_store"),
        SmolStr::new("__wr_atomic_i64_fetch_add"),
        SmolStr::new("__wr_pool_size"),
        SmolStr::new("__wr_pool_rr"),
        SmolStr::new("__wr_pool_queue_len"),
        SmolStr::new("__wr_actor_mailbox_len"),
        SmolStr::new("__wr_actor_pause"),
        SmolStr::new("__wr_actor_resume"),
        SmolStr::new("__wr_actor_pause_wait"),
        SmolStr::new("__wr_actor_fire_burst_begin"),
        SmolStr::new("__wr_actor_fire_burst_end"),
        SmolStr::new("__wr_actor_fire_burst_abort"),
        SmolStr::new("__wr_metrics_get"),
        SmolStr::new("__wr_metrics_dropped_paused_id"),
        SmolStr::new("__wr_metrics_messages_dropped_id"),
        SmolStr::new("__wr_metrics_scene_trace_id"),
        SmolStr::new("__wr_metrics_field_sample_id"),
        SmolStr::new("__wr_metrics_scene_trace_support_pruned_branch"),
        SmolStr::new("__wr_metrics_scene_trace_candidate_branch"),
        SmolStr::new("__wr_metrics_scene_trace_exact_path"),
        SmolStr::new("__wr_metrics_scene_trace_conservative_path"),
        SmolStr::new("__wr_metrics_scene_trace_hit"),
        SmolStr::new("__wr_metrics_scene_trace_support_pruned_branch_id"),
        SmolStr::new("__wr_metrics_scene_trace_candidate_branch_id"),
        SmolStr::new("__wr_metrics_scene_trace_exact_path_id"),
        SmolStr::new("__wr_metrics_scene_trace_conservative_path_id"),
        SmolStr::new("__wr_metrics_scene_trace_hit_count_id"),
        SmolStr::new("__wr_metrics_scene_trace_hit_steps_total_id"),
        SmolStr::new("__wr_metrics_scene_trace_hit_field_samples_total_id"),
        SmolStr::new("__wr_metrics_scene_trace_steps_le_1_id"),
        SmolStr::new("__wr_metrics_scene_trace_steps_le_4_id"),
        SmolStr::new("__wr_metrics_scene_trace_steps_le_8_id"),
        SmolStr::new("__wr_metrics_scene_trace_steps_le_16_id"),
        SmolStr::new("__wr_metrics_scene_trace_steps_gt_16_id"),
        SmolStr::new("__wr_metrics_scene_trace"),
        SmolStr::new("__wr_metrics_field_sample"),
        SmolStr::new("__wr_metrics_scene_trace_blend_cost"),
        SmolStr::new("__wr_metrics_scene_trace_deformation_cost"),
        SmolStr::new("__wr_metrics_scene_trace_blend_cost_id"),
        SmolStr::new("__wr_metrics_scene_trace_deformation_cost_id"),
        SmolStr::new("__wr_metrics_web_writev_calls_id"),
        SmolStr::new("__wr_metrics_web_sendfile_calls_id"),
        SmolStr::new("__wr_clock_ns"),
        SmolStr::new("__wr_sleep_ms"),
        SmolStr::new("__wr_env_get"),
        SmolStr::new("__wr_env_set"),
        SmolStr::new("__wr_runtime_configure"),
        SmolStr::new("__wr_db_core_open"),
        SmolStr::new("__wr_db_core_close"),
        SmolStr::new("__wr_db_core_submit_batch"),
        SmolStr::new("__wr_db_core_read_point"),
        SmolStr::new("__wr_db_core_read_range"),
        SmolStr::new("__wr_db_core_txn_begin"),
        SmolStr::new("__wr_db_core_txn_prepare"),
        SmolStr::new("__wr_db_core_txn_commit"),
        SmolStr::new("__wr_db_core_txn_abort"),
        SmolStr::new("__wr_db_admin_snapshot_start"),
        SmolStr::new("__wr_db_admin_snapshot_status"),
        SmolStr::new("__wr_db_admin_restore"),
        SmolStr::new("__wr_db_admin_checkpoint_create"),
        SmolStr::new("__wr_db_admin_checkpoint_restore_latest"),
        SmolStr::new("__wr_db_admin_schema_epoch_set"),
        SmolStr::new("__wr_db_admin_schema_set_all_voters_on_target_binary"),
        SmolStr::new("__wr_db_admin_autoscale_tick"),
        SmolStr::new("__wr_db_admin_plan_rehome"),
        SmolStr::new("__wr_db_admin_advance_rehome"),
        SmolStr::new("__wr_db_admin_promote_async_failover"),
        SmolStr::new("__wr_db_explain_checkpoint_count"),
        SmolStr::new("__wr_db_explain_schema_epoch_get"),
        SmolStr::new("__wr_db_explain_health_has_checkpoint_or_schema_error"),
        SmolStr::new("__wr_db_explain_private_mesh_status"),
        SmolStr::new("__wr_db_explain_logical_shard_count"),
        SmolStr::new("__wr_db_explain_active_group_count"),
        SmolStr::new("__wr_db_explain_autoscale_status"),
        SmolStr::new("__wr_db_explain_topology_status"),
        SmolStr::new("__wr_db_explain_shard_map_epoch"),
        SmolStr::new("__wr_db_explain_shard_for_key"),
        SmolStr::new("__wr_db_explain_resolve_owner"),
        SmolStr::new("__wr_db_explain_global_route_lookup"),
    ]
}

fn vector_component_index(ty: MirType, member: &SmolStr) -> Option<usize> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hir::lower as hir_lower;
    use crate::hir::typeck;
    use crate::parser::ast;
    use crate::parser::ast::AstNode;
    use crate::parser::parse;

    #[test]
    fn test_lower_marks_suspendable() {
        let input = "\
class Whale {\n    fn swim() -> Boolean {\n        return true\n    }\n}\n\nfn f() -> Result[Boolean] {\n    w = detach Whale() * 1\n    return await w.swim()\n}\n";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = hir_lower::lower(root);
        let mir = lower_module(&module);
        let func = mir.functions.iter().find(|f| f.name == "f").unwrap();
        assert!(func.suspendable);
    }

    #[test]
    fn test_lower_if_creates_blocks() {
        let input = "fn f() -> Nothing {\n    if true {\n        x = 1\n    } else {\n        x = 2\n    }\n}\n";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = hir_lower::lower(root);
        let mir = lower_module(&module);
        let func = mir.functions.iter().find(|f| f.name == "f").unwrap();
        assert!(func.blocks.len() >= 3);
    }

    #[test]
    fn test_lower_member_assign_sets_field() {
        let input = "\
class Counter {
    value: Integer
    fn add(delta: Integer) -> Nothing {
        self.value += delta
    }
}
";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = hir_lower::lower(root);
        let (_type_errors, type_info) = typeck::check_module_with_info(&module);
        let mir_module = lower_module_with_types(&module, &type_info);
        let func = mir_module
            .functions
            .iter()
            .find(|func| func.name == "Counter.add")
            .expect("missing Counter.add");
        let has_set_field = func.blocks.iter().any(|block| {
            block.stmts.iter().any(
                |stmt| matches!(stmt, MirStmt::SetField { field, .. } if field.as_str() == "value"),
            )
        });
        assert!(has_set_field, "expected SetField for member assign");
    }

    #[test]
    fn test_lower_field_defaults_emits_set_fields() {
        let input = "\
class Foo {
    x: Integer = 1
    y: List = [1, 2]
    z: Map = {\"a\": 1}
}

fn run() -> Nothing {
    a = Foo()
    b = Foo(x=5)
}
";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = hir_lower::lower(root);
        let mir_module = lower_module(&module);
        let func = mir_module
            .functions
            .iter()
            .find(|func| func.name == "run")
            .expect("missing run");

        let mut set_x = 0usize;
        let mut set_y = 0usize;
        let mut set_z = 0usize;
        let mut build_list = 0usize;
        let mut build_map = 0usize;

        for block in &func.blocks {
            for stmt in &block.stmts {
                match stmt {
                    MirStmt::SetField { field, .. } if field.as_str() == "x" => set_x += 1,
                    MirStmt::SetField { field, .. } if field.as_str() == "y" => set_y += 1,
                    MirStmt::SetField { field, .. } if field.as_str() == "z" => set_z += 1,
                    _ => {}
                }
                if let MirStmt::Assign { value, .. } = stmt {
                    match value {
                        Rvalue::BuildList { .. } => build_list += 1,
                        Rvalue::BuildMap { .. } => build_map += 1,
                        _ => {}
                    }
                }
            }
        }

        assert_eq!(set_x, 2, "expected default and override for x");
        assert_eq!(set_y, 2, "expected defaults for y in both instances");
        assert_eq!(set_z, 2, "expected defaults for z in both instances");
        assert!(build_list >= 1, "expected BuildList for default list");
        assert!(build_map >= 1, "expected BuildMap for default map");
    }

    #[test]
    fn test_capture_field_queries_lower_without_indirect_calls() {
        let input = r#"field exact distance sphere_field(p: Vec3) -> F32 {
    sphere(radius = 1.0)
}

fn run() -> Nothing {
    scene = capture sphere_field
    distance = distance_at(capture=scene, point=vec3(0.0, 0.0, 2.0))
    normal = normal_at(capture=scene, point=vec3(0.0, 0.0, 2.0))
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = hir_lower::lower(root);
        let (_type_errors, type_info) = typeck::check_module_with_info(&module);
        let mir_module = lower_module_with_types(&module, &type_info);

        let mut indirect_calls = Vec::new();
        for func in &mir_module.functions {
            for block in &func.blocks {
                for stmt in &block.stmts {
                    if let MirStmt::Assign {
                        value: Rvalue::Call { target, .. },
                        ..
                    } = stmt
                        && matches!(target, CallTarget::Indirect(_))
                    {
                        indirect_calls.push((func.name.clone(), target.clone()));
                    }
                }
            }
        }

        assert!(
            indirect_calls.is_empty(),
            "unexpected indirect calls: {indirect_calls:?}"
        );
    }

    #[test]
    fn test_shape_queries_with_semantic_wrappers_lower_without_indirect_calls() {
        let input = r#"field conservative distance translated_sphere(p: Vec3) -> F32 {
    translate = vec3(0.8, 0.0, 0.0) {
        sphere(radius=0.65)
    }
}

material orb_surface(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(32.0, 64.0, 255.0),
        roughness=0.2,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

shape orb_shape {
    field = translated_sphere
    material = orb_surface
    payload = Payload(
        entity_id=u64(2),
        material_id=u64(22),
        actor=ActorHandle(id=u64(202), generation=u32(0))
    )
}

fn run() -> Nothing {
    scene = capture orb_shape
    hit = trace_shape(
        capture=scene,
        origin=vec3(0.8, 0.0, 3.0),
        direction=vec3(0.0, 0.0, -1.0),
        max_distance=6.0,
        min_step=0.05,
        hit_epsilon=0.001,
        max_steps=96
    )
    surface = surface_at(capture=scene, hit=hit)
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = hir_lower::lower(root);
        let (_type_errors, type_info) = typeck::check_module_with_info(&module);
        let mir_module = lower_module_with_types(&module, &type_info);

        let mut indirect_calls = Vec::new();
        for func in &mir_module.functions {
            for block in &func.blocks {
                for stmt in &block.stmts {
                    if let MirStmt::Assign {
                        value: Rvalue::Call { target, .. },
                        ..
                    } = stmt
                        && matches!(target, CallTarget::Indirect(_))
                    {
                        indirect_calls.push((func.name.clone(), target.clone()));
                    }
                }
            }
        }

        assert!(
            indirect_calls.is_empty(),
            "unexpected indirect calls: {indirect_calls:?}"
        );
    }

    #[test]
    fn test_shape_queries_with_semantic_wrappers_stay_direct_after_opt() {
        let input = r#"field conservative distance translated_sphere(p: Vec3) -> F32 {
    translate = vec3(0.8, 0.0, 0.0) {
        sphere(radius=0.65)
    }
}

material orb_surface(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(32.0, 64.0, 255.0),
        roughness=0.2,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

shape orb_shape {
    field = translated_sphere
    material = orb_surface
    payload = Payload(
        entity_id=u64(2),
        material_id=u64(22),
        actor=ActorHandle(id=u64(202), generation=u32(0))
    )
}

fn run() -> Nothing {
    scene = capture orb_shape
    hit = trace_shape(
        capture=scene,
        origin=vec3(0.8, 0.0, 3.0),
        direction=vec3(0.0, 0.0, -1.0),
        max_distance=6.0,
        min_step=0.05,
        hit_epsilon=0.001,
        max_steps=96
    )
    surface = surface_at(capture=scene, hit=hit)
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = hir_lower::lower(root);
        let (_type_errors, type_info) = typeck::check_module_with_info(&module);
        let check_ir = crate::hir::checkir::extract_module(&module);
        let mut mir_module = lower_module_with_types(&module, &type_info);
        let analysis = crate::mir::analysis::analyze_module(&mir_module);
        for func in &mut mir_module.functions {
            let types = analysis.type_map.function(&func.name);
            crate::mir::opt::run_function_passes_with_types(func, types);
        }
        let _ = crate::mir::opt::run_module_passes_with_rulepack(&mut mir_module, Some(&check_ir));

        let mut indirect_calls = Vec::new();
        for func in &mir_module.functions {
            for block in &func.blocks {
                for stmt in &block.stmts {
                    if let MirStmt::Assign {
                        value: Rvalue::Call { target, .. },
                        ..
                    } = stmt
                        && matches!(target, CallTarget::Indirect(_))
                    {
                        indirect_calls.push((func.name.clone(), target.clone()));
                    }
                }
            }
        }

        assert!(
            indirect_calls.is_empty(),
            "unexpected indirect calls after optimization: {indirect_calls:?}"
        );
    }

    #[test]
    fn test_trace_metrics_shape_query_path_stays_direct_after_opt() {
        let input = r#"field exact distance orb(p: Vec3) -> F32 {
    translate = vec3(0.8, 0.0, 0.0) {
        sphere(radius=0.65)
    }
}

material orb_surface(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(32.0, 64.0, 255.0),
        roughness=0.2,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

shape orb_shape {
    field = orb
    material = orb_surface
    payload = Payload(
        entity_id=u64(2),
        material_id=u64(22),
        actor=ActorHandle(id=u64(202), generation=u32(0))
    )
}

fn run() -> Nothing {
    orb_scene = capture orb_shape
    exact_before = __wr_metrics_get(__wr_metrics_scene_trace_exact_path_id())
    hit = trace_shape(
        capture=orb_scene,
        origin=vec3(0.8, 0.0, 3.0),
        direction=vec3(0.0, 0.0, -1.0),
        max_distance=6.0,
        min_step=0.05,
        hit_epsilon=0.001,
        max_steps=96
    )
    field_samples_after_trace = __wr_metrics_get(__wr_metrics_field_sample_id())
    surface = surface_at(capture=orb_scene, hit=hit)
    exact_after = __wr_metrics_get(__wr_metrics_scene_trace_exact_path_id())
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = hir_lower::lower(root);
        let (_type_errors, type_info) = typeck::check_module_with_info(&module);
        let check_ir = crate::hir::checkir::extract_module(&module);
        let mut mir_module = lower_module_with_types(&module, &type_info);
        let analysis = crate::mir::analysis::analyze_module(&mir_module);
        for func in &mut mir_module.functions {
            let types = analysis.type_map.function(&func.name);
            crate::mir::opt::run_function_passes_with_types(func, types);
        }
        let _ = crate::mir::opt::run_module_passes_with_rulepack(&mut mir_module, Some(&check_ir));

        let mut indirect_calls = Vec::new();
        for func in &mir_module.functions {
            for block in &func.blocks {
                for stmt in &block.stmts {
                    if let MirStmt::Assign {
                        value: Rvalue::Call { target, .. },
                        ..
                    } = stmt
                        && matches!(target, CallTarget::Indirect(_))
                    {
                        indirect_calls.push((func.name.clone(), target.clone()));
                    }
                }
            }
        }

        assert!(
            indirect_calls.is_empty(),
            "unexpected indirect calls after optimization: {indirect_calls:?}"
        );
    }

    #[test]
    fn test_trace_metrics_assertions_stay_direct_after_opt() {
        let input = r#"field exact distance orb(p: Vec3) -> F32 {
    translate = vec3(0.8, 0.0, 0.0) {
        sphere(radius=0.65)
    }
}

material orb_surface(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(32.0, 64.0, 255.0),
        roughness=0.2,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

shape orb_shape {
    field = orb
    material = orb_surface
    payload = Payload(
        entity_id=u64(2),
        material_id=u64(22),
        actor=ActorHandle(id=u64(202), generation=u32(0))
    )
}

fn run() -> Nothing {
    orb_scene = capture orb_shape
    exact_before = __wr_metrics_get(__wr_metrics_scene_trace_exact_path_id())
    conservative_before = __wr_metrics_get(__wr_metrics_scene_trace_conservative_path_id())
    hit_count_before = __wr_metrics_get(__wr_metrics_scene_trace_hit_count_id())
    hit_steps_before = __wr_metrics_get(__wr_metrics_scene_trace_hit_steps_total_id())
    hit_samples_before = __wr_metrics_get(__wr_metrics_scene_trace_hit_field_samples_total_id())
    field_samples_before = __wr_metrics_get(__wr_metrics_field_sample_id())
    bucket_before = __wr_metrics_get(__wr_metrics_scene_trace_steps_le_1_id())
        + __wr_metrics_get(__wr_metrics_scene_trace_steps_le_4_id())
        + __wr_metrics_get(__wr_metrics_scene_trace_steps_le_8_id())
        + __wr_metrics_get(__wr_metrics_scene_trace_steps_le_16_id())
        + __wr_metrics_get(__wr_metrics_scene_trace_steps_gt_16_id())

    hit = trace_shape(
        capture=orb_scene,
        origin=vec3(0.8, 0.0, 3.0),
        direction=vec3(0.0, 0.0, -1.0),
        max_distance=6.0,
        min_step=0.05,
        hit_epsilon=0.001,
        max_steps=96
    )
    field_samples_after_trace = __wr_metrics_get(__wr_metrics_field_sample_id())
    surface = surface_at(capture=orb_scene, hit=hit)

    exact_after = __wr_metrics_get(__wr_metrics_scene_trace_exact_path_id())
    conservative_after = __wr_metrics_get(__wr_metrics_scene_trace_conservative_path_id())
    hit_count_after = __wr_metrics_get(__wr_metrics_scene_trace_hit_count_id())
    hit_steps_after = __wr_metrics_get(__wr_metrics_scene_trace_hit_steps_total_id())
    hit_samples_after = __wr_metrics_get(__wr_metrics_scene_trace_hit_field_samples_total_id())
    field_samples_after = __wr_metrics_get(__wr_metrics_field_sample_id())
    bucket_after = __wr_metrics_get(__wr_metrics_scene_trace_steps_le_1_id())
        + __wr_metrics_get(__wr_metrics_scene_trace_steps_le_4_id())
        + __wr_metrics_get(__wr_metrics_scene_trace_steps_le_8_id())
        + __wr_metrics_get(__wr_metrics_scene_trace_steps_le_16_id())
        + __wr_metrics_get(__wr_metrics_scene_trace_steps_gt_16_id())

    assert value hit.hit == true
    assert value hit.payload.material_id == u64(22)
    assert approx surface.albedo.x ~= 32.0 within 0.001
    assert approx surface.albedo.z ~= 255.0 within 0.001
    assert value exact_after - exact_before == 1
    assert value conservative_after - conservative_before == 0
    assert value hit_count_after - hit_count_before == 1
    assert value hit_steps_after - hit_steps_before == hit.steps
    assert value hit_samples_after - hit_samples_before == field_samples_after_trace - field_samples_before
    assert value bucket_after - bucket_before == 1
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = hir_lower::lower(root);
        let (_type_errors, type_info) = typeck::check_module_with_info(&module);
        let check_ir = crate::hir::checkir::extract_module(&module);
        let mut mir_module = lower_module_with_types(&module, &type_info);
        let analysis = crate::mir::analysis::analyze_module(&mir_module);
        for func in &mut mir_module.functions {
            let types = analysis.type_map.function(&func.name);
            crate::mir::opt::run_function_passes_with_types(func, types);
        }
        let _ = crate::mir::opt::run_module_passes_with_rulepack(&mut mir_module, Some(&check_ir));

        let mut indirect_calls = Vec::new();
        for func in &mir_module.functions {
            for block in &func.blocks {
                for stmt in &block.stmts {
                    if let MirStmt::Assign {
                        value: Rvalue::Call { target, .. },
                        ..
                    } = stmt
                        && matches!(target, CallTarget::Indirect(_))
                    {
                        indirect_calls.push((func.name.clone(), target.clone()));
                    }
                }
            }
        }

        assert!(
            indirect_calls.is_empty(),
            "unexpected indirect calls after optimization: {indirect_calls:?}"
        );
    }

    #[test]
    fn test_lower_integer_range_for_uses_typed_induction_fast_path() {
        let input = "\
fn run() -> Integer {
    start = 1
    stop = 4
    mutable total = 0
    for i in start...stop {
        total += i
    }
    return total
}
";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = hir_lower::lower(root);
        let (_type_errors, type_info) = typeck::check_module_with_info(&module);
        let mir_module = lower_module_with_types(&module, &type_info);
        let func = mir_module
            .functions
            .iter()
            .find(|func| func.name == "run")
            .expect("missing run");

        assert!(
            func.locals
                .iter()
                .any(|local| local.name.as_str() == "i" && local.ty == MirType::Integer),
            "expected typed loop variable for integer range",
        );
        assert!(
            func.locals
                .iter()
                .any(|local| local.name.starts_with("$range_idx") && local.ty == MirType::Integer),
            "expected typed integer induction local",
        );
        assert!(
            func.locals
                .iter()
                .any(|local| local.name.starts_with("$range_step") && local.ty == MirType::Integer),
            "expected typed integer step local",
        );

        for block in &func.blocks {
            for stmt in &block.stmts {
                assert!(
                    !matches!(stmt, MirStmt::IterInit { .. } | MirStmt::IterNext { .. }),
                    "typed integer range loop should not use iterator protocol",
                );
                if let MirStmt::Assign { value, .. } = stmt {
                    assert!(
                        !matches!(
                            value,
                            Rvalue::Binary {
                                op: crate::hir::BinaryOp::Range,
                                ..
                            }
                        ),
                        "typed integer range loop should not materialize range object",
                    );
                }
            }
        }
    }

    #[test]
    fn test_lower_member_field_ops_emit_slot_hints() {
        let input = "\
class Counter {
    mutable value: Integer
    mutable other: Integer

    fn bump() -> Nothing {
        self.value += 1
        self.other = 4
    }
}

fn run() -> Integer {
    c = Counter(value=1, other=2)
    c.value += 3
    return c.other
}
";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = hir_lower::lower(root);
        let (_type_errors, type_info) = typeck::check_module_with_info(&module);
        let mir_module = lower_module_with_types(&module, &type_info);

        let mut saw_get_value_slot = false;
        let mut saw_get_other_slot = false;
        let mut saw_set_value_slot = false;
        let mut saw_set_other_slot = false;

        for func in &mir_module.functions {
            for block in &func.blocks {
                for stmt in &block.stmts {
                    match stmt {
                        MirStmt::Assign {
                            value:
                                Rvalue::GetField {
                                    field,
                                    slot: Some(slot),
                                    ..
                                },
                            ..
                        } if field.as_str() == "value" && *slot == 0 => saw_get_value_slot = true,
                        MirStmt::Assign {
                            value:
                                Rvalue::GetField {
                                    field,
                                    slot: Some(slot),
                                    ..
                                },
                            ..
                        } if field.as_str() == "other" && *slot == 1 => saw_get_other_slot = true,
                        MirStmt::SetField {
                            field,
                            slot: Some(slot),
                            ..
                        } if field.as_str() == "value" && *slot == 0 => saw_set_value_slot = true,
                        MirStmt::SetField {
                            field,
                            slot: Some(slot),
                            ..
                        } if field.as_str() == "other" && *slot == 1 => saw_set_other_slot = true,
                        _ => {}
                    }
                }
            }
        }

        assert!(saw_get_value_slot, "expected slot-hinted get for value");
        assert!(saw_get_other_slot, "expected slot-hinted get for other");
        assert!(saw_set_value_slot, "expected slot-hinted set for value");
        assert!(saw_set_other_slot, "expected slot-hinted set for other");
    }
}
