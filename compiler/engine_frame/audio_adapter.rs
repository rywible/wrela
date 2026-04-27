//! Engine-frame audio snapshot publisher adapter (RFC 0011 Phase 68).
//!
//! The adapter is a long-lived object the host constructs once. Per-frame
//! state (the DSP plan to publish and the simulation tick to stamp it with)
//! lives behind shared `Arc<Mutex<...>>` slots so the host can update them
//! between frames without rebuilding the adapter (RFC 0011 H1 acceptance:
//! adapters pull frame state per build).

use super::{
    EngineFrameContext, EngineFrameError, EngineFrameInput, EngineFrameTimeline,
    EngineGpuTimingPolicy, EngineGraphBuilder, EngineJobAffinity, EngineMeasurementPolicy,
    EngineRuntimeSource, EngineSpanDomain, EngineSubsystemAdapter, EngineSubsystemDescriptor,
    EngineSubsystemKind, EngineSubsystemPlan, EngineSubsystemReport,
};
use crate::audio_exec::{AudioFrameReport, AudioSnapshotPublisher as Publisher};
use crate::audio_plan::AudioDspPlan;
use std::sync::{Arc, Mutex};

#[derive(Debug, Default, Clone)]
pub struct AudioAdapterFrameState {
    pub plan: AudioDspPlan,
    pub tick: u64,
}

pub struct AudioSnapshotPublisher {
    publisher: Arc<Publisher>,
    frame_state: Arc<Mutex<AudioAdapterFrameState>>,
    report: Arc<Mutex<Option<AudioFrameReport>>>,
    require_physics_ordering: bool,
}

impl AudioSnapshotPublisher {
    /// Long-lived constructor: caller owns the `frame_state` slot and updates
    /// it per frame.
    pub fn from_shared_state(
        publisher: Publisher,
        frame_state: Arc<Mutex<AudioAdapterFrameState>>,
    ) -> Self {
        Self {
            publisher: Arc::new(publisher),
            frame_state,
            report: Arc::new(Mutex::new(None)),
            require_physics_ordering: false,
        }
    }

    /// Compatibility constructor: bakes in an initial plan/tick. `prepare_frame`
    /// will continue to refresh the tick from the engine frame input each
    /// frame.
    pub fn new(publisher: Publisher, plan: AudioDspPlan, tick: u64) -> Self {
        let frame_state = Arc::new(Mutex::new(AudioAdapterFrameState { plan, tick }));
        Self::from_shared_state(publisher, frame_state)
    }

    pub fn frame_state(&self) -> Arc<Mutex<AudioAdapterFrameState>> {
        Arc::clone(&self.frame_state)
    }

    /// Mutate the in-flight plan from the host between frames.
    pub fn set_plan(&self, plan: AudioDspPlan) {
        if let Ok(mut state) = self.frame_state.lock() {
            state.plan = plan;
        }
    }

    pub fn with_physics_dependency(mut self) -> Self {
        self.require_physics_ordering = true;
        self
    }
}

impl EngineSubsystemAdapter for AudioSnapshotPublisher {
    fn prepare_frame(&mut self, input: &EngineFrameInput) {
        if let Ok(mut state) = self.frame_state.lock() {
            state.tick = input.current_clock.simulation_tick.get();
        }
        // RFC 0011 H7: clear last frame's report so a frame that fails to
        // publish does not surface a stale `published_voices` count.
        if let Ok(mut report) = self.report.lock() {
            *report = None;
        }
    }

    fn build(
        &mut self,
        builder: &mut EngineGraphBuilder,
    ) -> Result<EngineSubsystemPlan, EngineFrameError> {
        let mut runs_after = vec![
            EngineSubsystemKind::StateAdvance,
            EngineSubsystemKind::System,
        ];
        if self.require_physics_ordering {
            runs_after.push(EngineSubsystemKind::Physics);
        }
        let descriptor = EngineSubsystemDescriptor {
            kind: EngineSubsystemKind::Audio,
            label: "audio".to_string(),
            runs_after,
            requires_gpu: false,
            allows_hot_path_readback: false,
        };
        let publisher = Arc::clone(&self.publisher);
        let frame_state = Arc::clone(&self.frame_state);
        let report_slot = Arc::clone(&self.report);
        let job = builder.add_job(
            EngineSubsystemKind::Audio,
            "audio.publish_snapshot".to_string(),
            EngineJobAffinity::Cpu,
            EngineSpanDomain::Cpu,
            Vec::new(),
            false,
            move || {
                let (plan, tick) = {
                    let guard = frame_state.lock().map_err(|_| {
                        EngineFrameError::Message("audio adapter frame state poisoned".into())
                    })?;
                    (guard.plan.clone(), guard.tick)
                };
                let report = publisher.publish(tick, &plan);
                *report_slot.lock().map_err(|_| {
                    EngineFrameError::Message("audio report lock poisoned".into())
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
                    .filter(|span| span.subsystem == EngineSubsystemKind::Audio)
                    .map(|span| span.elapsed_micros())
                    .sum();
                let report = report_for_builder
                    .lock()
                    .map_err(|_| EngineFrameError::Message("audio report lock poisoned".into()))?
                    .clone()
                    .unwrap_or_default();
                ctx.violations.extend(
                    report
                        .structured_findings
                        .iter()
                        .map(|finding| finding.as_str().to_string()),
                );
                Ok(EngineSubsystemReport {
                    kind: descriptor.kind.clone(),
                    label: descriptor.label.clone(),
                    work_items: u64::from(report.published_voices),
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
                    notes: vec![format!(
                        "voices={} underruns={}",
                        report.published_voices, report.underruns
                    )],
                })
            },
        ))
    }
}
