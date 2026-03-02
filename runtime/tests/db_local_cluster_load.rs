mod support;

use serde_json::Value;
use std::time::{Duration, Instant};

use support::local_cluster::LocalCluster;

fn wait_for_key_on_all_nodes(
    cluster: &mut LocalCluster,
    key: &str,
    expected_value: &str,
    timeout: Duration,
) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    let mut last_issue = "load key not visible on all nodes".to_string();
    while Instant::now() < deadline {
        cluster.assert_all_processes_alive()?;
        let mut all_visible = true;
        for node_id in cluster.node_ids() {
            match cluster.load_read(&node_id, key) {
                Ok(response)
                    if response.status == 200
                        && response
                            .body
                            .get("ok")
                            .and_then(Value::as_bool)
                            .unwrap_or(false)
                        && response
                            .body
                            .get("value")
                            .and_then(Value::as_str)
                            .map(|value| value == expected_value)
                            .unwrap_or(false) => {}
                Ok(response) => {
                    all_visible = false;
                    last_issue = format!(
                        "node {} missing key {} value {}: status={} body={}",
                        node_id, key, expected_value, response.status, response.raw_body
                    );
                    break;
                }
                Err(err) => {
                    all_visible = false;
                    last_issue = format!("load read request failed for {node_id}: {err}");
                    break;
                }
            }
        }
        if all_visible {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(150));
    }
    Err(last_issue)
}

#[test]
#[ignore] // requires apps/wreladb-lab (deleted)
fn db_local_cluster_load_distributed_keys_converge() {
    let mut cluster = LocalCluster::boot_default().expect("boot local cluster");
    cluster
        .wait_for_all_healthy(Duration::from_secs(45))
        .expect("all nodes healthy");
    cluster
        .wait_for_mesh_ready(Duration::from_secs(45))
        .expect("mesh ready");

    for node_id in cluster.node_ids() {
        for sequence in 1..=6usize {
            let write = cluster
                .load_write(&node_id, "load", sequence)
                .expect("load write response");
            assert_eq!(
                write.status, 200,
                "load write must return 200 for {} seq={} body={}",
                node_id, sequence, write.raw_body
            );
            assert!(
                write
                    .body
                    .get("ok")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                "load write payload must report ok=true for {} seq={} body={}",
                node_id,
                sequence,
                write.raw_body
            );

            let response_machine = write
                .body
                .get("machineId")
                .and_then(Value::as_str)
                .expect("load write machineId");
            assert_eq!(
                response_machine, node_id,
                "targeted load write must stay on requested machine"
            );

            let key = write
                .body
                .get("key")
                .and_then(Value::as_str)
                .expect("load write key")
                .to_string();
            let value = write
                .body
                .get("value")
                .and_then(Value::as_str)
                .expect("load write value")
                .to_string();
            let version = write
                .body
                .get("version")
                .and_then(Value::as_i64)
                .expect("load write version");
            assert!(version > 0, "load write version must be positive");
            assert!(!key.is_empty(), "load write key must not be empty");
            assert!(!value.is_empty(), "load write value must not be empty");

            wait_for_key_on_all_nodes(&mut cluster, &key, &value, Duration::from_secs(15))
                .expect("distributed key must converge on all nodes");
        }
    }
}
