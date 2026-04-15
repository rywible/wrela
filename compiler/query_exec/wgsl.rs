pub(crate) mod codegen;

use self::codegen::{ShaderPlan, generate_shader};
use crate::acceleration::cache::SupportBrickCache;
use crate::acceleration::{AccelerationForest, BoundDescriptorKind};
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
use crate::query_exec::cpu::{DirectQueryOps, QueryExecError};
use crate::query_exec::ids::stable_semantic_id;
use crate::query_exec::world::{NormalRole, world_query_semantics_for_contract};
use crate::query_exec::{QueryExecutionObservability, select_query_wgsl_workgroup_size};
use crate::query_plan::CaptureKind;
use naga::valid::{Capabilities, ValidationFlags, Validator};
use smol_str::SmolStr;
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, mpsc};
use wgpu::util::{DeviceExt, initialize_adapter_from_env_or_default};

const QUERY_WGSL_BIND_GROUP_COUNT: u32 = 4;
const QUERY_WGSL_STORAGE_BUFFERS_PER_SHADER_STAGE: u32 = 10;
const QUERY_WGSL_ACCEL_FLAG_LEAF: u32 = 1;
const QUERY_WGSL_ACCEL_FLAG_HAS_BOUNDS: u32 = 2;
const QUERY_WGSL_OBSERVABILITY_U32S: usize = 18;

#[derive(Clone)]
pub(crate) struct NativeWgpuContext {
    pub(crate) device: wgpu::Device,
    pub(crate) queue: wgpu::Queue,
    pub(crate) adapter_limits: wgpu::Limits,
    pub(crate) requested_limits: wgpu::Limits,
}

#[derive(Debug, Clone)]
pub(crate) struct GpuDispatchRequest {
    pub(crate) dispatch: KernelValue,
    pub(crate) items: Vec<KernelValue>,
    pub(crate) world_shape_indices: Vec<u32>,
    pub(crate) accel_nodes: Vec<KernelValue>,
    pub(crate) accel_children: Vec<u32>,
    pub(crate) cache_bricks: Vec<KernelValue>,
    pub(crate) continuation_seeds: Vec<u32>,
}

