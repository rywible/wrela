use super::latency::{MeasurementQuality, MotionToPhotonContract};
use super::{
    ENGINE_FRAME_TIMELINE_VERSION, EngineBudgetDirectives, EngineFenceId,
    EngineFrameIdentityReport, EngineFrameReport, EngineFrameTimeline, EngineFutureReserveReport,
    EngineGpuFrameLedger, EngineJobAffinity, EngineJobHandle, EngineQueryLedger,
    EngineReadbackLedger, EngineResourceAccess, EngineResourceLedger, EngineResourceState,
    EngineSpanDomain, EngineSpanId, EngineSpanRecord, EngineSubsystemKind, EngineSubsystemReport,
    EngineSubsystemSpanRange,
};
use crate::gpu_runtime::GpuRuntimeMetrics;
use crate::perf_target::PerfClosureEngineFrameBudget;
use std::collections::BTreeMap;
use thiserror::Error;
use wrela_runtime::engine_executor::{
    EngineExecutor, EngineExecutorConfig, EngineTask, EngineTaskAffinity,
};

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
    pub input_snapshot_epoch: Option<u64>,
    pub published_snapshot_epoch: Option<u64>,
    pub active_degradations: Vec<String>,
    pub violations: Vec<String>,
    /// Monotonic nanosecond timestamp for the StateAdvance input sampling
    /// point, compatible with
    /// [`TickInputEvent::monotonic_nanos`](crate::state_advance::TickInputEvent::monotonic_nanos).
    /// Scheduler spans remain frame-relative; do not treat this as a
    /// scheduler span-zero/frame-origin timestamp.
    pub state_advance_input_sample_nanos: Option<u64>,
    /// Monotonic nanoseconds of the earliest raw input event that was
    /// observed for this frame. Adapters should populate this so the
    /// motion-to-photon contract can be computed honestly (RFC 0011 H3).
    pub earliest_input_arrival_nanos: Option<u64>,
    /// Count of non-zero input timestamps that were later than the
    /// StateAdvance input sample point. These indicate mixed clock domains or
    /// otherwise invalid sampler timestamps.
    pub future_input_timestamp_count: usize,
    /// Host-provided estimate from present callback return to visible photons.
    ///
    /// Presentation adapters should set this from the resolved present mode and
    /// display refresh interval when known. The scheduler preserves the value
    /// rather than treating this display-side stage as zero by default.
    pub estimated_present_to_photons_nanos: Option<u64>,
    pub resource_accesses: Vec<EngineResourceAccess>,
    pub resource_states: Vec<EngineResourceState>,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum EngineFrameError {
    #[error("engine frame subsystem dependency cycle or missing predecessor")]
    DependencyCycle,
    #[error("engine frame job graph referenced a missing job")]
    MissingJob,
    #[error("duplicate engine subsystem adapter for kind {0:?}")]
    DuplicateSubsystemKind(EngineSubsystemKind),
    #[error("{0}")]
    Message(String),
}

type EngineReportBuilder = Box<
    dyn Fn(
            &EngineFrameTimeline,
            &mut EngineFrameContext,
        ) -> Result<EngineSubsystemReport, EngineFrameError>
        + Send
        + Sync
        + 'static,
>;

pub struct EngineSubsystemPlan {
    pub descriptor: EngineSubsystemDescriptor,
    pub root_jobs: Vec<EngineJobHandle>,
    pub terminal_jobs: Vec<EngineJobHandle>,
    report_builder: EngineReportBuilder,
}

impl EngineSubsystemPlan {
    pub fn new<F>(
        descriptor: EngineSubsystemDescriptor,
        root_jobs: Vec<EngineJobHandle>,
        terminal_jobs: Vec<EngineJobHandle>,
        report_builder: F,
    ) -> Self
    where
        F: Fn(
                &EngineFrameTimeline,
                &mut EngineFrameContext,
            ) -> Result<EngineSubsystemReport, EngineFrameError>
            + Send
            + Sync
            + 'static,
    {
        Self {
            descriptor,
            root_jobs,
            terminal_jobs,
            report_builder: Box::new(report_builder),
        }
    }

    fn build_report(
        &self,
        timeline: &EngineFrameTimeline,
        ctx: &mut EngineFrameContext,
    ) -> Result<EngineSubsystemReport, EngineFrameError> {
        (self.report_builder)(timeline, ctx)
    }
}

pub trait EngineSubsystemAdapter {
    /// Called once per frame before [`EngineSubsystemAdapter::build`] so adapters can
    /// capture per-frame inputs (snapshot epoch, shared state slots, etc.).
    fn prepare_frame(&mut self, _input: &super::EngineFrameInput) {}

    fn build(
        &mut self,
        builder: &mut EngineGraphBuilder,
    ) -> Result<EngineSubsystemPlan, EngineFrameError>;
}

type EngineJobTask = Box<dyn FnOnce() -> Result<(), String> + Send + 'static>;

