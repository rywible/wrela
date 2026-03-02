use crate::db::net::interceptor::{intercept_rpc, intercept_rpc_with_pki};
use crate::db::raft::membership::MembershipChange;
use crate::db::security::authz::{CertIdentity, RpcClass};
use crate::db::security::pki::PkiStore;
use crate::db::{
    DbAutoscaleStatus, DbError, OwnerRecord, advance_home_relocation as db_advance_home_relocation,
    autoscale_tick as db_autoscale_tick, checkpoint_create as db_checkpoint_create,
    checkpoint_prune as db_checkpoint_prune,
    checkpoint_restore_latest as db_checkpoint_restore_latest, membership_abort_joint_change,
    membership_begin_joint_change, membership_commit_joint_change, membership_set_voters,
    merge_logical_shards as db_merge_logical_shards,
    plan_home_relocation as db_plan_home_relocation,
    promote_async_failover as db_promote_async_failover, restore_snapshot,
    schema_set_all_voters_on_target_binary as db_schema_set_binary_ready,
    schema_set_committed_epoch as db_schema_set_epoch, snapshot_start,
    split_logical_shard as db_split_logical_shard,
};

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

pub fn checkpoint_create(
    handle: i64,
    identity: &CertIdentity,
) -> Result<crate::db::checkpoint::CheckpointInfo, DbError> {
    require_operator(identity, || db_checkpoint_create(handle))
}

pub fn checkpoint_create_with_cert(
    handle: i64,
    identity: &CertIdentity,
    pki: &PkiStore,
    cert_serial: u64,
    now_epoch_s: u64,
) -> Result<crate::db::checkpoint::CheckpointInfo, DbError> {
    require_operator_with_cert(identity, pki, cert_serial, now_epoch_s, || {
        db_checkpoint_create(handle)
    })
}

pub fn checkpoint_restore_latest(
    handle: i64,
    identity: &CertIdentity,
) -> Result<crate::db::checkpoint::CheckpointInfo, DbError> {
    require_operator(identity, || db_checkpoint_restore_latest(handle))
}

pub fn checkpoint_restore_latest_with_cert(
    handle: i64,
    identity: &CertIdentity,
    pki: &PkiStore,
    cert_serial: u64,
    now_epoch_s: u64,
) -> Result<crate::db::checkpoint::CheckpointInfo, DbError> {
    require_operator_with_cert(identity, pki, cert_serial, now_epoch_s, || {
        db_checkpoint_restore_latest(handle)
    })
}

pub fn checkpoint_prune(
    handle: i64,
    retain: usize,
    identity: &CertIdentity,
) -> Result<(), DbError> {
    require_operator(identity, || db_checkpoint_prune(handle, retain))
}

pub fn checkpoint_prune_with_cert(
    handle: i64,
    retain: usize,
    identity: &CertIdentity,
    pki: &PkiStore,
    cert_serial: u64,
    now_epoch_s: u64,
) -> Result<(), DbError> {
    require_operator_with_cert(identity, pki, cert_serial, now_epoch_s, || {
        db_checkpoint_prune(handle, retain)
    })
}

pub fn schema_set_committed_epoch(
    handle: i64,
    epoch: u64,
    identity: &CertIdentity,
) -> Result<(), DbError> {
    require_operator(identity, || db_schema_set_epoch(handle, epoch))
}

pub fn schema_set_committed_epoch_with_cert(
    handle: i64,
    epoch: u64,
    identity: &CertIdentity,
    pki: &PkiStore,
    cert_serial: u64,
    now_epoch_s: u64,
) -> Result<(), DbError> {
    require_operator_with_cert(identity, pki, cert_serial, now_epoch_s, || {
        db_schema_set_epoch(handle, epoch)
    })
}

pub fn schema_set_all_voters_on_target_binary(
    handle: i64,
    ready: bool,
    identity: &CertIdentity,
) -> Result<(), DbError> {
    require_operator(identity, || db_schema_set_binary_ready(handle, ready))
}

