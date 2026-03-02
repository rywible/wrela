mod support;

use serde_json::Value;
use std::collections::BTreeMap;
use std::time::Duration;

use support::local_cluster::{LocalCluster, LocalClusterConfig};

fn quorum_config() -> LocalClusterConfig {
    let mut extra_env = BTreeMap::new();
    extra_env.insert("WRELADB_REPLICATION_FACTOR".to_string(), "3".to_string());
    extra_env.insert("WRELADB_WRITE_QUORUM".to_string(), "2".to_string());
    LocalClusterConfig {
        extra_env,
        ..Default::default()
    }
}

#[test]
#[ignore] // requires apps/wreladb-lab (deleted)
fn quorum_write_requires_majority_ack() {
    let mut cluster = LocalCluster::boot_with_config(quorum_config()).expect("boot quorum cluster");
    cluster
        .wait_for_all_healthy(Duration::from_secs(60))
        .expect("all nodes healthy");
    cluster
        .wait_for_mesh_ready(Duration::from_secs(60))
        .expect("mesh ready");

    for node_id in cluster.node_ids() {
        let write = cluster.probe_write(&node_id).expect("probe write");
        let ok = write
            .body
            .get("ok")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        assert!(
            ok,
            "quorum write must succeed on {} body={}",
            node_id, write.raw_body
        );
        let acks = write
            .body
            .get("replicationAcks")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        assert!(
            acks >= 2,
            "quorum write must have >= 2 acks on {} got={} body={}",
            node_id,
            acks,
            write.raw_body
        );
    }
}

#[test]
#[ignore] // requires apps/wreladb-lab (deleted)
fn quorum_write_converges_across_all_nodes() {
    let mut cluster = LocalCluster::boot_with_config(quorum_config()).expect("boot quorum cluster");
    cluster
        .wait_for_all_healthy(Duration::from_secs(60))
        .expect("all nodes healthy");
    cluster
        .wait_for_mesh_ready(Duration::from_secs(60))
        .expect("mesh ready");

    let write = cluster.probe_write("node-a").expect("probe write");
    let (expected_value, _version) = cluster
        .validate_successful_write(&write)
        .expect("write must succeed");

    cluster
        .wait_for_cluster_value(&expected_value, Duration::from_secs(20))
        .expect("all nodes must converge on the written value under quorum replication");
}
