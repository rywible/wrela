use crate::hir::{AssignOp, BinaryOp, Literal, Type, UnaryOp};
use crate::kernel::ir::{
    KernelBatchQueryPlan, KernelBlock, KernelDispatchGrid, KernelDispatchSchedule, KernelExpr,
    KernelFunction, KernelPlanStage, KernelStmt, ResolvedKernelDispatch,
};
use crate::kernel::program::KernelProgram;
use crate::query_exec::{
    QueryExecError, execute_batch_query_on, execute_capture_query_on, execute_world_query_on,
};
use crate::query_plan::DispatchBackend;
use smol_str::SmolStr;
use std::collections::{BTreeMap, HashMap};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KernelInvocation {
    pub logical_index: u32,
    pub scheduled_index: u32,
    pub global_id: [u32; 3],
    pub local_id: [u32; 3],
    pub workgroup_id: [u32; 3],
    pub num_workgroups: [u32; 3],
    pub workgroup_size: [u32; 3],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KernelBatchIterationTrace {
    pub item_index: u32,
    pub stages: Vec<KernelPlanStage>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KernelBatchQueryTrace {
    pub helper_name: String,
    pub begins_virtual_gpu_dispatch: bool,
    pub iterations: Vec<KernelBatchIterationTrace>,
    pub ends_virtual_gpu_dispatch: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum KernelValue {
    Nothing,
    Bool(bool),
    I32(i32),
    U32(u32),
    F32(f32),
    Vec2([f32; 2]),
    Vec3([f32; 3]),
    Vec4([f32; 4]),
    Mat3([f32; 9]),
    Mat4([f32; 16]),
    Quat([f32; 4]),
    Array(Vec<KernelValue>),
    Struct(KernelStructValue),
    Capture(SmolStr),
    DispatchBackend(DispatchBackend),
    GpuBuffer(u32),
    GpuAtomicI32(u32),
    GpuAtomicU32(u32),
}

#[derive(Debug, Clone, PartialEq)]
pub struct KernelStructValue {
    pub name: SmolStr,
    pub fields: Vec<(SmolStr, KernelValue)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct KernelBufferValue {
    pub element_type: Type,
    pub elements: Vec<KernelValue>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct KernelRuntimeState {
    next_handle: u32,
    buffers: BTreeMap<u32, KernelBufferValue>,
    atomic_i32: BTreeMap<u32, i32>,
    atomic_u32: BTreeMap<u32, u32>,
}

impl KernelRuntimeState {
    pub fn create_buffer(
        &mut self,
        len: usize,
        default_value: KernelValue,
        element_type: Type,
    ) -> Result<u32, KernelExecError> {
        ensure_matches_type(&default_value, &element_type)?;
        let handle = self.next_handle();
        self.buffers.insert(
            handle,
            KernelBufferValue {
                element_type,
                elements: vec![default_value; len],
            },
        );
        Ok(handle)
    }

    pub fn create_atomic_i32(&mut self, initial: i32) -> u32 {
        let handle = self.next_handle();
        self.atomic_i32.insert(handle, initial);
        handle
    }

    pub fn create_atomic_u32(&mut self, initial: u32) -> u32 {
        let handle = self.next_handle();
        self.atomic_u32.insert(handle, initial);
        handle
    }

    pub fn buffer(&self, handle: u32) -> Option<&KernelBufferValue> {
        self.buffers.get(&handle)
    }

    pub fn atomic_i32_value(&self, handle: u32) -> Option<i32> {
        self.atomic_i32.get(&handle).copied()
    }

    pub fn atomic_u32_value(&self, handle: u32) -> Option<u32> {
        self.atomic_u32.get(&handle).copied()
    }

    fn next_handle(&mut self) -> u32 {
        let handle = self.next_handle.max(1);
        self.next_handle = handle.saturating_add(1);
        handle
    }
}

#[derive(Debug, Error, Clone, PartialEq)]
pub enum KernelExecError {
    #[error("entry '{name}' was not found in the kernel module")]
    MissingEntry { name: SmolStr },
    #[error("wrong arity for '{name}': expected {expected}, found {found}")]
    ArityMismatch {
        name: SmolStr,
        expected: usize,
        found: usize,
    },
    #[error("cannot assign to immutable local '{name}'")]
    ImmutableAssign { name: SmolStr },
    #[error("unknown local '{name}'")]
    UnknownLocal { name: SmolStr },
    #[error("unknown field '{field}' on '{base}'")]
    UnknownField { base: String, field: SmolStr },
    #[error("index out of bounds")]
    IndexOutOfBounds,
    #[error("missing GPU buffer handle {handle}")]
    MissingBuffer { handle: u32 },
    #[error("missing GPU atomic handle {handle}")]
    MissingAtomic { handle: u32 },
    #[error("type mismatch: expected {expected}, found {found}")]
    TypeMismatch { expected: String, found: String },
    #[error("unsupported operation: {message}")]
    UnsupportedOperation { message: String },
    #[error("kernel crash: {message}")]
    Crash { message: String },
}

pub fn execute_entry(
    program: &KernelProgram,
    args: Vec<KernelValue>,
    runtime: &mut KernelRuntimeState,
) -> Result<KernelValue, KernelExecError> {
    execute_entry_on(program, DispatchBackend::Cpu, args, runtime)
}

pub fn execute_entry_on(
    program: &KernelProgram,
    query_backend: DispatchBackend,
    args: Vec<KernelValue>,
    runtime: &mut KernelRuntimeState,
) -> Result<KernelValue, KernelExecError> {
    execute_function_on(program, query_backend, program.entry.as_str(), args, runtime)
}

pub fn execute_function(
    program: &KernelProgram,
    name: &str,
    args: Vec<KernelValue>,
    runtime: &mut KernelRuntimeState,
) -> Result<KernelValue, KernelExecError> {
    execute_function_on(program, DispatchBackend::Cpu, name, args, runtime)
}

pub fn execute_function_on(
    program: &KernelProgram,
    query_backend: DispatchBackend,
    name: &str,
    args: Vec<KernelValue>,
    runtime: &mut KernelRuntimeState,
) -> Result<KernelValue, KernelExecError> {
    let Some(function) = program.function(name) else {
        return Err(KernelExecError::MissingEntry {
            name: SmolStr::new(name),
        });
    };
    let mut executor = KernelExecutor {
        program,
        runtime,
        invocation: None,
        query_backend,
    };
    executor.execute_function(function, args)
}

pub fn execute_dispatch(
    program: &KernelProgram,
    dispatch: &ResolvedKernelDispatch,
    args: Vec<KernelValue>,
    runtime: &mut KernelRuntimeState,
) -> Result<Vec<KernelInvocation>, KernelExecError> {
    execute_dispatch_on(program, DispatchBackend::Cpu, dispatch, args, runtime)
}

pub fn execute_dispatch_on(
    program: &KernelProgram,
    query_backend: DispatchBackend,
    dispatch: &ResolvedKernelDispatch,
    args: Vec<KernelValue>,
    runtime: &mut KernelRuntimeState,
) -> Result<Vec<KernelInvocation>, KernelExecError> {
    let Some(function) = program.function(dispatch.kernel.as_str()) else {
        return Err(KernelExecError::MissingEntry {
            name: dispatch.kernel.clone(),
        });
    };
    if args.len() != dispatch.kernel_arg_count {
        return Err(KernelExecError::ArityMismatch {
            name: dispatch.kernel.clone(),
            expected: dispatch.kernel_arg_count,
            found: args.len(),
        });
    }
    let invocations = interpret_dispatch(dispatch);
    let mut executor = KernelExecutor {
        program,
        runtime,
        invocation: None,
        query_backend,
    };
    for invocation in &invocations {
        executor.invocation = Some(*invocation);
        let _ = executor.execute_function(function, args.clone())?;
    }
    Ok(invocations)
}

pub fn interpret_dispatch(dispatch: &ResolvedKernelDispatch) -> Vec<KernelInvocation> {
    let Some(total_count) = dispatch.grid.total_count() else {
        return Vec::new();
    };
    let total_size = dispatch.grid.total_size();
    let mut out = Vec::with_capacity(total_count);
    for logical_index in 0..total_count {
        let scheduled_index =
            scheduled_linear_index(dispatch.grid, dispatch.schedule, logical_index);
        let global_id = decode_linear_coords(total_size, scheduled_index);
        let workgroup_id = [
            safe_div(global_id[0], dispatch.grid.workgroup_size[0]),
            safe_div(global_id[1], dispatch.grid.workgroup_size[1]),
            safe_div(global_id[2], dispatch.grid.workgroup_size[2]),
        ];
        let local_id = [
            safe_mod(global_id[0], dispatch.grid.workgroup_size[0]),
            safe_mod(global_id[1], dispatch.grid.workgroup_size[1]),
            safe_mod(global_id[2], dispatch.grid.workgroup_size[2]),
        ];
        out.push(KernelInvocation {
            logical_index: logical_index as u32,
            scheduled_index: scheduled_index as u32,
            global_id,
            local_id,
            workgroup_id,
            num_workgroups: dispatch.grid.workgroups,
            workgroup_size: dispatch.grid.workgroup_size,
        });
    }
    out
}

pub fn interpret_batch_query(
    plan: &KernelBatchQueryPlan,
    item_count: u32,
) -> KernelBatchQueryTrace {
    let per_item_stages = plan
        .stages
        .iter()
        .filter(|stage| {
            !matches!(
                stage,
                KernelPlanStage::SelectBackend
                    | KernelPlanStage::BeginVirtualGpuDispatch
                    | KernelPlanStage::EndVirtualGpuDispatch
            )
        })
        .cloned()
        .collect::<Vec<_>>();
    let mut iterations = Vec::with_capacity(item_count as usize);
    for item_index in 0..item_count {
        iterations.push(KernelBatchIterationTrace {
            item_index,
            stages: per_item_stages.clone(),
        });
    }
    KernelBatchQueryTrace {
        helper_name: plan.helper_name.to_string(),
        begins_virtual_gpu_dispatch: plan.requires_virtual_gpu_dispatch(),
        iterations,
        ends_virtual_gpu_dispatch: plan.requires_virtual_gpu_dispatch(),
    }
}

fn scheduled_linear_index(
    grid: KernelDispatchGrid,
    schedule: KernelDispatchSchedule,
    logical_index: usize,
) -> usize {
    let Some(total_count) = grid.total_count() else {
        return logical_index;
    };
    match schedule {
        KernelDispatchSchedule::Deterministic => logical_index,
        KernelDispatchSchedule::Reverse => total_count.saturating_sub(logical_index + 1),
        KernelDispatchSchedule::Shuffle(seed) => build_shuffle_order(total_count, seed.into())
            .get(logical_index)
            .copied()
            .map(|value| value as usize)
            .unwrap_or(logical_index),
        KernelDispatchSchedule::WorkgroupReverse => {
            scheduled_workgroup_linear_index(grid, logical_index)
        }
        KernelDispatchSchedule::WorkgroupShuffle(seed) => build_workgroup_order(grid, seed, false)
            .get(logical_index)
            .copied()
            .map(|value| value as usize)
            .unwrap_or(logical_index),
        KernelDispatchSchedule::RoundRobinWorkgroups => build_workgroup_order(grid, 0, true)
            .get(logical_index)
            .copied()
            .map(|value| value as usize)
            .unwrap_or(logical_index),
    }
}

fn build_shuffle_order(total_count: usize, seed: u64) -> Vec<u32> {
    let limit = total_count.min(u32::MAX as usize);
    let mut order = (0..limit)
        .map(|value| u32::try_from(value).expect("shuffle order index"))
        .collect::<Vec<_>>();
    let mut state = seed.wrapping_add(0x9e37_79b9_7f4a_7c15);
    for idx in (1..order.len()).rev() {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let swap_idx = (state % (idx as u64 + 1)) as usize;
        order.swap(idx, swap_idx);
    }
    order
}

fn build_workgroup_order(grid: KernelDispatchGrid, seed: u32, round_robin: bool) -> Vec<u32> {
    let group_count = grid.workgroups.iter().try_fold(1usize, |acc, value| {
        acc.checked_mul(usize::try_from(*value).ok()?)
    });
    let Some(group_count) = group_count else {
        return Vec::new();
    };
    if group_count == 0 {
        return Vec::new();
    }
    let local_volume = grid.workgroup_size.iter().try_fold(1usize, |acc, value| {
        acc.checked_mul(usize::try_from(*value).ok()?)
    });
    let Some(local_volume) = local_volume else {
        return Vec::new();
    };
    if local_volume == 0 {
        return Vec::new();
    }

    let total_size = grid.total_size();
    let total_count = grid.total_count().unwrap_or(0);
    let mut groups = (0..group_count)
        .map(|value| u32::try_from(value).expect("workgroup order index"))
        .collect::<Vec<_>>();
    if seed != 0 {
        let mut state = seed as u64;
        for idx in (1..groups.len()).rev() {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let swap_idx = (state % (idx as u64 + 1)) as usize;
            groups.swap(idx, swap_idx);
        }
    }

    let mut order = Vec::with_capacity(total_count);
    if round_robin {
        for local_linear in 0..local_volume {
            for &group_linear in &groups {
                let global_linear = workgroup_local_to_global_linear(
                    grid.workgroups,
                    grid.workgroup_size,
                    total_size,
                    group_linear as usize,
                    local_linear,
                )
                .and_then(|value| u32::try_from(value).ok());
                if let Some(global_linear) = global_linear {
                    order.push(global_linear);
                }
            }
        }
    } else {
        for &group_linear in &groups {
            for local_linear in 0..local_volume {
                let global_linear = workgroup_local_to_global_linear(
                    grid.workgroups,
                    grid.workgroup_size,
                    total_size,
                    group_linear as usize,
                    local_linear,
                )
                .and_then(|value| u32::try_from(value).ok());
                if let Some(global_linear) = global_linear {
                    order.push(global_linear);
                }
            }
        }
    }
    order
}

fn scheduled_workgroup_linear_index(grid: KernelDispatchGrid, logical_index: usize) -> usize {
    let workgroup_volume = grid.workgroup_size.iter().try_fold(1usize, |acc, value| {
        acc.checked_mul(usize::try_from(*value).ok()?)
    });
    let Some(workgroup_volume) = workgroup_volume else {
        return logical_index;
    };
    let workgroup_count = grid.workgroups.iter().try_fold(1usize, |acc, value| {
        acc.checked_mul(usize::try_from(*value).ok()?)
    });
    let Some(workgroup_count) = workgroup_count else {
        return logical_index;
    };
    if workgroup_volume == 0 || workgroup_count == 0 {
        return logical_index;
    }

    let group_linear = logical_index / workgroup_volume;
    let local_linear = logical_index % workgroup_volume;
    let actual_group = workgroup_count.saturating_sub(group_linear + 1);
    workgroup_local_to_global_linear(
        grid.workgroups,
        grid.workgroup_size,
        grid.total_size(),
        actual_group,
        local_linear,
    )
    .unwrap_or(logical_index)
}

fn decode_linear_coords(extents: [u32; 3], linear_index: usize) -> [u32; 3] {
    let extent_x = usize::try_from(extents[0]).ok().unwrap_or(0);
    let extent_y = usize::try_from(extents[1]).ok().unwrap_or(0);
    if extent_x == 0 || extent_y == 0 {
        return [0, 0, 0];
    }
    let x = linear_index % extent_x;
    let yz_linear = linear_index / extent_x;
    let y = yz_linear % extent_y;
    let z = yz_linear / extent_y;
    [
        u32::try_from(x).ok().unwrap_or(0),
        u32::try_from(y).ok().unwrap_or(0),
        u32::try_from(z).ok().unwrap_or(0),
    ]
}

fn encode_linear_coords(extents: [u32; 3], coords: [u32; 3]) -> Option<usize> {
    if coords[0] >= extents[0] || coords[1] >= extents[1] || coords[2] >= extents[2] {
        return None;
    }
    let extent_x = usize::try_from(extents[0]).ok()?;
    let extent_y = usize::try_from(extents[1]).ok()?;
    let x = usize::try_from(coords[0]).ok()?;
    let y = usize::try_from(coords[1]).ok()?;
    let z = usize::try_from(coords[2]).ok()?;
    z.checked_mul(extent_y)?
        .checked_add(y)?
        .checked_mul(extent_x)?
        .checked_add(x)
}

fn workgroup_local_to_global_linear(
    num_workgroups: [u32; 3],
    workgroup_size: [u32; 3],
    total_size: [u32; 3],
    group_linear: usize,
    local_linear: usize,
) -> Option<usize> {
    let group_coords = decode_linear_coords(num_workgroups, group_linear);
    let local_coords = decode_linear_coords(workgroup_size, local_linear);
    let global_coords = [
        group_coords[0]
            .checked_mul(workgroup_size[0])?
            .checked_add(local_coords[0])?,
        group_coords[1]
            .checked_mul(workgroup_size[1])?
            .checked_add(local_coords[1])?,
        group_coords[2]
            .checked_mul(workgroup_size[2])?
            .checked_add(local_coords[2])?,
    ];
    encode_linear_coords(total_size, global_coords)
}

fn safe_div(value: u32, divisor: u32) -> u32 {
    if divisor == 0 { 0 } else { value / divisor }
}

fn safe_mod(value: u32, divisor: u32) -> u32 {
    if divisor == 0 { 0 } else { value % divisor }
}

#[derive(Debug, Clone)]
struct Variable {
    value: KernelValue,
    mutable: bool,
}

#[derive(Debug, Clone, PartialEq)]
enum ExecFlow {
    None,
    Return(KernelValue),
    Break,
    Continue,
}

struct KernelExecutor<'a> {
    program: &'a KernelProgram,
    runtime: &'a mut KernelRuntimeState,
    invocation: Option<KernelInvocation>,
    query_backend: DispatchBackend,
}

impl<'a> KernelExecutor<'a> {
    fn execute_function(
        &mut self,
        function: &KernelFunction,
        args: Vec<KernelValue>,
    ) -> Result<KernelValue, KernelExecError> {
        if args.len() != function.params.len() {
            return Err(KernelExecError::ArityMismatch {
                name: function.name.clone(),
                expected: function.params.len(),
                found: args.len(),
            });
        }

        let mut scopes = vec![HashMap::new()];
        for (param, value) in function.params.iter().zip(args) {
            ensure_matches_type(&value, &param.ty)?;
            scopes.last_mut().expect("kernel scope").insert(
                param.name.clone(),
                Variable {
                    value,
                    mutable: false,
                },
            );
        }

        match self.execute_block(&function.body, &mut scopes)? {
            ExecFlow::None => {
                ensure_matches_type(&KernelValue::Nothing, &function.ret)?;
                Ok(KernelValue::Nothing)
            }
            ExecFlow::Return(value) => {
                ensure_matches_type(&value, &function.ret)?;
                Ok(value)
            }
            ExecFlow::Break | ExecFlow::Continue => Err(KernelExecError::UnsupportedOperation {
                message: format!("loop control escaped function '{}'", function.name.as_str()),
            }),
        }
    }

    fn execute_block(
        &mut self,
        block: &KernelBlock,
        scopes: &mut Vec<HashMap<SmolStr, Variable>>,
    ) -> Result<ExecFlow, KernelExecError> {
        scopes.push(HashMap::new());
        for stmt in block {
            let flow = self.execute_stmt(stmt, scopes)?;
            if !matches!(flow, ExecFlow::None) {
                scopes.pop();
                return Ok(flow);
            }
        }
        scopes.pop();
        Ok(ExecFlow::None)
    }

    fn execute_stmt(
        &mut self,
        stmt: &KernelStmt,
        scopes: &mut Vec<HashMap<SmolStr, Variable>>,
    ) -> Result<ExecFlow, KernelExecError> {
        match stmt {
            KernelStmt::Let {
                name,
                mutable,
                ty,
                value,
                ..
            } => {
                let value = self.execute_expr(value, scopes)?;
                ensure_matches_type(&value, ty)?;
                scopes.last_mut().expect("kernel scope").insert(
                    name.clone(),
                    Variable {
                        value,
                        mutable: *mutable,
                    },
                );
                Ok(ExecFlow::None)
            }
            KernelStmt::Assign {
                name, op, value, ..
            } => {
                let value = self.execute_expr(value, scopes)?;
                self.assign_local(scopes, name, *op, value)?;
                Ok(ExecFlow::None)
            }
            KernelStmt::Expr { value, .. } | KernelStmt::IgnoreResult { value, .. } => {
                let _ = self.execute_expr(value, scopes)?;
                Ok(ExecFlow::None)
            }
            KernelStmt::If {
                condition,
                then_block,
                else_block,
                ..
            } => match self.execute_expr(condition, scopes)? {
                KernelValue::Bool(true) => self.execute_block(then_block, scopes),
                KernelValue::Bool(false) => self.execute_block(else_block, scopes),
                other => Err(KernelExecError::TypeMismatch {
                    expected: "Bool".to_string(),
                    found: value_label(&other),
                }),
            },
            KernelStmt::While {
                condition, body, ..
            } => {
                loop {
                    let condition = self.execute_expr(condition, scopes)?;
                    match condition {
                        KernelValue::Bool(true) => match self.execute_block(body, scopes)? {
                            ExecFlow::None | ExecFlow::Continue => continue,
                            ExecFlow::Break => break,
                            ExecFlow::Return(value) => return Ok(ExecFlow::Return(value)),
                        },
                        KernelValue::Bool(false) => break,
                        other => {
                            return Err(KernelExecError::TypeMismatch {
                                expected: "Bool".to_string(),
                                found: value_label(&other),
                            });
                        }
                    }
                }
                Ok(ExecFlow::None)
            }
            KernelStmt::Return { value, .. } => Ok(ExecFlow::Return(if let Some(value) = value {
                self.execute_expr(value, scopes)?
            } else {
                KernelValue::Nothing
            })),
            KernelStmt::Break { .. } => Ok(ExecFlow::Break),
            KernelStmt::Continue { .. } => Ok(ExecFlow::Continue),
        }
    }

    fn execute_expr(
        &mut self,
        expr: &KernelExpr,
        scopes: &mut Vec<HashMap<SmolStr, Variable>>,
    ) -> Result<KernelValue, KernelExecError> {
        match expr {
            KernelExpr::Literal { value, ty, .. } => literal_to_value(value, ty),
            KernelExpr::Var { name, .. } => Ok(self.lookup_local(scopes, name)?.value.clone()),
            KernelExpr::Unary { op, expr, .. } => {
                let value = self.execute_expr(expr, scopes)?;
                eval_unary(*op, value)
            }
            KernelExpr::Binary { op, lhs, rhs, .. } => {
                if matches!(op, BinaryOp::And | BinaryOp::Or) {
                    let lhs = self.execute_expr(lhs, scopes)?;
                    return match (*op, lhs.clone()) {
                        (BinaryOp::And, KernelValue::Bool(false)) => Ok(KernelValue::Bool(false)),
                        (BinaryOp::Or, KernelValue::Bool(true)) => Ok(KernelValue::Bool(true)),
                        (BinaryOp::And | BinaryOp::Or, KernelValue::Bool(_)) => {
                            let rhs = self.execute_expr(rhs, scopes)?;
                            eval_binary(*op, lhs, rhs)
                        }
                        (_, other) => Err(KernelExecError::TypeMismatch {
                            expected: "Bool".to_string(),
                            found: value_label(&other),
                        }),
                    };
                }
                let lhs = self.execute_expr(lhs, scopes)?;
                let rhs = self.execute_expr(rhs, scopes)?;
                eval_binary(*op, lhs, rhs)
            }
            KernelExpr::Crash { expr, .. } => {
                let message = self.execute_expr(expr, scopes)?;
                Err(KernelExecError::Crash {
                    message: value_label(&message),
                })
            }
            KernelExpr::Call {
                target, args, ty, ..
            } => {
                let args = args
                    .iter()
                    .map(|arg| self.execute_expr(arg, scopes))
                    .collect::<Result<Vec<_>, _>>()?;
                let value = if let Some(value) = self.execute_builtin_call(target, &args, ty)? {
                    value
                } else {
                    let Some(function) = self.program.function(target.as_str()) else {
                        return Err(KernelExecError::MissingEntry {
                            name: target.clone(),
                        });
                    };
                    self.execute_function(function, args)?
                };
                ensure_matches_type(&value, ty)?;
                Ok(value)
            }
            KernelExpr::Capture { target, .. } => Ok(KernelValue::Capture(target.clone())),
            KernelExpr::DispatchBackend { backend, .. } => {
                Ok(KernelValue::DispatchBackend(*backend))
            }
            KernelExpr::CaptureQuery { plan, args, .. } => {
                let args = args
                    .iter()
                    .map(|arg| self.execute_expr(arg, scopes))
                    .collect::<Result<Vec<_>, _>>()?;
                execute_capture_query_on(
                    &self.program.query_exec,
                    self.query_backend,
                    plan,
                    &args,
                )
                .map_err(query_error)
            }
            KernelExpr::WorldQuery { plan, args, .. } => {
                let args = args
                    .iter()
                    .map(|arg| self.execute_expr(arg, scopes))
                    .collect::<Result<Vec<_>, _>>()?;
                execute_world_query_on(&self.program.query_exec, self.query_backend, plan, &args)
                    .map_err(query_error)
            }
            KernelExpr::BatchQuery { plan, args, .. } => {
                let args = args
                    .iter()
                    .map(|arg| self.execute_expr(arg, scopes))
                    .collect::<Result<Vec<_>, _>>()?;
                let backend = match plan.backend {
                    DispatchBackend::Auto => self.query_backend,
                    explicit => explicit,
                };
                execute_batch_query_on(&self.program.query_exec, backend, plan, &args)
                    .map_err(query_error)
            }
            KernelExpr::Member { base, member, .. } => {
                let base = self.execute_expr(base, scopes)?;
                self.eval_member_value(base, member)
            }
            KernelExpr::Index { base, index, .. } => {
                let base = self.execute_expr(base, scopes)?;
                let index = self.execute_expr(index, scopes)?;
                eval_index(base, index)
            }
            KernelExpr::ArrayLiteral { items, .. } => Ok(KernelValue::Array(
                items
                    .iter()
                    .map(|item| self.execute_expr(item, scopes))
                    .collect::<Result<Vec<_>, _>>()?,
            )),
            KernelExpr::StructLiteral { name, fields, .. } => {
                Ok(KernelValue::Struct(KernelStructValue {
                    name: name.clone(),
                    fields: fields
                        .iter()
                        .map(|(field, expr)| Ok((field.clone(), self.execute_expr(expr, scopes)?)))
                        .collect::<Result<Vec<_>, KernelExecError>>()?,
                }))
            }
        }
    }

    fn execute_builtin_call(
        &mut self,
        target: &SmolStr,
        args: &[KernelValue],
        ty: &Type,
    ) -> Result<Option<KernelValue>, KernelExecError> {
        let value = match target.as_str() {
            "i32" => Some(KernelValue::I32(cast_to_i32(
                expect_arity(target, args, 1)?[0].clone(),
            )?)),
            "u32" => Some(KernelValue::U32(cast_to_u32(
                expect_arity(target, args, 1)?[0].clone(),
            )?)),
            "f32" => Some(KernelValue::F32(cast_to_f32(
                expect_arity(target, args, 1)?[0].clone(),
            )?)),
            "vec2" => {
                let args = expect_arity(target, args, 2)?;
                Some(KernelValue::Vec2([
                    cast_to_f32(args[0].clone())?,
                    cast_to_f32(args[1].clone())?,
                ]))
            }
            "vec3" => {
                let args = expect_arity(target, args, 3)?;
                Some(KernelValue::Vec3([
                    cast_to_f32(args[0].clone())?,
                    cast_to_f32(args[1].clone())?,
                    cast_to_f32(args[2].clone())?,
                ]))
            }
            "vec4" => {
                let args = expect_arity(target, args, 4)?;
                Some(KernelValue::Vec4([
                    cast_to_f32(args[0].clone())?,
                    cast_to_f32(args[1].clone())?,
                    cast_to_f32(args[2].clone())?,
                    cast_to_f32(args[3].clone())?,
                ]))
            }
            "quat" => {
                let args = expect_arity(target, args, 4)?;
                Some(KernelValue::Quat([
                    cast_to_f32(args[0].clone())?,
                    cast_to_f32(args[1].clone())?,
                    cast_to_f32(args[2].clone())?,
                    cast_to_f32(args[3].clone())?,
                ]))
            }
            "gpu_buffer_new" => {
                let args = expect_arity(target, args, 2)?;
                let len = value_to_len(&args[0])?;
                let handle = self.runtime.create_buffer(
                    len,
                    args[1].clone(),
                    infer_buffer_element_type(&args[1], ty)?,
                )?;
                Some(KernelValue::GpuBuffer(handle))
            }
            "gpu_buffer_len" => {
                let args = expect_arity(target, args, 1)?;
                let handle = expect_gpu_buffer(&args[0])?;
                let len = self
                    .runtime
                    .buffer(handle)
                    .ok_or(KernelExecError::MissingBuffer { handle })?
                    .elements
                    .len();
                Some(KernelValue::I32(i32::try_from(len).map_err(|_| {
                    KernelExecError::UnsupportedOperation {
                        message: "GPU buffer length overflowed i32".to_string(),
                    }
                })?))
            }
            "gpu_buffer_get" => {
                let args = expect_arity(target, args, 2)?;
                let handle = expect_gpu_buffer(&args[0])?;
                let index = value_to_len(&args[1])?;
                let buffer = self
                    .runtime
                    .buffer(handle)
                    .ok_or(KernelExecError::MissingBuffer { handle })?;
                let value = buffer
                    .elements
                    .get(index)
                    .cloned()
                    .ok_or(KernelExecError::IndexOutOfBounds)?;
                Some(value)
            }
            "gpu_buffer_set" => {
                let args = expect_arity(target, args, 3)?;
                let handle = expect_gpu_buffer(&args[0])?;
                let index = value_to_len(&args[1])?;
                let buffer = self
                    .runtime
                    .buffers
                    .get_mut(&handle)
                    .ok_or(KernelExecError::MissingBuffer { handle })?;
                let slot = buffer
                    .elements
                    .get_mut(index)
                    .ok_or(KernelExecError::IndexOutOfBounds)?;
                ensure_matches_type(&args[2], &buffer.element_type)?;
                *slot = args[2].clone();
                Some(KernelValue::Nothing)
            }
            "gpu_atomic_i32_new" => {
                let args = expect_arity(target, args, 1)?;
                Some(KernelValue::GpuAtomicI32(
                    self.runtime.create_atomic_i32(expect_i32(&args[0])?),
                ))
            }
            "gpu_atomic_i32_drop" => {
                let args = expect_arity(target, args, 1)?;
                let handle = expect_gpu_atomic_i32(&args[0])?;
                Some(KernelValue::Bool(
                    self.runtime.atomic_i32.remove(&handle).is_some(),
                ))
            }
            "gpu_atomic_i32_load" => {
                let args = expect_arity(target, args, 1)?;
                let handle = expect_gpu_atomic_i32(&args[0])?;
                Some(KernelValue::I32(
                    self.runtime
                        .atomic_i32_value(handle)
                        .ok_or(KernelExecError::MissingAtomic { handle })?,
                ))
            }
            "gpu_atomic_i32_store" => {
                let args = expect_arity(target, args, 2)?;
                let handle = expect_gpu_atomic_i32(&args[0])?;
                let value = expect_i32(&args[1])?;
                let atomic = self
                    .runtime
                    .atomic_i32
                    .get_mut(&handle)
                    .ok_or(KernelExecError::MissingAtomic { handle })?;
                *atomic = value;
                Some(KernelValue::Nothing)
            }
            "gpu_atomic_i32_fetch_add" => {
                let args = expect_arity(target, args, 2)?;
                let handle = expect_gpu_atomic_i32(&args[0])?;
                let delta = expect_i32(&args[1])?;
                let atomic = self
                    .runtime
                    .atomic_i32
                    .get_mut(&handle)
                    .ok_or(KernelExecError::MissingAtomic { handle })?;
                let previous = *atomic;
                *atomic = atomic.saturating_add(delta);
                Some(KernelValue::I32(previous))
            }
            "gpu_atomic_u32_new" => {
                let args = expect_arity(target, args, 1)?;
                Some(KernelValue::GpuAtomicU32(
                    self.runtime.create_atomic_u32(expect_u32(&args[0])?),
                ))
            }
            "gpu_atomic_u32_drop" => {
                let args = expect_arity(target, args, 1)?;
                let handle = expect_gpu_atomic_u32(&args[0])?;
                Some(KernelValue::Bool(
                    self.runtime.atomic_u32.remove(&handle).is_some(),
                ))
            }
            "gpu_atomic_u32_load" => {
                let args = expect_arity(target, args, 1)?;
                let handle = expect_gpu_atomic_u32(&args[0])?;
                Some(KernelValue::U32(
                    self.runtime
                        .atomic_u32_value(handle)
                        .ok_or(KernelExecError::MissingAtomic { handle })?,
                ))
            }
            "gpu_atomic_u32_store" => {
                let args = expect_arity(target, args, 2)?;
                let handle = expect_gpu_atomic_u32(&args[0])?;
                let value = expect_u32(&args[1])?;
                let atomic = self
                    .runtime
                    .atomic_u32
                    .get_mut(&handle)
                    .ok_or(KernelExecError::MissingAtomic { handle })?;
                *atomic = value;
                Some(KernelValue::Nothing)
            }
            "gpu_atomic_u32_fetch_add" => {
                let args = expect_arity(target, args, 2)?;
                let handle = expect_gpu_atomic_u32(&args[0])?;
                let delta = expect_u32(&args[1])?;
                let atomic = self
                    .runtime
                    .atomic_u32
                    .get_mut(&handle)
                    .ok_or(KernelExecError::MissingAtomic { handle })?;
                let previous = *atomic;
                *atomic = atomic.saturating_add(delta);
                Some(KernelValue::U32(previous))
            }
            "global_invocation_id" => Some(KernelValue::Array(invocation_array(
                self.invocation,
                |invocation| invocation.global_id,
            ))),
            "local_invocation_id" => Some(KernelValue::Array(invocation_array(
                self.invocation,
                |invocation| invocation.local_id,
            ))),
            "workgroup_id" => Some(KernelValue::Array(invocation_array(
                self.invocation,
                |invocation| invocation.workgroup_id,
            ))),
            "num_workgroups" => Some(KernelValue::Array(invocation_array(
                self.invocation,
                |invocation| invocation.num_workgroups,
            ))),
            "workgroup_size" => Some(KernelValue::Array(invocation_array(
                self.invocation,
                |invocation| invocation.workgroup_size,
            ))),
            _ => None,
        };
        Ok(value)
    }

    fn eval_member_value(
        &self,
        base: KernelValue,
        member: &SmolStr,
    ) -> Result<KernelValue, KernelExecError> {
        match base {
            KernelValue::Struct(struct_value) => struct_value
                .fields
                .iter()
                .find(|(field, _)| field == member)
                .map(|(_, value)| value.clone())
                .ok_or_else(|| KernelExecError::UnknownField {
                    base: struct_value.name.to_string(),
                    field: member.clone(),
                }),
            KernelValue::Vec2(value) => vector_member("Vec2", &value.map(KernelValue::F32), member),
            KernelValue::Vec3(value) => vector_member("Vec3", &value.map(KernelValue::F32), member),
            KernelValue::Vec4(value) | KernelValue::Quat(value) => {
                vector_member("Vec4", &value.map(KernelValue::F32), member)
            }
            KernelValue::Capture(name) => self.capture_member_value(&name, member),
            other => Err(KernelExecError::UnknownField {
                base: value_label(&other),
                field: member.clone(),
            }),
        }
    }

    fn capture_member_value(
        &self,
        name: &SmolStr,
        member: &SmolStr,
    ) -> Result<KernelValue, KernelExecError> {
        match member.as_str() {
            "scene_id" => {
                let scene_id = if self.program.query_exec.field_names.contains(name) {
                    self.program.query_exec.field_scene_id(name)
                } else if self.program.query_exec.shape_names.contains(name) {
                    self.program.query_exec.shape_scene_id(name)
                } else if self.program.query_exec.regions_by_name.contains_key(name) {
                    self.program.query_exec.region_scene_id(name)
                } else {
                    0
                };
                Ok(KernelValue::U32(scene_id))
            }
            "epoch" => Ok(KernelValue::U32(0)),
            "root_feature_id" => {
                let feature_id = if self.program.query_exec.shape_names.contains(name) {
                    self.program.query_exec.shape_root_feature_id(name)
                } else {
                    0
                };
                Ok(KernelValue::U32(feature_id))
            }
            _ => Err(KernelExecError::UnknownField {
                base: format!("Capture({name})"),
                field: member.clone(),
            }),
        }
    }

    fn lookup_local<'b>(
        &self,
        scopes: &'b [HashMap<SmolStr, Variable>],
        name: &SmolStr,
    ) -> Result<&'b Variable, KernelExecError> {
        scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name))
            .ok_or_else(|| KernelExecError::UnknownLocal { name: name.clone() })
    }

    fn assign_local(
        &self,
        scopes: &mut [HashMap<SmolStr, Variable>],
        name: &SmolStr,
        op: AssignOp,
        value: KernelValue,
    ) -> Result<(), KernelExecError> {
        for scope in scopes.iter_mut().rev() {
            if let Some(variable) = scope.get_mut(name) {
                if !variable.mutable {
                    return Err(KernelExecError::ImmutableAssign { name: name.clone() });
                }
                variable.value = match op {
                    AssignOp::Assign => value,
                    AssignOp::AddAssign => {
                        eval_binary(BinaryOp::Add, variable.value.clone(), value)?
                    }
                    AssignOp::SubAssign => {
                        eval_binary(BinaryOp::Sub, variable.value.clone(), value)?
                    }
                    AssignOp::MulAssign => {
                        eval_binary(BinaryOp::Mul, variable.value.clone(), value)?
                    }
                    AssignOp::DivAssign => {
                        eval_binary(BinaryOp::Div, variable.value.clone(), value)?
                    }
                };
                return Ok(());
            }
        }
        Err(KernelExecError::UnknownLocal { name: name.clone() })
    }
}

