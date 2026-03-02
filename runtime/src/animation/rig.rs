use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RigJoint {
    pub name: String,
    pub parent_index: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RigContract {
    pub rig_id: String,
    pub revision: u32,
    pub joints: Vec<RigJoint>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RigValidationError {
    EmptyRig,
    DuplicateJointName(String),
    ParentOutOfRange { joint_index: u16, parent_index: u16 },
    ParentNotTopologicallyPrior { joint_index: u16, parent_index: u16 },
}

pub fn validate_rig(contract: &RigContract) -> Result<(), RigValidationError> {
    if contract.joints.is_empty() {
        return Err(RigValidationError::EmptyRig);
    }

    let mut seen_names = HashSet::with_capacity(contract.joints.len());
    for (joint_index, joint) in contract.joints.iter().enumerate() {
        if !seen_names.insert(joint.name.as_str()) {
            return Err(RigValidationError::DuplicateJointName(joint.name.clone()));
        }

        if let Some(parent_index) = joint.parent_index {
            if usize::from(parent_index) >= contract.joints.len() {
                return Err(RigValidationError::ParentOutOfRange {
                    joint_index: joint_index as u16,
                    parent_index,
                });
            }
            if usize::from(parent_index) >= joint_index {
                return Err(RigValidationError::ParentNotTopologicallyPrior {
                    joint_index: joint_index as u16,
                    parent_index,
                });
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{RigContract, RigJoint, RigValidationError, validate_rig};

    #[test]
    fn reject_duplicate_joint_names() {
        let contract = RigContract {
            rig_id: "humanoid-v2".to_string(),
            revision: 2,
            joints: vec![
                RigJoint {
                    name: "root".to_string(),
                    parent_index: None,
                },
                RigJoint {
                    name: "root".to_string(),
                    parent_index: Some(0),
                },
            ],
        };

        assert!(matches!(
            validate_rig(&contract),
            Err(RigValidationError::DuplicateJointName(name)) if name == "root"
        ));
    }
}
