//! Owns the query-MIR lowering seam: stable ids, shared query/capture lowering
//! imports, and the bridge modules consumed by higher MIR lowering stages.
//! Does not own general MIR entrypoint construction or the public query
//! contract/catalog surface.
//!
//! Key invariants:
//! - this seam re-exports split helper modules without reintroducing a godfile;
//!   responsibility stays with the named leaf modules.
//! - stable capture ids and shared helper imports must stay aligned across query,
//!   capture, and WGSL lowering paths.
//! - scene semantics remain internal to the query-MIR seam so higher layers can
//!   depend on a narrower set of bridge entrypoints.
//!
//! Primary entrypoints:
//! - the split lowering modules declared below
//! - `stable_*_capture_id_i64`
//! - `executable_region_shapes`
//!
//! Failure modes / common pitfalls:
//! - stuffing new lowering logic directly into this seam root weakens the module
//!   split completed in Phases 52-53.
//! - drifting stable-id helpers or shared imports here can desynchronize sibling
//!   lowerers in ways that are hard to spot from call sites.

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::hir::{
    self, BinaryOp, Expr, FieldBounds, FieldSupport, FunctionRole, Literal, Stmt as HirStmt,
    UnaryOp,
};
use crate::kernel::{
    KernelPlanStage, lower_batch_query_plan, lower_capture_query_plan, lower_world_query_plan,
    validate_batch_query_plan, validate_capture_query_plan, validate_world_query_plan,
};
use crate::mir::ir::Stmt as MirStmt;
use crate::mir::ir::*;
use crate::mir::lower::{
    FunctionLowerer, ShapeExecutionMode, portable_abi_from_type_ref, portable_value_struct_abi,
    vector_component_index,
};
use crate::portable::{PortableAbiType, builtin_record_by_function};
use crate::query_contract::{self, QueryContractDescriptor, QuerySurfaceKind};
use crate::query_exec::ids::{
    stable_field_scene_capture_id as stable_field_scene_capture_id_u32,
    stable_region_scene_capture_id as stable_region_scene_capture_id_u32,
    stable_shape_capture_id as stable_shape_capture_id_u32,
    stable_shape_scene_capture_id as stable_shape_scene_capture_id_u32,
};
use crate::query_exec::region::{executable_region_shape_lists, world_domain_mismatch_message};
use crate::query_exec::spec::{
    BatchQueryExecutionState, BatchQueryInvocationSpec, BatchQueryLoopInputs,
    ScalarQueryInvocationSpec,
};
use crate::query_exec::wgsl::NativeWgslBridgeConfig;
use crate::query_exec::world::{
    WorldDistanceBackend, WorldMediumBackend, WorldNormalBackend, WorldRadianceBackend,
    WorldSurfaceBackend, WorldTraceBackend, execute_world_normal, walk_world_distance_shapes,
    walk_world_medium_shapes, walk_world_radiance_shapes, walk_world_surface_shapes,
    walk_world_trace_shapes, world_query_semantics,
};
use crate::query_plan::{
    BatchQueryKind, BatchQueryPlan, CandidateStrategy, CaptureKind, CaptureQueryKind,
    CaptureQueryPlan, DispatchBackend, InternalKernelKind, PlanExecutor, PruningStrategy,
    QueryItemKind, QueryResultKind, SceneSummary, WorldQueryKind, WorldQueryPlan,
    batch_query_kind_for_contract_id,
};
use crate::scene_ir;
use rowan::TextRange;
use smol_str::SmolStr;

pub fn stable_shape_capture_id_i64(shape_name: &SmolStr) -> i64 {
    i64::from(stable_shape_capture_id_u32(shape_name))
}

pub fn stable_shape_scene_capture_id_i64(shape_name: &SmolStr) -> i64 {
    i64::from(stable_shape_scene_capture_id_u32(shape_name))
}

pub fn stable_field_scene_capture_id_i64(field_name: &SmolStr) -> i64 {
    i64::from(stable_field_scene_capture_id_u32(field_name))
}

pub fn stable_region_scene_capture_id_i64(region_name: &SmolStr) -> i64 {
    i64::from(stable_region_scene_capture_id_u32(region_name))
}

pub fn executable_region_shapes(
    func: &hir::Function,
) -> Result<(Vec<SmolStr>, Vec<SmolStr>), &'static str> {
    executable_region_shape_lists(func)
}

fn stable_shape_capture_id(shape_name: &SmolStr) -> i64 {
    stable_shape_capture_id_i64(shape_name)
}

fn stable_shape_scene_capture_id(shape_name: &SmolStr) -> i64 {
    stable_shape_scene_capture_id_i64(shape_name)
}

fn stable_field_scene_capture_id(field_name: &SmolStr) -> i64 {
    stable_field_scene_capture_id_i64(field_name)
}

fn stable_region_scene_capture_id(region_name: &SmolStr) -> i64 {
    stable_region_scene_capture_id_i64(region_name)
}

mod batch_query_lowering;
mod query_methods;
mod scene_capture_lowering;
mod scene_medium_capture_lowering;
mod scene_semantics;
mod shape_helper_lowering;
mod support_summary_lowering;
mod world_capture_lowering;

pub(crate) use batch_query_lowering::{
    lower_field_batch_queries_helper, lower_scene_surface_queries_helper,
    lower_scene_trace_queries_helper, lower_shape_batch_queries_helper,
    lower_world_batch_queries_helper,
};
pub(crate) use scene_capture_lowering::{
    lower_scene_distance_capture_helper, lower_scene_normal_capture_helper,
    lower_scene_occluded_capture_helper, lower_scene_radiance_capture_helper,
    lower_scene_surface_capture_helper, lower_scene_trace_capture_helper,
};
pub(crate) use scene_medium_capture_lowering::lower_scene_medium_capture_helper;
pub(crate) use shape_helper_lowering::{
    lower_shape_distance_helper, lower_shape_surface_helper, lower_shape_trace_helper,
};
pub(crate) use support_summary_lowering::{
    lower_scene_support_summary_capture_helper, lower_world_support_summary_capture_helper,
};
pub(crate) use world_capture_lowering::{
    lower_world_distance_capture_helper, lower_world_medium_capture_helper,
    lower_world_normal_capture_helper, lower_world_occluded_capture_helper,
    lower_world_radiance_capture_helper, lower_world_surface_capture_helper,
    lower_world_trace_capture_helper,
};
