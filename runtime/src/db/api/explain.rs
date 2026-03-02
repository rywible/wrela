use crate::db::admin_api::{
    ClusterHealthSnapshot, PolicyExplainSummary, QuorumExplainSummary, ResidencyAuditResult,
    cluster_health_snapshot as cluster_health_snapshot_impl,
    policy_explain_summary as policy_explain_summary_impl,
    quorum_explain_summary as quorum_explain_summary_impl, residency_audit as residency_audit_impl,
};
use crate::db::net::interceptor::{intercept_rpc, intercept_rpc_with_pki};
use crate::db::routing::RoutingPolicySpec;
use crate::db::routing::health::NodeHealth;
use crate::db::security::authz::{CertIdentity, RpcClass};
use crate::db::security::pki::PkiStore;
use crate::db::security::residency::ResidencyPolicy;
use crate::db::time::safe_time::{SafeTimeDiagnostics, SafeTimeLagBudget};
use crate::db::{
    DbAutopilotAuditRow, DbAutoscaleStatus, DbClientWritePathAggregate, DbCommitVisibilityStatus,
    DbError, DbHealthStatus, DbIntentConflict, DbIntentEffective, DbPrivateMeshStatus,
    DbRecommendation, DbTieringState, DbTopologyStatus, DbWalFlushStats, DbWriteStageAggregate,
    OwnerRecord, active_group_count as db_active_group_count,
    autopilot_last_actions as db_autopilot_last_actions, autoscale_status as db_autoscale_status,
    checkpoint_list as db_checkpoint_list,
    db_client_write_path_aggregate as db_client_write_path_aggregate_impl,
    db_commit_visibility_status as db_commit_visibility_status_impl,
    db_health_status as db_health_status_impl, db_wal_flush_stats as db_wal_flush_stats_impl,
    db_write_stage_aggregate as db_write_stage_aggregate_impl,
    global_route_lookup as db_global_route_lookup, intent_conflicts as db_intent_conflicts,
    intent_effective as db_intent_effective, logical_shard_count as db_logical_shard_count,
    private_mesh_status as db_private_mesh_status, recommendations as db_recommendations,
    resolve_owner as db_resolve_owner, safe_time_diagnostics as db_safe_time_diagnostics,
    schema_committed_epoch as db_schema_epoch, shard_for_key as db_shard_for_key,
    shard_map_epoch as db_shard_map_epoch, snapshot_status, tiering_state as db_tiering_state,
    topology_status as db_topology_status,
};
use std::collections::BTreeMap;

fn require_operator<T, F>(identity: &CertIdentity, op: F) -> Result<T, DbError>
where
    F: FnOnce() -> Result<T, DbError>,
{
    intercept_rpc(identity, RpcClass::ClusterAdmin, op)
        .map_err(|err| DbError::invalid_argument(format!("unauthorized rpc: {}", err.reason)))?
}

fn require_operator_with_cert<T, F>(
    identity: &CertIdentity,
    pki: &PkiStore,
    cert_serial: u64,
    now_epoch_s: u64,
    op: F,
) -> Result<T, DbError>
where
    F: FnOnce() -> Result<T, DbError>,
{
    intercept_rpc_with_pki(
        pki,
        cert_serial,
        now_epoch_s,
        identity,
        RpcClass::ClusterAdmin,
        op,
    )
    .map_err(|err| DbError::invalid_argument(format!("unauthorized rpc: {}", err.reason)))?
}

pub fn safe_time_diagnostics(
    handle: i64,
    budgets: SafeTimeLagBudget,
    identity: &CertIdentity,
) -> Result<SafeTimeDiagnostics, DbError> {
    require_operator(identity, || db_safe_time_diagnostics(handle, budgets))
}

pub fn safe_time_diagnostics_with_cert(
    handle: i64,
    budgets: SafeTimeLagBudget,
    identity: &CertIdentity,
    pki: &PkiStore,
    cert_serial: u64,
    now_epoch_s: u64,
) -> Result<SafeTimeDiagnostics, DbError> {
    require_operator_with_cert(identity, pki, cert_serial, now_epoch_s, || {
        db_safe_time_diagnostics(handle, budgets)
    })
}

