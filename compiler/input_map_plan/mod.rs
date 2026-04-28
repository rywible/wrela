//! Compiled semantic input map executor (RFC 0011 Phase 64).

use crate::input_contract::{
    InputFrame, InputMapBinding, InputMapId, SemanticActionId, SemanticActionState,
};
use crate::state_advance::{TickInputBatch, TickInputValue};
use crate::world_identity::SnapshotEpoch;
use smol_str::SmolStr;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputMapPlan {
    pub id: InputMapId,
    pub bindings: Vec<InputMapBinding>,
}

impl InputMapPlan {
    pub fn new(id: impl Into<SmolStr>, bindings: Vec<InputMapBinding>) -> Result<Self, String> {
        for binding in &bindings {
            if binding.action.0.trim().is_empty() {
                return Err("input action id must not be empty".to_string());
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
                    merge_action_state(&mut actions, binding.action.clone(), input.value);
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

fn merge_action_state(
    actions: &mut BTreeMap<SemanticActionId, SemanticActionState>,
    action: SemanticActionId,
    value: TickInputValue,
) {
    match value {
        TickInputValue::None => {}
        TickInputValue::Button { pressed } => {
            actions
                .entry(action)
                .and_modify(|state| merge_button_state(state, pressed))
                .or_insert_with(|| SemanticActionState::Button {
                    pressed,
                    just_pressed: pressed,
                    just_released: !pressed,
                });
        }
        TickInputValue::Axis1 { value_micros } => {
            actions.insert(
                action,
                SemanticActionState::Axis1 {
                    value: value_micros as f32 / 1_000_000.0,
                },
            );
        }
        TickInputValue::Axis2 { x_micros, y_micros } => {
            actions.insert(
                action,
                SemanticActionState::Axis2 {
                    x: x_micros as f32 / 1_000_000.0,
                    y: y_micros as f32 / 1_000_000.0,
                },
            );
        }
    }
}

fn merge_button_state(state: &mut SemanticActionState, pressed: bool) {
    match state {
        SemanticActionState::Button {
            pressed: current,
            just_pressed,
            just_released,
        } => {
            *just_pressed |= pressed;
            *just_released |= !pressed;
            *current = pressed || *current && !*just_released;
        }
        _ => {
            *state = SemanticActionState::Button {
                pressed,
                just_pressed: pressed,
                just_released: !pressed,
            };
        }
    }
}