fn literal_to_value(value: &Literal, ty: &Type) -> Result<KernelValue, KernelExecError> {
    match (value, ty) {
        (Literal::Boolean(value), _) => Ok(KernelValue::Bool(*value)),
        (
            Literal::Float(value),
            Type::Float | Type::F32 | Type::Number | Type::Unknown | Type::Never,
        ) => Ok(KernelValue::F32(*value as f32)),
        (Literal::Integer(value), Type::U32) => {
            Ok(KernelValue::U32(u32::try_from(*value).map_err(|_| {
                KernelExecError::TypeMismatch {
                    expected: "U32".to_string(),
                    found: value.to_string(),
                }
            })?))
        }
        (Literal::Integer(value), _) => {
            Ok(KernelValue::I32(i32::try_from(*value).map_err(|_| {
                KernelExecError::TypeMismatch {
                    expected: "I32".to_string(),
                    found: value.to_string(),
                }
            })?))
        }
        (Literal::Nil, _) => Ok(KernelValue::Nothing),
        (Literal::String(value), _) => Err(KernelExecError::UnsupportedOperation {
            message: format!("string literal '{value}' is not supported in portable compute"),
        }),
        (Literal::Float(value), _) => Ok(KernelValue::F32(*value as f32)),
    }
}

fn eval_unary(op: UnaryOp, value: KernelValue) -> Result<KernelValue, KernelExecError> {
    match (op, value) {
        (UnaryOp::Neg, KernelValue::I32(value)) => Ok(KernelValue::I32(value.saturating_neg())),
        (UnaryOp::Neg, KernelValue::F32(value)) => Ok(KernelValue::F32(-value)),
        (UnaryOp::Not, KernelValue::Bool(value)) => Ok(KernelValue::Bool(!value)),
        (op, value) => Err(KernelExecError::UnsupportedOperation {
            message: format!("unary {op:?} does not support {}", value_label(&value)),
        }),
    }
}

