pub(crate) mod codegen;

use self::codegen::{ShaderPlan, generate_shader};
use crate::execution_policy::QueryExecutionPolicy;
use crate::kernel::KernelBatchQueryTrace;
use crate::kernel::ir::{KernelBatchQueryPlan, KernelCaptureQueryPlan, KernelWorldQueryPlan};
use crate::kernel::{
    KernelStructValue, KernelValidationError, KernelValue, validate_batch_query_plan,
    validate_capture_query_plan, validate_world_query_plan,
};
use crate::portable::{
    PortableAbiType, portable_abi_array_stride, portable_abi_decode_slice,
    portable_abi_encode_slice, portable_abi_encode_value, portable_abi_layout,
};
use crate::query_contract::{
    QueryCardinality, QueryContractDescriptor, QueryItemKind, QueryResultKind, QueryTargetKind,
    SceneDomainFlag, query_contract, scene_domain_flag_name,
};
use crate::query_exec::QueryExecutionObservability;
use crate::query_exec::cpu::{DirectQueryOps, QueryExecError};
use crate::query_exec::world::{NormalRole, world_query_semantics_for_contract};
use crate::query_plan::CaptureKind;
use naga::valid::{Capabilities, ValidationFlags, Validator};
use smol_str::SmolStr;
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock, mpsc};
use wgpu::util::{DeviceExt, initialize_adapter_from_env_or_default};

#[derive(Clone)]
pub(crate) struct NativeWgpuContext {
    pub(crate) device: wgpu::Device,
    pub(crate) queue: wgpu::Queue,
}

#[derive(Debug, Clone)]
pub(crate) struct GpuDispatchRequest {
    pub(crate) dispatch: KernelValue,
    pub(crate) items: Vec<KernelValue>,
    pub(crate) world_shape_indices: Vec<u32>,
}

#[derive(Debug, Clone)]
pub(crate) struct GeneratedShaderModule {
    pub(crate) source: String,
    pub(crate) workgroup_size: u32,
    pub(crate) dispatch_abi: PortableAbiType,
    pub(crate) item_abi: PortableAbiType,
    pub(crate) result_abi: PortableAbiType,
}

#[derive(Debug, Clone)]
pub(crate) struct NativeWgslBridgeConfig {
    pub(crate) source: SmolStr,
    pub(crate) workgroup_size: i64,
}

#[derive(Clone)]
pub(crate) struct CachedPipeline {
    pub(crate) bind_group_layout: wgpu::BindGroupLayout,
    pub(crate) pipeline: wgpu::ComputePipeline,
}

pub(crate) fn execute_capture_query_with_observability(
    ctx: &crate::query_exec::context::QueryExecContext,
    plan: &KernelCaptureQueryPlan,
    args: &[KernelValue],
) -> Result<(KernelValue, QueryExecutionObservability), QueryExecError> {
    execute_capture_query_with_snapshot_observability(ctx, None, plan, args)
}

pub(crate) fn execute_capture_query_with_snapshot_observability(
    ctx: &crate::query_exec::context::QueryExecContext,
    snapshot: Option<&crate::world_identity::WorldSnapshotHandle>,
    plan: &KernelCaptureQueryPlan,
    args: &[KernelValue],
) -> Result<(KernelValue, QueryExecutionObservability), QueryExecError> {
    let ops = DirectQueryOps::new_with_snapshot(ctx, snapshot);
    ops.note_dispatch();
    if let Err(errors) = validate_capture_query_plan(plan) {
        ops.note_contract_validation_failure();
        return Err(validation_error("capture query", errors));
    }
    let request = build_capture_request(&ops, plan, args)?;
    let generated = generate_compiled_shader(ctx, ShaderPlan::Capture(plan))?;
    let mut values = dispatch_compiled_shader(&generated, request)?;
    let descriptor = descriptor_for_plan(plan.contract_id)?;
    note_contract_observability(&ops);
    note_result_observability(&ops, descriptor, &values);
    let value = values.pop().ok_or_else(|| QueryExecError::Unsupported {
        message: "native WGSL backend produced no capture result".to_string(),
    })?;
    Ok((value, ops.snapshot_observability()))
}

pub(crate) fn execute_world_query_with_observability(
    ctx: &crate::query_exec::context::QueryExecContext,
    plan: &KernelWorldQueryPlan,
    args: &[KernelValue],
) -> Result<(KernelValue, QueryExecutionObservability), QueryExecError> {
    let policy = QueryExecutionPolicy::conservative(plan.backend, None);
    execute_world_query_with_policy_with_observability(ctx, &policy, plan, args)
}

pub(crate) fn execute_world_query_with_policy_with_observability(
    ctx: &crate::query_exec::context::QueryExecContext,
    policy: &QueryExecutionPolicy,
    plan: &KernelWorldQueryPlan,
    args: &[KernelValue],
) -> Result<(KernelValue, QueryExecutionObservability), QueryExecError> {
    execute_world_query_with_policy_with_snapshot_observability(ctx, None, policy, plan, args)
}

pub(crate) fn execute_world_query_with_snapshot_observability(
    ctx: &crate::query_exec::context::QueryExecContext,
    snapshot: Option<&crate::world_identity::WorldSnapshotHandle>,
    plan: &KernelWorldQueryPlan,
    args: &[KernelValue],
) -> Result<(KernelValue, QueryExecutionObservability), QueryExecError> {
    let policy = QueryExecutionPolicy::conservative(plan.backend, None);
    execute_world_query_with_policy_with_snapshot_observability(ctx, snapshot, &policy, plan, args)
}

