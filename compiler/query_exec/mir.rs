use std::collections::{BTreeMap, HashMap, HashSet};

use crate::hir::{
    self, BinaryOp, Expr, FieldBounds, FieldClass, FieldSupport, FunctionRole, Literal,
    Stmt as HirStmt, UnaryOp,
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
use crate::query_exec::ids::{
    stable_field_scene_capture_id as stable_field_scene_capture_id_u32,
    stable_region_scene_capture_id as stable_region_scene_capture_id_u32,
    stable_shape_capture_id as stable_shape_capture_id_u32,
    stable_shape_scene_capture_id as stable_shape_scene_capture_id_u32,
};
use crate::query_exec::region::{
    build_region_exec_cases, executable_region_shape_lists, world_domain_mismatch_message,
};
use crate::query_exec::spec::{
    BatchQueryExecutionState, BatchQueryLoopInputs, FieldBatchQueryKind, FieldBatchQuerySpec,
    FieldQueryKind, FieldQuerySpec, ShapeBatchQueryKind, ShapeBatchQuerySpec, ShapeQueryKind,
    ShapeQuerySpec, WorldPointQueryKind, WorldPointQuerySpec, WorldShapeQueryKind,
    WorldShapeQuerySpec,
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

include!("mir_query_methods.rs");
include!("mir_scene_semantics.rs");
include!("mir_helpers.rs");
