use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemberRole {
    Voter,
    Learner,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    pub node_id: u64,
    pub role: MemberRole,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MembershipChange {
    AddVoter { node_id: u64 },
    RemoveVoter { node_id: u64 },
    AddLearner { node_id: u64 },
    RemoveLearner { node_id: u64 },
    PromoteLearner { node_id: u64 },
    DemoteVoter { node_id: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MembershipError {
    EmptyVoterSet,
    JointConfigInProgress,
    NoJointConfig,
    NodeAlreadyVoter(u64),
    NodeAlreadyLearner(u64),
    NodeMissing(u64),
    LastVoterRemovalDenied(u64),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JointMembership {
    pub outgoing_voters: BTreeSet<u64>,
    pub incoming_voters: BTreeSet<u64>,
    pub outgoing_learners: BTreeSet<u64>,
    pub started_at_log_index: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MembershipConfig {
    voters: BTreeSet<u64>,
    learners: BTreeSet<u64>,
    joint: Option<JointMembership>,
}

fn quorum_size(voters: usize) -> usize {
    (voters / 2) + 1
}

fn has_quorum(voters: &BTreeSet<u64>, durable_acks: &BTreeSet<u64>) -> bool {
    let acked = voters.iter().filter(|v| durable_acks.contains(v)).count();
    acked >= quorum_size(voters.len())
}

impl MembershipConfig {
    pub fn new(initial_voters: impl IntoIterator<Item = u64>) -> Result<Self, MembershipError> {
        let voters: BTreeSet<u64> = initial_voters.into_iter().collect();
        if voters.is_empty() {
            return Err(MembershipError::EmptyVoterSet);
        }
        Ok(Self {
            voters,
            learners: BTreeSet::new(),
            joint: None,
        })
    }

    pub fn voters(&self) -> &BTreeSet<u64> {
        &self.voters
    }

    pub fn learners(&self) -> &BTreeSet<u64> {
        &self.learners
    }

    pub fn joint(&self) -> Option<&JointMembership> {
        self.joint.as_ref()
    }

    pub fn is_voter(&self, node_id: u64) -> bool {
        self.voters.contains(&node_id)
    }

    pub fn is_learner(&self, node_id: u64) -> bool {
        self.learners.contains(&node_id)
    }

    pub fn begin_joint_change(
        &mut self,
        change: MembershipChange,
        log_index: u64,
    ) -> Result<(), MembershipError> {
        if self.joint.is_some() {
            return Err(MembershipError::JointConfigInProgress);
        }

        let mut incoming_voters = self.voters.clone();
        let mut incoming_learners = self.learners.clone();
        apply_change(&mut incoming_voters, &mut incoming_learners, change)?;

        self.joint = Some(JointMembership {
            outgoing_voters: self.voters.clone(),
            incoming_voters,
            outgoing_learners: self.learners.clone(),
            started_at_log_index: log_index,
        });
        self.learners = incoming_learners;
        Ok(())
    }

    pub fn commit_joint_change(&mut self) -> Result<(), MembershipError> {
        let joint = self.joint.take().ok_or(MembershipError::NoJointConfig)?;
        if joint.incoming_voters.is_empty() {
            return Err(MembershipError::EmptyVoterSet);
        }
        self.voters = joint.incoming_voters;
        Ok(())
    }

    pub fn abort_joint_change(&mut self) -> Result<(), MembershipError> {
        let joint = self.joint.take().ok_or(MembershipError::NoJointConfig)?;
        self.voters = joint.outgoing_voters;
        self.learners = joint.outgoing_learners;
        Ok(())
    }

    pub fn has_durable_quorum(&self, durable_acks: &BTreeSet<u64>) -> bool {
        if let Some(joint) = &self.joint {
            has_quorum(&joint.outgoing_voters, durable_acks)
                && has_quorum(&joint.incoming_voters, durable_acks)
        } else {
            has_quorum(&self.voters, durable_acks)
        }
    }
}

fn apply_change(
    voters: &mut BTreeSet<u64>,
    learners: &mut BTreeSet<u64>,
    change: MembershipChange,
) -> Result<(), MembershipError> {
    match change {
        MembershipChange::AddVoter { node_id } => {
            if voters.contains(&node_id) {
                return Err(MembershipError::NodeAlreadyVoter(node_id));
            }
            voters.insert(node_id);
            learners.remove(&node_id);
            Ok(())
        }
        MembershipChange::RemoveVoter { node_id } => {
            if !voters.contains(&node_id) {
                return Err(MembershipError::NodeMissing(node_id));
            }
            if voters.len() == 1 {
                return Err(MembershipError::LastVoterRemovalDenied(node_id));
            }
            voters.remove(&node_id);
            learners.remove(&node_id);
            Ok(())
        }
        MembershipChange::AddLearner { node_id } => {
            if voters.contains(&node_id) {
                return Err(MembershipError::NodeAlreadyVoter(node_id));
            }
            if !learners.insert(node_id) {
                return Err(MembershipError::NodeAlreadyLearner(node_id));
            }
            Ok(())
        }
        MembershipChange::RemoveLearner { node_id } => {
            if !learners.remove(&node_id) {
                return Err(MembershipError::NodeMissing(node_id));
            }
            Ok(())
        }
        MembershipChange::PromoteLearner { node_id } => {
            if voters.contains(&node_id) {
                return Err(MembershipError::NodeAlreadyVoter(node_id));
            }
            if !learners.remove(&node_id) {
                return Err(MembershipError::NodeMissing(node_id));
            }
            voters.insert(node_id);
            Ok(())
        }
        MembershipChange::DemoteVoter { node_id } => {
            if !voters.contains(&node_id) {
                return Err(MembershipError::NodeMissing(node_id));
            }
            if voters.len() == 1 {
                return Err(MembershipError::LastVoterRemovalDenied(node_id));
            }
            voters.remove(&node_id);
            learners.insert(node_id);
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joint_change_requires_dual_quorum() {
        let mut cfg = MembershipConfig::new([1, 2, 3]).expect("init");
        cfg.begin_joint_change(MembershipChange::AddVoter { node_id: 4 }, 12)
            .expect("joint");

        let only_old = BTreeSet::from([1, 2]);
        assert!(!cfg.has_durable_quorum(&only_old));

        let dual = BTreeSet::from([1, 2, 4]);
        assert!(cfg.has_durable_quorum(&dual));
    }

    #[test]
    fn commit_joint_promote_learner_transitions_sets() {
        let mut cfg = MembershipConfig::new([1, 2, 3]).expect("init");
        cfg.begin_joint_change(MembershipChange::AddLearner { node_id: 9 }, 20)
            .expect("add learner");
        cfg.commit_joint_change().expect("commit learner add");
        assert!(cfg.is_learner(9));

        cfg.begin_joint_change(MembershipChange::PromoteLearner { node_id: 9 }, 21)
            .expect("promote");
        cfg.commit_joint_change().expect("commit promote");

        assert!(cfg.is_voter(9));
        assert!(!cfg.is_learner(9));
    }

    #[test]
    fn removing_last_voter_is_rejected() {
        let mut cfg = MembershipConfig::new([11]).expect("init");
        let err = cfg
            .begin_joint_change(MembershipChange::RemoveVoter { node_id: 11 }, 5)
            .expect_err("must reject");
        assert_eq!(err, MembershipError::LastVoterRemovalDenied(11));
    }

    #[test]
    fn abort_restores_outgoing_learners_exactly() {
        let mut cfg = MembershipConfig::new([1, 2, 3]).expect("init");
        cfg.begin_joint_change(MembershipChange::AddLearner { node_id: 9 }, 1)
            .expect("add learner");
        cfg.commit_joint_change().expect("commit add learner");
        assert!(cfg.is_learner(9));

        cfg.begin_joint_change(MembershipChange::PromoteLearner { node_id: 9 }, 2)
            .expect("promote learner");
        assert!(
            cfg.joint()
                .expect("joint in progress")
                .incoming_voters
                .contains(&9)
        );
        assert!(!cfg.is_learner(9));

        cfg.abort_joint_change()
            .expect("abort must restore prior sets");
        assert!(!cfg.is_voter(9));
        assert!(cfg.is_learner(9));
    }
}