#[derive(Debug, Clone)]
pub(crate) struct GeneratedShaderModule {
    pub(crate) source: String,
    pub(crate) workgroup_size: u32,
    pub(crate) dispatch_abi: PortableAbiType,
    pub(crate) accel_node_abi: PortableAbiType,
    pub(crate) cache_brick_abi: PortableAbiType,
    pub(crate) shape_meta_abi: PortableAbiType,
    pub(crate) item_abi: PortableAbiType,
    pub(crate) result_abi: PortableAbiType,
    pub(crate) shape_meta_values: Vec<KernelValue>,
    pub(crate) layout_signature: u64,
    pub(crate) bind_group_count: u32,
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

#[derive(Clone)]
pub(crate) struct QueryCachedPipeline {
    pub(crate) bind_group_layouts: [wgpu::BindGroupLayout; QUERY_WGSL_BIND_GROUP_COUNT as usize],
    pub(crate) pipeline: wgpu::ComputePipeline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct WgslLimitRequest {
    max_storage_buffers_per_shader_stage: u32,
    max_storage_buffer_binding_size: u64,
}

impl Default for WgslLimitRequest {
    fn default() -> Self {
        Self {
            max_storage_buffers_per_shader_stage: wgpu::Limits::downlevel_defaults()
                .max_storage_buffers_per_shader_stage,
            max_storage_buffer_binding_size: wgpu::Limits::downlevel_defaults()
                .max_storage_buffer_binding_size,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct WgslDispatchDiagnostics {
    selected_workgroup_size: u32,
    used_max_storage_buffer_bytes: u64,
    requested_max_storage_buffer_bytes: u64,
}

#[derive(Debug, Clone, Copy)]
struct WgslAccelNodeRecord {
    min: [f32; 3],
    max: [f32; 3],
    child_start: u32,
    child_len: u32,
    leaf_shape_index: u32,
    flags: u32,
}

#[derive(Debug, Clone)]
struct WgslAccelerationForestData {
    root_index: u32,
    nodes: Vec<WgslAccelNodeRecord>,
    children: Vec<u32>,
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
    let (mut values, wgsl_observability) =
        dispatch_compiled_shader_with_observability(&generated, request)?;
    let descriptor = descriptor_for_plan(plan.contract_id)?;
    note_contract_observability(&ops);
    note_result_observability(&ops, descriptor, &values);
    let value = values.pop().ok_or_else(|| QueryExecError::Unsupported {
        message: "native WGSL backend produced no capture result".to_string(),
    })?;
    let mut observability = ops.snapshot_observability();
    observability.merge_from(&wgsl_observability);
    Ok((value, observability))
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
    if let Some(ray_solver) = &plan.ray_solver {
        ops.note_solver_plan(ray_solver);
    }
    if let Err(errors) = validate_world_query_plan(plan) {
        ops.note_contract_validation_failure();
        return Err(validation_error("world query", errors));
    }
    let request = build_world_request(&ops, plan, args)?;
    let generated = generate_compiled_shader(ctx, ShaderPlan::World(plan))?;
    let (mut values, wgsl_observability) =
        dispatch_compiled_shader_with_observability(&generated, request)?;
    let descriptor = descriptor_for_plan(plan.contract_id)?;
    note_contract_observability(&ops);
    note_result_observability(&ops, descriptor, &values);
    let value = values.pop().ok_or_else(|| QueryExecError::Unsupported {
        message: "native WGSL backend produced no world result".to_string(),
    })?;
    let mut observability = ops.snapshot_observability();
    observability.merge_from(&wgsl_observability);
    Ok((value, observability))
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
    if let Some(ray_solver) = &plan.ray_solver {
        ops.note_solver_plan(ray_solver);
    }
    if let Err(errors) = validate_batch_query_plan(plan) {
        ops.note_contract_validation_failure();
        return Err(validation_error("batch query", errors));
    }
    let generated = generate_compiled_shader(ctx, ShaderPlan::Batch(plan))?;
    let request = build_batch_request(&ops, plan, args)?;
    let workgroups_x = dispatch_workgroups_x_for_items(
        request.items.len() as u32,
        current_selected_query_workgroup_size()?,
    );
    ops.note_batch_dispatch_grid(
        request.items.len() as u32,
        workgroups_x.max(1),
        1,
        1,
        descriptor_for_plan(plan.contract_id)?.target == QueryTargetKind::World,
    );
    let (values, wgsl_observability) =
        dispatch_compiled_shader_with_observability(&generated, request)?;
    let descriptor = descriptor_for_plan(plan.contract_id)?;
    note_contract_observability(&ops);
    note_result_observability(&ops, descriptor, &values);
    let mut observability = ops.snapshot_observability();
    observability.merge_from(&wgsl_observability);
    Ok((KernelValue::Array(values), observability))
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

    let (capture_kind, capture_index, cache_bricks) = match descriptor.capture_kind {
        CaptureKind::Field => {
            let capture = ops.resolve_field_or_shape_capture(args.first())?;
            note_wgsl_normal_role_for_capture(ops, descriptor, &capture);
            (0u32, field_index(ops.context(), &capture)?, Vec::new())
        }
        CaptureKind::Shape => {
            let capture = ops.resolve_shape_capture(args.first())?;
            note_wgsl_normal_role_for_capture(ops, descriptor, &capture);
            (
                1u32,
                shape_index(ops.context(), &capture)?,
                shape_cache_brick_kernel_values(ops.context(), &capture),
            )
        }
        CaptureKind::Region => {
            return Err(QueryExecError::Unsupported {
                message: "region captures are only valid for world queries".to_string(),
            });
        }
    };

    let item = scalar_item_arg(descriptor, args.get(1))?;

    Ok(GpuDispatchRequest {
        dispatch: dispatch_config(
            capture_kind,
            capture_index,
            1,
            0,
            0,
            0,
            cache_bricks.len() as u32,
            true,
            true,
            true,
        ),
        items: vec![item],
        world_shape_indices: Vec::new(),
        accel_nodes: Vec::new(),
        accel_children: Vec::new(),
        cache_bricks,
        continuation_seeds: Vec::new(),
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
    let accel = world_acceleration_request_data(ops.context(), &capture, detail)?;
    let cache_bricks = world_cache_brick_kernel_values(ops.context(), &capture, detail);
    note_wgsl_normal_role_for_world(ops, descriptor, &world_shapes);
    let item = scalar_item_arg(descriptor, args.get(2))?;

    Ok(GpuDispatchRequest {
        dispatch: dispatch_config(
            2,
            0,
            1,
            world_shape_indices.len() as u32,
            accel.root_index,
            accel.nodes.len() as u32,
            cache_bricks.len() as u32,
            scene_domain_flag_enabled(domain, SceneDomainFlag::Material)?,
            scene_domain_flag_enabled(domain, SceneDomainFlag::Radiance)?,
            scene_domain_flag_enabled(domain, SceneDomainFlag::Media)?,
        ),
        items: vec![item],
        world_shape_indices,
        accel_nodes: accel_nodes_kernel_values(&accel.nodes),
        accel_children: accel.children,
        cache_bricks,
        continuation_seeds: Vec::new(),
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
    let cache_bricks = match descriptor.capture_kind {
        CaptureKind::Shape => shape_cache_brick_kernel_values(ops.context(), &capture),
        CaptureKind::Field | CaptureKind::Region => Vec::new(),
    };
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
            0,
            0,
            cache_bricks.len() as u32,
            true,
            true,
            true,
        ),
        items: items.to_vec(),
        world_shape_indices: Vec::new(),
        accel_nodes: Vec::new(),
        accel_children: Vec::new(),
        cache_bricks,
        continuation_seeds: Vec::new(),
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
    let accel = world_acceleration_request_data(ops.context(), &capture, detail)?;
    let cache_bricks = world_cache_brick_kernel_values(ops.context(), &capture, detail);
    note_wgsl_normal_role_for_world(ops, descriptor, &world_shapes);

    Ok(GpuDispatchRequest {
        dispatch: dispatch_config(
            2,
            0,
            items.len() as u32,
            world_shape_indices.len() as u32,
            accel.root_index,
            accel.nodes.len() as u32,
            cache_bricks.len() as u32,
            scene_domain_flag_enabled(domain, SceneDomainFlag::Material)?,
            scene_domain_flag_enabled(domain, SceneDomainFlag::Radiance)?,
            scene_domain_flag_enabled(domain, SceneDomainFlag::Media)?,
        ),
        items: items.to_vec(),
        world_shape_indices,
        accel_nodes: accel_nodes_kernel_values(&accel.nodes),
        accel_children: accel.children,
        cache_bricks,
        continuation_seeds: Vec::new(),
    })
}

fn generate_compiled_shader(
    ctx: &crate::query_exec::context::QueryExecContext,
    plan: ShaderPlan<'_>,
) -> Result<GeneratedShaderModule, QueryExecError> {
    let generated = generate_shader(ctx, plan)?;
    validate_generated_shader(&generated.source)?;
    let layout_signature = query_wgsl_layout_signature(
        &generated.dispatch_abi,
        &generated.accel_node_abi,
        &generated.cache_brick_abi,
        &generated.shape_meta_abi,
        &generated.item_abi,
        &generated.result_abi,
    );
    Ok(GeneratedShaderModule {
        source: generated.source,
        workgroup_size: generated.workgroup_size,
        dispatch_abi: generated.dispatch_abi,
        accel_node_abi: generated.accel_node_abi.clone(),
        cache_brick_abi: generated.cache_brick_abi.clone(),
        shape_meta_abi: generated.shape_meta_abi.clone(),
        item_abi: generated.item_abi,
        result_abi: generated.result_abi,
        shape_meta_values: generated.shape_meta_values,
        layout_signature,
        bind_group_count: QUERY_WGSL_BIND_GROUP_COUNT,
    })
}

fn query_wgsl_layout_signature(
    dispatch_abi: &PortableAbiType,
    accel_node_abi: &PortableAbiType,
    cache_brick_abi: &PortableAbiType,
    shape_meta_abi: &PortableAbiType,
    item_abi: &PortableAbiType,
    result_abi: &PortableAbiType,
) -> u64 {
    let dispatch = format!("{dispatch_abi:?}");
    let accel_node = format!("{accel_node_abi:?}");
    let cache_brick = format!("{cache_brick_abi:?}");
    let shape_meta = format!("{shape_meta_abi:?}");
    let item = format!("{item_abi:?}");
    let result = format!("{result_abi:?}");
    stable_semantic_id(&[
        b"query_exec::wgsl::layout::v2",
        dispatch.as_bytes(),
        accel_node.as_bytes(),
        cache_brick.as_bytes(),
        shape_meta.as_bytes(),
        item.as_bytes(),
        result.as_bytes(),
        &QUERY_WGSL_BIND_GROUP_COUNT.to_le_bytes(),
        &QUERY_WGSL_STORAGE_BUFFERS_PER_SHADER_STAGE.to_le_bytes(),
    ])
}

fn world_acceleration_request_data(
    ctx: &crate::query_exec::context::QueryExecContext,
    capture: &SmolStr,
    detail: i32,
) -> Result<WgslAccelerationForestData, QueryExecError> {
    let Some(forest) = ctx.world_acceleration_forest(capture, detail) else {
        return Ok(WgslAccelerationForestData {
            root_index: 0,
            nodes: Vec::new(),
            children: Vec::new(),
        });
    };
    build_wgsl_acceleration_forest_data(ctx, forest)
}

fn build_wgsl_acceleration_forest_data(
    ctx: &crate::query_exec::context::QueryExecContext,
    forest: &AccelerationForest,
) -> Result<WgslAccelerationForestData, QueryExecError> {
    let Some(root_id) = forest.root_nodes().first() else {
        return Ok(WgslAccelerationForestData {
            root_index: 0,
            nodes: Vec::new(),
            children: Vec::new(),
        });
    };
    let node_lookup = forest
        .nodes
        .iter()
        .map(|node| (node.id.clone(), node))
        .collect::<HashMap<_, _>>();
    let mut built = HashMap::<SmolStr, usize>::new();
    let mut nodes = Vec::new();
    let mut children = Vec::new();
    let root_index = build_wgsl_acceleration_subtree(
        ctx,
        root_id,
        &node_lookup,
        &mut built,
        &mut nodes,
        &mut children,
    )?;
    Ok(WgslAccelerationForestData {
        root_index: root_index as u32,
        nodes,
        children,
    })
}

fn build_wgsl_acceleration_subtree(
    ctx: &crate::query_exec::context::QueryExecContext,
    id: &SmolStr,
    node_lookup: &HashMap<SmolStr, &crate::acceleration::AccelerationNode>,
    built: &mut HashMap<SmolStr, usize>,
    nodes: &mut Vec<WgslAccelNodeRecord>,
    children: &mut Vec<u32>,
) -> Result<usize, QueryExecError> {
    if let Some(existing) = built.get(id).copied() {
        return Ok(existing);
    }
    let source = node_lookup
        .get(id)
        .copied()
        .ok_or_else(|| QueryExecError::Unsupported {
            message: format!("shared acceleration forest is missing node '{id}'"),
        })?;
    let mut child_indices = Vec::new();
    for child_id in &source.child_ids {
        child_indices.push(build_wgsl_acceleration_subtree(
            ctx,
            child_id,
            node_lookup,
            built,
            nodes,
            children,
        )? as u32);
    }
    let child_start = children.len() as u32;
    children.extend(child_indices.iter().copied());
    let bounds = acceleration_node_bounds(source);
    let leaf_shape_index = source
        .leaf_payload
        .as_ref()
        .map(|payload| shape_index(ctx, &payload.semantic_id))
        .transpose()?
        .unwrap_or(u32::MAX);
    let mut flags = 0;
    if source.leaf_payload.is_some() {
        flags |= QUERY_WGSL_ACCEL_FLAG_LEAF;
    }
    if bounds.is_some() {
        flags |= QUERY_WGSL_ACCEL_FLAG_HAS_BOUNDS;
    }
    let (min, max) = bounds.unwrap_or(([0.0, 0.0, 0.0], [0.0, 0.0, 0.0]));
    let index = nodes.len();
    nodes.push(WgslAccelNodeRecord {
        min,
        max,
        child_start,
        child_len: child_indices.len() as u32,
        leaf_shape_index,
        flags,
    });
    built.insert(id.clone(), index);
    Ok(index)
}

fn acceleration_node_bounds(
    node: &crate::acceleration::AccelerationNode,
) -> Option<([f32; 3], [f32; 3])> {
    node.bounds.iter().find_map(|bound| {
        if !matches!(bound.kind, BoundDescriptorKind::AxisAlignedBounds) {
            return None;
        }
        parse_support_bounds_summary(&bound.summary).map(|bounds| (bounds.min, bounds.max))
    })
}

#[derive(Debug, Clone, Copy)]
struct SupportBounds {
    min: [f32; 3],
    max: [f32; 3],
}

fn parse_support_bounds_summary(summary: &str) -> Option<SupportBounds> {
    let (min, max) = summary.split_once("|max=")?;
    Some(SupportBounds {
        min: parse_summary_vec3(min.strip_prefix("min=")?)?,
        max: parse_summary_vec3(max)?,
    })
}

fn parse_summary_vec3(summary: &str) -> Option<[f32; 3]> {
    let parts = summary
        .split(',')
        .map(|part| part.trim().parse::<f32>().ok())
        .collect::<Option<Vec<_>>>()?;
    let [x, y, z] = parts.try_into().ok()?;
    Some([x, y, z])
}

fn shape_cache_brick_kernel_values(
    ctx: &crate::query_exec::context::QueryExecContext,
    shape: &SmolStr,
) -> Vec<KernelValue> {
    cache_brick_kernel_values(ctx.shape_cache_support(shape))
}

fn world_cache_brick_kernel_values(
    ctx: &crate::query_exec::context::QueryExecContext,
    capture: &SmolStr,
    detail: i32,
) -> Vec<KernelValue> {
    cache_brick_kernel_values(ctx.world_cache_support(capture, detail))
}

fn cache_brick_kernel_values(cache: Option<&SupportBrickCache>) -> Vec<KernelValue> {
    let Some(cache) = cache.filter(|cache| cache.is_ready()) else {
        return Vec::new();
    };
    if cache.bricks.is_empty() {
        return Vec::new();
    }
    cache
        .bricks
        .iter()
        .map(|brick| {
            KernelValue::Struct(KernelStructValue {
                name: SmolStr::new("WgslCacheBrick"),
                fields: vec![
                    (SmolStr::new("min"), KernelValue::Vec3(brick.bounds.min)),
                    (SmolStr::new("max"), KernelValue::Vec3(brick.bounds.max)),
                ],
            })
        })
        .collect()
}

fn accel_nodes_kernel_values(nodes: &[WgslAccelNodeRecord]) -> Vec<KernelValue> {
    if nodes.is_empty() {
        return vec![empty_accel_node_kernel_value()];
    }
    nodes
        .iter()
        .map(|node| {
            KernelValue::Struct(KernelStructValue {
                name: SmolStr::new("WgslAccelNode"),
                fields: vec![
                    (SmolStr::new("min"), KernelValue::Vec3(node.min)),
                    (SmolStr::new("max"), KernelValue::Vec3(node.max)),
                    (
                        SmolStr::new("child_start"),
                        KernelValue::U32(node.child_start),
                    ),
                    (SmolStr::new("child_len"), KernelValue::U32(node.child_len)),
                    (
                        SmolStr::new("leaf_shape_index"),
                        KernelValue::U32(node.leaf_shape_index),
                    ),
                    (SmolStr::new("flags"), KernelValue::U32(node.flags)),
                ],
            })
        })
        .collect()
}

fn empty_accel_node_kernel_value() -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("WgslAccelNode"),
        fields: vec![
            (SmolStr::new("min"), KernelValue::Vec3([0.0, 0.0, 0.0])),
            (SmolStr::new("max"), KernelValue::Vec3([0.0, 0.0, 0.0])),
            (SmolStr::new("child_start"), KernelValue::U32(0)),
            (SmolStr::new("child_len"), KernelValue::U32(0)),
            (SmolStr::new("leaf_shape_index"), KernelValue::U32(u32::MAX)),
            (SmolStr::new("flags"), KernelValue::U32(0)),
        ],
    })
}

fn empty_cache_brick_kernel_value() -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("WgslCacheBrick"),
        fields: vec![
            (SmolStr::new("min"), KernelValue::Vec3([0.0, 0.0, 0.0])),
            (SmolStr::new("max"), KernelValue::Vec3([0.0, 0.0, 0.0])),
        ],
    })
}

fn empty_shape_meta_kernel_value() -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("WgslShapeMeta"),
        fields: vec![
            (SmolStr::new("root_shape_id"), KernelValue::U32(0)),
            (SmolStr::new("analytic_kind"), KernelValue::U32(0)),
        ],
    })
}

