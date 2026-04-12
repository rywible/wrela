use smol_str::SmolStr;
use wrela::hir;
use wrela::hir::lower as hir_lower;
use wrela::kernel::{
    KernelDispatchGrid, KernelDispatchSchedule, KernelExpr, KernelPlanStage, KernelRuntimeState,
    KernelStmt, KernelValue, ResolvedKernelDispatch, execute_dispatch, interpret_batch_query,
    interpret_dispatch, lower_batch_query_plan, lower_capture_query_plan,
    lower_kernel_entry_by_name, lower_world_query_plan, validate_batch_query_plan,
    validate_capture_query_plan, validate_dispatch, validate_module, validate_world_query_plan,
};
use wrela::parser::ast;
use wrela::parser::ast::AstNode;
use wrela::parser::parse;
use wrela::query_contract;
use wrela::query_plan::{
    ArtifactSchema, BatchQueryKind, BatchQueryPlan, CaptureKind, CaptureQueryKind,
    CaptureQueryPlan, DispatchBackend, WorldQueryKind, WorldQueryPlan,
};

fn lower_inline_module_from_source(source: &str) -> hir::Module {
    let node = parse(source);
    let root = ast::Root::cast(node).expect("root");
    hir_lower::lower(root)
}

#[test]
fn kernel_dispatch_interpreter_matches_reverse_schedule() {
    let dispatch = ResolvedKernelDispatch {
        kernel: SmolStr::new("run_kernel"),
        grid: KernelDispatchGrid {
            workgroups: [2, 1, 1],
            workgroup_size: [2, 1, 1],
        },
        schedule: KernelDispatchSchedule::Reverse,
        kernel_arg_count: 0,
    };
    assert!(validate_dispatch(&dispatch).is_ok());

    let invocations = interpret_dispatch(&dispatch);
    let observed = invocations
        .iter()
        .map(|invocation| {
            (
                invocation.global_id[0],
                invocation.workgroup_id[0],
                invocation.local_id[0],
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(observed, vec![(3, 1, 1), (2, 1, 0), (1, 0, 1), (0, 0, 0)]);
}

#[test]
fn kernel_dispatch_interpreter_matches_round_robin_workgroups() {
    let dispatch = ResolvedKernelDispatch {
        kernel: SmolStr::new("run_kernel"),
        grid: KernelDispatchGrid {
            workgroups: [2, 1, 1],
            workgroup_size: [2, 1, 1],
        },
        schedule: KernelDispatchSchedule::RoundRobinWorkgroups,
        kernel_arg_count: 0,
    };

    let invocations = interpret_dispatch(&dispatch);
    let observed = invocations
        .iter()
        .map(|invocation| {
            (
                invocation.global_id[0],
                invocation.workgroup_id[0],
                invocation.local_id[0],
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(observed, vec![(0, 0, 0), (2, 1, 0), (1, 0, 1), (3, 1, 1)]);
}

#[test]
fn batch_query_plan_lowers_into_kernel_contract_and_traces_iterations() {
    let plan =
        BatchQueryPlan::for_shape_query(BatchQueryKind::Trace, DispatchBackend::VirtualGpu, None);
    let kernel_plan = lower_batch_query_plan(&plan);
    assert!(validate_batch_query_plan(&kernel_plan).is_ok());
    assert_eq!(kernel_plan.contract_id, plan.contract_id);
    assert_eq!(kernel_plan.family, plan.family);
    assert_eq!(kernel_plan.surface, plan.surface);
    assert_eq!(kernel_plan.evidence_summary, plan.evidence_summary);
    assert!(kernel_plan.requires_virtual_gpu_dispatch());
    assert!(matches!(
        kernel_plan.item_contract,
        wrela::kernel::KernelBatchItemContract::CaptureQuery { .. }
    ));

    let trace = interpret_batch_query(&kernel_plan, 2);
    assert!(trace.begins_virtual_gpu_dispatch);
    assert!(trace.ends_virtual_gpu_dispatch);
    assert_eq!(trace.iterations.len(), 2);
    for iteration in &trace.iterations {
        assert!(
            iteration
                .stages
                .iter()
                .any(|stage| matches!(stage, wrela::kernel::KernelPlanStage::IterateItems { .. }))
        );
        assert!(
            iteration
                .stages
                .iter()
                .any(|stage| matches!(stage, wrela::kernel::KernelPlanStage::Execute { .. }))
        );
        assert!(
            iteration
                .stages
                .iter()
                .any(|stage| matches!(stage, wrela::kernel::KernelPlanStage::AppendResult { .. }))
        );
    }
}

#[test]
fn occluded_batch_query_lowers_ray_then_occlusion_contract() {
    let plan = BatchQueryPlan::for_shape_query(
        BatchQueryKind::Occluded,
        DispatchBackend::VirtualGpu,
        None,
    );
    let kernel_plan = lower_batch_query_plan(&plan);
    assert!(validate_batch_query_plan(&kernel_plan).is_ok());
    assert!(matches!(
        kernel_plan.item_contract,
        wrela::kernel::KernelBatchItemContract::RayThenOcclusion { .. }
    ));
}

#[test]
fn batch_query_validation_rejects_missing_dispatch_artifact_contract() {
    let mut plan = lower_batch_query_plan(&BatchQueryPlan::for_shape_query(
        BatchQueryKind::Trace,
        DispatchBackend::VirtualGpu,
        None,
    ));
    plan.artifact_contracts
        .retain(|artifact| !matches!(artifact.schema, ArtifactSchema::DispatchRecord { .. }));

    let errors = validate_batch_query_plan(&plan).expect_err("missing dispatch artifact");
    assert!(errors.iter().any(|error| {
        error
            .message
            .contains("dispatch artifact contract does not match")
    }));
}

#[test]
fn batch_query_validation_rejects_descriptor_family_mismatch() {
    let mut plan = lower_batch_query_plan(&BatchQueryPlan::for_shape_query(
        BatchQueryKind::Trace,
        DispatchBackend::VirtualGpu,
        None,
    ));
    plan.family = query_contract::QueryFamilyId::Surface;

    let errors = validate_batch_query_plan(&plan).expect_err("descriptor family mismatch");
    assert!(errors.iter().any(|error| {
        error.message.contains(plan.contract_id.as_str())
            && error.message.contains("v1")
            && error.message.contains("family")
    }));
}

#[test]
fn batch_query_validation_rejects_zero_version_and_unbalanced_dispatch_stages() {
    let mut plan = lower_batch_query_plan(&BatchQueryPlan::for_shape_query(
        BatchQueryKind::Trace,
        DispatchBackend::VirtualGpu,
        None,
    ));
    plan.contract_version = 0;
    plan.stages
        .retain(|stage| !matches!(stage, KernelPlanStage::EndVirtualGpuDispatch));

    let errors = validate_batch_query_plan(&plan).expect_err("invalid batch plan");
    assert!(errors.iter().any(|error| {
        error
            .message
            .contains("contract version must be greater than zero")
            && error.message.contains(plan.contract_id.as_str())
            && error.message.contains("v0")
    }));
    assert!(
        errors
            .iter()
            .any(|error| { error.message.contains("both begin and end stages") })
    );
}

#[test]
fn batch_query_validation_rejects_invalid_nested_item_contracts() {
    let mut plan = lower_batch_query_plan(&BatchQueryPlan::for_shape_query(
        BatchQueryKind::Trace,
        DispatchBackend::VirtualGpu,
        None,
    ));
    let wrela::kernel::KernelBatchItemContract::CaptureQuery { plan: nested } =
        &mut plan.item_contract
    else {
        panic!("expected capture item contract");
    };
    nested.contract_version = 0;
    nested
        .artifact_contracts
        .retain(|artifact| !matches!(artifact.schema, ArtifactSchema::HitResultBuffer { .. }));

    let errors = validate_batch_query_plan(&plan).expect_err("invalid nested capture plan");
    assert!(errors.iter().any(|error| {
        error.message.contains("capture query contract")
            && error
                .message
                .contains("contract version must be greater than zero")
            && error.message.contains("v0")
    }));
    assert!(errors.iter().any(|error| {
        error
            .message
            .contains("result artifact contract does not match the result record contract")
    }));
    assert!(errors.iter().any(|error| {
        error.message.contains("capture item contract version")
            && error
                .message
                .contains("does not match the parent batch contract")
    }));
}

#[test]
fn capture_query_plan_lowers_into_kernel_contract() {
    let plan =
        CaptureQueryPlan::for_query(CaptureQueryKind::Trace, CaptureKind::Shape, None).unwrap();
    let kernel_plan = lower_capture_query_plan(&plan);
    assert!(validate_capture_query_plan(&kernel_plan).is_ok());
    assert_eq!(kernel_plan.contract_id, plan.contract_id);
    assert_eq!(kernel_plan.family, plan.family);
    assert_eq!(kernel_plan.surface, plan.surface);
    assert!(
        kernel_plan
            .stages
            .iter()
            .any(|stage| matches!(stage, wrela::kernel::KernelPlanStage::LoadCapture))
    );
    assert!(
        kernel_plan
            .stages
            .iter()
            .any(|stage| matches!(stage, wrela::kernel::KernelPlanStage::Execute { .. }))
    );
    assert!(kernel_plan.preserves_local_hit_context);
}

#[test]
fn capture_query_validation_rejects_descriptor_surface_mismatch() {
    let mut plan = lower_capture_query_plan(
        &CaptureQueryPlan::for_query(CaptureQueryKind::Trace, CaptureKind::Shape, None).unwrap(),
    );
    plan.surface = query_contract::QuerySurfaceKind::WorldScalar;

    let errors = validate_capture_query_plan(&plan).expect_err("descriptor surface mismatch");
    assert!(errors.iter().any(|error| error.message.contains("surface")));
}

#[test]
fn query_plan_validation_rejects_target_and_cardinality_drift() {
    let mut capture_target_plan = lower_capture_query_plan(
        &CaptureQueryPlan::for_query(CaptureQueryKind::Trace, CaptureKind::Shape, None).unwrap(),
    );
    capture_target_plan.target = query_contract::QueryTargetKind::World;
    let capture_target_errors =
        validate_capture_query_plan(&capture_target_plan).expect_err("capture target drift");
    assert!(
        capture_target_errors
            .iter()
            .any(|error| error.message.contains("target"))
    );

    let mut capture_cardinality_plan = lower_capture_query_plan(
        &CaptureQueryPlan::for_query(CaptureQueryKind::Trace, CaptureKind::Shape, None).unwrap(),
    );
    capture_cardinality_plan.cardinality = query_contract::QueryCardinality::Batch;
    let capture_cardinality_errors = validate_capture_query_plan(&capture_cardinality_plan)
        .expect_err("capture cardinality drift");
    assert!(
        capture_cardinality_errors
            .iter()
            .any(|error| error.message.contains("cardinality"))
    );

    let mut world_target_plan =
        lower_world_query_plan(&WorldQueryPlan::for_query(WorldQueryKind::Nearest));
    world_target_plan.target = query_contract::QueryTargetKind::Capture;
    let world_target_errors =
        validate_world_query_plan(&world_target_plan).expect_err("world target drift");
    assert!(
        world_target_errors
            .iter()
            .any(|error| error.message.contains("target"))
    );

    let mut batch_cardinality_plan = lower_batch_query_plan(&BatchQueryPlan::for_shape_query(
        BatchQueryKind::Nearest,
        DispatchBackend::Auto,
        None,
    ));
    batch_cardinality_plan.cardinality = query_contract::QueryCardinality::Scalar;
    let batch_cardinality_errors =
        validate_batch_query_plan(&batch_cardinality_plan).expect_err("batch cardinality drift");
    assert!(
        batch_cardinality_errors
            .iter()
            .any(|error| error.message.contains("cardinality"))
    );
}

#[test]
fn capture_query_validation_rejects_participant_family_mismatch() {
    let mut plan = lower_capture_query_plan(
        &CaptureQueryPlan::for_query(CaptureQueryKind::Radiance, CaptureKind::Shape, None).unwrap(),
    );
    plan.participant_contract
        .as_mut()
        .expect("participant contract")
        .kind = CaptureQueryKind::Medium;

    let errors = validate_capture_query_plan(&plan).expect_err("participant family mismatch");
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("participant selection"))
    );
}

#[test]
fn query_plan_validation_rejects_legacy_kind_descriptor_mismatch() {
    let mut capture_plan = lower_capture_query_plan(
        &CaptureQueryPlan::for_query(CaptureQueryKind::Nearest, CaptureKind::Shape, None).unwrap(),
    );
    capture_plan.kind = CaptureQueryKind::Normal;
    let capture_errors =
        validate_capture_query_plan(&capture_plan).expect_err("capture kind mismatch");
    assert!(
        capture_errors
            .iter()
            .any(|error| error.message.contains("legacy kind"))
    );

    let mut world_plan =
        lower_world_query_plan(&WorldQueryPlan::for_query(WorldQueryKind::Nearest));
    world_plan.kind = WorldQueryKind::Surface;
    let world_errors = validate_world_query_plan(&world_plan).expect_err("world kind mismatch");
    assert!(
        world_errors
            .iter()
            .any(|error| error.message.contains("legacy kind"))
    );

    let mut batch_plan = lower_batch_query_plan(&BatchQueryPlan::for_shape_query(
        BatchQueryKind::Nearest,
        DispatchBackend::Auto,
        None,
    ));
    batch_plan.kind = BatchQueryKind::Surface;
    let batch_errors = validate_batch_query_plan(&batch_plan).expect_err("batch kind mismatch");
    assert!(
        batch_errors
            .iter()
            .any(|error| error.message.contains("legacy kind"))
    );
}

#[test]
fn query_plan_validation_rejects_capture_kind_descriptor_mismatch() {
    let mut capture_plan = lower_capture_query_plan(
        &CaptureQueryPlan::for_query(CaptureQueryKind::Nearest, CaptureKind::Shape, None).unwrap(),
    );
    capture_plan.capture_kind = CaptureKind::Field;
    let capture_errors =
        validate_capture_query_plan(&capture_plan).expect_err("capture kind mismatch");
    assert!(
        capture_errors
            .iter()
            .any(|error| error.message.contains("capture kind"))
    );

    let mut batch_plan = lower_batch_query_plan(&BatchQueryPlan::for_shape_query(
        BatchQueryKind::Nearest,
        DispatchBackend::Auto,
        None,
    ));
    batch_plan.capture_kind = CaptureKind::Field;
    let batch_errors = validate_batch_query_plan(&batch_plan).expect_err("batch capture mismatch");
    assert!(
        batch_errors
            .iter()
            .any(|error| error.message.contains("capture kind"))
    );
}

#[test]
fn world_query_plan_lowers_into_kernel_contract() {
    let plan = WorldQueryPlan::for_query_with_backend(
        WorldQueryKind::Trace,
        wrela::query_plan::DispatchBackend::Wgsl,
    );
    let kernel_plan = lower_world_query_plan(&plan);
    assert!(validate_world_query_plan(&kernel_plan).is_ok());
    assert_eq!(kernel_plan.contract_id, plan.contract_id);
    assert_eq!(kernel_plan.family, plan.family);
    assert_eq!(kernel_plan.surface, plan.surface);
    assert_eq!(
        kernel_plan.backend,
        wrela::query_plan::DispatchBackend::Wgsl
    );
    assert_eq!(
        kernel_plan.dispatch_contract.backend,
        wrela::query_plan::DispatchBackend::Wgsl
    );
    assert!(matches!(
        kernel_plan.stages.first(),
        Some(wrela::kernel::KernelPlanStage::SelectBackend)
    ));
    assert!(
        kernel_plan
            .stages
            .iter()
            .any(|stage| matches!(stage, wrela::kernel::KernelPlanStage::LoadCapture))
    );
    assert!(
        kernel_plan
            .stages
            .iter()
            .any(|stage| matches!(stage, wrela::kernel::KernelPlanStage::LoadDomainFlags))
    );
    assert!(kernel_plan.stages.iter().any(|stage| matches!(
        stage,
        wrela::kernel::KernelPlanStage::GenerateCandidates {
            strategy: wrela::query_plan::CandidateStrategy::SupportAcceleratedShapeTraversal
        }
    )));
    assert!(kernel_plan.stages.iter().any(|stage| matches!(
        stage,
        wrela::kernel::KernelPlanStage::PruneCandidates {
            strategy: wrela::query_plan::PruningStrategy::SupportLowerBound
        }
    )));
    assert!(
        kernel_plan
            .stages
            .iter()
            .any(|stage| matches!(stage, wrela::kernel::KernelPlanStage::AssembleHitContext))
    );
    assert!(
        kernel_plan
            .stages
            .iter()
            .any(|stage| matches!(stage, wrela::kernel::KernelPlanStage::AppendResult { .. }))
    );
    assert_eq!(
        kernel_plan.candidate_strategy,
        wrela::query_plan::CandidateStrategy::SupportAcceleratedShapeTraversal
    );
    assert_eq!(
        kernel_plan.pruning_strategy,
        wrela::query_plan::PruningStrategy::SupportLowerBound
    );
    assert!(
        kernel_plan
            .derived_artifacts
            .iter()
            .any(|artifact| matches!(
                artifact,
                wrela::query_plan::DerivedArtifact::CaptureCache {
                    capture_kind: wrela::query_plan::CaptureKind::Region
                }
            ))
    );
    assert!(
        kernel_plan
            .derived_artifacts
            .iter()
            .any(|artifact| matches!(
                artifact,
                wrela::query_plan::DerivedArtifact::CullingTable { .. }
            ))
    );
    assert!(
        kernel_plan
            .artifact_contracts
            .iter()
            .any(|artifact| matches!(
                artifact.schema,
                ArtifactSchema::CullingTable {
                    candidate_strategy:
                        wrela::query_plan::CandidateStrategy::SupportAcceleratedShapeTraversal,
                    pruning_strategy: wrela::query_plan::PruningStrategy::SupportLowerBound,
                    ..
                }
            ))
    );
}

#[test]
fn world_query_validation_rejects_unknown_contract_id() {
    let mut plan = lower_world_query_plan(&WorldQueryPlan::for_query(WorldQueryKind::Trace));
    plan.contract_id = query_contract::QueryContractId::new("missing.world.contract");

    let errors = validate_world_query_plan(&plan).expect_err("missing contract id");
    assert!(errors.iter().any(|error| {
        error
            .message
            .contains("was not found in the query registry")
            && error.message.contains("missing.world.contract")
            && error.message.contains("v1")
    }));
}

#[test]
fn kernel_lowering_preserves_dynamic_world_query_backend_arguments() {
    let source = r#"
kernel fn world_query_backend_passthrough(
    world_capture: RegionCapture,
    domain: SceneDomain,
    query_backend: DispatchBackend
) -> F32 {
    return distance_world(
        capture=world_capture,
        domain=domain,
        point=vec3(0.0, 0.0, 0.0),
        backend=query_backend
    )
}
"#;

    let module = lower_inline_module_from_source(source);
    let semantic = hir::semantic::check_module(&module);
    assert!(
        semantic.errors.is_empty(),
        "semantic errors: {:?}",
        semantic.errors
    );
    let (type_errors, type_info) = hir::typeck::check_module_with_info(&module);
    assert!(type_errors.is_empty(), "type errors: {type_errors:?}");
    let program =
        lower_kernel_entry_by_name(&module, &type_info, "world_query_backend_passthrough")
            .expect("kernel program");
    let function = program
        .function("world_query_backend_passthrough")
        .expect("lowered kernel function");
    let KernelStmt::Return {
        value: Some(KernelExpr::WorldQuery { plan, args, .. }),
        ..
    } = function.body.last().expect("kernel return")
    else {
        panic!(
            "expected lowered world query return, got: {:?}",
            function.body
        );
    };
    assert_eq!(plan.backend, DispatchBackend::Auto);
    assert_eq!(args.len(), 4);
    assert!(matches!(
        args.last(),
        Some(KernelExpr::Var { name, .. }) if name.as_str() == "query_backend"
    ));
}

#[test]
fn executable_kernel_ir_lowers_compute_body_and_struct_literals() {
    let source = r#"
value Pair {
    x: I32
    y: I32
}

kernel fn helper(seed: I32) -> I32 {
    return seed + i32(1)
}

kernel fn run_portable_kernel(output: GpuBuffer[I32]) -> Nothing {
    gid = global_invocation_id()
    pair = Pair(x=i32(gid[0]), y=i32(2))
    gpu_buffer_set(buffer=output, index=gid[0], value=helper(seed=pair.x))
}
"#;

    let module = lower_inline_module_from_source(source);
    let semantic = hir::semantic::check_module(&module);
    assert!(
        semantic.errors.is_empty(),
        "semantic errors: {:?}",
        semantic.errors
    );
    let (type_errors, type_info) = hir::typeck::check_module_with_info(&module);
    assert!(type_errors.is_empty(), "type errors: {type_errors:?}");

    let kernel = lower_kernel_entry_by_name(&module, &type_info, "run_portable_kernel")
        .expect("lower kernel");
    assert!(validate_module(&kernel).is_ok(), "kernel validation failed");
    assert!(
        kernel.function("helper").is_some(),
        "expected helper callee in kernel module"
    );
    let entry = kernel
        .function("run_portable_kernel")
        .expect("missing run_portable_kernel");

    assert!(matches!(
        &entry.body[0],
        KernelStmt::Let {
            name,
            value: KernelExpr::Call { target, .. },
            ..
        } if name == "gid" && target == "global_invocation_id"
    ));
    assert!(matches!(
        &entry.body[1],
        KernelStmt::Let {
            name,
            value: KernelExpr::StructLiteral { name: struct_name, fields, .. },
            ..
        } if name == "pair"
            && struct_name == "Pair"
            && fields.iter().map(|(field, _)| field.as_str()).collect::<Vec<_>>() == vec!["x", "y"]
    ));
    assert!(matches!(
        &entry.body[2],
        KernelStmt::Expr {
            value: KernelExpr::Call { target, args, .. },
            ..
        } if target == "gpu_buffer_set" && args.len() == 3
    ));
}

#[test]
fn executable_kernel_ir_canonicalizes_named_intrinsic_arguments() {
    let source = r#"
kernel fn vector_kernel() -> Vec3 {
    return vec3(z=3.0, x=1.0, y=2.0)
}
"#;

    let module = lower_inline_module_from_source(source);
    let semantic = hir::semantic::check_module(&module);
    assert!(
        semantic.errors.is_empty(),
        "semantic errors: {:?}",
        semantic.errors
    );
    let (type_errors, type_info) = hir::typeck::check_module_with_info(&module);
    assert!(type_errors.is_empty(), "type errors: {type_errors:?}");

    let kernel =
        lower_kernel_entry_by_name(&module, &type_info, "vector_kernel").expect("lower kernel");
    assert!(validate_module(&kernel).is_ok(), "kernel validation failed");
    let entry = kernel.function("vector_kernel").expect("vector_kernel");

    let KernelStmt::Return {
        value: Some(KernelExpr::Call { target, args, .. }),
        ..
    } = &entry.body[0]
    else {
        panic!("expected intrinsic call in return");
    };
    assert_eq!(target, "vec3");
    assert_eq!(args.len(), 3);
    assert!(
        matches!(&args[0], KernelExpr::Literal { value: hir::Literal::Float(value), .. } if (*value - 1.0).abs() < 0.0001)
    );
    assert!(
        matches!(&args[1], KernelExpr::Literal { value: hir::Literal::Float(value), .. } if (*value - 2.0).abs() < 0.0001)
    );
    assert!(
        matches!(&args[2], KernelExpr::Literal { value: hir::Literal::Float(value), .. } if (*value - 3.0).abs() < 0.0001)
    );
}

#[test]
fn executable_kernel_dispatch_runs_with_scheduled_invocations_and_shared_state() {
    let source = r#"
kernel fn add_one(value: I32) -> I32 {
    return value + i32(1)
}

kernel fn run_kernel(output: GpuBuffer[I32], counter: GpuAtomicI32) -> Nothing {
    gid = global_invocation_id()
    previous = gpu_atomic_i32_fetch_add(
        atomic=counter,
        delta=i32(1)
    )
    gpu_buffer_set(
        buffer=output,
        index=gid[0],
        value=add_one(value=previous)
    )
}
"#;

    let module = lower_inline_module_from_source(source);
    let semantic = hir::semantic::check_module(&module);
    assert!(
        semantic.errors.is_empty(),
        "semantic errors: {:?}",
        semantic.errors
    );
    let (type_errors, type_info) = hir::typeck::check_module_with_info(&module);
    assert!(type_errors.is_empty(), "type errors: {type_errors:?}");

    let kernel =
        lower_kernel_entry_by_name(&module, &type_info, "run_kernel").expect("lower kernel");
    assert!(validate_module(&kernel).is_ok(), "kernel validation failed");

    let output_dispatch = ResolvedKernelDispatch {
        kernel: SmolStr::new("run_kernel"),
        grid: KernelDispatchGrid {
            workgroups: [2, 1, 1],
            workgroup_size: [2, 1, 1],
        },
        schedule: KernelDispatchSchedule::Reverse,
        kernel_arg_count: 2,
    };

    let mut runtime = KernelRuntimeState::default();
    let output = runtime
        .create_buffer(4, KernelValue::I32(0), hir::Type::I32)
        .expect("create output buffer");
    let counter = runtime.create_atomic_i32(0);

    let invocations = execute_dispatch(
        &kernel,
        &output_dispatch,
        vec![
            KernelValue::GpuBuffer(output),
            KernelValue::GpuAtomicI32(counter),
        ],
        &mut runtime,
    )
    .expect("execute dispatch");

    assert_eq!(
        invocations
            .iter()
            .map(|invocation| invocation.global_id[0])
            .collect::<Vec<_>>(),
        vec![3, 2, 1, 0]
    );

    let buffer = runtime.buffer(output).expect("output buffer");
    assert_eq!(
        buffer.elements,
        vec![
            KernelValue::I32(4),
            KernelValue::I32(3),
            KernelValue::I32(2),
            KernelValue::I32(1)
        ]
    );
    assert_eq!(runtime.atomic_i32_value(counter), Some(4));
}

#[test]
fn executable_kernel_dispatch_exposes_invocation_builtins() {
    let source = r#"
kernel fn run_kernel(snapshot: GpuBuffer[I32]) -> Nothing {
    gid = global_invocation_id()
    lid = local_invocation_id()
    wid = workgroup_id()
    num = num_workgroups()
    size = workgroup_size()

    if gid[0] == u32(0) and lid[0] == u32(0) and wid[0] == u32(0) {
        gpu_buffer_set(buffer=snapshot, index=i32(0), value=i32(gid[0]))
        gpu_buffer_set(buffer=snapshot, index=i32(1), value=i32(lid[0]))
        gpu_buffer_set(buffer=snapshot, index=i32(2), value=i32(wid[0]))
        gpu_buffer_set(buffer=snapshot, index=i32(3), value=i32(num[0]))
        gpu_buffer_set(buffer=snapshot, index=i32(4), value=i32(size[0]))
    }
}
"#;

    let module = lower_inline_module_from_source(source);
    let semantic = hir::semantic::check_module(&module);
    assert!(
        semantic.errors.is_empty(),
        "semantic errors: {:?}",
        semantic.errors
    );
    let (type_errors, type_info) = hir::typeck::check_module_with_info(&module);
    assert!(type_errors.is_empty(), "type errors: {type_errors:?}");

    let kernel =
        lower_kernel_entry_by_name(&module, &type_info, "run_kernel").expect("lower kernel");
    assert!(validate_module(&kernel).is_ok(), "kernel validation failed");

    let dispatch = ResolvedKernelDispatch {
        kernel: SmolStr::new("run_kernel"),
        grid: KernelDispatchGrid {
            workgroups: [2, 1, 1],
            workgroup_size: [2, 1, 1],
        },
        schedule: KernelDispatchSchedule::Deterministic,
        kernel_arg_count: 1,
    };

    let mut runtime = KernelRuntimeState::default();
    let snapshot = runtime
        .create_buffer(5, KernelValue::I32(0), hir::Type::I32)
        .expect("create snapshot buffer");

    execute_dispatch(
        &kernel,
        &dispatch,
        vec![KernelValue::GpuBuffer(snapshot)],
        &mut runtime,
    )
    .expect("execute dispatch");

    let buffer = runtime.buffer(snapshot).expect("snapshot buffer");
    assert_eq!(
        buffer.elements,
        vec![
            KernelValue::I32(0),
            KernelValue::I32(0),
            KernelValue::I32(0),
            KernelValue::I32(2),
            KernelValue::I32(2)
        ]
    );
}
