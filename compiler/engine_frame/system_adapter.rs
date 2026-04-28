//! Engine-frame adapter for CPU systems (RFC 0011 Phase 65).
//!
//! Schedules CPU systems honouring:
//! - Phase order (Init -> Update -> Late) via `previous_phase_terminals`.
//! - Intra-phase dependency DAG (RFC 0011 H8 acceptance) so independent
//!   systems in the same phase can run in parallel; only systems whose
//!   reads/writes overlap are serialised against each other.

use super::{
    EngineFrameContext, EngineFrameError, EngineFrameTimeline, EngineGpuTimingPolicy,
    EngineGraphBuilder, EngineJobAffinity, EngineJobHandle, EngineMeasurementPolicy,
    EngineRuntimeSource, EngineSpanDomain, EngineSubsystemAdapter, EngineSubsystemDescriptor,
    EngineSubsystemKind, EngineSubsystemPlan, EngineSubsystemReport,
};
use crate::input_contract::InputFrame;
use crate::system_contract::SystemPhase;
use crate::system_exec::{SystemExecutor, SystemInvocationContext, SystemMirInvoker};
use crate::system_plan::{SystemProgram, declared_runs_before};
use std::sync::{Arc, Mutex};

pub struct SystemSubsystemAdapter {
    program: SystemProgram,
    executor: Arc<Mutex<SystemExecutor>>,
    input_frame: Arc<Mutex<Option<InputFrame>>>,
    dt_seconds: Arc<Mutex<f64>>,
    report_notes: Vec<String>,
}

impl SystemSubsystemAdapter {
    pub fn new(program: SystemProgram, input_frame: Arc<Mutex<Option<InputFrame>>>) -> Self {
        Self {
            program,
            executor: Arc::new(Mutex::new(SystemExecutor::with_default_invoker())),
            input_frame,
            dt_seconds: Arc::new(Mutex::new(0.0)),
            report_notes: Vec::new(),
        }
    }

    pub fn with_invoker(
        program: SystemProgram,
        input_frame: Arc<Mutex<Option<InputFrame>>>,
        invoker: Arc<dyn SystemMirInvoker>,
    ) -> Self {
        Self {
            program,
            executor: Arc::new(Mutex::new(SystemExecutor::new(invoker))),
            input_frame,
            dt_seconds: Arc::new(Mutex::new(0.0)),
            report_notes: Vec::new(),
        }
    }

    pub fn executor(&self) -> Arc<Mutex<SystemExecutor>> {
        Arc::clone(&self.executor)
    }

    pub fn with_report_notes(mut self, notes: Vec<String>) -> Self {
        self.report_notes = notes;
        self
    }
}

impl EngineSubsystemAdapter for SystemSubsystemAdapter {
    fn prepare_frame(&mut self, input: &super::EngineFrameInput) {
        if let Ok(mut dt_seconds) = self.dt_seconds.lock() {
            *dt_seconds = input.frame_dt_seconds();
        }
    }

