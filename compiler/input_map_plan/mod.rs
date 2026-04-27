//! Compiled semantic input map executor (RFC 0011 Phase 64).

use crate::input_contract::{
    InputFrame, InputMapBinding, InputMapId, SemanticActionId, SemanticActionState,
};
use crate::state_advance::TickInputBatch;
use crate::world_identity::SnapshotEpoch;
use smol_str::SmolStr;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputMapPlan {
    pub id: InputMapId,
    pub bindings: Vec<InputMapBinding>,
}

impl InputMapPlan {
    pub fn new(id: impl Into<SmolStr>, bindings: Vec<InputMapBinding>) -> Result<Self, String> {
        let mut actions = BTreeSet::new();
        for binding in &bindings {
            if !actions.insert(binding.action.clone()) {
                return Err(format!("duplicate input action `{}`", binding.action.0));
            }
        }
        Ok(Self {
            id: InputMapId(id.into()),
            bindings,
        })
    }

    pub fn empty(id: impl Into<SmolStr>) -> Self {
        Self {
            id: InputMapId(id.into()),
            bindings: Vec::new(),
        }
    }

    pub fn translate(&self, batch: &TickInputBatch, epoch: SnapshotEpoch) -> InputFrame {
        let mut actions = BTreeMap::<SemanticActionId, SemanticActionState>::new();
        for input in &batch.inputs {
            for binding in &self.bindings {
                if binding.source == input.source && binding.detail == input.detail {
                    actions.insert(
                        binding.action.clone(),
                        SemanticActionState::pressed_button(),
                    );
                }
            }
        }
        InputFrame {
            epoch,
            tick: batch.tick,
            actions,
        }
    }
}