pub(crate) fn execute_world_query_with_policy_with_snapshot_observability(
    ctx: &crate::query_exec::context::QueryExecContext,
    snapshot: Option<&crate::world_identity::WorldSnapshotHandle>,
    _policy: &QueryExecutionPolicy,
    plan: &KernelWorldQueryPlan,
    args: &[KernelValue],
) -> Result<(KernelValue, QueryExecutionObservability), QueryExecError> {
    let ops = DirectQueryOps::new_with_snapshot(ctx, snapshot);
    ops.note_dispatch();
    if let Err(errors) = validate_world_query_plan(plan) {
        ops.note_contract_validation_failure();
        return Err(validation_error("world query", errors));
    }
    let request = build_world_request(&ops, plan, args)?;
    let generated = generate_compiled_shader(ctx, ShaderPlan::World(plan))?;
    let mut values = dispatch_compiled_shader(&generated, request)?;
    let descriptor = descriptor_for_plan(plan.contract_id)?;
    note_contract_observability(&ops);
    note_wgsl_solver_fallback(&ops, plan.ray_solver.as_ref(), values.len() as u32);
    note_result_observability(&ops, descriptor, &values);
    let value = values.pop().ok_or_else(|| QueryExecError::Unsupported {
        message: "native WGSL backend produced no world result".to_string(),
    })?;
    Ok((value, ops.snapshot_observability()))
}

pub(crate) fn execute_batch_query_with_observability(
    ctx: &crate::query_exec::context::QueryExecContext,
    plan: &KernelBatchQueryPlan,
    args: &[KernelValue],
    _trace: &KernelBatchQueryTrace,
) -> Result<(KernelValue, QueryExecutionObservability), QueryExecError> {
    execute_batch_query_with_snapshot_observability(ctx, None, plan, args, _trace)
}

pub(crate) fn execute_batch_query_with_snapshot_observability(
    ctx: &crate::query_exec::context::QueryExecContext,
    snapshot: Option<&crate::world_identity::WorldSnapshotHandle>,
    plan: &KernelBatchQueryPlan,
    args: &[KernelValue],
    _trace: &KernelBatchQueryTrace,
) -> Result<(KernelValue, QueryExecutionObservability), QueryExecError> {
    let ops = DirectQueryOps::new_with_snapshot(ctx, snapshot);
    ops.note_dispatch();
    if let Err(errors) = validate_batch_query_plan(plan) {
        ops.note_contract_validation_failure();
        return Err(validation_error("batch query", errors));
    }
    let generated = generate_compiled_shader(ctx, ShaderPlan::Batch(plan))?;
    let request = build_batch_request(&ops, plan, args)?;
    let workgroups_x = (request.items.len() as u32).div_ceil(generated.workgroup_size);
    ops.note_batch_dispatch_grid(
        request.items.len() as u32,
        workgroups_x.max(1),
        1,
        1,
        descriptor_for_plan(plan.contract_id)?.target == QueryTargetKind::World,
    );
    let values = dispatch_compiled_shader(&generated, request)?;
    let descriptor = descriptor_for_plan(plan.contract_id)?;
    note_contract_observability(&ops);
    note_wgsl_solver_fallback(&ops, plan.ray_solver.as_ref(), values.len() as u32);
    note_result_observability(&ops, descriptor, &values);
    Ok((KernelValue::Array(values), ops.snapshot_observability()))
}

pub(crate) fn compile_world_shader(
    ctx: &crate::query_exec::context::QueryExecContext,
    plan: &KernelWorldQueryPlan,
) -> Result<GeneratedShaderModule, QueryExecError> {
    generate_compiled_shader(ctx, ShaderPlan::World(plan))
}

pub(crate) fn compile_batch_shader(
    ctx: &crate::query_exec::context::QueryExecContext,
    plan: &KernelBatchQueryPlan,
) -> Result<GeneratedShaderModule, QueryExecError> {
    generate_compiled_shader(ctx, ShaderPlan::Batch(plan))
}

pub(crate) fn build_batch_request_for_shader(
    ctx: &crate::query_exec::context::QueryExecContext,
    plan: &KernelBatchQueryPlan,
    args: &[KernelValue],
) -> Result<GpuDispatchRequest, QueryExecError> {
    if let Err(errors) = validate_batch_query_plan(plan) {
        return Err(validation_error("batch query", errors));
    }
    let ops = DirectQueryOps::new(ctx);
    build_batch_request(&ops, plan, args)
}

pub(crate) fn bridge_config(shader: &GeneratedShaderModule) -> NativeWgslBridgeConfig {
    NativeWgslBridgeConfig {
        source: SmolStr::new(shader.source.as_str()),
        workgroup_size: i64::from(shader.workgroup_size),
    }
}

