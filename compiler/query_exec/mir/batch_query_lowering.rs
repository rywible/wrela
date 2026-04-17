//! Owns MIR lowering for scene, field, shape, and world batch-query helpers.
//! Does not own capture-helper lowering or the public query-contract catalog.
//!
//! Key invariants:
//! - lowered batch helpers preserve contract-selected semantics while making the
//!   backend guard path explicit in MIR.
//! - native and WGSL bridge cases must share the same result-schema ordering so
//!   downstream execution remains comparable.
//! - batch lowering may specialize by query kind, but it must not silently
//!   change world/shape dispatch authority.
//!
//! Primary entrypoints:
//! - `lower_scene_trace_queries_helper`
//! - `lower_field_batch_queries_helper`
//! - `lower_world_batch_queries_helper`
//!
//! Failure modes / common pitfalls:
//! - forgetting to route unsupported backends through the shared failure helpers
//!   makes contract guarantees drift by query kind.
//! - mixing capture-specific state into this file weakens the split established
//!   in Phase 53.

use super::scene_medium_capture_lowering::{
    batch_auto_backend, lower_batch_wgsl_bridge_call, lower_wgsl_bridge_failure,
    lower_world_batch_wgsl_bridge_call, lower_world_domain_validation, lower_world_region_dispatch,
};
use super::{
    BatchQueryKind, BatchQueryPlan, BinaryOp, CandidateStrategy, CaptureKind, DispatchBackend,
    FunctionLowerer, HashMap, HashSet, InternalKernelKind, Literal, MirFunction, MirStmt, MirType,
    NativeWgslBridgeConfig, PortableAbiType, PruningStrategy, QueryItemKind, QueryResultKind,
    QuerySurfaceKind, SmolStr, TextRange, TypeTagId, Value, WorldQueryKind,
    batch_query_kind_for_contract_id, stable_field_scene_capture_id, stable_shape_capture_id,
    stable_shape_scene_capture_id, world_query_semantics,
};
use crate::hir;
use crate::mir::ir::*;

