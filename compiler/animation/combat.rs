use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MoveSpec {
    pub name: String,
    pub startup_frames: u32,
    pub active_frames: u32,
    pub recovery_frames: u32,
    pub cancel_start: u32,
    pub cancel_end: u32,
    pub airborne: bool,
    pub chains_to: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CombatError {
    InvalidCancelWindow { move_name: String },
    UnknownMoveInChain { move_name: String },
    MoveNotAirborne { move_name: String },
    ChainLinkInvalid { from: String, to: String },
}

pub fn validate_cancel_windows(moves: &[MoveSpec]) -> Result<(), CombatError> {
    for mv in moves {
        let total_frames = mv.startup_frames + mv.active_frames + mv.recovery_frames;
        if mv.cancel_start > mv.cancel_end
            || mv.cancel_end > total_frames
            || mv.cancel_start < mv.startup_frames
        {
            return Err(CombatError::InvalidCancelWindow {
                move_name: mv.name.clone(),
            });
        }
    }
    Ok(())
}

pub fn validate_aerial_chain(
    chain: &[String],
    catalog: &BTreeMap<String, MoveSpec>,
) -> Result<(), CombatError> {
    if chain.is_empty() {
        return Ok(());
    }

    for move_name in chain {
        let mv = catalog
            .get(move_name)
            .ok_or_else(|| CombatError::UnknownMoveInChain {
                move_name: move_name.clone(),
            })?;
        if !mv.airborne {
            return Err(CombatError::MoveNotAirborne {
                move_name: move_name.clone(),
            });
        }
    }

    for pair in chain.windows(2) {
        let from = &pair[0];
        let to = &pair[1];
        let mv = catalog
            .get(from)
            .ok_or_else(|| CombatError::UnknownMoveInChain {
                move_name: from.clone(),
            })?;
        if !mv.chains_to.iter().any(|candidate| candidate == to) {
            return Err(CombatError::ChainLinkInvalid {
                from: from.clone(),
                to: to.clone(),
            });
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{CombatError, MoveSpec, validate_aerial_chain, validate_cancel_windows};
    use std::collections::BTreeMap;

    #[test]
    fn cancel_window_contracts() {
        let valid = MoveSpec {
            name: "traveller.light_1".to_owned(),
            startup_frames: 6,
            active_frames: 3,
            recovery_frames: 12,
            cancel_start: 7,
            cancel_end: 18,
            airborne: false,
            chains_to: vec!["traveller.light_2".to_owned()],
        };
        assert!(validate_cancel_windows(std::slice::from_ref(&valid)).is_ok());

        let invalid = MoveSpec {
            cancel_start: 20,
            cancel_end: 25,
            ..valid
        };
        let result = validate_cancel_windows(&[invalid]);
        assert!(matches!(
            result,
            Err(CombatError::InvalidCancelWindow { .. })
        ));
    }

    #[test]
    fn aerial_chain_validity() {
        let mut catalog = BTreeMap::new();
        catalog.insert(
            "jump_slash".to_owned(),
            MoveSpec {
                name: "jump_slash".to_owned(),
                startup_frames: 5,
                active_frames: 4,
                recovery_frames: 8,
                cancel_start: 6,
                cancel_end: 14,
                airborne: true,
                chains_to: vec!["air_fang".to_owned()],
            },
        );
        catalog.insert(
            "air_fang".to_owned(),
            MoveSpec {
                name: "air_fang".to_owned(),
                startup_frames: 4,
                active_frames: 3,
                recovery_frames: 10,
                cancel_start: 5,
                cancel_end: 12,
                airborne: true,
                chains_to: vec!["dive_kick".to_owned()],
            },
        );
        catalog.insert(
            "dive_kick".to_owned(),
            MoveSpec {
                name: "dive_kick".to_owned(),
                startup_frames: 7,
                active_frames: 5,
                recovery_frames: 16,
                cancel_start: 9,
                cancel_end: 18,
                airborne: true,
                chains_to: vec![],
            },
        );

        let valid_chain = vec![
            "jump_slash".to_owned(),
            "air_fang".to_owned(),
            "dive_kick".to_owned(),
        ];
        assert!(validate_aerial_chain(&valid_chain, &catalog).is_ok());

        let invalid_chain = vec!["jump_slash".to_owned(), "dive_kick".to_owned()];
        let result = validate_aerial_chain(&invalid_chain, &catalog);
        assert!(matches!(result, Err(CombatError::ChainLinkInvalid { .. })));
    }
}
