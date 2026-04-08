use crate::kernel::ir::{
    KernelBatchQueryPlan, KernelBlock, KernelCaptureQueryPlan, KernelDispatchGrid, KernelExpr,
    KernelFunction, KernelModule, KernelPlanStage, KernelStmt, KernelWorldQueryPlan,
    ResolvedKernelDispatch,
};
use crate::portable::{
    BUILTIN_FIELD_PRIMITIVE_FUNCTIONS, BUILTIN_HELPER_FUNCTIONS, builtin_record_by_function,
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
    if !matches!(plan.stages.first(), Some(KernelPlanStage::SelectBackend)) {
        errors.push(KernelValidationError {
            message: format!(
                "batch query '{}' must start with SelectBackend",
                plan.helper_name
            ),
        });
    }
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
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
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