pub fn schema_set_all_voters_on_target_binary_with_cert(
    handle: i64,
    ready: bool,
    identity: &CertIdentity,
    pki: &PkiStore,
    cert_serial: u64,
    now_epoch_s: u64,
) -> Result<(), DbError> {
    require_operator_with_cert(identity, pki, cert_serial, now_epoch_s, || {
        db_schema_set_binary_ready(handle, ready)
    })
}

pub fn autoscale_tick(handle: i64, identity: &CertIdentity) -> Result<DbAutoscaleStatus, DbError> {
    require_operator(identity, || db_autoscale_tick(handle))
}

pub fn autoscale_tick_with_cert(
    handle: i64,
    identity: &CertIdentity,
    pki: &PkiStore,
    cert_serial: u64,
    now_epoch_s: u64,
) -> Result<DbAutoscaleStatus, DbError> {
    require_operator_with_cert(identity, pki, cert_serial, now_epoch_s, || {
        db_autoscale_tick(handle)
    })
}

pub fn plan_rehome(
    handle: i64,
    keyrange_id: String,
    target_region: String,
    reason: String,
    identity: &CertIdentity,
) -> Result<crate::db::placement::RelocationJob, DbError> {
    require_operator(identity, || {
        db_plan_home_relocation(handle, keyrange_id, target_region, reason)
    })
}

pub fn plan_rehome_with_cert(
    handle: i64,
    keyrange_id: String,
    target_region: String,
    reason: String,
    identity: &CertIdentity,
    pki: &PkiStore,
    cert_serial: u64,
    now_epoch_s: u64,
) -> Result<crate::db::placement::RelocationJob, DbError> {
    require_operator_with_cert(identity, pki, cert_serial, now_epoch_s, || {
        db_plan_home_relocation(handle, keyrange_id, target_region, reason)
    })
}

pub fn advance_rehome(
    handle: i64,
    job_id: String,
    phase_ack: Option<crate::db::placement::RelocationPhase>,
    identity: &CertIdentity,
) -> Result<crate::db::placement::RelocationJob, DbError> {
    require_operator(identity, || {
        db_advance_home_relocation(handle, job_id, phase_ack)
    })
}

pub fn advance_rehome_with_cert(
    handle: i64,
    job_id: String,
    phase_ack: Option<crate::db::placement::RelocationPhase>,
    identity: &CertIdentity,
    pki: &PkiStore,
    cert_serial: u64,
    now_epoch_s: u64,
) -> Result<crate::db::placement::RelocationJob, DbError> {
    require_operator_with_cert(identity, pki, cert_serial, now_epoch_s, || {
        db_advance_home_relocation(handle, job_id, phase_ack)
    })
}

pub fn promote_async_failover(
    handle: i64,
    keyrange_id: String,
    region: String,
    expected_epoch: u64,
    identity: &CertIdentity,
) -> Result<OwnerRecord, DbError> {
    require_operator(identity, || {
        db_promote_async_failover(handle, keyrange_id, region, expected_epoch)
    })
}

pub fn promote_async_failover_with_cert(
    handle: i64,
    keyrange_id: String,
    region: String,
    expected_epoch: u64,
    identity: &CertIdentity,
    pki: &PkiStore,
    cert_serial: u64,
    now_epoch_s: u64,
) -> Result<OwnerRecord, DbError> {
    require_operator_with_cert(identity, pki, cert_serial, now_epoch_s, || {
        db_promote_async_failover(handle, keyrange_id, region, expected_epoch)
    })
}

pub fn split_logical_shard(
    handle: i64,
    shard_id: u32,
    identity: &CertIdentity,
) -> Result<(u32, u32), DbError> {
    require_operator(identity, || db_split_logical_shard(handle, shard_id))
}

pub fn split_logical_shard_with_cert(
    handle: i64,
    shard_id: u32,
    identity: &CertIdentity,
    pki: &PkiStore,
    cert_serial: u64,
    now_epoch_s: u64,
) -> Result<(u32, u32), DbError> {
    require_operator_with_cert(identity, pki, cert_serial, now_epoch_s, || {
        db_split_logical_shard(handle, shard_id)
    })
}

