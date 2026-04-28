//! CPU system executor and observability (RFC 0011 Phase 65).

#![forbid(unsafe_code)]

use crate::hir::body::{Arg, AssignOp, BinaryOp, Body, Expr, Literal, Stmt, UnaryOp};
use crate::hir::{Param, TypeRef};
use crate::input_contract::InputFrame;
use crate::mir::passes::system_access::build_system_program_from_module;
use crate::system_contract::{EventTypeId, SystemId};
use crate::system_plan::{SystemPlan, SystemPlanError, SystemProgram};
use crate::world_identity::{SnapshotEpoch, WorldSnapshotHandle};
use smol_str::SmolStr;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use thiserror::Error;

pub const DEFAULT_SIMULATION_DT_SECONDS: f64 = 0.0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemExecutionRecord {
    pub system: SystemId,
    pub observed_input_actions: usize,
    pub visible_events: Vec<EventTypeId>,
    pub emitted_events: Vec<EventTypeId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SystemExecutionReport {
    pub records: Vec<SystemExecutionRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SystemExecError {
    #[error("system executor mismatch")]
    Mismatch,
    #[error("{0}")]
    Invoke(String),
}

#[derive(Debug, Error)]
pub enum CompiledSystemRuntimeError {
    #[error(transparent)]
    Plan(#[from] SystemPlanError),
}

/// Invokes compiled system bodies keyed by `SystemPlan::mir_function_id`.
pub trait SystemMirInvoker: Send + Sync {
    fn invoke(
        &self,
        mir_function_id: u32,
        ctx: &mut SystemInvocationContext<'_>,
    ) -> Result<(), String>;
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SystemResourceStore {
    resources: BTreeMap<SmolStr, BTreeMap<SmolStr, SystemValue>>,
}

impl SystemResourceStore {
    pub fn get_member(&self, resource: &SmolStr, member: &SmolStr) -> Option<&SystemValue> {
        self.resources
            .get(resource)
            .and_then(|record| record.get(member))
    }

    pub fn set_member(&mut self, resource: SmolStr, member: SmolStr, value: SystemValue) {
        self.resources
            .entry(resource)
            .or_default()
            .insert(member, value);
    }
}

#[derive(Debug)]
pub struct SystemInvocationContext<'a> {
    pub input: &'a InputFrame,
    pub resources: Arc<Mutex<SystemResourceStore>>,
    pub emitted_events: &'a mut Vec<EventTypeId>,
    pub dt_seconds: f64,
    pub snapshot_epoch: SnapshotEpoch,
    pub snapshot: Option<WorldSnapshotHandle>,
}

#[derive(Debug)]
pub struct HirSystemInvoker {
    bodies: BTreeMap<u32, HirSystemBody>,
}

#[derive(Debug, Clone)]
struct HirSystemBody {
    params: Vec<Param>,
    body: Body,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SystemValue {
    Bool(bool),
    Int(i64),
    Float(f64),
    String(SmolStr),
    InputFrame,
    Resource(SmolStr),
    EventEmitter(EventTypeId),
    Nil,
}

enum SystemFlow {
    Continue,
    Return,
}

impl HirSystemInvoker {
    pub fn from_project(project: &crate::hir::project::LoadedProject) -> Self {
        let bodies = project
            .module
            .functions
            .iter()
            .filter_map(|(idx, function)| {
                (function.role == crate::hir::FunctionRole::System)
                    .then(|| {
                        function.body.clone().map(|body| {
                            (
                                idx.into_raw() as u32,
                                HirSystemBody {
                                    params: function.params.clone(),
                                    body,
                                },
                            )
                        })
                    })
                    .flatten()
            })
            .collect();
        Self { bodies }
    }

    fn execute_body(
        &self,
        system: &HirSystemBody,
        ctx: &mut SystemInvocationContext<'_>,
    ) -> Result<(), String> {
        let mut locals = BTreeMap::new();
        locals.insert(
            SmolStr::new("input_action_count"),
            SystemValue::Int(ctx.input.actions.len() as i64),
        );
        bind_system_params(&system.params, &mut locals)?;
        match execute_system_block(&system.body, &system.body.root_stmts, &mut locals, ctx)? {
            SystemFlow::Continue | SystemFlow::Return => Ok(()),
        }
    }
}

impl SystemMirInvoker for HirSystemInvoker {
    fn invoke(
        &self,
        mir_function_id: u32,
        ctx: &mut SystemInvocationContext<'_>,
    ) -> Result<(), String> {
        let body = self.bodies.get(&mir_function_id).ok_or_else(|| {
            format!("system MIR invoker is not configured for function {mir_function_id}")
        })?;
        self.execute_body(body, ctx)
    }
}

fn bind_system_params(
    params: &[Param],
    locals: &mut BTreeMap<SmolStr, SystemValue>,
) -> Result<(), String> {
    for param in params {
        let Some(ty) = &param.ty else {
            locals.insert(param.name.clone(), SystemValue::Nil);
            continue;
        };
        if ty.name == "InputFrame" {
            locals.insert(param.name.clone(), SystemValue::InputFrame);
        } else if ty.name == "EventEmitter" {
            let event = ty.args.first().map(type_ref_name).ok_or_else(|| {
                format!(
                    "EventEmitter parameter `{}` is missing an event type",
                    param.name
                )
            })?;
            locals.insert(
                param.name.clone(),
                SystemValue::EventEmitter(EventTypeId::new(event.as_str())),
            );
        } else {
            locals.insert(param.name.clone(), SystemValue::Resource(type_ref_name(ty)));
        }
    }
    Ok(())
}

fn type_ref_name(ty: &TypeRef) -> SmolStr {
    ty.name.clone()
}

fn execute_system_block(
    body: &Body,
    stmts: &[crate::hir::Idx<Stmt>],
    locals: &mut BTreeMap<SmolStr, SystemValue>,
    ctx: &mut SystemInvocationContext<'_>,
) -> Result<SystemFlow, String> {
    for stmt_id in stmts {
        match &body.stmts[*stmt_id] {
            Stmt::Expr(expr) | Stmt::IgnoreResult { expr } => {
                eval_system_expr(body, *expr, locals, ctx)?;
            }
            Stmt::Let { name, value, .. } => {
                let value = eval_system_expr(body, *value, locals, ctx)?;
                locals.insert(name.clone(), value);
            }
            Stmt::Assign {
                name, op, value, ..
            } => {
                let rhs = eval_system_expr(body, *value, locals, ctx)?;
                let next = match op {
                    AssignOp::Assign => rhs,
                    AssignOp::AddAssign
                    | AssignOp::SubAssign
                    | AssignOp::MulAssign
                    | AssignOp::DivAssign => {
                        let current = locals.get(name).cloned().ok_or_else(|| {
                            format!("system assignment to unknown local `{name}`")
                        })?;
                        apply_system_assign_op(*op, current, rhs)?
                    }
                };
                assign_system_name(name, next, locals, ctx)?;
            }
            Stmt::If {
                condition,
                then_branch,
                else_branch,
            } => {
                if system_value_truthy(&eval_system_expr(body, *condition, locals, ctx)?) {
                    if matches!(
                        execute_system_block(body, then_branch, locals, ctx)?,
                        SystemFlow::Return
                    ) {
                        return Ok(SystemFlow::Return);
                    }
                } else if let Some(else_branch) = else_branch
                    && matches!(
                        execute_system_block(body, else_branch, locals, ctx)?,
                        SystemFlow::Return
                    )
                {
                    return Ok(SystemFlow::Return);
                }
            }
            Stmt::Return(expr) => {
                if let Some(expr) = expr {
                    eval_system_expr(body, *expr, locals, ctx)?;
                }
                return Ok(SystemFlow::Return);
            }
            Stmt::Use { .. } => {}
            unsupported => {
                return Err(format!(
                    "unsupported authored system statement: {unsupported:?}"
                ));
            }
        }
    }
    Ok(SystemFlow::Continue)
}

fn eval_system_expr(
    body: &Body,
    expr: crate::hir::Idx<Expr>,
    locals: &BTreeMap<SmolStr, SystemValue>,
    ctx: &mut SystemInvocationContext<'_>,
) -> Result<SystemValue, String> {
    match &body.exprs[expr] {
        Expr::Literal(Literal::Integer(value)) => Ok(SystemValue::Int(*value)),
        Expr::Literal(Literal::Float(value)) => Ok(SystemValue::Float(*value)),
        Expr::Literal(Literal::String(value)) => Ok(SystemValue::String(value.clone())),
        Expr::Literal(Literal::Boolean(value)) => Ok(SystemValue::Bool(*value)),
        Expr::Literal(Literal::Nil) => Ok(SystemValue::Nil),
        Expr::Variable(name) => locals
            .get(name)
            .cloned()
            .ok_or_else(|| format!("unsupported authored system variable `{name}`")),
        Expr::Unary { op, expr, .. } => {
            let value = eval_system_expr(body, *expr, locals, ctx)?;
            match (op, value) {
                (UnaryOp::Not, value) => Ok(SystemValue::Bool(!system_value_truthy(&value))),
                (UnaryOp::Neg, SystemValue::Int(value)) => Ok(SystemValue::Int(-value)),
                (UnaryOp::Neg, SystemValue::Float(value)) => Ok(SystemValue::Float(-value)),
                _ => Err(format!("unsupported authored system unary op `{op:?}`")),
            }
        }
        Expr::Binary { lhs, op, rhs, .. } => {
            if matches!(
                op,
                BinaryOp::Assign
                    | BinaryOp::AddAssign
                    | BinaryOp::SubAssign
                    | BinaryOp::MulAssign
                    | BinaryOp::DivAssign
            ) {
                return eval_system_assignment_expr(body, *lhs, *op, *rhs, locals, ctx);
            }
            let lhs = eval_system_expr(body, *lhs, locals, ctx)?;
            let rhs = eval_system_expr(body, *rhs, locals, ctx)?;
            apply_system_binary_op(*op, lhs, rhs)
        }
        Expr::Member { object, member, .. } => {
            let object = eval_system_expr(body, *object, locals, ctx)?;
            eval_system_member(object, member, ctx)
        }
        Expr::Call { callee, args, .. } => eval_system_call(body, *callee, args, locals, ctx),
        unsupported => Err(format!(
            "unsupported authored system expression: {unsupported:?}"
        )),
    }
}

fn eval_system_assignment_expr(
    body: &Body,
    lhs: crate::hir::Idx<Expr>,
    op: BinaryOp,
    rhs: crate::hir::Idx<Expr>,
    locals: &BTreeMap<SmolStr, SystemValue>,
    ctx: &mut SystemInvocationContext<'_>,
) -> Result<SystemValue, String> {
    let rhs_value = eval_system_expr(body, rhs, locals, ctx)?;
    let value = match op {
        BinaryOp::Assign => rhs_value,
        BinaryOp::AddAssign | BinaryOp::SubAssign | BinaryOp::MulAssign | BinaryOp::DivAssign => {
            let current = eval_system_expr(body, lhs, locals, ctx)?;
            let assign_op = match op {
                BinaryOp::AddAssign => AssignOp::AddAssign,
                BinaryOp::SubAssign => AssignOp::SubAssign,
                BinaryOp::MulAssign => AssignOp::MulAssign,
                BinaryOp::DivAssign => AssignOp::DivAssign,
                _ => unreachable!(),
            };
            apply_system_assign_op(assign_op, current, rhs_value)?
        }
        _ => unreachable!(),
    };
    match &body.exprs[lhs] {
        Expr::Variable(name) => Ok(locals.get(name).cloned().unwrap_or(value)),
        Expr::Member { object, member, .. } => {
            let object = eval_system_expr(body, *object, locals, ctx)?;
            let SystemValue::Resource(resource) = object else {
                return Err(format!(
                    "unsupported authored system assignment target: {:?}",
                    body.exprs[lhs]
                ));
            };
            ctx.resources
                .lock()
                .map_err(|_| "system resource store lock poisoned".to_string())?
                .set_member(resource, member.clone(), value.clone());
            Ok(value)
        }
        _ => Err(format!(
            "unsupported authored system assignment target: {:?}",
            body.exprs[lhs]
        )),
    }
}

fn assign_system_name(
    name: &SmolStr,
    value: SystemValue,
    locals: &mut BTreeMap<SmolStr, SystemValue>,
    ctx: &mut SystemInvocationContext<'_>,
) -> Result<(), String> {
    if let Some((object_name, member)) = name.as_str().split_once('.') {
        let object = locals
            .get(object_name)
            .cloned()
            .ok_or_else(|| format!("system assignment to unknown resource `{object_name}`"))?;
        if let SystemValue::Resource(resource) = object {
            ctx.resources
                .lock()
                .map_err(|_| "system resource store lock poisoned".to_string())?
                .set_member(resource, SmolStr::new(member), value);
            return Ok(());
        }
    }
    locals.insert(name.clone(), value);
    Ok(())
}

fn eval_system_member(
    object: SystemValue,
    member: &SmolStr,
    ctx: &SystemInvocationContext<'_>,
) -> Result<SystemValue, String> {
    match object {
        SystemValue::InputFrame if member.as_str() == "action_count" => {
            Ok(SystemValue::Int(ctx.input.actions.len() as i64))
        }
        SystemValue::InputFrame if member.as_str() == "tick" => {
            Ok(SystemValue::Int(ctx.input.tick.0 as i64))
        }
        SystemValue::Resource(resource) => Ok(ctx
            .resources
            .lock()
            .map_err(|_| "system resource store lock poisoned".to_string())?
            .get_member(&resource, member)
            .cloned()
            .unwrap_or(SystemValue::Nil)),
        unsupported => Err(format!(
            "unsupported authored system member `{member}` on {unsupported:?}"
        )),
    }
}

fn eval_system_call(
    body: &Body,
    callee: crate::hir::Idx<Expr>,
    args: &[Arg],
    locals: &BTreeMap<SmolStr, SystemValue>,
    ctx: &mut SystemInvocationContext<'_>,
) -> Result<SystemValue, String> {
    match &body.exprs[callee] {
        Expr::Variable(name) if name.as_str() == "dt" => Ok(SystemValue::Float(ctx.dt_seconds)),
        Expr::Member { object, member, .. } if member.as_str() == "send" => {
            let object = eval_system_expr(body, *object, locals, ctx)?;
            let SystemValue::EventEmitter(event) = object else {
                return Err("send() called on a non-EventEmitter value".into());
            };
            for arg in args {
                let expr = match arg {
                    Arg::Positional { value, .. } | Arg::Named { value, .. } => *value,
                };
                let _ = eval_system_expr(body, expr, locals, ctx)?;
            }
            ctx.emitted_events.push(event);
            Ok(SystemValue::Nil)
        }
        _ => Err(format!(
            "unsupported authored system call expression: {:?}",
            body.exprs[callee]
        )),
    }
}

fn apply_system_assign_op(
    op: AssignOp,
    lhs: SystemValue,
    rhs: SystemValue,
) -> Result<SystemValue, String> {
    let op = match op {
        AssignOp::AddAssign => BinaryOp::Add,
        AssignOp::SubAssign => BinaryOp::Sub,
        AssignOp::MulAssign => BinaryOp::Mul,
        AssignOp::DivAssign => BinaryOp::Div,
        AssignOp::Assign => return Ok(rhs),
    };
    apply_system_binary_op(op, lhs, rhs)
}

fn apply_system_binary_op(
    op: BinaryOp,
    lhs: SystemValue,
    rhs: SystemValue,
) -> Result<SystemValue, String> {
    match (op, lhs, rhs) {
        (BinaryOp::Add, SystemValue::Int(a), SystemValue::Int(b)) => Ok(SystemValue::Int(a + b)),
        (BinaryOp::Sub, SystemValue::Int(a), SystemValue::Int(b)) => Ok(SystemValue::Int(a - b)),
        (BinaryOp::Mul, SystemValue::Int(a), SystemValue::Int(b)) => Ok(SystemValue::Int(a * b)),
        (BinaryOp::Div, SystemValue::Int(a), SystemValue::Int(b)) => Ok(SystemValue::Int(a / b)),
        (BinaryOp::Add, SystemValue::Float(a), SystemValue::Float(b)) => {
            Ok(SystemValue::Float(a + b))
        }
        (BinaryOp::Sub, SystemValue::Float(a), SystemValue::Float(b)) => {
            Ok(SystemValue::Float(a - b))
        }
        (BinaryOp::Mul, SystemValue::Float(a), SystemValue::Float(b)) => {
            Ok(SystemValue::Float(a * b))
        }
        (BinaryOp::Div, SystemValue::Float(a), SystemValue::Float(b)) => {
            Ok(SystemValue::Float(a / b))
        }
        (BinaryOp::Eq, a, b) => Ok(SystemValue::Bool(a == b)),
        (BinaryOp::Ne, a, b) => Ok(SystemValue::Bool(a != b)),
        (BinaryOp::And, a, b) => Ok(SystemValue::Bool(
            system_value_truthy(&a) && system_value_truthy(&b),
        )),
        (BinaryOp::Or, a, b) | (BinaryOp::Otherwise, a, b) => Ok(SystemValue::Bool(
            system_value_truthy(&a) || system_value_truthy(&b),
        )),
        (op, lhs, rhs) => Err(format!(
            "unsupported authored system binary op `{op:?}` for {lhs:?} and {rhs:?}"
        )),
    }
}

fn system_value_truthy(value: &SystemValue) -> bool {
    match value {
        SystemValue::Bool(value) => *value,
        SystemValue::Int(value) => *value != 0,
        SystemValue::Float(value) => *value != 0.0,
        SystemValue::String(value) => !value.is_empty(),
        SystemValue::InputFrame | SystemValue::Resource(_) | SystemValue::EventEmitter(_) => true,
        SystemValue::Nil => false,
    }
}

#[derive(Clone)]
pub struct SystemExecutor {
    report: SystemExecutionReport,
    visible_events: Vec<EventTypeId>,
    next_tick_events: Vec<EventTypeId>,
    pending_emitted_events: BTreeMap<SystemId, Vec<EventTypeId>>,
    resources: Arc<Mutex<SystemResourceStore>>,
    invoker: Arc<dyn SystemMirInvoker>,
    default_simulation_dt_seconds: f64,
}

#[derive(Debug, Clone)]
pub struct CompiledSystemRuntime {
    pub program: SystemProgram,
    pub executor: SystemExecutor,
}

impl CompiledSystemRuntime {
    pub fn from_project(
        project: &crate::hir::project::LoadedProject,
    ) -> Result<Self, CompiledSystemRuntimeError> {
        let program = build_system_program_from_module(&project.module)?;
        Ok(Self {
            program,
            executor: SystemExecutor::new(Arc::new(HirSystemInvoker::from_project(project))),
        })
    }
}

impl std::fmt::Debug for SystemExecutor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SystemExecutor")
            .field("records", &self.report.records.len())
            .finish_non_exhaustive()
    }
}

impl SystemExecutor {
    pub fn new(invoker: Arc<dyn SystemMirInvoker>) -> Self {
        Self {
            report: SystemExecutionReport::default(),
            visible_events: Vec::new(),
            next_tick_events: Vec::new(),
            pending_emitted_events: BTreeMap::new(),
            resources: Arc::new(Mutex::new(SystemResourceStore::default())),
            invoker,
            default_simulation_dt_seconds: DEFAULT_SIMULATION_DT_SECONDS,
        }
    }

    pub fn with_default_invoker() -> Self {
        Self::new(Arc::new(HirSystemInvoker {
            bodies: BTreeMap::new(),
        }))
    }

    pub fn invoker(&self) -> Arc<dyn SystemMirInvoker> {
        Arc::clone(&self.invoker)
    }

    pub fn default_simulation_dt_seconds(&self) -> f64 {
        self.default_simulation_dt_seconds
    }

    pub fn set_default_simulation_dt_seconds(&mut self, dt_seconds: f64) {
        self.default_simulation_dt_seconds = dt_seconds;
    }

    pub fn with_default_simulation_dt_seconds(mut self, dt_seconds: f64) -> Self {
        self.set_default_simulation_dt_seconds(dt_seconds);
        self
    }

    pub fn begin_tick(&mut self) {
        self.visible_events = std::mem::take(&mut self.next_tick_events);
        self.report.records.clear();
        self.pending_emitted_events.clear();
    }

    pub fn resources(&self) -> Arc<Mutex<SystemResourceStore>> {
        Arc::clone(&self.resources)
    }

    pub fn replace_resources(&mut self, resources: SystemResourceStore) {
        self.resources = Arc::new(Mutex::new(resources));
    }

    pub fn invoke_system_body(
        &mut self,
        plan: &SystemPlan,
        input: &InputFrame,
    ) -> Result<(), SystemExecError> {
        self.invoke_system_body_with_dt(plan, input, self.default_simulation_dt_seconds)
    }

    pub fn invoke_system_body_with_dt(
        &mut self,
        plan: &SystemPlan,
        input: &InputFrame,
        dt_seconds: f64,
    ) -> Result<(), SystemExecError> {
        let mut emitted_events = Vec::new();
        let mut ctx = SystemInvocationContext {
            input,
            resources: Arc::clone(&self.resources),
            emitted_events: &mut emitted_events,
            dt_seconds,
            snapshot_epoch: input.epoch,
            snapshot: None,
        };
        self.invoker
            .invoke(plan.mir_function_id, &mut ctx)
            .map_err(SystemExecError::Invoke)?;
        self.enqueue_system_emitted_events(plan.id.clone(), emitted_events);
        Ok(())
    }

    pub fn record_system_execution(
        &mut self,
        plan: &SystemPlan,
        input: &InputFrame,
        emitted_events: Vec<EventTypeId>,
    ) -> SystemExecutionRecord {
        let record = SystemExecutionRecord {
            system: plan.id.clone(),
            observed_input_actions: input.actions.len(),
            visible_events: self.visible_events.clone(),
            emitted_events,
        };
        self.report.records.push(record.clone());
        record
    }

    pub fn enqueue_emitted_events(&mut self, emitted_events: Vec<EventTypeId>) {
        self.next_tick_events.extend(emitted_events);
    }

    pub fn enqueue_system_emitted_events(
        &mut self,
        system: SystemId,
        emitted_events: Vec<EventTypeId>,
    ) {
        if emitted_events.is_empty() {
            return;
        }
        self.pending_emitted_events
            .entry(system)
            .or_default()
            .extend(emitted_events);
    }

    pub fn run_system(
        &mut self,
        plan: &SystemPlan,
        input: &InputFrame,
    ) -> Result<SystemExecutionRecord, SystemExecError> {
        self.run_system_with_dt(plan, input, self.default_simulation_dt_seconds)
    }

    pub fn run_system_with_dt(
        &mut self,
        plan: &SystemPlan,
        input: &InputFrame,
        dt_seconds: f64,
    ) -> Result<SystemExecutionRecord, SystemExecError> {
        self.invoke_system_body_with_dt(plan, input, dt_seconds)?;
        let emitted_events = self
            .pending_emitted_events
            .remove(&plan.id)
            .unwrap_or_default();
        self.next_tick_events.extend(emitted_events.iter().cloned());
        Ok(self.record_system_execution(plan, input, emitted_events))
    }

    pub fn commit_program_execution_records(
        &mut self,
        program: &SystemProgram,
        input: &InputFrame,
    ) -> SystemExecutionReport {
        for phase in &program.phases {
            for plan in phase {
                let emitted_events = self
                    .pending_emitted_events
                    .remove(&plan.id)
                    .unwrap_or_default();
                self.next_tick_events.extend(emitted_events.iter().cloned());
                self.record_system_execution(plan, input, emitted_events);
            }
        }
        self.report.clone()
    }

    pub fn run_program(
        &mut self,
        program: &SystemProgram,
        input: &InputFrame,
    ) -> Result<SystemExecutionReport, SystemExecError> {
        self.run_program_with_dt(program, input, self.default_simulation_dt_seconds)
    }

    pub fn run_program_with_dt(
        &mut self,
        program: &SystemProgram,
        input: &InputFrame,
        dt_seconds: f64,
    ) -> Result<SystemExecutionReport, SystemExecError> {
        self.begin_tick();
        for phase in &program.phases {
            for plan in phase {
                self.run_system_with_dt(plan, input, dt_seconds)?;
            }
        }
        Ok(self.report.clone())
    }

    pub fn report(&self) -> &SystemExecutionReport {
        &self.report
    }
}

impl Default for SystemExecutor {
    fn default() -> Self {
        Self::with_default_invoker()
    }
}
