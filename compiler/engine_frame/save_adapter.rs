//! One-shot save publisher subsystem (RFC 0011 Phase 69).
//!
//! Frame-state pulls (RFC 0011 H1 acceptance: per-build state):
//! - `request`: whether the host wants a save record this frame.
//! - `snapshot`: world snapshot handle to persist (refreshed via
//!   `prepare_frame`).
//! - `sim_tick` / `presentation_frame`: refreshed every frame from
//!   `EngineFrameInput`.

use super::{
    EngineFrameContext, EngineFrameError, EngineFrameInput, EngineFrameTimeline,
    EngineGpuTimingPolicy, EngineGraphBuilder, EngineJobAffinity, EngineMeasurementPolicy,
    EngineRuntimeSource, EngineSpanDomain, EngineSubsystemAdapter, EngineSubsystemDescriptor,
    EngineSubsystemKind, EngineSubsystemPlan, EngineSubsystemReport,
};
use crate::persistence::{
    PersistenceProject, SnapshotLedgerRecord, SnapshotSaveRecord, save_snapshot_record,
};
use crate::state_advance::StateAdvanceResult;
use crate::world_identity::{
    ArtifactKeySeed, AuthoredContentId, EntityLineageId, PortableSceneCaptureProjection,
    SemanticEntityHandle, SnapshotCaptureKind, SnapshotEntityId, SnapshotEpoch,
    WorldSnapshotHandle, WorldSnapshotId,
};
use smol_str::SmolStr;
use std::sync::{Arc, Mutex};

pub trait SaveLedgerSource: std::fmt::Debug + Send + Sync {
    fn collect(
        &self,
        snapshot: &WorldSnapshotHandle,
    ) -> Result<Vec<SnapshotLedgerRecord>, EngineFrameError>;
}

#[derive(Debug, Clone)]
pub struct StaticSaveLedgerSource {
    ledger: Vec<SnapshotLedgerRecord>,
}

impl StaticSaveLedgerSource {
    pub fn new(ledger: Vec<SnapshotLedgerRecord>) -> Self {
        Self { ledger }
    }
}

impl SaveLedgerSource for StaticSaveLedgerSource {
    fn collect(
        &self,
        _snapshot: &WorldSnapshotHandle,
    ) -> Result<Vec<SnapshotLedgerRecord>, EngineFrameError> {
        Ok(self.ledger.clone())
    }
}

#[derive(Debug, Clone)]
pub struct SaveAdapterFrameState {
    pub request: bool,
    pub snapshot: WorldSnapshotHandle,
    pub project: PersistenceProject,
    pub sim_tick: u64,
    pub presentation_frame: u64,
    pub ledger_source: Arc<dyn SaveLedgerSource>,
}

pub struct SavePublisher {
    frame_state: Arc<Mutex<SaveAdapterFrameState>>,
    record: Arc<Mutex<Option<SnapshotSaveRecord>>>,
    world: SaveWorldBinding,
}

#[derive(Clone)]
enum SaveWorldBinding {
    /// Compatibility/headless path: use the frame-state snapshot refreshed
    /// from `EngineFrameInput`.
    FrameStateSnapshot,
    /// Runtime path: read the successful StateAdvance output snapshot when
    /// the save job executes.
    StateOutcome {
        slot: Arc<Mutex<Option<Result<StateAdvanceResult, EngineFrameError>>>>,
        schedule: SaveSchedule,
    },
}

#[derive(Clone, Copy)]
enum SaveSchedule {
    HeadlessAfterStateAdvance,
    AfterPresentation,
}

impl SavePublisher {
    /// Construct a publisher backed by a shared frame-state slot. The host is
    /// expected to update the slot between frames.
    pub fn from_shared_state(frame_state: Arc<Mutex<SaveAdapterFrameState>>) -> Self {
        Self {
            frame_state,
            record: Arc::new(Mutex::new(None)),
            world: SaveWorldBinding::FrameStateSnapshot,
        }
    }