pub fn merge_logical_shards(
    handle: i64,
    left_shard_id: u32,
    right_shard_id: u32,
    identity: &CertIdentity,
) -> Result<u32, DbError> {
    require_operator(identity, || {
        db_merge_logical_shards(handle, left_shard_id, right_shard_id)
    })
}

pub fn merge_logical_shards_with_cert(
    handle: i64,
    left_shard_id: u32,
    right_shard_id: u32,
    identity: &CertIdentity,
    pki: &PkiStore,
    cert_serial: u64,
    now_epoch_s: u64,
) -> Result<u32, DbError> {
    require_operator_with_cert(identity, pki, cert_serial, now_epoch_s, || {
        db_merge_logical_shards(handle, left_shard_id, right_shard_id)
    })
}

pub fn configure_membership_voters(
    handle: i64,
    voters: Vec<u64>,
    identity: &CertIdentity,
) -> Result<(), DbError> {
    require_operator(identity, || membership_set_voters(handle, voters))
}

pub fn configure_membership_voters_with_cert(
    handle: i64,
    voters: Vec<u64>,
    identity: &CertIdentity,
    pki: &PkiStore,
    cert_serial: u64,
    now_epoch_s: u64,
) -> Result<(), DbError> {
    require_operator_with_cert(identity, pki, cert_serial, now_epoch_s, || {
        membership_set_voters(handle, voters)
    })
}

pub fn begin_joint_membership_change(
    handle: i64,
    change: MembershipChange,
    log_index: u64,
    identity: &CertIdentity,
) -> Result<(), DbError> {
    require_operator(identity, || {
        membership_begin_joint_change(handle, change, log_index)
    })
}

pub fn begin_joint_membership_change_with_cert(
    handle: i64,
    change: MembershipChange,
    log_index: u64,
    identity: &CertIdentity,
    pki: &PkiStore,
    cert_serial: u64,
    now_epoch_s: u64,
) -> Result<(), DbError> {
    require_operator_with_cert(identity, pki, cert_serial, now_epoch_s, || {
        membership_begin_joint_change(handle, change, log_index)
    })
}

pub fn commit_joint_membership_change(handle: i64, identity: &CertIdentity) -> Result<(), DbError> {
    require_operator(identity, || membership_commit_joint_change(handle))
}

pub fn commit_joint_membership_change_with_cert(
    handle: i64,
    identity: &CertIdentity,
    pki: &PkiStore,
    cert_serial: u64,
    now_epoch_s: u64,
) -> Result<(), DbError> {
    require_operator_with_cert(identity, pki, cert_serial, now_epoch_s, || {
        membership_commit_joint_change(handle)
    })
}

pub fn abort_joint_membership_change(handle: i64, identity: &CertIdentity) -> Result<(), DbError> {
    require_operator(identity, || membership_abort_joint_change(handle))
}

pub fn abort_joint_membership_change_with_cert(
    handle: i64,
    identity: &CertIdentity,
    pki: &PkiStore,
    cert_serial: u64,
    now_epoch_s: u64,
) -> Result<(), DbError> {
    require_operator_with_cert(identity, pki, cert_serial, now_epoch_s, || {
        membership_abort_joint_change(handle)
    })
}

pub fn start_snapshot(handle: i64, identity: &CertIdentity) -> Result<u64, DbError> {
    require_operator(identity, || snapshot_start(handle))
}

pub fn start_snapshot_with_cert(
    handle: i64,
    identity: &CertIdentity,
    pki: &PkiStore,
    cert_serial: u64,
    now_epoch_s: u64,
) -> Result<u64, DbError> {
    require_operator_with_cert(identity, pki, cert_serial, now_epoch_s, || {
        snapshot_start(handle)
    })
}

pub fn restore(handle: i64, snapshot: u64, identity: &CertIdentity) -> Result<(), DbError> {
    require_operator(identity, || restore_snapshot(handle, snapshot))
}

