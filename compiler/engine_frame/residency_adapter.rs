//! Engine-frame adapter for region residency (RFC 0011 Phase 66).

use super::{
    EngineFrameContext, EngineFrameError, EngineFrameTimeline, EngineGpuTimingPolicy,
    EngineGraphBuilder, EngineJobAffinity, EngineMeasurementPolicy, EngineResourceAccess,
    EngineResourceAccessMode, EngineResourceEpochState, EngineResourceId, EngineResourceResidency,
    EngineResourceState, EngineRuntimeSource, EngineSpanDomain, EngineSubsystemAdapter,
    EngineSubsystemDescriptor, EngineSubsystemKind, EngineSubsystemPlan, EngineSubsystemReport,
};
use crate::residency::follow::FollowTarget;
use crate::residency::{RegionResidencyService, ResidencyGpuCache, ResidencyPlan, ResidencyReport};
use crate::state_advance::{SimulationTick, StateAdvanceResult};
use crate::world_identity::WorldSnapshotHandle;
use std::sync::{Arc, Mutex};

/// Where residency reads the active world snapshot for this frame.
#[derive(Clone)]
pub enum ResidencyWorldBinding {
    /// Tests and offline tooling: fixed snapshot/tick captured at construction.
    Fixed {
        snapshot: WorldSnapshotHandle,
        tick: SimulationTick,
    },
    /// Live path: read `to_snapshot` after state advance from the shared runtime slot.
    StateOutcome(Arc<Mutex<Option<Result<StateAdvanceResult, EngineFrameError>>>>),
}

pub struct ResidencySubsystemAdapter {
    service: Arc<Mutex<RegionResidencyService>>,
    gpu_cache: Arc<Mutex<ResidencyGpuCache>>,
    target: FollowTarget,
    world: ResidencyWorldBinding,
    report: Arc<Mutex<Option<ResidencyReport>>>,
}

impl ResidencySubsystemAdapter {
    pub fn new(
        service: RegionResidencyService,
        target: FollowTarget,
        snapshot: WorldSnapshotHandle,
        tick: SimulationTick,
    ) -> Self {
        Self {
            service: Arc::new(Mutex::new(service)),
            gpu_cache: Arc::new(Mutex::new(ResidencyGpuCache::default())),
            target,
            world: ResidencyWorldBinding::Fixed { snapshot, tick },
            report: Arc::new(Mutex::new(None)),
        }
    }

    /// Use with [`crate::engine_frame::EngineFrameRuntime::state_advance_outcome_slot`].
    pub fn with_state_outcome(
        service: RegionResidencyService,
        target: FollowTarget,
        outcome: Arc<Mutex<Option<Result<StateAdvanceResult, EngineFrameError>>>>,
    ) -> Self {
        Self {
            service: Arc::new(Mutex::new(service)),
            gpu_cache: Arc::new(Mutex::new(ResidencyGpuCache::default())),
            target,
            world: ResidencyWorldBinding::StateOutcome(outcome),
            report: Arc::new(Mutex::new(None)),
        }
    }

    pub fn gpu_cache(&self) -> Arc<Mutex<ResidencyGpuCache>> {
        Arc::clone(&self.gpu_cache)
    }
}

impl EngineSubsystemAdapter for ResidencySubsystemAdapter {
    fn prepare_frame(&mut self, _input: &super::EngineFrameInput) {
        // RFC 0011 H7: residency report is per-frame; clear so the report
        // builder cannot pick up stale `admit/evict/deferred` counts.
        if let Ok(mut report) = self.report.lock() {
            *report = None;
        }
    }