fn eval_binary(
    op: BinaryOp,
    lhs: KernelValue,
    rhs: KernelValue,
) -> Result<KernelValue, KernelExecError> {
    match (op, lhs, rhs) {
        (BinaryOp::Eq, lhs, rhs) => Ok(KernelValue::Bool(lhs == rhs)),
        (BinaryOp::Ne, lhs, rhs) => Ok(KernelValue::Bool(lhs != rhs)),
        (BinaryOp::And, KernelValue::Bool(lhs), KernelValue::Bool(rhs)) => {
            Ok(KernelValue::Bool(lhs && rhs))
        }
        (BinaryOp::Or, KernelValue::Bool(lhs), KernelValue::Bool(rhs)) => {
            Ok(KernelValue::Bool(lhs || rhs))
        }
        (BinaryOp::Add, KernelValue::I32(lhs), KernelValue::I32(rhs)) => {
            Ok(KernelValue::I32(lhs.saturating_add(rhs)))
        }
        (BinaryOp::Sub, KernelValue::I32(lhs), KernelValue::I32(rhs)) => {
            Ok(KernelValue::I32(lhs.saturating_sub(rhs)))
        }
        (BinaryOp::Mul, KernelValue::I32(lhs), KernelValue::I32(rhs)) => {
            Ok(KernelValue::I32(lhs.saturating_mul(rhs)))
        }
        (BinaryOp::Div, KernelValue::I32(lhs), KernelValue::I32(rhs)) => {
            Ok(KernelValue::I32(lhs.checked_div(rhs).unwrap_or(0)))
        }
        (BinaryOp::Mod, KernelValue::I32(lhs), KernelValue::I32(rhs)) => {
            Ok(KernelValue::I32(lhs.checked_rem(rhs).unwrap_or(0)))
        }
        (BinaryOp::Lt, KernelValue::I32(lhs), KernelValue::I32(rhs)) => {
            Ok(KernelValue::Bool(lhs < rhs))
        }
        (BinaryOp::Gt, KernelValue::I32(lhs), KernelValue::I32(rhs)) => {
            Ok(KernelValue::Bool(lhs > rhs))
        }
        (BinaryOp::Le, KernelValue::I32(lhs), KernelValue::I32(rhs)) => {
            Ok(KernelValue::Bool(lhs <= rhs))
        }
        (BinaryOp::Ge, KernelValue::I32(lhs), KernelValue::I32(rhs)) => {
            Ok(KernelValue::Bool(lhs >= rhs))
        }
        (BinaryOp::BitAnd, KernelValue::I32(lhs), KernelValue::I32(rhs)) => {
            Ok(KernelValue::I32(lhs & rhs))
        }
        (BinaryOp::BitOr, KernelValue::I32(lhs), KernelValue::I32(rhs)) => {
            Ok(KernelValue::I32(lhs | rhs))
        }
        (BinaryOp::BitXor, KernelValue::I32(lhs), KernelValue::I32(rhs)) => {
            Ok(KernelValue::I32(lhs ^ rhs))
        }
        (BinaryOp::Add, KernelValue::U32(lhs), KernelValue::U32(rhs)) => {
            Ok(KernelValue::U32(lhs.saturating_add(rhs)))
        }
        (BinaryOp::Sub, KernelValue::U32(lhs), KernelValue::U32(rhs)) => {
            Ok(KernelValue::U32(lhs.saturating_sub(rhs)))
        }
        (BinaryOp::Mul, KernelValue::U32(lhs), KernelValue::U32(rhs)) => {
            Ok(KernelValue::U32(lhs.saturating_mul(rhs)))
        }
        (BinaryOp::Div, KernelValue::U32(lhs), KernelValue::U32(rhs)) => {
            Ok(KernelValue::U32(if rhs == 0 { 0 } else { lhs / rhs }))
        }
        (BinaryOp::Mod, KernelValue::U32(lhs), KernelValue::U32(rhs)) => {
            Ok(KernelValue::U32(if rhs == 0 { 0 } else { lhs % rhs }))
        }
        (BinaryOp::Lt, KernelValue::U32(lhs), KernelValue::U32(rhs)) => {
            Ok(KernelValue::Bool(lhs < rhs))
        }
        (BinaryOp::Gt, KernelValue::U32(lhs), KernelValue::U32(rhs)) => {
            Ok(KernelValue::Bool(lhs > rhs))
        }
        (BinaryOp::Le, KernelValue::U32(lhs), KernelValue::U32(rhs)) => {
            Ok(KernelValue::Bool(lhs <= rhs))
        }
        (BinaryOp::Ge, KernelValue::U32(lhs), KernelValue::U32(rhs)) => {
            Ok(KernelValue::Bool(lhs >= rhs))
        }
        (BinaryOp::BitAnd, KernelValue::U32(lhs), KernelValue::U32(rhs)) => {
            Ok(KernelValue::U32(lhs & rhs))
        }
        (BinaryOp::BitOr, KernelValue::U32(lhs), KernelValue::U32(rhs)) => {
            Ok(KernelValue::U32(lhs | rhs))
        }
        (BinaryOp::BitXor, KernelValue::U32(lhs), KernelValue::U32(rhs)) => {
            Ok(KernelValue::U32(lhs ^ rhs))
        }
        (BinaryOp::Add, KernelValue::F32(lhs), KernelValue::F32(rhs)) => {
            Ok(KernelValue::F32(lhs + rhs))
        }
        (BinaryOp::Sub, KernelValue::F32(lhs), KernelValue::F32(rhs)) => {
            Ok(KernelValue::F32(lhs - rhs))
        }
        (BinaryOp::Mul, KernelValue::F32(lhs), KernelValue::F32(rhs)) => {
            Ok(KernelValue::F32(lhs * rhs))
        }
        (BinaryOp::Div, KernelValue::F32(lhs), KernelValue::F32(rhs)) => {
            Ok(KernelValue::F32(lhs / rhs))
        }
        (BinaryOp::Lt, KernelValue::F32(lhs), KernelValue::F32(rhs)) => {
            Ok(KernelValue::Bool(lhs < rhs))
        }
        (BinaryOp::Gt, KernelValue::F32(lhs), KernelValue::F32(rhs)) => {
            Ok(KernelValue::Bool(lhs > rhs))
        }
        (BinaryOp::Le, KernelValue::F32(lhs), KernelValue::F32(rhs)) => {
            Ok(KernelValue::Bool(lhs <= rhs))
        }
        (BinaryOp::Ge, KernelValue::F32(lhs), KernelValue::F32(rhs)) => {
            Ok(KernelValue::Bool(lhs >= rhs))
        }
        (op, lhs, rhs) => Err(KernelExecError::UnsupportedOperation {
            message: format!(
                "binary {op:?} does not support {} and {}",
                value_label(&lhs),
                value_label(&rhs)
            ),
        }),
    }
}