fn build_capture_request(
    ops: &DirectQueryOps<'_>,
    plan: &KernelCaptureQueryPlan,
    args: &[KernelValue],
) -> Result<GpuDispatchRequest, QueryExecError> {
    let descriptor = descriptor_for_plan(plan.contract_id)?;
    if descriptor.result_kind == QueryResultKind::SupportSummaryResult {
        return Err(QueryExecError::Unsupported {
            message: "support.summary is not supported by the native WGSL backend".to_string(),
        });
    }

    let (capture_kind, capture_index) = match descriptor.capture_kind {
        CaptureKind::Field => {
            let capture = ops.resolve_field_or_shape_capture(args.first())?;
            note_wgsl_normal_role_for_capture(ops, descriptor, &capture);
            (0u32, field_index(ops.context(), &capture)?)
        }
        CaptureKind::Shape => {
            let capture = ops.resolve_shape_capture(args.first())?;
            note_wgsl_normal_role_for_capture(ops, descriptor, &capture);
            (1u32, shape_index(ops.context(), &capture)?)
        }
        CaptureKind::Region => {
            return Err(QueryExecError::Unsupported {
                message: "region captures are only valid for world queries".to_string(),
            });
        }
    };

    let item = scalar_item_arg(descriptor, args.get(1))?;

    Ok(GpuDispatchRequest {
        dispatch: dispatch_config(capture_kind, capture_index, 1, 0, true, true, true),
        items: vec![item],
        world_shape_indices: Vec::new(),
    })
}

fn scalar_item_arg(
    descriptor: &QueryContractDescriptor,
    value: Option<&KernelValue>,
) -> Result<KernelValue, QueryExecError> {
    match descriptor.item_kind {
        QueryItemKind::Unit => Err(QueryExecError::Unsupported {
            message: "unit query items are not supported by the native WGSL backend".to_string(),
        }),
        QueryItemKind::PointQuery => Ok(point_query(expect_vec3_arg(value, "point")?)),
        QueryItemKind::RayQuery => {
            expect_struct_arg(value, "RayQuery")?;
            value
                .cloned()
                .ok_or(QueryExecError::MissingCaptureTarget { kind: "ray" })
        }
        QueryItemKind::Hit3 => value
            .cloned()
            .ok_or(QueryExecError::MissingCaptureTarget { kind: "hit" }),
        QueryItemKind::PointDirectionQuery => {
            expect_struct_arg(value, "PointDirectionQuery")?;
            value
                .cloned()
                .ok_or(QueryExecError::MissingCaptureTarget { kind: "sample" })
        }
    }
}

fn descriptor_for_plan(
    contract_id: crate::query_contract::QueryContractId,
) -> Result<&'static QueryContractDescriptor, QueryExecError> {
    query_contract(contract_id).ok_or_else(|| QueryExecError::Unsupported {
        message: format!("missing query contract '{}'", contract_id.as_str()),
    })
}

fn note_contract_observability(ops: &DirectQueryOps<'_>) {
    ops.note_artifact_load();
}

fn note_wgsl_solver_fallback(
    ops: &DirectQueryOps<'_>,
    solver: Option<&crate::query_solver::RaySolverPlan>,
    count: u32,
) {
    let Some(solver) = solver else {
        return;
    };
    for _ in 0..count {
        ops.note_solver_generated_dense_fallback(solver);
    }
}

fn note_wgsl_normal_role_for_capture(
    ops: &DirectQueryOps<'_>,
    descriptor: &QueryContractDescriptor,
    capture: &SmolStr,
) {
    if descriptor.result_kind != QueryResultKind::NormalResult {
        return;
    }
    let role = match descriptor.capture_kind {
        CaptureKind::Field => field_normal_role_for_capture(ops.context(), capture),
        CaptureKind::Shape => shape_normal_role_for_capture(ops.context(), capture),
        CaptureKind::Region => None,
    }
    .unwrap_or(NormalRole::HeuristicShadingNormal);
    ops.note_normal_role(role);
}

fn note_wgsl_normal_role_for_world(
    ops: &DirectQueryOps<'_>,
    descriptor: &QueryContractDescriptor,
    world_shapes: &[SmolStr],
) {
    if descriptor.result_kind != QueryResultKind::NormalResult {
        return;
    }
    let role = if world_shapes.len() == 1 {
        shape_normal_role_for_capture(ops.context(), &world_shapes[0])
    } else {
        None
    }
    .unwrap_or(NormalRole::HeuristicShadingNormal);
    ops.note_normal_role(role);
}

fn field_normal_role_for_capture(
    ctx: &crate::query_exec::context::QueryExecContext,
    capture: &SmolStr,
) -> Option<NormalRole> {
    let scene = ctx.scene.fields.get(capture)?;
    if scene.opaque_boundary
        || !matches!(
            scene.analysis.differential_support,
            crate::scene_ir::SceneDifferentialSupport::CertifiedGradient
        )
    {
        return None;
    }
    field_normal_role_for_node(ctx, &scene.root)
}