pub(crate) fn lower_scene_trace_queries_helper(
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
    let ray = lowerer.build_ray_query_value(
        Value::Temp(origin),
        Value::Temp(direction),
        Value::Temp(max_distance),
        Value::Temp(min_step),
        Value::Temp(hit_epsilon),
        Value::Temp(max_steps),
        span,
    );
    let hit = lowerer.lower_call_temp(
        MirType::Named(SmolStr::new("Hit3")),
        SmolStr::new("__wr_scene_trace_capture"),
        vec![Value::Temp(capture), ray],
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

pub(crate) fn lower_scene_surface_queries_helper(
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

fn lower_native_batch_backend_guard(
    lowerer: &mut FunctionLowerer,
    backend: LocalId,
    auto_backend: DispatchBackend,
    cpu_block: BlockId,
    vgpu_block: BlockId,
    wgsl_block: BlockId,
    invalid_backend_block: BlockId,
    span: TextRange,
) {
    let is_cpu = lowerer.lower_binary_temp(
        MirType::Boolean,
        BinaryOp::Eq,
        Value::Local(backend),
        Value::Const(Literal::Integer(0)),
        span,
    );
    let is_vgpu = lowerer.lower_binary_temp(
        MirType::Boolean,
        BinaryOp::Eq,
        Value::Local(backend),
        Value::Const(Literal::Integer(1)),
        span,
    );
    let is_wgsl = lowerer.lower_binary_temp(
        MirType::Boolean,
        BinaryOp::Eq,
        Value::Local(backend),
        Value::Const(Literal::Integer(2)),
        span,
    );
    let is_auto = lowerer.lower_binary_temp(
        MirType::Boolean,
        BinaryOp::Eq,
        Value::Local(backend),
        Value::Const(Literal::Integer(3)),
        span,
    );

    let auto_block = lowerer.new_block();
    let vgpu_check_block = lowerer.new_block();
    lowerer.set_terminator(Terminator::Branch {
        cond: is_auto,
        then_target: auto_block,
        else_target: vgpu_check_block,
        span,
    });

    lowerer.current_block = auto_block;
    lowerer.set_terminator(Terminator::Jump {
        target: match batch_auto_backend(auto_backend) {
            DispatchBackend::Cpu => cpu_block,
            DispatchBackend::VirtualGpu => vgpu_block,
            DispatchBackend::Wgsl => wgsl_block,
            DispatchBackend::Auto => cpu_block,
        },
        span,
    });

    let cpu_check_block = lowerer.new_block();
    lowerer.current_block = vgpu_check_block;
    lowerer.set_terminator(Terminator::Branch {
        cond: is_vgpu,
        then_target: vgpu_block,
        else_target: cpu_check_block,
        span,
    });

    let wgsl_check_block = lowerer.new_block();
    lowerer.current_block = cpu_check_block;
    lowerer.set_terminator(Terminator::Branch {
        cond: is_cpu,
        then_target: cpu_block,
        else_target: wgsl_check_block,
        span,
    });

    lowerer.current_block = wgsl_check_block;
    lowerer.set_terminator(Terminator::Branch {
        cond: is_wgsl,
        then_target: wgsl_block,
        else_target: invalid_backend_block,
        span,
    });
}

pub(crate) fn lower_field_batch_queries_helper(
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
    plan: &BatchQueryPlan,
    auto_backend: DispatchBackend,
    wgsl_config: Option<&Result<NativeWgslBridgeConfig, SmolStr>>,
    capture_indices: &HashMap<SmolStr, u32>,
) -> MirFunction {
    debug_assert!(matches!(
        plan.kernel,
        InternalKernelKind::FieldDistanceCapture
            | InternalKernelKind::ShapeDistanceCapture
            | InternalKernelKind::FieldNormalCapture
            | InternalKernelKind::ShapeNormalCapture
    ));
    match plan.capture_kind {
        CaptureKind::Field => {
            debug_assert_eq!(
                plan.candidate_strategy(),
                CandidateStrategy::DirectFieldCapture
            );
            debug_assert_eq!(plan.pruning_strategy(), PruningStrategy::None);
            debug_assert!(!plan.has_opaque_pessimization_boundary());
        }
        CaptureKind::Shape => {
            debug_assert!(matches!(
                plan.candidate_strategy(),
                CandidateStrategy::ShapeBranchTraversal
                    | CandidateStrategy::SupportAcceleratedShapeTraversal
                    | CandidateStrategy::OpaqueFallback
            ));
            debug_assert!(matches!(
                plan.pruning_strategy(),
                PruningStrategy::ConservativeTraversal
                    | PruningStrategy::SupportLowerBound
                    | PruningStrategy::CullingTable
                    | PruningStrategy::OpaquePessimizationBoundary
            ));
            if matches!(plan.candidate_strategy(), CandidateStrategy::OpaqueFallback) {
                debug_assert!(plan.has_opaque_pessimization_boundary());
            }
        }
        CaptureKind::Region => panic!("field batch helper does not support region captures"),
    }
    debug_assert!(!matches!(plan.capture_kind, CaptureKind::Region));
    debug_assert!(!plan.preserves_local_hit_context);
    let helper_name = plan.helper_name.clone();
    let (capture_type, capture_field, stable_capture_id, invalid_capture_message) =
        match plan.capture_kind {
            CaptureKind::Field => (
                "FieldCapture",
                "scene_id",
                stable_field_scene_capture_id as fn(&SmolStr) -> i64,
                "field batch WGSL dispatch requires a known field capture",
            ),
            CaptureKind::Shape => (
                "ShapeCapture",
                "scene_id",
                stable_shape_scene_capture_id as fn(&SmolStr) -> i64,
                "shape batch WGSL dispatch requires a known shape scene capture",
            ),
            CaptureKind::Region => panic!("field batch helper does not support region captures"),
        };
    let wgsl_bridge_symbol = match plan.kernel {
        InternalKernelKind::FieldDistanceCapture => "__wr_wgsl_field_distance_batch_queries",
        InternalKernelKind::ShapeDistanceCapture => "__wr_wgsl_shape_distance_batch_queries",
        InternalKernelKind::FieldNormalCapture => "__wr_wgsl_field_normal_batch_queries",
        InternalKernelKind::ShapeNormalCapture => "__wr_wgsl_shape_normal_batch_queries",
        other => panic!("unexpected field batch kernel for WGSL bridge: {other:?}"),
    };
    debug_assert!(matches!(plan.item_kind, QueryItemKind::PointQuery));
    debug_assert!(matches!(
        plan.result_kind,
        QueryResultKind::DistanceResult | QueryResultKind::NormalResult
    ));
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
        MirType::Named(SmolStr::new(capture_type)),
    );
    lowerer.declare_local(SmolStr::new("capture"), capture);
    lowerer.params.push(capture);

    let items = lowerer.new_local(
        SmolStr::new("items"),
        false,
        MirType::Named(SmolStr::new("List")),
    );
    lowerer.declare_local(SmolStr::new("items"), items);
    lowerer.params.push(items);

    let backend = lowerer.new_local(SmolStr::new("backend"), false, MirType::Integer);
    lowerer.declare_local(SmolStr::new("backend"), backend);
    lowerer.params.push(backend);

    let result = lowerer.new_local(
        SmolStr::new("$field_batch_result"),
        true,
        MirType::Named(SmolStr::new("List")),
    );

    let entry = lowerer.new_block();
    lowerer.current_block = entry;
    lowerer.push_stmt(MirStmt::Assign {
        place: Place::Local(result),
        value: Rvalue::BuildList {
            items: Vec::new(),
            alloc: AllocKind::Escaping,
        },
        span,
    });

    let cpu_block = lowerer.new_block();
    let vgpu_block = lowerer.new_block();
    let wgsl_block = lowerer.new_block();
    let invalid_backend_block = lowerer.new_block();
    let merge_block = lowerer.new_block();
    lower_native_batch_backend_guard(
        &mut lowerer,
        backend,
        auto_backend,
        cpu_block,
        vgpu_block,
        wgsl_block,
        invalid_backend_block,
        span,
    );

    lowerer.current_block = invalid_backend_block;
    lower_wgsl_bridge_failure(
        &mut lowerer,
        SmolStr::new("scene batch dispatch backend must be cpu, virtual_gpu, wgsl, or auto"),
        span,
    );

    lowerer.current_block = cpu_block;
    lowerer.lower_batch_query_loop(
        plan,
        Value::Local(items),
        Value::Local(capture),
        result,
        span,
        false,
        merge_block,
    );

    lowerer.current_block = vgpu_block;
    lowerer.lower_batch_query_loop(
        plan,
        Value::Local(items),
        Value::Local(capture),
        result,
        span,
        true,
        merge_block,
    );

    lowerer.current_block = wgsl_block;
    if let Some(value) = lower_batch_wgsl_bridge_call(
        &mut lowerer,
        capture,
        items,
        wgsl_config,
        wgsl_bridge_symbol,
        capture_type,
        capture_field,
        capture_indices,
        stable_capture_id,
        invalid_capture_message,
        span,
    ) {
        lowerer.assign_use(Place::Local(result), value, span);
        lowerer.set_terminator(Terminator::Jump {
            target: merge_block,
            span,
        });
    }

    lowerer.current_block = merge_block;
    lowerer.set_terminator(Terminator::Return {
        value: Some(Value::Local(result)),
        span,
    });

    MirFunction {
        name: helper_name,
        params: lowerer.params,
        abi_params: vec![
            PortableAbiType::Value,
            PortableAbiType::Value,
            PortableAbiType::Value,
        ],
        abi_return: PortableAbiType::Value,
        locals: lowerer.locals,
        temps: lowerer.temps,
        blocks: lowerer.blocks,
        entry,
        suspendable: false,
    }
}

pub(crate) fn lower_shape_batch_queries_helper(
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
    plan: &BatchQueryPlan,
    auto_backend: DispatchBackend,
    wgsl_config: Option<&Result<NativeWgslBridgeConfig, SmolStr>>,
    capture_indices: &HashMap<SmolStr, u32>,
) -> MirFunction {
    debug_assert!(matches!(
        plan.kernel,
        InternalKernelKind::ShapeTraceCapture
            | InternalKernelKind::ShapeSurfaceCapture
            | InternalKernelKind::ShapeOccludedCapture
    ));
    debug_assert_eq!(plan.capture_kind, CaptureKind::Shape);
    let helper_name = plan.helper_name.clone();
    let batch_kind = batch_query_kind_for_contract_id(plan.contract_id)
        .expect("batch query plan contract id must resolve");
    match batch_kind {
        BatchQueryKind::Nearest | BatchQueryKind::Trace | BatchQueryKind::Occluded => {
            debug_assert!(matches!(
                plan.candidate_strategy(),
                CandidateStrategy::ShapeBranchTraversal
                    | CandidateStrategy::SupportAcceleratedShapeTraversal
                    | CandidateStrategy::OpaqueFallback
            ));
            debug_assert!(matches!(
                plan.pruning_strategy(),
                PruningStrategy::ConservativeTraversal
                    | PruningStrategy::SupportLowerBound
                    | PruningStrategy::CullingTable
                    | PruningStrategy::OpaquePessimizationBoundary
            ));
            debug_assert!(plan.preserves_local_hit_context);
            if matches!(plan.candidate_strategy(), CandidateStrategy::OpaqueFallback) {
                debug_assert!(plan.has_opaque_pessimization_boundary());
            }
            if matches!(
                plan.pruning_strategy(),
                PruningStrategy::OpaquePessimizationBoundary
            ) {
                debug_assert!(matches!(
                    plan.candidate_strategy(),
                    CandidateStrategy::OpaqueFallback
                ));
                debug_assert!(plan.has_opaque_pessimization_boundary());
            }
        }
        BatchQueryKind::Surface => {
            debug_assert_eq!(
                plan.candidate_strategy(),
                CandidateStrategy::SurfaceHitReuse
            );
            debug_assert_eq!(plan.pruning_strategy(), PruningStrategy::None);
            debug_assert!(!plan.preserves_local_hit_context);
            debug_assert!(!plan.has_opaque_pessimization_boundary());
        }
        other => panic!("shape batch helper does not support {other:?}"),
    }
    debug_assert!(matches!(
        plan.item_kind,
        QueryItemKind::RayQuery | QueryItemKind::Hit3
    ));
    debug_assert!(matches!(
        plan.result_kind,
        QueryResultKind::Hit3 | QueryResultKind::Surface | QueryResultKind::OcclusionResult
    ));
    let wgsl_bridge_symbol = match plan.kernel {
        InternalKernelKind::ShapeTraceCapture => "__wr_wgsl_shape_trace_batch_queries",
        InternalKernelKind::ShapeSurfaceCapture => "__wr_wgsl_shape_surface_batch_queries",
        InternalKernelKind::ShapeOccludedCapture => "__wr_wgsl_shape_occluded_batch_queries",
        other => panic!("unexpected shape batch kernel for WGSL bridge: {other:?}"),
    };
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
    lowerer.declare_local(SmolStr::new("capture"), capture);
    lowerer.params.push(capture);

    let items = lowerer.new_local(
        SmolStr::new("items"),
        false,
        MirType::Named(SmolStr::new("List")),
    );
    lowerer.declare_local(SmolStr::new("items"), items);
    lowerer.params.push(items);

    let backend = lowerer.new_local(SmolStr::new("backend"), false, MirType::Integer);
    lowerer.declare_local(SmolStr::new("backend"), backend);
    lowerer.params.push(backend);

    let result = lowerer.new_local(
        SmolStr::new("$shape_batch_result"),
        true,
        MirType::Named(SmolStr::new("List")),
    );

    let entry = lowerer.new_block();
    lowerer.current_block = entry;
    lowerer.push_stmt(MirStmt::Assign {
        place: Place::Local(result),
        value: Rvalue::BuildList {
            items: Vec::new(),
            alloc: AllocKind::Escaping,
        },
        span,
    });

    let cpu_block = lowerer.new_block();
    let vgpu_block = lowerer.new_block();
    let wgsl_block = lowerer.new_block();
    let invalid_backend_block = lowerer.new_block();
    let merge_block = lowerer.new_block();
    lower_native_batch_backend_guard(
        &mut lowerer,
        backend,
        auto_backend,
        cpu_block,
        vgpu_block,
        wgsl_block,
        invalid_backend_block,
        span,
    );

    lowerer.current_block = invalid_backend_block;
    lower_wgsl_bridge_failure(
        &mut lowerer,
        SmolStr::new("scene batch dispatch backend must be cpu, virtual_gpu, wgsl, or auto"),
        span,
    );

    lowerer.current_block = cpu_block;
    lowerer.lower_batch_query_loop(
        plan,
        Value::Local(items),
        Value::Local(capture),
        result,
        span,
        false,
        merge_block,
    );

    lowerer.current_block = vgpu_block;
    lowerer.lower_batch_query_loop(
        plan,
        Value::Local(items),
        Value::Local(capture),
        result,
        span,
        true,
        merge_block,
    );

    lowerer.current_block = wgsl_block;
    if let Some(value) = lower_batch_wgsl_bridge_call(
        &mut lowerer,
        capture,
        items,
        wgsl_config,
        wgsl_bridge_symbol,
        "ShapeCapture",
        "root_feature_id",
        capture_indices,
        stable_shape_capture_id,
        "shape batch WGSL dispatch requires a known shape capture",
        span,
    ) {
        lowerer.assign_use(Place::Local(result), value, span);
        lowerer.set_terminator(Terminator::Jump {
            target: merge_block,
            span,
        });
    }

    lowerer.current_block = merge_block;
    lowerer.set_terminator(Terminator::Return {
        value: Some(Value::Local(result)),
        span,
    });

    MirFunction {
        name: helper_name,
        params: lowerer.params,
        abi_params: vec![
            PortableAbiType::Value,
            PortableAbiType::Value,
            PortableAbiType::Value,
        ],
        abi_return: PortableAbiType::Value,
        locals: lowerer.locals,
        temps: lowerer.temps,
        blocks: lowerer.blocks,
        entry,
        suspendable: false,
    }
}

pub(crate) fn lower_world_batch_queries_helper(
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
    plan: &BatchQueryPlan,
    auto_backend: DispatchBackend,
    wgsl_config: Option<&Result<NativeWgslBridgeConfig, SmolStr>>,
    wgsl_shape_indices: &HashMap<SmolStr, u32>,
) -> MirFunction {
    debug_assert_eq!(plan.capture_kind, CaptureKind::Region);
    debug_assert_eq!(plan.surface, QuerySurfaceKind::WorldBatch);
    let helper_name = plan.helper_name.clone();
    let batch_kind = batch_query_kind_for_contract_id(plan.contract_id)
        .expect("world batch query plan contract id must resolve");
    let semantics = match batch_kind {
        BatchQueryKind::Distance => world_query_semantics(WorldQueryKind::Distance),
        BatchQueryKind::Normal => world_query_semantics(WorldQueryKind::Normal),
        BatchQueryKind::Nearest | BatchQueryKind::Trace => {
            world_query_semantics(WorldQueryKind::Trace)
        }
        BatchQueryKind::Occluded => world_query_semantics(WorldQueryKind::Occluded),
        BatchQueryKind::Surface => world_query_semantics(WorldQueryKind::Surface),
        BatchQueryKind::Radiance => world_query_semantics(WorldQueryKind::Radiance),
        BatchQueryKind::Medium => world_query_semantics(WorldQueryKind::Medium),
    };
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
    let items = lowerer.new_local(
        SmolStr::new("items"),
        false,
        MirType::Named(SmolStr::new("List")),
    );
    let backend = lowerer.new_local(SmolStr::new("backend"), false, MirType::Integer);
    for (name, local) in [
        (SmolStr::new("capture"), capture),
        (SmolStr::new("domain"), domain),
        (SmolStr::new("items"), items),
        (SmolStr::new("backend"), backend),
    ] {
        lowerer.declare_local(name, local);
        lowerer.params.push(local);
    }

    let result = lowerer.new_local(
        SmolStr::new("$world_batch_result"),
        true,
        MirType::Named(SmolStr::new("List")),
    );

    let entry = lowerer.new_block();
    lowerer.current_block = entry;
    lowerer.push_stmt(MirStmt::Assign {
        place: Place::Local(result),
        value: Rvalue::BuildList {
            items: Vec::new(),
            alloc: AllocKind::Escaping,
        },
        span,
    });
    let (capture_scene_id, detail) =
        lower_world_domain_validation(&mut lowerer, capture, domain, semantics.query_name, span);

    let cpu_block = lowerer.new_block();
    let vgpu_block = lowerer.new_block();
    let wgsl_block = lowerer.new_block();
    let invalid_backend_block = lowerer.new_block();
    let merge_block = lowerer.new_block();
    lower_native_batch_backend_guard(
        &mut lowerer,
        backend,
        auto_backend,
        cpu_block,
        vgpu_block,
        wgsl_block,
        invalid_backend_block,
        span,
    );

    lowerer.current_block = invalid_backend_block;
    lower_wgsl_bridge_failure(
        &mut lowerer,
        SmolStr::new("world batch dispatch backend must be cpu, virtual_gpu, wgsl, or auto"),
        span,
    );

    lowerer.current_block = cpu_block;
    lowerer.lower_world_batch_query_loop(
        plan,
        Value::Local(items),
        Value::Local(capture),
        Value::Local(domain),
        Value::Const(Literal::Integer(0)),
        result,
        span,
        merge_block,
    );

    lowerer.current_block = vgpu_block;
    lowerer.lower_world_batch_query_loop(
        plan,
        Value::Local(items),
        Value::Local(capture),
        Value::Local(domain),
        Value::Const(Literal::Integer(1)),
        result,
        span,
        merge_block,
    );

    lowerer.current_block = wgsl_block;
    lower_world_region_dispatch(
        &mut lowerer,
        module,
        capture_scene_id,
        detail,
        merge_block,
        "world batch WGSL dispatch requires a capture created from a region declaration",
        span,
        |lowerer, shapes, span| {
            if let Some(value) = lower_world_batch_wgsl_bridge_call(
                lowerer,
                wgsl_config,
                plan.contract_id,
                shapes,
                wgsl_shape_indices,
                domain,
                items,
                span,
            ) {
                lowerer.assign_use(Place::Local(result), value, span);
                lowerer.set_terminator(Terminator::Jump {
                    target: merge_block,
                    span,
                });
            }
        },
    );

    lowerer.current_block = merge_block;
    lowerer.set_terminator(Terminator::Return {
        value: Some(Value::Local(result)),
        span,
    });

    MirFunction {
        name: helper_name,
        params: lowerer.params,
        abi_params: vec![
            PortableAbiType::Value,
            PortableAbiType::Value,
            PortableAbiType::Value,
            PortableAbiType::Value,
        ],
        abi_return: PortableAbiType::Value,
        locals: lowerer.locals,
        temps: lowerer.temps,
        blocks: lowerer.blocks,
        entry,
        suspendable: false,
    }
}