fn encode_u32_values(values: &[u32]) -> Result<Vec<u8>, QueryExecError> {
    let values = if values.is_empty() {
        vec![KernelValue::U32(0)]
    } else {
        values.iter().copied().map(KernelValue::U32).collect()
    };
    encode_slice(&PortableAbiType::U32, &values)
}

fn encode_accel_node_values(
    abi: &PortableAbiType,
    values: &[KernelValue],
) -> Result<Vec<u8>, QueryExecError> {
    if values.is_empty() {
        encode_slice(abi, &[empty_accel_node_kernel_value()])
    } else {
        encode_slice(abi, values)
    }
}

fn encode_cache_brick_values(
    abi: &PortableAbiType,
    values: &[KernelValue],
) -> Result<Vec<u8>, QueryExecError> {
    if values.is_empty() {
        encode_slice(abi, &[empty_cache_brick_kernel_value()])
    } else {
        encode_slice(abi, values)
    }
}

fn encode_shape_meta_values(
    abi: &PortableAbiType,
    values: &[KernelValue],
) -> Result<Vec<u8>, QueryExecError> {
    if values.is_empty() {
        encode_slice(abi, &[empty_shape_meta_kernel_value()])
    } else {
        encode_slice(abi, values)
    }
}

fn storage_buffer_size(bytes: &[u8]) -> u64 {
    bytes.len().max(4) as u64
}