pub fn health_status(handle: i64, identity: &CertIdentity) -> Result<DbHealthStatus, DbError> {
    require_operator(identity, || db_health_status_impl(handle))
}

pub fn private_mesh_status(
    handle: i64,
    identity: &CertIdentity,
) -> Result<DbPrivateMeshStatus, DbError> {
    require_operator(identity, || db_private_mesh_status(handle))
}

pub fn health_status_with_cert(
    handle: i64,
    identity: &CertIdentity,
    pki: &PkiStore,
    cert_serial: u64,
    now_epoch_s: u64,
) -> Result<DbHealthStatus, DbError> {
    require_operator_with_cert(identity, pki, cert_serial, now_epoch_s, || {
        db_health_status_impl(handle)
    })
}

pub fn commit_visibility_status(
    handle: i64,
    identity: &CertIdentity,
) -> Result<DbCommitVisibilityStatus, DbError> {
    require_operator(identity, || db_commit_visibility_status_impl(handle))
}

pub fn commit_visibility_status_with_cert(
    handle: i64,
    identity: &CertIdentity,
    pki: &PkiStore,
    cert_serial: u64,
    now_epoch_s: u64,
) -> Result<DbCommitVisibilityStatus, DbError> {
    require_operator_with_cert(identity, pki, cert_serial, now_epoch_s, || {
        db_commit_visibility_status_impl(handle)
    })
}

pub fn client_write_path_aggregate(
    handle: i64,
    identity: &CertIdentity,
) -> Result<DbClientWritePathAggregate, DbError> {
    require_operator(identity, || db_client_write_path_aggregate_impl(handle))
}

pub fn client_write_path_aggregate_with_cert(
    handle: i64,
    identity: &CertIdentity,
    pki: &PkiStore,
    cert_serial: u64,
    now_epoch_s: u64,
) -> Result<DbClientWritePathAggregate, DbError> {
    require_operator_with_cert(identity, pki, cert_serial, now_epoch_s, || {
        db_client_write_path_aggregate_impl(handle)
    })
}

pub fn write_stage_aggregate(
    handle: i64,
    identity: &CertIdentity,
) -> Result<DbWriteStageAggregate, DbError> {
    require_operator(identity, || db_write_stage_aggregate_impl(handle))
}

pub fn write_stage_aggregate_with_cert(
    handle: i64,
    identity: &CertIdentity,
    pki: &PkiStore,
    cert_serial: u64,
    now_epoch_s: u64,
) -> Result<DbWriteStageAggregate, DbError> {
    require_operator_with_cert(identity, pki, cert_serial, now_epoch_s, || {
        db_write_stage_aggregate_impl(handle)
    })
}

pub fn wal_flush_stats(handle: i64, identity: &CertIdentity) -> Result<DbWalFlushStats, DbError> {
    require_operator(identity, || db_wal_flush_stats_impl(handle))
}

pub fn wal_flush_stats_with_cert(
    handle: i64,
    identity: &CertIdentity,
    pki: &PkiStore,
    cert_serial: u64,
    now_epoch_s: u64,
) -> Result<DbWalFlushStats, DbError> {
    require_operator_with_cert(identity, pki, cert_serial, now_epoch_s, || {
        db_wal_flush_stats_impl(handle)
    })
}

pub fn checkpoint_list(
    handle: i64,
    identity: &CertIdentity,
) -> Result<Vec<crate::db::checkpoint::CheckpointInfo>, DbError> {
    require_operator(identity, || db_checkpoint_list(handle))
}

pub fn checkpoint_list_with_cert(
    handle: i64,
    identity: &CertIdentity,
    pki: &PkiStore,
    cert_serial: u64,
    now_epoch_s: u64,
) -> Result<Vec<crate::db::checkpoint::CheckpointInfo>, DbError> {
    require_operator_with_cert(identity, pki, cert_serial, now_epoch_s, || {
        db_checkpoint_list(handle)
    })
}

pub fn schema_committed_epoch(handle: i64, identity: &CertIdentity) -> Result<u64, DbError> {
    require_operator(identity, || db_schema_epoch(handle))
}

