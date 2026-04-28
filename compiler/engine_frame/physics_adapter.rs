//! Engine-frame adapter for physics (RFC 0011 Phase 67).

use super::{
    EngineFrameContext, EngineFrameError, EngineFrameTimeline, EngineGpuTimingPolicy,
    EngineGraphBuilder, EngineJobAffinity, EngineMeasurementPolicy, EngineRuntimeSource,
    EngineSpanDomain, EngineSubsystemAdapter, EngineSubsystemDescriptor, EngineSubsystemKind,
    EngineSubsystemPlan, EngineSubsystemReport,
};
use crate::physics_exec::{PhysicsFrameReport, PhysicsSolver, PhysicsStepFrame};
use crate::physics_plan::PhysicsBackend;
use std::sync::{Arc, Mutex};

pub struct PhysicsSubsystemAdapter {
    solver: Arc<Mutex<PhysicsSolver>>,
    dt: Arc<Mutex<f32>>,
    report: Arc<Mutex<Option<PhysicsFrameReport>>>,
}

impl PhysicsSubsystemAdapter {
    pub fn new(solver: PhysicsSolver, dt: f32) -> Self {
        Self {
            solver: Arc::new(Mutex::new(solver)),
            dt: Arc::new(Mutex::new(dt)),
            report: Arc::new(Mutex::new(None)),
        }
    }

    pub fn solver(&self) -> Arc<Mutex<PhysicsSolver>> {
        Arc::clone(&self.solver)
    }

    /// Update the integration step (typically called by the host whenever the
    /// fixed timestep changes). Has no effect on in-flight frames.
    pub fn set_dt(&self, dt: f32) {
        if let Ok(mut guard) = self.dt.lock() {
            *guard = dt;
        }
    }
}

impl EngineSubsystemAdapter for PhysicsSubsystemAdapter {
    fn prepare_frame(&mut self, input: &super::EngineFrameInput) {
        // RFC 0011 H7: clear last frame's report so a build that aborts before
        // `solver.step` can't surface stale findings/work_items.
        if let Ok(mut report) = self.report.lock() {
            *report = None;
        }
        if let Ok(mut dt) = self.dt.lock() {
            *dt = input.frame_dt_seconds() as f32;
        }
    }