    fn build(
        &mut self,
        builder: &mut EngineGraphBuilder,
    ) -> Result<EngineSubsystemPlan, EngineFrameError> {
        let descriptor = EngineSubsystemDescriptor {
            kind: EngineSubsystemKind::System,
            label: "system".to_string(),
            runs_after: vec![
                EngineSubsystemKind::StateAdvance,
                EngineSubsystemKind::Input,
            ],
            requires_gpu: false,
            allows_hot_path_readback: false,
        };
        let executor_for_begin = Arc::clone(&self.executor);
        let begin_job = builder.add_job(
            EngineSubsystemKind::System,
            "system.begin_tick".to_string(),
            EngineJobAffinity::Cpu,
            EngineSpanDomain::Cpu,
            Vec::new(),
            false,
            move || {
                executor_for_begin
                    .lock()
                    .map_err(|_| EngineFrameError::Message("system executor lock poisoned".into()))?
                    .begin_tick();
                Ok(())
            },
        );
        let root_jobs = vec![begin_job];
        let mut terminal_jobs = Vec::new();
        let mut previous_phase_terminals: Vec<EngineJobHandle> = vec![begin_job];
        for phase in SystemPhase::ALL {
            let plans = self.program.phase(phase);
            if plans.is_empty() {
                continue;
            }

            let mut deps_within_phase: Vec<Vec<usize>> = Vec::with_capacity(plans.len());
            for (i, later) in plans.iter().enumerate() {
                let mut deps = Vec::new();
                for (j, earlier) in plans.iter().enumerate() {
                    if j < i && declared_runs_before(earlier, later) {
                        deps.push(j);
                    }
                }
                deps_within_phase.push(deps);
            }

            let mut phase_jobs: Vec<EngineJobHandle> = Vec::with_capacity(plans.len());
            for (i, plan) in plans.iter().cloned().enumerate() {
                let mut deps: Vec<EngineJobHandle> =
                    Vec::with_capacity(previous_phase_terminals.len() + deps_within_phase[i].len());
                if deps_within_phase[i].is_empty() {
                    deps.extend(previous_phase_terminals.iter().copied());
                } else {
                    for j in &deps_within_phase[i] {
                        deps.push(phase_jobs[*j]);
                    }
                }
                let executor = Arc::clone(&self.executor);
                let input_frame = Arc::clone(&self.input_frame);
                let dt_seconds = Arc::clone(&self.dt_seconds);
                let label = format!("system.{}.{}", phase.label(), plan.id.0);
                let job = builder.add_job(
                    EngineSubsystemKind::System,
                    label,
                    EngineJobAffinity::Cpu,
                    EngineSpanDomain::Cpu,
                    deps,
                    false,
                    move || {
                        let input = input_frame
                            .lock()
                            .map_err(|_| {
                                EngineFrameError::Message("input frame lock poisoned".into())
                            })?
                            .clone()
                            .ok_or_else(|| {
                                EngineFrameError::Message(
                                    "system subsystem ran before input frame was published".into(),
                                )
                            })?;
                        let (invoker, resources) = {
                            let executor = executor.lock().map_err(|_| {
                                EngineFrameError::Message("system executor lock poisoned".into())
                            })?;
                            (executor.invoker(), executor.resources())
                        };
                        let mut emitted_events = Vec::new();
                        let dt_seconds = *dt_seconds.lock().map_err(|_| {
                            EngineFrameError::Message("system dt lock poisoned".into())
                        })?;
                        let mut ctx = SystemInvocationContext {
                            dt_seconds,
                            snapshot_epoch: input.epoch,
                            snapshot: None,
                            input: &input,
                            resources,
                            emitted_events: &mut emitted_events,
                        };
                        invoker
                            .invoke(plan.mir_function_id, &mut ctx)
                            .map_err(|err| EngineFrameError::Message(err))?;
                        executor
                            .lock()
                            .map_err(|_| {
                                EngineFrameError::Message("system executor lock poisoned".into())
                            })?
                            .enqueue_system_emitted_events(plan.id.clone(), emitted_events);
                        Ok(())
                    },
                );
                phase_jobs.push(job);
            }

            // Phase terminals are the jobs in this phase that no other job
            // within the phase depends on.
            let mut has_dependent = vec![false; phase_jobs.len()];
            for deps in &deps_within_phase {
                for j in deps {
                    has_dependent[*j] = true;
                }
            }
            let phase_terminals: Vec<EngineJobHandle> = phase_jobs
                .iter()
                .enumerate()
                .filter_map(|(i, job)| if has_dependent[i] { None } else { Some(*job) })
                .collect();
            if !phase_terminals.is_empty() {
                previous_phase_terminals = phase_terminals.clone();
                terminal_jobs = phase_terminals;
            }
        }
        if terminal_jobs.is_empty() {
            let job = builder.add_job(
                EngineSubsystemKind::System,
                "system.idle".to_string(),
                EngineJobAffinity::Cpu,
                EngineSpanDomain::Cpu,
                vec![begin_job],
                false,
                || Ok(()),
            );
            terminal_jobs.push(job);
        } else {
            let executor = Arc::clone(&self.executor);
            let input_frame = Arc::clone(&self.input_frame);
            let program = self.program.clone();
            let job = builder.add_job(
                EngineSubsystemKind::System,
                "system.join".to_string(),
                EngineJobAffinity::Cpu,
                EngineSpanDomain::Cpu,
                terminal_jobs.clone(),
                false,
                move || {
                    let input = input_frame
                        .lock()
                        .map_err(|_| EngineFrameError::Message("input frame lock poisoned".into()))?
                        .clone()
                        .ok_or_else(|| {
                            EngineFrameError::Message(
                                "system subsystem joined before input frame was published".into(),
                            )
                        })?;
                    executor
                        .lock()
                        .map_err(|_| {
                            EngineFrameError::Message("system executor lock poisoned".into())
                        })?
                        .commit_program_execution_records(&program, &input);
                    Ok(())
                },
            );
            terminal_jobs = vec![job];
        }
        let executor_for_report = Arc::clone(&self.executor);
        let report_notes = self.report_notes.clone();
        Ok(EngineSubsystemPlan::new(
            descriptor.clone(),
            root_jobs,
            terminal_jobs,
            move |timeline: &EngineFrameTimeline, _ctx: &mut EngineFrameContext| {
                let executed = timeline
                    .spans
                    .iter()
                    .filter(|span| span.subsystem == EngineSubsystemKind::System)
                    .map(|span| span.elapsed_micros())
                    .sum();
                let work_items = executor_for_report
                    .lock()
                    .map_err(|_| EngineFrameError::Message("system executor lock poisoned".into()))?
                    .report()
                    .records
                    .len() as u64;
                Ok(EngineSubsystemReport {
                    kind: descriptor.kind.clone(),
                    label: descriptor.label.clone(),
                    work_items,
                    cpu_critical_path_micros: executed,
                    gpu_critical_path_micros: None,
                    executed_wall_time_micros: executed,
                    self_reported_runtime_micros: Some(executed),
                    orchestration_gap_micros: 0,
                    measurement_policy: EngineMeasurementPolicy {
                        runtime_source: EngineRuntimeSource::TimelineSpans,
                        gpu_timing: EngineGpuTimingPolicy::Disabled,
                        hot_path_readback_allowed: false,
                        export_readback_allowed: false,
                    },
                    queue_submit_count: 0,
                    hot_path_readback_bytes: 0,
                    scene_reupload_bytes: 0,
                    timestamped_pass_count: 0,
                    timing_readback_bytes: 0,
                    wait_time_micros: 0,
                    notes: report_notes.clone(),
                })
            },
        ))
    }
}