pub fn schema_committed_epoch_with_cert(
    handle: i64,
    identity: &CertIdentity,
    pki: &PkiStore,
    cert_serial: u64,
    now_epoch_s: u64,
) -> Result<u64, DbError> {
    require_operator_with_cert(identity, pki, cert_serial, now_epoch_s, || {
        db_schema_epoch(handle)
    })
}

pub fn logical_shard_count(handle: i64, identity: &CertIdentity) -> Result<u32, DbError> {
    require_operator(identity, || db_logical_shard_count(handle))
}

pub fn logical_shard_count_with_cert(
    handle: i64,
    identity: &CertIdentity,
    pki: &PkiStore,
    cert_serial: u64,
    now_epoch_s: u64,
) -> Result<u32, DbError> {
    require_operator_with_cert(identity, pki, cert_serial, now_epoch_s, || {
        db_logical_shard_count(handle)
    })
}

pub fn active_group_count(handle: i64, identity: &CertIdentity) -> Result<u32, DbError> {
    require_operator(identity, || db_active_group_count(handle))
}

pub fn active_group_count_with_cert(
    handle: i64,
    identity: &CertIdentity,
    pki: &PkiStore,
    cert_serial: u64,
    now_epoch_s: u64,
) -> Result<u32, DbError> {
    require_operator_with_cert(identity, pki, cert_serial, now_epoch_s, || {
        db_active_group_count(handle)
    })
}

pub fn topology_status(handle: i64, identity: &CertIdentity) -> Result<DbTopologyStatus, DbError> {
    require_operator(identity, || db_topology_status(handle))
}

pub fn topology_status_with_cert(
    handle: i64,
    identity: &CertIdentity,
    pki: &PkiStore,
    cert_serial: u64,
    now_epoch_s: u64,
) -> Result<DbTopologyStatus, DbError> {
    require_operator_with_cert(identity, pki, cert_serial, now_epoch_s, || {
        db_topology_status(handle)
    })
}

pub fn autoscale_status(
    handle: i64,
    identity: &CertIdentity,
) -> Result<DbAutoscaleStatus, DbError> {
    require_operator(identity, || db_autoscale_status(handle))
}

pub fn autoscale_status_with_cert(
    handle: i64,
    identity: &CertIdentity,
    pki: &PkiStore,
    cert_serial: u64,
    now_epoch_s: u64,
) -> Result<DbAutoscaleStatus, DbError> {
    require_operator_with_cert(identity, pki, cert_serial, now_epoch_s, || {
        db_autoscale_status(handle)
    })
}

pub fn intent_effective(
    handle: i64,
    identity: &CertIdentity,
) -> Result<DbIntentEffective, DbError> {
    require_operator(identity, || db_intent_effective(handle))
}

pub fn intent_effective_with_cert(
    handle: i64,
    identity: &CertIdentity,
    pki: &PkiStore,
    cert_serial: u64,
    now_epoch_s: u64,
) -> Result<DbIntentEffective, DbError> {
    require_operator_with_cert(identity, pki, cert_serial, now_epoch_s, || {
        db_intent_effective(handle)
    })
}

pub fn intent_conflicts(
    handle: i64,
    identity: &CertIdentity,
) -> Result<Vec<DbIntentConflict>, DbError> {
    require_operator(identity, || db_intent_conflicts(handle))
}

pub fn intent_conflicts_with_cert(
    handle: i64,
    identity: &CertIdentity,
    pki: &PkiStore,
    cert_serial: u64,
    now_epoch_s: u64,
) -> Result<Vec<DbIntentConflict>, DbError> {
    require_operator_with_cert(identity, pki, cert_serial, now_epoch_s, || {
        db_intent_conflicts(handle)
    })
}

pub fn autopilot_last_actions(
    handle: i64,
    limit: usize,
    identity: &CertIdentity,
) -> Result<Vec<DbAutopilotAuditRow>, DbError> {
    require_operator(identity, || db_autopilot_last_actions(handle, limit))
}