fn vector_member(
    base: &str,
    values: &[KernelValue],
    member: &SmolStr,
) -> Result<KernelValue, KernelExecError> {
    let index = match member.as_str() {
        "x" => Some(0),
        "y" => Some(1),
        "z" => Some(2),
        "w" => Some(3),
        _ => None,
    }
    .ok_or_else(|| KernelExecError::UnknownField {
        base: base.to_string(),
        field: member.clone(),
    })?;
    values
        .get(index)
        .cloned()
        .ok_or(KernelExecError::IndexOutOfBounds)
}

fn eval_index(base: KernelValue, index: KernelValue) -> Result<KernelValue, KernelExecError> {
    let index = value_to_len(&index)?;
    match base {
        KernelValue::Array(items) => items
            .get(index)
            .cloned()
            .ok_or(KernelExecError::IndexOutOfBounds),
        KernelValue::Vec2(value) => value
            .get(index)
            .copied()
            .map(KernelValue::F32)
            .ok_or(KernelExecError::IndexOutOfBounds),
        KernelValue::Vec3(value) => value
            .get(index)
            .copied()
            .map(KernelValue::F32)
            .ok_or(KernelExecError::IndexOutOfBounds),
        KernelValue::Vec4(value) | KernelValue::Quat(value) => value
            .get(index)
            .copied()
            .map(KernelValue::F32)
            .ok_or(KernelExecError::IndexOutOfBounds),
        other => Err(KernelExecError::UnsupportedOperation {
            message: format!("cannot index {}", value_label(&other)),
        }),
    }
}

