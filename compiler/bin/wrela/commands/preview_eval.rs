//! Owns preview/presentation evaluation helpers that bind authored view inputs
//! into executable presentation plans.
//! Does not own CLI parsing or the final frame execution/report rendering.
//!
//! Key invariants:
//! - bound preview values must preserve authored presentation semantics closely
//!   enough for debug/report surfaces to explain what executed.
//! - helper evaluation may simplify compatibility inputs, but it must not invent
//!   view/domain bindings that the source program did not declare.
//! - export-attachment stripping only removes export bookkeeping, never the data
//!   dependencies needed by execution.
//!
//! Primary entrypoints:
//! - `prepare_presentation_execution`
//! - `preview_eval_body`
//! - `preview_eval_expr`
//!
//! Failure modes / common pitfalls:
//! - treating preview evaluation as a general interpreter would blur the
//!   boundary between authored inputs and runtime execution.
//! - losing canonical parameter binding order here makes downstream reports hard
//!   to reconcile with the original source.

use super::presentation_command::{
    PreparedPresentationExecution, body_terminal_expr_id, domain_execution_inputs,
    helper_call_named_expr_id, resolve_view_dimension,
};
use super::*;

pub(crate) type PreviewEvalBindings = HashMap<SmolStr, wrela::kernel::KernelValue>;

pub(crate) fn prepare_presentation_execution(
    module: &hir::Module,
    query_ctx: &wrela::query_exec::QueryExecContext,
    base_plan: &wrela::presentation_plan::PresentationPlan,
    view_func: &hir::Function,
    region_name: SmolStr,
    domain_name: SmolStr,
    camera: wrela::presentation_contract::CanonicalCameraInput,
    width_override: Option<u32>,
    height_override: Option<u32>,
    frame_index: u32,
    delta_seconds: f32,
    query_backend: wrela::query_plan::DispatchBackend,
    query_trace_solver_mode: wrela::query_exec::QueryTraceSolverMode,
    disable_export_attachment: bool,
) -> Result<PreparedPresentationExecution, String> {
    let region_snapshot = query_ctx
        .region_snapshot_handle(&region_name)
        .cloned()
        .ok_or_else(|| format!("missing region snapshot for `{region_name}`"))?;
    let domain_func = module
        .functions
        .iter()
        .find(|(_, func)| func.name == domain_name && func.role == hir::FunctionRole::Domain)
        .map(|(_, func)| func)
        .ok_or_else(|| format!("missing domain `{domain_name}`"))?;
    let width = resolve_view_dimension(view_func, width_override, true)?;
    let height = resolve_view_dimension(view_func, height_override, false)?;
    let domain_inputs = domain_execution_inputs(module, domain_func, &region_name, query_backend)?;
    let mut plan = base_plan.clone();
    let domain_metadata = domain_func
        .domain
        .as_ref()
        .ok_or_else(|| format!("selected domain `{domain_name}` is missing domain metadata"))?;
    plan.apply_participant_policy(domain_metadata.radiance, domain_metadata.media);
    if disable_export_attachment {
        strip_presentation_export_attachment(&mut plan);
    }
    let validation_errors = plan.validate();
    if !validation_errors.is_empty() {
        return Err(format!(
            "presentation execution plan `{}` failed validation after participant policy: {}",
            plan.name,
            validation_errors
                .into_iter()
                .map(|err| err.message.to_string())
                .collect::<Vec<_>>()
                .join("; ")
        ));
    }
    let bindings = bind_presentation_function_params(view_func, &region_snapshot, camera);
    let lighting = authored_presentation_lighting_inputs(view_func, &bindings)?;
    let compatibility_projection =
        authored_compatibility_projection_input(&plan, view_func, &bindings, camera)?;
    let frame_state = wrela::presentation_exec::frame_state_value(
        camera,
        camera,
        wrela::presentation_contract::CanonicalViewportInput { width, height },
        [0.0, 0.0],
        frame_index,
        delta_seconds,
    );
    Ok(PreparedPresentationExecution {
        plan,
        input: wrela::presentation_exec::PresentationExecutionInput {
            region_snapshot,
            frame_domain: domain_inputs.frame_domain,
            frame_state,
            history: None,
            resident_history_attachments: None,
            materialize_cpu_attachments: true,
            runtime_summary_only: false,
            collect_gpu_timing_readback: true,
            lighting,
            compatibility_projection,
            execution_policy: domain_inputs.execution_policy,
            query_trace_solver_mode,
            quality_override: None,
            backend: query_backend,
        },
        semantic_domain: domain_inputs.semantic_domain,
        execution_policy: domain_inputs.execution_policy,
        camera,
        viewport: wrela::presentation_contract::CanonicalViewportInput { width, height },
    })
}

