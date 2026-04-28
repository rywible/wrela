pub use crate::time_semantics::{
    ChangeClass, ChangeCompatibility, ChangeSummary, ClockDomain, PresentationFrame,
    SimulationTick, SnapshotEpoch, TemporalClock, TemporalValidityHorizon, WallClockStamp,
};

use crate::time_semantics::{ChangeClass as ChangeClassKind, ChangeCompatibility as ChangeBudget};
use crate::world_identity::WorldSnapshotHandle;
use smol_str::SmolStr;

pub type StateAdvanceAuthorityContract = StateAdvanceContract;
pub type QueryTransitionContract = StateAdvanceContract;
pub type QueryTransitionRecord = StateAdvanceTransitionRecord;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TickInputKind {
    Command,
    Event,
    Observation,
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TickInputValue {
    None,
    Button { pressed: bool },
    Axis1 { value_micros: i32 },
    Axis2 { x_micros: i32, y_micros: i32 },
}

impl TickInputValue {
    pub fn pressed_button() -> Self {
        Self::Button { pressed: true }
    }

    pub fn button(pressed: bool) -> Self {
        Self::Button { pressed }
    }

    pub fn axis1(value: f32) -> Self {
        Self::Axis1 {
            value_micros: scaled_f32_to_i32_micros(value),
        }
    }

    pub fn axis2(x: f32, y: f32) -> Self {
        Self::Axis2 {
            x_micros: scaled_f32_to_i32_micros(x),
            y_micros: scaled_f32_to_i32_micros(y),
        }
    }

    pub fn axis1_value(self) -> Option<f32> {
        match self {
            TickInputValue::Axis1 { value_micros } => Some(value_micros as f32 / 1_000_000.0),
            _ => None,
        }
    }

    pub fn axis2_value(self) -> Option<(f32, f32)> {
        match self {
            TickInputValue::Axis2 { x_micros, y_micros } => {
                Some((x_micros as f32 / 1_000_000.0, y_micros as f32 / 1_000_000.0))
            }
            _ => None,
        }
    }
}

fn scaled_f32_to_i32_micros(value: f32) -> i32 {
    if !value.is_finite() {
        return 0;
    }
    let scaled = (value as f64 * 1_000_000.0).round();
    scaled.clamp(i32::MIN as f64, i32::MAX as f64) as i32
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TickInputEvent {
    pub tick: SimulationTick,
    pub kind: TickInputKind,
    pub source: SmolStr,
    pub detail: SmolStr,
    pub value: TickInputValue,
    /// Wall-clock time when the platform observed this input (RFC 0011).
    pub wall_clock: WallClockStamp,
    /// Monotonic nanoseconds for latency staging (RFC 0011).
    pub monotonic_nanos: u64,
}

impl TickInputEvent {
    pub fn new(
        tick: SimulationTick,
        kind: TickInputKind,
        source: impl Into<SmolStr>,
        detail: impl Into<SmolStr>,
    ) -> Self {
        Self {
            tick,
            kind,
            source: source.into(),
            detail: detail.into(),
            value: TickInputValue::pressed_button(),
            wall_clock: WallClockStamp::new(0),
            monotonic_nanos: 0,
        }
    }

    pub fn with_timestamps(
        tick: SimulationTick,
        kind: TickInputKind,
        source: impl Into<SmolStr>,
        detail: impl Into<SmolStr>,
        wall_clock: WallClockStamp,
        monotonic_nanos: u64,
    ) -> Self {
        Self {
            tick,
            kind,
            source: source.into(),
            detail: detail.into(),
            value: TickInputValue::pressed_button(),
            wall_clock,
            monotonic_nanos,
        }
    }

    pub fn with_timestamps_and_value(
        tick: SimulationTick,
        kind: TickInputKind,
        source: impl Into<SmolStr>,
        detail: impl Into<SmolStr>,
        value: TickInputValue,
        wall_clock: WallClockStamp,
        monotonic_nanos: u64,
    ) -> Self {
        Self {
            tick,
            kind,
            source: source.into(),
            detail: detail.into(),
            value,
            wall_clock,
            monotonic_nanos,
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
    pub persistent_identity: SmolStr,
    pub previous_snapshot_identity: Option<SmolStr>,
    pub current_snapshot_identity: Option<SmolStr>,
    pub detail: SmolStr,
}

impl IdentityTransitionEvent {
    pub fn new(
        kind: IdentityTransitionKind,
        persistent_identity: impl Into<SmolStr>,
        previous_snapshot_identity: Option<SmolStr>,
        current_snapshot_identity: Option<SmolStr>,
        detail: impl Into<SmolStr>,
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
pub struct StateAdvanceContract {
    pub current_clock: TemporalClock,
    pub previous_clock: Option<TemporalClock>,
    pub validity_horizon: TemporalValidityHorizon,
    pub change: ChangeSummary,
    pub compatibility: ChangeCompatibility,
}

impl StateAdvanceContract {
    pub fn is_transition_compatible(&self) -> bool {
        self.compatibility.allows(self.change.class)
    }

    pub fn query_transition_summary(&self) -> QueryTransitionRecord {
        QueryTransitionRecord::from_contract(self.clone(), self.is_transition_compatible(), None)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TransitionRejectionReason {
    SnapshotEpochMismatch,
    ValidityHorizonExceeded,
    ChangeCompatibilityExceeded,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldTransitionRecord {
    pub from_snapshot: Option<WorldSnapshotHandle>,
    pub to_snapshot: WorldSnapshotHandle,
    pub previous_clock: Option<TemporalClock>,
    pub current_clock: TemporalClock,
    pub tick: SimulationTick,
    pub inputs: TickInputBatch,
    pub identity_events: Vec<IdentityTransitionEvent>,
}

impl WorldTransitionRecord {
    pub fn new(
        from_snapshot: Option<WorldSnapshotHandle>,
        to_snapshot: WorldSnapshotHandle,
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
    fn from_contract(
        contract: StateAdvanceContract,
        accepted: bool,
        rejection: Option<TransitionRejectionReason>,
    ) -> Self {
        Self {
            current_clock: contract.current_clock,
            previous_clock: contract.previous_clock,
            tick: contract.current_clock.simulation_tick,
            input_count: 0,
            identity_event_count: 0,
            change: contract.change,
            compatibility: contract.compatibility,
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

    pub fn accepted(contract: StateAdvanceContract) -> Self {
        Self::from_contract(contract, true, None)
    }

    pub fn rejected(contract: StateAdvanceContract, rejection: TransitionRejectionReason) -> Self {
        Self::from_contract(contract, false, Some(rejection))
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

impl ChangeCompatibility {
    pub const fn transition_budget(maximum: ChangeClass) -> Self {
        Self::new(maximum)
    }
}

impl StateAdvanceContract {
    pub fn new(
        current_clock: TemporalClock,
        previous_clock: Option<TemporalClock>,
        validity_horizon: TemporalValidityHorizon,
        change: impl Into<SmolStr>,
        change_class: ChangeClassKind,
        compatibility: ChangeBudget,
    ) -> Self {
        Self {
            current_clock,
            previous_clock,
            validity_horizon,
            change: ChangeSummary::new(change_class, change),
            compatibility,
        }
    }
}