fn ensure_matches_type(value: &KernelValue, ty: &Type) -> Result<(), KernelExecError> {
    let matches = match ty {
        Type::Unknown | Type::Never => true,
        Type::Nil => matches!(value, KernelValue::Nothing),
        Type::Boolean => matches!(value, KernelValue::Bool(_)),
        Type::Integer | Type::I32 => matches!(value, KernelValue::I32(_)),
        Type::U32 => matches!(value, KernelValue::U32(_)),
        Type::Float | Type::F32 | Type::Number => matches!(value, KernelValue::F32(_)),
        Type::Vec2 => matches!(value, KernelValue::Vec2(_)),
        Type::Vec3 => matches!(value, KernelValue::Vec3(_)),
        Type::Vec4 => matches!(value, KernelValue::Vec4(_)),
        Type::Mat3 => matches!(value, KernelValue::Mat3(_)),
        Type::Mat4 => matches!(value, KernelValue::Mat4(_)),
        Type::Quat => matches!(value, KernelValue::Quat(_)),
        Type::Array(inner, len) => match value {
            KernelValue::Array(items) => {
                items.len() == *len
                    && items
                        .iter()
                        .all(|item| ensure_matches_type(item, inner).is_ok())
            }
            _ => false,
        },
        Type::List(inner) => match value {
            KernelValue::Array(items) => items
                .iter()
                .all(|item| ensure_matches_type(item, inner).is_ok()),
            _ => false,
        },
        Type::Named(name, _) => match value {
            KernelValue::Struct(struct_value) => struct_value.name == *name,
            KernelValue::Capture(_) => matches!(
                name.as_str(),
                "FieldCapture" | "ShapeCapture" | "RegionCapture"
            ),
            KernelValue::DispatchBackend(_) => name.as_str() == "DispatchBackend",
            _ => false,
        },
        Type::GpuBuffer(_) => matches!(value, KernelValue::GpuBuffer(_)),
        Type::GpuAtomicI32 => matches!(value, KernelValue::GpuAtomicI32(_)),
        Type::GpuAtomicU32 => matches!(value, KernelValue::GpuAtomicU32(_)),
        _ => false,
    };
    if matches {
        Ok(())
    } else {
        Err(KernelExecError::TypeMismatch {
            expected: format!("{ty:?}"),
            found: value_label(value),
        })
    }
}