pub fn autopilot_last_actions_with_cert(
    handle: i64,
    limit: usize,
    identity: &CertIdentity,
    pki: &PkiStore,
    cert_serial: u64,
    now_epoch_s: u64,
) -> Result<Vec<DbAutopilotAuditRow>, DbError> {
    require_operator_with_cert(identity, pki, cert_serial, now_epoch_s, || {
        db_autopilot_last_actions(handle, limit)
    })
}

pub fn tiering_state(handle: i64, identity: &CertIdentity) -> Result<DbTieringState, DbError> {
    require_operator(identity, || db_tiering_state(handle))
}

pub fn tiering_state_with_cert(
    handle: i64,
    identity: &CertIdentity,
    pki: &PkiStore,
    cert_serial: u64,
    now_epoch_s: u64,
) -> Result<DbTieringState, DbError> {
    require_operator_with_cert(identity, pki, cert_serial, now_epoch_s, || {
        db_tiering_state(handle)
    })
}

pub fn recommendations(
    handle: i64,
    identity: &CertIdentity,
) -> Result<Vec<DbRecommendation>, DbError> {
    require_operator(identity, || db_recommendations(handle))
}

pub fn recommendations_with_cert(
    handle: i64,
    identity: &CertIdentity,
    pki: &PkiStore,
    cert_serial: u64,
    now_epoch_s: u64,
) -> Result<Vec<DbRecommendation>, DbError> {
    require_operator_with_cert(identity, pki, cert_serial, now_epoch_s, || {
        db_recommendations(handle)
    })
}

pub fn shard_map_epoch(handle: i64, identity: &CertIdentity) -> Result<u64, DbError> {
    require_operator(identity, || db_shard_map_epoch(handle))
}

pub fn shard_map_epoch_with_cert(
    handle: i64,
    identity: &CertIdentity,
    pki: &PkiStore,
    cert_serial: u64,
    now_epoch_s: u64,
) -> Result<u64, DbError> {
    require_operator_with_cert(identity, pki, cert_serial, now_epoch_s, || {
        db_shard_map_epoch(handle)
    })
}

pub fn shard_for_key(
    handle: i64,
    namespace: Vec<u8>,
    key: Vec<u8>,
    identity: &CertIdentity,
) -> Result<u32, DbError> {
    require_operator(identity, || db_shard_for_key(handle, namespace, key))
}

pub fn shard_for_key_with_cert(
    handle: i64,
    namespace: Vec<u8>,
    key: Vec<u8>,
    identity: &CertIdentity,
    pki: &PkiStore,
    cert_serial: u64,
    now_epoch_s: u64,
) -> Result<u32, DbError> {
    require_operator_with_cert(identity, pki, cert_serial, now_epoch_s, || {
        db_shard_for_key(handle, namespace, key)
    })
}

pub fn resolve_owner(
    handle: i64,
    namespace: Vec<u8>,
    key: Vec<u8>,
    identity: &CertIdentity,
) -> Result<OwnerRecord, DbError> {
    require_operator(identity, || db_resolve_owner(handle, namespace, key))
}

pub fn resolve_owner_with_cert(
    handle: i64,
    namespace: Vec<u8>,
    key: Vec<u8>,
    identity: &CertIdentity,
    pki: &PkiStore,
    cert_serial: u64,
    now_epoch_s: u64,
) -> Result<OwnerRecord, DbError> {
    require_operator_with_cert(identity, pki, cert_serial, now_epoch_s, || {
        db_resolve_owner(handle, namespace, key)
    })
}

pub fn global_route_lookup(
    handle: i64,
    namespace: Vec<u8>,
    key: Vec<u8>,
    identity: &CertIdentity,
) -> Result<OwnerRecord, DbError> {
    require_operator(identity, || db_global_route_lookup(handle, namespace, key))
}

pub fn global_route_lookup_with_cert(
    handle: i64,
    namespace: Vec<u8>,
    key: Vec<u8>,
    identity: &CertIdentity,
    pki: &PkiStore,
    cert_serial: u64,
    now_epoch_s: u64,
) -> Result<OwnerRecord, DbError> {
    require_operator_with_cert(identity, pki, cert_serial, now_epoch_s, || {
        db_global_route_lookup(handle, namespace, key)
    })
}