pub(crate) fn dispatch_compiled_shader(
    generated: &GeneratedShaderModule,
    request: GpuDispatchRequest,
) -> Result<Vec<KernelValue>, QueryExecError> {
    dispatch_compiled_shader_with_observability(generated, request).map(|(values, _)| values)
}

pub(crate) fn dispatch_compiled_shader_with_observability(
    generated: &GeneratedShaderModule,
    request: GpuDispatchRequest,
) -> Result<(Vec<KernelValue>, QueryExecutionObservability), QueryExecError> {
    if request.items.is_empty() {
        return Ok((Vec::new(), QueryExecutionObservability::default()));
    }

    let dispatch_bytes = encode_value(&generated.dispatch_abi, &request.dispatch)?;
    let input_bytes = encode_slice(&generated.item_abi, &request.items)?;
    let accel_node_bytes =
        encode_accel_node_values(&generated.accel_node_abi, &request.accel_nodes)?;
    let accel_child_bytes = encode_u32_values(&request.accel_children)?;
    let cache_brick_bytes =
        encode_cache_brick_values(&generated.cache_brick_abi, &request.cache_bricks)?;
    let shape_meta_bytes =
        encode_shape_meta_values(&generated.shape_meta_abi, &generated.shape_meta_values)?;
    let world_shape_bytes = encode_shape_indices(&request.world_shape_indices)?;
    let continuation_seed_bytes = encode_u32_values(&request.continuation_seeds)?;
    let result_stride = portable_abi_array_stride(&generated.result_abi) as usize;
    let result_buffer_size = (result_stride * request.items.len()).max(result_stride.max(4)) as u64;
    let used_max_storage_buffer_bytes = [
        storage_buffer_size(&dispatch_bytes),
        storage_buffer_size(&input_bytes),
        result_buffer_size,
        storage_buffer_size(&accel_node_bytes),
        storage_buffer_size(&accel_child_bytes),
        storage_buffer_size(&cache_brick_bytes),
        storage_buffer_size(&shape_meta_bytes),
        storage_buffer_size(&world_shape_bytes),
        storage_buffer_size(&continuation_seed_bytes),
        (QUERY_WGSL_OBSERVABILITY_U32S * std::mem::size_of::<u32>()) as u64,
    ]
    .into_iter()
    .max()
    .unwrap_or(4);
    let limit_request = WgslLimitRequest {
        max_storage_buffers_per_shader_stage: QUERY_WGSL_STORAGE_BUFFERS_PER_SHADER_STAGE,
        max_storage_buffer_binding_size: used_max_storage_buffer_bytes,
    };
    let native = native_wgpu_context_for_limits(limit_request)?;
    let selected_workgroup_size = select_query_wgsl_workgroup_size(&native.adapter_limits)?;
    let input_buffer = native
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("wrela.wgsl.input"),
            contents: &input_bytes,
            usage: wgpu::BufferUsages::STORAGE,
        });
    let output_buffer = native.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("wrela.wgsl.output"),
        size: result_buffer_size,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let observability_buffer =
        native
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("wrela.wgsl.observability"),
                contents: &[0u8; QUERY_WGSL_OBSERVABILITY_U32S * std::mem::size_of::<u32>()],
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            });

    let diagnostics = WgslDispatchDiagnostics {
        selected_workgroup_size,
        used_max_storage_buffer_bytes,
        requested_max_storage_buffer_bytes: native.requested_limits.max_storage_buffer_binding_size,
    };
    dispatch_compiled_shader_with_buffers(
        generated,
        &request,
        &input_buffer,
        &output_buffer,
        &observability_buffer,
        diagnostics,
    )?;
    let bytes = readback_storage_buffer_on(&native, &output_buffer, result_buffer_size)?;
    let observability_bytes = readback_storage_buffer_on(
        &native,
        &observability_buffer,
        (QUERY_WGSL_OBSERVABILITY_U32S * std::mem::size_of::<u32>()) as u64,
    )?;

    Ok((
        decode_slice(&generated.result_abi, &bytes, request.items.len())?,
        decode_wgsl_observability(generated, &diagnostics, &observability_bytes),
    ))
}