fn infer_buffer_element_type(value: &KernelValue, ty: &Type) -> Result<Type, KernelExecError> {
    if let Type::GpuBuffer(inner) = ty {
        return Ok((**inner).clone());
    }
    match value {
        KernelValue::Bool(_) => Ok(Type::Boolean),
        KernelValue::I32(_) => Ok(Type::I32),
        KernelValue::U32(_) => Ok(Type::U32),
        KernelValue::F32(_) => Ok(Type::F32),
        KernelValue::Vec2(_) => Ok(Type::Vec2),
        KernelValue::Vec3(_) => Ok(Type::Vec3),
        KernelValue::Vec4(_) => Ok(Type::Vec4),
        KernelValue::Mat3(_) => Ok(Type::Mat3),
        KernelValue::Mat4(_) => Ok(Type::Mat4),
        KernelValue::Quat(_) => Ok(Type::Quat),
        KernelValue::Struct(struct_value) => Ok(Type::Named(struct_value.name.clone(), Vec::new())),
        other => Err(KernelExecError::UnsupportedOperation {
            message: format!(
                "cannot infer GPU buffer element type from {}",
                value_label(other)
            ),
        }),
    }
}

fn query_error(error: QueryExecError) -> KernelExecError {
    KernelExecError::UnsupportedOperation {
        message: error.to_string(),
    }
}