pub fn get_snapshot_status(
    handle: i64,
    snapshot: u64,
    identity: &CertIdentity,
) -> Result<u8, DbError> {
    require_operator(identity, || snapshot_status(handle, snapshot))
}

pub fn get_snapshot_status_with_cert(
    handle: i64,
    snapshot: u64,
    identity: &CertIdentity,
    pki: &PkiStore,
    cert_serial: u64,
    now_epoch_s: u64,
) -> Result<u8, DbError> {
    require_operator_with_cert(identity, pki, cert_serial, now_epoch_s, || {
        snapshot_status(handle, snapshot)
    })
}

pub fn cluster_health_snapshot(
    nodes: &[NodeHealth],
    identity: &CertIdentity,
) -> Result<ClusterHealthSnapshot, DbError> {
    require_operator(identity, || Ok(cluster_health_snapshot_impl(nodes)))
}

pub fn cluster_health_snapshot_with_cert(
    nodes: &[NodeHealth],
    identity: &CertIdentity,
    pki: &PkiStore,
    cert_serial: u64,
    now_epoch_s: u64,
) -> Result<ClusterHealthSnapshot, DbError> {
    require_operator_with_cert(identity, pki, cert_serial, now_epoch_s, || {
        Ok(cluster_health_snapshot_impl(nodes))
    })
}

pub fn quorum_explain_summary(
    nodes: &[NodeHealth],
    desired_voters: usize,
    latency_hint_ms: &BTreeMap<String, u64>,
    identity: &CertIdentity,
) -> Result<QuorumExplainSummary, DbError> {
    require_operator(identity, || {
        quorum_explain_summary_impl(nodes, desired_voters, latency_hint_ms)
            .map_err(|err| DbError::invalid_argument(format!("quorum explain failed: {err:?}")))
    })
}

pub fn quorum_explain_summary_with_cert(
    nodes: &[NodeHealth],
    desired_voters: usize,
    latency_hint_ms: &BTreeMap<String, u64>,
    identity: &CertIdentity,
    pki: &PkiStore,
    cert_serial: u64,
    now_epoch_s: u64,
) -> Result<QuorumExplainSummary, DbError> {
    require_operator_with_cert(identity, pki, cert_serial, now_epoch_s, || {
        quorum_explain_summary_impl(nodes, desired_voters, latency_hint_ms)
            .map_err(|err| DbError::invalid_argument(format!("quorum explain failed: {err:?}")))
    })
}

pub fn policy_explain_summary(
    spec: &RoutingPolicySpec,
    identity: &CertIdentity,
) -> Result<PolicyExplainSummary, DbError> {
    require_operator(identity, || {
        policy_explain_summary_impl(spec)
            .map_err(|err| DbError::invalid_argument(format!("policy explain failed: {err:?}")))
    })
}

pub fn policy_explain_summary_with_cert(
    spec: &RoutingPolicySpec,
    identity: &CertIdentity,
    pki: &PkiStore,
    cert_serial: u64,
    now_epoch_s: u64,
) -> Result<PolicyExplainSummary, DbError> {
    require_operator_with_cert(identity, pki, cert_serial, now_epoch_s, || {
        policy_explain_summary_impl(spec)
            .map_err(|err| DbError::invalid_argument(format!("policy explain failed: {err:?}")))
    })
}

pub fn residency_audit(
    shard: &[u8],
    sink_region: &str,
    policy: &ResidencyPolicy,
    identity: &CertIdentity,
) -> Result<ResidencyAuditResult, DbError> {
    require_operator(identity, || {
        Ok(residency_audit_impl(shard, sink_region, policy))
    })
}