    fn build(
        &mut self,
        builder: &mut EngineGraphBuilder,
    ) -> Result<EngineSubsystemPlan, EngineFrameError> {
        let descriptor = EngineSubsystemDescriptor {
            kind: EngineSubsystemKind::Residency,
            label: "residency".to_string(),
            runs_after: vec![
                EngineSubsystemKind::StateAdvance,
                EngineSubsystemKind::System,
            ],
            requires_gpu: false,
            allows_hot_path_readback: false,
        };
        let plan_slot = Arc::new(Mutex::new(None::<ResidencyPlan>));
        let service_for_plan = Arc::clone(&self.service);
        let target = self.target;
        let world_binding = self.world.clone();
        let plan_slot_for_job = Arc::clone(&plan_slot);
        let plan_job = builder.add_job(
            EngineSubsystemKind::Residency,
            "residency.plan".to_string(),
            EngineJobAffinity::Cpu,
            EngineSpanDomain::Cpu,
            Vec::new(),
            false,
            move || {
                let (snapshot, tick) = match &world_binding {
                    ResidencyWorldBinding::Fixed { snapshot, tick } => (snapshot.clone(), *tick),
                    ResidencyWorldBinding::StateOutcome(slot) => {
                        let guard = slot.lock().map_err(|_| {
                            EngineFrameError::Message("state outcome lock poisoned".into())
                        })?;
                        let Some(result) = guard.as_ref() else {
                            return Err(EngineFrameError::Message(
                                "residency plan before state advance outcome".into(),
                            ));
                        };
                        let ok = result.as_ref().map_err(|e| {
                            EngineFrameError::Message(format!(
                                "state advance failed before residency plan: {e}"
                            ))
                        })?;
                        (
                            ok.transition_record.to_snapshot.clone(),
                            ok.transition_record.current_clock.simulation_tick,
                        )
                    }
                };
                let plan = service_for_plan
                    .lock()
                    .map_err(|_| {
                        EngineFrameError::Message("residency service lock poisoned".into())
                    })?
                    .plan(target, &snapshot, tick);
                *plan_slot_for_job.lock().map_err(|_| {
                    EngineFrameError::Message("residency plan lock poisoned".into())
                })? = Some(plan);
                Ok(())
            },
        );
        let service_for_apply = Arc::clone(&self.service);
        let cache_for_apply = Arc::clone(&self.gpu_cache);
        let report_slot = Arc::clone(&self.report);
        let world_for_apply = self.world.clone();
        let apply_job = builder.add_job(
            EngineSubsystemKind::Residency,
            "residency.apply".to_string(),
            EngineJobAffinity::Cpu,
            EngineSpanDomain::Cpu,
            vec![plan_job],
            false,
            move || {
                let plan = plan_slot
                    .lock()
                    .map_err(|_| EngineFrameError::Message("residency plan lock poisoned".into()))?
                    .clone()
                    .ok_or_else(|| EngineFrameError::Message("missing residency plan".into()))?;
                let (snapshot, tick) = match &world_for_apply {
                    ResidencyWorldBinding::Fixed { snapshot, tick } => (snapshot.clone(), *tick),
                    ResidencyWorldBinding::StateOutcome(slot) => {
                        let guard = slot.lock().map_err(|_| {
                            EngineFrameError::Message("state outcome lock poisoned".into())
                        })?;
                        let Some(Ok(ok)) = guard.as_ref() else {
                            return Err(EngineFrameError::Message(
                                "residency apply without successful state advance".into(),
                            ));
                        };
                        (
                            ok.transition_record.to_snapshot.clone(),
                            ok.transition_record.current_clock.simulation_tick,
                        )
                    }
                };
                let mut gpu_cache = cache_for_apply.lock().map_err(|_| {
                    EngineFrameError::Message("residency GPU cache lock poisoned".into())
                })?;
                let report = service_for_apply
                    .lock()
                    .map_err(|_| {
                        EngineFrameError::Message("residency service lock poisoned".into())
                    })?
                    .apply_with_gpu_cache(&plan, &snapshot, tick, &mut gpu_cache)
                    .map_err(|err| EngineFrameError::Message(err.to_string()))?;
                *report_slot.lock().map_err(|_| {
                    EngineFrameError::Message("residency report lock poisoned".into())
                })? = Some(report);
                Ok(())
            },
        );
        let report_for_builder = Arc::clone(&self.report);
        Ok(EngineSubsystemPlan::new(
            descriptor.clone(),
            vec![plan_job],
            vec![apply_job],
            move |timeline: &EngineFrameTimeline, ctx: &mut EngineFrameContext| {
                let executed = timeline
                    .spans
                    .iter()
                    .filter(|span| span.subsystem == EngineSubsystemKind::Residency)
                    .map(|span| span.elapsed_micros())
                    .sum();
                let report = report_for_builder
                    .lock()
                    .map_err(|_| {
                        EngineFrameError::Message("residency report lock poisoned".into())
                    })?
                    .clone()
                    .unwrap_or_default();
                for region_id in &report.admitted_region_ids {
                    ctx.resource_accesses.push(EngineResourceAccess {
                        subsystem: descriptor.kind.clone(),
                        resource: EngineResourceId::ResidentRegion {
                            region_id: region_id.0.to_string(),
                            epoch: ctx.published_snapshot_epoch.unwrap_or_default(),
                        },
                        mode: EngineResourceAccessMode::Write,
                    });
                }
                for region_id in &report.resident_region_ids {
                    ctx.resource_accesses.push(EngineResourceAccess {
                        subsystem: descriptor.kind.clone(),
                        resource: EngineResourceId::ResidentRegion {
                            region_id: region_id.0.to_string(),
                            epoch: ctx.published_snapshot_epoch.unwrap_or_default(),
                        },
                        mode: EngineResourceAccessMode::Read,
                    });
                    ctx.resource_states.push(EngineResourceState {
                        resource: EngineResourceId::ResidentRegion {
                            region_id: region_id.0.to_string(),
                            epoch: ctx.published_snapshot_epoch.unwrap_or_default(),
                        },
                        residency: EngineResourceResidency::GpuResident,
                        epoch_state: EngineResourceEpochState::Valid {
                            epoch: ctx.published_snapshot_epoch.unwrap_or_default(),
                        },
                        producer: descriptor.kind.clone(),
                    });
                }
                Ok(EngineSubsystemReport {
                    kind: descriptor.kind.clone(),
                    label: descriptor.label.clone(),
                    work_items: u64::from(report.admitted + report.evicted + report.unchanged),
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
                    scene_reupload_bytes: report.bytes_uploaded,
                    timestamped_pass_count: 0,
                    timing_readback_bytes: 0,
                    wait_time_micros: 0,
                    notes: vec![format!(
                        "admit={} evict={} deferred={} gpu_cache_hits={} gpu_cache_misses={}",
                        report.admitted,
                        report.evicted,
                        report.deferred,
                        report.gpu_cache_hits,
                        report.gpu_cache_misses
                    )],
                })
            },
        ))
    }
}