pub(crate) fn strip_presentation_export_attachment(
    plan: &mut wrela::presentation_plan::PresentationPlan,
) {
    let export_binding_ids = plan
        .passes
        .iter()
        .filter_map(|pass| match &pass.kind {
            wrela::presentation_plan::PresentationPassKind::ExportAttachment { .. } => {
                pass.binding.clone()
            }
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    plan.passes.retain(|pass| {
        !matches!(
            pass.kind,
            wrela::presentation_plan::PresentationPassKind::ExportAttachment { .. }
        )
    });
    if export_binding_ids.is_empty() {
        return;
    }
    plan.bindings
        .retain(|binding| !export_binding_ids.contains(&binding.id));
}

pub(crate) fn bind_presentation_function_params(
    function: &hir::Function,
    region_snapshot: &wrela::world_identity::WorldSnapshotHandle,
    camera: wrela::presentation_contract::CanonicalCameraInput,
) -> PreviewEvalBindings {
    let mut bindings = PreviewEvalBindings::new();
    for param in &function.params {
        match param.ty.as_ref().map(|ty| ty.name.as_str()) {
            Some("RegionCapture") => {
                bindings.insert(param.name.clone(), region_snapshot.capture_value());
            }
            Some("Camera") => {
                bindings.insert(param.name.clone(), preview_camera_value(camera));
            }
            _ => {}
        }
    }
    bindings
}

pub(crate) fn authored_presentation_lighting_inputs(
    view_func: &hir::Function,
    bindings: &PreviewEvalBindings,
) -> Result<wrela::presentation_contract::PresentationLightingInputs, String> {
    let metadata = view_func.presentation.as_ref().ok_or_else(|| {
        format!(
            "selected view `{}` is missing presentation metadata",
            view_func.name
        )
    })?;
    if metadata.lighting.lights.is_some() {
        return Err(format!(
            "presentation execution does not yet support plural `lights` metadata on `{}`; author `key_light` instead",
            view_func.name
        ));
    }
    let grouped = metadata.lighting.grouped.as_ref();
    let key_light = match grouped
        .and_then(|body| {
            helper_call_named_expr_id(body, "key_light", "light").map(|expr_id| (body, expr_id))
        })
        .or_else(|| {
            metadata
                .lighting
                .light
                .as_ref()
                .and_then(|body| body_terminal_expr_id(body).map(|expr_id| (body, expr_id)))
        }) {
        Some((body, expr_id)) => preview_expect_light(
            &preview_eval_expr(body, expr_id, bindings, "presentation lighting key_light")?,
            "presentation lighting key_light",
        )?,
        None => default_preview_key_light(),
    };
    let fill_direction = match grouped
        .and_then(|body| {
            helper_call_named_expr_id(body, "key_light", "fill_direction")
                .map(|expr_id| (body, expr_id))
        })
        .or_else(|| {
            metadata
                .lighting
                .fill_dir
                .as_ref()
                .and_then(|body| body_terminal_expr_id(body).map(|expr_id| (body, expr_id)))
        }) {
        Some((body, expr_id)) => preview_expect_vec3(
            &preview_eval_expr(
                body,
                expr_id,
                bindings,
                "presentation lighting fill_direction",
            )?,
            "presentation lighting fill_direction",
        )?,
        None => normalize_preview_vec3([-0.9, 0.45, 0.2]),
    };
    let fill_strength = match grouped
        .and_then(|body| {
            helper_call_named_expr_id(body, "key_light", "fill_strength")
                .map(|expr_id| (body, expr_id))
        })
        .or_else(|| {
            metadata
                .lighting
                .fill_strength
                .as_ref()
                .and_then(|body| body_terminal_expr_id(body).map(|expr_id| (body, expr_id)))
        }) {
        Some((body, expr_id)) => preview_expect_f32(
            &preview_eval_expr(
                body,
                expr_id,
                bindings,
                "presentation lighting fill_strength",
            )?,
            "presentation lighting fill_strength",
        )?,
        None => 0.22,
    };
    let ambient_color = match grouped
        .and_then(|body| {
            helper_call_named_expr_id(body, "key_light", "ambient_color")
                .map(|expr_id| (body, expr_id))
        })
        .or_else(|| {
            metadata
                .lighting
                .ambient_color
                .as_ref()
                .and_then(|body| body_terminal_expr_id(body).map(|expr_id| (body, expr_id)))
        }) {
        Some((body, expr_id)) => preview_expect_vec3(
            &preview_eval_expr(
                body,
                expr_id,
                bindings,
                "presentation lighting ambient_color",
            )?,
            "presentation lighting ambient_color",
        )?,
        None => [0.12, 0.12, 0.12],
    };
    Ok(wrela::presentation_contract::PresentationLightingInputs {
        key_light,
        fill_direction,
        fill_strength,
        ambient_color,
    })
}

pub(crate) fn authored_compatibility_projection_input(
    plan: &wrela::presentation_plan::PresentationPlan,
    view_func: &hir::Function,
    bindings: &PreviewEvalBindings,
    camera: wrela::presentation_contract::CanonicalCameraInput,
) -> Result<Option<wrela::presentation_contract::LegacyCompatibilityProjectionInput>, String> {
    if !plan.view.compatibility_projection.legacy_path_active {
        return Ok(None);
    }
    let metadata = view_func.presentation.as_ref().ok_or_else(|| {
        format!(
            "selected view `{}` is missing presentation metadata",
            view_func.name
        )
    })?;
    let world_up = match metadata.compatibility.world_up.as_ref() {
        Some(body) => preview_expect_vec3(
            &preview_eval_body(body, bindings, "presentation compatibility world_up")?,
            "presentation compatibility world_up",
        )?,
        None => camera.up,
    };
    let view_scale = match metadata.compatibility.view_scale.as_ref() {
        Some(body) => preview_expect_f32(
            &preview_eval_body(body, bindings, "presentation compatibility view_scale")?,
            "presentation compatibility view_scale",
        )?,
        None => 0.72,
    };
    Ok(Some(
        wrela::presentation_contract::LegacyCompatibilityProjectionInput {
            world_up,
            view_scale,
        },
    ))
}

pub(crate) fn preview_eval_body(
    body: &hir::Body,
    base_bindings: &PreviewEvalBindings,
    context: &str,
) -> Result<wrela::kernel::KernelValue, String> {
    let mut bindings = base_bindings.clone();
    let mut last_value = None;
    for stmt in &body.root_stmts {
        match &body.stmts[*stmt] {
            hir::Stmt::Expr(expr) => {
                last_value = Some(preview_eval_expr(body, *expr, &bindings, context)?);
            }
            hir::Stmt::Return(Some(expr)) => {
                return preview_eval_expr(body, *expr, &bindings, context);
            }
            hir::Stmt::Let { name, value, .. }
            | hir::Stmt::Assign {
                name,
                op: hir::AssignOp::Assign,
                value,
                ..
            } => {
                let value = preview_eval_expr(body, *value, &bindings, context)?;
                bindings.insert(name.clone(), value);
            }
            hir::Stmt::IgnoreResult { expr } => {
                preview_eval_expr(body, *expr, &bindings, context)?;
            }
            _ => {
                return Err(format!(
                    "{context} only supports literal, arithmetic, constructor, and member-expression bodies"
                ));
            }
        }
    }
    last_value.ok_or_else(|| format!("{context} requires a terminal expression"))
}

pub(crate) fn preview_eval_expr(
    body: &hir::Body,
    expr_id: hir::Idx<hir::Expr>,
    bindings: &PreviewEvalBindings,
    context: &str,
) -> Result<wrela::kernel::KernelValue, String> {
    match &body.exprs[expr_id] {
        hir::Expr::Literal(literal) => preview_literal_value(literal, context),
        hir::Expr::Variable(name) => bindings
            .get(name)
            .cloned()
            .ok_or_else(|| format!("{context} cannot resolve `{name}`")),
        hir::Expr::Unary { op, expr, .. } => {
            let value = preview_eval_expr(body, *expr, bindings, context)?;
            preview_apply_unary(*op, value, context)
        }
        hir::Expr::Binary { lhs, op, rhs, .. } => {
            let lhs = preview_eval_expr(body, *lhs, bindings, context)?;
            let rhs = preview_eval_expr(body, *rhs, bindings, context)?;
            preview_apply_binary(lhs, *op, rhs, context)
        }
        hir::Expr::Call { callee, args, .. } => {
            let hir::Expr::Variable(name) = &body.exprs[*callee] else {
                return Err(format!(
                    "{context} does not support indirect preview-evaluation calls"
                ));
            };
            if name == "capture" {
                let Some(target_expr) = preview_named_or_pos_expr(args, "scene", 0) else {
                    return Err(format!("{context} is missing `scene` for capture"));
                };
                let Some(region_name) = preview_capture_region_name(body, target_expr) else {
                    return Err(format!(
                        "{context} could not resolve the capture scene target"
                    ));
                };
                return Ok(wrela::kernel::KernelValue::Capture(region_name));
            }
            preview_eval_call(name, body, args, bindings, context)
        }
        hir::Expr::Member { object, member, .. } => {
            let object = preview_eval_expr(body, *object, bindings, context)?;
            preview_struct_field(&object, member, context)
        }
        _ => Err(format!(
            "{context} only supports literal, arithmetic, constructor, and member expressions"
        )),
    }
}

pub(crate) fn preview_eval_call(
    callee: &SmolStr,
    body: &hir::Body,
    args: &[hir::Arg],
    bindings: &PreviewEvalBindings,
    context: &str,
) -> Result<wrela::kernel::KernelValue, String> {
    let (positional, mut named) = preview_eval_call_arguments(body, args, bindings, context)?;
    match callee.as_str() {
        "vec3" => Ok(wrela::kernel::KernelValue::Vec3([
            preview_expect_f32(
                &preview_named_or_pos_value(&mut named, &positional, "x", 0, context)?,
                context,
            )?,
            preview_expect_f32(
                &preview_named_or_pos_value(&mut named, &positional, "y", 1, context)?,
                context,
            )?,
            preview_expect_f32(
                &preview_named_or_pos_value(&mut named, &positional, "z", 2, context)?,
                context,
            )?,
        ])),
        "normalize" => {
            let value = preview_named_or_pos_value(&mut named, &positional, "value", 0, context)?;
            Ok(wrela::kernel::KernelValue::Vec3(normalize_preview_vec3(
                preview_expect_vec3(&value, context)?,
            )))
        }
        "Light" => {
            let position = preview_expect_vec3(
                &preview_named_or_pos_value(&mut named, &positional, "position", 0, context)?,
                context,
            )?;
            let direction = preview_expect_vec3(
                &preview_named_or_pos_value(&mut named, &positional, "direction", 1, context)?,
                context,
            )?;
            let intensity = preview_expect_vec3(
                &preview_named_or_pos_value(&mut named, &positional, "intensity", 2, context)?,
                context,
            )?;
            let range = preview_expect_f32(
                &preview_named_or_pos_value(&mut named, &positional, "range", 3, context)?,
                context,
            )?;
            Ok(wrela::presentation_exec::light_value(
                wrela::presentation_contract::CanonicalLightInput {
                    position,
                    direction,
                    intensity,
                    range,
                },
            ))
        }
        "Camera" => {
            let position = preview_expect_vec3(
                &preview_named_or_pos_value(&mut named, &positional, "position", 0, context)?,
                context,
            )?;
            let forward = preview_expect_vec3(
                &preview_named_or_pos_value(&mut named, &positional, "forward", 1, context)?,
                context,
            )?;
            let up = preview_expect_vec3(
                &preview_named_or_pos_value(&mut named, &positional, "up", 2, context)?,
                context,
            )?;
            let vertical_fov_degrees = preview_expect_f32(
                &preview_named_or_pos_value(
                    &mut named,
                    &positional,
                    "vertical_fov_degrees",
                    3,
                    context,
                )?,
                context,
            )?;
            Ok(preview_camera_value(
                wrela::presentation_contract::CanonicalCameraInput {
                    position,
                    forward,
                    up,
                    vertical_fov_degrees,
                },
            ))
        }
        "f32" => Ok(wrela::kernel::KernelValue::F32(preview_expect_f32(
            &preview_named_or_pos_value(&mut named, &positional, "value", 0, context)?,
            context,
        )?)),
        "i32" => Ok(wrela::kernel::KernelValue::I32(preview_expect_i32(
            &preview_named_or_pos_value(&mut named, &positional, "value", 0, context)?,
            context,
        )?)),
        "u32" => Ok(wrela::kernel::KernelValue::U32(preview_expect_u32(
            &preview_named_or_pos_value(&mut named, &positional, "value", 0, context)?,
            context,
        )?)),
        _ => Err(format!(
            "{context} does not support preview evaluation for call `{callee}`"
        )),
    }
}

pub(crate) fn preview_eval_call_arguments(
    body: &hir::Body,
    args: &[hir::Arg],
    bindings: &PreviewEvalBindings,
    context: &str,
) -> Result<(Vec<wrela::kernel::KernelValue>, PreviewEvalBindings), String> {
    let mut positional = Vec::new();
    let mut named = PreviewEvalBindings::new();
    for arg in args {
        match arg {
            hir::Arg::Positional { value, .. } => {
                positional.push(preview_eval_expr(body, *value, bindings, context)?);
            }
            hir::Arg::Named { name, value, .. } => {
                named.insert(
                    name.clone(),
                    preview_eval_expr(body, *value, bindings, context)?,
                );
            }
        }
    }
    Ok((positional, named))
}

pub(crate) fn preview_named_or_pos_expr(
    args: &[hir::Arg],
    name: &str,
    index: usize,
) -> Option<hir::Idx<hir::Expr>> {
    args.iter()
        .find_map(|arg| match arg {
            hir::Arg::Named {
                name: arg_name,
                value,
                ..
            } if arg_name == name => Some(*value),
            _ => None,
        })
        .or_else(|| {
            args.iter()
                .filter_map(|arg| match arg {
                    hir::Arg::Positional { value, .. } => Some(*value),
                    _ => None,
                })
                .nth(index)
        })
}

pub(crate) fn preview_named_or_pos_value(
    named: &mut PreviewEvalBindings,
    positional: &[wrela::kernel::KernelValue],
    name: &str,
    index: usize,
    context: &str,
) -> Result<wrela::kernel::KernelValue, String> {
    named
        .remove(name)
        .or_else(|| positional.get(index).cloned())
        .ok_or_else(|| format!("{context} is missing `{name}`"))
}

pub(crate) fn preview_capture_region_name(
    body: &hir::Body,
    expr_id: hir::Idx<hir::Expr>,
) -> Option<SmolStr> {
    match &body.exprs[expr_id] {
        hir::Expr::Variable(name) => Some(name.clone()),
        hir::Expr::Call { callee, .. } => match &body.exprs[*callee] {
            hir::Expr::Variable(name) => Some(name.clone()),
            _ => None,
        },
        _ => None,
    }
}

pub(crate) fn preview_literal_value(
    literal: &hir::Literal,
    context: &str,
) -> Result<wrela::kernel::KernelValue, String> {
    match literal {
        hir::Literal::Integer(value) => Ok(wrela::kernel::KernelValue::I32(*value as i32)),
        hir::Literal::Float(value) => Ok(wrela::kernel::KernelValue::F32(*value as f32)),
        hir::Literal::Boolean(value) => Ok(wrela::kernel::KernelValue::Bool(*value)),
        _ => Err(format!("{context} does not support that literal kind")),
    }
}

pub(crate) fn preview_apply_unary(
    op: hir::UnaryOp,
    value: wrela::kernel::KernelValue,
    context: &str,
) -> Result<wrela::kernel::KernelValue, String> {
    match (op, value) {
        (hir::UnaryOp::Neg, wrela::kernel::KernelValue::I32(value)) => {
            Ok(wrela::kernel::KernelValue::I32(-value))
        }
        (hir::UnaryOp::Neg, wrela::kernel::KernelValue::F32(value)) => {
            Ok(wrela::kernel::KernelValue::F32(-value))
        }
        (hir::UnaryOp::Neg, wrela::kernel::KernelValue::Vec3(value)) => {
            Ok(wrela::kernel::KernelValue::Vec3([
                -value[0], -value[1], -value[2],
            ]))
        }
        _ => Err(format!("{context} does not support that unary operation")),
    }
}

pub(crate) fn preview_apply_binary(
    lhs: wrela::kernel::KernelValue,
    op: hir::BinaryOp,
    rhs: wrela::kernel::KernelValue,
    context: &str,
) -> Result<wrela::kernel::KernelValue, String> {
    match op {
        hir::BinaryOp::Add => match (&lhs, &rhs) {
            (wrela::kernel::KernelValue::Vec3(lhs), wrela::kernel::KernelValue::Vec3(rhs)) => {
                Ok(wrela::kernel::KernelValue::Vec3([
                    lhs[0] + rhs[0],
                    lhs[1] + rhs[1],
                    lhs[2] + rhs[2],
                ]))
            }
            _ => preview_numeric_binary(lhs, rhs, |lhs, rhs| lhs + rhs, |lhs, rhs| lhs + rhs),
        },
        hir::BinaryOp::Sub => match (&lhs, &rhs) {
            (wrela::kernel::KernelValue::Vec3(lhs), wrela::kernel::KernelValue::Vec3(rhs)) => {
                Ok(wrela::kernel::KernelValue::Vec3([
                    lhs[0] - rhs[0],
                    lhs[1] - rhs[1],
                    lhs[2] - rhs[2],
                ]))
            }
            _ => preview_numeric_binary(lhs, rhs, |lhs, rhs| lhs - rhs, |lhs, rhs| lhs - rhs),
        },
        hir::BinaryOp::Mul => match (&lhs, &rhs) {
            (wrela::kernel::KernelValue::Vec3(value), scalar) => {
                let scalar = preview_expect_f32(scalar, context)?;
                Ok(wrela::kernel::KernelValue::Vec3([
                    value[0] * scalar,
                    value[1] * scalar,
                    value[2] * scalar,
                ]))
            }
            (scalar, wrela::kernel::KernelValue::Vec3(value)) => {
                let scalar = preview_expect_f32(scalar, context)?;
                Ok(wrela::kernel::KernelValue::Vec3([
                    value[0] * scalar,
                    value[1] * scalar,
                    value[2] * scalar,
                ]))
            }
            _ => preview_numeric_binary(lhs, rhs, |lhs, rhs| lhs * rhs, |lhs, rhs| lhs * rhs),
        },
        hir::BinaryOp::Div => match (&lhs, &rhs) {
            (wrela::kernel::KernelValue::Vec3(value), scalar) => {
                let scalar = preview_expect_f32(scalar, context)?;
                Ok(wrela::kernel::KernelValue::Vec3([
                    value[0] / scalar,
                    value[1] / scalar,
                    value[2] / scalar,
                ]))
            }
            _ => preview_numeric_binary(lhs, rhs, |lhs, rhs| lhs / rhs, |lhs, rhs| lhs / rhs),
        },
        _ => Err(format!("{context} does not support that binary operation")),
    }
}

pub(crate) fn preview_numeric_binary(
    lhs: wrela::kernel::KernelValue,
    rhs: wrela::kernel::KernelValue,
    integer_op: impl FnOnce(i32, i32) -> i32,
    float_op: impl FnOnce(f32, f32) -> f32,
) -> Result<wrela::kernel::KernelValue, String> {
    match (&lhs, &rhs) {
        (wrela::kernel::KernelValue::I32(lhs), wrela::kernel::KernelValue::I32(rhs)) => {
            Ok(wrela::kernel::KernelValue::I32(integer_op(*lhs, *rhs)))
        }
        _ => Ok(wrela::kernel::KernelValue::F32(float_op(
            preview_scalar_f32(&lhs)?,
            preview_scalar_f32(&rhs)?,
        ))),
    }
}

pub(crate) fn preview_scalar_f32(value: &wrela::kernel::KernelValue) -> Result<f32, String> {
    match value {
        wrela::kernel::KernelValue::I32(value) => Ok(*value as f32),
        wrela::kernel::KernelValue::U32(value) => Ok(*value as f32),
        wrela::kernel::KernelValue::F32(value) => Ok(*value),
        _ => Err("expected a scalar numeric value".to_string()),
    }
}

pub(crate) fn preview_struct_field(
    value: &wrela::kernel::KernelValue,
    field_name: &str,
    context: &str,
) -> Result<wrela::kernel::KernelValue, String> {
    let wrela::kernel::KernelValue::Struct(record) = value else {
        return Err(format!(
            "{context} expected a struct value for .{field_name}"
        ));
    };
    record
        .fields
        .iter()
        .find(|(name, _)| name == field_name)
        .map(|(_, value)| value.clone())
        .ok_or_else(|| format!("{context} could not find field `{field_name}`"))
}

pub(crate) fn preview_expect_f32(
    value: &wrela::kernel::KernelValue,
    context: &str,
) -> Result<f32, String> {
    preview_scalar_f32(value).map_err(|_| format!("{context} expected an f32-compatible value"))
}

pub(crate) fn preview_expect_i32(
    value: &wrela::kernel::KernelValue,
    context: &str,
) -> Result<i32, String> {
    match value {
        wrela::kernel::KernelValue::I32(value) => Ok(*value),
        wrela::kernel::KernelValue::U32(value) => Ok(*value as i32),
        wrela::kernel::KernelValue::F32(value) => Ok(*value as i32),
        _ => Err(format!("{context} expected an i32-compatible value")),
    }
}

pub(crate) fn preview_expect_u32(
    value: &wrela::kernel::KernelValue,
    context: &str,
) -> Result<u32, String> {
    match value {
        wrela::kernel::KernelValue::I32(value) => Ok((*value).max(0) as u32),
        wrela::kernel::KernelValue::U32(value) => Ok(*value),
        wrela::kernel::KernelValue::F32(value) => Ok(value.max(0.0) as u32),
        _ => Err(format!("{context} expected a u32-compatible value")),
    }
}

pub(crate) fn preview_expect_vec3(
    value: &wrela::kernel::KernelValue,
    context: &str,
) -> Result<[f32; 3], String> {
    match value {
        wrela::kernel::KernelValue::Vec3(value) => Ok(*value),
        _ => Err(format!("{context} expected a vec3 value")),
    }
}

pub(crate) fn preview_expect_light(
    value: &wrela::kernel::KernelValue,
    context: &str,
) -> Result<wrela::presentation_contract::CanonicalLightInput, String> {
    let position =
        preview_expect_vec3(&preview_struct_field(value, "position", context)?, context)?;
    let direction =
        preview_expect_vec3(&preview_struct_field(value, "direction", context)?, context)?;
    let intensity =
        preview_expect_vec3(&preview_struct_field(value, "intensity", context)?, context)?;
    let range = preview_expect_f32(&preview_struct_field(value, "range", context)?, context)?;
    Ok(wrela::presentation_contract::CanonicalLightInput {
        position,
        direction,
        intensity,
        range,
    })
}

pub(crate) fn preview_camera_value(
    camera: wrela::presentation_contract::CanonicalCameraInput,
) -> wrela::kernel::KernelValue {
    wrela::kernel::KernelValue::Struct(wrela::kernel::KernelStructValue {
        name: SmolStr::new("Camera"),
        fields: vec![
            (
                SmolStr::new("position"),
                wrela::kernel::KernelValue::Vec3(camera.position),
            ),
            (
                SmolStr::new("forward"),
                wrela::kernel::KernelValue::Vec3(camera.forward),
            ),
            (
                SmolStr::new("up"),
                wrela::kernel::KernelValue::Vec3(camera.up),
            ),
            (
                SmolStr::new("vertical_fov_degrees"),
                wrela::kernel::KernelValue::F32(camera.vertical_fov_degrees),
            ),
        ],
    })
}

pub(crate) fn default_preview_key_light() -> wrela::presentation_contract::CanonicalLightInput {
    wrela::presentation_contract::CanonicalLightInput {
        position: [2.4, 2.8, 2.4],
        direction: normalize_preview_vec3([-0.8, -0.9, -0.9]),
        intensity: [1.0, 0.98, 0.95],
        range: 12.0,
    }
}

pub(crate) fn normalize_preview_vec3(value: [f32; 3]) -> [f32; 3] {
    let len_sq = value[0] * value[0] + value[1] * value[1] + value[2] * value[2];
    if len_sq <= f32::EPSILON {
        return value;
    }
    let inv_len = len_sq.sqrt().recip();
    [value[0] * inv_len, value[1] * inv_len, value[2] * inv_len]
}
