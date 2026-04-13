#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SimulationTick(pub u64);

impl SimulationTick {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PresentationFrame(pub u64);

impl PresentationFrame {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WallClockStamp(pub u64);

impl WallClockStamp {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SnapshotEpoch(pub u64);

impl SnapshotEpoch {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldSnapshotHandleRecord {
    pub capture_name: String,
    pub snapshot_id: u64,
    pub epoch: SnapshotEpoch,
}

impl WorldSnapshotHandleRecord {
    pub fn new(capture_name: impl Into<String>, snapshot_id: u64, epoch: SnapshotEpoch) -> Self {
        Self {
            capture_name: capture_name.into(),
            snapshot_id,
            epoch,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum ChangeClass {
    None = 0,
    Presentation = 1,
    Structural = 2,
    Topology = 3,
    Identity = 4,
    Incompatible = 5,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChangeCompatibility {
    pub maximum: ChangeClass,
}

impl ChangeCompatibility {
    pub const fn new(maximum: ChangeClass) -> Self {
        Self { maximum }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TickInputKind {
    Command,
    Event,
    Observation,
    System,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TickInputEvent {
    pub tick: SimulationTick,
    pub kind: TickInputKind,
    pub source: String,
    pub detail: String,
}

impl TickInputEvent {
    pub fn new(
        tick: SimulationTick,
        kind: TickInputKind,
        source: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            tick,
            kind,
            source: source.into(),
            detail: detail.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TickInputBatch {
    pub tick: SimulationTick,
    pub inputs: Vec<TickInputEvent>,
}

impl TickInputBatch {
    pub fn new(tick: SimulationTick, inputs: impl Into<Vec<TickInputEvent>>) -> Self {
        Self {
            tick,
            inputs: inputs.into(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.inputs.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IdentityTransitionKind {
    Preserved,
    Spawned,
    Despawned,
    Rebound,
    Replaced,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityTransitionEvent {
    pub kind: IdentityTransitionKind,
    pub persistent_identity: String,
    pub previous_snapshot_identity: Option<String>,
    pub current_snapshot_identity: Option<String>,
    pub detail: String,
}

impl IdentityTransitionEvent {
    pub fn new(
        kind: IdentityTransitionKind,
        persistent_identity: impl Into<String>,
        previous_snapshot_identity: Option<String>,
        current_snapshot_identity: Option<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            persistent_identity: persistent_identity.into(),
            previous_snapshot_identity,
            current_snapshot_identity,
            detail: detail.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeSummary {
    pub class: ChangeClass,
    pub detail: String,
}

impl ChangeSummary {
    pub fn new(class: ChangeClass, detail: impl Into<String>) -> Self {
        Self {
            class,
            detail: detail.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TransitionRejectionReason {
    SnapshotEpochMismatch,
    ValidityHorizonExceeded,
    ChangeCompatibilityExceeded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TemporalClock {
    pub snapshot_epoch: SnapshotEpoch,
    pub simulation_tick: SimulationTick,
    pub presentation_frame: PresentationFrame,
    pub wall_clock: WallClockStamp,
}

impl TemporalClock {
    pub const fn new(
        snapshot_epoch: SnapshotEpoch,
        simulation_tick: SimulationTick,
        presentation_frame: PresentationFrame,
        wall_clock: WallClockStamp,
    ) -> Self {
        Self {
            snapshot_epoch,
            simulation_tick,
            presentation_frame,
            wall_clock,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TemporalValidityHorizon {
    pub max_snapshot_age: u64,
    pub max_simulation_tick_age: u64,
    pub max_presentation_frame_age: u64,
    pub max_wall_clock_age_ms: u64,
}

impl TemporalValidityHorizon {
    pub const fn new(
        max_snapshot_age: u64,
        max_simulation_tick_age: u64,
        max_presentation_frame_age: u64,
        max_wall_clock_age_ms: u64,
    ) -> Self {
        Self {
            max_snapshot_age,
            max_simulation_tick_age,
            max_presentation_frame_age,
            max_wall_clock_age_ms,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldTransitionRecord {
    pub from_snapshot: Option<WorldSnapshotHandleRecord>,
    pub to_snapshot: WorldSnapshotHandleRecord,
    pub previous_clock: Option<TemporalClock>,
    pub current_clock: TemporalClock,
    pub tick: SimulationTick,
    pub inputs: TickInputBatch,
    pub identity_events: Vec<IdentityTransitionEvent>,
}

impl WorldTransitionRecord {
    pub fn new(
        from_snapshot: Option<WorldSnapshotHandleRecord>,
        to_snapshot: WorldSnapshotHandleRecord,
        previous_clock: Option<TemporalClock>,
        current_clock: TemporalClock,
        inputs: TickInputBatch,
        identity_events: impl Into<Vec<IdentityTransitionEvent>>,
    ) -> Self {
        Self {
            from_snapshot,
            to_snapshot,
            previous_clock,
            current_clock,
            tick: inputs.tick,
            inputs,
            identity_events: identity_events.into(),
        }
    }

    pub fn planner_summary(
        &self,
        change: ChangeSummary,
        compatibility: ChangeCompatibility,
        accepted: bool,
        rejection: Option<TransitionRejectionReason>,
    ) -> StateAdvanceTransitionRecord {
        StateAdvanceTransitionRecord {
            current_clock: self.current_clock,
            previous_clock: self.previous_clock,
            tick: self.tick,
            input_count: self.inputs.inputs.len(),
            identity_event_count: self.identity_events.len(),
            change,
            compatibility,
            accepted,
            rejection,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateAdvanceExecutorContract {
    pub current_clock: TemporalClock,
    pub previous_clock: Option<TemporalClock>,
    pub validity_horizon: TemporalValidityHorizon,
    pub change: ChangeSummary,
    pub compatibility: ChangeCompatibility,
}

impl StateAdvanceExecutorContract {
    pub fn is_transition_compatible(&self) -> bool {
        self.compatibility.maximum as u8 >= self.change.class as u8
            && !matches!(self.change.class, ChangeClass::Incompatible)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateAdvanceTransitionRecord {
    pub current_clock: TemporalClock,
    pub previous_clock: Option<TemporalClock>,
    pub tick: SimulationTick,
    pub input_count: usize,
    pub identity_event_count: usize,
    pub change: ChangeSummary,
    pub compatibility: ChangeCompatibility,
    pub accepted: bool,
    pub rejection: Option<TransitionRejectionReason>,
}

impl StateAdvanceTransitionRecord {
    fn from_executor(
        executor: StateAdvanceExecutorContract,
        accepted: bool,
        rejection: Option<TransitionRejectionReason>,
    ) -> Self {
        Self {
            current_clock: executor.current_clock,
            previous_clock: executor.previous_clock,
            tick: executor.current_clock.simulation_tick,
            input_count: 0,
            identity_event_count: 0,
            change: executor.change,
            compatibility: executor.compatibility,
            accepted,
            rejection,
        }
    }

    pub fn from_world_transition(
        transition: &WorldTransitionRecord,
        change: ChangeSummary,
        compatibility: ChangeCompatibility,
        accepted: bool,
        rejection: Option<TransitionRejectionReason>,
    ) -> Self {
        transition.planner_summary(change, compatibility, accepted, rejection)
    }

    pub fn accepted(executor: StateAdvanceExecutorContract) -> Self {
        Self::from_executor(executor, true, None)
    }

    pub fn rejected(
        executor: StateAdvanceExecutorContract,
        rejection: TransitionRejectionReason,
    ) -> Self {
        Self::from_executor(executor, false, Some(rejection))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateAdvanceResult {
    pub transition_record: WorldTransitionRecord,
    pub change_summary: ChangeSummary,
}

impl StateAdvanceResult {
    pub fn new(transition_record: WorldTransitionRecord, change_summary: ChangeSummary) -> Self {
        Self {
            transition_record,
            change_summary,
        }
    }

    pub fn planner_summary(
        &self,
        compatibility: ChangeCompatibility,
        accepted: bool,
        rejection: Option<TransitionRejectionReason>,
    ) -> StateAdvanceTransitionRecord {
        self.transition_record.planner_summary(
            self.change_summary.clone(),
            compatibility,
            accepted,
            rejection,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateAdvanceMirrorContract {
    pub executor: StateAdvanceExecutorContract,
    pub accepted: bool,
}