pub fn restore_with_cert(
    handle: i64,
    snapshot: u64,
    identity: &CertIdentity,
    pki: &PkiStore,
    cert_serial: u64,
    now_epoch_s: u64,
) -> Result<(), DbError> {
    require_operator_with_cert(identity, pki, cert_serial, now_epoch_s, || {
        restore_snapshot(handle, snapshot)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::api::core::{close, open, put};
    use crate::db::security::authz::MembershipRole;
    use crate::db::security::pki::PkiStore;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn temp_dir() -> PathBuf {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        let base = std::env::temp_dir().join(format!(
            "wrela_db_api_admin_test_{}_{}_{}",
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
    fn membership_mutations_require_cluster_admin_role() {
        let dir = temp_dir();
        let handle = open(&dir).expect("open");
        let gateway = id(MembershipRole::Gateway);
        let admin = id(MembershipRole::Admin);

        let deny =
            configure_membership_voters(handle, vec![1, 2, 3], &gateway).expect_err("must deny");
        assert!(deny.message.contains("unauthorized rpc"));

        configure_membership_voters(handle, vec![1, 2, 3], &admin).expect("admin ok");
        begin_joint_membership_change(
            handle,
            MembershipChange::AddVoter { node_id: 4 },
            12,
            &admin,
        )
        .expect("admin can start joint change");
        commit_joint_membership_change(handle, &admin).expect("admin can commit joint change");
        put(handle, b"core".to_vec(), b"k".to_vec(), b"v".to_vec(), None).expect("write");
        assert!(close(handle));
    }

    #[test]
    fn membership_mutations_support_valid_cluster_admin_cert() {
        let dir = temp_dir();
        let handle = open(&dir).expect("open");
        let admin = id(MembershipRole::Admin);
        let mut pki = PkiStore::default();
        let cert = pki.issue_cert("cluster-a".to_string(), "node-1".to_string(), 100, 60);

        configure_membership_voters_with_cert(
            handle,
            vec![1, 2, 3],
            &admin,
            &pki,
            cert.serial,
            120,
        )
        .expect("cert admin membership change");
        assert!(close(handle));
    }

    #[test]
    fn split_and_merge_require_cluster_admin_role() {
        let gateway = id(MembershipRole::Gateway);

        let split_deny =
            split_logical_shard(-1, 0, &gateway).expect_err("gateway must be denied split");
        assert!(split_deny.message.contains("unauthorized rpc"));

        let merge_deny =
            merge_logical_shards(-1, 0, 1, &gateway).expect_err("gateway must be denied merge");
        assert!(merge_deny.message.contains("unauthorized rpc"));
    }

    #[test]
    fn ownership_mutations_require_cluster_admin_role() {
        let gateway = id(MembershipRole::Gateway);

        let plan_deny = plan_rehome(
            -1,
            "core".to_string(),
            "eu".to_string(),
            "operator-cutover".to_string(),
            &gateway,
        )
        .expect_err("gateway must be denied rehome");
        assert!(plan_deny.message.contains("unauthorized rpc"));

        let failover_deny =
            promote_async_failover(-1, "core".to_string(), "eu".to_string(), 0, &gateway)
                .expect_err("gateway must be denied failover promote");
        assert!(failover_deny.message.contains("unauthorized rpc"));
    }

    #[test]
    fn ownership_mutations_with_cert_require_cluster_admin_role() {
        let gateway = id(MembershipRole::Gateway);
        let mut pki = PkiStore::default();
        let cert = pki.issue_cert("cluster-a".to_string(), "node-1".to_string(), 100, 60);

        let plan_deny = plan_rehome_with_cert(
            -1,
            "core".to_string(),
            "eu".to_string(),
            "operator-cutover".to_string(),
            &gateway,
            &pki,
            cert.serial,
            120,
        )
        .expect_err("gateway cert must still be denied rehome");
        assert!(plan_deny.message.contains("unauthorized rpc"));
    }

    #[test]
    fn admin_surface_fails_closed_before_db_lookup() {
        let malformed = CertIdentity {
            cluster_id: "".to_string(),
            node_id: "node-1".to_string(),
            role: MembershipRole::Admin,
        };
        let err = autoscale_tick(-1, &malformed).expect_err("must deny before db lookup");
        assert!(err.message.contains("unauthorized rpc"));
    }
}
