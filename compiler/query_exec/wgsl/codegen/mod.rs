//! Owns WGSL shader generation for query execution plus the ABI helpers the WGSL
//! runtime consumes.
//! Does not own query planning, shader dispatch/runtime execution, or CPU oracle
//! evaluation.
//!
//! Key invariants:
//! - generated shader bindings and ABI layouts must stay isomorphic to the
//!   portable/kernel model used by CPU execution.
//! - codegen may specialize for query shape or pruning policy, but it must not
//!   change the contract semantics that the planner selected.
//! - scene/shape emission helpers must agree on stable identity so readback and
//!   observability can be correlated across backends.
//!
//! Primary entrypoints:
//! - `generate_shader`
//! - shader ABI emission helpers in this module
//!
//! Failure modes / common pitfalls:
//! - string-building shader snippets without keeping ABI helpers in lockstep can
//!   create backend-only bugs that compile successfully.
//! - changing stable binding or identity rules here can invalidate runtime cache
//!   reuse and perf evidence.

use crate::gpu_runtime::{
    GPU_RUNTIME_FRAME_BIND_GROUP_INDEX, GPU_RUNTIME_PASS_BIND_GROUP_INDEX,
    GPU_RUNTIME_SCENE_BIND_GROUP_INDEX, GPU_RUNTIME_SCRATCH_BIND_GROUP_INDEX,
};
use crate::hir;
use crate::kernel::ir::{KernelBatchQueryPlan, KernelCaptureQueryPlan, KernelWorldQueryPlan};
use crate::kernel::{KernelStructValue, KernelValue};
use crate::mir::ir::TypeTagId;
use crate::mir::lower::portable_value_struct_abi;
use crate::pir;
use crate::portable::{
    PortableAbiType, PortableStructField, all_builtin_records, portable_abi_emit_wgsl_structs,
    portable_any_builtin_record_abi, portable_query_item_abi, portable_query_result_abi,
};
use crate::query_contract::{
    self, QueryCardinality, QueryContractDescriptor, QueryExecutionBinding, QueryResultKind,
};
use crate::query_exec::QueryExecContext;
use crate::query_exec::cpu::{DirectQueryOps, QueryExecError};
use crate::query_plan::{NormalizedQueryBehavior, NormalizedQueryValuePath, PruningStrategy};
use crate::query_solver::{RaySolverPlan, ray_solver_method_name};
use crate::scene_ir::{
    FieldNodeKindSummary, FieldNodeRecord, RepeatKind, SceneOperatorPayload, SceneProfileExpr,
    SceneValueExpr, ShapeMergeProvenancePolicy, ShapeNodeKindSummary, ShapeNodeProvenancePolicy,
    ShapeSubtractProvenancePolicy, SmoothKind, TransformKind,
};
use smol_str::SmolStr;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt::Write;

mod abi;
mod bindings;
mod literals;
mod pir_emit;
mod query_helpers;
mod scene_emit;
mod shape_emit;

use self::abi::{
    abi_type_name, build_shape_meta_values, emit_struct_conversions, emit_value_and_abi_structs,
    from_abi_expr, to_abi_expr,
};
use self::{bindings::*, literals::*, pir_emit::*, query_helpers::*, scene_emit::*, shape_emit::*};

pub(crate) use self::abi::{
    wgsl_accel_node_abi, wgsl_cache_brick_abi, wgsl_dispatch_config_abi,
    wgsl_item_abi_for_descriptor, wgsl_result_abi_for_descriptor, wgsl_shape_meta_abi,
};

const WORKGROUP_SIZE: u32 = 64;