fn dispatch_compiled_shader_with_buffers(
    generated: &GeneratedShaderModule,
    request: &GpuDispatchRequest,
    input_buffer: &wgpu::Buffer,
    output_buffer: &wgpu::Buffer,
    observability_buffer: &wgpu::Buffer,
    diagnostics: WgslDispatchDiagnostics,
) -> Result<(), QueryExecError> {
    if request.items.is_empty() {
        return Ok(());
    }
    let native = native_wgpu_context_for_limits(WgslLimitRequest {
        max_storage_buffers_per_shader_stage: QUERY_WGSL_STORAGE_BUFFERS_PER_SHADER_STAGE,
        max_storage_buffer_binding_size: diagnostics.used_max_storage_buffer_bytes,
    })?;
    let dispatch_bytes = encode_value(&generated.dispatch_abi, &request.dispatch)?;
    let accel_node_bytes =
        encode_accel_node_values(&generated.accel_node_abi, &request.accel_nodes)?;
    let accel_child_bytes = encode_u32_values(&request.accel_children)?;
    let cache_brick_bytes =
        encode_cache_brick_values(&generated.cache_brick_abi, &request.cache_bricks)?;
    let shape_meta_bytes =
        encode_shape_meta_values(&generated.shape_meta_abi, &generated.shape_meta_values)?;
    let shape_bytes = encode_shape_indices(&request.world_shape_indices)?;
    let continuation_seed_bytes = encode_u32_values(&request.continuation_seeds)?;
    let dispatch_buffer = native
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("wrela.wgsl.dispatch"),
            contents: &dispatch_bytes,
            usage: wgpu::BufferUsages::STORAGE,
        });
    let accel_nodes_buffer = native
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("wrela.wgsl.accel_nodes"),
            contents: &accel_node_bytes,
            usage: wgpu::BufferUsages::STORAGE,
        });
    let accel_children_buffer =
        native
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("wrela.wgsl.accel_children"),
                contents: &accel_child_bytes,
                usage: wgpu::BufferUsages::STORAGE,
            });
    let cache_bricks_buffer = native
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("wrela.wgsl.cache_bricks"),
            contents: &cache_brick_bytes,
            usage: wgpu::BufferUsages::STORAGE,
        });
    let shape_meta_buffer = native
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("wrela.wgsl.shape_meta"),
            contents: &shape_meta_bytes,
            usage: wgpu::BufferUsages::STORAGE,
        });
    let world_shapes_buffer = native
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("wrela.wgsl.world_shapes"),
            contents: &shape_bytes,
            usage: wgpu::BufferUsages::STORAGE,
        });
    let continuation_seed_buffer =
        native
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("wrela.wgsl.continuation"),
                contents: &continuation_seed_bytes,
                usage: wgpu::BufferUsages::STORAGE,
            });
    let cached = compiled_query_pipeline(
        &native,
        &generated.source,
        diagnostics.selected_workgroup_size,
        generated,
    )?;
    let bind_group0 = native.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("wrela.wgsl.bind_group0"),
        layout: &cached.bind_group_layouts[0],
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: dispatch_buffer.as_entire_binding(),
        }],
    });
    let bind_group1 = native.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("wrela.wgsl.bind_group1"),
        layout: &cached.bind_group_layouts[1],
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: accel_nodes_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: accel_children_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: shape_meta_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: cache_bricks_buffer.as_entire_binding(),
            },
        ],
    });
    let bind_group2 = native.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("wrela.wgsl.bind_group2"),
        layout: &cached.bind_group_layouts[2],
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: input_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: output_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: world_shapes_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: observability_buffer.as_entire_binding(),
            },
        ],
    });
    let bind_group3 = native.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("wrela.wgsl.bind_group3"),
        layout: &cached.bind_group_layouts[3],
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: continuation_seed_buffer.as_entire_binding(),
        }],
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
        pass.set_bind_group(0, &bind_group0, &[]);
        pass.set_bind_group(1, &bind_group1, &[]);
        pass.set_bind_group(2, &bind_group2, &[]);
        pass.set_bind_group(3, &bind_group3, &[]);
        pass.dispatch_workgroups(
            dispatch_workgroups_x_for_items(
                request.items.len() as u32,
                diagnostics.selected_workgroup_size,
            ),
            1,
            1,
        );
    }
    native.queue.submit(Some(encoder.finish()));
    Ok(())
}

