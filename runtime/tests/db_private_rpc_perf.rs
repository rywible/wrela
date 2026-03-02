use bytes::Bytes;
use serde::Serialize;
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Barrier, Mutex, RwLock};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use wrela_runtime::db::DbConfig;
use wrela_runtime::db::config::ReplicationConfig;
use wrela_runtime::db::rpc::grpc::{GrpcEdgeService, WriteBatchRequest};
use wrela_runtime::db::rpc::private_network::{
    start_private_rpc_server, write_batch_over_private_rpc,
};
use wrela_runtime::db::types::BatchOp;
use wrela_runtime::db::{close_db, open_db_with_config};

const PRIVATE_RPC_REPORT_SCHEMA_NAME: &str = "wrela.private_rpc_perf";
const PRIVATE_RPC_REPORT_SCHEMA_VERSION: u32 = 3;
const PRIVATE_RPC_INDEX_SCHEMA_NAME: &str = "wrela.private_rpc_perf_index";
const PRIVATE_RPC_INDEX_SCHEMA_VERSION: u32 = 3;
const TEST_HOME_EPOCH: u64 = 1;
const TEST_SHARD_MAP_EPOCH: u64 = 1;
const TEST_OWNERSHIP_TOKEN: &str = "private-rpc-perf-token";

fn open_private_rpc_perf_db(path: &std::path::Path) -> i64 {
    let config = DbConfig::for_testing().with_replication(ReplicationConfig {
        factor: 3,
        write_quorum: 2,
        ..DbConfig::for_testing().replication
    });
    open_db_with_config(path, &config).expect("open db")
}

#[derive(Debug, Default, Clone)]
struct WorkerMetrics {
    attempts: u64,
    success: u64,
    failures: u64,
    latencies_us: Vec<u64>,
}

#[derive(Debug, Serialize)]
struct PrivateRpcPerfReport {
    schema_name: &'static str,
    schema_version: u32,
    metadata: RunMetadata,
    run_id: String,
    wire_format: String,
    duration_seconds: u64,
    concurrency: usize,
    ops_per_batch: usize,
    payload_bytes: usize,
    attempts: u64,
    success: u64,
    failures: u64,
    tps: f64,
    p50_ms: f64,
    p95_ms: f64,
    p99_ms: f64,
    p999_ms: f64,
}

#[derive(Debug, Serialize, Clone)]
struct RunMetadata {
    os: String,
    arch: String,
    cpu_count: Option<usize>,
    hostname: Option<String>,
}

#[derive(Debug, Serialize)]
struct PrivateRpcPerfIndex {
    schema_name: &'static str,
    schema_version: u32,
    run_id: String,
    wire_format: String,
    report_path: String,
    metadata: RunMetadata,
}

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|raw| raw.trim().parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|raw| raw.trim().parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

fn percentile_ms(mut values_us: Vec<u64>, percentile: f64) -> f64 {
    if values_us.is_empty() {
        return 0.0;
    }
    values_us.sort_unstable();
    let idx = ((values_us.len() - 1) as f64 * percentile).round() as usize;
    values_us[idx] as f64 / 1_000.0
}

