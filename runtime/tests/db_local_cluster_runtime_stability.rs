mod support;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use support::local_cluster::LocalCluster;

#[derive(Debug, Default, Clone)]
struct StabilityStats {
    total_ops: u64,
    failures: u64,
    last_error: Option<String>,
}

#[test]
#[ignore] // requires apps/wreladb-lab (deleted)
fn db_local_cluster_runtime_stability_under_high_churn() {
    let mut cluster = LocalCluster::boot_default().expect("boot local cluster");
    cluster
        .wait_for_all_healthy(Duration::from_secs(45))
        .expect("all nodes healthy");
    cluster
        .wait_for_mesh_ready(Duration::from_secs(45))
        .expect("mesh ready");

    let node_ids = Arc::new(cluster.node_ids());
    let endpoints = Arc::new(cluster.node_endpoints());
    let stop = Arc::new(AtomicBool::new(false));
    let stats = Arc::new(Mutex::new(StabilityStats::default()));

    let worker_count = 4usize;
    let mut workers = Vec::with_capacity(worker_count);
    for worker_idx in 0..worker_count {
        let worker_stop = stop.clone();
        let worker_stats = stats.clone();
        let worker_ids = node_ids.clone();
        let worker_endpoints = endpoints.clone();
        workers.push(thread::spawn(move || {
            let mut seq = worker_idx as u64;
            while !worker_stop.load(Ordering::Relaxed) {
                let node_id = &worker_ids[(seq as usize) % worker_ids.len()];
                let Some(base_url) = worker_endpoints.get(node_id) else {
                    let mut guard = worker_stats.lock().expect("stats lock");
                    guard.failures = guard.failures.saturating_add(1);
                    guard.last_error = Some(format!("missing endpoint for node {}", node_id));
                    thread::sleep(Duration::from_millis(10));
                    continue;
                };

                {
                    let mut guard = worker_stats.lock().expect("stats lock");
                    guard.total_ops = guard.total_ops.saturating_add(1);
                }
                let write = LocalCluster::request_json_url(base_url, "POST", "/api/probe/write");
                match write {
                    Ok(write_response) => {
                        let is_ok = write_response
                            .body
                            .get("ok")
                            .and_then(serde_json::Value::as_bool)
                            .unwrap_or(false);
                        let version = write_response
                            .body
                            .get("version")
                            .and_then(serde_json::Value::as_i64)
                            .unwrap_or(-1);
                        if write_response.status != 200 || !is_ok || version <= 0 {
                            let mut guard = worker_stats.lock().expect("stats lock");
                            guard.failures = guard.failures.saturating_add(1);
                            guard.last_error = Some(format!(
                                "write invariant failed node={} status={} body={}",
                                node_id, write_response.status, write_response.raw_body
                            ));
                        }
                    }
                    Err(err) => {
                        let mut guard = worker_stats.lock().expect("stats lock");
                        guard.failures = guard.failures.saturating_add(1);
                        guard.last_error =
                            Some(format!("write request error node={}: {}", node_id, err));
                    }
                }

                seq = seq.wrapping_add(1);
                thread::sleep(Duration::from_millis(12));
            }
        }));
    }

    let stability_window = Instant::now() + Duration::from_secs(15);
    while Instant::now() < stability_window {
        cluster
            .assert_all_processes_alive()
            .expect("all node processes alive during churn");
        thread::sleep(Duration::from_millis(200));
    }

    stop.store(true, Ordering::Relaxed);
    for worker in workers {
        worker.join().expect("worker thread join");
    }

    cluster
        .wait_for_all_healthy(Duration::from_secs(30))
        .expect("cluster healthy after churn");
    cluster
        .wait_for_mesh_ready(Duration::from_secs(30))
        .expect("mesh ready after churn");
    cluster
        .assert_all_processes_alive()
        .expect("all node processes alive after churn");

    let stats = stats.lock().expect("stats lock").clone();
    assert!(
        stats.total_ops > 0,
        "stability test executed zero operations"
    );
    assert_eq!(
        stats.failures, 0,
        "stability failures={} last_error={:?}",
        stats.failures, stats.last_error
    );
}