fn dispatch_workgroups_x_for_items(item_count: u32, workgroup_size: u32) -> u32 {
    item_count.div_ceil(workgroup_size.max(1))
}

fn current_selected_query_workgroup_size() -> Result<u32, QueryExecError> {
    let native = native_wgpu_context()?;
    select_query_wgsl_workgroup_size(&native.adapter_limits)
}

pub(crate) fn readback_storage_buffer(
    buffer: &wgpu::Buffer,
    size: u64,
) -> Result<Vec<u8>, QueryExecError> {
    let native = native_wgpu_context()?;
    readback_storage_buffer_on(&native, buffer, size)
}

pub(crate) fn readback_storage_buffer_on(
    native: &NativeWgpuContext,
    buffer: &wgpu::Buffer,
    size: u64,
) -> Result<Vec<u8>, QueryExecError> {
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
    let pipeline = create_compute_pipeline(
        native,
        source,
        workgroup_size,
        &pipeline_layout,
        "wrela.wgsl.pipeline",
    )?;

    let cached = CachedPipeline {
        bind_group_layout,
        pipeline,
    };
    let mut guard = cache.lock().unwrap_or_else(|poison| poison.into_inner());
    Ok(guard.entry(key).or_insert_with(|| cached.clone()).clone())
}

fn compiled_query_pipeline(
    native: &NativeWgpuContext,
    source: &str,
    workgroup_size: u32,
    generated: &GeneratedShaderModule,
) -> Result<QueryCachedPipeline, QueryExecError> {
    static PIPELINES: OnceLock<Mutex<HashMap<(String, u32, u64, u32, u64), QueryCachedPipeline>>> =
        OnceLock::new();
    let cache = PIPELINES.get_or_init(|| Mutex::new(HashMap::new()));
    let key = (
        source.to_string(),
        workgroup_size,
        generated.layout_signature,
        native.requested_limits.max_storage_buffers_per_shader_stage,
        native.requested_limits.max_storage_buffer_binding_size,
    );

    {
        let guard = cache.lock().unwrap_or_else(|poison| poison.into_inner());
        if let Some(cached) = guard.get(&key) {
            return Ok(cached.clone());
        }
    }

    let dispatch_layout =
        native
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("wrela.wgsl.query.group0"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(
                            portable_abi_layout(&generated.dispatch_abi).size as u64,
                        ),
                    },
                    count: None,
                }],
            });
    let static_layout = native
        .device
        .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("wrela.wgsl.query.group1"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(
                            portable_abi_layout(&generated.accel_node_abi).size as u64,
                        ),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(4),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(
                            portable_abi_layout(&generated.shape_meta_abi).size as u64,
                        ),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(
                            portable_abi_layout(&generated.cache_brick_abi).size as u64,
                        ),
                    },
                    count: None,
                },
            ],
        });
    let io_layout = native
        .device
        .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("wrela.wgsl.query.group2"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(4),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(
                            (QUERY_WGSL_OBSERVABILITY_U32S * std::mem::size_of::<u32>()) as u64,
                        ),
                    },
                    count: None,
                },
            ],
        });
    let temporal_layout =
        native
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("wrela.wgsl.query.group3"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(4),
                    },
                    count: None,
                }],
            });
    let pipeline_layout = native
        .device
        .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("wrela.wgsl.query.pipeline_layout"),
            bind_group_layouts: &[
                Some(&dispatch_layout),
                Some(&static_layout),
                Some(&io_layout),
                Some(&temporal_layout),
            ],
            immediate_size: 0,
        });
    let pipeline = create_compute_pipeline(
        native,
        source,
        workgroup_size,
        &pipeline_layout,
        "wrela.wgsl.query.pipeline",
    )?;

    let cached = QueryCachedPipeline {
        bind_group_layouts: [dispatch_layout, static_layout, io_layout, temporal_layout],
        pipeline,
    };
    let mut guard = cache.lock().unwrap_or_else(|poison| poison.into_inner());
    Ok(guard.entry(key).or_insert_with(|| cached.clone()).clone())
}