    /// Compatibility constructor that bakes initial state. `prepare_frame`
    /// will refresh the tick/presentation/snapshot fields from
    /// [`EngineFrameInput`] each frame.
    pub fn new(
        request: bool,
        snapshot: WorldSnapshotHandle,
        project: PersistenceProject,
        sim_tick: u64,
        presentation_frame: u64,
        ledger: Vec<SnapshotLedgerRecord>,
    ) -> Self {
        let frame_state = Arc::new(Mutex::new(SaveAdapterFrameState {
            request,
            snapshot,
            project,
            sim_tick,
            presentation_frame,
            ledger_source: Arc::new(StaticSaveLedgerSource::new(ledger)),
        }));
        Self::from_shared_state(frame_state)
    }

    /// Live/runtime constructor. The save reads the successful StateAdvance
    /// output snapshot at job time and is scheduled after Presentation.
    ///
    /// Use [`SavePublisher::with_state_outcome_headless`] for tests or tools
    /// that do not register a Presentation adapter.
    pub fn with_state_outcome(
        request: bool,
        outcome: Arc<Mutex<Option<Result<StateAdvanceResult, EngineFrameError>>>>,
        project: PersistenceProject,
        sim_tick: u64,
        presentation_frame: u64,
        ledger: Vec<SnapshotLedgerRecord>,
    ) -> Self {
        Self::from_state_outcome(
            request,
            outcome,
            project,
            sim_tick,
            presentation_frame,
            ledger,
            SaveSchedule::AfterPresentation,
        )
    }

    /// Headless/test constructor. The save reads the successful StateAdvance
    /// output snapshot at job time and only depends on StateAdvance, avoiding
    /// a hard Presentation dependency in frames that do not present.
    pub fn with_state_outcome_headless(
        request: bool,
        outcome: Arc<Mutex<Option<Result<StateAdvanceResult, EngineFrameError>>>>,
        project: PersistenceProject,
        sim_tick: u64,
        presentation_frame: u64,
        ledger: Vec<SnapshotLedgerRecord>,
    ) -> Self {
        Self::from_state_outcome(
            request,
            outcome,
            project,
            sim_tick,
            presentation_frame,
            ledger,
            SaveSchedule::HeadlessAfterStateAdvance,
        )
    }

    fn from_state_outcome(
        request: bool,
        outcome: Arc<Mutex<Option<Result<StateAdvanceResult, EngineFrameError>>>>,
        project: PersistenceProject,
        sim_tick: u64,
        presentation_frame: u64,
        ledger: Vec<SnapshotLedgerRecord>,
        schedule: SaveSchedule,
    ) -> Self {
        let snapshot = WorldSnapshotHandle::new(
            SmolStr::new("save_state_outcome_pending"),
            SnapshotCaptureKind::Region,
            WorldSnapshotId(0),
            SnapshotEpoch(0),
            ArtifactKeySeed(0),
            SemanticEntityHandle::new(
                AuthoredContentId(0),
                EntityLineageId(0),
                SnapshotEntityId(0),
            ),
            PortableSceneCaptureProjection {
                scene_id: 0,
                epoch: 0,
                root_feature_id: 0,
            },
        );
        Self {
            frame_state: Arc::new(Mutex::new(SaveAdapterFrameState {
                request,
                snapshot,
                project,
                sim_tick,
                presentation_frame,
                ledger_source: Arc::new(StaticSaveLedgerSource::new(ledger)),
            })),
            record: Arc::new(Mutex::new(None)),
            world: SaveWorldBinding::StateOutcome {
                slot: outcome,
                schedule,
            },
        }
    }

    pub fn frame_state(&self) -> Arc<Mutex<SaveAdapterFrameState>> {
        Arc::clone(&self.frame_state)
    }

    pub fn record(&self) -> Arc<Mutex<Option<SnapshotSaveRecord>>> {
        Arc::clone(&self.record)
    }

    pub fn request_save(&self, request: bool) {
        if let Ok(mut state) = self.frame_state.lock() {
            state.request = request;
        }
    }

    pub fn set_ledger_source(&self, ledger_source: Arc<dyn SaveLedgerSource>) {
        if let Ok(mut state) = self.frame_state.lock() {
            state.ledger_source = ledger_source;
        }
    }
}