pub fn residency_audit_with_cert(
    shard: &[u8],
    sink_region: &str,
    policy: &ResidencyPolicy,
    identity: &CertIdentity,
    pki: &PkiStore,
    cert_serial: u64,
    now_epoch_s: u64,
) -> Result<ResidencyAuditResult, DbError> {
    require_operator_with_cert(identity, pki, cert_serial, now_epoch_s, || {
        Ok(residency_audit_impl(shard, sink_region, policy))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::api::core::{close, open};
    use crate::db::routing::health::{HealthState, MemberRole};
    use crate::db::security::authz::MembershipRole;

    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn temp_dir() -> PathBuf {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        let base = std::env::temp_dir().join(format!(
            "wrela_db_api_explain_test_{}_{}_{}",
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

    fn id(role: MembershipRole) -> CertIdentity {
        CertIdentity {
            cluster_id: "cluster-a".to_string(),
            node_id: "node-1".to_string(),
            role,
        }
    }

    #[test]
    fn explain_surface_requires_operator_identity() {
        let gateway = id(MembershipRole::Gateway);
        let err = topology_status(-1, &gateway).expect_err("gateway must be denied");
        assert!(err.message.contains("unauthorized rpc"));
    }

    #[test]
    fn explain_surface_allows_cluster_admin() {
        let dir = temp_dir();
        let handle = open(&dir).expect("open");
        let admin = id(MembershipRole::Admin);
        let status = topology_status(handle, &admin).expect("admin can read topology");
        assert!(status.logical_shards >= 1);
        assert!(close(handle));
    }

    #[test]
    fn explain_helper_wrappers_require_operator_identity() {
        let nodes = vec![NodeHealth {
            node_id: "n1".to_string(),
            region: "us".to_string(),
            state: HealthState::Healthy,
            observed_at_ms: 1,
            role: MemberRole::Unknown,
        }];
        let gateway = id(MembershipRole::Gateway);
        let err = cluster_health_snapshot(&nodes, &gateway).expect_err("gateway denied");
        assert!(err.message.contains("unauthorized rpc"));
    }

    #[test]
    fn autopilot_explain_surfaces_require_operator_identity() {
        let gateway = id(MembershipRole::Gateway);
        let err = intent_effective(-1, &gateway).expect_err("gateway denied");
        assert!(err.message.contains("unauthorized rpc"));
    }

    #[test]
    fn autopilot_explain_surfaces_return_typed_results_for_cluster_admin() {
        let dir = temp_dir();
        let handle = open(&dir).expect("open");
        let admin = id(MembershipRole::Admin);

        let effective = intent_effective(handle, &admin).expect("intent effective");
        assert_eq!(effective.mode, "full_auto");
        assert!(effective.replication_factor >= 1);

        let _conflicts = intent_conflicts(handle, &admin).expect("intent conflicts");
        let actions = autopilot_last_actions(handle, 16, &admin).expect("last actions");
        assert!(!actions.is_empty(), "boot tick should produce an action");

        let tiering = tiering_state(handle, &admin).expect("tiering");
        assert!(tiering.boundary_max_live_bytes >= tiering.boundary_min_live_bytes);

        let _recs = recommendations(handle, &admin).expect("recommendations");
        assert!(close(handle));
    }

    #[test]
    fn ownership_explain_surface_requires_operator_identity() {
        let gateway = id(MembershipRole::Gateway);

        let owner_err = resolve_owner(-1, b"core".to_vec(), b"k".to_vec(), &gateway)
            .expect_err("gateway must be denied owner resolve");
        assert!(owner_err.message.contains("unauthorized rpc"));

        let route_err = global_route_lookup(-1, b"core".to_vec(), b"k".to_vec(), &gateway)
            .expect_err("gateway must be denied route lookup");
        assert!(route_err.message.contains("unauthorized rpc"));
    }

    #[test]
    fn ownership_explain_surface_returns_owner_for_cluster_admin() {
        let dir = temp_dir();
        let handle = open(&dir).expect("open");
        let admin = id(MembershipRole::Admin);
        let owner = resolve_owner(handle, b"core".to_vec(), b"k-owner".to_vec(), &admin)
            .expect("admin can resolve ownership");
        assert!(!owner.keyrange_id.is_empty());
        assert!(!owner.leader_node_id.is_empty());
        assert!(!owner.ownership_token.is_empty());

        let route = global_route_lookup(handle, b"core".to_vec(), b"k-owner".to_vec(), &admin)
            .expect("admin can inspect routing ownership");
        assert_eq!(route.keyrange_id, owner.keyrange_id);
        assert_eq!(route.ownership_token, owner.ownership_token);
        assert!(close(handle));
    }
}