#[derive(Debug, Clone)]
pub(crate) enum ShaderPlan<'a> {
    Capture(&'a KernelCaptureQueryPlan),
    World(&'a KernelWorldQueryPlan),
    Batch(&'a KernelBatchQueryPlan),
}

#[derive(Debug, Clone)]
pub(crate) struct GeneratedShader {
    pub(crate) source: String,
    pub(crate) workgroup_size: u32,
    pub(crate) dispatch_abi: PortableAbiType,
    pub(crate) accel_node_abi: PortableAbiType,
    pub(crate) cache_brick_abi: PortableAbiType,
    pub(crate) shape_meta_abi: PortableAbiType,
    pub(crate) item_abi: PortableAbiType,
    pub(crate) result_abi: PortableAbiType,
    pub(crate) shape_meta_values: Vec<KernelValue>,
}

#[derive(Debug, Clone)]
struct NormalizedShaderBehavior {
    cardinality: QueryCardinality,
    result_kind: QueryResultKind,
    requires_material: bool,
    requires_radiance: bool,
    requires_volume: bool,
    requires_trace: bool,
    requires_root_shape_lookup: bool,
    value_path: NormalizedQueryValuePath,
    ray_solver: Option<RaySolverPlan>,
    world_support_lower_bound_pruning: bool,
}

#[derive(Debug, Clone, Copy)]
struct CacheObservabilitySeed {
    resident_shared_snapshot_artifacts: u32,
    resident_observer_local_artifacts: u32,
    upload_attempts: u32,
    upload_rejections: u32,
}

impl NormalizedShaderBehavior {
    fn from_plan(plan: ShaderPlan<'_>) -> Result<Self, QueryExecError> {
        let (normalized_behavior, ray_solver, pruning_strategy) = match plan {
            ShaderPlan::Capture(plan) => (
                plan.normalized_behavior.clone(),
                None,
                PruningStrategy::None,
            ),
            ShaderPlan::World(plan) => (
                plan.normalized_behavior.clone(),
                plan.ray_solver.clone(),
                plan.pruning_strategy,
            ),
            ShaderPlan::Batch(plan) => (
                plan.normalized_behavior.clone(),
                plan.ray_solver.clone(),
                plan.pruning_strategy,
            ),
        };
        let NormalizedQueryBehavior {
            cardinality,
            result_kind,
            requires_material,
            requires_radiance,
            requires_volume,
            requires_trace,
            requires_root_shape_lookup,
            value_path,
            ..
        } = normalized_behavior;
        let world_support_lower_bound_pruning =
            matches!(pruning_strategy, PruningStrategy::SupportLowerBound);
        Ok(Self {
            cardinality,
            result_kind,
            requires_material,
            requires_radiance,
            requires_volume,
            requires_trace,
            requires_root_shape_lookup,
            value_path,
            ray_solver,
            world_support_lower_bound_pruning,
        })
    }

    fn requires_trace(&self) -> bool {
        self.requires_trace
    }

    fn requires_root_shape_lookup(&self) -> bool {
        self.requires_root_shape_lookup
    }

    fn scalar_eval_expr(&self, item_expr: &str) -> String {
        match self.value_path {
            NormalizedQueryValuePath::SupportSummary => {
                panic!(
                    "WGSL portable lowering does not support support.summary normalized behavior"
                )
            }
            NormalizedQueryValuePath::CaptureDistance => {
                format!("capture_distance_point({item_expr})")
            }
            NormalizedQueryValuePath::CaptureNormal => {
                format!("capture_normal_point({item_expr})")
            }
            NormalizedQueryValuePath::CaptureTrace => format!("capture_trace_ray({item_expr})"),
            NormalizedQueryValuePath::CaptureOcclusion => {
                format!("wr_occlusion_result_from_hit(capture_trace_ray({item_expr}))")
            }
            NormalizedQueryValuePath::CaptureSurface => format!("capture_surface_hit({item_expr})"),
            NormalizedQueryValuePath::CaptureRadiance => {
                format!("capture_radiance_query({item_expr})")
            }
            NormalizedQueryValuePath::CaptureMedium => format!("capture_medium_point({item_expr})"),
            NormalizedQueryValuePath::WorldDistance => {
                format!("world_distance_point({item_expr}.point)")
            }
            NormalizedQueryValuePath::WorldNormal => {
                format!("world_normal_point({item_expr}.point)")
            }
            NormalizedQueryValuePath::WorldTrace => format!("world_trace_ray({item_expr})"),
            NormalizedQueryValuePath::WorldOcclusion => {
                format!("wr_occlusion_result_from_hit(world_trace_ray({item_expr}))")
            }
            NormalizedQueryValuePath::WorldSurface => format!("world_surface_hit({item_expr})"),
            NormalizedQueryValuePath::WorldRadiance => format!("world_radiance_query({item_expr})"),
            NormalizedQueryValuePath::WorldMedium => format!("world_medium_point({item_expr})"),
        }
    }

    fn batch_eval_expr(&self, item_expr: &str) -> String {
        match self.result_kind {
            QueryResultKind::DistanceResult => {
                format!("DistanceResult({})", self.scalar_eval_expr(item_expr))
            }
            QueryResultKind::NormalResult => {
                format!("NormalResult({})", self.scalar_eval_expr(item_expr))
            }
            _ => self.scalar_eval_expr(item_expr),
        }
    }
}

#[derive(Debug, Clone)]
struct ShaderSceneIndex {
    field_indices: BTreeMap<SmolStr, u32>,
    shape_indices: BTreeMap<SmolStr, u32>,
}

impl ShaderSceneIndex {
    fn new(ctx: &QueryExecContext) -> Self {
        let field_indices = ctx
            .scene
            .fields
            .keys()
            .enumerate()
            .map(|(index, name)| (name.clone(), index as u32))
            .collect();
        let shape_indices = ctx
            .scene
            .shapes
            .keys()
            .enumerate()
            .map(|(index, name)| (name.clone(), index as u32))
            .collect();
        Self {
            field_indices,
            shape_indices,
        }
    }

    fn field(&self, name: &SmolStr) -> Result<u32, QueryExecError> {
        self.field_indices
            .get(name)
            .copied()
            .ok_or_else(|| QueryExecError::MissingField { name: name.clone() })
    }

    fn shape(&self, name: &SmolStr) -> Result<u32, QueryExecError> {
        self.shape_indices
            .get(name)
            .copied()
            .ok_or_else(|| QueryExecError::MissingShape { name: name.clone() })
    }
}

pub(crate) fn generate_shader(
    ctx: &QueryExecContext,
    plan: ShaderPlan<'_>,
) -> Result<GeneratedShader, QueryExecError> {
    let (descriptor, _binding) = shader_contract(plan.clone())?;
    let behavior = NormalizedShaderBehavior::from_plan(plan)?;

    let type_tags = build_type_tags(ctx);
    let scene_index = ShaderSceneIndex::new(ctx);
    let dispatch_abi = wgsl_dispatch_config_abi();
    let accel_node_abi = wgsl_accel_node_abi();
    let cache_brick_abi = wgsl_cache_brick_abi();
    let shape_meta_abi = wgsl_shape_meta_abi();
    let item_abi = wgsl_item_abi_for_descriptor(descriptor)?;
    let result_abi = wgsl_result_abi_for_descriptor(descriptor)?;
    let shape_meta_values = build_shape_meta_values(ctx, &behavior, &scene_index)?;
    let cache_seed = cache_observability_seed(ctx);
    let mut rendered = String::new();

    rendered.push_str("// Generated by wr query_exec::wgsl\n");
    rendered.push_str("const WR_ACCEL_NODE_FLAG_LEAF: u32 = 1u;\n");
    rendered.push_str("const WR_ACCEL_NODE_FLAG_HAS_BOUNDS: u32 = 2u;\n");
    if behavior.requires_trace() {
        rendered.push_str("// ray_solver: generated_dense_fallback\n");
        rendered.push_str("const WR_RAY_SOLVER_GENERATED_DENSE_FALLBACK: u32 = 1u;\n");
        let solver_support_enabled = behavior.ray_solver.as_ref().is_some_and(|solver| {
            solver.method_enabled(
                crate::query_solver::RaySolverMethod::SupportBoundCandidateRejection,
            )
        });
        let solver_analytic_enabled = behavior.ray_solver.as_ref().is_some_and(|solver| {
            solver
                .method_enabled(crate::query_solver::RaySolverMethod::AnalyticPrimitiveIntersection)
        });
        rendered.push_str(&format!(
            "const WR_SOLVER_ENABLE_SUPPORT: u32 = {}u;\n",
            u32::from(solver_support_enabled)
        ));
        rendered.push_str(&format!(
            "const WR_SOLVER_ENABLE_ANALYTIC: u32 = {}u;\n",
            u32::from(solver_analytic_enabled)
        ));
        rendered.push_str("const WR_SHAPE_ANALYTIC_NONE: u32 = 0u;\n");
        rendered.push_str("const WR_SHAPE_ANALYTIC_SPHERE: u32 = 1u;\n");
        rendered.push_str("const WR_SHAPE_ANALYTIC_PLANE: u32 = 2u;\n");
        rendered.push_str("const WR_SHAPE_ANALYTIC_BOX: u32 = 3u;\n");
        if let Some(solver) = &behavior.ray_solver {
            let methods = solver
                .diagnostic_summary()
                .methods
                .iter()
                .map(|method| ray_solver_method_name(*method))
                .collect::<Vec<_>>()
                .join("|");
            rendered.push_str(&format!("// ray_solver_methods={methods}\n"));
        }
    }
    rendered.push_str("override WG_SIZE: u32 = 64u;\n\n");
    rendered.push_str(&emit_value_and_abi_structs(
        ctx,
        &type_tags,
        &[
            dispatch_abi.clone(),
            accel_node_abi.clone(),
            cache_brick_abi.clone(),
            shape_meta_abi.clone(),
            item_abi.clone(),
            result_abi.clone(),
        ],
    )?);
    rendered.push('\n');
    rendered.push_str(WGSL_PRELUDE);
    rendered.push('\n');
    rendered.push_str(&emit_normal_sample_support()?);
    rendered.push('\n');
    rendered.push_str(&emit_struct_conversions(
        ctx,
        &type_tags,
        &[dispatch_abi.clone(), item_abi.clone(), result_abi.clone()],
    )?);
    rendered.push('\n');
    rendered.push_str(&emit_scene_functions(ctx, &scene_index, &behavior)?);
    rendered.push('\n');
    rendered.push_str(&emit_portable_functions(ctx, &behavior)?);
    rendered.push('\n');
    rendered.push_str(&emit_bindings(
        &dispatch_abi,
        &accel_node_abi,
        &cache_brick_abi,
        &shape_meta_abi,
        &item_abi,
        &result_abi,
    )?);
    rendered.push('\n');
    rendered.push_str(&emit_query_helpers(ctx, &scene_index, &behavior)?);
    rendered.push('\n');
    rendered.push_str(&emit_main(&behavior, &item_abi, &result_abi, cache_seed)?);

    Ok(GeneratedShader {
        source: rendered,
        workgroup_size: WORKGROUP_SIZE,
        dispatch_abi,
        accel_node_abi,
        cache_brick_abi,
        shape_meta_abi,
        item_abi,
        result_abi,
        shape_meta_values,
    })
}

fn cache_observability_seed(ctx: &QueryExecContext) -> CacheObservabilitySeed {
    let catalog = ctx.shared_acceleration.cache_catalog();
    let resident_shared_snapshot_artifacts = catalog
        .shape_support
        .values()
        .filter(|cache| cache.is_ready())
        .count()
        + catalog
            .shape_distance
            .values()
            .filter(|cache| cache.is_ready())
            .count()
        + catalog
            .world_support
            .values()
            .filter(|cache| cache.is_ready())
            .count()
        + catalog
            .world_distance
            .values()
            .filter(|cache| cache.is_ready())
            .count();
    CacheObservabilitySeed {
        resident_shared_snapshot_artifacts: resident_shared_snapshot_artifacts as u32,
        resident_observer_local_artifacts: 0,
        upload_attempts: resident_shared_snapshot_artifacts as u32,
        upload_rejections: 0,
    }
}

fn shader_contract(
    plan: ShaderPlan<'_>,
) -> Result<
    (
        &'static QueryContractDescriptor,
        &'static QueryExecutionBinding,
    ),
    QueryExecError,
> {
    let contract_id = match plan {
        ShaderPlan::Capture(plan) => plan.contract_id,
        ShaderPlan::World(plan) => plan.contract_id,
        ShaderPlan::Batch(plan) => plan.contract_id,
    };
    query_contract::query_contract_bundle(contract_id).ok_or_else(|| QueryExecError::Unsupported {
        message: format!("missing WGSL query contract '{}'", contract_id.as_str()),
    })
}

fn build_type_tags(ctx: &QueryExecContext) -> HashMap<SmolStr, TypeTagId> {
    let mut next = 1usize;
    let mut tags = HashMap::new();
    for record in all_builtin_records() {
        tags.insert(SmolStr::new(record.name), TypeTagId(next));
        next += 1;
    }
    for (_idx, class) in ctx.module.classes.iter() {
        if class.role == hir::ClassRole::Value {
            tags.insert(class.name.clone(), TypeTagId(next));
            next += 1;
        }
    }
    tags
}
