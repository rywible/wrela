use super::{
    EngineFrameReport, EngineFutureReserveReport, EngineSubsystemKind, EngineSubsystemReport,
};
use crate::gpu_runtime::GpuRuntimeMetrics;
use crate::perf_target::PerfClosureEngineFrameBudget;
use std::collections::{BTreeMap, VecDeque};
use std::time::Instant;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineSubsystemDescriptor {
    pub kind: EngineSubsystemKind,
    pub label: String,
    pub runs_after: Vec<EngineSubsystemKind>,
    pub requires_gpu: bool,
    pub allows_hot_path_readback: bool,
}

#[derive(Debug, Default)]
pub struct EngineFrameContext {
    pub active_degradations: Vec<String>,
    pub violations: Vec<String>,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum EngineFrameError {
    #[error("engine frame subsystem dependency cycle or missing predecessor")]
    DependencyCycle,
    #[error("{0}")]
    Message(String),
}

pub trait EngineSubsystemWork {
    fn descriptor(&self) -> EngineSubsystemDescriptor;
    fn prepare(&mut self, ctx: &mut EngineFrameContext) -> Result<(), EngineFrameError>;
    fn encode(&mut self, ctx: &mut EngineFrameContext) -> Result<(), EngineFrameError>;
    fn finish(
        &mut self,
        ctx: &mut EngineFrameContext,
    ) -> Result<EngineSubsystemReport, EngineFrameError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EngineBudgetDecision {
    pub violations: Vec<String>,
    pub active_degradations: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct EngineBudgetGovernor;

impl EngineBudgetGovernor {
    pub fn observe_engine_frame(
        &mut self,
        report: &EngineFrameReport,
        budget: &PerfClosureEngineFrameBudget,
    ) -> EngineBudgetDecision {
        let mut decision = EngineBudgetDecision::default();
        if micros_to_ms(report.frame_wall_time_micros) > budget.frame_wall_time_p95_ms {
            decision
                .violations
                .push("engine_frame_wall_time_budget_exceeded".to_string());
        }
        if report.gpu_runtime.readback_bytes > budget.max_hot_path_readback_bytes_per_frame {
            decision
                .violations
                .push("engine_frame_hot_path_readback_budget_exceeded".to_string());
        }
        if report.gpu_runtime.queue_submit_count > budget.max_queue_submit_count_per_frame {
            decision
                .violations
                .push("engine_frame_queue_submit_budget_exceeded".to_string());
        }
        decision
    }
}

#[derive(Debug, Clone, Default)]
pub struct EngineFrameScheduler {
    pub budget: Option<PerfClosureEngineFrameBudget>,
}

impl EngineFrameScheduler {
    pub fn run_frame(
        &mut self,
        scenario_id: impl Into<String>,
        frame_index: u32,
        subsystems: &mut [Box<dyn EngineSubsystemWork>],
    ) -> Result<EngineFrameReport, EngineFrameError> {
        let scenario_id = scenario_id.into();
        let order = topological_order(subsystems)?;
        let started = Instant::now();
        let mut ctx = EngineFrameContext::default();
        let mut subsystem_reports = Vec::with_capacity(subsystems.len());
        for index in order {
            let subsystem = &mut subsystems[index];
            let descriptor = subsystem.descriptor();
            subsystem.prepare(&mut ctx)?;
            subsystem.encode(&mut ctx)?;
            let report = subsystem.finish(&mut ctx)?;
            subsystem_reports.push((descriptor, report));
        }

        let cpu_critical_path_micros = subsystem_reports
            .iter()
            .map(|(_, report)| report.cpu_critical_path_micros)
            .sum::<u128>();
        let gpu_critical_path_micros = subsystem_reports
            .iter()
            .filter_map(|(_, report)| report.gpu_critical_path_micros)
            .sum::<u128>();
        let queue_submit_count = observed_engine_frame_queue_submit_count(&subsystem_reports);
        let hot_path_readback_bytes = subsystem_reports
            .iter()
            .map(|(_, report)| report.hot_path_readback_bytes)
            .sum::<u64>();
        let scene_reupload_bytes = subsystem_reports
            .iter()
            .map(|(_, report)| report.scene_reupload_bytes)
            .sum::<u64>();
        let reports = subsystem_reports
            .into_iter()
            .map(|(_, report)| report)
            .collect::<Vec<_>>();

        let mut report = EngineFrameReport {
            scenario_id,
            frame_index,
            frame_wall_time_micros: started.elapsed().as_micros().max(cpu_critical_path_micros),
            cpu_critical_path_micros,
            gpu_critical_path_micros: (gpu_critical_path_micros > 0)
                .then_some(gpu_critical_path_micros),
            present_wait_micros: 0,
            gpu_wait_micros: 0,
            readback_wait_micros: 0,
            steady_state_fps: 0.0,
            gpu_runtime: GpuRuntimeMetrics {
                queue_submit_count,
                readback_bytes: hot_path_readback_bytes,
                scene_reupload_bytes,
                ..GpuRuntimeMetrics::default()
            },
            subsystems: reports,
            future_subsystem_reserve: EngineFutureReserveReport::default(),
            active_degradations: ctx.active_degradations,
            violations: ctx.violations,
        };

        if report.frame_wall_time_micros > 0 {
            report.steady_state_fps = 1_000_000.0 / report.frame_wall_time_micros as f64;
        }
        if let Some(budget) = &self.budget {
            let mut governor = EngineBudgetGovernor;
            let decision = governor.observe_engine_frame(&report, budget);
            report.violations.extend(decision.violations);
            report
                .active_degradations
                .extend(decision.active_degradations);
            let reserved_micros = ms_to_micros(budget.future_subsystem_reserve_ms);
            let remaining_micros = ms_to_micros(budget.frame_wall_time_median_ms) as i128
                - report.frame_wall_time_micros as i128
                - reserved_micros as i128;
            report.future_subsystem_reserve = EngineFutureReserveReport {
                reserved_micros,
                remaining_micros,
                exhausted: remaining_micros < 0,
            };
            if report.future_subsystem_reserve.exhausted {
                report
                    .violations
                    .push("engine_frame_future_reserve_exhausted".to_string());
            }
        }

        Ok(report)
    }
}

fn topological_order(
    subsystems: &[Box<dyn EngineSubsystemWork>],
) -> Result<Vec<usize>, EngineFrameError> {
    let descriptors = subsystems
        .iter()
        .map(|subsystem| subsystem.descriptor())
        .collect::<Vec<_>>();
    let mut indices_by_kind = BTreeMap::new();
    for (index, descriptor) in descriptors.iter().enumerate() {
        indices_by_kind.insert(descriptor.kind.clone(), index);
    }

    let mut indegree = vec![0usize; descriptors.len()];
    let mut outgoing = vec![Vec::<usize>::new(); descriptors.len()];
    for (index, descriptor) in descriptors.iter().enumerate() {
        for dependency in &descriptor.runs_after {
            let Some(&dependency_index) = indices_by_kind.get(dependency) else {
                return Err(EngineFrameError::DependencyCycle);
            };
            indegree[index] += 1;
            outgoing[dependency_index].push(index);
        }
    }

    let mut ready = indegree
        .iter()
        .enumerate()
        .filter_map(|(index, degree)| (*degree == 0).then_some(index))
        .collect::<VecDeque<_>>();
    let mut order = Vec::with_capacity(descriptors.len());
    while let Some(index) = ready.pop_front() {
        order.push(index);
        for target in &outgoing[index] {
            indegree[*target] -= 1;
            if indegree[*target] == 0 {
                ready.push_back(*target);
            }
        }
    }

    if order.len() != descriptors.len() {
        return Err(EngineFrameError::DependencyCycle);
    }
    Ok(order)
}

fn micros_to_ms(value: u128) -> f32 {
    value as f32 / 1_000.0
}

fn ms_to_micros(value: f32) -> u128 {
    (value.max(0.0) * 1_000.0).round() as u128
}

fn observed_engine_frame_queue_submit_count(
    subsystem_reports: &[(EngineSubsystemDescriptor, EngineSubsystemReport)],
) -> u32 {
    subsystem_reports.iter().fold(0_u32, |total, (_, report)| {
        total.saturating_add(report.queue_submit_count)
    })
}
