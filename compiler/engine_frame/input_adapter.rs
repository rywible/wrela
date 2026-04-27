//! Semantic input subsystem adapter (RFC 0011 Phase 64).

use super::{
    EngineFrameContext, EngineFrameError, EngineFrameTimeline, EngineGpuTimingPolicy,
    EngineGraphBuilder, EngineJobAffinity, EngineMeasurementPolicy, EngineRuntimeSource,
    EngineSpanDomain, EngineSubsystemAdapter, EngineSubsystemDescriptor, EngineSubsystemKind,
    EngineSubsystemPlan, EngineSubsystemReport, MaterializedTickInputSlot,
};
use crate::input_contract::InputFrame;
use crate::input_map_plan::InputMapPlan;
use crate::world_identity::SnapshotEpoch;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct InputSubsystemAdapter {
    map: InputMapPlan,
    input_slot: MaterializedTickInputSlot,
    shared_frame: Arc<Mutex<Option<InputFrame>>>,
    output_epoch: Arc<Mutex<u64>>,
}

impl InputSubsystemAdapter {
    pub fn new(map: InputMapPlan, input_slot: MaterializedTickInputSlot) -> Self {
        Self {
            map,
            input_slot,
            shared_frame: Arc::new(Mutex::new(None)),
            output_epoch: Arc::new(Mutex::new(0)),
        }
    }

    pub fn shared_frame(&self) -> Arc<Mutex<Option<InputFrame>>> {
        Arc::clone(&self.shared_frame)
    }
}

impl EngineSubsystemAdapter for InputSubsystemAdapter {
    fn prepare_frame(&mut self, input: &super::EngineFrameInput) {
        if let Ok(mut guard) = self.output_epoch.lock() {
            *guard = input.current_clock.snapshot_epoch.get();
        }
        // RFC 0011 H7: clear last frame's input frame so a build that fails
        // before publish cannot leak stale data into the System adapter.
        if let Ok(mut guard) = self.shared_frame.lock() {
            *guard = None;
        }
    }

    fn build(
        &mut self,
        builder: &mut EngineGraphBuilder,
    ) -> Result<EngineSubsystemPlan, EngineFrameError> {
        let descriptor = EngineSubsystemDescriptor {
            kind: EngineSubsystemKind::Input,
            label: "input".to_string(),
            runs_after: vec![EngineSubsystemKind::StateAdvance],
            requires_gpu: false,
            allows_hot_path_readback: false,
        };
        let map = self.map.clone();
        let input_slot = self.input_slot.clone();
        let frame_slot = Arc::clone(&self.shared_frame);
        let frame_slot_for_report = Arc::clone(&self.shared_frame);
        let epoch_hint = Arc::clone(&self.output_epoch);
        let job = builder.add_job(
            EngineSubsystemKind::Input,
            "input.translate".to_string(),
            EngineJobAffinity::Cpu,
            EngineSpanDomain::Cpu,
            Vec::new(),
            false,
            move || {
                let Some(batch) = input_slot.snapshot()? else {
                    return Err(EngineFrameError::Message(
                        "input subsystem ran before state advance materialized tick inputs".into(),
                    ));
                };
                let epoch = epoch_hint.lock().map_err(|_| {
                    EngineFrameError::Message("input epoch scratch poisoned".into())
                })?;
                let input_frame = map.translate(&batch, SnapshotEpoch(*epoch));
                let mut guard = frame_slot.lock().map_err(|_| {
                    EngineFrameError::Message("input frame slot lock poisoned".into())
                })?;
                *guard = Some(input_frame);
                Ok(())
            },
        );
        Ok(EngineSubsystemPlan::new(
            descriptor.clone(),
            vec![job],
            vec![job],
            move |timeline: &EngineFrameTimeline, ctx: &mut EngineFrameContext| {
                let executed = timeline
                    .spans
                    .iter()
                    .filter(|span| span.subsystem == EngineSubsystemKind::Input)
                    .map(|span| span.elapsed_micros())
                    .sum();
                if let Some(epoch) = ctx.published_snapshot_epoch {
                    if let Some(frame) = frame_slot_for_report
                        .lock()
                        .map_err(|_| {
                            EngineFrameError::Message("input frame slot lock poisoned".into())
                        })?
                        .as_mut()
                    {
                        frame.epoch = SnapshotEpoch(epoch);
                    }
                }
                let work_items = frame_slot_for_report
                    .lock()
                    .map_err(|_| {
                        EngineFrameError::Message("input frame slot lock poisoned".into())
                    })?
                    .as_ref()
                    .map(|frame| frame.actions.len() as u64)
                    .unwrap_or(0);
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
                    notes: Vec::new(),
                })
            },
        ))
    }
}
