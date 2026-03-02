use crate::physics::core::PhysicsBodyStateV1;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InteractionTypeV1 {
    Use,
    Harvest,
    Break,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InteractionStateV1 {
    pub lifecycle_state: PhysicsBodyStateV1,
    pub resource_milli: i64,
}

pub fn apply_interaction_depletion(
    state: &InteractionStateV1,
    interaction: InteractionTypeV1,
    depletion_milli: i64,
) -> InteractionStateV1 {
    let mut next = state.clone();
    if matches!(
        interaction,
        InteractionTypeV1::Use | InteractionTypeV1::Harvest
    ) {
        next.resource_milli = (next.resource_milli - depletion_milli).max(0);
    }
    if matches!(interaction, InteractionTypeV1::Break) {
        next.lifecycle_state = PhysicsBodyStateV1::Baked;
    }
    if next.resource_milli == 0 && next.lifecycle_state == PhysicsBodyStateV1::Active {
        next.lifecycle_state = PhysicsBodyStateV1::Harvested;
    }
    next
}

#[cfg(test)]
mod tests {
    use super::{InteractionStateV1, InteractionTypeV1, apply_interaction_depletion};
    use crate::physics::core::PhysicsBodyStateV1;

    #[test]
    fn restart_persistence_depletion_logic_is_deterministic() {
        let start = InteractionStateV1 {
            lifecycle_state: PhysicsBodyStateV1::Active,
            resource_milli: 200,
        };
        let after_harvest = apply_interaction_depletion(&start, InteractionTypeV1::Harvest, 200);
        assert_eq!(after_harvest.resource_milli, 0);
        assert_eq!(after_harvest.lifecycle_state, PhysicsBodyStateV1::Harvested);

        let replay = apply_interaction_depletion(&start, InteractionTypeV1::Harvest, 200);
        assert_eq!(after_harvest, replay);
    }
}
