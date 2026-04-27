//! CPU system executor and observability (RFC 0011 Phase 65).

#![forbid(unsafe_code)]

use crate::input_contract::InputFrame;
use crate::mir::passes::system_access::build_system_program_from_module;
use crate::system_contract::{EventTypeId, SystemId};
use crate::system_plan::{SystemPlan, SystemPlanError, SystemProgram};
use std::sync::Arc;
use thiserror::Error;

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
    #[error("runtime backend not implemented for {system_count} project system(s)")]
    UnsupportedBackend { system_count: usize },
}

/// Invokes compiled system bodies keyed by `SystemPlan::mir_function_id`.
pub trait SystemMirInvoker: Send + Sync {
    fn invoke(&self, mir_function_id: u32, input: &InputFrame) -> Result<(), String>;
}

#[derive(Debug)]
struct DefaultMirInvoker;

impl SystemMirInvoker for DefaultMirInvoker {
    fn invoke(&self, mir_function_id: u32, _input: &InputFrame) -> Result<(), String> {
        Err(format!(
            "system MIR invoker is not configured for function {mir_function_id}"
        ))
    }
}

#[derive(Clone)]
pub struct SystemExecutor {
    report: SystemExecutionReport,
    visible_events: Vec<EventTypeId>,
    next_tick_events: Vec<EventTypeId>,
    invoker: Arc<dyn SystemMirInvoker>,
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
        let system_count = program.phases.iter().map(Vec::len).sum();
        if system_count > 0 {
            return Err(CompiledSystemRuntimeError::UnsupportedBackend { system_count });
        }
        Ok(Self {
            program,
            executor: SystemExecutor::with_default_invoker(),
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
            invoker,
        }
    }

    pub fn with_default_invoker() -> Self {
        Self::new(Arc::new(DefaultMirInvoker))
    }

    pub fn invoker(&self) -> Arc<dyn SystemMirInvoker> {
        Arc::clone(&self.invoker)
    }

    pub fn begin_tick(&mut self) {
        self.visible_events = std::mem::take(&mut self.next_tick_events);
        self.report.records.clear();
    }

    pub fn invoke_system_body(
        &self,
        plan: &SystemPlan,
        input: &InputFrame,
    ) -> Result<(), SystemExecError> {
        self.invoker
            .invoke(plan.mir_function_id, input)
            .map_err(SystemExecError::Invoke)
    }

    pub fn record_system_execution(
        &mut self,
        plan: &SystemPlan,
        input: &InputFrame,
    ) -> SystemExecutionRecord {
        let emitted_events = plan.access.emits_events.iter().cloned().collect::<Vec<_>>();
        self.next_tick_events.extend(emitted_events.iter().cloned());
        let record = SystemExecutionRecord {
            system: plan.id.clone(),
            observed_input_actions: input.actions.len(),
            visible_events: self.visible_events.clone(),
            emitted_events,
        };
        self.report.records.push(record.clone());
        record
    }

    pub fn run_system(
        &mut self,
        plan: &SystemPlan,
        input: &InputFrame,
    ) -> Result<SystemExecutionRecord, SystemExecError> {
        self.invoke_system_body(plan, input)?;
        Ok(self.record_system_execution(plan, input))
    }

    pub fn commit_program_execution_records(
        &mut self,
        program: &SystemProgram,
        input: &InputFrame,
    ) -> SystemExecutionReport {
        for phase in &program.phases {
            for plan in phase {
                self.record_system_execution(plan, input);
            }
        }
        self.report.clone()
    }

    pub fn run_program(
        &mut self,
        program: &SystemProgram,
        input: &InputFrame,
    ) -> Result<SystemExecutionReport, SystemExecError> {
        self.begin_tick();
        for phase in &program.phases {
            for plan in phase {
                self.invoke_system_body(plan, input)?;
            }
        }
        Ok(self.commit_program_execution_records(program, input))
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