fn field_normal_role_for_node(
    ctx: &crate::query_exec::context::QueryExecContext,
    node: &crate::scene_ir::FieldNode,
) -> Option<NormalRole> {
    match node {
        crate::scene_ir::FieldNode::Use { target } => field_normal_role_for_capture(ctx, target),
        crate::scene_ir::FieldNode::Primitive { primitive, .. } => match primitive {
            crate::hir::FieldPrimitive::Sphere | crate::hir::FieldPrimitive::Plane => {
                Some(NormalRole::CertifiedFieldGradient)
            }
            _ => None,
        },
        crate::scene_ir::FieldNode::Transform { kind, inner, .. } => match kind {
            crate::scene_ir::TransformKind::Translate
            | crate::scene_ir::TransformKind::Rotate
            | crate::scene_ir::TransformKind::UniformScale => {
                field_normal_role_for_node(ctx, inner)
            }
            _ => None,
        },
        crate::scene_ir::FieldNode::Smooth {
            smoothing, items, ..
        } => {
            let smoothing = smoothing
                .as_ref()
                .and_then(|expr| eval_scene_constant_f32(ctx, expr))
                .unwrap_or(0.0);
            if smoothing <= 0.0 {
                return None;
            }
            let first = items.first()?;
            field_normal_role_for_node(ctx, first)?;
            for item in items.iter().skip(1) {
                field_normal_role_for_node(ctx, item)?;
            }
            Some(NormalRole::CertifiedFieldGradient)
        }
        crate::scene_ir::FieldNode::Repeat { .. }
        | crate::scene_ir::FieldNode::Union { .. }
        | crate::scene_ir::FieldNode::Intersection { .. }
        | crate::scene_ir::FieldNode::Subtract { .. }
        | crate::scene_ir::FieldNode::Extrude { .. }
        | crate::scene_ir::FieldNode::Revolve { .. }
        | crate::scene_ir::FieldNode::Sweep { .. }
        | crate::scene_ir::FieldNode::Loft { .. }
        | crate::scene_ir::FieldNode::OpaqueLeaf => None,
    }
}

fn shape_normal_role_for_capture(
    ctx: &crate::query_exec::context::QueryExecContext,
    capture: &SmolStr,
) -> Option<NormalRole> {
    let scene = ctx.scene.shapes.get(capture)?;
    if scene.opaque_boundary
        || !matches!(
            scene.analysis.differential_support,
            crate::scene_ir::SceneDifferentialSupport::CertifiedGradient
        )
    {
        return None;
    }
    match &scene.root {
        crate::scene_ir::ShapeNode::Use { target } => shape_normal_role_for_capture(ctx, target),
        crate::scene_ir::ShapeNode::Leaf(leaf) => {
            field_normal_role_for_capture(ctx, &leaf.field)?;
            Some(NormalRole::FeatureNormal)
        }
        crate::scene_ir::ShapeNode::Union { .. }
        | crate::scene_ir::ShapeNode::Intersection { .. }
        | crate::scene_ir::ShapeNode::Subtract { .. } => None,
    }
}

fn eval_scene_constant_f32(
    ctx: &crate::query_exec::context::QueryExecContext,
    expr: &crate::scene_ir::SceneValueExpr,
) -> Option<f32> {
    let ops = DirectQueryOps::new(ctx);
    match ops.eval_scene_constant(expr).ok()? {
        KernelValue::F32(value) => Some(value),
        KernelValue::I32(value) => Some(value as f32),
        KernelValue::U32(value) => Some(value as f32),
        _ => None,
    }
}

fn note_result_observability(
    ops: &DirectQueryOps<'_>,
    descriptor: &QueryContractDescriptor,
    values: &[KernelValue],
) {
    if !descriptor.observability.trace_steps {
        return;
    }
    for value in values {
        match descriptor.result_kind {
            QueryResultKind::Hit3 => {
                if let Ok(hit) = expect_struct_arg(Some(value), "Hit3") {
                    let hit_value = expect_struct_bool(hit, "hit").unwrap_or(false);
                    let steps = expect_struct_i32(hit, "steps").unwrap_or_default().max(0) as u32;
                    // WGSL currently reports trace work from result records rather than a
                    // sideband metric buffer. Preserve the encoded value exactly; miss
                    // records that carry zero steps must stay zero instead of inventing
                    // work the backend did not report.
                    ops.note_trace_steps(steps);
                    ops.note_hit_result(hit_value, steps);
                }
            }
            QueryResultKind::OcclusionResult => {
                if let Ok(occlusion) = expect_struct_arg(Some(value), "OcclusionResult") {
                    let hit_value = expect_struct_bool(occlusion, "occluded").unwrap_or(false);
                    let steps = expect_struct_i32(occlusion, "steps")
                        .unwrap_or_default()
                        .max(0) as u32;
                    ops.note_trace_steps(steps);
                    ops.note_hit_result(hit_value, steps);
                }
            }
            _ => {
                if matches!(
                    descriptor.item_kind,
                    QueryItemKind::RayQuery | QueryItemKind::Hit3
                ) {
                    ops.note_trace_step();
                }
            }
        }
    }
}

fn scene_domain_flag_enabled(
    domain: &KernelStructValue,
    flag: SceneDomainFlag,
) -> Result<bool, QueryExecError> {
    let flag_name = scene_domain_flag_name(flag);
    let (contract_field, contract_name) = match flag {
        SceneDomainFlag::Material => ("surface", "SurfaceDomainContract"),
        SceneDomainFlag::Radiance | SceneDomainFlag::Media => {
            ("participants", "ParticipantDomainContract")
        }
    };
    let contract = expect_struct_arg(struct_field(domain, contract_field), contract_name)?;
    expect_struct_bool(contract, flag_name)
}

fn batch_array_label(descriptor: &QueryContractDescriptor) -> &'static str {
    match descriptor.item_kind {
        QueryItemKind::PointQuery => "points",
        QueryItemKind::RayQuery => "rays",
        QueryItemKind::Hit3 => "hits",
        QueryItemKind::PointDirectionQuery => "samples",
        QueryItemKind::Unit => "items",
    }
}

