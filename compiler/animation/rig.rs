#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Constraint {
    pub bone: String,
    pub min_angle_deg: i16,
    pub max_angle_deg: i16,
    pub sampled_angle_deg: i16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RigError {
    ParentOutOfRange {
        child: usize,
        parent: usize,
    },
    CycleDetected {
        node: usize,
    },
    InvalidConstraintBounds {
        bone: String,
    },
    ConstraintOutOfBounds {
        bone: String,
        sampled: i16,
        min: i16,
        max: i16,
    },
}

pub fn validate_acyclic(parents: &[Option<usize>]) -> Result<(), RigError> {
    for (child, parent) in parents.iter().enumerate() {
        if let Some(parent_index) = parent {
            if *parent_index >= parents.len() {
                return Err(RigError::ParentOutOfRange {
                    child,
                    parent: *parent_index,
                });
            }
        }
    }

    fn visit(node: usize, parents: &[Option<usize>], states: &mut [u8]) -> Result<(), RigError> {
        match states[node] {
            2 => return Ok(()),
            1 => return Err(RigError::CycleDetected { node }),
            _ => {}
        }

        states[node] = 1;
        if let Some(parent) = parents[node] {
            visit(parent, parents, states)?;
        }
        states[node] = 2;
        Ok(())
    }

    let mut states = vec![0_u8; parents.len()];
    for node in 0..parents.len() {
        if states[node] == 0 {
            visit(node, parents, &mut states)?;
        }
    }
    Ok(())
}

pub fn enforce_constraint_bounds(constraint: &Constraint) -> Result<(), RigError> {
    if constraint.min_angle_deg > constraint.max_angle_deg {
        return Err(RigError::InvalidConstraintBounds {
            bone: constraint.bone.clone(),
        });
    }
    if constraint.sampled_angle_deg < constraint.min_angle_deg
        || constraint.sampled_angle_deg > constraint.max_angle_deg
    {
        return Err(RigError::ConstraintOutOfBounds {
            bone: constraint.bone.clone(),
            sampled: constraint.sampled_angle_deg,
            min: constraint.min_angle_deg,
            max: constraint.max_angle_deg,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Constraint, RigError, enforce_constraint_bounds, validate_acyclic};

    #[test]
    fn rejects_cycles() {
        let cyclic_parents = vec![Some(1), Some(2), Some(0)];
        let result = validate_acyclic(&cyclic_parents);
        assert!(matches!(result, Err(RigError::CycleDetected { .. })));
    }

    #[test]
    fn constraint_bounds_enforced() {
        let valid = Constraint {
            bone: "knee_l".to_owned(),
            min_angle_deg: -10,
            max_angle_deg: 125,
            sampled_angle_deg: 64,
        };
        assert!(enforce_constraint_bounds(&valid).is_ok());

        let invalid = Constraint {
            sampled_angle_deg: 140,
            ..valid
        };
        let result = enforce_constraint_bounds(&invalid);
        assert!(matches!(
            result,
            Err(RigError::ConstraintOutOfBounds { .. })
        ));
    }
}