    fn build(
        &mut self,
        builder: &mut EngineGraphBuilder,
    ) -> Result<EngineSubsystemPlan, EngineFrameError> {
        let backend = self
            .solver
            .lock()
            .map_err(|_| EngineFrameError::Message("physics solver lock poisoned".into()))?
            .backend();
        let collision_backed = backend == PhysicsBackend::CollisionBacked;
        let descriptor = EngineSubsystemDescriptor {
            kind: EngineSubsystemKind::Physics,
            label: "physics".to_string(),
            runs_after: vec![
                EngineSubsystemKind::StateAdvance,
                EngineSubsystemKind::System,
            ],
            requires_gpu: collision_backed,
            allows_hot_path_readback: collision_backed,
        };
        let solver = Arc::clone(&self.solver);
        let report_slot = Arc::clone(&self.report);
        let dt_slot = Arc::clone(&self.dt);
        let step_frame = Arc::new(Mutex::new(None::<PhysicsStepFrame>));
        let substeps = self
            .solver
            .lock()
            .map_err(|_| EngineFrameError::Message("physics solver lock poisoned".into()))?
            .planned_substeps_per_frame();
        let job_affinity = EngineJobAffinity::Cpu;
        let span_domain = EngineSpanDomain::Cpu;
        let mut first_job = None;
        let mut previous = Vec::new();
        for _ in 0..substeps {
            let integrate = {
                let solver = Arc::clone(&solver);
                let step_frame = Arc::clone(&step_frame);
                let dt_slot = Arc::clone(&dt_slot);
                builder.add_job(
                    EngineSubsystemKind::Physics,
                    "physics.integrate".to_string(),
                    job_affinity,
                    span_domain,
                    previous,
                    false,
                    move || {
                        let mut solver = solver.lock().map_err(|_| {
                            EngineFrameError::Message("physics solver lock poisoned".into())
                        })?;
                        let mut frame = step_frame.lock().map_err(|_| {
                            EngineFrameError::Message("physics staged frame lock poisoned".into())
                        })?;
                        if frame.is_none() {
                            let dt = *dt_slot.lock().map_err(|_| {
                                EngineFrameError::Message("physics dt lock poisoned".into())
                            })?;
                            *frame = Some(solver.begin_frame(dt));
                        }
                        let frame = frame.as_mut().ok_or_else(|| {
                            EngineFrameError::Message("physics staged frame missing".into())
                        })?;
                        solver
                            .stage_integrate(frame)
                            .map_err(|err| EngineFrameError::Message(err.to_string()))
                    },
                )
            };
            if first_job.is_none() {
                first_job = Some(integrate);
            }
            let broadphase = {
                let solver = Arc::clone(&solver);
                let step_frame = Arc::clone(&step_frame);
                builder.add_job(
                    EngineSubsystemKind::Physics,
                    "physics.broadphase".to_string(),
                    job_affinity,
                    span_domain,
                    vec![integrate],
                    false,
                    move || {
                        run_physics_stage(&solver, &step_frame, |solver, frame| {
                            solver.stage_broadphase(frame)
                        })
                    },
                )
            };
            let detect_contacts = {
                let solver = Arc::clone(&solver);
                let step_frame = Arc::clone(&step_frame);
                builder.add_job(
                    EngineSubsystemKind::Physics,
                    "physics.detect_contacts".to_string(),
                    job_affinity,
                    span_domain,
                    vec![broadphase],
                    false,
                    move || {
                        run_physics_stage(&solver, &step_frame, |solver, frame| {
                            solver.stage_detect_contacts(frame)
                        })
                    },
                )
            };
            let solve_positions = {
                let solver = Arc::clone(&solver);
                let step_frame = Arc::clone(&step_frame);
                builder.add_job(
                    EngineSubsystemKind::Physics,
                    "physics.solve_positions".to_string(),
                    job_affinity,
                    span_domain,
                    vec![detect_contacts],
                    false,
                    move || {
                        run_physics_stage(&solver, &step_frame, |solver, frame| {
                            solver.stage_solve_positions(frame)
                        })
                    },
                )
            };
            let solve_velocities = {
                let solver = Arc::clone(&solver);
                let step_frame = Arc::clone(&step_frame);
                builder.add_job(
                    EngineSubsystemKind::Physics,
                    "physics.solve_velocities".to_string(),
                    job_affinity,
                    span_domain,
                    vec![solve_positions],
                    false,
                    move || {
                        run_physics_stage(&solver, &step_frame, |solver, frame| {
                            solver.stage_solve_velocities(frame)
                        })
                    },
                )
            };
            previous = vec![solve_velocities];
        }
        let move_fsm = {
            let solver = Arc::clone(&solver);
            let step_frame = Arc::clone(&step_frame);
            let report_slot = Arc::clone(&report_slot);
            builder.add_job(
                EngineSubsystemKind::Physics,
                "physics.move_fsm".to_string(),
                job_affinity,
                span_domain,
                previous,
                false,
                move || {
                    let mut solver = solver.lock().map_err(|_| {
                        EngineFrameError::Message("physics solver lock poisoned".into())
                    })?;
                    let mut frame_guard = step_frame.lock().map_err(|_| {
                        EngineFrameError::Message("physics staged frame lock poisoned".into())
                    })?;
                    let frame = frame_guard.as_mut().ok_or_else(|| {
                        EngineFrameError::Message("physics staged frame missing".into())
                    })?;
                    solver
                        .stage_move_fsm(frame)
                        .map_err(|err| EngineFrameError::Message(err.to_string()))?;
                    let report =
                        PhysicsSolver::finish_frame(frame_guard.take().ok_or_else(|| {
                            EngineFrameError::Message("physics staged frame missing".into())
                        })?);
                    *report_slot.lock().map_err(|_| {
                        EngineFrameError::Message("physics report lock poisoned".into())
                    })? = Some(report);
                    Ok(())
                },
            )
        };
        let report_for_builder = Arc::clone(&self.report);
        Ok(EngineSubsystemPlan::new(
            descriptor.clone(),
            vec![
                first_job
                    .ok_or_else(|| EngineFrameError::Message("physics stage graph empty".into()))?,
            ],
            vec![move_fsm],
            move |timeline: &EngineFrameTimeline, ctx: &mut EngineFrameContext| {
                let executed = timeline
                    .spans
                    .iter()
                    .filter(|span| span.subsystem == EngineSubsystemKind::Physics)
                    .map(|span| span.elapsed_micros())
                    .sum();
                let report = report_for_builder
                    .lock()
                    .map_err(|_| EngineFrameError::Message("physics report lock poisoned".into()))?
                    .clone()
                    .unwrap_or_default();
                ctx.violations.extend(report.findings.clone());
                Ok(EngineSubsystemReport {
                    kind: descriptor.kind.clone(),
                    label: descriptor.label.clone(),
                    work_items: u64::from(report.integrations + report.contacts_resolved),
                    cpu_critical_path_micros: executed,
                    gpu_critical_path_micros: None,
                    executed_wall_time_micros: executed,
                    self_reported_runtime_micros: Some(executed),
                    orchestration_gap_micros: 0,
                    measurement_policy: EngineMeasurementPolicy {
                        runtime_source: EngineRuntimeSource::TimelineSpans,
                        gpu_timing: EngineGpuTimingPolicy::Disabled,
                        hot_path_readback_allowed: collision_backed,
                        export_readback_allowed: false,
                    },
                    queue_submit_count: 0,
                    hot_path_readback_bytes: report.readback_bytes,
                    scene_reupload_bytes: 0,
                    timestamped_pass_count: 0,
                    timing_readback_bytes: 0,
                    wait_time_micros: report.contact_readback_micros,
                    notes: physics_report_notes(&report),
                })
            },
        ))
    }
}

fn run_physics_stage<F>(
    solver: &Arc<Mutex<PhysicsSolver>>,
    step_frame: &Arc<Mutex<Option<PhysicsStepFrame>>>,
    stage: F,
) -> Result<(), EngineFrameError>
where
    F: FnOnce(
        &mut PhysicsSolver,
        &mut PhysicsStepFrame,
    ) -> Result<(), crate::physics_exec::PhysicsError>,
{
    let mut solver = solver
        .lock()
        .map_err(|_| EngineFrameError::Message("physics solver lock poisoned".into()))?;
    let mut frame = step_frame
        .lock()
        .map_err(|_| EngineFrameError::Message("physics staged frame lock poisoned".into()))?;
    let frame = frame
        .as_mut()
        .ok_or_else(|| EngineFrameError::Message("physics staged frame missing".into()))?;
    stage(&mut solver, frame).map_err(|err| EngineFrameError::Message(err.to_string()))
}

fn physics_report_notes(report: &PhysicsFrameReport) -> Vec<String> {
    let mut notes = vec![format!(
        "substeps={} contacts={}",
        report.substeps, report.contacts_resolved
    )];
    if !report.collision_batches.is_empty() {
        notes.push(format!(
            "collision_batches={} contact_readback_bytes={}",
            report.collision_batches.len(),
            report.readback_bytes
        ));
    }
    notes
}