struct EngineJobSpec {
    handle: EngineJobHandle,
    subsystem: EngineSubsystemKind,
    label: String,
    affinity: EngineJobAffinity,
    domain: EngineSpanDomain,
    depends_on: Vec<EngineJobHandle>,
    queue_submission: bool,
    simulated_elapsed_micros: Option<u128>,
    task: Option<EngineJobTask>,
}

pub struct EngineFrameGraph {
    jobs: Vec<EngineJobSpec>,
    subsystem_plans: Vec<EngineSubsystemPlan>,
}

impl EngineFrameGraph {
    pub fn job_count(&self) -> usize {
        self.jobs.len()
    }

    pub fn subsystem_count(&self) -> usize {
        self.subsystem_plans.len()
    }
}

#[derive(Default)]
pub struct EngineGraphBuilder {
    jobs: Vec<EngineJobSpec>,
    next_job_id: u32,
    next_fence_id: u32,
}

impl EngineGraphBuilder {
    pub fn add_job<F>(
        &mut self,
        subsystem: EngineSubsystemKind,
        label: impl Into<String>,
        affinity: EngineJobAffinity,
        domain: EngineSpanDomain,
        depends_on: Vec<EngineJobHandle>,
        queue_submission: bool,
        task: F,
    ) -> EngineJobHandle
    where
        F: FnOnce() -> Result<(), EngineFrameError> + Send + 'static,
    {
        self.push_job(
            subsystem,
            label,
            affinity,
            domain,
            depends_on,
            queue_submission,
            None,
            Box::new(move || task().map_err(|err| err.to_string())),
        )
    }

    pub fn add_synthetic_job(
        &mut self,
        subsystem: EngineSubsystemKind,
        label: impl Into<String>,
        affinity: EngineJobAffinity,
        domain: EngineSpanDomain,
        depends_on: Vec<EngineJobHandle>,
        queue_submission: bool,
        simulated_elapsed_micros: u128,
    ) -> EngineJobHandle {
        self.push_job(
            subsystem,
            label,
            affinity,
            domain,
            depends_on,
            queue_submission,
            Some(simulated_elapsed_micros),
            Box::new(|| Ok(())),
        )
    }

    pub fn add_dependency(
        &mut self,
        job: EngineJobHandle,
        dependency: EngineJobHandle,
    ) -> Result<(), EngineFrameError> {
        let Some(index) = self
            .jobs
            .iter()
            .position(|candidate| candidate.handle == job)
        else {
            return Err(EngineFrameError::MissingJob);
        };
        if !self.jobs[index].depends_on.contains(&dependency) {
            self.jobs[index].depends_on.push(dependency);
        }
        Ok(())
    }

    pub fn next_fence_id(&mut self) -> EngineFenceId {
        let fence = EngineFenceId(self.next_fence_id);
        self.next_fence_id = self.next_fence_id.saturating_add(1);
        fence
    }

    pub fn finish(self, subsystem_plans: Vec<EngineSubsystemPlan>) -> EngineFrameGraph {
        EngineFrameGraph {
            jobs: self.jobs,
            subsystem_plans,
        }
    }

    fn push_job(
        &mut self,
        subsystem: EngineSubsystemKind,
        label: impl Into<String>,
        affinity: EngineJobAffinity,
        domain: EngineSpanDomain,
        depends_on: Vec<EngineJobHandle>,
        queue_submission: bool,
        simulated_elapsed_micros: Option<u128>,
        task: EngineJobTask,
    ) -> EngineJobHandle {
        let handle = EngineJobHandle(self.next_job_id);
        self.next_job_id = self.next_job_id.saturating_add(1);
        self.jobs.push(EngineJobSpec {
            handle,
            subsystem,
            label: label.into(),
            affinity,
            domain,
            depends_on,
            queue_submission,
            simulated_elapsed_micros,
            task: Some(task),
        });
        handle
    }
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

#[derive(Debug, Clone)]
pub struct EngineFrameScheduler {
    pub budget: Option<PerfClosureEngineFrameBudget>,
    executor: EngineExecutor,
}

impl Default for EngineFrameScheduler {
    fn default() -> Self {
        Self {
            budget: None,
            executor: EngineExecutor::default(),
        }
    }
}

impl EngineFrameScheduler {
    pub fn with_executor_config(config: EngineExecutorConfig) -> Self {
        Self {
            budget: None,
            executor: EngineExecutor::new(config),
        }
    }

    pub fn run_frame(
        &mut self,
        scenario_id: impl Into<String>,
        frame_index: u32,
        adapters: &mut [Box<dyn EngineSubsystemAdapter>],
    ) -> Result<EngineFrameReport, EngineFrameError> {
        let mut refs: Vec<&mut dyn EngineSubsystemAdapter> = adapters
            .iter_mut()
            .map(|adapter| adapter.as_mut() as &mut dyn EngineSubsystemAdapter)
            .collect::<Vec<_>>();
        self.run_frame_borrowed(scenario_id, frame_index, &mut refs)
    }