fn build_world_request(
    ops: &DirectQueryOps<'_>,
    plan: &KernelWorldQueryPlan,
    args: &[KernelValue],
) -> Result<GpuDispatchRequest, QueryExecError> {
    let descriptor = descriptor_for_plan(plan.contract_id)?;
    if descriptor.result_kind == QueryResultKind::SupportSummaryResult {
        return Err(QueryExecError::Unsupported {
            message: "support.summary is not supported by the native WGSL backend".to_string(),
        });
    }
    let capture = ops.resolve_region_capture(args.first())?;
    let domain = expect_struct_arg(args.get(1), "SceneDomain")?;
    let detail = ops.validate_world_domain(
        &capture,
        domain,
        world_query_semantics_for_contract(plan.contract_id).query_name,
    )?;
    let surface_root_shape_id = if descriptor.item_kind == QueryItemKind::Hit3 {
        let hit = expect_struct_arg(args.get(2), "Hit3")?;
        Some(expect_struct_u32(hit, "root_shape_id")?)
    } else {
        None
    };
    let world_shapes = ops.resolve_world_shapes(&capture, detail, surface_root_shape_id)?;
    ops.note_candidate_count(world_shapes.len() as u32);
    let world_shape_indices = world_shapes
        .iter()
        .map(|shape| shape_index(ops.context(), shape))
        .collect::<Result<Vec<_>, _>>()?;
    note_wgsl_normal_role_for_world(ops, descriptor, &world_shapes);
    let item = scalar_item_arg(descriptor, args.get(2))?;

    Ok(GpuDispatchRequest {
        dispatch: dispatch_config(
            2,
            0,
            1,
            world_shape_indices.len() as u32,
            scene_domain_flag_enabled(domain, SceneDomainFlag::Material)?,
            scene_domain_flag_enabled(domain, SceneDomainFlag::Radiance)?,
            scene_domain_flag_enabled(domain, SceneDomainFlag::Media)?,
        ),
        items: vec![item],
        world_shape_indices,
    })
}

fn build_batch_request(
    ops: &DirectQueryOps<'_>,
    plan: &KernelBatchQueryPlan,
    args: &[KernelValue],
) -> Result<GpuDispatchRequest, QueryExecError> {
    let descriptor = descriptor_for_plan(plan.contract_id)?;
    if descriptor.cardinality != QueryCardinality::Batch {
        return Err(QueryExecError::Unsupported {
            message: format!(
                "WGSL batch execution requires a batch contract, got '{}'",
                descriptor.id.as_str()
            ),
        });
    }
    if descriptor.result_kind == QueryResultKind::SupportSummaryResult {
        return Err(QueryExecError::Unsupported {
            message: "support.summary is not supported by the native WGSL backend".to_string(),
        });
    }
    if descriptor.target == QueryTargetKind::World {
        return build_world_batch_request(ops, plan, descriptor, args);
    }
    let capture = match descriptor.capture_kind {
        CaptureKind::Field => {
            let capture = ops.resolve_field_or_shape_capture(args.first())?;
            note_wgsl_normal_role_for_capture(ops, descriptor, &capture);
            capture
        }
        CaptureKind::Shape => {
            let capture = ops.resolve_shape_capture(args.first())?;
            note_wgsl_normal_role_for_capture(ops, descriptor, &capture);
            capture
        }
        CaptureKind::Region => {
            return Err(QueryExecError::Unsupported {
                message: "region captures are only valid for world queries".to_string(),
            });
        }
    };
    let items = expect_array_arg(args.get(1), batch_array_label(descriptor))?;
    ops.note_candidate_count(items.len() as u32);
    ops.note_batch_execution_mode(!matches!(
        plan.pruning_strategy,
        crate::query_plan::PruningStrategy::None
            | crate::query_plan::PruningStrategy::ConservativeTraversal
    ));
    Ok(GpuDispatchRequest {
        dispatch: dispatch_config(
            match descriptor.capture_kind {
                CaptureKind::Field => 0,
                CaptureKind::Shape => 1,
                CaptureKind::Region => 2,
            },
            match descriptor.capture_kind {
                CaptureKind::Field => field_index(ops.context(), &capture)?,
                CaptureKind::Shape => shape_index(ops.context(), &capture)?,
                CaptureKind::Region => 0,
            },
            items.len() as u32,
            0,
            true,
            true,
            true,
        ),
        items: items.to_vec(),
        world_shape_indices: Vec::new(),
    })
}

fn build_world_batch_request(
    ops: &DirectQueryOps<'_>,
    plan: &KernelBatchQueryPlan,
    descriptor: &QueryContractDescriptor,
    args: &[KernelValue],
) -> Result<GpuDispatchRequest, QueryExecError> {
    let capture = ops.resolve_region_capture(args.first())?;
    let domain = expect_struct_arg(args.get(1), "SceneDomain")?;
    let detail = ops.validate_world_domain(
        &capture,
        domain,
        world_query_semantics_for_contract(plan.contract_id).query_name,
    )?;
    let items = expect_array_arg(args.get(2), batch_array_label(descriptor))?;
    let world_shapes = ops.resolve_world_shapes(&capture, detail, None)?;
    ops.note_candidate_count((world_shapes.len() * items.len()) as u32);
    ops.note_batch_execution_mode(!matches!(
        plan.pruning_strategy,
        crate::query_plan::PruningStrategy::None
            | crate::query_plan::PruningStrategy::ConservativeTraversal
    ));
    let world_shape_indices = world_shapes
        .iter()
        .map(|shape| shape_index(ops.context(), shape))
        .collect::<Result<Vec<_>, _>>()?;
    note_wgsl_normal_role_for_world(ops, descriptor, &world_shapes);

    Ok(GpuDispatchRequest {
        dispatch: dispatch_config(
            2,
            0,
            items.len() as u32,
            world_shape_indices.len() as u32,
            scene_domain_flag_enabled(domain, SceneDomainFlag::Material)?,
            scene_domain_flag_enabled(domain, SceneDomainFlag::Radiance)?,
            scene_domain_flag_enabled(domain, SceneDomainFlag::Media)?,
        ),
        items: items.to_vec(),
        world_shape_indices,
    })
}