fn create_compute_pipeline(
    native: &NativeWgpuContext,
    source: &str,
    workgroup_size: u32,
    pipeline_layout: &wgpu::PipelineLayout,
    label: &str,
) -> Result<wgpu::ComputePipeline, QueryExecError> {
    let shader_module = native
        .device
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(label),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(source)),
        });
    let error_scope = native
        .device
        .push_error_scope(wgpu::ErrorFilter::Validation);
    let pipeline = native
        .device
        .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some(label),
            layout: Some(pipeline_layout),
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
    Ok(pipeline)
}

fn decode_wgsl_observability(
    generated: &GeneratedShaderModule,
    diagnostics: &WgslDispatchDiagnostics,
    bytes: &[u8],
) -> QueryExecutionObservability {
    let read_u32 = |index: usize| -> u32 {
        let start = index * std::mem::size_of::<u32>();
        let end = start + std::mem::size_of::<u32>();
        bytes
            .get(start..end)
            .and_then(|slice| slice.try_into().ok())
            .map(u32::from_le_bytes)
            .unwrap_or_default()
    };
    QueryExecutionObservability {
        acceleration_node_visits: read_u32(0),
        shape_leaf_visits: read_u32(1),
        acceleration_pruned_nodes: read_u32(2),
        ray_support_interval_rejections: read_u32(3),
        ray_support_entry_jumps: read_u32(4),
        cache_brick_visits: read_u32(5),
        cache_brick_hits: read_u32(6),
        cache_brick_misses: read_u32(7),
        cache_interval_advances: read_u32(8),
        cache_resident_shared_snapshot_artifacts: read_u32(9),
        cache_resident_observer_local_artifacts: read_u32(10),
        cache_upload_attempts: read_u32(11),
        cache_upload_rejections: read_u32(12),
        cache_budget_rejections: read_u32(13),
        cache_dense_fallback_rays: read_u32(14),
        solver_analytic_hits: read_u32(15),
        solver_generated_dense_fallback_rays: read_u32(16),
        solver_support_rejections: read_u32(17),
        wgsl_layout_signature: Some(generated.layout_signature),
        wgsl_bind_group_count: generated.bind_group_count,
        wgsl_requested_max_storage_buffer_bytes: diagnostics.requested_max_storage_buffer_bytes,
        wgsl_used_max_storage_buffer_bytes: diagnostics.used_max_storage_buffer_bytes,
        wgsl_selected_workgroup_size: diagnostics.selected_workgroup_size,
        ..QueryExecutionObservability::default()
    }
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

pub(crate) fn native_wgpu_context() -> Result<Arc<NativeWgpuContext>, QueryExecError> {
    native_wgpu_context_for_limits(WgslLimitRequest::default())
}

fn native_wgpu_context_for_limits(
    request: WgslLimitRequest,
) -> Result<Arc<NativeWgpuContext>, QueryExecError> {
    static CONTEXTS: OnceLock<
        Mutex<HashMap<WgslLimitRequest, Result<Arc<NativeWgpuContext>, String>>>,
    > = OnceLock::new();
    let cache = CONTEXTS.get_or_init(|| Mutex::new(HashMap::new()));
    {
        let guard = cache.lock().unwrap_or_else(|poison| poison.into_inner());
        if let Some(context) = guard.get(&request) {
            return context
                .clone()
                .map_err(|message| QueryExecError::Unsupported {
                    message: format!("native WGSL backend initialization failed: {message}"),
                });
        }
    }
    let context = init_native_wgpu_context_for_limits(request).map(Arc::new);
    let mut guard = cache.lock().unwrap_or_else(|poison| poison.into_inner());
    let entry = guard.entry(request).or_insert_with(|| context.clone());
    match entry {
        Ok(context) => Ok(context.clone()),
        Err(message) => Err(QueryExecError::Unsupported {
            message: format!("native WGSL backend initialization failed: {message}"),
        }),
    }
}

fn init_native_wgpu_context_for_limits(
    request: WgslLimitRequest,
) -> Result<NativeWgpuContext, String> {
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let adapter = pollster::block_on(initialize_adapter_from_env_or_default(&instance, None))
        .map_err(|err| format!("request adapter failed: {err}"))?;
    let adapter_limits = adapter.limits();
    if request.max_storage_buffers_per_shader_stage
        > adapter_limits.max_storage_buffers_per_shader_stage
    {
        return Err(format!(
            "requested {} storage buffers per shader stage but adapter profile only supports {}",
            request.max_storage_buffers_per_shader_stage,
            adapter_limits.max_storage_buffers_per_shader_stage
        ));
    }
    if request.max_storage_buffer_binding_size > adapter_limits.max_storage_buffer_binding_size {
        return Err(format!(
            "requested storage buffer binding size {} exceeds adapter profile {}",
            request.max_storage_buffer_binding_size, adapter_limits.max_storage_buffer_binding_size
        ));
    }
    if QUERY_WGSL_BIND_GROUP_COUNT > adapter_limits.max_bind_groups {
        return Err(format!(
            "query WGSL layout needs {} bind groups but adapter profile only supports {}",
            QUERY_WGSL_BIND_GROUP_COUNT, adapter_limits.max_bind_groups
        ));
    }
    let selected_workgroup_size =
        select_query_wgsl_workgroup_size(&adapter_limits).map_err(|err| err.to_string())?;
    let mut required_limits = wgpu::Limits::downlevel_defaults()
        .using_resolution(adapter_limits.clone())
        .using_alignment(adapter_limits.clone());
    required_limits.max_bind_groups = QUERY_WGSL_BIND_GROUP_COUNT;
    required_limits.max_storage_buffers_per_shader_stage =
        request.max_storage_buffers_per_shader_stage;
    required_limits.max_storage_buffer_binding_size = request.max_storage_buffer_binding_size;
    required_limits.max_compute_invocations_per_workgroup = selected_workgroup_size;
    required_limits.max_compute_workgroup_size_x = selected_workgroup_size;
    required_limits.max_compute_workgroup_size_y = 1;
    required_limits.max_compute_workgroup_size_z = 1;
    let descriptor = wgpu::DeviceDescriptor {
        label: Some("wrela.wgsl.device"),
        required_features: wgpu::Features::empty(),
        required_limits: required_limits.clone(),
        experimental_features: wgpu::ExperimentalFeatures::disabled(),
        memory_hints: wgpu::MemoryHints::Performance,
        trace: wgpu::Trace::Off,
    };
    let (device, queue) = pollster::block_on(adapter.request_device(&descriptor))
        .map_err(|err| format!("request device failed: {err}"))?;
    Ok(NativeWgpuContext {
        device,
        queue,
        adapter_limits,
        requested_limits: required_limits,
    })
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
    accel_root_index: u32,
    accel_node_count: u32,
    cache_brick_count: u32,
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
                SmolStr::new("accel_root_index"),
                KernelValue::U32(accel_root_index),
            ),
            (
                SmolStr::new("accel_node_count"),
                KernelValue::U32(accel_node_count),
            ),
            (
                SmolStr::new("cache_brick_count"),
                KernelValue::U32(cache_brick_count),
            ),
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

#[cfg(test)]
mod tests {
    use super::dispatch_workgroups_x_for_items;

    #[test]
    fn dispatch_workgroups_follow_selected_workgroup_size() {
        assert_eq!(dispatch_workgroups_x_for_items(96, 32), 3);
        assert_eq!(dispatch_workgroups_x_for_items(96, 64), 2);
        assert_eq!(dispatch_workgroups_x_for_items(96, 128), 1);
    }
}
