use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClusterState {
    pub voters: BTreeSet<String>,
    pub learners: BTreeSet<String>,
    pub drained_regions: BTreeSet<String>,
    pub node_regions: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrchestrationTrace {
    pub operation: String,
    pub steps: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MembershipError {
    NodeExists(String),
    NodeMissing(String),
    ReplacementTargetExists(String),
    LastVoterRemovalDenied,
    MetadataAuthorityNodeMissing(String),
    MetadataAuthorityUnavailable(String),
    MetadataAuthorityNoEligibleFailover,
    MetadataAuthorityRegionDrained(String),
    MetadataAuthorityEpochMismatch { expected: u64, current: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetadataAuthority {
    pub node_id: String,
    pub epoch: u64,
    pub lease_ttl_ms: u64,
    pub leased_at_ms: u64,
}

impl ClusterState {
    pub fn new(voters: BTreeMap<String, String>) -> Self {
        let mut voter_set = BTreeSet::new();
        let mut node_regions = BTreeMap::new();
        for (node, region) in voters {
            voter_set.insert(node.clone());
            node_regions.insert(node, region);
        }
        Self {
            voters: voter_set,
            learners: BTreeSet::new(),
            drained_regions: BTreeSet::new(),
            node_regions,
        }
    }
}

pub fn add_learner(
    state: &mut ClusterState,
    node_id: &str,
    region: &str,
) -> Result<OrchestrationTrace, MembershipError> {
    if state.voters.contains(node_id) || state.learners.contains(node_id) {
        return Err(MembershipError::NodeExists(node_id.to_string()));
    }
    state.learners.insert(node_id.to_string());
    state
        .node_regions
        .insert(node_id.to_string(), region.to_string());
    Ok(OrchestrationTrace {
        operation: "add_learner".to_string(),
        steps: vec![
            format!("plan:add_learner:{node_id}:{region}"),
            format!("apply:add_learner:{node_id}"),
        ],
    })
}

pub fn promote_learner(
    state: &mut ClusterState,
    node_id: &str,
) -> Result<OrchestrationTrace, MembershipError> {
    if !state.learners.remove(node_id) {
        return Err(MembershipError::NodeMissing(node_id.to_string()));
    }
    state.voters.insert(node_id.to_string());
    Ok(OrchestrationTrace {
        operation: "promote_learner".to_string(),
        steps: vec![
            format!("plan:promote:{node_id}"),
            format!("apply:promote:{node_id}"),
        ],
    })
}

pub fn remove_voter(
    state: &mut ClusterState,
    node_id: &str,
) -> Result<OrchestrationTrace, MembershipError> {
    if !state.voters.contains(node_id) {
        return Err(MembershipError::NodeMissing(node_id.to_string()));
    }
    if state.voters.len() <= 1 {
        return Err(MembershipError::LastVoterRemovalDenied);
    }
    state.voters.remove(node_id);
    Ok(OrchestrationTrace {
        operation: "remove_voter".to_string(),
        steps: vec![
            format!("plan:remove_voter:{node_id}"),
            format!("apply:remove_voter:{node_id}"),
        ],
    })
}

pub fn replace_node(
    state: &mut ClusterState,
    old_node: &str,
    new_node: &str,
    region: &str,
) -> Result<OrchestrationTrace, MembershipError> {
    if !state.voters.contains(old_node) {
        return Err(MembershipError::NodeMissing(old_node.to_string()));
    }
    if state.voters.contains(new_node) || state.learners.contains(new_node) {
        return Err(MembershipError::ReplacementTargetExists(
            new_node.to_string(),
        ));
    }

    let mut steps = Vec::new();
    steps.push(format!("plan:replace:{old_node}:{new_node}"));
    steps.extend(add_learner(state, new_node, region)?.steps);
    steps.extend(promote_learner(state, new_node)?.steps);
    steps.extend(remove_voter(state, old_node)?.steps);
    state.node_regions.remove(old_node);

    Ok(OrchestrationTrace {
        operation: "replace_node".to_string(),
        steps,
    })
}

pub fn drain_region(state: &mut ClusterState, region: &str) -> OrchestrationTrace {
    state.drained_regions.insert(region.to_string());
    OrchestrationTrace {
        operation: "drain_region".to_string(),
        steps: vec![
            format!("plan:drain_region:{region}"),
            format!("apply:no_new_leaders:{region}"),
            format!("apply:migrate_leaders:{region}"),
        ],
    }
}

pub fn bootstrap_metadata_authority(
    state: &ClusterState,
    node_id: &str,
    lease_ttl_ms: u64,
    now_ms: u64,
) -> Result<(MetadataAuthority, OrchestrationTrace), MembershipError> {
    if !state.voters.contains(node_id) {
        return Err(MembershipError::MetadataAuthorityNodeMissing(
            node_id.to_string(),
        ));
    }
    if lease_ttl_ms == 0 {
        return Err(MembershipError::MetadataAuthorityUnavailable(
            "lease ttl must be positive".to_string(),
        ));
    }
    Ok((
        MetadataAuthority {
            node_id: node_id.to_string(),
            epoch: 1,
            lease_ttl_ms,
            leased_at_ms: now_ms,
        },
        OrchestrationTrace {
            operation: "bootstrap_metadata_authority".to_string(),
            steps: vec![
                format!("plan:metadata_authority:bootstrap:{node_id}"),
                format!("apply:metadata_authority:epoch=1:{node_id}"),
            ],
        },
    ))
}

pub fn failover_metadata_authority(
    state: &ClusterState,
    authority: &mut MetadataAuthority,
    unavailable_nodes: &BTreeSet<String>,
    now_ms: u64,
) -> Result<OrchestrationTrace, MembershipError> {
    if !unavailable_nodes.contains(&authority.node_id) {
        return Err(MembershipError::MetadataAuthorityUnavailable(
            authority.node_id.clone(),
        ));
    }
    let candidate = state
        .voters
        .iter()
        .filter(|node| *node != &authority.node_id)
        .filter(|node| !unavailable_nodes.contains(*node))
        .find(|node| {
            state
                .node_regions
                .get(*node)
                .map(|region| !state.drained_regions.contains(region))
                .unwrap_or(true)
        })
        .cloned()
        .ok_or(MembershipError::MetadataAuthorityNoEligibleFailover)?;

    authority.node_id = candidate.clone();
    authority.epoch = authority.epoch.saturating_add(1);
    authority.leased_at_ms = now_ms;
    Ok(OrchestrationTrace {
        operation: "failover_metadata_authority".to_string(),
        steps: vec![
            format!("plan:metadata_authority:failover:{}", authority.node_id),
            format!(
                "apply:metadata_authority:epoch={}:{}",
                authority.epoch, authority.node_id
            ),
        ],
    })
}

pub fn rebootstrap_metadata_authority(
    state: &ClusterState,
    authority: &mut MetadataAuthority,
    expected_epoch: u64,
    target_node: &str,
    now_ms: u64,
) -> Result<OrchestrationTrace, MembershipError> {
    if authority.epoch != expected_epoch {
        return Err(MembershipError::MetadataAuthorityEpochMismatch {
            expected: expected_epoch,
            current: authority.epoch,
        });
    }
    if !state.voters.contains(target_node) {
        return Err(MembershipError::MetadataAuthorityNodeMissing(
            target_node.to_string(),
        ));
    }
    if let Some(region) = state.node_regions.get(target_node) {
        if state.drained_regions.contains(region) {
            return Err(MembershipError::MetadataAuthorityRegionDrained(
                region.clone(),
            ));
        }
    }
    authority.node_id = target_node.to_string();
    authority.epoch = authority.epoch.saturating_add(1);
    authority.leased_at_ms = now_ms;
    Ok(OrchestrationTrace {
        operation: "rebootstrap_metadata_authority".to_string(),
        steps: vec![
            format!("plan:metadata_authority:rebootstrap:{target_node}"),
            format!(
                "apply:metadata_authority:epoch={}:{}",
                authority.epoch, authority.node_id
            ),
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> ClusterState {
        ClusterState::new(BTreeMap::from([
            ("n1".to_string(), "us".to_string()),
            ("n2".to_string(), "eu".to_string()),
            ("n3".to_string(), "ap".to_string()),
        ]))
    }

    #[test]
    fn replace_node_is_deterministic_and_idempotent_safe() {
        let mut st = state();
        let trace = replace_node(&mut st, "n1", "n4", "us").expect("replace");
        assert_eq!(trace.operation, "replace_node");
        assert!(st.voters.contains("n4"));
        assert!(!st.voters.contains("n1"));
    }

    #[test]
    fn drain_region_records_trace_steps() {
        let mut st = state();
        let trace = drain_region(&mut st, "eu");
        assert!(st.drained_regions.contains("eu"));
        assert_eq!(trace.steps[0], "plan:drain_region:eu");
    }

    #[test]
    fn bootstrap_metadata_authority_initializes_epoch_one() {
        let st = state();
        let (authority, trace) =
            bootstrap_metadata_authority(&st, "n1", 30_000, 1_000).expect("bootstrap");
        assert_eq!(authority.node_id, "n1");
        assert_eq!(authority.epoch, 1);
        assert_eq!(authority.lease_ttl_ms, 30_000);
        assert_eq!(trace.operation, "bootstrap_metadata_authority");
    }

    #[test]
    fn failover_metadata_authority_promotes_next_healthy_voter() {
        let st = state();
        let (mut authority, _) =
            bootstrap_metadata_authority(&st, "n1", 30_000, 100).expect("bootstrap");
        let unavailable = BTreeSet::from(["n1".to_string()]);
        let trace =
            failover_metadata_authority(&st, &mut authority, &unavailable, 500).expect("failover");
        assert_eq!(authority.node_id, "n2");
        assert_eq!(authority.epoch, 2);
        assert_eq!(authority.leased_at_ms, 500);
        assert_eq!(trace.operation, "failover_metadata_authority");
    }

    #[test]
    fn failover_metadata_authority_skips_drained_regions() {
        let mut st = state();
        drain_region(&mut st, "eu");
        let (mut authority, _) =
            bootstrap_metadata_authority(&st, "n1", 30_000, 100).expect("bootstrap");
        let unavailable = BTreeSet::from(["n1".to_string()]);
        failover_metadata_authority(&st, &mut authority, &unavailable, 500).expect("failover");
        assert_eq!(authority.node_id, "n3");
    }

    #[test]
    fn rebootstrap_metadata_authority_rejects_stale_epoch() {
        let st = state();
        let (mut authority, _) =
            bootstrap_metadata_authority(&st, "n1", 30_000, 100).expect("bootstrap");
        let err =
            rebootstrap_metadata_authority(&st, &mut authority, 0, "n2", 700).expect_err("stale");
        assert_eq!(
            err,
            MembershipError::MetadataAuthorityEpochMismatch {
                expected: 0,
                current: 1
            }
        );
    }

    #[test]
    fn rebootstrap_metadata_authority_moves_authority_and_bumps_epoch() {
        let st = state();
        let (mut authority, _) =
            bootstrap_metadata_authority(&st, "n1", 30_000, 100).expect("bootstrap");
        let trace =
            rebootstrap_metadata_authority(&st, &mut authority, 1, "n2", 700).expect("rebootstrap");
        assert_eq!(authority.node_id, "n2");
        assert_eq!(authority.epoch, 2);
        assert_eq!(trace.operation, "rebootstrap_metadata_authority");
    }
}