impl EngineSubsystemAdapter for SavePublisher {
    fn prepare_frame(&mut self, input: &EngineFrameInput) {
        if let Ok(mut state) = self.frame_state.lock() {
            state.sim_tick = input.current_clock.simulation_tick.get();
            state.presentation_frame = input.current_clock.presentation_frame.get();
            state.snapshot = input.previous_snapshot.clone();
        }
        // RFC 0011 H7: clear the previous frame's record so an unrequested
        // save this frame can't be confused with a leftover from last frame.
        if let Ok(mut record) = self.record.lock() {
            *record = None;
        }
    }

    fn build(
        &mut self,
        builder: &mut EngineGraphBuilder,
    ) -> Result<EngineSubsystemPlan, EngineFrameError> {
        let descriptor = EngineSubsystemDescriptor {
            kind: EngineSubsystemKind::Save,
            label: "save".to_string(),
            runs_after: match &self.world {
                SaveWorldBinding::FrameStateSnapshot => vec![EngineSubsystemKind::StateAdvance],
                SaveWorldBinding::StateOutcome {
                    schedule: SaveSchedule::HeadlessAfterStateAdvance,
                    ..
                } => vec![EngineSubsystemKind::StateAdvance],
                SaveWorldBinding::StateOutcome {
                    schedule: SaveSchedule::AfterPresentation,
                    ..
                } => vec![EngineSubsystemKind::Presentation],
            },
            requires_gpu: false,
            allows_hot_path_readback: false,
        };
        let frame_state = Arc::clone(&self.frame_state);
        let record_slot = Arc::clone(&self.record);
        let world = self.world.clone();
        let job =
            builder.add_job(
                EngineSubsystemKind::Save,
                "save.publish".to_string(),
                EngineJobAffinity::Cpu,
                EngineSpanDomain::Cpu,
                Vec::new(),
                false,
                move || {
                    let (
                        request,
                        frame_snapshot,
                        project,
                        sim_tick,
                        presentation_frame,
                        ledger_source,
                    ) = {
                        let guard = frame_state.lock().map_err(|_| {
                            EngineFrameError::Message("save adapter frame state poisoned".into())
                        })?;
                        (
                            guard.request,
                            guard.snapshot.clone(),
                            guard.project.clone(),
                            guard.sim_tick,
                            guard.presentation_frame,
                            Arc::clone(&guard.ledger_source),
                        )
                    };
                    if request {
                        let snapshot = match &world {
                            SaveWorldBinding::FrameStateSnapshot => frame_snapshot,
                            SaveWorldBinding::StateOutcome { slot, .. } => {
                                let guard = slot.lock().map_err(|_| {
                                    EngineFrameError::Message("state outcome lock poisoned".into())
                                })?;
                                let Some(result) = guard.as_ref() else {
                                    return Err(EngineFrameError::Message(
                                        "save publish before state advance outcome".into(),
                                    ));
                                };
                                let ok = result.as_ref().map_err(|err| {
                                    EngineFrameError::Message(format!(
                                        "state advance failed before save publish: {err}"
                                    ))
                                })?;
                                ok.transition_record.to_snapshot.clone()
                            }
                        };
                        let ledger = ledger_source.collect(&snapshot)?;
                        let record = save_snapshot_record(
                            &snapshot,
                            &project,
                            sim_tick,
                            presentation_frame,
                            ledger,
                        )
                        .map_err(|err| EngineFrameError::Message(err.to_string()))?;
                        *record_slot.lock().map_err(|_| {
                            EngineFrameError::Message("save record lock poisoned".into())
                        })? = Some(record);
                    } else {
                        *record_slot.lock().map_err(|_| {
                            EngineFrameError::Message("save record lock poisoned".into())
                        })? = None;
                    }
                    Ok(())
                },
            );
        let record_for_report = Arc::clone(&self.record);
        Ok(EngineSubsystemPlan::new(
            descriptor.clone(),
            vec![job],
            vec![job],
            move |timeline: &EngineFrameTimeline, _ctx: &mut EngineFrameContext| {
                let executed = timeline
                    .spans
                    .iter()
                    .filter(|span| span.subsystem == EngineSubsystemKind::Save)
                    .map(|span| span.elapsed_micros())
                    .sum();
                let work_items = u64::from(
                    record_for_report
                        .lock()
                        .map_err(|_| EngineFrameError::Message("save record lock poisoned".into()))?
                        .is_some(),
                );
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