fn generate_compiled_shader(
    ctx: &crate::query_exec::context::QueryExecContext,
    plan: ShaderPlan<'_>,
) -> Result<GeneratedShaderModule, QueryExecError> {
    let generated = generate_shader(ctx, plan)?;
    validate_generated_shader(&generated.source)?;
    Ok(GeneratedShaderModule {
        source: generated.source,
        workgroup_size: generated.workgroup_size,
        dispatch_abi: generated.dispatch_abi,
        item_abi: generated.item_abi,
        result_abi: generated.result_abi,
    })
}

pub(crate) fn dispatch_compiled_shader(
    generated: &GeneratedShaderModule,
    request: GpuDispatchRequest,
) -> Result<Vec<KernelValue>, QueryExecError> {
    if request.items.is_empty() {
        return Ok(Vec::new());
    }

    let native = native_wgpu_context()?;
    let input_bytes = encode_slice(&generated.item_abi, &request.items)?;
    let result_stride = portable_abi_array_stride(&generated.result_abi) as usize;
    let result_buffer_size = (result_stride * request.items.len()).max(result_stride.max(4));
    let input_buffer = native
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("wrela.wgsl.input"),
            contents: &input_bytes,
            usage: wgpu::BufferUsages::STORAGE,
        });
    let output_buffer = native.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("wrela.wgsl.output"),
        size: result_buffer_size as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });

    dispatch_compiled_shader_with_buffers(generated, &request, &input_buffer, &output_buffer)?;
    let bytes = readback_storage_buffer(&output_buffer, result_buffer_size as u64)?;

    decode_slice(&generated.result_abi, &bytes, request.items.len())
}

pub(crate) fn dispatch_compiled_shader_with_buffers(
    generated: &GeneratedShaderModule,
    request: &GpuDispatchRequest,
    input_buffer: &wgpu::Buffer,
    output_buffer: &wgpu::Buffer,
) -> Result<(), QueryExecError> {
    if request.items.is_empty() {
        return Ok(());
    }
    let native = native_wgpu_context()?;
    let dispatch_bytes = encode_value(&generated.dispatch_abi, &request.dispatch)?;
    let shape_bytes = encode_shape_indices(&request.world_shape_indices)?;
    let dispatch_buffer = native
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("wrela.wgsl.dispatch"),
            contents: &dispatch_bytes,
            usage: wgpu::BufferUsages::STORAGE,
        });
    let world_shapes_buffer = native
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("wrela.wgsl.world_shapes"),
            contents: &shape_bytes,
            usage: wgpu::BufferUsages::STORAGE,
        });
    let dispatch_min_size =
        wgpu::BufferSize::new(portable_abi_layout(&generated.dispatch_abi).size as u64);
    let cached = compiled_pipeline(
        native,
        &generated.source,
        generated.workgroup_size,
        dispatch_min_size,
    )?;
    let bind_group = native.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("wrela.wgsl.bind_group"),
        layout: &cached.bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: dispatch_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: input_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: output_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: world_shapes_buffer.as_entire_binding(),
            },
        ],
    });

    let mut encoder = native
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("wrela.wgsl.encoder"),
        });
    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("wrela.wgsl.compute_pass"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&cached.pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups(
            (request.items.len() as u32).div_ceil(generated.workgroup_size),
            1,
            1,
        );
    }
    native.queue.submit(Some(encoder.finish()));
    Ok(())
}

pub(crate) fn readback_storage_buffer(
    buffer: &wgpu::Buffer,
    size: u64,
) -> Result<Vec<u8>, QueryExecError> {
    let native = native_wgpu_context()?;
    let readback_buffer = native.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("wrela.wgsl.readback"),
        size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = native
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("wrela.wgsl.readback_encoder"),
        });
    encoder.copy_buffer_to_buffer(buffer, 0, &readback_buffer, 0, size);
    native.queue.submit(Some(encoder.finish()));

    let slice = readback_buffer.slice(..size);
    let (tx, rx) = mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = tx.send(result);
    });
    native
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(wgpu_poll_error)?;
    match rx.recv() {
        Ok(Ok(())) => {}
        Ok(Err(err)) => {
            return Err(QueryExecError::Unsupported {
                message: format!("native WGSL readback failed: {err}"),
            });
        }
        Err(err) => {
            return Err(QueryExecError::Unsupported {
                message: format!("native WGSL readback channel failed: {err}"),
            });
        }
    }
    let bytes = slice.get_mapped_range().to_vec();
    let _ = slice;
    readback_buffer.unmap();
    Ok(bytes)
}

