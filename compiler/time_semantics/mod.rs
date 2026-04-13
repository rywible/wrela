use smol_str::SmolStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SimulationTick(pub u64);

impl SimulationTick {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }

    pub const fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PresentationFrame(pub u64);

impl PresentationFrame {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }

    pub const fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WallClockStamp(pub u64);

impl WallClockStamp {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SnapshotEpoch(pub u64);

impl SnapshotEpoch {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }

    pub const fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ClockDomain {
    Simulation,
    Presentation,
    Wall,
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

impl ChangeClass {
    const fn severity(self) -> u8 {
        self as u8
    }

    pub const fn join(self, other: Self) -> Self {
        if self.severity() >= other.severity() {
            self
        } else {
            other
        }
    }

    pub const fn meet(self, other: Self) -> Self {
        if self.severity() <= other.severity() {
            self
        } else {
            other
        }
    }

    pub const fn allows(self, observed: Self) -> bool {
        observed.severity() <= self.severity() && !matches!(observed, Self::Incompatible)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChangeCompatibility {
    pub maximum: ChangeClass,
}

impl ChangeCompatibility {
    pub const fn new(maximum: ChangeClass) -> Self {
        Self { maximum }
    }

    pub const fn allows(self, change: ChangeClass) -> bool {
        self.maximum.allows(change)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeSummary {
    pub class: ChangeClass,
    pub detail: SmolStr,
}

impl ChangeSummary {
    pub fn new(class: ChangeClass, detail: impl Into<SmolStr>) -> Self {
        Self {
            class,
            detail: detail.into(),
        }
    }
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
