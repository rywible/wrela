mod support;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use support::local_cluster::LocalCluster;

#[derive(Debug, Default, Clone)]
struct TrafficStats {
    total_requests: u64,
    failed_requests: u64,
    last_error: Option<String>,
    last_value: Option<String>,
}

#[test]
#[ignore] // requires apps/wreladb-lab (deleted)
fn db_local_cluster_rolling_restart_under_traffic_converges() {
    let mut cluster = LocalCluster::boot_default().expect("boot local cluster");
    cluster
        .wait_for_all_healthy(Duration::from_secs(45))
        .expect("all nodes healthy");
    cluster
        .wait_for_mesh_ready(Duration::from_secs(45))
        .expect("mesh ready");

    let node_ids = cluster.node_ids();
    let endpoints = cluster.node_endpoints();
    let stop = Arc::new(AtomicBool::new(false));
    let stats = Arc::new(Mutex::new(TrafficStats::default()));

    let traffic_stop = stop.clone();
    let traffic_stats = stats.clone();
    let traffic_node_ids = node_ids.clone();
    let traffic_endpoints = endpoints.clone();
    let traffic_thread = thread::spawn(move || {
        let mut idx = 0usize;
        while !traffic_stop.load(Ordering::Relaxed) {
            let node_id = &traffic_node_ids[idx % traffic_node_ids.len()];
            idx = idx.wrapping_add(1);
            let Some(base_url) = traffic_endpoints.get(node_id) else {
                let mut guard = traffic_stats.lock().expect("traffic stats lock");
                guard.failed_requests = guard.failed_requests.saturating_add(1);
                guard.last_error = Some(format!("missing endpoint for {}", node_id));
                thread::sleep(Duration::from_millis(20));
                continue;
            };

            let write = LocalCluster::request_json_url(base_url, "POST", "/api/probe/write");
            let mut guard = traffic_stats.lock().expect("traffic stats lock");
            guard.total_requests = guard.total_requests.saturating_add(1);
            match write {
                Ok(response) => {
                    let is_ok = response
                        .body
                        .get("ok")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false);
                    let version = response
                        .body
                        .get("version")
                        .and_then(serde_json::Value::as_i64)
                        .unwrap_or(-1);
                    let value = response
                        .body
                        .get("value")
                        .and_then(serde_json::Value::as_str)
                        .map(ToString::to_string);
                    if response.status != 200 || !is_ok || version <= 0 {
                        guard.failed_requests = guard.failed_requests.saturating_add(1);
                        guard.last_error = Some(format!(
                            "write invariant failed node={} status={} body={}",
                            node_id, response.status, response.raw_body
                        ));
                    } else if let Some(value) = value {
                        guard.last_value = Some(value);
                    }
                }
                Err(err) => {
                    guard.failed_requests = guard.failed_requests.saturating_add(1);
                    guard.last_error =
                        Some(format!("write request error node={}: {}", node_id, err));
                }
            }
            drop(guard);
            thread::sleep(Duration::from_millis(35));
        }
    });

    let leader_before = cluster.current_leader_id().expect("leader before restart");
    let follower = node_ids
        .iter()
        .find(|node_id| node_id.as_str() != leader_before.as_str())
        .cloned()
        .expect("follower node");
    cluster.restart_node(&follower).expect("restart follower");
    cluster
        .wait_for_all_healthy(Duration::from_secs(30))
        .expect("healthy after follower restart");
    cluster
        .wait_for_mesh_ready(Duration::from_secs(30))
        .expect("mesh ready after follower restart");

    let leader_now = cluster
        .current_leader_id()
        .expect("leader before leader restart");
    cluster.restart_node(&leader_now).expect("restart leader");
    cluster
        .wait_for_all_healthy(Duration::from_secs(30))
        .expect("healthy after leader restart");
    cluster
        .wait_for_mesh_ready(Duration::from_secs(30))
        .expect("mesh ready after leader restart");
    let leader_after_rolling = cluster.current_leader_id().expect("leader after rolling");
    assert_eq!(
        leader_after_rolling, "node-a",
        "leader election must deterministically converge to lexicographically-smallest node"
    );

    let churn_until = Instant::now() + Duration::from_secs(3);
    while Instant::now() < churn_until {
        cluster
            .assert_all_processes_alive()
            .expect("all node processes alive");
        thread::sleep(Duration::from_millis(120));
    }

    stop.store(true, Ordering::Relaxed);
    traffic_thread.join().expect("traffic thread join");

    let stats = stats.lock().expect("stats lock").clone();
    assert!(
        stats.total_requests > 0,
        "traffic loop did not execute any requests"
    );
    assert_eq!(
        stats.failed_requests, 0,
        "rolling traffic failures={} last_error={:?}",
        stats.failed_requests, stats.last_error
    );
    cluster
        .assert_all_processes_alive()
        .expect("all node processes alive at end");
}
