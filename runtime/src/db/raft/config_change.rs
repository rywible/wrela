use crate::db::raft::membership::{Member, MemberRole, MembershipChange, MembershipConfig};

pub fn promote_learner(members: &mut [Member], node_id: u64) -> bool {
    for m in members {
        if m.node_id == node_id && m.role == MemberRole::Learner {
            m.role = MemberRole::Voter;
            return true;
        }
    }
    false
}

pub fn begin_promote_learner(config: &mut MembershipConfig, node_id: u64, log_index: u64) -> bool {
    config
        .begin_joint_change(MembershipChange::PromoteLearner { node_id }, log_index)
        .is_ok()
}
