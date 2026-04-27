//! System runtime contract types (RFC 0011 Phase 65).

use smol_str::SmolStr;
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SystemContractId(pub SmolStr);

impl SystemContractId {
    pub fn new(value: impl Into<SmolStr>) -> Self {
        Self(value.into())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SystemId(pub SmolStr);

impl SystemId {
    pub fn new(value: impl Into<SmolStr>) -> Self {
        Self(value.into())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EventTypeId(pub SmolStr);

impl EventTypeId {
    pub fn new(value: impl Into<SmolStr>) -> Self {
        Self(value.into())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SystemFamilyId {
    Sim,
    Presentation,
    Debug,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SystemPhase {
    PreSim,
    Sim,
    PostSim,
}

impl SystemPhase {
    pub const ALL: [Self; 3] = [Self::PreSim, Self::Sim, Self::PostSim];

    pub const fn index(self) -> usize {
        match self {
            Self::PreSim => 0,
            Self::Sim => 1,
            Self::PostSim => 2,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::PreSim => "pre_sim",
            Self::Sim => "sim",
            Self::PostSim => "post_sim",
        }
    }

    pub fn parse_label(value: &str) -> Option<Self> {
        match value {
            "pre_sim" => Some(Self::PreSim),
            "sim" => Some(Self::Sim),
            "post_sim" => Some(Self::PostSim),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SystemResourceId {
    Resource(SmolStr),
    WorldCapture(SmolStr),
    InputFrame,
    Snapshot,
    SnapshotMut,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SystemAccessSummary {
    pub reads: BTreeSet<SystemResourceId>,
    pub writes: BTreeSet<SystemResourceId>,
    pub reads_events: BTreeSet<EventTypeId>,
    pub emits_events: BTreeSet<EventTypeId>,
}

impl SystemAccessSummary {
    pub fn reads(mut self, resource: SystemResourceId) -> Self {
        self.reads.insert(resource);
        self
    }

    pub fn writes(mut self, resource: SystemResourceId) -> Self {
        self.writes.insert(resource);
        self
    }

    pub fn emits_event(mut self, event: EventTypeId) -> Self {
        self.emits_events.insert(event);
        self
    }
}