    pub fn run_frame_borrowed(
        &mut self,
        scenario_id: impl Into<String>,
        frame_index: u32,
        adapters: &mut [&mut dyn EngineSubsystemAdapter],
    ) -> Result<EngineFrameReport, EngineFrameError> {
        let scenario_id = scenario_id.into();
        let mut builder = EngineGraphBuilder::default();
        let mut subsystem_plans = Vec::with_capacity(adapters.len());
        for adapter in adapters {
            subsystem_plans.push(adapter.build(&mut builder)?);
        }
        validate_unique_subsystem_kinds(&subsystem_plans)?;
        let mut graph = builder.finish(subsystem_plans);
        wire_subsystem_dependencies(&mut graph)?;
        let (timeline, timeline_spans) = execute_graph(&mut graph, &self.executor)?;
        let mut ctx = EngineFrameContext::default();
        let subsystem_order = topological_subsystem_order(&graph.subsystem_plans)?;
        let mut reports = Vec::with_capacity(graph.subsystem_plans.len());
        for subsystem_index in subsystem_order {
            let report =
                graph.subsystem_plans[subsystem_index].build_report(&timeline, &mut ctx)?;
            reports.push(report);
        }
        let (cpu_critical_path_micros, gpu_critical_path_micros) =
            critical_path_split(&timeline, &timeline_spans);
        let cpu_busy_micros = busy_duration_for_domains(
            &timeline.spans,
            &[EngineSpanDomain::Cpu, EngineSpanDomain::External],
        );
        let gpu_busy_micros = busy_duration_for_domains(
            &timeline.spans,
            &[
                EngineSpanDomain::Gpu,
                EngineSpanDomain::GpuWait,
                EngineSpanDomain::ReadbackWait,
                EngineSpanDomain::PresentWait,
            ],
        );
        let overlap_ratio = overlap_ratio(
            cpu_busy_micros,
            gpu_busy_micros,
            timeline_spans.frame_wall_time_micros,
        );
        let scheduler_owned_queue_submits = timeline.queue_submission_spans.len() as u32;
        let queue_submit_count = observed_engine_frame_queue_submit_count(&reports);
        let private_queue_submits =
            queue_submit_count.saturating_sub(scheduler_owned_queue_submits);
        let hot_path_readback_bytes = reports
            .iter()
            .map(|report| report.hot_path_readback_bytes)
            .sum::<u64>();
        let scene_reupload_bytes = reports
            .iter()
            .map(|report| report.scene_reupload_bytes)
            .sum::<u64>();
        let state_advance_input_sample_nanos = ctx.state_advance_input_sample_nanos;
        let mut violations = ctx.violations;
        let mut active_degradations = ctx.active_degradations;
        if ctx.future_input_timestamp_count > 0 {
            extend_unique_strings(
                &mut violations,
                ["latency.input_timestamp_after_sample".to_string()],
            );
            extend_unique_strings(
                &mut active_degradations,
                ["latency.input_timestamp_domain_invalid".to_string()],
            );
        }
        let latency = motion_to_photon_contract_from_timeline(
            &timeline,
            timeline_spans.frame_wall_time_micros,
            ctx.earliest_input_arrival_nanos,
            state_advance_input_sample_nanos,
            ctx.estimated_present_to_photons_nanos.unwrap_or(0),
        );
        let mut report = EngineFrameReport {
            scenario_id,
            frame_index,
            identity: EngineFrameIdentityReport::default(),
            state_advance: None,
            resource_ledger: EngineResourceLedger {
                accesses: ctx.resource_accesses,
                states: ctx.resource_states,
                violations: Vec::new(),
            },
            readback_ledger: EngineReadbackLedger::default(),
            query_ledger: EngineQueryLedger::default(),
            gpu_frame_ledger: EngineGpuFrameLedger {
                scheduler_owned_queue_submits,
                private_queue_submits,
                resident_cache_hits: 0,
                resident_cache_misses: 0,
                upload_bytes: scene_reupload_bytes,
                readback_ticket_count: 0,
                attachment_cpu_bounce_count: 0,
                cpu_screen_sample_allocations: 0,
                violations: if private_queue_submits > 0 {
                    vec!["engine_frame_private_gpu_submit_detected".to_string()]
                } else {
                    Vec::new()
                },
            },
            budget_directives: EngineBudgetDirectives::default(),
            frame_wall_time_micros: timeline_spans.frame_wall_time_micros,
            cpu_critical_path_micros,
            gpu_critical_path_micros: (gpu_critical_path_micros > 0)
                .then_some(gpu_critical_path_micros),
            present_wait_micros: duration_for_domain(
                &timeline.spans,
                EngineSpanDomain::PresentWait,
            ),
            gpu_wait_micros: duration_for_domain(&timeline.spans, EngineSpanDomain::GpuWait),
            readback_wait_micros: duration_for_domain(
                &timeline.spans,
                EngineSpanDomain::ReadbackWait,
            ),
            steady_state_fps: fps_from_frame_time_micros(timeline_spans.frame_wall_time_micros),
            gpu_runtime: GpuRuntimeMetrics {
                queue_submit_count,
                readback_bytes: hot_path_readback_bytes,
                scene_reupload_bytes,
                ..GpuRuntimeMetrics::default()
            },
            timeline_version: timeline.version,
            critical_path_span_ids: timeline.critical_path_span_ids.clone(),
            cpu_busy_micros,
            gpu_busy_micros,
            overlap_ratio,
            queue_submission_spans: timeline.queue_submission_spans.clone(),
            subsystem_span_ranges: timeline.subsystem_span_ranges.clone(),
            timeline_spans: timeline.spans.clone(),
            subsystems: reports,
            future_subsystem_reserve: EngineFutureReserveReport::default(),
            active_degradations,
            violations,
            latency,
            closure_findings: Vec::new(),
        };

        if let Some(budget) = &self.budget {
            let mut governor = EngineBudgetGovernor;
            let decision = governor.observe_engine_frame(&report, budget);
            extend_unique_strings(&mut report.violations, decision.violations);
            extend_unique_strings(
                &mut report.active_degradations,
                decision.active_degradations,
            );
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

#[derive(Debug, Clone, Default)]
struct TimelineDerivedMetrics {
    frame_wall_time_micros: u128,
}

fn validate_unique_subsystem_kinds(plans: &[EngineSubsystemPlan]) -> Result<(), EngineFrameError> {
    let mut seen = std::collections::BTreeSet::<EngineSubsystemKind>::new();
    for plan in plans {
        let kind = &plan.descriptor.kind;
        if !seen.insert(kind.clone()) {
            return Err(EngineFrameError::DuplicateSubsystemKind(kind.clone()));
        }
    }
    Ok(())
}

fn wire_subsystem_dependencies(graph: &mut EngineFrameGraph) -> Result<(), EngineFrameError> {
    let subsystem_order = topological_subsystem_order(&graph.subsystem_plans)?;
    let mut terminal_jobs_by_kind = BTreeMap::<EngineSubsystemKind, Vec<EngineJobHandle>>::new();
    for subsystem_index in subsystem_order {
        let descriptor = graph.subsystem_plans[subsystem_index].descriptor.clone();
        for dependency_kind in &descriptor.runs_after {
            let Some(terminal_jobs) = terminal_jobs_by_kind.get(dependency_kind) else {
                return Err(EngineFrameError::DependencyCycle);
            };
            for root_job in graph.subsystem_plans[subsystem_index].root_jobs.clone() {
                let Some(job_index) = graph.jobs.iter().position(|job| job.handle == root_job)
                else {
                    return Err(EngineFrameError::MissingJob);
                };
                for terminal_job in terminal_jobs {
                    if !graph.jobs[job_index].depends_on.contains(terminal_job) {
                        graph.jobs[job_index].depends_on.push(*terminal_job);
                    }
                }
            }
        }
        terminal_jobs_by_kind.insert(
            descriptor.kind,
            graph.subsystem_plans[subsystem_index].terminal_jobs.clone(),
        );
    }
    Ok(())
}

fn execute_graph(
    graph: &mut EngineFrameGraph,
    executor: &EngineExecutor,
) -> Result<(EngineFrameTimeline, TimelineDerivedMetrics), EngineFrameError> {
    let job_indices = graph
        .jobs
        .iter()
        .enumerate()
        .map(|(index, job)| (job.handle, index))
        .collect::<BTreeMap<_, _>>();
    let mut indegree = vec![0usize; graph.jobs.len()];
    let mut outgoing = vec![Vec::<usize>::new(); graph.jobs.len()];
    for (index, job) in graph.jobs.iter().enumerate() {
        for dependency in &job.depends_on {
            let Some(&dependency_index) = job_indices.get(dependency) else {
                return Err(EngineFrameError::MissingJob);
            };
            indegree[index] += 1;
            outgoing[dependency_index].push(index);
        }
    }

    let frame_started = std::time::Instant::now();
    let mut ready = graph
        .jobs
        .iter()
        .enumerate()
        .filter_map(|(index, _)| (indegree[index] == 0).then_some(index))
        .collect::<Vec<_>>();
    ready.sort_by_key(|index| graph.jobs[*index].handle.0);
    let mut spans_by_job = vec![None::<EngineSpanRecord>; graph.jobs.len()];
    let mut next_span_id = 0u32;
    let mut completed_jobs = 0usize;

    while !ready.is_empty() {
        let wave = std::mem::take(&mut ready);
        let mut tasks = Vec::with_capacity(wave.len());
        for index in &wave {
            let job = &mut graph.jobs[*index];
            let task = job.task.take().unwrap_or_else(|| Box::new(|| Ok(())));
            tasks.push(EngineTask {
                task_id: job.handle.0 as u64,
                label: job.label.clone(),
                affinity: runtime_affinity(job.affinity),
                order_key: job.handle.0 as u64,
                task,
            });
        }
        let (outcomes, executor_report) = executor
            .execute_batch(tasks)
            .map_err(EngineFrameError::Message)?;
        if executor_report.tokio_runtime_violations > 0 {
            return Err(EngineFrameError::Message(
                "engine executor observed frame-critical work on a Tokio runtime thread"
                    .to_string(),
            ));
        }

        for outcome in outcomes {
            let Some(&job_index) = job_indices.get(&EngineJobHandle(outcome.task_id as u32)) else {
                return Err(EngineFrameError::MissingJob);
            };
            if let Some(error) = outcome.error {
                return Err(EngineFrameError::Message(format!(
                    "engine frame job '{}' failed: {error}",
                    outcome.label
                )));
            }
            let job = &graph.jobs[job_index];
            let dependency_end_micros = job
                .depends_on
                .iter()
                .filter_map(|dependency| {
                    job_indices
                        .get(dependency)
                        .and_then(|dependency_index| spans_by_job[*dependency_index].as_ref())
                        .map(|span| span.ended_micros)
                })
                .max()
                .unwrap_or(0);
            let started_micros = if job.simulated_elapsed_micros.is_some() {
                dependency_end_micros
            } else {
                outcome
                    .started_at
                    .duration_since(frame_started)
                    .as_micros()
                    .max(dependency_end_micros)
            };
            let ended_micros = job
                .simulated_elapsed_micros
                .map(|elapsed| started_micros.saturating_add(elapsed))
                .unwrap_or_else(|| outcome.ended_at.duration_since(frame_started).as_micros())
                .max(started_micros);
            spans_by_job[job_index] = Some(EngineSpanRecord {
                id: EngineSpanId(next_span_id),
                subsystem: job.subsystem.clone(),
                label: job.label.clone(),
                domain: job.domain,
                started_micros,
                ended_micros,
                thread_name: outcome.thread_name,
                queue_submission: job.queue_submission,
            });
            next_span_id = next_span_id.saturating_add(1);
            completed_jobs += 1;
            for target in &outgoing[job_index] {
                indegree[*target] = indegree[*target].saturating_sub(1);
            }
        }

        ready = indegree
            .iter()
            .enumerate()
            .filter_map(|(index, degree)| {
                (*degree == 0 && spans_by_job[index].is_none()).then_some(index)
            })
            .collect::<Vec<_>>();
        ready.sort_by_key(|index| graph.jobs[*index].handle.0);
    }

    if completed_jobs != graph.jobs.len() {
        return Err(EngineFrameError::DependencyCycle);
    }

    let spans = spans_by_job
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or(EngineFrameError::MissingJob)?;
    let critical_path_span_ids = critical_path_span_ids(&graph.jobs, &job_indices, &spans);
    let queue_submission_spans = spans
        .iter()
        .filter_map(|span| span.queue_submission.then_some(span.id))
        .collect::<Vec<_>>();
    let subsystem_span_ranges = subsystem_span_ranges(&graph.subsystem_plans, &spans);
    let frame_wall_time_micros = spans
        .iter()
        .map(|span| span.ended_micros)
        .max()
        .unwrap_or_default();
    Ok((
        EngineFrameTimeline {
            version: ENGINE_FRAME_TIMELINE_VERSION,
            critical_path_span_ids,
            queue_submission_spans,
            subsystem_span_ranges,
            spans,
        },
        TimelineDerivedMetrics {
            frame_wall_time_micros,
        },
    ))
}

fn critical_path_span_ids(
    jobs: &[EngineJobSpec],
    job_indices: &BTreeMap<EngineJobHandle, usize>,
    spans: &[EngineSpanRecord],
) -> Vec<EngineSpanId> {
    if jobs.is_empty() {
        return Vec::new();
    }
    let mut predecessor = vec![None::<usize>; jobs.len()];
    let mut best_end = vec![0_u128; jobs.len()];
    let order =
        topological_job_order(jobs, job_indices).unwrap_or_else(|_| (0..jobs.len()).collect());
    for index in order {
        let span = &spans[index];
        let mut best_parent_end = 0_u128;
        let mut best_parent = None;
        for dependency in &jobs[index].depends_on {
            let Some(&dependency_index) = job_indices.get(dependency) else {
                continue;
            };
            let candidate_end =
                best_end[dependency_index].max(spans[dependency_index].ended_micros);
            if candidate_end >= best_parent_end {
                best_parent_end = candidate_end;
                best_parent = Some(dependency_index);
            }
        }
        predecessor[index] = best_parent;
        best_end[index] = span.ended_micros.max(best_parent_end);
    }
    let Some((mut cursor, _)) = spans
        .iter()
        .enumerate()
        .max_by_key(|(_, span)| span.ended_micros)
    else {
        return Vec::new();
    };
    let mut path = Vec::new();
    loop {
        path.push(spans[cursor].id);
        if let Some(parent) = predecessor[cursor] {
            cursor = parent;
        } else {
            break;
        }
    }
    path.reverse();
    path
}

fn subsystem_span_ranges(
    subsystem_plans: &[EngineSubsystemPlan],
    spans: &[EngineSpanRecord],
) -> Vec<EngineSubsystemSpanRange> {
    subsystem_plans
        .iter()
        .map(|plan| {
            let subsystem_spans = spans
                .iter()
                .filter(|span| span.subsystem == plan.descriptor.kind)
                .collect::<Vec<_>>();
            EngineSubsystemSpanRange {
                kind: plan.descriptor.kind.clone(),
                start_span_id: subsystem_spans.first().map(|span| span.id),
                end_span_id: subsystem_spans.last().map(|span| span.id),
            }
        })
        .collect()
}

/// Derive motion-to-photon stages from the executed job timeline using real
/// span boundaries (RFC 0011 H3 acceptance: honest measurements).
///
/// The contract is computed from actual `started_micros` / `ended_micros`
/// boundaries rather than from a proportional share of wall time. The five
/// stages are derived as follows:
///
/// 1. `event_arrival_to_state_advance_nanos`: from the earliest input
///    event monotonic timestamp (when known) to the StateAdvance input
///    sample timestamp. This requires a compatible monotonic
///    `state_advance_input_sample_nanos`; otherwise we fall back to
///    (state-advance-start - frame-start), i.e. frame-relative state-start
///    latency. We do not compare monotonic event timestamps directly to
///    frame-relative scheduler spans.
/// 2. `state_advance_to_render_submit_nanos`: from the end of `StateAdvance`
///    to the first GPU/queue-submission span (or, lacking GPU work, to the
///    end of the last CPU-side post-state span).
/// 3. `render_submit_to_gpu_complete_nanos`: total elapsed in GPU domain
///    spans plus any `GpuWait` time stacked on the critical path.
/// 4. `gpu_complete_to_present_callback_nanos`: total `PresentWait` domain
///    duration.
/// 5. `estimated_present_to_photons_nanos`: panel/scan-out latency estimate
///    provided by the host/presentation adapter.
fn motion_to_photon_contract_from_timeline(
    timeline: &EngineFrameTimeline,
    frame_wall_time_micros: u128,
    earliest_input_arrival_nanos: Option<u64>,
    state_advance_input_sample_nanos: Option<u64>,
    estimated_present_to_photons_nanos: u64,
) -> MotionToPhotonContract {
    if frame_wall_time_micros == 0 && timeline.spans.is_empty() {
        return MotionToPhotonContract::synthetic_idle();
    }
    let spans = &timeline.spans;
    let frame_start_micros = spans
        .iter()
        .map(|span| span.started_micros)
        .min()
        .unwrap_or(0);
    let frame_end_micros = spans
        .iter()
        .map(|span| span.ended_micros)
        .max()
        .unwrap_or(frame_start_micros);

    let state_advance_start_micros = spans
        .iter()
        .filter(|span| span.subsystem == EngineSubsystemKind::StateAdvance)
        .map(|span| span.started_micros)
        .min();
    let state_advance_end_micros = spans
        .iter()
        .filter(|span| span.subsystem == EngineSubsystemKind::StateAdvance)
        .map(|span| span.ended_micros)
        .max();

    let render_submit_start_micros = spans
        .iter()
        .filter(|span| span.queue_submission || span.domain == EngineSpanDomain::Gpu)
        .map(|span| span.started_micros)
        .min();
    let gpu_complete_end_micros = spans
        .iter()
        .filter(|span| span.domain == EngineSpanDomain::Gpu)
        .map(|span| span.ended_micros)
        .max();
    let present_start_micros = spans
        .iter()
        .filter(|span| span.domain == EngineSpanDomain::PresentWait)
        .map(|span| span.started_micros)
        .min();
    let present_end_micros = spans
        .iter()
        .filter(|span| span.domain == EngineSpanDomain::PresentWait)
        .map(|span| span.ended_micros)
        .max();

    let mu_to_ns = |mu: u128| (mu.saturating_mul(1000)).min(u64::MAX as u128) as u64;

    let frame_start_nanos = mu_to_ns(frame_start_micros);
    let state_start_nanos = state_advance_start_micros.map(mu_to_ns);
    let state_end_nanos = state_advance_end_micros.map(mu_to_ns);
    let render_submit_nanos = render_submit_start_micros.map(mu_to_ns);
    let gpu_end_nanos = gpu_complete_end_micros.map(mu_to_ns);
    let present_start_nanos = present_start_micros.map(mu_to_ns);
    let present_end_nanos = present_end_micros.map(mu_to_ns);

    // Stage 1: event arrival -> StateAdvance input sample
    let stage1 = match (
        state_start_nanos,
        earliest_input_arrival_nanos,
        state_advance_input_sample_nanos,
    ) {
        (_, Some(arrival), Some(sample)) => sample.saturating_sub(arrival),
        (Some(state_start), _, _) => state_start.saturating_sub(frame_start_nanos),
        (None, _, _) => 0,
    };
    // Stage 2: state_advance_end -> render_submit_start
    let stage2 = match (state_end_nanos, render_submit_nanos) {
        (Some(state_end), Some(submit)) if submit > state_end => submit - state_end,
        (Some(state_end), None) => mu_to_ns(frame_end_micros).saturating_sub(state_end),
        _ => 0,
    };
    // Stage 3: render_submit -> gpu_complete
    let stage3 = match (render_submit_nanos, gpu_end_nanos) {
        (Some(submit), Some(end)) if end > submit => end - submit,
        _ => duration_for_domain(spans, EngineSpanDomain::Gpu)
            .saturating_add(duration_for_domain(spans, EngineSpanDomain::GpuWait))
            .saturating_mul(1000)
            .min(u64::MAX as u128) as u64,
    };
    // Stage 4: gpu_complete -> present_callback (return)
    let stage4 = match (gpu_end_nanos, present_end_nanos, present_start_nanos) {
        (Some(gpu_end), Some(present_end), _) if present_end > gpu_end => present_end - gpu_end,
        (None, Some(present_end), Some(present_start)) => present_end.saturating_sub(present_start),
        _ => duration_for_domain(spans, EngineSpanDomain::PresentWait)
            .saturating_mul(1000)
            .min(u64::MAX as u128) as u64,
    };

    let mut contract = MotionToPhotonContract {
        event_arrival_to_state_advance_nanos: stage1,
        state_advance_to_render_submit_nanos: stage2,
        render_submit_to_gpu_complete_nanos: stage3,
        gpu_complete_to_present_callback_nanos: stage4,
        estimated_present_to_photons_nanos,
        total_estimate_nanos: 0,
        measurement_quality: if earliest_input_arrival_nanos.is_none()
            && present_end_nanos.is_none()
            && gpu_end_nanos.is_none()
        {
            MeasurementQuality::Synthetic
        } else {
            MeasurementQuality::EstimatedFromCpuClock
        },
    };
    contract.recompute_total();
    contract
}

fn critical_path_split(
    timeline: &EngineFrameTimeline,
    spans: &TimelineDerivedMetrics,
) -> (u128, u128) {
    let critical_spans = timeline
        .critical_path_span_ids
        .iter()
        .filter_map(|id| timeline.spans.iter().find(|span| span.id == *id))
        .collect::<Vec<_>>();
    let cpu = critical_spans
        .iter()
        .filter(|span| {
            matches!(
                span.domain,
                EngineSpanDomain::Cpu | EngineSpanDomain::External
            )
        })
        .map(|span| span.elapsed_micros())
        .sum::<u128>()
        .min(spans.frame_wall_time_micros);
    let gpu = critical_spans
        .iter()
        .filter(|span| {
            matches!(
                span.domain,
                EngineSpanDomain::Gpu
                    | EngineSpanDomain::GpuWait
                    | EngineSpanDomain::ReadbackWait
                    | EngineSpanDomain::PresentWait
            )
        })
        .map(|span| span.elapsed_micros())
        .sum::<u128>()
        .min(spans.frame_wall_time_micros);
    (cpu, gpu)
}

fn duration_for_domain(spans: &[EngineSpanRecord], domain: EngineSpanDomain) -> u128 {
    busy_duration_for_domains(spans, &[domain])
}

fn busy_duration_for_domains(spans: &[EngineSpanRecord], domains: &[EngineSpanDomain]) -> u128 {
    let mut intervals = spans
        .iter()
        .filter(|span| domains.contains(&span.domain))
        .map(|span| (span.started_micros, span.ended_micros))
        .collect::<Vec<_>>();
    union_duration(&mut intervals)
}

fn overlap_ratio(
    cpu_busy_micros: u128,
    gpu_busy_micros: u128,
    frame_wall_time_micros: u128,
) -> f32 {
    if frame_wall_time_micros == 0 {
        return 0.0;
    }
    let overlap = cpu_busy_micros
        .saturating_add(gpu_busy_micros)
        .saturating_sub(frame_wall_time_micros)
        .min(frame_wall_time_micros);
    overlap as f32 / frame_wall_time_micros as f32
}

fn union_duration(intervals: &mut Vec<(u128, u128)>) -> u128 {
    if intervals.is_empty() {
        return 0;
    }
    intervals.sort_unstable_by_key(|interval| interval.0);
    let mut total = 0_u128;
    let mut current = intervals[0];
    for interval in intervals.iter().copied().skip(1) {
        if interval.0 <= current.1 {
            current.1 = current.1.max(interval.1);
        } else {
            total = total.saturating_add(current.1.saturating_sub(current.0));
            current = interval;
        }
    }
    total.saturating_add(current.1.saturating_sub(current.0))
}

fn topological_subsystem_order(
    subsystem_plans: &[EngineSubsystemPlan],
) -> Result<Vec<usize>, EngineFrameError> {
    let descriptors = subsystem_plans
        .iter()
        .map(|plan| &plan.descriptor)
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
        .collect::<Vec<_>>();
    ready.sort_unstable();
    let mut order = Vec::with_capacity(descriptors.len());
    while let Some(index) = ready.first().copied() {
        ready.remove(0);
        order.push(index);
        for target in &outgoing[index] {
            indegree[*target] = indegree[*target].saturating_sub(1);
            if indegree[*target] == 0 {
                ready.push(*target);
                ready.sort_unstable();
            }
        }
    }
    if order.len() != descriptors.len() {
        return Err(EngineFrameError::DependencyCycle);
    }
    Ok(order)
}

fn topological_job_order(
    jobs: &[EngineJobSpec],
    job_indices: &BTreeMap<EngineJobHandle, usize>,
) -> Result<Vec<usize>, EngineFrameError> {
    let mut indegree = vec![0usize; jobs.len()];
    let mut outgoing = vec![Vec::<usize>::new(); jobs.len()];
    for (index, job) in jobs.iter().enumerate() {
        for dependency in &job.depends_on {
            let Some(&dependency_index) = job_indices.get(dependency) else {
                return Err(EngineFrameError::MissingJob);
            };
            indegree[index] += 1;
            outgoing[dependency_index].push(index);
        }
    }
    let mut ready = indegree
        .iter()
        .enumerate()
        .filter_map(|(index, degree)| (*degree == 0).then_some(index))
        .collect::<Vec<_>>();
    ready.sort_by_key(|index| jobs[*index].handle.0);
    let mut order = Vec::with_capacity(jobs.len());
    while let Some(index) = ready.first().copied() {
        ready.remove(0);
        order.push(index);
        for target in &outgoing[index] {
            indegree[*target] = indegree[*target].saturating_sub(1);
            if indegree[*target] == 0 {
                ready.push(*target);
                ready.sort_by_key(|job_index| jobs[*job_index].handle.0);
            }
        }
    }
    if order.len() != jobs.len() {
        return Err(EngineFrameError::DependencyCycle);
    }
    Ok(order)
}

fn runtime_affinity(affinity: EngineJobAffinity) -> EngineTaskAffinity {
    match affinity {
        EngineJobAffinity::Cpu => EngineTaskAffinity::Cpu,
        EngineJobAffinity::Gpu => EngineTaskAffinity::Gpu,
        EngineJobAffinity::External => EngineTaskAffinity::External,
    }
}

fn extend_unique_strings(target: &mut Vec<String>, values: impl IntoIterator<Item = String>) {
    for value in values {
        if !target.contains(&value) {
            target.push(value);
        }
    }
}

fn micros_to_ms(value: u128) -> f32 {
    value as f32 / 1_000.0
}

fn ms_to_micros(value: f32) -> u128 {
    (value.max(0.0) * 1_000.0).round() as u128
}

fn fps_from_frame_time_micros(frame_time_micros: u128) -> f64 {
    if frame_time_micros == 0 {
        0.0
    } else {
        1_000_000.0 / frame_time_micros as f64
    }
}

fn observed_engine_frame_queue_submit_count(subsystem_reports: &[EngineSubsystemReport]) -> u32 {
    subsystem_reports.iter().fold(0_u32, |total, report| {
        total.saturating_add(report.queue_submit_count)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn motion_to_photon_contract_includes_display_latency_estimate() {
        let timeline = EngineFrameTimeline {
            version: ENGINE_FRAME_TIMELINE_VERSION,
            critical_path_span_ids: Vec::new(),
            queue_submission_spans: Vec::new(),
            subsystem_span_ranges: Vec::new(),
            spans: vec![
                EngineSpanRecord {
                    id: EngineSpanId(0),
                    subsystem: EngineSubsystemKind::StateAdvance,
                    label: "state_advance.advance".to_string(),
                    domain: EngineSpanDomain::Cpu,
                    started_micros: 0,
                    ended_micros: 100,
                    thread_name: "test".to_string(),
                    queue_submission: false,
                },
                EngineSpanRecord {
                    id: EngineSpanId(1),
                    subsystem: EngineSubsystemKind::Presentation,
                    label: "presentation.submit".to_string(),
                    domain: EngineSpanDomain::Gpu,
                    started_micros: 150,
                    ended_micros: 250,
                    thread_name: "test".to_string(),
                    queue_submission: true,
                },
                EngineSpanRecord {
                    id: EngineSpanId(2),
                    subsystem: EngineSubsystemKind::Presentation,
                    label: "presentation.present".to_string(),
                    domain: EngineSpanDomain::PresentWait,
                    started_micros: 250,
                    ended_micros: 300,
                    thread_name: "test".to_string(),
                    queue_submission: false,
                },
            ],
        };

        let contract =
            motion_to_photon_contract_from_timeline(&timeline, 300, None, None, 16_666_667);

        assert_eq!(contract.estimated_present_to_photons_nanos, 16_666_667);
        assert_eq!(
            contract.total_estimate_nanos,
            contract
                .event_arrival_to_state_advance_nanos
                .saturating_add(contract.state_advance_to_render_submit_nanos)
                .saturating_add(contract.render_submit_to_gpu_complete_nanos)
                .saturating_add(contract.gpu_complete_to_present_callback_nanos)
                .saturating_add(16_666_667)
        );
    }
}
