pub(crate) mod codegen;

use self::codegen::{ShaderPlan, generate_shader};
use crate::acceleration::cache::SupportBrickCache;
use crate::acceleration::{AccelerationForest, BoundDescriptorKind};
use crate::execution_policy::QueryExecutionPolicy;
use crate::gpu_runtime::{
    GpuLimitRequest, GpuPassProfiler, GpuRuntimeContext, GpuRuntimeMetrics,
    readback_storage_buffer_on as shared_readback_storage_buffer_on, shared_wgpu_context,
};
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
#[cfg(test)]
use std::cell::Cell;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use wgpu::util::DeviceExt;

const QUERY_WGSL_BIND_GROUP_COUNT: u32 = 4;
const QUERY_WGSL_STORAGE_BUFFERS_PER_SHADER_STAGE: u32 = 10;
const QUERY_WGSL_ACCEL_FLAG_LEAF: u32 = 1;
const QUERY_WGSL_ACCEL_FLAG_HAS_BOUNDS: u32 = 2;
const QUERY_WGSL_OBSERVABILITY_U32S: usize = 18;

pub(crate) type NativeWgpuContext = GpuRuntimeContext;

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

type WgslLimitRequest = GpuLimitRequest;

#[derive(Debug, Clone, Copy)]
struct WgslDispatchDiagnostics {
    selected_workgroup_size: u32,
    used_max_storage_buffer_bytes: u64,
    requested_max_storage_buffer_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct WgslDispatchChunkPlan {
    items_per_chunk: usize,
    chunk_count: usize,
}

#[cfg(test)]
thread_local! {
    static TEST_WGSL_CHUNK_STORAGE_BUFFER_LIMIT_OVERRIDE: Cell<Option<u64>> = const { Cell::new(None) };
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
    if let Some(ray_solver) = &plan.ray_solver {
        ops.note_solver_plan(ray_solver);
    }
    if let Err(errors) = validate_batch_query_plan(plan) {
        ops.note_contract_validation_failure();
        return Err(validation_error("batch query", errors));
    }
    let generated = generate_compiled_shader(ctx, ShaderPlan::Batch(plan))?;
    let request = build_batch_request(&ops, plan, args)?;
    if descriptor_for_plan(plan.contract_id)?.target == QueryTargetKind::World {
        ops.note_world_batch_items(request.items.len() as u32);
    }
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

    let chunk_plan = compute_wgsl_dispatch_chunk_plan(generated, &request)?;
    if chunk_plan.chunk_count > 1 {
        let mut values = Vec::with_capacity(request.items.len());
        let mut observability = QueryExecutionObservability::default();
        for (chunk_index, start) in (0..request.items.len())
            .step_by(chunk_plan.items_per_chunk)
            .enumerate()
        {
            let end = (start + chunk_plan.items_per_chunk).min(request.items.len());
            let chunk_request = slice_gpu_dispatch_request(&request, start..end)?;
            let (chunk_values, mut chunk_observability) =
                dispatch_compiled_shader_single_with_observability(generated, chunk_request)?;
            if chunk_index > 0 {
                suppress_repeated_chunk_seed_metrics(&mut chunk_observability);
            }
            values.extend(chunk_values);
            observability.merge_from(&chunk_observability);
        }
        observability.gpu_runtime.dispatch_fragmentation_count = observability
            .gpu_runtime
            .dispatch_fragmentation_count
            .saturating_add(chunk_plan.chunk_count.saturating_sub(1) as u32);
        return Ok((values, observability));
    }

    dispatch_compiled_shader_single_with_observability(generated, request)
}

fn dispatch_compiled_shader_single_with_observability(
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
    let mut profiler = GpuPassProfiler::new(&native, 1);
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
    let mut gpu_runtime = GpuRuntimeMetrics {
        timestamps_supported: profiler.timestamps_supported(),
        timestamped_pass_count: 0,
        gpu_time_total_micros: 0,
        gpu_time_max_micros: 0,
        queue_submit_count: 0,
        transient_buffer_creations: 3,
        transient_bind_group_creations: 0,
        upload_bytes: storage_buffer_size(&input_bytes)
            + (QUERY_WGSL_OBSERVABILITY_U32S * std::mem::size_of::<u32>()) as u64,
        readback_bytes: 0,
        cpu_screen_sample_allocations: 0,
        attachment_decode_count: 0,
        attachment_encode_count: 0,
        primary_visibility_packet_fanout_count: 0,
        dispatch_fragmentation_count: 0,
        scene_reupload_bytes: 0,
        pipeline_cache_hits: 0,
        pipeline_cache_misses: 0,
    };
    gpu_runtime.merge_from(&dispatch_compiled_shader_with_buffers(
        generated,
        &request,
        &input_buffer,
        &output_buffer,
        &observability_buffer,
        diagnostics,
        &mut profiler,
    )?);
    let bytes = readback_storage_buffer_on(&native, &output_buffer, result_buffer_size)?;
    let observability_bytes = readback_storage_buffer_on(
        &native,
        &observability_buffer,
        (QUERY_WGSL_OBSERVABILITY_U32S * std::mem::size_of::<u32>()) as u64,
    )?;
    let gpu_elapsed_micros = profiler
        .readback_gpu_elapsed_micros(&native)
        .map_err(|message| QueryExecError::Unsupported {
            message: format!("native WGSL GPU timing readback failed: {message}"),
        })?;
    gpu_runtime.note_gpu_timings(profiler.timestamps_supported(), &gpu_elapsed_micros);
    gpu_runtime.queue_submit_count = gpu_runtime
        .queue_submit_count
        .saturating_add(2 + u32::from(profiler.timestamps_supported()));
    gpu_runtime.transient_buffer_creations = gpu_runtime
        .transient_buffer_creations
        .saturating_add(2 + u32::from(profiler.timestamps_supported()));
    gpu_runtime.readback_bytes = gpu_runtime
        .readback_bytes
        .saturating_add(result_buffer_size)
        .saturating_add((QUERY_WGSL_OBSERVABILITY_U32S * std::mem::size_of::<u32>()) as u64)
        .saturating_add(gpu_runtime.timestamped_pass_count as u64 * 16);

    Ok((
        decode_slice(&generated.result_abi, &bytes, request.items.len())?,
        decode_wgsl_observability(
            generated,
            &diagnostics,
            &observability_bytes,
            request.items.len() as u32,
            gpu_runtime,
        ),
    ))
}

fn compute_wgsl_dispatch_chunk_plan(
    generated: &GeneratedShaderModule,
    request: &GpuDispatchRequest,
) -> Result<WgslDispatchChunkPlan, QueryExecError> {
    let item_count = request.items.len();
    if item_count == 0 {
        return Ok(WgslDispatchChunkPlan {
            items_per_chunk: 0,
            chunk_count: 0,
        });
    }

    let native = native_wgpu_context()?;
    let per_storage_buffer_limit = effective_chunk_storage_buffer_limit(&native.requested_limits);
    let item_stride = portable_abi_array_stride(&generated.item_abi) as u64;
    let result_stride = portable_abi_array_stride(&generated.result_abi) as u64;
    let per_item_seed_stride = continuation_seed_stride_for_chunking(request)?;
    let items_per_chunk = max_chunk_item_count(
        per_storage_buffer_limit,
        item_stride,
        result_stride,
        per_item_seed_stride,
    )?;
    Ok(WgslDispatchChunkPlan {
        items_per_chunk,
        chunk_count: item_count.div_ceil(items_per_chunk),
    })
}

fn effective_chunk_storage_buffer_limit(requested_limits: &wgpu::Limits) -> u64 {
    #[cfg(test)]
    if let Some(limit) = TEST_WGSL_CHUNK_STORAGE_BUFFER_LIMIT_OVERRIDE.with(Cell::get) {
        return limit;
    }

    requested_limits
        .max_storage_buffer_binding_size
        .min(requested_limits.max_buffer_size)
}

#[cfg(test)]
fn with_test_chunk_storage_buffer_limit_override<T>(limit: u64, f: impl FnOnce() -> T) -> T {
    struct Reset(Option<u64>);

    impl Drop for Reset {
        fn drop(&mut self) {
            TEST_WGSL_CHUNK_STORAGE_BUFFER_LIMIT_OVERRIDE.with(|cell| cell.set(self.0));
        }
    }

    TEST_WGSL_CHUNK_STORAGE_BUFFER_LIMIT_OVERRIDE.with(|cell| {
        let previous = cell.replace(Some(limit));
        let _reset = Reset(previous);
        f()
    })
}

fn continuation_seed_stride_for_chunking(
    request: &GpuDispatchRequest,
) -> Result<Option<u64>, QueryExecError> {
    if request.continuation_seeds.is_empty() {
        return Ok(None);
    }
    if request.continuation_seeds.len() == request.items.len() {
        return Ok(Some(std::mem::size_of::<u32>() as u64));
    }
    Err(QueryExecError::Unsupported {
        message: format!(
            "WGSL batch chunking requires continuation seeds to be empty or one-per-item, found {} seeds for {} items",
            request.continuation_seeds.len(),
            request.items.len()
        ),
    })
}

pub(crate) fn max_chunk_item_count(
    per_storage_buffer_limit: u64,
    item_stride: u64,
    result_stride: u64,
    per_item_seed_stride: Option<u64>,
) -> Result<usize, QueryExecError> {
    let mut item_limits = Vec::new();
    item_limits.push(max_items_for_stride(
        per_storage_buffer_limit,
        item_stride,
        "WGSL input item ABI",
    )?);
    item_limits.push(max_items_for_stride(
        per_storage_buffer_limit,
        result_stride,
        "WGSL result ABI",
    )?);
    if let Some(seed_stride) = per_item_seed_stride {
        item_limits.push(max_items_for_stride(
            per_storage_buffer_limit,
            seed_stride,
            "WGSL continuation seed ABI",
        )?);
    }
    let items_per_chunk = item_limits.into_iter().min().unwrap_or(1);
    Ok(items_per_chunk.max(1))
}

fn max_items_for_stride(
    per_storage_buffer_limit: u64,
    stride: u64,
    label: &str,
) -> Result<usize, QueryExecError> {
    if stride == 0 {
        return Err(QueryExecError::Unsupported {
            message: format!("{label} reported a zero byte stride"),
        });
    }
    let max_items = per_storage_buffer_limit / stride;
    if max_items == 0 {
        return Err(QueryExecError::Unsupported {
            message: format!(
                "{label} requires {stride} bytes per item but the adapter only allows {} bytes per storage buffer",
                per_storage_buffer_limit
            ),
        });
    }
    Ok(max_items as usize)
}

fn slice_gpu_dispatch_request(
    request: &GpuDispatchRequest,
    range: std::ops::Range<usize>,
) -> Result<GpuDispatchRequest, QueryExecError> {
    let mut dispatch = expect_struct_arg(Some(&request.dispatch), "WgslDispatchConfig")?.clone();
    if let Some((_, value)) = dispatch
        .fields
        .iter_mut()
        .find(|(name, _)| name == "item_count")
    {
        *value = KernelValue::U32(range.len() as u32);
    } else {
        return Err(QueryExecError::Unsupported {
            message: "WGSL dispatch config is missing item_count".to_string(),
        });
    }
    Ok(GpuDispatchRequest {
        dispatch: KernelValue::Struct(dispatch),
        items: request.items[range.clone()].to_vec(),
        world_shape_indices: request.world_shape_indices.clone(),
        accel_nodes: request.accel_nodes.clone(),
        accel_children: request.accel_children.clone(),
        cache_bricks: request.cache_bricks.clone(),
        continuation_seeds: if request.continuation_seeds.len() == request.items.len() {
            request.continuation_seeds[range].to_vec()
        } else {
            request.continuation_seeds.clone()
        },
    })
}

fn suppress_repeated_chunk_seed_metrics(observability: &mut QueryExecutionObservability) {
    observability.cache_resident_shared_snapshot_artifacts = 0;
    observability.cache_resident_observer_local_artifacts = 0;
    observability.cache_upload_attempts = 0;
    observability.cache_upload_rejections = 0;
}

fn dispatch_compiled_shader_with_buffers(
    generated: &GeneratedShaderModule,
    request: &GpuDispatchRequest,
    input_buffer: &wgpu::Buffer,
    output_buffer: &wgpu::Buffer,
    observability_buffer: &wgpu::Buffer,
    diagnostics: WgslDispatchDiagnostics,
    profiler: &mut GpuPassProfiler,
) -> Result<GpuRuntimeMetrics, QueryExecError> {
    if request.items.is_empty() {
        return Ok(GpuRuntimeMetrics::default());
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
    let mut gpu_runtime = GpuRuntimeMetrics::default();
    gpu_runtime.upload_bytes = storage_buffer_size(&dispatch_bytes)
        + storage_buffer_size(&accel_node_bytes)
        + storage_buffer_size(&accel_child_bytes)
        + storage_buffer_size(&cache_brick_bytes)
        + storage_buffer_size(&shape_meta_bytes)
        + storage_buffer_size(&shape_bytes)
        + storage_buffer_size(&continuation_seed_bytes);
    gpu_runtime.scene_reupload_bytes = storage_buffer_size(&accel_node_bytes)
        + storage_buffer_size(&accel_child_bytes)
        + storage_buffer_size(&cache_brick_bytes)
        + storage_buffer_size(&shape_meta_bytes)
        + storage_buffer_size(&shape_bytes);
    gpu_runtime.transient_buffer_creations = 7;
    gpu_runtime.transient_bind_group_creations = 4;
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
        &mut gpu_runtime,
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
        let timestamp_writes = profiler.compute_pass_timestamp_writes();
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("wrela.wgsl.compute_pass"),
            timestamp_writes,
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
    profiler.resolve_into(&mut encoder);
    native.queue.submit(Some(encoder.finish()));
    gpu_runtime.queue_submit_count = 1;
    Ok(gpu_runtime)
}

fn dispatch_workgroups_x_for_items(item_count: u32, workgroup_size: u32) -> u32 {
    item_count.div_ceil(workgroup_size.max(1))
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
    shared_readback_storage_buffer_on(native, buffer, size).map_err(|message| {
        QueryExecError::Unsupported {
            message: format!("native WGSL readback failed: {message}"),
        }
    })
}

pub(crate) fn compiled_pipeline(
    native: &NativeWgpuContext,
    source: &str,
    workgroup_size: u32,
    dispatch_min_size: Option<wgpu::BufferSize>,
    gpu_runtime: &mut GpuRuntimeMetrics,
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
            gpu_runtime.pipeline_cache_hits = gpu_runtime.pipeline_cache_hits.saturating_add(1);
            return Ok(cached.clone());
        }
    }
    gpu_runtime.pipeline_cache_misses = gpu_runtime.pipeline_cache_misses.saturating_add(1);

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
    gpu_runtime: &mut GpuRuntimeMetrics,
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
            gpu_runtime.pipeline_cache_hits = gpu_runtime.pipeline_cache_hits.saturating_add(1);
            return Ok(cached.clone());
        }
    }
    gpu_runtime.pipeline_cache_misses = gpu_runtime.pipeline_cache_misses.saturating_add(1);

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
    dispatch_items: u32,
    gpu_runtime: GpuRuntimeMetrics,
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
        dispatch_count: 1,
        dispatch_items,
        dispatch_workgroups_x: dispatch_workgroups_x_for_items(
            dispatch_items,
            diagnostics.selected_workgroup_size,
        ),
        dispatch_workgroups_y: 1,
        dispatch_workgroups_z: 1,
        wgsl_layout_signature: Some(generated.layout_signature),
        wgsl_bind_group_count: generated.bind_group_count,
        wgsl_requested_max_storage_buffer_bytes: diagnostics.requested_max_storage_buffer_bytes,
        wgsl_used_max_storage_buffer_bytes: diagnostics.used_max_storage_buffer_bytes,
        wgsl_selected_workgroup_size: diagnostics.selected_workgroup_size,
        gpu_runtime,
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
    let mut guard = cache.lock().unwrap_or_else(|poison| poison.into_inner());
    let entry = guard.entry(request).or_insert_with(|| {
        shared_wgpu_context(request)
            .map_err(|message| format!("native WGSL backend initialization failed: {message}"))
    });
    match entry {
        Ok(context) => Ok(context.clone()),
        Err(message) => Err(QueryExecError::Unsupported {
            message: format!("native WGSL backend initialization failed: {message}"),
        }),
    }
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
    use super::{
        GeneratedShaderModule, GpuDispatchRequest, WgslDispatchChunkPlan,
        dispatch_compiled_shader_with_observability, dispatch_config,
        dispatch_workgroups_x_for_items, max_chunk_item_count, slice_gpu_dispatch_request,
        suppress_repeated_chunk_seed_metrics, with_test_chunk_storage_buffer_limit_override,
    };
    use crate::kernel::{KernelStructValue, KernelValue};
    use crate::portable::{PortableAbiType, portable_abi_emit_wgsl_structs};
    use crate::query_exec::QueryExecutionObservability;
    use crate::query_exec::wgsl::codegen::{
        wgsl_accel_node_abi, wgsl_cache_brick_abi, wgsl_dispatch_config_abi, wgsl_shape_meta_abi,
    };
    use smol_str::SmolStr;

    #[test]
    fn dispatch_workgroups_follow_selected_workgroup_size() {
        assert_eq!(dispatch_workgroups_x_for_items(96, 32), 3);
        assert_eq!(dispatch_workgroups_x_for_items(96, 64), 2);
        assert_eq!(dispatch_workgroups_x_for_items(96, 128), 1);
    }

    #[test]
    fn chunk_plan_caps_items_to_storage_buffer_limits() {
        let plan = WgslDispatchChunkPlan {
            items_per_chunk: max_chunk_item_count(128 << 20, 48, 256, None).unwrap(),
            chunk_count: 2_073_600usize.div_ceil(524_288),
        };
        assert_eq!(plan.items_per_chunk, 524_288);
        assert_eq!(plan.chunk_count, 4);
    }

    #[test]
    fn slice_request_updates_dispatch_item_count_and_seeds() {
        let request = GpuDispatchRequest {
            dispatch: dispatch_config(2, 0, 6, 3, 1, 4, 0, true, false, false),
            items: (0..6).map(KernelValue::U32).collect(),
            world_shape_indices: vec![7, 8, 9],
            accel_nodes: Vec::new(),
            accel_children: Vec::new(),
            cache_bricks: Vec::new(),
            continuation_seeds: vec![10, 11, 12, 13, 14, 15],
        };
        let sliced = slice_gpu_dispatch_request(&request, 2..5).unwrap();
        let dispatch = match sliced.dispatch {
            KernelValue::Struct(ref dispatch) => dispatch,
            _ => panic!("expected struct dispatch config"),
        };
        let item_count = dispatch
            .fields
            .iter()
            .find(|(name, _)| name == "item_count")
            .and_then(|(_, value)| match value {
                KernelValue::U32(count) => Some(*count),
                _ => None,
            })
            .expect("item_count field");
        assert_eq!(item_count, 3);
        assert_eq!(sliced.items.len(), 3);
        assert_eq!(sliced.continuation_seeds, vec![12, 13, 14]);
        assert_eq!(sliced.world_shape_indices, vec![7, 8, 9]);
    }

    #[test]
    fn repeated_chunk_seed_metrics_are_suppressed_after_first_chunk() {
        let mut merged = QueryExecutionObservability {
            cache_resident_shared_snapshot_artifacts: 3,
            cache_resident_observer_local_artifacts: 2,
            cache_upload_attempts: 5,
            cache_upload_rejections: 1,
            dispatch_count: 1,
            dispatch_items: 4,
            ..QueryExecutionObservability::default()
        };
        let mut repeated_chunk = QueryExecutionObservability {
            cache_resident_shared_snapshot_artifacts: 3,
            cache_resident_observer_local_artifacts: 2,
            cache_upload_attempts: 5,
            cache_upload_rejections: 1,
            dispatch_count: 1,
            dispatch_items: 2,
            ..QueryExecutionObservability::default()
        };

        suppress_repeated_chunk_seed_metrics(&mut repeated_chunk);
        merged.merge_from(&repeated_chunk);

        assert_eq!(merged.cache_resident_shared_snapshot_artifacts, 3);
        assert_eq!(merged.cache_resident_observer_local_artifacts, 2);
        assert_eq!(merged.cache_upload_attempts, 5);
        assert_eq!(merged.cache_upload_rejections, 1);
        assert_eq!(merged.dispatch_count, 2);
        assert_eq!(merged.dispatch_items, 6);
    }

    #[test]
    fn chunked_direct_dispatch_preserves_results_and_merged_observability() {
        let structs = portable_abi_emit_wgsl_structs(&[
            wgsl_dispatch_config_abi(),
            wgsl_accel_node_abi(),
            wgsl_cache_brick_abi(),
            wgsl_shape_meta_abi(),
        ])
        .expect("wgsl structs");
        let source = format!(
            "{structs}

override WG_SIZE: u32 = 64u;

struct InputBuffer {{
  values: array<u32>,
}}

struct ResultBuffer {{
  values: array<u32>,
}}

struct ShapeIndexBuffer {{
  values: array<u32>,
}}

struct AccelNodeBuffer {{
  values: array<WgslAccelNode>,
}}

struct ShapeMetaBuffer {{
  values: array<WgslShapeMeta>,
}}

struct CacheBrickBuffer {{
  values: array<WgslCacheBrick>,
}}

struct ContinuationSeedBuffer {{
  values: array<u32>,
}}

struct WgslObservabilityBuffer {{
  acceleration_node_visits: atomic<u32>,
  shape_leaf_visits: atomic<u32>,
  acceleration_pruned_nodes: atomic<u32>,
  ray_support_interval_rejections: atomic<u32>,
  ray_support_entry_jumps: atomic<u32>,
  cache_brick_visits: atomic<u32>,
  cache_brick_hits: atomic<u32>,
  cache_brick_misses: atomic<u32>,
  cache_interval_advances: atomic<u32>,
  cache_resident_shared_snapshot_artifacts: atomic<u32>,
  cache_resident_observer_local_artifacts: atomic<u32>,
  cache_upload_attempts: atomic<u32>,
  cache_upload_rejections: atomic<u32>,
  cache_budget_rejections: atomic<u32>,
  cache_dense_fallback_rays: atomic<u32>,
  solver_analytic_hits: atomic<u32>,
  solver_generated_dense_fallback_rays: atomic<u32>,
  solver_support_rejections: atomic<u32>,
}}

@group(0) @binding(0)
var<storage, read> dispatch_config: WgslDispatchConfig;
@group(1) @binding(0)
var<storage, read> accel_nodes: AccelNodeBuffer;
@group(1) @binding(1)
var<storage, read> accel_children: ShapeIndexBuffer;
@group(1) @binding(2)
var<storage, read> shape_meta: ShapeMetaBuffer;
@group(1) @binding(3)
var<storage, read> cache_bricks: CacheBrickBuffer;
@group(2) @binding(0)
var<storage, read> input_items: InputBuffer;
@group(2) @binding(1)
var<storage, read_write> output_items: ResultBuffer;
@group(2) @binding(2)
var<storage, read> world_shapes: ShapeIndexBuffer;
@group(2) @binding(3)
var<storage, read_write> observability_metrics: WgslObservabilityBuffer;
@group(3) @binding(0)
var<storage, read> continuation_seeds: ContinuationSeedBuffer;

@compute @workgroup_size(WG_SIZE)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {{
  let index = global_id.x;
  _ = accel_nodes.values[0].flags;
  _ = accel_children.values[0];
  _ = shape_meta.values[0].root_shape_id;
  _ = cache_bricks.values[0].min.x;
  _ = world_shapes.values[0];
  _ = continuation_seeds.values[0];
  if (index >= dispatch_config.item_count) {{
    return;
  }}
  if (index == 0u) {{
    atomicAdd(&observability_metrics.cache_resident_shared_snapshot_artifacts, 3u);
    atomicAdd(&observability_metrics.cache_upload_attempts, 3u);
  }}
  output_items.values[index] = input_items.values[index];
}}
"
        );
        let generated = GeneratedShaderModule {
            source,
            workgroup_size: 64,
            dispatch_abi: wgsl_dispatch_config_abi(),
            accel_node_abi: wgsl_accel_node_abi(),
            cache_brick_abi: wgsl_cache_brick_abi(),
            shape_meta_abi: wgsl_shape_meta_abi(),
            item_abi: PortableAbiType::U32,
            result_abi: PortableAbiType::U32,
            shape_meta_values: vec![KernelValue::Struct(KernelStructValue {
                name: SmolStr::new("WgslShapeMeta"),
                fields: vec![
                    (SmolStr::new("root_shape_id"), KernelValue::U32(0)),
                    (SmolStr::new("analytic_kind"), KernelValue::U32(0)),
                ],
            })],
            layout_signature: 1,
            bind_group_count: 4,
        };
        let request = GpuDispatchRequest {
            dispatch: dispatch_config(0, 0, 6, 1, 0, 1, 1, false, false, false),
            items: (0..6).map(KernelValue::U32).collect(),
            world_shape_indices: vec![0],
            accel_nodes: vec![KernelValue::Struct(KernelStructValue {
                name: SmolStr::new("WgslAccelNode"),
                fields: vec![
                    (SmolStr::new("min"), KernelValue::Vec3([0.0, 0.0, 0.0])),
                    (SmolStr::new("max"), KernelValue::Vec3([1.0, 1.0, 1.0])),
                    (SmolStr::new("child_start"), KernelValue::U32(0)),
                    (SmolStr::new("child_len"), KernelValue::U32(0)),
                    (SmolStr::new("leaf_shape_index"), KernelValue::U32(0)),
                    (SmolStr::new("flags"), KernelValue::U32(0)),
                ],
            })],
            accel_children: vec![0],
            cache_bricks: vec![KernelValue::Struct(KernelStructValue {
                name: SmolStr::new("WgslCacheBrick"),
                fields: vec![
                    (SmolStr::new("min"), KernelValue::Vec3([0.0, 0.0, 0.0])),
                    (SmolStr::new("max"), KernelValue::Vec3([1.0, 1.0, 1.0])),
                ],
            })],
            continuation_seeds: vec![0; 6],
        };

        let (values, observability) = with_test_chunk_storage_buffer_limit_override(16, || {
            dispatch_compiled_shader_with_observability(&generated, request.clone())
                .expect("chunked direct dispatch")
        });

        assert_eq!(values, request.items);
        assert_eq!(observability.dispatch_count, 2);
        assert_eq!(observability.dispatch_items, 6);
        assert_eq!(observability.cache_resident_shared_snapshot_artifacts, 3);
        assert_eq!(observability.cache_upload_attempts, 3);
    }
}
