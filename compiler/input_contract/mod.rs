//! Semantic input frame contract (RFC 0011 Phase 64).

use crate::state_advance::SimulationTick;
use crate::world_identity::SnapshotEpoch;
use smol_str::SmolStr;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SemanticActionId(pub SmolStr);

impl SemanticActionId {
    pub fn new(value: impl Into<SmolStr>) -> Self {
        Self(value.into())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InputMapId(pub SmolStr);

#[derive(Debug, Clone, PartialEq)]
pub enum SemanticActionState {
    Button {
        pressed: bool,
        just_pressed: bool,
        just_released: bool,
    },
    Axis1 {
        value: f32,
    },
    Axis2 {
        x: f32,
        y: f32,
    },
}

impl SemanticActionState {
    pub fn pressed_button() -> Self {
        Self::Button {
            pressed: true,
            just_pressed: true,
            just_released: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InputMapBinding {
    pub action: SemanticActionId,
    pub source: SmolStr,
    pub detail: SmolStr,
}

impl InputMapBinding {
    pub fn new(
        action: impl Into<SmolStr>,
        source: impl Into<SmolStr>,
        detail: impl Into<SmolStr>,
    ) -> Self {
        Self {
            action: SemanticActionId::new(action),
            source: source.into(),
            detail: detail.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct InputFrame {
    pub epoch: SnapshotEpoch,
    pub tick: SimulationTick,
    pub actions: BTreeMap<SemanticActionId, SemanticActionState>,
}