fn value_to_len(value: &KernelValue) -> Result<usize, KernelExecError> {
    match value {
        KernelValue::I32(value) => {
            usize::try_from(*value).map_err(|_| KernelExecError::IndexOutOfBounds)
        }
        KernelValue::U32(value) => {
            usize::try_from(*value).map_err(|_| KernelExecError::IndexOutOfBounds)
        }
        other => Err(KernelExecError::TypeMismatch {
            expected: "I32/U32".to_string(),
            found: value_label(other),
        }),
    }
}

fn cast_to_i32(value: KernelValue) -> Result<i32, KernelExecError> {
    match value {
        KernelValue::I32(value) => Ok(value),
        KernelValue::U32(value) => {
            i32::try_from(value).map_err(|_| KernelExecError::UnsupportedOperation {
                message: "u32 to i32 cast overflowed".to_string(),
            })
        }
        KernelValue::F32(value) => Ok(value as i32),
        other => Err(KernelExecError::TypeMismatch {
            expected: "I32/U32/F32".to_string(),
            found: value_label(&other),
        }),
    }
}

fn cast_to_u32(value: KernelValue) -> Result<u32, KernelExecError> {
    match value {
        KernelValue::I32(value) => {
            u32::try_from(value).map_err(|_| KernelExecError::UnsupportedOperation {
                message: "i32 to u32 cast overflowed".to_string(),
            })
        }
        KernelValue::U32(value) => Ok(value),
        KernelValue::F32(value) => Ok(value.max(0.0) as u32),
        other => Err(KernelExecError::TypeMismatch {
            expected: "I32/U32/F32".to_string(),
            found: value_label(&other),
        }),
    }
}