pub(crate) fn compiled_pipeline(
    native: &NativeWgpuContext,
    source: &str,
    workgroup_size: u32,
    dispatch_min_size: Option<wgpu::BufferSize>,
) -> Result<CachedPipeline, QueryExecError> {
    static PIPELINES: OnceLock<Mutex<HashMap<(String, u32, u64), CachedPipeline>>> =
        OnceLock::new();
    let cache = PIPELINES.get_or_init(|| Mutex::new(HashMap::new()));
    let key = (
        source.to_string(),
        workgroup_size,
        dispatch_min_size.map(wgpu::BufferSize::get).unwrap_or(0),
    );

    {
        let guard = cache.lock().unwrap_or_else(|poison| poison.into_inner());
        if let Some(cached) = guard.get(&key) {
            return Ok(cached.clone());
        }
    }

    let bind_group_layout =
        native
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("wrela.wgsl.bind_group_layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: dispatch_min_size,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: false },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });
    let pipeline_layout = native
        .device
        .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("wrela.wgsl.pipeline_layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
    let shader_module = native
        .device
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("wrela.wgsl.shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(source)),
        });
    let error_scope = native
        .device
        .push_error_scope(wgpu::ErrorFilter::Validation);
    let pipeline = native
        .device
        .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("wrela.wgsl.pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader_module,
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions {
                constants: &[("WG_SIZE", workgroup_size as f64)],
                zero_initialize_workgroup_memory: true,
            },
            cache: None,
        });
    native
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(wgpu_poll_error)?;
    if let Some(err) = pollster::block_on(error_scope.pop()) {
        return Err(QueryExecError::Unsupported {
            message: format!("native WGSL validation failed: {err}"),
        });
    }

    let cached = CachedPipeline {
        bind_group_layout,
        pipeline,
    };
    let mut guard = cache.lock().unwrap_or_else(|poison| poison.into_inner());
    Ok(guard.entry(key).or_insert_with(|| cached.clone()).clone())
}

fn validate_generated_shader(source: &str) -> Result<(), QueryExecError> {
    let module =
        naga::front::wgsl::parse_str(source).map_err(|err| QueryExecError::Unsupported {
            message: format!("native WGSL parse failed: {err}"),
        })?;
    Validator::new(ValidationFlags::all(), Capabilities::all())
        .validate(&module)
        .map_err(|err| QueryExecError::Unsupported {
            message: format!("native WGSL validation failed: {err}"),
        })?;
    Ok(())
}

pub(crate) fn native_wgpu_context() -> Result<&'static NativeWgpuContext, QueryExecError> {
    static CONTEXT: OnceLock<Result<NativeWgpuContext, String>> = OnceLock::new();
    match CONTEXT.get_or_init(init_native_wgpu_context) {
        Ok(context) => Ok(context),
        Err(message) => Err(QueryExecError::Unsupported {
            message: format!("native WGSL backend initialization failed: {message}"),
        }),
    }
}

fn init_native_wgpu_context() -> Result<NativeWgpuContext, String> {
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let adapter = pollster::block_on(initialize_adapter_from_env_or_default(&instance, None))
        .map_err(|err| format!("request adapter failed: {err}"))?;
    let descriptor = wgpu::DeviceDescriptor {
        label: Some("wrela.wgsl.device"),
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits::default(),
        experimental_features: wgpu::ExperimentalFeatures::disabled(),
        memory_hints: wgpu::MemoryHints::Performance,
        trace: wgpu::Trace::Off,
    };
    let (device, queue) = pollster::block_on(adapter.request_device(&descriptor))
        .map_err(|err| format!("request device failed: {err}"))?;
    Ok(NativeWgpuContext { device, queue })
}

fn validation_error(label: &str, errors: Vec<KernelValidationError>) -> QueryExecError {
    let messages = errors
        .into_iter()
        .map(|error| error.message)
        .collect::<Vec<_>>()
        .join("; ");
    QueryExecError::Unsupported {
        message: format!("native WGSL contract validation failed for {label}: {messages}"),
    }
}

fn field_index(
    ctx: &crate::query_exec::context::QueryExecContext,
    name: &SmolStr,
) -> Result<u32, QueryExecError> {
    ctx.scene
        .fields
        .keys()
        .enumerate()
        .find_map(|(index, candidate)| (candidate == name).then_some(index as u32))
        .ok_or_else(|| QueryExecError::MissingField { name: name.clone() })
}

fn shape_index(
    ctx: &crate::query_exec::context::QueryExecContext,
    name: &SmolStr,
) -> Result<u32, QueryExecError> {
    ctx.scene
        .shapes
        .keys()
        .enumerate()
        .find_map(|(index, candidate)| (candidate == name).then_some(index as u32))
        .ok_or_else(|| QueryExecError::MissingShape { name: name.clone() })
}

pub(crate) fn dispatch_config(
    capture_kind: u32,
    capture_index: u32,
    item_count: u32,
    shape_count: u32,
    material_enabled: bool,
    radiance_enabled: bool,
    media_enabled: bool,
) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("WgslDispatchConfig"),
        fields: vec![
            (SmolStr::new("capture_kind"), KernelValue::U32(capture_kind)),
            (
                SmolStr::new("capture_index"),
                KernelValue::U32(capture_index),
            ),
            (SmolStr::new("item_count"), KernelValue::U32(item_count)),
            (SmolStr::new("shape_count"), KernelValue::U32(shape_count)),
            (
                SmolStr::new("material_enabled"),
                KernelValue::Bool(material_enabled),
            ),
            (
                SmolStr::new("radiance_enabled"),
                KernelValue::Bool(radiance_enabled),
            ),
            (
                SmolStr::new("media_enabled"),
                KernelValue::Bool(media_enabled),
            ),
        ],
    })
}

