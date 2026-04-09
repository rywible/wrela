use crate::kernel::ir::{
    KernelBatchItemContract, KernelBatchQueryPlan, KernelBlock, KernelCaptureQueryPlan,
    KernelDispatchGrid, KernelExpr, KernelFunction, KernelModule, KernelPlanStage, KernelStmt,
    KernelWorldQueryPlan, ResolvedKernelDispatch,
};
use crate::portable::{
    BUILTIN_FIELD_PRIMITIVE_FUNCTIONS, BUILTIN_HELPER_FUNCTIONS, builtin_record_by_function,
};
use crate::query_plan::{
    ArtifactContract, ArtifactSchema, DerivedArtifact, DispatchRecordContract, ResultRecordContract,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KernelValidationError {
    pub message: String,
}

pub fn validate_dispatch(
    dispatch: &ResolvedKernelDispatch,
) -> Result<(), Vec<KernelValidationError>> {
    let mut errors = Vec::new();
    validate_grid(dispatch.grid, &mut errors);
    if dispatch.kernel.is_empty() {
        errors.push(KernelValidationError {
            message: "dispatch kernel name must not be empty".to_string(),
        });
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

pub fn validate_module(module: &KernelModule) -> Result<(), Vec<KernelValidationError>> {
    let mut errors = Vec::new();
    if module.entry.is_empty() {
        errors.push(KernelValidationError {
            message: "kernel module entry must not be empty".to_string(),
        });
    } else if module.function(module.entry.as_str()).is_none() {
        errors.push(KernelValidationError {
            message: format!(
                "kernel module entry '{}' was not found in the module",
                module.entry.as_str()
            ),
        });
    }

    let mut seen = std::collections::BTreeSet::new();
    for function in &module.functions {
        if !seen.insert(function.name.clone()) {
            errors.push(KernelValidationError {
                message: format!("duplicate kernel function '{}'", function.name.as_str()),
            });
        }
        validate_function(module, function, &mut errors);
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

pub fn validate_batch_query_plan(
    plan: &KernelBatchQueryPlan,
) -> Result<(), Vec<KernelValidationError>> {
    let mut errors = Vec::new();
    validate_plan_stages(&plan.stages, &mut errors);
    validate_contract_version(plan.contract_version, "batch query", &mut errors);
    validate_artifact_contracts(
        &plan.artifact_contracts,
        Some(&plan.dispatch_contract),
        &plan.result_contract,
        &plan.derived_artifacts,
        &mut errors,
    );
    if !matches!(plan.stages.first(), Some(KernelPlanStage::SelectBackend)) {
        errors.push(KernelValidationError {
            message: format!(
                "batch query '{}' must start with SelectBackend",
                plan.helper_name
            ),
        });
    }
    validate_batch_item_contract(
        &plan.item_contract,
        &plan.result_contract,
        plan.contract_version,
        &mut errors,
    );
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

pub fn validate_capture_query_plan(
    plan: &KernelCaptureQueryPlan,
) -> Result<(), Vec<KernelValidationError>> {
    let mut errors = Vec::new();
    validate_plan_stages(&plan.stages, &mut errors);
    validate_contract_version(plan.contract_version, "capture query", &mut errors);
    validate_artifact_contracts(
        &plan.artifact_contracts,
        None,
        &plan.result_contract,
        &plan.derived_artifacts,
        &mut errors,
    );
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

pub fn validate_world_query_plan(
    plan: &KernelWorldQueryPlan,
) -> Result<(), Vec<KernelValidationError>> {
    let mut errors = Vec::new();
    validate_plan_stages(&plan.stages, &mut errors);
    validate_contract_version(plan.contract_version, "world query", &mut errors);
    validate_artifact_contracts(
        &plan.artifact_contracts,
        Some(&plan.dispatch_contract),
        &plan.result_contract,
        &plan.derived_artifacts,
        &mut errors,
    );
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn validate_contract_version(version: u32, label: &str, errors: &mut Vec<KernelValidationError>) {
    if version == 0 {
        errors.push(KernelValidationError {
            message: format!("{label} contract version must be greater than zero"),
        });
    }
}

fn validate_batch_item_contract(
    contract: &KernelBatchItemContract,
    result: &crate::query_plan::ResultRecordContract,
    batch_contract_version: u32,
    errors: &mut Vec<KernelValidationError>,
) {
    match contract {
        KernelBatchItemContract::CaptureQuery { plan } => {
            if let Err(plan_errors) = validate_capture_query_plan(plan) {
                errors.extend(plan_errors);
            }
            if plan.result_kind != result.result_kind {
                errors.push(KernelValidationError {
                    message: "batch capture item contract result kind does not match batch result contract".to_string(),
                });
            }
            if plan.contract_version != batch_contract_version {
                errors.push(KernelValidationError {
                    message: "batch capture item contract version does not match the parent batch contract".to_string(),
                });
            }
        }
        KernelBatchItemContract::TraceThenOcclusion { trace_plan } => {
            if let Err(plan_errors) = validate_capture_query_plan(trace_plan) {
                errors.extend(plan_errors);
            }
            if !matches!(
                result.result_kind,
                crate::query_plan::QueryResultKind::OcclusionResult
            ) {
                errors.push(KernelValidationError {
                    message: "TraceThenOcclusion contracts must produce OcclusionResult"
                        .to_string(),
                });
            }
            if !matches!(
                trace_plan.result_kind,
                crate::query_plan::QueryResultKind::Hit3
            ) {
                errors.push(KernelValidationError {
                    message: "TraceThenOcclusion contracts must embed a Hit3 trace plan"
                        .to_string(),
                });
            }
            if trace_plan.contract_version != batch_contract_version {
                errors.push(KernelValidationError {
                    message: "TraceThenOcclusion trace plan version does not match the parent batch contract".to_string(),
                });
            }
        }
    }
}

fn validate_artifact_contracts(
    artifacts: &[ArtifactContract],
    dispatch: Option<&DispatchRecordContract>,
    result: &ResultRecordContract,
    derived: &[DerivedArtifact],
    errors: &mut Vec<KernelValidationError>,
) {
    for artifact in artifacts {
        if artifact.id.is_empty() {
            errors.push(KernelValidationError {
                message: "artifact contracts must carry a stable id".to_string(),
            });
        }
        if artifact.version == 0 {
            errors.push(KernelValidationError {
                message: format!(
                    "artifact contract '{}' must have a non-zero version",
                    artifact.id
                ),
            });
        }
    }

    if let Some(dispatch) = dispatch {
        let has_dispatch_contract = artifacts.iter().any(|artifact| {
            matches!(
                artifact.schema,
                ArtifactSchema::DispatchRecord {
                    item_kind,
                    result_kind,
                } if item_kind == dispatch.item_kind && result_kind == dispatch.result_kind
            )
        });
        if !has_dispatch_contract {
            errors.push(KernelValidationError {
                message: "dispatch artifact contract does not match the dispatch record contract"
                    .to_string(),
            });
        }
    }

    let has_result_contract = artifacts.iter().any(|artifact| {
        matches!(
            artifact.schema,
            ArtifactSchema::HitResultBuffer {
                result_kind,
                preserves_local_hit_context,
            } if result_kind == result.result_kind
                && preserves_local_hit_context == result.preserves_local_hit_context
        )
    });
    if !has_result_contract {
        errors.push(KernelValidationError {
            message: "result artifact contract does not match the result record contract"
                .to_string(),
        });
    }

    for artifact in derived {
        let matches_schema = artifacts
            .iter()
            .any(|contract| match (&contract.schema, artifact) {
                (
                    ArtifactSchema::SupportSummary {
                        semantics,
                        support_class,
                        can_coarse_support_pruning,
                        semantic_root,
                        support_root,
                        node_count,
                        support_node_count,
                        leaf_count,
                        identity_source_count,
                    },
                    DerivedArtifact::SupportSummary {
                        semantics: expected_semantics,
                        support_class: expected_support_class,
                        can_coarse_support_pruning: expected_pruning,
                    },
                ) => {
                    semantics == expected_semantics
                        && support_class == expected_support_class
                        && can_coarse_support_pruning == expected_pruning
                        && (*semantic_root == 0 || *node_count != 0)
                        && (*support_root == 0 || *support_node_count != 0)
                        && *leaf_count <= *node_count
                        && *identity_source_count <= *node_count
                }
                (
                    ArtifactSchema::CaptureCache {
                        capture_kind,
                        semantic_root: _,
                    },
                    DerivedArtifact::CaptureCache {
                        capture_kind: expected_capture_kind,
                    },
                ) => capture_kind == expected_capture_kind,
                (
                    ArtifactSchema::CullingTable {
                        candidate_strategy,
                        pruning_strategy,
                        support_root,
                        support_node_count,
                        leaf_count,
                        identity_source_count,
                        ..
                    },
                    DerivedArtifact::CullingTable {
                        candidate_strategy: expected_candidate_strategy,
                        pruning_strategy: expected_pruning_strategy,
                    },
                ) => {
                    candidate_strategy == expected_candidate_strategy
                        && pruning_strategy == expected_pruning_strategy
                        && (*support_root == 0 || *support_node_count != 0)
                        && *leaf_count <= *support_node_count
                        && *identity_source_count <= *support_node_count
                }
                (
                    ArtifactSchema::OpaquePessimizationBoundary {
                        support_root,
                        support_node_count,
                    },
                    DerivedArtifact::OpaquePessimizationBoundary,
                ) => *support_root == 0 || *support_node_count != 0,
                _ => false,
            });
        if !matches_schema {
            errors.push(KernelValidationError {
                message: format!("missing artifact contract for derived artifact '{artifact:?}'"),
            });
        }
    }
}

fn validate_grid(grid: KernelDispatchGrid, errors: &mut Vec<KernelValidationError>) {
    if grid.total_count().is_none() {
        errors.push(KernelValidationError {
            message: "dispatch grid overflowed the host-side reference interpreter".to_string(),
        });
    }
}

fn validate_plan_stages(stages: &[KernelPlanStage], errors: &mut Vec<KernelValidationError>) {
    if stages.is_empty() {
        errors.push(KernelValidationError {
            message: "kernel plan must contain at least one stage".to_string(),
        });
        return;
    }

    let begin = stages
        .iter()
        .position(|stage| matches!(stage, KernelPlanStage::BeginVirtualGpuDispatch));
    let end = stages
        .iter()
        .position(|stage| matches!(stage, KernelPlanStage::EndVirtualGpuDispatch));
    if begin.is_some() != end.is_some() {
        errors.push(KernelValidationError {
            message: "virtual GPU dispatch scaffolding must include both begin and end stages"
                .to_string(),
        });
    }

    let execute = stages
        .iter()
        .position(|stage| matches!(stage, KernelPlanStage::Execute { .. }));
    let append = stages
        .iter()
        .position(|stage| matches!(stage, KernelPlanStage::AppendResult { .. }));
    let iterate = stages
        .iter()
        .position(|stage| matches!(stage, KernelPlanStage::IterateItems { .. }));
    let load_capture = stages
        .iter()
        .position(|stage| matches!(stage, KernelPlanStage::LoadCapture));

    if let Some(iterate) = iterate {
        if let Some(load_capture) = load_capture
            && iterate < load_capture
        {
            errors.push(KernelValidationError {
                message: "IterateItems must happen after LoadCapture".to_string(),
            });
        }
    }
    if let (Some(iterate), Some(execute)) = (iterate, execute)
        && execute < iterate
    {
        errors.push(KernelValidationError {
            message: "Execute must happen after IterateItems".to_string(),
        });
    }
    if let (Some(execute), Some(append)) = (execute, append)
        && append < execute
    {
        errors.push(KernelValidationError {
            message: "AppendResult must happen after Execute".to_string(),
        });
    }
    if let (Some(begin), Some(end)) = (begin, end)
        && end < begin
    {
        errors.push(KernelValidationError {
            message: "EndVirtualGpuDispatch must happen after BeginVirtualGpuDispatch".to_string(),
        });
    }
}

fn validate_function(
    module: &KernelModule,
    function: &KernelFunction,
    errors: &mut Vec<KernelValidationError>,
) {
    validate_block(module, &function.body, errors, 0, function.name.as_str());
}

fn validate_block(
    module: &KernelModule,
    block: &KernelBlock,
    errors: &mut Vec<KernelValidationError>,
    loop_depth: usize,
    function_name: &str,
) {
    for stmt in block {
        match stmt {
            KernelStmt::Let { value, .. }
            | KernelStmt::Assign { value, .. }
            | KernelStmt::Expr { value, .. }
            | KernelStmt::IgnoreResult { value, .. } => {
                validate_expr(module, value, errors, function_name);
            }
            KernelStmt::If {
                condition,
                then_block,
                else_block,
                ..
            } => {
                validate_expr(module, condition, errors, function_name);
                validate_block(module, then_block, errors, loop_depth, function_name);
                validate_block(module, else_block, errors, loop_depth, function_name);
            }
            KernelStmt::While {
                condition, body, ..
            } => {
                validate_expr(module, condition, errors, function_name);
                validate_block(module, body, errors, loop_depth + 1, function_name);
            }
            KernelStmt::Return { value, .. } => {
                if let Some(value) = value {
                    validate_expr(module, value, errors, function_name);
                }
            }
            KernelStmt::Break { .. } | KernelStmt::Continue { .. } if loop_depth == 0 => {
                errors.push(KernelValidationError {
                    message: format!(
                        "kernel function '{}' uses loop control outside of a loop",
                        function_name
                    ),
                });
            }
            KernelStmt::Break { .. } | KernelStmt::Continue { .. } => {}
        }
    }
}

fn validate_expr(
    module: &KernelModule,
    expr: &KernelExpr,
    errors: &mut Vec<KernelValidationError>,
    function_name: &str,
) {
    match expr {
        KernelExpr::Unary { expr, .. } | KernelExpr::Crash { expr, .. } => {
            validate_expr(module, expr, errors, function_name);
        }
        KernelExpr::Binary { lhs, rhs, .. } => {
            validate_expr(module, lhs, errors, function_name);
            validate_expr(module, rhs, errors, function_name);
        }
        KernelExpr::Call { target, args, .. } => {
            for arg in args {
                validate_expr(module, arg, errors, function_name);
            }
            if module.function(target.as_str()).is_none() && !looks_like_builtin(target.as_str()) {
                errors.push(KernelValidationError {
                    message: format!(
                        "kernel function '{}' calls unknown target '{}'",
                        function_name,
                        target.as_str()
                    ),
                });
            }
        }
        KernelExpr::CaptureQuery { plan, args, .. } => {
            for arg in args {
                validate_expr(module, arg, errors, function_name);
            }
            if let Err(plan_errors) = validate_capture_query_plan(plan) {
                errors.extend(plan_errors);
            }
        }
        KernelExpr::WorldQuery { plan, args, .. } => {
            for arg in args {
                validate_expr(module, arg, errors, function_name);
            }
            if let Err(plan_errors) = validate_world_query_plan(plan) {
                errors.extend(plan_errors);
            }
        }
        KernelExpr::BatchQuery { plan, args, .. } => {
            for arg in args {
                validate_expr(module, arg, errors, function_name);
            }
            if let Err(plan_errors) = validate_batch_query_plan(plan) {
                errors.extend(plan_errors);
            }
        }
        KernelExpr::Member { base, .. } => validate_expr(module, base, errors, function_name),
        KernelExpr::Index { base, index, .. } => {
            validate_expr(module, base, errors, function_name);
            validate_expr(module, index, errors, function_name);
        }
        KernelExpr::ArrayLiteral { items, .. } => {
            for item in items {
                validate_expr(module, item, errors, function_name);
            }
        }
        KernelExpr::StructLiteral { fields, .. } => {
            for (_, expr) in fields {
                validate_expr(module, expr, errors, function_name);
            }
        }
        KernelExpr::Literal { .. }
        | KernelExpr::Var { .. }
        | KernelExpr::Capture { .. }
        | KernelExpr::DispatchBackend { .. } => {}
    }
}

fn looks_like_builtin(name: &str) -> bool {
    builtin_record_by_function(name).is_some()
        || BUILTIN_HELPER_FUNCTIONS.contains(&name)
        || BUILTIN_FIELD_PRIMITIVE_FUNCTIONS.contains(&name)
        || matches!(
            name,
            "i32"
                | "u32"
                | "f32"
                | "vec2"
                | "vec3"
                | "vec4"
                | "quat"
                | "mat3_identity"
                | "mat3_cols"
                | "mat4_identity"
                | "mat4_cols"
                | "dot"
                | "cross"
                | "min"
                | "max"
                | "clamp"
                | "mix"
                | "abs"
                | "sign"
                | "floor"
                | "ceil"
                | "fract"
                | "sin"
                | "cos"
                | "sqrt"
                | "pow"
                | "length"
                | "normalize"
                | "distance"
                | "reflect"
                | "translate"
                | "rotate"
                | "uniform_scale"
                | "affine_transform"
                | "warp"
                | "repeat_linear"
                | "repeat_grid"
                | "radial_repeat"
                | "mirror_array"
                | "instance_array"
                | "global_invocation_id"
                | "local_invocation_id"
                | "workgroup_id"
                | "num_workgroups"
                | "workgroup_size"
                | "workgroup_barrier"
                | "storage_barrier"
                | "gpu_buffer_new"
                | "gpu_buffer_len"
                | "gpu_buffer_get"
                | "gpu_buffer_set"
                | "gpu_atomic_i32_new"
                | "gpu_atomic_i32_drop"
                | "gpu_atomic_i32_load"
                | "gpu_atomic_i32_store"
                | "gpu_atomic_i32_fetch_add"
                | "gpu_atomic_u32_new"
                | "gpu_atomic_u32_drop"
                | "gpu_atomic_u32_load"
                | "gpu_atomic_u32_store"
                | "gpu_atomic_u32_fetch_add"
        )
}
