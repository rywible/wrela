//! Engine-frame adapter for physics (RFC 0011 Phase 67).

use super::{
    EngineFrameContext, EngineFrameError, EngineFrameTimeline, EngineGpuTimingPolicy,
    EngineGraphBuilder, EngineJobAffinity, EngineMeasurementPolicy, EngineRuntimeSource,
    EngineSpanDomain, EngineSubsystemAdapter, EngineSubsystemDescriptor, EngineSubsystemKind,
    EngineSubsystemPlan, EngineSubsystemReport,
};
use crate::physics_exec::{PhysicsFrameReport, PhysicsSolver};
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
    fn prepare_frame(&mut self, _input: &super::EngineFrameInput) {
        // RFC 0011 H7: clear last frame's report so a build that aborts before
        // `solver.step` can't surface stale findings/work_items.
        if let Ok(mut report) = self.report.lock() {
            *report = None;
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
        let job_affinity = if collision_backed {
            EngineJobAffinity::Gpu
        } else {
            EngineJobAffinity::Cpu
        };
        let span_domain = if collision_backed {
            EngineSpanDomain::Gpu
        } else {
            EngineSpanDomain::Cpu
        };
        let job = builder.add_job(
            EngineSubsystemKind::Physics,
            "physics.xpbd".to_string(),
            job_affinity,
            span_domain,
            Vec::new(),
            false,
            move || {
                let dt = *dt_slot
                    .lock()
                    .map_err(|_| EngineFrameError::Message("physics dt lock poisoned".into()))?;
                let report = solver
                    .lock()
                    .map_err(|_| EngineFrameError::Message("physics solver lock poisoned".into()))?
                    .step(dt)
                    .map_err(|err| EngineFrameError::Message(err.to_string()))?;
                *report_slot.lock().map_err(|_| {
                    EngineFrameError::Message("physics report lock poisoned".into())
                })? = Some(report);
                Ok(())
            },
        );
        let report_for_builder = Arc::clone(&self.report);
        Ok(EngineSubsystemPlan::new(
            descriptor.clone(),
            vec![job],
            vec![job],
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
                    cpu_critical_path_micros: if collision_backed { 0 } else { executed },
                    gpu_critical_path_micros: if collision_backed {
                        Some(executed)
                    } else {
                        None
                    },
                    executed_wall_time_micros: executed,
                    self_reported_runtime_micros: Some(executed),
                    orchestration_gap_micros: 0,
                    measurement_policy: EngineMeasurementPolicy {
                        runtime_source: EngineRuntimeSource::TimelineSpans,
                        gpu_timing: if collision_backed {
                            EngineGpuTimingPolicy::RuntimeProxy
                        } else {
                            EngineGpuTimingPolicy::Disabled
                        },
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