fn point_query(point: [f32; 3]) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("PointQuery"),
        fields: vec![(SmolStr::new("point"), KernelValue::Vec3(point))],
    })
}

pub(crate) fn encode_shape_indices(indices: &[u32]) -> Result<Vec<u8>, QueryExecError> {
    let values = if indices.is_empty() {
        vec![KernelValue::U32(0)]
    } else {
        indices.iter().copied().map(KernelValue::U32).collect()
    };
    encode_slice(&PortableAbiType::U32, &values)
}

pub(crate) fn encode_value(
    abi: &PortableAbiType,
    value: &KernelValue,
) -> Result<Vec<u8>, QueryExecError> {
    portable_abi_encode_value(abi, value).map_err(portable_abi_error)
}

pub(crate) fn encode_slice(
    abi: &PortableAbiType,
    values: &[KernelValue],
) -> Result<Vec<u8>, QueryExecError> {
    portable_abi_encode_slice(abi, values).map_err(portable_abi_error)
}

pub(crate) fn decode_slice(
    abi: &PortableAbiType,
    bytes: &[u8],
    len: usize,
) -> Result<Vec<KernelValue>, QueryExecError> {
    portable_abi_decode_slice(abi, bytes, len).map_err(portable_abi_error)
}

fn portable_abi_error(err: crate::portable::PortableAbiError) -> QueryExecError {
    QueryExecError::Unsupported {
        message: format!("native WGSL ABI conversion failed: {err}"),
    }
}

pub(crate) fn wgpu_poll_error(err: wgpu::PollError) -> QueryExecError {
    QueryExecError::Unsupported {
        message: format!("native WGSL device poll failed: {err}"),
    }
}

fn expect_array_arg<'a>(
    value: Option<&'a KernelValue>,
    name: &'static str,
) -> Result<&'a [KernelValue], QueryExecError> {
    match value {
        Some(KernelValue::Array(values)) => Ok(values),
        Some(other) => Err(QueryExecError::TypeMismatch {
            expected: format!("Array for {name}"),
            found: format!("{other:?}"),
        }),
        None => Err(QueryExecError::MissingCaptureTarget { kind: name }),
    }
}

fn expect_struct_arg<'a>(
    value: Option<&'a KernelValue>,
    name: &'static str,
) -> Result<&'a KernelStructValue, QueryExecError> {
    match value {
        Some(KernelValue::Struct(value)) if value.name.as_str() == name => Ok(value),
        Some(other) => Err(QueryExecError::TypeMismatch {
            expected: name.to_string(),
            found: format!("{other:?}"),
        }),
        None => Err(QueryExecError::MissingCaptureTarget { kind: name }),
    }
}

fn struct_field<'a>(value: &'a KernelStructValue, field: &str) -> Option<&'a KernelValue> {
    value
        .fields
        .iter()
        .find_map(|(name, value)| (name.as_str() == field).then_some(value))
}

fn expect_struct_bool(value: &KernelStructValue, field: &str) -> Result<bool, QueryExecError> {
    let Some(value) = struct_field(value, field) else {
        return Err(QueryExecError::MissingCaptureTarget {
            kind: "struct field",
        });
    };
    match value {
        KernelValue::Bool(value) => Ok(*value),
        other => Err(QueryExecError::TypeMismatch {
            expected: format!("Bool for field {field}"),
            found: format!("{other:?}"),
        }),
    }
}

fn expect_struct_u32(value: &KernelStructValue, field: &str) -> Result<u32, QueryExecError> {
    let Some(value) = struct_field(value, field) else {
        return Err(QueryExecError::MissingCaptureTarget {
            kind: "struct field",
        });
    };
    match value {
        KernelValue::U32(value) => Ok(*value),
        KernelValue::I32(value) if *value >= 0 => Ok(*value as u32),
        other => Err(QueryExecError::TypeMismatch {
            expected: format!("U32 for field {field}"),
            found: format!("{other:?}"),
        }),
    }
}

fn expect_struct_i32(value: &KernelStructValue, field: &str) -> Result<i32, QueryExecError> {
    let Some(value) = struct_field(value, field) else {
        return Err(QueryExecError::MissingCaptureTarget {
            kind: "struct field",
        });
    };
    match value {
        KernelValue::I32(value) => Ok(*value),
        KernelValue::U32(value) if *value <= i32::MAX as u32 => Ok(*value as i32),
        other => Err(QueryExecError::TypeMismatch {
            expected: format!("I32 for field {field}"),
            found: format!("{other:?}"),
        }),
    }
}

fn expect_vec3_arg(
    value: Option<&KernelValue>,
    name: &'static str,
) -> Result<[f32; 3], QueryExecError> {
    match value {
        Some(KernelValue::Vec3(value)) => Ok(*value),
        Some(other) => Err(QueryExecError::TypeMismatch {
            expected: format!("Vec3 for {name}"),
            found: format!("{other:?}"),
        }),
        None => Err(QueryExecError::MissingCaptureTarget { kind: name }),
    }
}
