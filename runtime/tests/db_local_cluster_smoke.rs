mod support;

use serde_json::Value;
use std::time::Duration;

use support::local_cluster::LocalCluster;

#[test]
#[ignore] // requires apps/wreladb-lab (deleted); run with --ignored when app restored
fn db_local_cluster_smoke_converges_write_read_and_mesh() {
    let mut cluster = LocalCluster::boot_default().expect("boot local cluster");
    cluster
        .wait_for_all_healthy(Duration::from_secs(45))
        .expect("all nodes healthy");
    for node_id in cluster.node_ids() {
        let live = cluster.probe_live(&node_id).expect("probe live");
        assert_eq!(live.status, 200, "live status must be 200 for {node_id}");
        assert!(
            live.body
                .get("ok")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            "live payload must report ok=true for {node_id}: {}",
            live.raw_body
        );
    }
    let snapshots = cluster
        .wait_for_mesh_ready(Duration::from_secs(45))
        .expect("mesh ready");
    assert_eq!(snapshots.len(), 3, "expected three mesh snapshots");

    for node_id in cluster.node_ids() {
        let write = cluster.probe_write(&node_id).expect("probe write");
        let (expected_value, committed_version) = cluster
            .validate_successful_write(&write)
            .expect("write must be strict success");
        assert!(
            committed_version > 0,
            "commit version must be positive for {}: {}",
            node_id,
            write.raw_body
        );

        // Fail closed on write/read mismatch for the exact writer node before checking cluster-wide convergence.
        cluster
            .wait_for_node_value(&node_id, &expected_value, false, Duration::from_secs(10))
            .expect("writer read must see committed value");
    }
}

#[test]
#[ignore] // requires apps/wreladb-lab (deleted)
fn db_local_cluster_smoke_detects_unavailable_node() {
    let mut cluster = LocalCluster::boot_default().expect("boot local cluster");
    cluster
        .wait_for_all_healthy(Duration::from_secs(45))
        .expect("all nodes healthy");
    cluster
        .stop_node("node-c")
        .expect("stop node-c for negative health scenario");
    let err = cluster
        .wait_for_all_healthy(Duration::from_secs(5))
        .expect_err("health gate must fail with missing node");
    assert!(
        err.contains("node-c") || err.contains("exited unexpectedly"),
        "unexpected health error: {err}"
    );
}

#[test]
#[ignore] // requires apps/wreladb-lab (deleted)
fn db_local_cluster_smoke_delayed_rejoin_recovers_and_converges() {
    let mut cluster = LocalCluster::boot_default().expect("boot local cluster");
    cluster
        .wait_for_all_healthy(Duration::from_secs(45))
        .expect("all nodes healthy");
    cluster
        .wait_for_mesh_ready(Duration::from_secs(45))
        .expect("mesh ready");

    cluster
        .stop_node("node-c")
        .expect("stop node-c before delayed rejoin");
    let health_err = cluster
        .wait_for_all_healthy(Duration::from_secs(5))
        .expect_err("health gate must fail while node-c is down");
    assert!(
        health_err.contains("node-c") || health_err.contains("exited unexpectedly"),
        "unexpected degraded health message: {health_err}"
    );

    let write_while_degraded = cluster
        .probe_write("node-a")
        .expect("leader write should still respond while follower is down");
    let (degraded_value, _) = cluster
        .validate_successful_write(&write_while_degraded)
        .expect("degraded write must still be strict success");
    cluster
        .wait_for_node_value("node-a", &degraded_value, false, Duration::from_secs(10))
        .expect("leader must read its own write while degraded");

    cluster
        .restart_node("node-c")
        .expect("restart delayed node-c");
    cluster
        .wait_for_all_healthy(Duration::from_secs(45))
        .expect("all nodes healthy after delayed rejoin");
    cluster
        .wait_for_mesh_ready(Duration::from_secs(45))
        .expect("mesh ready after delayed rejoin");

    let write_after_rejoin = cluster
        .probe_write("node-c")
        .expect("rejoined node write should respond");
    let (post_rejoin_value, version) = cluster
        .validate_successful_write(&write_after_rejoin)
        .expect("post-rejoin write must be strict success");
    assert!(version > 0, "post-rejoin write version must be positive");
    cluster
        .wait_for_cluster_value(&post_rejoin_value, Duration::from_secs(20))
        .expect("cluster pooled reads should converge after delayed rejoin");
}

#[test]
#[ignore] // requires apps/wreladb-lab (deleted)
fn db_local_cluster_smoke_follower_write_fails_closed_when_leader_unavailable() {
    let mut cluster = LocalCluster::boot_default().expect("boot local cluster");
    cluster
        .wait_for_all_healthy(Duration::from_secs(45))
        .expect("all nodes healthy");
    cluster
        .wait_for_mesh_ready(Duration::from_secs(45))
        .expect("mesh ready");

    let leader = cluster.current_leader_id().expect("current leader");
    let follower = cluster
        .node_ids()
        .into_iter()
        .find(|node_id| node_id != &leader)
        .expect("follower id");
    cluster.stop_node(&leader).expect("stop leader");

    let degraded_write = cluster
        .probe_write(&follower)
        .expect("follower write should return fail-closed response");
    assert!(
        degraded_write.status >= 500,
        "degraded write must fail closed status={} body={}",
        degraded_write.status,
        degraded_write.raw_body
    );
    assert!(
        !degraded_write
            .body
            .get("ok")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        "degraded write must never report ok=true: {}",
        degraded_write.raw_body
    );
    let err_message = degraded_write
        .body
        .get("error")
        .and_then(Value::as_str)
        .unwrap_or_default();
    assert!(
        err_message.contains("RETRY_AFTER_MS")
            || err_message.contains("private rpc")
            || err_message.contains("leader"),
        "degraded write error should expose retryable forwarding failure, got: {}",
        degraded_write.raw_body
    );

    cluster
        .restart_node(&leader)
        .expect("restart leader after fail-closed check");
    cluster
        .wait_for_all_healthy(Duration::from_secs(45))
        .expect("all nodes healthy after leader restart");
    cluster
        .wait_for_mesh_ready(Duration::from_secs(45))
        .expect("mesh ready after leader restart");

    let recovery_write = cluster
        .probe_write(&follower)
        .expect("follower write should recover once leader is back");
    let (value, _) = cluster
        .validate_successful_write(&recovery_write)
        .expect("recovered follower write must be strict success");
    cluster
        .wait_for_cluster_value(&value, Duration::from_secs(20))
        .expect("cluster must converge after leader recovery");
}