fn cast_to_f32(value: KernelValue) -> Result<f32, KernelExecError> {
    match value {
        KernelValue::I32(value) => Ok(value as f32),
        KernelValue::U32(value) => Ok(value as f32),
        KernelValue::F32(value) => Ok(value),
        other => Err(KernelExecError::TypeMismatch {
            expected: "I32/U32/F32".to_string(),
            found: value_label(&other),
        }),
    }
}

fn expect_i32(value: &KernelValue) -> Result<i32, KernelExecError> {
    match value {
        KernelValue::I32(value) => Ok(*value),
        other => Err(KernelExecError::TypeMismatch {
            expected: "I32".to_string(),
            found: value_label(other),
        }),
    }
}

fn expect_u32(value: &KernelValue) -> Result<u32, KernelExecError> {
    match value {
        KernelValue::U32(value) => Ok(*value),
        other => Err(KernelExecError::TypeMismatch {
            expected: "U32".to_string(),
            found: value_label(other),
        }),
    }
}

fn expect_gpu_buffer(value: &KernelValue) -> Result<u32, KernelExecError> {
    match value {
        KernelValue::GpuBuffer(handle) => Ok(*handle),
        other => Err(KernelExecError::TypeMismatch {
            expected: "GpuBuffer".to_string(),
            found: value_label(other),
        }),
    }
}

fn expect_gpu_atomic_i32(value: &KernelValue) -> Result<u32, KernelExecError> {
    match value {
        KernelValue::GpuAtomicI32(handle) => Ok(*handle),
        other => Err(KernelExecError::TypeMismatch {
            expected: "GpuAtomicI32".to_string(),
            found: value_label(other),
        }),
    }
}

fn expect_gpu_atomic_u32(value: &KernelValue) -> Result<u32, KernelExecError> {
    match value {
        KernelValue::GpuAtomicU32(handle) => Ok(*handle),
        other => Err(KernelExecError::TypeMismatch {
            expected: "GpuAtomicU32".to_string(),
            found: value_label(other),
        }),
    }
}

fn expect_arity<'a>(
    target: &SmolStr,
    args: &'a [KernelValue],
    expected: usize,
) -> Result<&'a [KernelValue], KernelExecError> {
    if args.len() == expected {
        Ok(args)
    } else {
        Err(KernelExecError::ArityMismatch {
            name: target.clone(),
            expected,
            found: args.len(),
        })
    }
}

fn invocation_array(
    invocation: Option<KernelInvocation>,
    projector: impl FnOnce(KernelInvocation) -> [u32; 3],
) -> Vec<KernelValue> {
    let values = projector(invocation.unwrap_or(KernelInvocation {
        logical_index: 0,
        scheduled_index: 0,
        global_id: [0, 0, 0],
        local_id: [0, 0, 0],
        workgroup_id: [0, 0, 0],
        num_workgroups: [0, 0, 0],
        workgroup_size: [0, 0, 0],
    }));
    values.into_iter().map(KernelValue::U32).collect()
}

fn value_label(value: &KernelValue) -> String {
    match value {
        KernelValue::Nothing => "Nothing".to_string(),
        KernelValue::Bool(_) => "Bool".to_string(),
        KernelValue::I32(_) => "I32".to_string(),
        KernelValue::U32(_) => "U32".to_string(),
        KernelValue::F32(_) => "F32".to_string(),
        KernelValue::Vec2(_) => "Vec2".to_string(),
        KernelValue::Vec3(_) => "Vec3".to_string(),
        KernelValue::Vec4(_) => "Vec4".to_string(),
        KernelValue::Mat3(_) => "Mat3".to_string(),
        KernelValue::Mat4(_) => "Mat4".to_string(),
        KernelValue::Quat(_) => "Quat".to_string(),
        KernelValue::Array(_) => "Array".to_string(),
        KernelValue::Struct(struct_value) => struct_value.name.to_string(),
        KernelValue::Capture(name) => format!("Capture({name})"),
        KernelValue::DispatchBackend(_) => "DispatchBackend".to_string(),
        KernelValue::GpuBuffer(_) => "GpuBuffer".to_string(),
        KernelValue::GpuAtomicI32(_) => "GpuAtomicI32".to_string(),
        KernelValue::GpuAtomicU32(_) => "GpuAtomicU32".to_string(),
    }
}