fn host_name() -> Option<String> {
    std::env::var("HOSTNAME")
        .ok()
        .or_else(|| std::env::var("COMPUTERNAME").ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn run_metadata() -> RunMetadata {
    RunMetadata {
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        cpu_count: std::thread::available_parallelism()
            .ok()
            .map(|count| count.get()),
        hostname: host_name(),
    }
}

fn assert_private_rpc_report_schema(report: &PrivateRpcPerfReport) {
    let value = serde_json::to_value(report).expect("serialize private rpc report");
    assert_eq!(
        value
            .get("schema_name")
            .and_then(Value::as_str)
            .expect("schema_name"),
        PRIVATE_RPC_REPORT_SCHEMA_NAME
    );
    assert_eq!(
        value
            .get("schema_version")
            .and_then(Value::as_u64)
            .expect("schema_version"),
        PRIVATE_RPC_REPORT_SCHEMA_VERSION as u64
    );
    for key in [
        "run_id",
        "wire_format",
        "duration_seconds",
        "concurrency",
        "tps",
        "p99_ms",
        "p999_ms",
    ] {
        assert!(
            value.get(key).is_some(),
            "private-rpc report missing required key {key}"
        );
    }
    assert!(
        value
            .get("metadata")
            .and_then(|item| item.get("os"))
            .and_then(Value::as_str)
            .is_some(),
        "private-rpc report metadata.os missing"
    );
    assert!(
        value
            .get("metadata")
            .and_then(|item| item.get("arch"))
            .and_then(Value::as_str)
            .is_some(),
        "private-rpc report metadata.arch missing"
    );
}

fn assert_private_rpc_index_schema(index: &PrivateRpcPerfIndex) {
    let value = serde_json::to_value(index).expect("serialize private rpc index");
    assert_eq!(
        value
            .get("schema_name")
            .and_then(Value::as_str)
            .expect("schema_name"),
        PRIVATE_RPC_INDEX_SCHEMA_NAME
    );
    assert_eq!(
        value
            .get("schema_version")
            .and_then(Value::as_u64)
            .expect("schema_version"),
        PRIVATE_RPC_INDEX_SCHEMA_VERSION as u64
    );
    assert!(
        value
            .get("metadata")
            .and_then(|item| item.get("os"))
            .and_then(Value::as_str)
            .is_some(),
        "private-rpc index metadata.os missing"
    );
    assert!(
        value
            .get("metadata")
            .and_then(|item| item.get("arch"))
            .and_then(Value::as_str)
            .is_some(),
        "private-rpc index metadata.arch missing"
    );
}

#[test]
#[ignore = "manual private-rpc perf harness"]
fn db_private_rpc_perf_harness_emits_json_artifact() {
    let duration_seconds = env_u64("WRELA_PRIVATE_RPC_PERF_DURATION_SECONDS", 8);
    let concurrency = env_usize("WRELA_PRIVATE_RPC_PERF_CONCURRENCY", 16);
    let ops_per_batch = env_usize("WRELA_PRIVATE_RPC_PERF_OPS_PER_BATCH", 1);
    let payload_bytes = env_usize("WRELA_PRIVATE_RPC_PERF_PAYLOAD_BYTES", 32);
    let duration = Duration::from_secs(duration_seconds);
    let wire_format = std::env::var("WRELA_PRIVATE_RPC_PERF_WIRE_FORMAT")
        .ok()
        .unwrap_or_else(|| "json".to_string());
    let metadata = run_metadata();

    let run_id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|dur| {
            let millis = dur.as_millis() as u64;
            let pid_suffix = (std::process::id() as u64) % 1_000;
            millis
                .saturating_mul(1_000)
                .saturating_add(pid_suffix)
                .to_string()
        })
        .unwrap_or_else(|_| "unknown".to_string());
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(|path| path.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    let artifact_dir = workspace_root
        .join(".artifacts")
        .join("perf")
        .join("local-db-write")
        .join("private-rpc");
    fs::create_dir_all(&artifact_dir).expect("create private-rpc artifact dir");

    let tempdir = tempfile::tempdir().expect("tempdir");
    let handle = open_private_rpc_perf_db(tempdir.path());
    let mut service = GrpcEdgeService::new("node-a", "node-a");
    service.bind_handle(handle);
    let service = Arc::new(RwLock::new(service));
    let mut server = start_private_rpc_server("127.0.0.1:0", service, Duration::from_millis(1500))
        .expect("start private rpc server");
    let addr = server.listen_addr().to_string();

    let barrier = Arc::new(Barrier::new(concurrency.max(1)));
    let results: Arc<Mutex<Vec<WorkerMetrics>>> = Arc::new(Mutex::new(Vec::new()));
    let started = Instant::now();
    let mut workers = Vec::with_capacity(concurrency);
    for worker_id in 0..concurrency {
        let worker_barrier = barrier.clone();
        let worker_results = results.clone();
        let target_addr = addr.clone();
        workers.push(thread::spawn(move || {
            let mut metrics = WorkerMetrics::default();
            worker_barrier.wait();
            let deadline = Instant::now() + duration;
            let mut sequence = 0u64;
            while Instant::now() < deadline {
                sequence = sequence.saturating_add(1);
                metrics.attempts = metrics.attempts.saturating_add(1);
                let mut ops = Vec::with_capacity(ops_per_batch);
                for item in 0..ops_per_batch {
                    let key = format!("rpc-perf-{worker_id}-{sequence}-{item}").into_bytes();
                    ops.push(BatchOp::Put {
                        namespace: Bytes::from_static(b"perf_rpc"),
                        key: key.into(),
                        value: Bytes::from(vec![b'v'; payload_bytes]),
                        expected_version: None,
                    });
                }
                let op_started = Instant::now();
                let result = write_batch_over_private_rpc(
                    &target_addr,
                    WriteBatchRequest {
                        handle: 0,
                        ops,
                        idempotency_token: None,
                        expected_home_epoch: TEST_HOME_EPOCH,
                        expected_shard_map_epoch: TEST_SHARD_MAP_EPOCH,
                        ownership_token: TEST_OWNERSHIP_TOKEN.to_string(),
                    },
                    Duration::from_millis(1500),
                );
                match result {
                    Ok(_) => {
                        metrics.success = metrics.success.saturating_add(1);
                        metrics
                            .latencies_us
                            .push(op_started.elapsed().as_micros().min(u64::MAX as u128) as u64);
                    }
                    Err(_) => {
                        metrics.failures = metrics.failures.saturating_add(1);
                    }
                }
            }
            if let Ok(mut shared) = worker_results.lock() {
                shared.push(metrics);
            }
        }));
    }
    for worker in workers {
        let _ = worker.join();
    }
    let elapsed = started.elapsed();
    let collected = results.lock().expect("metrics lock");
    let mut attempts = 0u64;
    let mut success = 0u64;
    let mut failures = 0u64;
    let mut latencies = Vec::new();
    for metric in collected.iter() {
        attempts = attempts.saturating_add(metric.attempts);
        success = success.saturating_add(metric.success);
        failures = failures.saturating_add(metric.failures);
        latencies.extend_from_slice(&metric.latencies_us);
    }

    server.shutdown();
    assert!(close_db(handle));

    let report = PrivateRpcPerfReport {
        schema_name: PRIVATE_RPC_REPORT_SCHEMA_NAME,
        schema_version: PRIVATE_RPC_REPORT_SCHEMA_VERSION,
        metadata: metadata.clone(),
        run_id: run_id.clone(),
        wire_format: wire_format.clone(),
        duration_seconds,
        concurrency,
        ops_per_batch,
        payload_bytes,
        attempts,
        success,
        failures,
        tps: if elapsed.is_zero() {
            0.0
        } else {
            success as f64 / elapsed.as_secs_f64()
        },
        p50_ms: percentile_ms(latencies.clone(), 0.50),
        p95_ms: percentile_ms(latencies.clone(), 0.95),
        p99_ms: percentile_ms(latencies, 0.99),
        p999_ms: percentile_ms(
            collected
                .iter()
                .flat_map(|metric| metric.latencies_us.iter().copied())
                .collect::<Vec<_>>(),
            0.999,
        ),
    };
    assert_private_rpc_report_schema(&report);

    let report_path = artifact_dir.join(format!("{run_id}-{wire_format}.json"));
    fs::write(
        &report_path,
        serde_json::to_string_pretty(&report).expect("serialize private-rpc report"),
    )
    .expect("write private-rpc report");

    let index_path = artifact_dir.join(format!("{run_id}-index.json"));
    let index = PrivateRpcPerfIndex {
        schema_name: PRIVATE_RPC_INDEX_SCHEMA_NAME,
        schema_version: PRIVATE_RPC_INDEX_SCHEMA_VERSION,
        run_id,
        wire_format,
        report_path: report_path.to_string_lossy().to_string(),
        metadata,
    };
    assert_private_rpc_index_schema(&index);
    fs::write(
        &index_path,
        serde_json::to_string_pretty(&index).expect("serialize private-rpc index"),
    )
    .expect("write private-rpc index");

    eprintln!(
        "private-rpc perf artifact written to {}",
        report_path.display()
    );
}
