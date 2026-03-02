use crate::db::quorum::{QuorumSelectionError, select_nearest_healthy_quorum};
use crate::db::routing::health::{HealthState, NodeHealth};
use crate::db::routing::{
    CompiledRoutingPolicy, RoutingPolicyError, RoutingPolicySpec, compile_policy,
};
use crate::db::security::residency::{ResidencyErrorToken, ResidencyPolicy};
use crate::db::{
    DbAutopilotAuditRow, DbClientWritePathAggregate, DbCommitVisibilityStatus, DbError,
    DbHealthStatus, DbIntentConflict, DbIntentEffective, DbRecommendation, DbTieringState,
    DbWalFlushStats, DbWriteStageAggregate,
    autopilot_last_actions as runtime_autopilot_last_actions,
    db_client_write_path_aggregate as runtime_db_client_write_path_aggregate,
    db_commit_visibility_status as runtime_db_commit_visibility_status,
    db_health_status as runtime_db_health_status, db_wal_flush_stats as runtime_db_wal_flush_stats,
    db_write_stage_aggregate as runtime_db_write_stage_aggregate,
    intent_conflicts as runtime_intent_conflicts, intent_effective as runtime_intent_effective,
    recommendations as runtime_recommendations, tiering_state as runtime_tiering_state,
};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClusterHealthSnapshot {
    pub total_nodes: usize,
    pub healthy_nodes: usize,
    pub degraded_nodes: usize,
    pub unavailable_nodes: usize,
    pub nodes_by_region: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuorumExplainSummary {
    pub quorum_size: usize,
    pub selected_nodes: Vec<String>,
    pub max_selected_latency_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyExplainSummary {
    pub policy_id: String,
    pub policy_hash: u64,
    pub shard_fields: Vec<String>,
    pub shard_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResidencyAuditResult {
    pub shard: Vec<u8>,
    pub sink_region: String,
    pub allowed: bool,
    pub token: Option<&'static str>,
    pub reason: String,
}

pub fn cluster_health_snapshot(nodes: &[NodeHealth]) -> ClusterHealthSnapshot {
    let mut healthy_nodes = 0usize;
    let mut degraded_nodes = 0usize;
    let mut unavailable_nodes = 0usize;
    let mut nodes_by_region = BTreeMap::new();

    for node in nodes {
        match node.state {
            HealthState::Healthy => healthy_nodes += 1,
            HealthState::Degraded => degraded_nodes += 1,
            HealthState::Unavailable => unavailable_nodes += 1,
        }
        *nodes_by_region.entry(node.region.clone()).or_insert(0) += 1;
    }

    ClusterHealthSnapshot {
        total_nodes: nodes.len(),
        healthy_nodes,
        degraded_nodes,
        unavailable_nodes,
        nodes_by_region,
    }
}

pub fn quorum_explain_summary(
    nodes: &[NodeHealth],
    desired_voters: usize,
    latency_hint_ms: &BTreeMap<String, u64>,
) -> Result<QuorumExplainSummary, QuorumSelectionError> {
    let selection = select_nearest_healthy_quorum(nodes, desired_voters, latency_hint_ms)?;
    let max_selected_latency_ms = selection
        .selected_nodes
        .iter()
        .map(|node| latency_hint_ms.get(node).copied().unwrap_or(u64::MAX / 2))
        .max()
        .unwrap_or(0);

    Ok(QuorumExplainSummary {
        quorum_size: selection.quorum_size,
        selected_nodes: selection.selected_nodes,
        max_selected_latency_ms,
    })
}

pub fn policy_explain_summary(
    spec: &RoutingPolicySpec,
) -> Result<PolicyExplainSummary, RoutingPolicyError> {
    let compiled: CompiledRoutingPolicy = compile_policy(spec)?;
    let policy_id = compiled.policy_id.clone();
    let policy_hash = compiled.policy_hash;
    let shard_fields = compiled.shard_key_policy().shard_fields().to_vec();
    let shard_count = compiled.shard_key_policy().shard_count();
    Ok(PolicyExplainSummary {
        policy_id,
        policy_hash,
        shard_fields,
        shard_count,
    })
}

pub fn residency_audit(
    shard: &[u8],
    sink_region: &str,
    policy: &ResidencyPolicy,
) -> ResidencyAuditResult {
    match policy.authorize_egress(shard, sink_region) {
        Ok(()) => ResidencyAuditResult {
            shard: shard.to_vec(),
            sink_region: sink_region.to_string(),
            allowed: true,
            token: None,
            reason: "allowed".to_string(),
        },
        Err(err) => ResidencyAuditResult {
            shard: shard.to_vec(),
            sink_region: sink_region.to_string(),
            allowed: false,
            token: Some(match err.token {
                ResidencyErrorToken::EgressDeny => ResidencyErrorToken::EgressDeny.as_str(),
                ResidencyErrorToken::EgressPolicyUnsat => {
                    ResidencyErrorToken::EgressPolicyUnsat.as_str()
                }
            }),
            reason: err.reason,
        },
    }
}

pub fn db_health_status(handle: i64) -> Result<DbHealthStatus, DbError> {
    runtime_db_health_status(handle)
}

pub fn db_commit_visibility_status(handle: i64) -> Result<DbCommitVisibilityStatus, DbError> {
    runtime_db_commit_visibility_status(handle)
}

pub fn db_write_stage_aggregate(handle: i64) -> Result<DbWriteStageAggregate, DbError> {
    runtime_db_write_stage_aggregate(handle)
}

pub fn db_client_write_path_aggregate(handle: i64) -> Result<DbClientWritePathAggregate, DbError> {
    runtime_db_client_write_path_aggregate(handle)
}

pub fn db_wal_flush_stats(handle: i64) -> Result<DbWalFlushStats, DbError> {
    runtime_db_wal_flush_stats(handle)
}

pub fn intent_effective(handle: i64) -> Result<DbIntentEffective, DbError> {
    runtime_intent_effective(handle)
}

pub fn intent_conflicts(handle: i64) -> Result<Vec<DbIntentConflict>, DbError> {
    runtime_intent_conflicts(handle)
}

pub fn autopilot_last_actions(
    handle: i64,
    limit: usize,
) -> Result<Vec<DbAutopilotAuditRow>, DbError> {
    runtime_autopilot_last_actions(handle, limit)
}

pub fn tiering_state(handle: i64) -> Result<DbTieringState, DbError> {
    runtime_tiering_state(handle)
}

pub fn recommendations(handle: i64) -> Result<Vec<DbRecommendation>, DbError> {
    runtime_recommendations(handle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::routing::RoutingPolicySpec;
    use crate::db::routing::health::MemberRole;
    use crate::db::security::residency::{ResidencyPolicy, ResidencyRule};
    use crate::db::{close_db, open_db, submit_put};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn temp_dir() -> PathBuf {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        let base = std::env::temp_dir().join(format!(
            "wrela_db_admin_api_test_{}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("unix epoch")
                .as_nanos(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&base).expect("create temp dir");
        base
    }

    #[test]
    fn cluster_health_snapshot_counts_states_and_regions() {
        let nodes = vec![
            NodeHealth {
                node_id: "n1".to_string(),
                region: "us".to_string(),
                state: HealthState::Healthy,
                observed_at_ms: 1,
                role: MemberRole::Unknown,
            },
            NodeHealth {
                node_id: "n2".to_string(),
                region: "us".to_string(),
                state: HealthState::Degraded,
                observed_at_ms: 1,
                role: MemberRole::Unknown,
            },
            NodeHealth {
                node_id: "n3".to_string(),
                region: "eu".to_string(),
                state: HealthState::Unavailable,
                observed_at_ms: 1,
                role: MemberRole::Unknown,
            },
        ];

        let snapshot = cluster_health_snapshot(&nodes);
        assert_eq!(snapshot.total_nodes, 3);
        assert_eq!(snapshot.healthy_nodes, 1);
        assert_eq!(snapshot.degraded_nodes, 1);
        assert_eq!(snapshot.unavailable_nodes, 1);
        assert_eq!(snapshot.nodes_by_region.get("us"), Some(&2));
        assert_eq!(snapshot.nodes_by_region.get("eu"), Some(&1));
    }

    #[test]
    fn quorum_explain_selects_nodes_and_latency() {
        let nodes = vec![
            NodeHealth {
                node_id: "n1".to_string(),
                region: "us".to_string(),
                state: HealthState::Healthy,
                observed_at_ms: 1,
                role: MemberRole::Unknown,
            },
            NodeHealth {
                node_id: "n2".to_string(),
                region: "eu".to_string(),
                state: HealthState::Healthy,
                observed_at_ms: 1,
                role: MemberRole::Unknown,
            },
            NodeHealth {
                node_id: "n3".to_string(),
                region: "ap".to_string(),
                state: HealthState::Degraded,
                observed_at_ms: 1,
                role: MemberRole::Unknown,
            },
        ];
        let latency = BTreeMap::from([
            ("n1".to_string(), 5_u64),
            ("n2".to_string(), 7_u64),
            ("n3".to_string(), 10_u64),
        ]);

        let summary = quorum_explain_summary(&nodes, 3, &latency).expect("summary");
        assert_eq!(summary.quorum_size, 2);
        assert_eq!(
            summary.selected_nodes,
            vec!["n1".to_string(), "n2".to_string()]
        );
        assert_eq!(summary.max_selected_latency_ms, 7);
    }

    #[test]
    fn policy_explain_is_deterministic() {
        let spec = RoutingPolicySpec {
            policy_id: "orders".to_string(),
            shard_fields: vec!["tenant".to_string(), "order".to_string()],
            shard_count: 32,
            single_field_waiver_reason: None,
        };

        let a = policy_explain_summary(&spec).expect("explain");
        let b = policy_explain_summary(&spec).expect("explain");
        assert_eq!(a, b);
    }

    #[test]
    fn residency_audit_fails_closed_on_policy_unsat() {
        let policy = ResidencyPolicy::with_rules(vec![ResidencyRule {
            shard: b"core".to_vec(),
            allowed_regions: vec!["us".to_string()],
        }]);

        let denied = residency_audit(b"core", "eu", &policy);
        assert!(!denied.allowed);
        assert_eq!(denied.token, Some(ResidencyErrorToken::EgressDeny.as_str()));

        let unsat = residency_audit(b"missing", "us", &policy);
        assert!(!unsat.allowed);
        assert_eq!(
            unsat.token,
            Some(ResidencyErrorToken::EgressPolicyUnsat.as_str())
        );
    }

    #[test]
    fn db_health_status_reports_clean_engine_after_open() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open");
        let health = super::db_health_status(handle).expect("health");
        assert!(health.clock_persist_error.is_none());
        assert!(health.raft_persist_error.is_none());
        assert!(health.cdc_checkpoint_persist_error.is_none());
        assert!(close_db(handle));
    }

    #[test]
    fn autopilot_surfaces_are_typed_and_non_panicking() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open");
        let effective = super::intent_effective(handle).expect("intent effective");
        assert_eq!(effective.mode, "full_auto");
        assert!(effective.replication_factor >= 1);

        let _conflicts = super::intent_conflicts(handle).expect("intent conflicts");
        let actions = super::autopilot_last_actions(handle, 8).expect("actions");
        assert!(
            !actions.is_empty(),
            "boot tick should emit at least one action"
        );

        let tiering = super::tiering_state(handle).expect("tiering state");
        assert!(tiering.boundary_max_live_bytes >= tiering.boundary_min_live_bytes);

        let _recommendations = super::recommendations(handle).expect("recommendations");
        assert!(close_db(handle));
    }

    #[test]
    fn db_write_stage_aggregate_reports_committed_samples() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open");
        submit_put(
            handle,
            b"core".to_vec(),
            b"telemetry-k".to_vec(),
            b"telemetry-v".to_vec(),
            None,
        )
        .expect("write");
        let aggregate = super::db_write_stage_aggregate(handle).expect("aggregate");
        assert!(aggregate.sample_count >= 1);
        assert!(aggregate.op_count >= 1);
        assert!(aggregate.validate_route_pct >= 0.0);
        assert!(aggregate.retry_after_pct >= 0.0);
        assert!(close_db(handle));
    }

    #[test]
    fn db_client_write_path_aggregate_reports_client_samples() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open");
        submit_put(
            handle,
            b"core".to_vec(),
            b"client-telemetry-k".to_vec(),
            b"client-telemetry-v".to_vec(),
            None,
        )
        .expect("write");
        let aggregate = super::db_client_write_path_aggregate(handle).expect("aggregate");
        assert!(aggregate.sample_count >= 1);
        assert!(aggregate.avg_total_us >= 0.0);
        assert!(aggregate.response_wait_pct >= 0.0);
        assert!(close_db(handle));
    }

    #[test]
    fn db_wal_flush_stats_reports_flush_activity() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open");
        submit_put(
            handle,
            b"core".to_vec(),
            b"wal-k".to_vec(),
            b"wal-v".to_vec(),
            None,
        )
        .expect("write");
        let stats = super::db_wal_flush_stats(handle).expect("wal stats");
        assert!(stats.flushes >= 1);
        assert!(stats.avg_ops_per_flush >= 1.0);
        assert!(close_db(handle));
    }
}
